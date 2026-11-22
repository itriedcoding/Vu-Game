//! Recommendations built from measured data.
//!
//! Volt never closes anything on its own. It ranks what is actually running and
//! explains why a program competes with Roblox, so the user decides.

use crate::model::{AppUsage, Recommendation, RobloxState, SystemSnapshot};
use std::collections::BTreeMap;

pub struct KnownApp {
    pub exe: &'static str,
    pub title: &'static str,
    pub note: &'static str,
}

/// Background programs that commonly compete with a game for resources.
pub const KNOWN_APPS: &[KnownApp] = &[
    KnownApp { exe: "discord.exe", title: "Discord", note: "Voice and chat client. Its overlay renders on top of the game and can add frame time." },
    KnownApp { exe: "spotify.exe", title: "Spotify", note: "Streaming client with constant CPU and network activity." },
    KnownApp { exe: "chrome.exe", title: "Google Chrome", note: "Browser tabs and GPU acceleration compete for memory and CPU." },
    KnownApp { exe: "msedge.exe", title: "Microsoft Edge", note: "Background tabs and web apps keep rendering work alive." },
    KnownApp { exe: "firefox.exe", title: "Mozilla Firefox", note: "Browser tabs keep CPU and GPU work alive in the background." },
    KnownApp { exe: "brave.exe", title: "Brave", note: "Chromium-based browser, same cost as other Chromium apps." },
    KnownApp { exe: "opera.exe", title: "Opera", note: "Chromium-based browser with background processes." },
    KnownApp { exe: "onedrive.exe", title: "OneDrive", note: "Cloud sync. Disk and network activity while it uploads or checks files." },
    KnownApp { exe: "dropbox.exe", title: "Dropbox", note: "Cloud sync with background indexing activity." },
    KnownApp { exe: "googledrivefs.exe", title: "Google Drive", note: "Cloud sync and file streaming activity." },
    KnownApp { exe: "steam.exe", title: "Steam", note: "Store client, overlay and background downloads." },
    KnownApp { exe: "steamwebhelper.exe", title: "Steam web helper", note: "Steam's embedded Chromium processes." },
    KnownApp { exe: "epicgameslauncher.exe", title: "Epic Games Launcher", note: "Store client with its own overlay and update checks." },
    KnownApp { exe: "battle.net.exe", title: "Battle.net", note: "Launcher with background update traffic." },
    KnownApp { exe: "riotclientservices.exe", title: "Riot Client", note: "Launcher service with background update checks." },
    KnownApp { exe: "obs64.exe", title: "OBS Studio", note: "Recording and encoding use CPU or GPU resources. Only close it if you are not recording." },
    KnownApp { exe: "nvidia overlay.exe", title: "NVIDIA overlay", note: "Instant replay keeps an encoder active while it runs." },
    KnownApp { exe: "nvidia share.exe", title: "NVIDIA Share", note: "ShadowPlay capture pipeline." },
    KnownApp { exe: "nvsphelper64.exe", title: "NVIDIA Share helper", note: "Part of the ShadowPlay capture pipeline." },
    KnownApp { exe: "ms-teams.exe", title: "Microsoft Teams", note: "Chat and calls, with web-rendered UI in the background." },
    KnownApp { exe: "teams.exe", title: "Microsoft Teams", note: "Chat and calls, with web-rendered UI in the background." },
    KnownApp { exe: "slack.exe", title: "Slack", note: "Electron chat client that keeps rendering notifications." },
    KnownApp { exe: "zoom.exe", title: "Zoom", note: "Video conferencing with camera and audio processing." },
    KnownApp { exe: "wallpaper32.exe", title: "Wallpaper Engine", note: "Animated wallpapers pause for most games; check its performance settings." },
    KnownApp { exe: "wallpaper64.exe", title: "Wallpaper Engine", note: "Animated wallpapers pause for most games; check its performance settings." },
    KnownApp { exe: "icue.exe", title: "Corsair iCUE", note: "Peripheral software with lighting effects and device polling." },
    KnownApp { exe: "razer synapse.exe", title: "Razer Synapse", note: "Peripheral software with device polling and lighting effects." },
    KnownApp { exe: "razercentralservice.exe", title: "Razer Central", note: "Peripheral software service with background telemetry." },
    KnownApp { exe: "lghub.exe", title: "Logitech G HUB", note: "Peripheral software with lighting and macro polling." },
    KnownApp { exe: "msiafterburner.exe", title: "MSI Afterburner", note: "Monitoring tool. On-screen display can cost frame time." },
    KnownApp { exe: "rtss.exe", title: "RivaTuner Statistics Server", note: "Overlay that hooks rendering for on-screen stats." },
    KnownApp { exe: "overwolf.exe", title: "Overwolf", note: "In-game overlay framework that renders on top of games." },
    KnownApp { exe: "overwolfbrowser.exe", title: "Overwolf browser", note: "Overlay web content running alongside the game." },
    KnownApp { exe: "gamebar.exe", title: "Xbox Game Bar", note: "Overlay with background widgets and capture hooks." },
    KnownApp { exe: "gamebarft.exe", title: "Xbox Game Bar", note: "Overlay with background widgets and capture hooks." },
    KnownApp { exe: "phoneexperiencehost.exe", title: "Phone Link", note: "Phone mirroring that syncs notifications and media." },
    KnownApp { exe: "yourphone.exe", title: "Phone Link", note: "Phone mirroring that syncs notifications and media." },
    KnownApp { exe: "creative cloud.exe", title: "Adobe Creative Cloud", note: "Background sync, font and update services." },
    KnownApp { exe: "adobe desktop service.exe", title: "Adobe desktop service", note: "Background asset sync and update checks." },
    KnownApp { exe: "docker desktop.exe", title: "Docker Desktop", note: "Virtual machine that reserves CPU and memory." },
    KnownApp { exe: "vmware-vmx.exe", title: "VMware virtual machine", note: "A running VM reserves cores and memory." },
    KnownApp { exe: "virtualboxvm.exe", title: "VirtualBox VM", note: "A running VM reserves cores and memory." },
    KnownApp { exe: "fraps.exe", title: "Fraps", note: "Overlay and capture tool that hooks rendering." },
];

/// Windows components and security software that must never be closed by Volt.
const PROTECTED: &[&str] = &[
    "system",
    "registry",
    "memory compression",
    "secure system",
    "smss.exe",
    "csrss.exe",
    "wininit.exe",
    "winlogon.exe",
    "services.exe",
    "lsass.exe",
    "lsaiso.exe",
    "svchost.exe",
    "dwm.exe",
    "explorer.exe",
    "fontdrvhost.exe",
    "audiodg.exe",
    "sihost.exe",
    "ctfmon.exe",
    "taskhostw.exe",
    "runtimebroker.exe",
    "searchindexer.exe",
    "searchhost.exe",
    "startmenuexperiencehost.exe",
    "shellexperiencehost.exe",
    "applicationframehost.exe",
    "textinputhost.exe",
    "lockapp.exe",
    "dllhost.exe",
    "conhost.exe",
    "spoolsv.exe",
    "trustedinstaller.exe",
    "wuauclt.exe",
    "securityhealthservice.exe",
    "securityhealthsystray.exe",
    "msmpeng.exe",
    "nissrv.exe",
    "mpdefendercoreservice.exe",
    "smartscreen.exe",
    "systemsettings.exe",
    "taskmgr.exe",
];

pub fn is_protected(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    PROTECTED.contains(&lower.as_str())
}

pub fn lookup(name: &str) -> Option<&'static KnownApp> {
    let lower = name.to_ascii_lowercase();
    KNOWN_APPS.iter().find(|app| app.exe == lower)
}

/// Rank running background programs by measured impact.
pub fn recommendations(apps: &[AppUsage]) -> Vec<Recommendation> {
    let mut grouped: BTreeMap<String, Recommendation> = BTreeMap::new();

    for app in apps {
        let lower = app.name.to_ascii_lowercase();
        if lower == "volt.exe" || crate::roblox::is_player_process(&lower) || is_protected(&lower) {
            continue;
        }

        let known = lookup(&lower);
        let heavy_unknown =
            known.is_none() && (app.memory >= 1_500_000_000 || app.cpu_percent >= 8.0);
        if known.is_none() && !heavy_unknown {
            continue;
        }
        if known.is_some() && app.memory < 150_000_000 && app.cpu_percent < 0.6 {
            continue;
        }

        let entry = grouped
            .entry(lower.clone())
            .or_insert_with(|| Recommendation {
                exe: app.name.clone(),
                title: known
                    .map(|app| app.title.to_owned())
                    .unwrap_or_else(|| friendly_name(&app.name)),
                note: known.map(|app| app.note.to_owned()).unwrap_or_else(|| {
                    "Heavy background program. Close it while playing if you do not need it."
                        .to_owned()
                }),
                memory: 0,
                cpu_percent: 0.0,
                instances: 0,
                pids: Vec::new(),
            });

        entry.memory = entry.memory.saturating_add(app.memory);
        entry.cpu_percent += app.cpu_percent;
        entry.instances += 1;
        entry.pids.push(app.pid);
    }

    let mut items: Vec<Recommendation> = grouped.into_values().collect();
    for item in &mut items {
        item.cpu_percent = item.cpu_percent.clamp(0.0, 100.0);
    }
    items.sort_by(|a, b| {
        score(b)
            .partial_cmp(&score(a))
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    items.truncate(6);
    items
}

fn score(item: &Recommendation) -> f64 {
    item.cpu_percent as f64 * 8.0 + item.memory as f64 / 400_000_000.0
}

fn friendly_name(exe: &str) -> String {
    let stem = exe
        .strip_suffix(".exe")
        .or_else(|| exe.strip_suffix(".EXE"))
        .unwrap_or(exe);
    let mut chars = stem.chars();
    match chars.next() {
        Some(first) => format!("{}{}", first.to_uppercase(), chars.as_str()),
        None => "Background app".to_owned(),
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tone {
    Good,
    Info,
    Warn,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Tip {
    pub title: String,
    pub body: String,
    pub tone: Tone,
}

/// Guidance derived from the current measurements, with no promised gains.
pub fn memory_tips(system: &SystemSnapshot, roblox: &RobloxState) -> Vec<Tip> {
    let mut tips = Vec::new();

    if system.ram_total > 0 {
        if system.ram_percent >= 85.0 {
            tips.push(Tip {
                title: "Memory is nearly full".into(),
                body: format!(
                    "{:.0}% of your RAM is in use. Closing the programs above gives Roblox more memory headroom for textures and geometry.",
                    system.ram_percent
                ),
                tone: Tone::Warn,
            });
        } else if system.ram_percent >= 70.0 {
            tips.push(Tip {
                title: "Memory pressure is building".into(),
                body: format!(
                    "{:.0}% of your RAM is in use. Windows starts paging when free memory runs out, which shows up as stutter.",
                    system.ram_percent
                ),
                tone: Tone::Info,
            });
        } else {
            tips.push(Tip {
                title: "Memory headroom is healthy".into(),
                body: format!(
                    "{:.0}% of your RAM is in use, leaving {} free.",
                    system.ram_percent,
                    crate::format::bytes(system.ram_total.saturating_sub(system.ram_used))
                ),
                tone: Tone::Good,
            });
        }
    }

    if roblox.running {
        if roblox.memory >= 3_500_000_000 {
            tips.push(Tip {
                title: "Roblox is holding a lot of memory".into(),
                body: format!(
                    "Roblox is using {}. Lowering graphics quality or render distance in-game usually reduces this, and a client restart clears what a long session accumulated.",
                    crate::format::bytes(roblox.memory)
                ),
                tone: Tone::Warn,
            });
        } else if roblox.memory > 0 {
            tips.push(Tip {
                title: "Roblox memory looks normal".into(),
                body: format!(
                    "Roblox is using {} in its main process.",
                    crate::format::bytes(roblox.memory)
                ),
                tone: Tone::Good,
            });
        }

        if let Some(uptime) = roblox.uptime_secs {
            if uptime > 7_200 {
                tips.push(Tip {
                    title: "Long session".into(),
                    body: format!(
                        "This client has been open for {}. Restarting Roblox after long sessions clears accumulated asset memory.",
                        crate::format::duration(uptime)
                    ),
                    tone: Tone::Info,
                });
            }
        }
    } else {
        tips.push(Tip {
            title: "Roblox is not running".into(),
            body: "Launch Roblox Player and Volt will measure its memory, CPU share and process priority live.".into(),
            tone: Tone::Info,
        });
    }

    tips
}

/// Network guidance.
///
/// Volt deliberately changes no network settings and measures no latency, so these
/// are recommendations only — no registry tweaks and no promised ping numbers.
pub fn network_tips() -> Vec<Tip> {
    vec![
        Tip {
            title: "Volt changes no network settings".into(),
            body: "No TCP registry tweaks, no DNS swapping, no adapter resets. Those edits are system-wide and rarely change how your ping feels.".into(),
            tone: Tone::Info,
        },
        Tip {
            title: "Prefer a wired connection".into(),
            body: "Ethernet avoids Wi-Fi retries and interference, which is what usually causes latency spikes rather than a high average ping.".into(),
            tone: Tone::Info,
        },
        Tip {
            title: "Pause downloads while playing".into(),
            body: "Launchers, cloud sync and stream uploads share your link with Roblox and show up as lag spikes.".into(),
            tone: Tone::Info,
        },
        Tip {
            title: "If you use a VPN, pick the closest server".into(),
            body: "A VPN adds a hop. Compare your in-game ping with and without it — Volt cannot measure that for you.".into(),
            tone: Tone::Info,
        },
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn app(name: &str, pid: u32, memory: u64, cpu: f32) -> AppUsage {
        AppUsage {
            pid,
            name: name.to_owned(),
            memory,
            cpu_percent: cpu,
        }
    }

    #[test]
    fn protects_windows_components() {
        assert!(is_protected("explorer.exe"));
        assert!(is_protected("MsMpEng.exe"));
        assert!(is_protected("System"));
        assert!(!is_protected("discord.exe"));
    }

    #[test]
    fn looks_up_known_apps_case_insensitively() {
        assert_eq!(lookup("DISCORD.EXE").map(|app| app.title), Some("Discord"));
        assert!(lookup("unknown.exe").is_none());
    }

    #[test]
    fn groups_and_ranks_background_apps() {
        let apps = vec![
            app("chrome.exe", 10, 400_000_000, 1.5),
            app("chrome.exe", 11, 400_000_000, 1.5),
            app("discord.exe", 12, 300_000_000, 0.4),
            app("explorer.exe", 13, 200_000_000, 0.2),
            app("RobloxPlayerBeta.exe", 14, 1_000_000_000, 20.0),
            app("volt.exe", 15, 50_000_000, 0.3),
        ];

        let items = recommendations(&apps);
        assert_eq!(items.len(), 2);
        assert_eq!(items[0].exe, "chrome.exe");
        assert_eq!(items[0].instances, 2);
        assert_eq!(items[0].memory, 800_000_000);
        assert_eq!(items[0].pids, vec![10, 11]);
        assert_eq!(items[1].exe, "discord.exe");
        assert!(items
            .iter()
            .all(|item| item.exe != "explorer.exe" && item.exe != "volt.exe"));
    }

    #[test]
    fn flags_unknown_heavy_processes() {
        let apps = vec![app("mystery.exe", 20, 2_000_000_000, 1.0)];
        let items = recommendations(&apps);
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].title, "Mystery");
        assert!(items[0].note.contains("Heavy background program"));
    }

    #[test]
    fn network_tips_make_no_ping_claims() {
        let tips = network_tips();
        assert!(tips.len() >= 3);
        for tip in &tips {
            assert_eq!(tip.tone, Tone::Info);
            assert!(!tip.title.to_lowercase().contains("reduce ping"));
        }
    }

    #[test]
    fn tips_follow_measured_memory() {
        let system = SystemSnapshot {
            cpu_percent: 20.0,
            ram_used: 15_000,
            ram_total: 16_000,
            ram_percent: 93.8,
            processes: 250,
        };
        let roblox = RobloxState::default();
        let tips = memory_tips(&system, &roblox);
        assert!(tips.iter().any(|tip| tip.tone == Tone::Warn));
        assert!(tips.iter().any(|tip| tip.title.contains("not running")));
    }
}
