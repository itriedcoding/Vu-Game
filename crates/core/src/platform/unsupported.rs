//! Stub used when the engine is compiled for a non-Windows target.
//!
//! This exists so the optimisation logic, parsers and state handling can be unit
//! tested on any platform. Volt is only distributed for Windows.

use super::{CpuTimes, MemoryInfo, PlatformError, ProcessEntry, Result};
use crate::model::Priority;
use std::path::{Path, PathBuf};

pub fn is_supported() -> bool {
    false
}

pub fn is_elevated() -> bool {
    false
}

pub fn local_app_data() -> Option<PathBuf> {
    None
}

pub fn app_data() -> Option<PathBuf> {
    None
}

pub fn temp_dir() -> Option<PathBuf> {
    None
}

pub fn cpu_times() -> Option<CpuTimes> {
    None
}

pub fn memory() -> Option<MemoryInfo> {
    None
}

pub fn list_processes() -> Result<Vec<ProcessEntry>> {
    Ok(Vec::new())
}

pub fn process_path(_pid: u32) -> Option<PathBuf> {
    None
}

pub fn priority(_pid: u32) -> Option<Priority> {
    None
}

pub fn set_priority(_pid: u32, _priority: Priority) -> Result<()> {
    Err(PlatformError(
        "Priority changes need the Windows build of Volt.".into(),
    ))
}

pub fn terminate(_pid: u32) -> Result<()> {
    Err(PlatformError(
        "Closing processes needs the Windows build of Volt.".into(),
    ))
}

pub fn open_folder(_path: &Path) -> Result<()> {
    Err(PlatformError(
        "Opening folders needs the Windows build.".into(),
    ))
}

pub fn reveal_path(_path: &Path) -> Result<()> {
    Err(PlatformError(
        "Opening folders needs the Windows build.".into(),
    ))
}

pub fn registry_dword(_path: &str, _name: &str) -> Option<u32> {
    None
}

pub fn set_registry_dword(_path: &str, _name: &str, _value: u32) -> Result<()> {
    Err(PlatformError(
        "Registry changes need the Windows build of Volt.".into(),
    ))
}

pub fn delete_registry_value(_path: &str, _name: &str) -> Result<()> {
    Err(PlatformError(
        "Registry changes need the Windows build of Volt.".into(),
    ))
}

pub fn active_power_scheme() -> Option<String> {
    None
}

pub fn set_active_power_scheme(_guid: &str) -> Result<()> {
    Err(PlatformError(
        "Power plan changes need the Windows build of Volt.".into(),
    ))
}
