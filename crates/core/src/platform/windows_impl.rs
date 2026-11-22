//! Win32 implementation of the platform layer.
//!
//! Only read-only inspection and the small, reversible tweaks Volt advertises are
//! implemented here. No code is injected into any process and no Roblox file is
//! modified.

use super::{CpuTimes, MemoryInfo, PlatformError, ProcessEntry, Result, SystemInfo};
use crate::model::Priority;
use std::ffi::c_void;
use std::path::{Path, PathBuf};
use windows::core::{GUID, PCWSTR, PWSTR};
use windows::Win32::Foundation::{
    CloseHandle, LocalFree, ERROR_FILE_NOT_FOUND, ERROR_SUCCESS, FILETIME, HANDLE, HLOCAL,
};
use windows::Win32::System::Diagnostics::ToolHelp::{
    CreateToolhelp32Snapshot, Process32FirstW, Process32NextW, PROCESSENTRY32W, TH32CS_SNAPPROCESS,
};
use windows::Win32::System::Power::{
    PowerGetActiveScheme, PowerReadFriendlyName, PowerSetActiveScheme,
};
use windows::Win32::System::ProcessStatus::{K32GetProcessMemoryInfo, PROCESS_MEMORY_COUNTERS};
use windows::Win32::System::Registry::{
    RegDeleteKeyValueW, RegGetValueW, RegSetKeyValueW, HKEY, HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE,
    REG_DWORD, RRF_RT_REG_DWORD, RRF_RT_REG_SZ,
};
use windows::Win32::System::SystemInformation::{GlobalMemoryStatusEx, MEMORYSTATUSEX};
use windows::Win32::System::Threading::{
    GetPriorityClass, GetProcessTimes, GetSystemTimes, OpenProcess, QueryFullProcessImageNameW,
    SetPriorityClass, TerminateProcess, ABOVE_NORMAL_PRIORITY_CLASS, BELOW_NORMAL_PRIORITY_CLASS,
    HIGH_PRIORITY_CLASS, IDLE_PRIORITY_CLASS, NORMAL_PRIORITY_CLASS, PROCESS_ACCESS_RIGHTS,
    PROCESS_CREATION_FLAGS, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION,
    PROCESS_SET_INFORMATION, PROCESS_TERMINATE, PROCESS_VM_READ, REALTIME_PRIORITY_CLASS,
};
use windows::Win32::UI::Shell::{IsUserAnAdmin, ShellExecuteW};
use windows::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;

/// Windows FILETIME epoch (1601-01-01) to Unix epoch difference, in seconds.
const FILETIME_UNIX_OFFSET: u64 = 11_644_473_600;

pub fn is_supported() -> bool {
    true
}

pub fn is_elevated() -> bool {
    unsafe { IsUserAnAdmin().as_bool() }
}

pub fn local_app_data() -> Option<PathBuf> {
    std::env::var_os("LOCALAPPDATA").map(PathBuf::from)
}

pub fn app_data() -> Option<PathBuf> {
    std::env::var_os("APPDATA").map(PathBuf::from)
}

pub fn temp_dir() -> Option<PathBuf> {
    std::env::var_os("TEMP").map(PathBuf::from)
}

pub fn cpu_times() -> Option<CpuTimes> {
    let mut idle = FILETIME::default();
    let mut kernel = FILETIME::default();
    let mut user = FILETIME::default();
    unsafe {
        GetSystemTimes(
            Some(&mut idle as *mut FILETIME),
            Some(&mut kernel as *mut FILETIME),
            Some(&mut user as *mut FILETIME),
        )
    }
    .ok()?;
    Some(CpuTimes {
        idle: filetime_to_u64(idle),
        kernel: filetime_to_u64(kernel),
        user: filetime_to_u64(user),
    })
}

pub fn memory() -> Option<MemoryInfo> {
    let mut status = MEMORYSTATUSEX {
        dwLength: std::mem::size_of::<MEMORYSTATUSEX>() as u32,
        ..Default::default()
    };
    unsafe { GlobalMemoryStatusEx(&mut status) }.ok()?;
    Some(MemoryInfo {
        total: status.ullTotalPhys,
        available: status.ullAvailPhys,
    })
}

pub fn list_processes() -> Result<Vec<ProcessEntry>> {
    let snapshot = unsafe { CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) }.map_err(|err| {
        PlatformError(format!("Could not read the process list: {}", err.code().0))
    })?;
    let snapshot = OwnedHandle(snapshot);

    let mut entry = PROCESSENTRY32W {
        dwSize: std::mem::size_of::<PROCESSENTRY32W>() as u32,
        ..Default::default()
    };
    let mut processes = Vec::with_capacity(256);

    if unsafe { Process32FirstW(snapshot.0, &mut entry) }.is_err() {
        return Ok(processes);
    }

    loop {
        let pid = entry.th32ProcessID;
        if pid != 0 {
            let (memory, cpu_time, created_unix) = usage(pid).unwrap_or((0, 0, None));
            processes.push(ProcessEntry {
                pid,
                name: string_from_wide(&entry.szExeFile),
                threads: entry.cntThreads,
                memory,
                cpu_time,
                created_unix,
            });
        }
        if unsafe { Process32NextW(snapshot.0, &mut entry) }.is_err() {
            break;
        }
    }

    Ok(processes)
}

pub fn process_path(pid: u32) -> Option<PathBuf> {
    let handle = open_process(pid, PROCESS_QUERY_LIMITED_INFORMATION).ok()?;
    let mut buffer = [0u16; 1024];
    let mut length = buffer.len() as u32;
    unsafe {
        QueryFullProcessImageNameW(
            handle.0,
            PROCESS_NAME_WIN32,
            PWSTR(buffer.as_mut_ptr()),
            &mut length,
        )
    }
    .ok()?;
    let text = String::from_utf16_lossy(&buffer[..length as usize]);
    Some(PathBuf::from(text))
}

pub fn priority(pid: u32) -> Option<Priority> {
    let handle = open_process(pid, PROCESS_QUERY_LIMITED_INFORMATION).ok()?;
    let class = unsafe { GetPriorityClass(handle.0) };
    Some(match_priority(class))
}

pub fn set_priority(pid: u32, priority: Priority) -> Result<()> {
    let handle = open_process(
        pid,
        PROCESS_SET_INFORMATION | PROCESS_QUERY_LIMITED_INFORMATION,
    )?;
    let class = priority_class(priority);
    unsafe { SetPriorityClass(handle.0, class) }.map_err(|err| {
        PlatformError(format!(
            "Windows refused the priority change (error {}). Roblox may be running as administrator.",
            err.code().0
        ))
    })
}

pub fn terminate(pid: u32) -> Result<()> {
    if pid <= 4 {
        return Err(PlatformError(
            "System processes cannot be closed from Volt.".into(),
        ));
    }
    let handle = open_process(pid, PROCESS_TERMINATE)?;
    unsafe { TerminateProcess(handle.0, 1) }.map_err(|err| {
        PlatformError(format!(
            "Windows refused to close the process (error {}).",
            err.code().0
        ))
    })
}

pub fn open_folder(path: &Path) -> Result<()> {
    std::process::Command::new("explorer.exe")
        .arg(path)
        .spawn()
        .map(|_| ())
        .map_err(|err| PlatformError(format!("Could not open the folder: {err}")))
}

pub fn reveal_path(path: &Path) -> Result<()> {
    if path.is_dir() {
        open_folder(path)
    } else {
        std::process::Command::new("explorer.exe")
            .arg(format!("/select,{}", path.display()))
            .spawn()
            .map(|_| ())
            .map_err(|err| PlatformError(format!("Could not open the item: {err}")))
    }
}

pub fn registry_dword(path: &str, name: &str) -> Option<u32> {
    let path = wide(path);
    let name = wide(name);
    let mut value: u32 = 0;
    let mut size: u32 = std::mem::size_of::<u32>() as u32;
    let status = unsafe {
        RegGetValueW(
            HKEY_CURRENT_USER,
            PCWSTR(path.as_ptr()),
            PCWSTR(name.as_ptr()),
            RRF_RT_REG_DWORD,
            None,
            Some(&mut value as *mut u32 as *mut c_void),
            Some(&mut size),
        )
    };
    if status == ERROR_SUCCESS {
        Some(value)
    } else {
        None
    }
}

pub fn set_registry_dword(path: &str, name: &str, value: u32) -> Result<()> {
    let path_w = wide(path);
    let name_w = wide(name);
    let status = unsafe {
        RegSetKeyValueW(
            HKEY_CURRENT_USER,
            PCWSTR(path_w.as_ptr()),
            PCWSTR(name_w.as_ptr()),
            REG_DWORD.0,
            Some(&value as *const u32 as *const c_void),
            std::mem::size_of::<u32>() as u32,
        )
    };
    if status == ERROR_SUCCESS {
        Ok(())
    } else {
        Err(PlatformError(format!(
            "Could not write HKEY_CURRENT_USER\\{path} (error {}).",
            status.0
        )))
    }
}

pub fn delete_registry_value(path: &str, name: &str) -> Result<()> {
    let path_w = wide(path);
    let name_w = wide(name);
    let status = unsafe {
        RegDeleteKeyValueW(
            HKEY_CURRENT_USER,
            PCWSTR(path_w.as_ptr()),
            PCWSTR(name_w.as_ptr()),
        )
    };
    if status == ERROR_SUCCESS || status == ERROR_FILE_NOT_FOUND {
        Ok(())
    } else {
        Err(PlatformError(format!(
            "Could not remove HKEY_CURRENT_USER\\{path} (error {}).",
            status.0
        )))
    }
}

pub fn active_power_scheme() -> Option<String> {
    active_power_guid().map(|scheme| guid_to_string(&scheme))
}

/// Friendly name of the active power plan, read from `powrprof`.
pub fn active_power_plan_name() -> Option<String> {
    let scheme = active_power_guid()?;

    let mut size: u32 = 0;
    let _ = unsafe { PowerReadFriendlyName(None, Some(&scheme), None, None, None, &mut size) };
    if size == 0 {
        return None;
    }

    let mut buffer = vec![0u8; size as usize];
    let status = unsafe {
        PowerReadFriendlyName(
            None,
            Some(&scheme),
            None,
            None,
            Some(buffer.as_mut_ptr()),
            &mut size,
        )
    };
    if status != ERROR_SUCCESS {
        return None;
    }

    let units: Vec<u16> = buffer
        .chunks_exact(2)
        .map(|chunk| u16::from_le_bytes([chunk[0], chunk[1]]))
        .collect();
    let name = string_from_wide(&units).trim().to_owned();
    if name.is_empty() {
        None
    } else {
        Some(name)
    }
}

/// Machine facts for the System page and the diagnostics report.
pub fn system_info() -> SystemInfo {
    let cpu = machine_string(CPU_KEY, "ProcessorNameString")
        .map(|value| value.trim().to_owned())
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| "Not reported".to_owned());
    let (gpu, gpu_driver) = graphics_adapter();

    SystemInfo {
        cpu,
        gpu,
        gpu_driver,
        os: windows_label(),
        logical_cores: std::thread::available_parallelism()
            .map(|value| value.get() as u32)
            .unwrap_or(0),
        power_plan: active_power_plan_name().unwrap_or_else(|| "Not reported".to_owned()),
    }
}

/// Ask Windows to start a new, elevated copy of Volt.
pub fn relaunch_elevated() -> Result<()> {
    if is_elevated() {
        return Ok(());
    }

    let exe = std::env::current_exe()
        .map_err(|err| PlatformError(format!("Could not find Volt's own executable: {err}")))?;
    let file = wide(&exe.display().to_string());
    let verb = wide("runas");

    let result = unsafe {
        ShellExecuteW(
            None,
            PCWSTR(verb.as_ptr()),
            PCWSTR(file.as_ptr()),
            PCWSTR::null(),
            PCWSTR::null(),
            SW_SHOWNORMAL,
        )
    };
    if result.0 as isize <= 32 {
        return Err(PlatformError(
            "Windows declined the elevation request. Approve the User Account Control prompt, or sign in with an administrator account."
                .into(),
        ));
    }
    Ok(())
}

pub fn set_active_power_scheme(guid: &str) -> Result<()> {
    let scheme = parse_guid(guid)
        .ok_or_else(|| PlatformError(format!("Unrecognised power plan identifier: {guid}")))?;
    let status = unsafe { PowerSetActiveScheme(None, Some(&scheme)) };
    if status == ERROR_SUCCESS {
        Ok(())
    } else {
        Err(PlatformError(format!(
            "Windows rejected the power plan change (error {}). Pick the plan manually in Settings → System → Power.",
            status.0
        )))
    }
}

// ---------------------------------------------------------------------------
// machine information
// ---------------------------------------------------------------------------

const CPU_KEY: &str = r"HARDWARE\DESCRIPTION\System\CentralProcessor\0";
const GPU_CLASS_KEY: &str =
    r"SYSTEM\CurrentControlSet\Control\Class\{4d36e968-e325-11ce-bfc1-08002be10318}";
const WINDOWS_KEY: &str = r"SOFTWARE\Microsoft\Windows NT\CurrentVersion";

fn active_power_guid() -> Option<GUID> {
    let mut raw: *mut GUID = std::ptr::null_mut();
    let status = unsafe { PowerGetActiveScheme(None, &mut raw) };
    if status != ERROR_SUCCESS || raw.is_null() {
        return None;
    }
    let scheme = unsafe { *raw };
    unsafe {
        let _ = LocalFree(Some(HLOCAL(raw as *mut c_void)));
    }
    Some(scheme)
}

/// The display adapter Windows reports for this machine. Laptops often list the
/// integrated GPU first, so any non-basic adapter is preferred over the fallback.
fn graphics_adapter() -> (String, String) {
    let mut basic: Option<(String, String)> = None;

    for index in 0..4 {
        let path = format!("{GPU_CLASS_KEY}\\{index:04}");
        let Some(description) = machine_string(&path, "DriverDesc") else {
            continue;
        };
        let description = description.trim().to_owned();
        if description.is_empty() {
            continue;
        }
        let driver = machine_string(&path, "DriverVersion")
            .unwrap_or_default()
            .trim()
            .to_owned();
        let candidate = (description, driver);

        if candidate.0.starts_with("Microsoft Basic") {
            basic.get_or_insert(candidate);
            continue;
        }
        return candidate;
    }

    basic.unwrap_or_else(|| ("Not reported".to_owned(), String::new()))
}

/// Product name, release and build. Windows 11 still reports `Windows 10` as the
/// product name, so the build number decides which one to show.
fn windows_label() -> String {
    let product = machine_string(WINDOWS_KEY, "ProductName").unwrap_or_default();
    let release = machine_string(WINDOWS_KEY, "DisplayVersion").unwrap_or_default();
    let build = machine_string(WINDOWS_KEY, "CurrentBuildNumber").unwrap_or_default();
    let revision = machine_dword(WINDOWS_KEY, "UBR");

    let build_number = build.trim().parse::<u32>().unwrap_or(0);
    let product = product.trim();
    let mut label = if build_number >= 22_000 && product.starts_with("Windows 10") {
        product.replacen("Windows 10", "Windows 11", 1)
    } else if product.is_empty() {
        "Windows".to_owned()
    } else {
        product.to_owned()
    };

    let release = release.trim();
    if !release.is_empty() {
        label.push_str(" \u{b7} ");
        label.push_str(release);
    }

    let build = build.trim();
    if !build.is_empty() {
        label.push_str(" \u{b7} build ");
        label.push_str(build);
        if let Some(revision) = revision {
            label.push('.');
            label.push_str(&revision.to_string());
        }
    }

    label
}

fn machine_string(path: &str, name: &str) -> Option<String> {
    registry_string(HKEY_LOCAL_MACHINE, path, name)
}

fn machine_dword(path: &str, name: &str) -> Option<u32> {
    let path = wide(path);
    let name = wide(name);
    let mut value: u32 = 0;
    let mut size: u32 = std::mem::size_of::<u32>() as u32;
    let status = unsafe {
        RegGetValueW(
            HKEY_LOCAL_MACHINE,
            PCWSTR(path.as_ptr()),
            PCWSTR(name.as_ptr()),
            RRF_RT_REG_DWORD,
            None,
            Some(&mut value as *mut u32 as *mut c_void),
            Some(&mut size),
        )
    };
    if status == ERROR_SUCCESS {
        Some(value)
    } else {
        None
    }
}

fn registry_string(root: HKEY, path: &str, name: &str) -> Option<String> {
    let path = wide(path);
    let name = wide(name);

    let mut size: u32 = 0;
    let status = unsafe {
        RegGetValueW(
            root,
            PCWSTR(path.as_ptr()),
            PCWSTR(name.as_ptr()),
            RRF_RT_REG_SZ,
            None,
            None,
            Some(&mut size),
        )
    };
    if status != ERROR_SUCCESS || size == 0 {
        return None;
    }

    let mut buffer = vec![0u16; (size as usize).div_ceil(2) + 1];
    let status = unsafe {
        RegGetValueW(
            root,
            PCWSTR(path.as_ptr()),
            PCWSTR(name.as_ptr()),
            RRF_RT_REG_SZ,
            None,
            Some(buffer.as_mut_ptr() as *mut c_void),
            Some(&mut size),
        )
    };
    if status != ERROR_SUCCESS {
        return None;
    }

    Some(string_from_wide(&buffer))
}

// ---------------------------------------------------------------------------
// helpers
// ---------------------------------------------------------------------------

struct OwnedHandle(HANDLE);

impl Drop for OwnedHandle {
    fn drop(&mut self) {
        if !self.0.is_invalid() {
            unsafe {
                let _ = CloseHandle(self.0);
            }
        }
    }
}

fn open_process(pid: u32, access: PROCESS_ACCESS_RIGHTS) -> Result<OwnedHandle> {
    unsafe { OpenProcess(access, false, pid) }
        .map(OwnedHandle)
        .map_err(|err| {
            PlatformError(format!(
                "Could not open process {pid} (error {}).",
                err.code().0
            ))
        })
}

/// Working set, cumulative CPU time and creation time for one process.
fn usage(pid: u32) -> Option<(u64, u64, Option<u64>)> {
    let handle = open_process(pid, PROCESS_QUERY_LIMITED_INFORMATION | PROCESS_VM_READ).ok()?;

    let mut counters = PROCESS_MEMORY_COUNTERS::default();
    let memory = if unsafe {
        K32GetProcessMemoryInfo(
            handle.0,
            &mut counters,
            std::mem::size_of::<PROCESS_MEMORY_COUNTERS>() as u32,
        )
    }
    .as_bool()
    {
        counters.WorkingSetSize as u64
    } else {
        0
    };

    let mut created = FILETIME::default();
    let mut exit = FILETIME::default();
    let mut kernel = FILETIME::default();
    let mut user = FILETIME::default();
    let mut cpu_time = 0u64;
    let mut created_unix = None;

    if unsafe { GetProcessTimes(handle.0, &mut created, &mut exit, &mut kernel, &mut user) }.is_ok()
    {
        cpu_time = filetime_to_u64(kernel).saturating_add(filetime_to_u64(user));
        let seconds = filetime_to_u64(created) / 10_000_000;
        if seconds > FILETIME_UNIX_OFFSET {
            created_unix = Some(seconds - FILETIME_UNIX_OFFSET);
        }
    }

    Some((memory, cpu_time, created_unix))
}

fn priority_class(priority: Priority) -> PROCESS_CREATION_FLAGS {
    match priority {
        Priority::Idle => IDLE_PRIORITY_CLASS,
        Priority::BelowNormal => BELOW_NORMAL_PRIORITY_CLASS,
        Priority::Normal => NORMAL_PRIORITY_CLASS,
        Priority::AboveNormal => ABOVE_NORMAL_PRIORITY_CLASS,
        Priority::High => HIGH_PRIORITY_CLASS,
        Priority::Realtime => REALTIME_PRIORITY_CLASS,
    }
}

fn match_priority(class: u32) -> Priority {
    if class == IDLE_PRIORITY_CLASS.0 {
        Priority::Idle
    } else if class == BELOW_NORMAL_PRIORITY_CLASS.0 {
        Priority::BelowNormal
    } else if class == ABOVE_NORMAL_PRIORITY_CLASS.0 {
        Priority::AboveNormal
    } else if class == HIGH_PRIORITY_CLASS.0 {
        Priority::High
    } else if class == REALTIME_PRIORITY_CLASS.0 {
        Priority::Realtime
    } else {
        Priority::Normal
    }
}

fn wide(value: &str) -> Vec<u16> {
    value.encode_utf16().chain(std::iter::once(0)).collect()
}

fn string_from_wide(buffer: &[u16]) -> String {
    let end = buffer
        .iter()
        .position(|&unit| unit == 0)
        .unwrap_or(buffer.len());
    String::from_utf16_lossy(&buffer[..end])
}

fn filetime_to_u64(time: FILETIME) -> u64 {
    ((time.dwHighDateTime as u64) << 32) | time.dwLowDateTime as u64
}

fn parse_guid(text: &str) -> Option<GUID> {
    let cleaned = text.trim().trim_matches(|c| c == '{' || c == '}');
    let parts: Vec<&str> = cleaned.split('-').collect();
    if parts.len() != 5 {
        return None;
    }
    let data1 = u32::from_str_radix(parts[0], 16).ok()?;
    let data2 = u16::from_str_radix(parts[1], 16).ok()?;
    let data3 = u16::from_str_radix(parts[2], 16).ok()?;
    let tail = format!("{}{}", parts[3], parts[4]);
    if tail.len() != 16 {
        return None;
    }
    let mut data4 = [0u8; 8];
    for (index, byte) in data4.iter_mut().enumerate() {
        *byte = u8::from_str_radix(&tail[index * 2..index * 2 + 2], 16).ok()?;
    }
    Some(GUID {
        data1,
        data2,
        data3,
        data4,
    })
}

fn guid_to_string(guid: &GUID) -> String {
    let [a, b, c, d, e, f, g, h] = guid.data4;
    format!(
        "{:08x}-{:04x}-{:04x}-{:02x}{:02x}-{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}",
        guid.data1, guid.data2, guid.data3, a, b, c, d, e, f, g, h
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn guid_round_trips() {
        let text = "8c5e7fda-e8bf-4a96-9a85-a6e23a8c635c";
        let guid = parse_guid(text).expect("guid parses");
        assert_eq!(guid_to_string(&guid), text);
    }

    #[test]
    fn rejects_bad_guids() {
        assert!(parse_guid("not-a-guid").is_none());
        assert!(parse_guid("8c5e7fda-e8bf-4a96-9a85").is_none());
    }

    #[test]
    fn reads_wide_strings() {
        let mut buffer = [0u16; 8];
        for (slot, unit) in buffer.iter_mut().zip("abc".encode_utf16()) {
            *slot = unit;
        }
        assert_eq!(string_from_wide(&buffer), "abc");
    }
}
