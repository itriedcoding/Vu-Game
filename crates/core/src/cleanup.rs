//! Roblox cleanup: session logs, crash dumps and temporary Roblox files.
//!
//! The scope is deliberately narrow. Volt removes only files Roblox wrote for
//! diagnostics and leftovers in `%TEMP%`; installed client content, assets and
//! game files are never touched. Files modified in the last two minutes are
//! treated as in use and skipped.

use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

#[derive(Debug, Clone, Copy)]
pub struct TargetSpec {
    pub id: &'static str,
    pub label: &'static str,
    pub summary: &'static str,
}

pub const TARGETS: &[TargetSpec] = &[
    TargetSpec {
        id: "logs",
        label: "Session logs and crash dumps",
        summary:
            "Roblox Player logs, MicroProfiler captures and crash dumps older than two minutes.",
    },
    TargetSpec {
        id: "temp",
        label: "Temporary Roblox files",
        summary:
            "Leftover Roblox entries in the Windows temp folder. Files still in use are skipped.",
    },
];

/// Files younger than this are considered in use.
const MIN_AGE: Duration = Duration::from_secs(120);
const LOG_EXTENSIONS: [&str; 3] = ["log", "dmp", "html"];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileEntry {
    pub path: PathBuf,
    pub size: u64,
}

#[derive(Debug, Clone)]
pub struct TargetScan {
    pub id: &'static str,
    pub label: &'static str,
    pub summary: &'static str,
    pub files: Vec<FileEntry>,
    pub dirs: Vec<PathBuf>,
    pub skipped: u64,
}

impl TargetScan {
    fn new(spec: &TargetSpec) -> Self {
        Self {
            id: spec.id,
            label: spec.label,
            summary: spec.summary,
            files: Vec::new(),
            dirs: Vec::new(),
            skipped: 0,
        }
    }

    pub fn bytes(&self) -> u64 {
        self.files.iter().map(|file| file.size).sum()
    }

    pub fn count(&self) -> u64 {
        self.files.len() as u64
    }
}

#[derive(Debug, Clone, Default)]
pub struct CleanupScan {
    pub targets: Vec<TargetScan>,
}

impl CleanupScan {
    pub fn bytes(&self) -> u64 {
        self.targets.iter().map(TargetScan::bytes).sum()
    }

    pub fn files(&self) -> u64 {
        self.targets.iter().map(TargetScan::count).sum()
    }

    pub fn is_empty(&self) -> bool {
        self.files() == 0
    }
}

#[derive(Debug, Clone, Default)]
pub struct TargetResult {
    pub id: &'static str,
    pub label: &'static str,
    pub files: u64,
    pub bytes: u64,
}

#[derive(Debug, Clone, Default)]
pub struct CleanupReport {
    pub files: u64,
    pub bytes: u64,
    pub failed: u64,
    pub samples: Vec<String>,
    pub targets: Vec<TargetResult>,
}

impl CleanupReport {
    pub fn is_empty(&self) -> bool {
        self.files == 0 && self.failed == 0
    }
}

/// Scan Roblox's diagnostic folders without deleting anything.
pub fn scan(local_app_data: &Path, temp: &Path, now: SystemTime) -> CleanupScan {
    let mut scan = CleanupScan {
        targets: TARGETS.iter().map(TargetScan::new).collect(),
    };

    let logs = crate::roblox::logs_dir(local_app_data);
    if let (Ok(entries), Some(target)) = (
        std::fs::read_dir(&logs),
        scan.targets.iter_mut().find(|target| target.id == "logs"),
    ) {
        for entry in entries.flatten() {
            let path = entry.path();
            let Ok(metadata) = entry.metadata() else {
                target.skipped += 1;
                continue;
            };
            if !metadata.is_file() {
                continue;
            }
            let extension = path
                .extension()
                .and_then(|value| value.to_str())
                .unwrap_or("")
                .to_ascii_lowercase();
            if !LOG_EXTENSIONS.contains(&extension.as_str()) {
                continue;
            }
            if metadata.len() == 0 {
                continue;
            }
            if recently_modified(metadata.modified().ok(), now) {
                target.skipped += 1;
                continue;
            }
            target.files.push(FileEntry {
                path,
                size: metadata.len(),
            });
        }
    }

    if let (Ok(entries), Some(target)) = (
        std::fs::read_dir(temp),
        scan.targets.iter_mut().find(|target| target.id == "temp"),
    ) {
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().to_ascii_lowercase();
            if !name.starts_with("roblox") {
                continue;
            }
            collect(&entry.path(), target, now);
        }
    }

    scan
}

fn collect(path: &Path, target: &mut TargetScan, now: SystemTime) {
    let Ok(metadata) = std::fs::symlink_metadata(path) else {
        target.skipped += 1;
        return;
    };
    if metadata.file_type().is_symlink() {
        return;
    }
    if metadata.is_dir() {
        target.dirs.push(path.to_path_buf());
        match std::fs::read_dir(path) {
            Ok(entries) => {
                for entry in entries.flatten() {
                    collect(&entry.path(), target, now);
                }
            }
            Err(_) => target.skipped += 1,
        }
    } else if metadata.is_file() {
        if recently_modified(metadata.modified().ok(), now) {
            target.skipped += 1;
            return;
        }
        target.files.push(FileEntry {
            path: path.to_path_buf(),
            size: metadata.len(),
        });
    }
}

fn recently_modified(modified: Option<SystemTime>, now: SystemTime) -> bool {
    match modified {
        Some(stamp) => match now.duration_since(stamp) {
            Ok(age) => age < MIN_AGE,
            // Modified in the future: assume it is active.
            Err(_) => true,
        },
        None => true,
    }
}

/// Delete the files found by [`scan`]. In-use files are reported, never forced.
pub fn clean(scan: &CleanupScan) -> CleanupReport {
    let mut report = CleanupReport::default();
    let mut failures = 0u64;

    for target in &scan.targets {
        let mut result = TargetResult {
            id: target.id,
            label: target.label,
            files: 0,
            bytes: 0,
        };

        for file in &target.files {
            match std::fs::remove_file(&file.path) {
                Ok(()) => {
                    result.files += 1;
                    result.bytes += file.size;
                }
                Err(err) => {
                    failures += 1;
                    if report.samples.len() < 3 {
                        report.samples.push(format!(
                            "{} — {}",
                            file.path
                                .file_name()
                                .map(|name| name.to_string_lossy().to_string())
                                .unwrap_or_else(|| file.path.display().to_string()),
                            err
                        ));
                    }
                }
            }
        }

        // Remove now-empty folders, deepest first.
        for dir in target.dirs.iter().rev() {
            let _ = std::fs::remove_dir(dir);
        }

        report.files += result.files;
        report.bytes += result.bytes;
        report.targets.push(result);
    }

    report.failed = failures;
    report
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "volt-cleanup-{name}-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("scratch dir");
        dir
    }

    fn age_file(path: &Path, secs: u64) {
        let file = std::fs::OpenOptions::new()
            .write(true)
            .open(path)
            .expect("open file");
        file.set_modified(SystemTime::now() - Duration::from_secs(secs))
            .expect("set mtime");
    }

    #[test]
    fn finds_old_logs_and_skips_fresh_ones() {
        let root = scratch("logs");
        let local = root.join("local");
        let temp = root.join("temp");
        let logs = local.join("Roblox").join("logs");
        std::fs::create_dir_all(&logs).expect("logs dir");
        std::fs::create_dir_all(&temp).expect("temp dir");

        let old = logs.join("0.600.0_20240101T000000Z_Player_abc_last.log");
        let fresh = logs.join("0.600.0_20240102T000000Z_Player_def_last.log");
        std::fs::write(&old, "x".repeat(2048)).expect("write");
        std::fs::write(&fresh, "y".repeat(1024)).expect("write");
        age_file(&old, 3_600);

        let scan = scan(&local, &temp, SystemTime::now());
        assert_eq!(scan.files(), 1);
        assert_eq!(scan.bytes(), 2048);
        assert_eq!(scan.targets[0].skipped, 1);

        let report = clean(&scan);
        assert_eq!(report.files, 1);
        assert_eq!(report.bytes, 2048);
        assert_eq!(report.failed, 0);
        assert!(!old.exists());
        assert!(fresh.exists());

        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn cleans_roblox_temp_folders_only() {
        let root = scratch("temp");
        let local = root.join("local");
        let temp = root.join("temp");
        std::fs::create_dir_all(&local).expect("local");
        let nested = temp.join("Roblox").join("cache");
        std::fs::create_dir_all(&nested).expect("nested");
        let payload = nested.join("data.bin");
        std::fs::write(&payload, vec![0u8; 4096]).expect("write");
        age_file(&payload, 3_600);
        let unrelated = temp.join("other-app.tmp");
        std::fs::write(&unrelated, b"keep").expect("write");

        let scan = scan(&local, &temp, SystemTime::now());
        assert_eq!(scan.files(), 1);

        let report = clean(&scan);
        assert_eq!(report.files, 1);
        assert_eq!(report.bytes, 4096);
        assert!(!payload.exists());
        assert!(unrelated.exists());

        let _ = std::fs::remove_dir_all(&root);
    }
}
