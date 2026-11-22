//! Shared data types used by the engine and the interface.

use crate::platform::SystemInfo;
use std::path::PathBuf;
use std::sync::Arc;

/// Windows process priority classes Volt understands.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Default)]
pub enum Priority {
    Idle,
    BelowNormal,
    #[default]
    Normal,
    AboveNormal,
    High,
    Realtime,
}

impl Priority {
    pub fn label(self) -> &'static str {
        match self {
            Priority::Idle => "Idle",
            Priority::BelowNormal => "Below Normal",
            Priority::Normal => "Normal",
            Priority::AboveNormal => "Above Normal",
            Priority::High => "High",
            Priority::Realtime => "Realtime",
        }
    }
}

/// One live sample of overall system load.
#[derive(Debug, Clone, Default)]
pub struct SystemSnapshot {
    /// Total CPU utilisation, 0-100.
    pub cpu_percent: f32,
    pub ram_used: u64,
    pub ram_total: u64,
    /// Physical memory in use, 0-100.
    pub ram_percent: f32,
    pub processes: u32,
}

/// What Volt knows about Roblox Player right now.
#[derive(Debug, Clone, Default)]
pub struct RobloxState {
    pub running: bool,
    pub pid: Option<u32>,
    /// Number of Roblox Player processes (clients plus launchers).
    pub instances: usize,
    pub memory: u64,
    /// Share of total CPU used by Roblox, 0-100.
    pub cpu_percent: f32,
    pub priority: Option<Priority>,
    pub threads: u32,
    pub uptime_secs: Option<u64>,
    pub exe: Option<PathBuf>,
}

/// A background process considered for the "background load" list.
#[derive(Debug, Clone, Default)]
pub struct AppUsage {
    pub pid: u32,
    pub name: String,
    pub memory: u64,
    pub cpu_percent: f32,
}

/// A background program Volt ranks because it competes with Roblox.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Recommendation {
    pub exe: String,
    pub title: String,
    pub note: String,
    pub memory: u64,
    pub cpu_percent: f32,
    /// How many processes share this executable.
    pub instances: u32,
    pub pids: Vec<u32>,
}

/// State of the frame-rate probe.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FpsStatus {
    /// Roblox Player is not running.
    NotRunning,
    /// Roblox is running but has not written a session log yet.
    Waiting,
    /// Roblox is running and writing logs, but exposes no frame-rate value.
    Unavailable,
    /// A fresh frame-rate value was read from Roblox's own log.
    Measured,
}

/// A frame-rate value reported by the Roblox client itself.
#[derive(Debug, Clone, PartialEq)]
pub struct MeasuredFps {
    pub fps: f32,
    pub age_secs: u64,
    /// The log line the value came from, trimmed for display.
    pub detail: String,
}

#[derive(Debug, Clone)]
pub struct FpsState {
    pub status: FpsStatus,
    pub measured: Option<MeasuredFps>,
    pub detail: String,
}

impl Default for FpsState {
    fn default() -> Self {
        Self {
            status: FpsStatus::NotRunning,
            measured: None,
            detail: "Roblox Player is not running.".to_owned(),
        }
    }
}

/// A Roblox Player installation found on disk.
#[derive(Debug, Clone, PartialEq)]
pub struct RobloxInstall {
    pub dir: PathBuf,
    pub exe: PathBuf,
    /// Short label for the version folder, e.g. `build 6f2a91cd`.
    pub build: String,
}

/// Live on/off state of one optimisation, verified against Windows.
#[derive(Debug, Clone)]
pub struct ItemState {
    pub id: &'static str,
    pub active: bool,
}

/// The complete sample handed to the interface roughly once per second.
#[derive(Debug, Clone, Default)]
pub struct Snapshot {
    pub seq: u64,
    /// Seconds since Volt started sampling.
    pub uptime_secs: u64,
    /// False on builds without a Windows backend.
    pub platform_ready: bool,
    pub elevated: bool,
    /// Machine facts (CPU, GPU, Windows build) and the active power plan.
    pub info: Arc<SystemInfo>,
    pub system: SystemSnapshot,
    pub roblox: RobloxState,
    pub apps: Vec<Recommendation>,
    pub fps: FpsState,
    pub installs: Vec<RobloxInstall>,
    pub items: Vec<ItemState>,
    /// CPU history, oldest first, one value per second.
    pub cpu_history: Vec<f32>,
    /// Memory history, oldest first, one value per second.
    pub ram_history: Vec<f32>,
}

impl Snapshot {
    /// Number of optimisations that are currently active.
    pub fn active_items(&self) -> usize {
        self.items.iter().filter(|item| item.active).count()
    }
}
