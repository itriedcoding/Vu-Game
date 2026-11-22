//! Plain-text diagnostics report.
//!
//! The report contains exactly what the interface shows: measured values, the
//! recorded change ledger, and the machine facts Volt read from Windows. It never
//! estimates a frame rate, and it is written locally — nothing is uploaded.

use crate::format;
use crate::model::{FpsStatus, Snapshot};
use crate::optimize::ITEMS;
use crate::state::SavedState;

const LABEL_WIDTH: usize = 26;

/// `2026-11-22 07:15:04 UTC` for a Unix timestamp.
pub fn stamp(unix: u64) -> String {
    let days = (unix / 86_400) as i64;
    let seconds = unix % 86_400;
    let (year, month, day) = civil_from_days(days);
    format!(
        "{year:04}-{month:02}-{day:02} {:02}:{:02}:{:02} UTC",
        seconds / 3_600,
        (seconds % 3_600) / 60,
        seconds % 60
    )
}

/// File-name-safe stamp, e.g. `20261122-071504`.
pub fn file_stamp(unix: u64) -> String {
    let days = (unix / 86_400) as i64;
    let seconds = unix % 86_400;
    let (year, month, day) = civil_from_days(days);
    format!(
        "{year:04}{month:02}{day:02}-{:02}{:02}{:02}",
        seconds / 3_600,
        (seconds % 3_600) / 60,
        seconds % 60
    )
}

/// Howard Hinnant's `civil_from_days`: days since 1970-01-01 to a calendar date.
fn civil_from_days(days: i64) -> (i64, u32, u32) {
    let z = days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let day_of_era = (z - era * 146_097) as u64;
    let year_of_era =
        (day_of_era - day_of_era / 1_460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let year = year_of_era as i64 + era * 400;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let month_index = (5 * day_of_year + 2) / 153;
    let day = (day_of_year - (153 * month_index + 2) / 5 + 1) as u32;
    let month = if month_index < 10 {
        month_index + 3
    } else {
        month_index - 9
    } as u32;
    (if month <= 2 { year + 1 } else { year }, month, day)
}

/// Build the report for the current snapshot and recorded state.
pub fn build(snapshot: &Snapshot, state: &SavedState) -> String {
    let mut out = String::with_capacity(2_048);

    out.push_str(&format!(
        "Volt {} - diagnostics report\n{}\n\n",
        crate::APP_VERSION,
        stamp(crate::now_unix())
    ));
    out.push_str("Frame rate is only reported when the Roblox client writes it to its own log.\n");
    out.push_str(
        "Volt never estimates frame rate, never injects code and never edits Roblox files.\n",
    );

    section(&mut out, "SYSTEM");
    let info = &snapshot.info;
    row(&mut out, "Processor", reported(&info.cpu));
    row(&mut out, "Graphics", reported(&info.gpu));
    if !info.gpu_driver.is_empty() {
        row(&mut out, "Graphics driver", &info.gpu_driver);
    }
    row(
        &mut out,
        "Memory",
        &format!(
            "{} total, {} in use",
            format::bytes(snapshot.system.ram_total),
            format::percent(snapshot.system.ram_percent)
        ),
    );
    row(&mut out, "Windows", reported(&info.os));
    row(
        &mut out,
        "Logical processors",
        &if info.logical_cores == 0 {
            "Not reported".to_owned()
        } else {
            info.logical_cores.to_string()
        },
    );
    row(&mut out, "Power plan", reported(&info.power_plan));
    row(
        &mut out,
        "Elevation",
        if snapshot.elevated {
            "Administrator"
        } else {
            "Standard user"
        },
    );

    section(&mut out, "ROBLOX");
    let roblox = &snapshot.roblox;
    if roblox.running {
        row(
            &mut out,
            "Status",
            &format!(
                "Running, {} client{}",
                roblox.instances,
                if roblox.instances == 1 { "" } else { "s" }
            ),
        );
        row(
            &mut out,
            "Process",
            &format!("PID {}", roblox.pid.unwrap_or_default()),
        );
        row(&mut out, "Memory", &format::bytes(roblox.memory));
        row(&mut out, "CPU share", &format::percent(roblox.cpu_percent));
        row(
            &mut out,
            "Priority",
            roblox
                .priority
                .map(|priority| priority.label())
                .unwrap_or("Unknown"),
        );
        row(&mut out, "Threads", &roblox.threads.to_string());
        row(
            &mut out,
            "Session",
            &roblox
                .uptime_secs
                .map(format::duration)
                .unwrap_or_else(|| "Unknown".to_owned()),
        );
    } else {
        row(&mut out, "Status", "Not running");
    }
    if let Some(install) = snapshot.installs.first() {
        row(&mut out, "Client build", &install.build);
    }
    row(&mut out, "Frame rate", &fps_line(snapshot));

    section(&mut out, "OPTIMIZATIONS");
    for item in ITEMS {
        let active = snapshot
            .items
            .iter()
            .any(|state| state.id == item.id && state.active);
        row(&mut out, item.title, if active { "Active" } else { "Off" });
    }
    row(
        &mut out,
        "Recorded changes",
        &recorded_changes(state).to_string(),
    );
    row(
        &mut out,
        "Apply when Roblox starts",
        if state.auto_apply { "On" } else { "Off" },
    );
    if let Some(last) = state.last_optimized_unix {
        row(
            &mut out,
            "Last optimized",
            &format!(
                "{} ({} run{})",
                stamp(last),
                state.optimize_runs,
                if state.optimize_runs == 1 { "" } else { "s" }
            ),
        );
    }

    if !state.backups.is_empty() {
        section(&mut out, "RECORDED REGISTRY CHANGES");
        for backup in &state.backups {
            let original = backup
                .original
                .map(|value| value.to_string())
                .unwrap_or_else(|| "not set".to_owned());
            out.push_str(&format!(
                "  HKEY_CURRENT_USER\\{}\\{}\n    {original} -> {}\n",
                backup.path, backup.name, backup.applied
            ));
        }
    }

    section(&mut out, "CLEANUP");
    match &state.last_cleanup {
        Some(last) => row(
            &mut out,
            "Last run",
            &format!(
                "{} across {} file{} ({})",
                format::bytes(last.bytes),
                last.files,
                if last.files == 1 { "" } else { "s" },
                stamp(last.at_unix)
            ),
        ),
        None => row(&mut out, "Last run", "Never"),
    }
    row(
        &mut out,
        "Lifetime",
        &format::bytes(state.lifetime_cleaned_bytes),
    );

    out.push_str("\nGenerated locally by Volt. Nothing in this report was sent anywhere.\n");
    out
}

fn fps_line(snapshot: &Snapshot) -> String {
    match (&snapshot.fps.measured, snapshot.fps.status) {
        (Some(measured), _) => format!(
            "{} fps measured {}s ago from Roblox's own log",
            format::fps(measured.fps),
            measured.age_secs
        ),
        (None, FpsStatus::NotRunning) => "- (Roblox is not running)".to_owned(),
        (None, FpsStatus::Waiting) => "- (waiting for the Roblox session log)".to_owned(),
        (None, _) => "- (not exposed by Roblox, never estimated)".to_owned(),
    }
}

fn recorded_changes(state: &SavedState) -> usize {
    state.backups.len() + usize::from(state.power_scheme.is_some()) + state.priority_pids.len()
}

fn reported(value: &str) -> &str {
    if value.is_empty() {
        "Not reported"
    } else {
        value
    }
}

fn section(out: &mut String, title: &str) {
    out.push('\n');
    out.push_str(title);
    out.push('\n');
}

fn row(out: &mut String, label: &str, value: &str) {
    out.push_str(&format!("  {label:<LABEL_WIDTH$} {value}\n"));
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::RegBackup;

    fn row_ends_with(report: &str, label: &str, value: &str) -> bool {
        report
            .lines()
            .any(|line| line.trim_start().starts_with(label) && line.trim_end().ends_with(value))
    }

    #[test]
    fn stamps_known_instants() {
        assert_eq!(stamp(0), "1970-01-01 00:00:00 UTC");
        assert_eq!(stamp(1_700_000_000), "2023-11-14 22:13:20 UTC");
        assert_eq!(stamp(951_782_400), "2000-02-29 00:00:00 UTC");
        assert_eq!(file_stamp(1_700_000_000), "20231114-221320");
    }

    #[test]
    fn reports_measurements_not_guesses() {
        let snapshot = Snapshot::default();
        let report = build(&snapshot, &SavedState::default());

        assert!(report.contains("Volt "));
        assert!(report.contains("never estimates frame rate"));
        assert!(report.contains("- (Roblox is not running)"));
        assert!(row_ends_with(&report, "Recorded changes", "0"));
        assert!(report.contains("Generated locally by Volt"));
    }

    #[test]
    fn lists_the_recorded_ledger() {
        let mut state = SavedState::default();
        state.backups.push(RegBackup {
            path: "Software\\Microsoft\\GameBar".into(),
            name: "AutoGameModeEnabled".into(),
            original: None,
            applied: 1,
        });

        let report = build(&Snapshot::default(), &state);
        assert!(
            report.contains("HKEY_CURRENT_USER\\Software\\Microsoft\\GameBar\\AutoGameModeEnabled")
        );
        assert!(report.contains("not set -> 1"));
        assert!(row_ends_with(&report, "Recorded changes", "1"));
    }
}
