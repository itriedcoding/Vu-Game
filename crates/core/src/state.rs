//! Persistent record of every change Volt made, so Restore Defaults always knows
//! what to put back. Stored as JSON in `%APPDATA%\Volt\state.json`.

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

pub const STATE_VERSION: u32 = 1;

/// A registry value Volt changed, with the value it had before.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RegBackup {
    pub path: String,
    pub name: String,
    /// `None` means the value did not exist before Volt created it.
    pub original: Option<u32>,
    pub applied: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CleanupSummary {
    pub files: u64,
    pub bytes: u64,
    pub at_unix: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SavedState {
    #[serde(default)]
    pub version: u32,
    /// Registry values to restore.
    #[serde(default)]
    pub backups: Vec<RegBackup>,
    /// Power plan that was active before Volt changed it.
    #[serde(default)]
    pub power_scheme: Option<String>,
    /// Optimisations that were applied at least once.
    #[serde(default)]
    pub applied: Vec<String>,
    /// Roblox processes whose priority Volt changed.
    #[serde(default)]
    pub priority_pids: Vec<u32>,
    #[serde(default)]
    pub last_cleanup: Option<CleanupSummary>,
    #[serde(default)]
    pub lifetime_cleaned_bytes: u64,
    #[serde(default)]
    pub optimize_runs: u32,
    #[serde(default)]
    pub last_optimized_unix: Option<u64>,
    /// Re-apply the selected tweaks whenever a new Roblox Player process appears.
    #[serde(default)]
    pub auto_apply: bool,
}

impl Default for SavedState {
    fn default() -> Self {
        Self {
            version: STATE_VERSION,
            backups: Vec::new(),
            power_scheme: None,
            applied: Vec::new(),
            priority_pids: Vec::new(),
            last_cleanup: None,
            lifetime_cleaned_bytes: 0,
            optimize_runs: 0,
            last_optimized_unix: None,
            auto_apply: false,
        }
    }
}

impl SavedState {
    /// True when something is recorded that Restore Defaults can undo.
    pub fn has_backups(&self) -> bool {
        !self.backups.is_empty() || self.power_scheme.is_some() || !self.priority_pids.is_empty()
    }

    /// Remember that optimisations were applied just now.
    pub fn mark_optimized(&mut self) {
        self.optimize_runs = self.optimize_runs.saturating_add(1);
        self.last_optimized_unix = Some(crate::now_unix());
    }

    /// Record a cleanup result in the running totals.
    pub fn record_cleanup(&mut self, files: u64, bytes: u64) {
        self.lifetime_cleaned_bytes = self.lifetime_cleaned_bytes.saturating_add(bytes);
        self.last_cleanup = Some(CleanupSummary {
            files,
            bytes,
            at_unix: crate::now_unix(),
        });
    }
}

/// `%APPDATA%\Volt\state.json` on Windows.
pub fn state_path() -> Option<PathBuf> {
    Some(crate::platform::app_data()?.join("Volt").join("state.json"))
}

pub fn load() -> SavedState {
    let Some(path) = state_path() else {
        return SavedState::default();
    };
    let Ok(text) = std::fs::read_to_string(path) else {
        return SavedState::default();
    };
    serde_json::from_str(&text).unwrap_or_default()
}

/// Write the state atomically (temp file + rename).
pub fn save(state: &SavedState) -> Result<(), String> {
    let path =
        state_path().ok_or_else(|| "The Windows profile folder is unavailable.".to_owned())?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|err| format!("Could not create {}: {err}", parent.display()))?;
    }
    let text = serde_json::to_string_pretty(state).map_err(|err| err.to_string())?;
    let temporary = path.with_extension("json.tmp");
    std::fs::write(&temporary, text).map_err(|err| format!("Could not write state: {err}"))?;
    std::fs::rename(&temporary, &path).map_err(|err| format!("Could not save state: {err}"))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_are_empty() {
        let state = SavedState::default();
        assert!(!state.has_backups());
        assert_eq!(state.version, STATE_VERSION);
        assert!(state.applied.is_empty());
    }

    #[test]
    fn round_trips_through_json() {
        let mut state = SavedState::default();
        state.backups.push(RegBackup {
            path: "Software\\Microsoft\\GameBar".into(),
            name: "AutoGameModeEnabled".into(),
            original: None,
            applied: 1,
        });
        state.power_scheme = Some("381b4222-f694-41f0-9685-ff5bb260df2e".into());
        state.priority_pids.push(4242);
        state.applied.push("game_mode".into());
        state.record_cleanup(12, 4096);

        let text = serde_json::to_string(&state).expect("serialise");
        let restored: SavedState = serde_json::from_str(&text).expect("deserialise");

        assert!(restored.has_backups());
        assert_eq!(restored.backups.len(), 1);
        assert_eq!(restored.backups[0].original, None);
        assert_eq!(restored.power_scheme, state.power_scheme);
        assert_eq!(restored.lifetime_cleaned_bytes, 4096);
        assert!(restored.last_cleanup.is_some());
    }

    #[test]
    fn tolerates_partial_files() {
        let restored: SavedState = serde_json::from_str("{\"version\":1}").expect("parse");
        assert_eq!(restored.version, 1);
        assert!(!restored.has_backups());
        assert!(!restored.auto_apply);
    }

    #[test]
    fn keeps_the_auto_apply_preference() {
        let state = SavedState {
            auto_apply: true,
            ..SavedState::default()
        };
        let text = serde_json::to_string(&state).expect("serialise");
        let restored: SavedState = serde_json::from_str(&text).expect("deserialise");
        assert!(restored.auto_apply);
    }
}
