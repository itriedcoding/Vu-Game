//! Human readable formatting helpers. Pure functions, covered by unit tests.

/// Format a byte count the way the interface shows it.
pub fn bytes(value: u64) -> String {
    const UNITS: [&str; 5] = ["B", "KB", "MB", "GB", "TB"];
    if value < 1024 {
        return format!("{value} B");
    }
    let mut size = value as f64;
    let mut unit = 0;
    while size >= 1024.0 && unit < UNITS.len() - 1 {
        size /= 1024.0;
        unit += 1;
    }
    if size >= 100.0 {
        format!("{size:.0} {}", UNITS[unit])
    } else if size >= 10.0 {
        format!("{size:.1} {}", UNITS[unit])
    } else {
        format!("{size:.2} {}", UNITS[unit])
    }
}

/// Rounded percentage with a `%` suffix.
pub fn percent(value: f32) -> String {
    let rounded = value.clamp(0.0, 100.0).round();
    format!("{rounded:.0}%")
}

/// Compact uptime, e.g. `4m 12s`, `2h 08m`, `3d 4h`.
pub fn duration(seconds: u64) -> String {
    if seconds < 60 {
        return format!("{seconds}s");
    }
    if seconds < 3600 {
        let minutes = seconds / 60;
        let rest = seconds % 60;
        return format!("{minutes}m {rest:02}s");
    }
    if seconds < 86_400 {
        return format!("{}h {:02}m", seconds / 3600, (seconds % 3600) / 60);
    }
    format!("{}d {}h", seconds / 86_400, (seconds % 86_400) / 3600)
}

/// Frame rate with at most one decimal.
pub fn fps(value: f32) -> String {
    if value >= 100.0 || value.fract() < 0.05 {
        format!("{value:.0}")
    } else {
        format!("{value:.1}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats_bytes() {
        assert_eq!(bytes(0), "0 B");
        assert_eq!(bytes(900), "900 B");
        assert_eq!(bytes(1024), "1.00 KB");
        assert_eq!(bytes(15 * 1024 * 1024), "15.0 MB");
        assert_eq!(bytes(2 * 1024 * 1024 * 1024), "2.00 GB");
    }

    #[test]
    fn formats_percent_and_duration() {
        assert_eq!(percent(42.4), "42%");
        assert_eq!(percent(99.6), "100%");
        assert_eq!(duration(9), "9s");
        assert_eq!(duration(125), "2m 05s");
        assert_eq!(duration(3_600 + 8 * 60), "1h 08m");
        assert_eq!(duration(86_400), "1d 0h");
    }

    #[test]
    fn formats_fps() {
        assert_eq!(fps(60.0), "60");
        assert_eq!(fps(59.94), "59.9");
        assert_eq!(fps(144.2), "144");
    }
}
