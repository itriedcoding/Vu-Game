//! Roblox discovery: processes, installations and log locations.
//!
//! Volt only reads Roblox metadata. It never opens, patches or injects anything
//! into the Roblox client.

use crate::model::RobloxInstall;
use std::path::{Path, PathBuf};

pub const PLAYER_EXECUTABLES: [&str; 2] = ["robloxplayerbeta.exe", "robloxplayer.exe"];

/// Roblox Player keeps its per-version folders here.
pub fn versions_dir(local_app_data: &Path) -> PathBuf {
    local_app_data.join("Roblox").join("Versions")
}

/// Roblox Player writes session logs and crash dumps here.
pub fn logs_dir(local_app_data: &Path) -> PathBuf {
    local_app_data.join("Roblox").join("logs")
}

pub fn is_player_process(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    PLAYER_EXECUTABLES.contains(&lower.as_str())
}

/// Turn a `version-<hash>` folder name into a short label.
pub fn build_label(folder_name: &str) -> String {
    let hash = folder_name
        .strip_prefix("version-")
        .or_else(|| folder_name.strip_prefix("Version-"))
        .unwrap_or(folder_name);
    let short: String = hash.chars().take(8).collect();
    format!("build {short}")
}

/// All Roblox Player installations found on disk, newest first.
pub fn installs(local_app_data: &Path) -> Vec<RobloxInstall> {
    let root = versions_dir(local_app_data);
    let Ok(entries) = std::fs::read_dir(&root) else {
        return Vec::new();
    };

    let mut found = Vec::new();
    for entry in entries.flatten() {
        let dir = entry.path();
        if !dir.is_dir() {
            continue;
        }
        let folder = entry.file_name().to_string_lossy().to_string();
        if !folder.to_ascii_lowercase().starts_with("version-") {
            continue;
        }
        for exe_name in ["RobloxPlayerBeta.exe", "RobloxPlayer.exe"] {
            let exe = dir.join(exe_name);
            if exe.is_file() {
                found.push(RobloxInstall {
                    build: build_label(&folder),
                    dir: dir.clone(),
                    exe,
                });
                break;
            }
        }
    }

    found.sort_by_key(|install| std::cmp::Reverse(modified_at(&install.exe)));
    found
}

/// PID of the Roblox Player instance holding the most memory, if any is running.
/// Volt re-checks this immediately before touching a process.
pub fn primary_player_pid() -> Option<u32> {
    crate::platform::list_processes()
        .ok()?
        .into_iter()
        .filter(|process| is_player_process(&process.name))
        .max_by_key(|process| process.memory)
        .map(|process| process.pid)
}

/// The newest Roblox Player session log, if one exists.
pub fn latest_log(dir: &Path) -> Option<PathBuf> {
    let entries = std::fs::read_dir(dir).ok()?;
    let mut newest: Option<(std::time::SystemTime, PathBuf)> = None;

    for entry in entries.flatten() {
        let path = entry.path();
        let name = entry.file_name().to_string_lossy().to_ascii_lowercase();
        if !name.ends_with(".log") || !name.contains("player") {
            continue;
        }
        let Ok(metadata) = entry.metadata() else {
            continue;
        };
        if !metadata.is_file() {
            continue;
        }
        let stamp = modified_at(&path);
        if newest
            .as_ref()
            .map(|(best, _)| stamp > *best)
            .unwrap_or(true)
        {
            newest = Some((stamp, path));
        }
    }

    newest.map(|(_, path)| path)
}

fn modified_at(path: &Path) -> std::time::SystemTime {
    std::fs::metadata(path)
        .and_then(|metadata| metadata.modified())
        .unwrap_or(std::time::UNIX_EPOCH)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recognises_player_executables() {
        assert!(is_player_process("RobloxPlayerBeta.exe"));
        assert!(is_player_process("robloxplayerbeta.EXE"));
        assert!(is_player_process("RobloxPlayer.exe"));
        assert!(!is_player_process("RobloxStudioBeta.exe"));
        assert!(!is_player_process("notrobloxplayerbeta.exe"));
    }

    #[test]
    fn builds_short_labels() {
        assert_eq!(build_label("version-6f2a91cd99"), "build 6f2a91cd");
        assert_eq!(build_label("unexpected"), "build unexpect");
    }

    #[test]
    fn finds_newest_player_log() {
        let root = std::env::temp_dir().join(format!("volt-log-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).expect("temp dir");

        let older = root.join("0.500.0_20240101T000000Z_Player_abc_last.log");
        let newer = root.join("0.600.0_20240102T000000Z_Player_def_last.log");
        let studio = root.join("0.600.0_20240102T000000Z_Studio_def_last.log");
        std::fs::write(&older, "old").expect("write log");
        std::fs::write(&newer, "new").expect("write log");
        std::fs::write(&studio, "studio").expect("write log");

        let now = std::time::SystemTime::now();
        std::fs::OpenOptions::new()
            .write(true)
            .open(&older)
            .expect("open old log")
            .set_modified(now - std::time::Duration::from_secs(3_600))
            .expect("age old log");
        std::fs::OpenOptions::new()
            .write(true)
            .open(&studio)
            .expect("open studio log")
            .set_modified(now - std::time::Duration::from_secs(1_800))
            .expect("age studio log");

        let found = latest_log(&root).expect("log found");
        assert_eq!(found.file_name(), newer.file_name());

        let _ = std::fs::remove_dir_all(&root);
    }
}
