//! Frame-rate probe.
//!
//! Volt does not inject into Roblox and does not hook its renderer, so it cannot
//! invent an FPS number. What it can do is read the frame-rate values the Roblox
//! client itself occasionally writes into its session log, and only report a value
//! while that stat is fresh. When no fresh stat exists the dashboard says so
//! instead of guessing.

use crate::model::{FpsState, FpsStatus, MeasuredFps};
use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};

/// How many bytes of new log output are parsed per refresh.
const MAX_CHUNK: usize = 192 * 1024;
/// A frame-rate stat older than this is not shown as the current FPS.
const FRESH_SECS: u64 = 20;

const NO_DATA_MESSAGE: &str = "Roblox does not expose live frame rate to other applications. Volt shows a value only when the Roblox client writes a fresh frame-rate stat to its own log — it never estimates FPS.";

#[derive(Debug, Default)]
pub struct LogProbe {
    path: Option<PathBuf>,
    offset: u64,
    last_value: Option<(f32, u64, String)>,
    saw_log: bool,
}

impl LogProbe {
    pub fn reset(&mut self) {
        *self = LogProbe::default();
    }

    /// Refresh the probe. `now_unix` is the current wall clock in seconds.
    pub fn update(&mut self, log_dir: Option<&Path>, running: bool, now_unix: u64) -> FpsState {
        if !running {
            self.reset();
            return FpsState {
                status: FpsStatus::NotRunning,
                measured: None,
                detail: "Start Roblox Player to read its performance data.".to_owned(),
            };
        }

        let Some(dir) = log_dir else {
            return self.without_value(
                FpsStatus::Unavailable,
                "The Roblox log folder could not be located on this account.",
            );
        };

        let Some(path) = crate::roblox::latest_log(dir) else {
            return self.without_value(
                FpsStatus::Waiting,
                "Waiting for Roblox to start a session log…",
            );
        };

        if self.path.as_deref() != Some(path.as_path()) {
            self.path = Some(path.clone());
            self.offset = 0;
        }
        self.saw_log = true;

        let length = std::fs::metadata(&path).map(|meta| meta.len()).unwrap_or(0);
        if length < self.offset {
            // The log was rotated or truncated; start again near its end.
            self.offset = length.saturating_sub(MAX_CHUNK as u64);
        }

        if length > self.offset {
            self.read_new_lines(&path, now_unix);
        }

        if let Some((fps, at, detail)) = &self.last_value {
            let age = now_unix.saturating_sub(*at);
            if age <= FRESH_SECS {
                return FpsState {
                    status: FpsStatus::Measured,
                    measured: Some(MeasuredFps {
                        fps: *fps,
                        age_secs: age,
                        detail: detail.clone(),
                    }),
                    detail: "Reported by the Roblox client log.".to_owned(),
                };
            }
        }

        if self.saw_log {
            self.without_value(FpsStatus::Unavailable, NO_DATA_MESSAGE)
        } else {
            self.without_value(FpsStatus::Waiting, "Waiting for Roblox performance output…")
        }
    }

    fn without_value(&self, status: FpsStatus, detail: &str) -> FpsState {
        FpsState {
            status,
            measured: None,
            detail: detail.to_owned(),
        }
    }

    fn read_new_lines(&mut self, path: &Path, now_unix: u64) {
        let Ok(mut file) = File::open(path) else {
            return;
        };
        if file.seek(SeekFrom::Start(self.offset)).is_err() {
            return;
        }

        let mut buffer = vec![0u8; MAX_CHUNK];
        let Ok(read) = file.read(&mut buffer) else {
            return;
        };
        if read == 0 {
            return;
        }
        buffer.truncate(read);

        // Only consume whole lines; a partial trailing line is read again later.
        let complete = buffer
            .iter()
            .rposition(|&byte| byte == b'\n')
            .map(|index| index + 1)
            .unwrap_or(0);
        if complete == 0 {
            return;
        }
        self.offset += complete as u64;

        let text = String::from_utf8_lossy(&buffer[..complete]);
        for line in text.lines() {
            if let Some(fps) = parse_reported_fps(line) {
                let stamp = parse_log_timestamp(line).unwrap_or(now_unix);
                let detail: String = line.trim().chars().take(140).collect();
                self.last_value = Some((fps, stamp, detail));
            }
        }
    }
}

/// Extract a frame rate that the client reports in a log line.
///
/// Settings dumps (`FramerateCap`, `DFIntTaskSchedulerTargetFps`, …) are rejected:
/// they describe a limit, not a measurement.
pub fn parse_reported_fps(line: &str) -> Option<f32> {
    let lower = line.to_ascii_lowercase();
    const BLOCKED: [&str; 6] = ["cap", "target", "limit", "dfint", "setting", "quality"];
    if BLOCKED.iter().any(|word| lower.contains(word)) {
        return None;
    }

    let index = lower.find("fps")?;
    // The token must not be glued to another word, e.g. "ssfps".
    if index > 0 {
        let previous = lower[..index].chars().next_back()?;
        if previous.is_ascii_alphanumeric() {
            return None;
        }
    }

    let rest = &line[index + 3..];
    let rest = rest.trim_start_matches(|c: char| {
        c.is_whitespace() || matches!(c, ':' | '=' | '[' | ']' | '(' | ')' | '-')
    });
    let number: String = rest
        .chars()
        .take_while(|c| c.is_ascii_digit() || *c == '.')
        .collect();
    if number.is_empty() {
        return None;
    }
    let value: f32 = number.parse().ok()?;
    if (5.0..=1000.0).contains(&value) {
        Some(value)
    } else {
        None
    }
}

/// Parse the leading timestamp of a Roblox log line into Unix seconds.
///
/// Supports Roblox's ISO format (`2024-04-13T01:10:24.830Z,…`) and the older
/// Unix-seconds format (`1712972981.05664,…`).
pub fn parse_log_timestamp(line: &str) -> Option<u64> {
    let bytes = line.as_bytes();

    if bytes.len() >= 19 && bytes[4] == b'-' && bytes[7] == b'-' {
        let separator = bytes[10];
        if separator != b'T' && separator != b' ' {
            return None;
        }
        let year: i64 = line.get(0..4)?.parse().ok()?;
        let month: i64 = line.get(5..7)?.parse().ok()?;
        let day: i64 = line.get(8..10)?.parse().ok()?;
        let hour: i64 = line.get(11..13)?.parse().ok()?;
        let minute: i64 = line.get(14..16)?.parse().ok()?;
        let second: i64 = line.get(17..19)?.parse().ok()?;
        if !(1..=12).contains(&month) || !(1..=31).contains(&day) {
            return None;
        }
        let days = days_from_civil(year, month, day);
        let seconds = days * 86_400 + hour * 3_600 + minute * 60 + second;
        return u64::try_from(seconds).ok();
    }

    let prefix: String = line
        .chars()
        .take_while(|c| c.is_ascii_digit() || *c == '.')
        .collect();
    let seconds = prefix.split('.').next()?;
    if seconds.len() >= 9 {
        seconds.parse().ok()
    } else {
        None
    }
}

/// Days since 1970-01-01 for a proleptic Gregorian date (Howard Hinnant's algorithm).
fn days_from_civil(year: i64, month: i64, day: i64) -> i64 {
    let year = if month <= 2 { year - 1 } else { year };
    let era = if year >= 0 { year } else { year - 399 } / 400;
    let year_of_era = year - era * 400;
    let shifted_month = (month + 9) % 12;
    let day_of_year = (153 * shifted_month + 2) / 5 + day - 1;
    let day_of_era = year_of_era * 365 + year_of_era / 4 - year_of_era / 100 + day_of_year;
    era * 146_097 + day_of_era - 719_468
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_iso_timestamps() {
        let stamp = parse_log_timestamp("2024-04-13T01:10:24.830Z,0.83,6c08,6 [FLog::Output] hi")
            .expect("timestamp");
        // 2024-04-13T01:10:24Z
        assert_eq!(stamp, 1_712_970_624);
    }

    #[test]
    fn parses_unix_timestamps() {
        assert_eq!(
            parse_log_timestamp("1712972981.05664,7fb4,6 [FLog::Output] The channel is NULL"),
            Some(1_712_972_981)
        );
    }

    #[test]
    fn rejects_lines_without_timestamps() {
        assert_eq!(parse_log_timestamp("no timestamp here"), None);
        assert_eq!(parse_log_timestamp("12:00:00 short"), None);
    }

    #[test]
    fn reads_reported_frame_rates() {
        assert_eq!(parse_reported_fps("[FLog::Output] FPS: 58"), Some(58.0));
        assert_eq!(parse_reported_fps("average fps = 59.9"), Some(59.9));
        assert_eq!(
            parse_reported_fps("[FLog::RenderStats] fps 144"),
            Some(144.0)
        );
    }

    #[test]
    fn ignores_limits_and_settings() {
        assert_eq!(parse_reported_fps("\"FramerateCap\": 60"), None);
        assert_eq!(
            parse_reported_fps("DFIntTaskSchedulerTargetFps = 240"),
            None
        );
        assert_eq!(parse_reported_fps("FPS cap set to 30"), None);
        assert_eq!(parse_reported_fps("nothing to see"), None);
        assert_eq!(parse_reported_fps("FPS: 0"), None);
        assert_eq!(parse_reported_fps("FPS: 4000"), None);
    }
}
