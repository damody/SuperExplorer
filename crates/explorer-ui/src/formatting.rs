const UNITS: [&str; 4] = ["KB", "MB", "GB", "TB"];

/// Formats a byte count with adaptive binary units for Explorer surfaces.
#[allow(
    clippy::cast_precision_loss,
    reason = "Explorer size labels intentionally use bounded human-readable precision"
)]
pub fn format_file_size(bytes: u64) -> String {
    if bytes == 0 {
        return "0 KB".to_owned();
    }
    let mut value = bytes as f64 / 1024.0;
    let mut unit = 0;
    while value >= 1024.0 && unit + 1 < UNITS.len() {
        value /= 1024.0;
        unit += 1;
    }
    if unit == 0 && value < 1.0 {
        value = 1.0;
    }
    let number = format!("{value:.1}");
    format!("{number} {}", UNITS[unit])
}

#[cfg(test)]
mod tests {
    use super::format_file_size;

    #[test]
    fn promotes_units_and_rounds_small_files() {
        let cases = [
            (0, "0 KB"),
            (1, "1.0 KB"),
            (1023, "1.0 KB"),
            (1024, "1.0 KB"),
            (1536, "1.5 KB"),
            (10 * 1024, "10.0 KB"),
            (1024 * 1024, "1.0 MB"),
            (1536 * 1024, "1.5 MB"),
            (1024 * 1024 * 1024, "1.0 GB"),
            (5_427_537_920, "5.1 GB"),
            (250 * 1024_u64.pow(3) + 512 * 1024_u64.pow(2), "250.5 GB"),
            (1024_u64.pow(4), "1.0 TB"),
        ];
        for (bytes, expected) in cases {
            assert_eq!(format_file_size(bytes), expected, "bytes={bytes}");
        }
    }
}
