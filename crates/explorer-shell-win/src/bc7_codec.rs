use std::{
    io,
    sync::atomic::{AtomicU64, Ordering},
    time::Instant,
};

const BLOCK_TEXELS: u32 = 4;
const BLOCK_BYTES_U32: u32 = 16;
pub(crate) const MAX_BC7_DIMENSION: u32 = 16_384;
pub(crate) const MAX_BC7_STAGING_BYTES: usize = 64 * 1024 * 1024;
static ACTIVE_ENCODERS: AtomicU64 = AtomicU64::new(0);
static PEAK_ACTIVE_ENCODERS: AtomicU64 = AtomicU64::new(0);
static ACTIVE_STAGING_BYTES: AtomicU64 = AtomicU64::new(0);
static PEAK_STAGING_BYTES: AtomicU64 = AtomicU64::new(0);
static ENCODE_COUNT: AtomicU64 = AtomicU64::new(0);
static ENCODE_ERRORS: AtomicU64 = AtomicU64::new(0);
static TOTAL_ENCODE_MICROS: AtomicU64 = AtomicU64::new(0);
static MAX_ENCODE_MICROS: AtomicU64 = AtomicU64::new(0);

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct Bc7PipelineStatsV1 {
    pub active_encoders: u64,
    pub peak_active_encoders: u64,
    pub active_staging_bytes: u64,
    pub peak_staging_bytes: u64,
    pub encode_count: u64,
    pub encode_errors: u64,
    pub total_encode_micros: u64,
    pub max_encode_micros: u64,
    pub staging_limit_bytes: u64,
}

pub fn bc7_pipeline_stats() -> Bc7PipelineStatsV1 {
    Bc7PipelineStatsV1 {
        active_encoders: ACTIVE_ENCODERS.load(Ordering::Acquire),
        peak_active_encoders: PEAK_ACTIVE_ENCODERS.load(Ordering::Acquire),
        active_staging_bytes: ACTIVE_STAGING_BYTES.load(Ordering::Acquire),
        peak_staging_bytes: PEAK_STAGING_BYTES.load(Ordering::Acquire),
        encode_count: ENCODE_COUNT.load(Ordering::Acquire),
        encode_errors: ENCODE_ERRORS.load(Ordering::Acquire),
        total_encode_micros: TOTAL_ENCODE_MICROS.load(Ordering::Acquire),
        max_encode_micros: MAX_ENCODE_MICROS.load(Ordering::Acquire),
        staging_limit_bytes: MAX_BC7_STAGING_BYTES as u64,
    }
}

struct EncodeGuard {
    staging_bytes: u64,
}

impl EncodeGuard {
    fn new(staging_bytes: usize) -> Self {
        let staging_bytes = staging_bytes as u64;
        let active = ACTIVE_ENCODERS.fetch_add(1, Ordering::AcqRel) + 1;
        let staging =
            ACTIVE_STAGING_BYTES.fetch_add(staging_bytes, Ordering::AcqRel) + staging_bytes;
        update_peak(&PEAK_ACTIVE_ENCODERS, active);
        update_peak(&PEAK_STAGING_BYTES, staging);
        Self { staging_bytes }
    }
}

impl Drop for EncodeGuard {
    fn drop(&mut self) {
        ACTIVE_ENCODERS.fetch_sub(1, Ordering::AcqRel);
        ACTIVE_STAGING_BYTES.fetch_sub(self.staging_bytes, Ordering::AcqRel);
    }
}

fn update_peak(target: &AtomicU64, value: u64) {
    let mut current = target.load(Ordering::Acquire);
    while value > current {
        match target.compare_exchange_weak(current, value, Ordering::AcqRel, Ordering::Acquire) {
            Ok(_) => break,
            Err(observed) => current = observed,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub(crate) enum Bc7ContentKind {
    Icon = 1,
    Thumbnail = 2,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct Bc7Raster {
    pub(crate) kind: Bc7ContentKind,
    pub(crate) width: u32,
    pub(crate) height: u32,
    pub(crate) padded_width: u32,
    pub(crate) padded_height: u32,
    pub(crate) row_pitch: u32,
    pub(crate) blocks: Vec<u8>,
}

impl Bc7Raster {
    pub(crate) fn validate(&self) -> io::Result<()> {
        let layout = checked_layout(self.width, self.height)?;
        if self.padded_width != layout.padded_width
            || self.padded_height != layout.padded_height
            || self.row_pitch != layout.row_pitch
            || self.blocks.len() != layout.payload_bytes
        {
            return Err(invalid("BC7 block layout mismatch"));
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct Bc7Layout {
    pub(crate) padded_width: u32,
    pub(crate) padded_height: u32,
    pub(crate) row_pitch: u32,
    pub(crate) payload_bytes: usize,
}

pub(crate) fn checked_layout(width: u32, height: u32) -> io::Result<Bc7Layout> {
    if width == 0 || height == 0 || width > MAX_BC7_DIMENSION || height > MAX_BC7_DIMENSION {
        return Err(invalid("BC7 dimensions are outside the supported range"));
    }
    let padded_width = width
        .checked_add(3)
        .ok_or_else(|| invalid("BC7 width overflow"))?
        & !3;
    let padded_height = height
        .checked_add(3)
        .ok_or_else(|| invalid("BC7 height overflow"))?
        & !3;
    let blocks_wide = padded_width / BLOCK_TEXELS;
    let blocks_high = padded_height / BLOCK_TEXELS;
    let row_pitch = blocks_wide
        .checked_mul(BLOCK_BYTES_U32)
        .ok_or_else(|| invalid("BC7 pitch overflow"))?;
    let payload_bytes = usize::try_from(row_pitch)
        .ok()
        .and_then(|pitch| {
            usize::try_from(blocks_high)
                .ok()
                .and_then(|rows| pitch.checked_mul(rows))
        })
        .ok_or_else(|| invalid("BC7 payload overflow"))?;
    if payload_bytes > MAX_BC7_STAGING_BYTES {
        return Err(invalid("BC7 payload exceeds the staging bound"));
    }
    Ok(Bc7Layout {
        padded_width,
        padded_height,
        row_pitch,
        payload_bytes,
    })
}

pub(crate) fn encode_rgba(
    kind: Bc7ContentKind,
    width: u32,
    height: u32,
    stride: u32,
    rgba: &[u8],
) -> io::Result<Bc7Raster> {
    let started = Instant::now();
    let result = encode_rgba_inner(kind, width, height, stride, rgba);
    let elapsed = u64::try_from(started.elapsed().as_micros()).unwrap_or(u64::MAX);
    if result.is_ok() {
        ENCODE_COUNT.fetch_add(1, Ordering::Relaxed);
        TOTAL_ENCODE_MICROS.fetch_add(elapsed, Ordering::Relaxed);
        update_peak(&MAX_ENCODE_MICROS, elapsed);
    } else {
        ENCODE_ERRORS.fetch_add(1, Ordering::Relaxed);
    }
    result
}

fn encode_rgba_inner(
    kind: Bc7ContentKind,
    width: u32,
    height: u32,
    stride: u32,
    rgba: &[u8],
) -> io::Result<Bc7Raster> {
    let layout = checked_layout(width, height)?;
    let minimum_stride = width
        .checked_mul(4)
        .ok_or_else(|| invalid("RGBA stride overflow"))?;
    let source_bytes = usize::try_from(stride)
        .ok()
        .and_then(|pitch| {
            usize::try_from(height)
                .ok()
                .and_then(|rows| pitch.checked_mul(rows))
        })
        .ok_or_else(|| invalid("RGBA byte length overflow"))?;
    if stride < minimum_stride || rgba.len() != source_bytes || source_bytes > MAX_BC7_STAGING_BYTES
    {
        return Err(invalid(
            "RGBA source layout is invalid or exceeds the staging bound",
        ));
    }

    let padded_stride = usize::try_from(layout.padded_width)
        .ok()
        .and_then(|width| width.checked_mul(4))
        .ok_or_else(|| invalid("padded RGBA stride overflow"))?;
    let padded_bytes = padded_stride
        .checked_mul(layout.padded_height as usize)
        .ok_or_else(|| invalid("padded RGBA length overflow"))?;
    if padded_bytes > MAX_BC7_STAGING_BYTES {
        return Err(invalid("padded RGBA exceeds the staging bound"));
    }
    let _guard = EncodeGuard::new(
        padded_bytes
            .checked_add(layout.payload_bytes)
            .ok_or_else(|| invalid("combined BC7 staging overflow"))?,
    );
    let mut padded = vec![0_u8; padded_bytes];
    let source_stride = stride as usize;
    let row_bytes = minimum_stride as usize;
    for row in 0..height as usize {
        let source = &rgba[row * source_stride..row * source_stride + row_bytes];
        padded[row * padded_stride..row * padded_stride + row_bytes].copy_from_slice(source);
        if layout.padded_width > width {
            let edge = source[row_bytes - 4..row_bytes].to_vec();
            for column in width as usize..layout.padded_width as usize {
                padded[row * padded_stride + column * 4..row * padded_stride + column * 4 + 4]
                    .copy_from_slice(&edge);
            }
        }
    }
    if layout.padded_height > height {
        let last = (height as usize - 1) * padded_stride;
        for row in height as usize..layout.padded_height as usize {
            let source = padded[last..last + padded_stride].to_vec();
            padded[row * padded_stride..(row + 1) * padded_stride].copy_from_slice(&source);
        }
    }

    let padded_stride_u32 = u32::try_from(padded_stride)
        .map_err(|_| invalid("padded RGBA stride exceeds the encoder contract"))?;
    let surface = intel_tex_2::RgbaSurface {
        data: &padded,
        width: layout.padded_width,
        height: layout.padded_height,
        stride: padded_stride_u32,
    };
    let settings = if rgba.chunks_exact(4).all(|pixel| pixel[3] == 255) {
        intel_tex_2::bc7::opaque_very_fast_settings()
    } else {
        intel_tex_2::bc7::alpha_very_fast_settings()
    };
    let blocks = intel_tex_2::bc7::compress_blocks(&settings, &surface);
    if blocks.len() != layout.payload_bytes {
        return Err(invalid("BC7 encoder returned an unexpected payload length"));
    }
    Ok(Bc7Raster {
        kind,
        width,
        height,
        padded_width: layout.padded_width,
        padded_height: layout.padded_height,
        row_pitch: layout.row_pitch,
        blocks,
    })
}

fn invalid(message: &'static str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message)
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Barrier};

    use super::*;

    static TEST_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

    #[test]
    fn odd_dimensions_are_padded_to_complete_bc7_block_rows() {
        let _serial = TEST_LOCK.lock().unwrap();
        let rgba = vec![0x80; 5 * 7 * 4];
        let encoded = encode_rgba(Bc7ContentKind::Thumbnail, 5, 7, 20, &rgba).unwrap();
        assert_eq!((encoded.padded_width, encoded.padded_height), (8, 8));
        assert_eq!(encoded.row_pitch, 32);
        assert_eq!(encoded.blocks.len(), 64);
        encoded.validate().unwrap();
    }

    #[test]
    fn concurrent_encoder_accounting_is_bounded_observable_and_released() {
        let _serial = TEST_LOCK.lock().unwrap();
        let starting_encoders = ACTIVE_ENCODERS.load(Ordering::Acquire);
        let starting_staging = ACTIVE_STAGING_BYTES.load(Ordering::Acquire);
        let entered = Arc::new(Barrier::new(5));
        let release = Arc::new(Barrier::new(5));
        let workers = (0..4)
            .map(|_| {
                let entered = Arc::clone(&entered);
                let release = Arc::clone(&release);
                std::thread::spawn(move || {
                    let _guard = EncodeGuard::new(1_024);
                    entered.wait();
                    release.wait();
                })
            })
            .collect::<Vec<_>>();
        entered.wait();
        assert_eq!(
            ACTIVE_ENCODERS.load(Ordering::Acquire),
            starting_encoders + 4
        );
        assert_eq!(
            ACTIVE_STAGING_BYTES.load(Ordering::Acquire),
            starting_staging + 4_096
        );
        assert!(PEAK_ACTIVE_ENCODERS.load(Ordering::Acquire) >= starting_encoders + 4);
        assert!(PEAK_STAGING_BYTES.load(Ordering::Acquire) >= starting_staging + 4_096);
        release.wait();
        for worker in workers {
            worker.join().expect("encoder accounting worker");
        }
        assert_eq!(ACTIVE_ENCODERS.load(Ordering::Acquire), starting_encoders);
        assert_eq!(
            ACTIVE_STAGING_BYTES.load(Ordering::Acquire),
            starting_staging
        );
    }

    #[test]
    fn bc7_payload_is_exactly_one_quarter_of_padded_rgba() {
        let _serial = TEST_LOCK.lock().unwrap();
        let rgba = vec![255; 32 * 20 * 4];
        let encoded = encode_rgba(Bc7ContentKind::Icon, 32, 20, 128, &rgba).unwrap();
        assert_eq!(encoded.blocks.len() * 4, rgba.len());
    }

    #[test]
    fn malformed_and_excessive_sources_fail_before_encoding() {
        let _serial = TEST_LOCK.lock().unwrap();
        assert!(checked_layout(0, 1).is_err());
        assert!(checked_layout(MAX_BC7_DIMENSION + 1, 1).is_err());
        assert!(encode_rgba(Bc7ContentKind::Icon, 4, 4, 15, &[0; 60]).is_err());
        assert!(encode_rgba(Bc7ContentKind::Icon, 4, 4, 16, &[0; 63]).is_err());
    }

    #[test]
    fn maximum_accepted_layout_is_bounded_without_allocating_fixture_pixels() {
        let _serial = TEST_LOCK.lock().unwrap();
        let layout = checked_layout(MAX_BC7_DIMENSION, 4_096)
            .expect("maximum single-axis dimensions remain representable at the byte bound");
        assert_eq!(layout.padded_width, MAX_BC7_DIMENSION);
        assert_eq!(layout.padded_height, 4_096);
        assert_eq!(layout.row_pitch, 65_536);
        assert_eq!(layout.payload_bytes, MAX_BC7_STAGING_BYTES);
        assert!(checked_layout(MAX_BC7_DIMENSION, MAX_BC7_DIMENSION).is_err());
        assert!(checked_layout(MAX_BC7_DIMENSION + 1, MAX_BC7_DIMENSION).is_err());
        assert!(checked_layout(MAX_BC7_DIMENSION, MAX_BC7_DIMENSION + 1).is_err());
    }

    #[test]
    #[allow(clippy::cast_precision_loss)]
    fn representative_icon_thumbnail_alpha_and_high_contrast_shapes_are_deterministic() {
        let _serial = TEST_LOCK.lock().unwrap();
        let started = Instant::now();
        let mut total_rgba_bytes = 0_usize;
        let mut total_padded_rgba_bytes = 0_usize;
        let mut total_bc7_bytes = 0_usize;
        let fixtures = [
            (Bc7ContentKind::Icon, 16, 16, false),
            (Bc7ContentKind::Icon, 20, 20, true),
            (Bc7ContentKind::Icon, 24, 24, false),
            (Bc7ContentKind::Icon, 32, 32, true),
            (Bc7ContentKind::Thumbnail, 127, 65, true),
        ];
        for (kind, width, height, alpha) in fixtures {
            let mut rgba = vec![0_u8; width as usize * height as usize * 4];
            for (index, pixel) in rgba.chunks_exact_mut(4).enumerate() {
                let high = if (index + index / width as usize).is_multiple_of(2) {
                    255
                } else {
                    0
                };
                pixel.copy_from_slice(&[high, 255 - high, high, if alpha { high } else { 255 }]);
            }
            let stride = width * 4;
            total_rgba_bytes = total_rgba_bytes.saturating_add(rgba.len());
            let first = encode_rgba(kind, width, height, stride, &rgba).expect("first encode");
            let second = encode_rgba(kind, width, height, stride, &rgba).expect("second encode");
            assert_eq!(first, second, "encoder output must be deterministic");
            assert_eq!(
                first.blocks.len(),
                checked_layout(width, height).unwrap().payload_bytes
            );
            let padded_rgba = first.padded_width as usize * first.padded_height as usize * 4;
            assert_eq!(first.blocks.len() * 4, padded_rgba);
            total_padded_rgba_bytes = total_padded_rgba_bytes.saturating_add(padded_rgba);
            total_bc7_bytes = total_bc7_bytes.saturating_add(first.blocks.len());
        }
        let elapsed_us = started.elapsed().as_micros();
        println!(
            "bc7-fixture-profile fixtures={} logical_rgba_bytes={} padded_rgba_bytes={} bc7_bytes={} logical_ratio={:.4} padded_ratio={:.4} elapsed_us={} peak_staging_bytes={}",
            fixtures.len(),
            total_rgba_bytes,
            total_padded_rgba_bytes,
            total_bc7_bytes,
            total_bc7_bytes as f64 / total_rgba_bytes as f64,
            total_bc7_bytes as f64 / total_padded_rgba_bytes as f64,
            elapsed_us,
            bc7_pipeline_stats().peak_staging_bytes,
        );
    }
}
