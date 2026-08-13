use std::io;

const BLOCK_TEXELS: u32 = 4;
const BLOCK_BYTES: usize = 16;
pub(crate) const MAX_BC7_DIMENSION: u32 = 16_384;
pub(crate) const MAX_BC7_STAGING_BYTES: usize = 64 * 1024 * 1024;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
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
        .checked_mul(BLOCK_BYTES as u32)
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

    let surface = intel_tex_2::RgbaSurface {
        data: &padded,
        width: layout.padded_width,
        height: layout.padded_height,
        stride: padded_stride as u32,
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
    use super::*;

    #[test]
    fn odd_dimensions_are_padded_to_complete_bc7_block_rows() {
        let rgba = vec![0x80; 5 * 7 * 4];
        let encoded = encode_rgba(Bc7ContentKind::Thumbnail, 5, 7, 20, &rgba).unwrap();
        assert_eq!((encoded.padded_width, encoded.padded_height), (8, 8));
        assert_eq!(encoded.row_pitch, 32);
        assert_eq!(encoded.blocks.len(), 64);
        encoded.validate().unwrap();
    }

    #[test]
    fn bc7_payload_is_exactly_one_quarter_of_padded_rgba() {
        let rgba = vec![255; 32 * 20 * 4];
        let encoded = encode_rgba(Bc7ContentKind::Icon, 32, 20, 128, &rgba).unwrap();
        assert_eq!(encoded.blocks.len() * 4, rgba.len());
    }

    #[test]
    fn malformed_and_excessive_sources_fail_before_encoding() {
        assert!(checked_layout(0, 1).is_err());
        assert!(checked_layout(MAX_BC7_DIMENSION + 1, 1).is_err());
        assert!(encode_rgba(Bc7ContentKind::Icon, 4, 4, 15, &[0; 60]).is_err());
        assert!(encode_rgba(Bc7ContentKind::Icon, 4, 4, 16, &[0; 63]).is_err());
    }
}
