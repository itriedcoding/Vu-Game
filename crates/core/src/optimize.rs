//! The optimisation catalogue and its apply/restore engine.
//!
//! Every entry here is a real, reversible change:
//! * process priority for Roblox Player,
//! * per-user Game Bar / Game DVR settings in `HKEY_CURRENT_USER`,
//! * the Windows power plan.
//!
//! Volt never edits Roblox files, never touches Defender or Windows services and
//! never writes machine-wide policy keys.

use crate::model::{ItemState, Priority};
use crate::platform;
use crate::state::{RegBackup, SavedState};

pub const HIGH_PERFORMANCE_SCHEME: &str = "8c5e7fda-e8bf-4a96-9a85-a6e23a8c635c";

const GAMEDVR_PATH: &str = r"Software\Microsoft\Windows\CurrentVersion\GameDVR";
const GAMECONFIG_PATH: &str = r"System\GameConfigStore";
const GAMEBAR_PATH: &str = r"Software\Microsoft\GameBar";

#[derive(Debug, Clone)]
pub struct Item {
    pub id: &'static str,
    pub title: &'static str,
    pub summary: &'static str,
    pub note: &'static str,
    pub needs_roblox: bool,
}

pub const ITEMS: &[Item] = &[
    Item {
        id: "roblox_priority",
        title: "Roblox process priority",
        summary: "Runs Roblox Player at Above Normal priority so Windows schedules its threads ahead of background work.",
        note: "Applied to the running client only. Restore Defaults puts it back to Normal.",
        needs_roblox: true,
    },
    Item {
        id: "game_dvr",
        title: "Background recording",
        summary: "Turns off Game Bar's background clip recording for your Windows user, which keeps a capture pipeline active while you play.",
        note: "Per-user setting in HKEY_CURRENT_USER. Windows applies it the next time a game starts.",
        needs_roblox: false,
    },
    Item {
        id: "game_mode",
        title: "Windows Game Mode",
        summary: "Makes sure Game Mode is enabled so Windows gives the foreground game priority for CPU and GPU work.",
        note: "Per-user setting in HKEY_CURRENT_USER.",
        needs_roblox: false,
    },
    Item {
        id: "game_bar_startup",
        title: "Game Bar startup panel",
        summary: "Stops the Game Bar panel from opening automatically when a game starts.",
        note: "Per-user setting in HKEY_CURRENT_USER.",
        needs_roblox: false,
    },
    Item {
        id: "power_plan",
        title: "High performance power plan",
        summary: "Switches Windows to the High performance plan so the CPU keeps its clocks up instead of parking cores.",
        note: "Some laptops and OEM tools block third-party power plan changes; Volt reports that instead of hiding it.",
        needs_roblox: false,
    },
];

pub fn item(id: &str) -> Option<&'static Item> {
    ITEMS.iter().find(|item| item.id == id)
}

/// Registry values behind each catalogue entry: (key path, value name, target).
fn tweaks_for(id: &str) -> &'static [(&'static str, &'static str, u32)] {
    match id {
        "game_dvr" => &[
            (GAMEDVR_PATH, "AppCaptureEnabled", 0),
            (GAMECONFIG_PATH, "GameDVR_Enabled", 0),
        ],
        "game_mode" => &[(GAMEBAR_PATH, "AutoGameModeEnabled", 1)],
        "game_bar_startup" => &[(GAMEBAR_PATH, "ShowStartupPanel", 0)],
        _ => &[],
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StepStatus {
    Applied,
    Restored,
    AlreadyActive,
    Skipped,
    NothingToRestore,
    Failed,
}

impl StepStatus {
    pub fn label(self) -> &'static str {
        match self {
            StepStatus::Applied => "Applied",
            StepStatus::Restored => "Restored",
            StepStatus::AlreadyActive => "Already on",
            StepStatus::Skipped => "Skipped",
            StepStatus::NothingToRestore => "Nothing to restore",
            StepStatus::Failed => "Failed",
        }
    }

    pub fn is_positive(self) -> bool {
        matches!(
            self,
            StepStatus::Applied | StepStatus::Restored | StepStatus::AlreadyActive
        )
    }
}

#[derive(Debug, Clone)]
pub struct Step {
    pub id: &'static str,
    pub title: &'static str,
    pub status: StepStatus,
    pub detail: String,
}

#[derive(Debug, Clone, Default)]
pub struct ApplyReport {
    pub steps: Vec<Step>,
}

impl ApplyReport {
    pub fn applied(&self) -> usize {
        self.steps
            .iter()
            .filter(|step| step.status == StepStatus::Applied)
            .count()
    }

    pub fn failures(&self) -> usize {
        self.steps
            .iter()
            .filter(|step| step.status == StepStatus::Failed)
            .count()
    }

    pub fn is_empty(&self) -> bool {
        self.steps.is_empty()
    }
}

#[derive(Debug, Clone, Default)]
pub struct RestoreReport {
    pub steps: Vec<Step>,
    pub restored: usize,
}

impl RestoreReport {
    pub fn failures(&self) -> usize {
        self.steps
            .iter()
            .filter(|step| step.status == StepStatus::Failed)
            .count()
    }
}

/// Apply the selected optimisations. Never blocks longer than a few registry and
/// process calls, but the caller still runs it off the UI thread.
pub fn apply(ids: &[String], roblox_pid: Option<u32>, state: &mut SavedState) -> ApplyReport {
    let mut report = ApplyReport::default();
    let mut applied_any = false;

    for item in ITEMS {
        if !ids.iter().any(|id| id == item.id) {
            continue;
        }

        let mut step = Step {
            id: item.id,
            title: item.title,
            status: StepStatus::Skipped,
            detail: String::new(),
        };

        match item.id {
            "roblox_priority" => apply_priority(&mut step, roblox_pid, state),
            "power_plan" => apply_power_plan(&mut step, state),
            id => apply_registry(&mut step, id, state),
        }

        if step.status.is_positive() && !state.applied.iter().any(|id| id == item.id) {
            state.applied.push(item.id.to_owned());
        }
        applied_any |= step.status == StepStatus::Applied;
        report.steps.push(step);
    }

    if applied_any {
        state.mark_optimized();
    }
    report
}

fn apply_registry(step: &mut Step, id: &str, state: &mut SavedState) {
    let mut changed = 0usize;
    let mut already = 0usize;
    let mut failures: Vec<String> = Vec::new();

    for (path, name, target) in tweaks_for(id) {
        let original = platform::registry_dword(path, name);
        if original == Some(*target) {
            already += 1;
            continue;
        }

        let recorded = state
            .backups
            .iter()
            .any(|backup| backup.path == *path && backup.name == *name);
        if !recorded {
            state.backups.push(RegBackup {
                path: (*path).to_owned(),
                name: (*name).to_owned(),
                original,
                applied: *target,
            });
        }

        match platform::set_registry_dword(path, name, *target) {
            Ok(()) => changed += 1,
            Err(err) => {
                failures.push(err.to_string());
                if !recorded {
                    state
                        .backups
                        .retain(|backup| !(backup.path == *path && backup.name == *name));
                }
            }
        }
    }

    if !failures.is_empty() {
        step.status = StepStatus::Failed;
        step.detail = failures.join(" ");
    } else if changed > 0 {
        step.status = StepStatus::Applied;
        step.detail = format!("Updated {changed} Windows setting{}.", plural(changed));
    } else if already > 0 {
        step.status = StepStatus::AlreadyActive;
        step.detail = "Already in the optimised state.".to_owned();
    } else {
        step.status = StepStatus::Skipped;
        step.detail = "Nothing to change on this system.".to_owned();
    }
}

fn apply_power_plan(step: &mut Step, state: &mut SavedState) {
    let current = platform::active_power_scheme();
    if current.as_deref() == Some(HIGH_PERFORMANCE_SCHEME) {
        step.status = StepStatus::AlreadyActive;
        step.detail = "High performance is already the active plan.".to_owned();
        return;
    }

    match platform::set_active_power_scheme(HIGH_PERFORMANCE_SCHEME) {
        Ok(()) => {
            if state.power_scheme.is_none() {
                state.power_scheme = current;
            }
            step.status = StepStatus::Applied;
            step.detail = "Windows is now using the High performance plan.".to_owned();
        }
        Err(err) => {
            step.status = StepStatus::Failed;
            step.detail = err.to_string();
        }
    }
}

fn apply_priority(step: &mut Step, roblox_pid: Option<u32>, state: &mut SavedState) {
    let Some(pid) = roblox_pid else {
        step.status = StepStatus::Skipped;
        step.detail = "Roblox Player is not running. Start it, then run Optimize again.".to_owned();
        return;
    };

    let processes = platform::list_processes().unwrap_or_default();
    let is_roblox = processes
        .iter()
        .any(|process| process.pid == pid && crate::roblox::is_player_process(&process.name));
    if !is_roblox {
        step.status = StepStatus::Skipped;
        step.detail = "Roblox Player is not running.".to_owned();
        return;
    }

    if let Some(current) = platform::priority(pid) {
        if current >= Priority::AboveNormal {
            step.status = StepStatus::AlreadyActive;
            step.detail = format!("Roblox is already at {} priority.", current.label());
            return;
        }
    }

    match platform::set_priority(pid, Priority::AboveNormal) {
        Ok(()) => {
            if !state.priority_pids.contains(&pid) {
                state.priority_pids.push(pid);
            }
            step.status = StepStatus::Applied;
            step.detail = "Roblox now runs at Above Normal priority.".to_owned();
        }
        Err(err) => {
            step.status = StepStatus::Failed;
            step.detail = err.to_string();
        }
    }
}

/// Undo everything Volt recorded. Failures keep their backup so a retry works.
pub fn restore(state: &mut SavedState) -> RestoreReport {
    let mut report = RestoreReport::default();

    for item in ITEMS {
        let mut step = Step {
            id: item.id,
            title: item.title,
            status: StepStatus::NothingToRestore,
            detail: "Volt has no recorded change for this setting.".to_owned(),
        };

        match item.id {
            "roblox_priority" => restore_priority(&mut step, state),
            "power_plan" => restore_power_plan(&mut step, state),
            id => restore_registry(&mut step, id, state),
        }

        if step.status == StepStatus::Restored {
            report.restored += 1;
            state.applied.retain(|applied| applied != item.id);
        }
        if step.status != StepStatus::NothingToRestore {
            report.steps.push(step);
        }
    }

    report
}

fn restore_registry(step: &mut Step, id: &str, state: &mut SavedState) {
    let tweaks = tweaks_for(id);
    let keys: Vec<(String, String)> = state
        .backups
        .iter()
        .filter(|backup| {
            tweaks
                .iter()
                .any(|(path, name, _)| backup.path == *path && backup.name == *name)
        })
        .map(|backup| (backup.path.clone(), backup.name.clone()))
        .collect();

    if keys.is_empty() {
        return;
    }

    let mut failures: Vec<String> = Vec::new();
    let mut restored = 0usize;

    for (path, name) in &keys {
        let Some(backup) = state
            .backups
            .iter()
            .find(|backup| backup.path == *path && backup.name == *name)
            .cloned()
        else {
            continue;
        };

        let result = match backup.original {
            Some(value) => platform::set_registry_dword(path, name, value),
            None => platform::delete_registry_value(path, name),
        };

        match result {
            Ok(()) => {
                restored += 1;
                state
                    .backups
                    .retain(|entry| !(entry.path == *path && entry.name == *name));
            }
            Err(err) => failures.push(err.to_string()),
        }
    }

    if !failures.is_empty() {
        step.status = StepStatus::Failed;
        step.detail = format!(
            "{restored} of {} values restored. {}",
            keys.len(),
            failures.join(" ")
        );
    } else {
        step.status = StepStatus::Restored;
        step.detail = "Returned to the value Windows had before Volt changed it.".to_owned();
    }
}

fn restore_power_plan(step: &mut Step, state: &mut SavedState) {
    let Some(scheme) = state.power_scheme.clone() else {
        return;
    };

    match platform::set_active_power_scheme(&scheme) {
        Ok(()) => {
            state.power_scheme = None;
            step.status = StepStatus::Restored;
            step.detail = "Switched back to your previous power plan.".to_owned();
        }
        Err(err) => {
            step.status = StepStatus::Failed;
            step.detail = err.to_string();
        }
    }
}

fn restore_priority(step: &mut Step, state: &mut SavedState) {
    if state.priority_pids.is_empty() {
        return;
    }

    let pids = state.priority_pids.clone();
    let mut failures: Vec<String> = Vec::new();
    let mut restored = 0usize;
    let mut remaining = Vec::new();

    // A recorded PID can be reused by an unrelated process after Roblox exits, so
    // only touch processes that are still Roblox Player right now.
    let players: Vec<u32> = platform::list_processes()
        .unwrap_or_default()
        .into_iter()
        .filter(|process| crate::roblox::is_player_process(&process.name))
        .map(|process| process.pid)
        .collect();

    for pid in pids {
        if !players.contains(&pid) {
            // The client exited: nothing left to restore for that process.
            restored += 1;
            continue;
        }
        match platform::priority(pid) {
            None => restored += 1,
            Some(Priority::Normal) => restored += 1,
            Some(_) => match platform::set_priority(pid, Priority::Normal) {
                Ok(()) => restored += 1,
                Err(err) => {
                    failures.push(err.to_string());
                    remaining.push(pid);
                }
            },
        }
    }

    state.priority_pids = remaining;

    if !failures.is_empty() {
        step.status = StepStatus::Failed;
        step.detail = failures.join(" ");
    } else {
        step.status = StepStatus::Restored;
        step.detail = "Roblox is back to Normal priority.".to_owned();
        let _ = restored;
    }
}

/// Live state of every catalogue entry, read straight from Windows.
pub fn verify(roblox_priority: Option<Priority>, power_scheme: Option<String>) -> Vec<ItemState> {
    ITEMS
        .iter()
        .map(|item| ItemState {
            id: item.id,
            active: match item.id {
                "roblox_priority" => roblox_priority
                    .map(|priority| priority >= Priority::AboveNormal)
                    .unwrap_or(false),
                "power_plan" => power_scheme.as_deref() == Some(HIGH_PERFORMANCE_SCHEME),
                id => tweaks_for(id).iter().all(|(path, name, target)| {
                    platform::registry_dword(path, name) == Some(*target)
                }),
            },
        })
        .collect()
}

fn plural(count: usize) -> &'static str {
    if count == 1 {
        ""
    } else {
        "s"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn catalogue_ids_are_unique_and_looked_up() {
        let mut seen = Vec::new();
        for entry in ITEMS {
            assert!(!seen.contains(&entry.id), "duplicate id {}", entry.id);
            seen.push(entry.id);
            assert_eq!(item(entry.id).map(|found| found.title), Some(entry.title));
        }
        assert!(item("does_not_exist").is_none());
    }

    #[test]
    fn registry_backed_items_have_tweaks() {
        for id in ["game_dvr", "game_mode", "game_bar_startup"] {
            assert!(!tweaks_for(id).is_empty(), "{id} needs a registry tweak");
        }
        assert!(tweaks_for("roblox_priority").is_empty());
        assert!(tweaks_for("power_plan").is_empty());
    }

    #[test]
    fn empty_selection_changes_nothing() {
        let mut state = SavedState::default();
        let report = apply(&[], Some(1234), &mut state);
        assert!(report.is_empty());
        assert!(!state.has_backups());
        assert_eq!(state.optimize_runs, 0);
    }

    #[test]
    fn restore_without_backups_is_a_no_op() {
        let mut state = SavedState::default();
        let report = restore(&mut state);
        assert_eq!(report.restored, 0);
        assert!(report.steps.is_empty());
    }

    #[test]
    fn verify_marks_unset_items_inactive() {
        let states = verify(None, None);
        assert_eq!(states.len(), ITEMS.len());
        assert!(states.iter().all(|state| !state.active));
    }
}
