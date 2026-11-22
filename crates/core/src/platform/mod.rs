//! Thin platform layer.
//!
//! The Windows implementation talks to `kernel32`, `advapi32` and `powrprof`
//! directly through the `windows` crate. Other targets compile against a stub so
//! the optimisation logic can be tested anywhere; Volt itself only ships for
//! Windows.

#[cfg(windows)]
mod windows_impl;
#[cfg(windows)]
use windows_impl as imp;

#[cfg(not(windows))]
mod unsupported;
#[cfg(not(windows))]
use unsupported as imp;

/// A Windows call failed. The message is safe to show to the user.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlatformError(pub String);

impl std::fmt::Display for PlatformError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for PlatformError {}

pub type Result<T> = std::result::Result<T, PlatformError>;

/// Helpers shared by both implementations.
pub fn error(message: impl Into<String>) -> PlatformError {
    PlatformError(message.into())
}

pub fn unsupported_error() -> PlatformError {
    PlatformError("This build has no Windows backend, so system information is unavailable.".into())
}

/// Cumulative system CPU counters (100 ns ticks).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct CpuTimes {
    pub idle: u64,
    pub kernel: u64,
    pub user: u64,
}

impl CpuTimes {
    /// Kernel time already includes idle time, so busy time is kernel + user - idle.
    pub fn busy(&self) -> u64 {
        (self.kernel + self.user).saturating_sub(self.idle)
    }

    pub fn total(&self) -> u64 {
        self.kernel + self.user
    }
}

/// Facts about this machine, read from Windows once and refreshed occasionally.
///
/// Everything here is reported verbatim; Volt never substitutes a plausible
/// value when Windows does not expose one.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SystemInfo {
    pub cpu: String,
    pub gpu: String,
    pub gpu_driver: String,
    /// Product name, release and build, e.g. `Windows 11 Pro · 24H2 · build 26100.2314`.
    pub os: String,
    pub logical_cores: u32,
    /// Friendly name of the active power plan, e.g. `High performance`.
    pub power_plan: String,
}

#[derive(Debug, Clone, Copy, Default)]
pub struct MemoryInfo {
    pub total: u64,
    pub available: u64,
}

impl MemoryInfo {
    pub fn used(&self) -> u64 {
        self.total.saturating_sub(self.available)
    }

    pub fn percent(&self) -> f32 {
        if self.total == 0 {
            0.0
        } else {
            (self.used() as f64 / self.total as f64 * 100.0) as f32
        }
    }
}

/// A process as reported by the OS.
#[derive(Debug, Clone, Default)]
pub struct ProcessEntry {
    pub pid: u32,
    pub name: String,
    pub threads: u32,
    pub memory: u64,
    /// Cumulative CPU time in 100 ns ticks.
    pub cpu_time: u64,
    /// Process start time as Unix seconds, when readable.
    pub created_unix: Option<u64>,
}

pub use imp::{
    active_power_plan_name, active_power_scheme, app_data, cpu_times, delete_registry_value,
    is_elevated, is_supported, list_processes, local_app_data, memory, open_folder, priority,
    process_path, registry_dword, relaunch_elevated, reveal_path, set_active_power_scheme,
    set_priority, set_registry_dword, system_info, temp_dir, terminate,
};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cpu_times_exclude_idle_from_busy() {
        let times = CpuTimes {
            idle: 100,
            kernel: 300,
            user: 200,
        };
        assert_eq!(times.busy(), 400);
        assert_eq!(times.total(), 500);
    }

    #[test]
    fn memory_percent_is_safe_without_total() {
        let info = MemoryInfo {
            total: 0,
            available: 0,
        };
        assert_eq!(info.percent(), 0.0);
        assert_eq!(info.used(), 0);

        let info = MemoryInfo {
            total: 16_000,
            available: 4_000,
        };
        assert_eq!(info.used(), 12_000);
        assert!((info.percent() - 75.0).abs() < f32::EPSILON);
    }

    #[test]
    fn priority_and_paths_are_exported() {
        // Smoke test that the re-exported platform surface keeps its signatures.
        let _: Option<crate::model::Priority> = priority(0);
        let _: Option<std::path::PathBuf> = local_app_data();
    }
}
