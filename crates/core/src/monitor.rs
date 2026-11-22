//! Background sampler.
//!
//! One thread wakes about once per second, reads the metrics Volt displays and
//! sends a [`Snapshot`] to the interface. The thread exits when [`Monitor`] is
//! dropped, so closing Volt leaves nothing running.

use crate::model::{
    AppUsage, ItemState, Priority, Recommendation, RobloxInstall, RobloxState, Snapshot,
    SystemSnapshot,
};
use crate::platform;
use std::collections::{HashMap, VecDeque};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::Arc;
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

const SAMPLE_INTERVAL: Duration = Duration::from_millis(1_000);
const INSTALL_RESCAN: Duration = Duration::from_secs(30);
const VERIFY_INTERVAL: Duration = Duration::from_secs(5);
const MACHINE_RESCAN: Duration = Duration::from_secs(300);
const HISTORY_LEN: usize = 90;

pub struct Monitor {
    rx: Receiver<Snapshot>,
    stop: Arc<AtomicBool>,
    wake: Arc<AtomicBool>,
    join: Option<JoinHandle<()>>,
}

impl Monitor {
    pub fn start() -> Self {
        let (tx, rx) = mpsc::channel();
        let stop = Arc::new(AtomicBool::new(false));
        let wake = Arc::new(AtomicBool::new(false));

        let thread_stop = Arc::clone(&stop);
        let thread_wake = Arc::clone(&wake);
        let join = thread::Builder::new()
            .name("volt-monitor".to_owned())
            .spawn(move || run(tx, thread_stop, thread_wake))
            .ok();

        Self {
            rx,
            stop,
            wake,
            join,
        }
    }

    /// The most recent snapshot, discarding any backlog.
    pub fn latest(&self) -> Option<Snapshot> {
        self.rx.try_iter().last()
    }

    /// Ask the sampler to take a reading immediately (used after an action).
    pub fn wake(&self) {
        self.wake.store(true, Ordering::Relaxed);
    }
}

impl Drop for Monitor {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        self.wake.store(true, Ordering::Relaxed);
        if let Some(join) = self.join.take() {
            let _ = join.join();
        }
    }
}

fn run(tx: Sender<Snapshot>, stop: Arc<AtomicBool>, wake: Arc<AtomicBool>) {
    let mut sampler = Sampler::new();

    loop {
        if stop.load(Ordering::Relaxed) {
            break;
        }

        if tx.send(sampler.tick()).is_err() {
            break;
        }

        let deadline = Instant::now() + SAMPLE_INTERVAL;
        while Instant::now() < deadline {
            if stop.load(Ordering::Relaxed) {
                return;
            }
            if wake.swap(false, Ordering::Relaxed) {
                break;
            }
            thread::sleep(Duration::from_millis(40));
        }
    }
}

struct Sampler {
    started: Instant,
    seq: u64,
    cores: f32,
    prev_system: Option<platform::CpuTimes>,
    prev_processes: HashMap<u32, u64>,
    prev_at: Option<Instant>,
    fps: crate::fps::LogProbe,
    installs: Vec<RobloxInstall>,
    installs_at: Option<Instant>,
    verify_at: Option<Instant>,
    info: Arc<platform::SystemInfo>,
    info_at: Option<Instant>,
    items: Vec<ItemState>,
    roblox: RobloxState,
    apps: Vec<Recommendation>,
    cpu_history: VecDeque<f32>,
    ram_history: VecDeque<f32>,
    platform_ready: bool,
    elevated: bool,
    local_app_data: Option<PathBuf>,
}

impl Sampler {
    fn new() -> Self {
        Self {
            started: Instant::now(),
            seq: 0,
            cores: std::thread::available_parallelism()
                .map(|value| value.get() as f32)
                .unwrap_or(1.0),
            prev_system: None,
            prev_processes: HashMap::new(),
            prev_at: None,
            fps: crate::fps::LogProbe::default(),
            installs: Vec::new(),
            installs_at: None,
            verify_at: None,
            info: Arc::new(platform::system_info()),
            info_at: None,
            items: Vec::new(),
            roblox: RobloxState::default(),
            apps: Vec::new(),
            cpu_history: VecDeque::with_capacity(HISTORY_LEN),
            ram_history: VecDeque::with_capacity(HISTORY_LEN),
            platform_ready: platform::is_supported(),
            elevated: platform::is_elevated(),
            local_app_data: platform::local_app_data(),
        }
    }

    fn tick(&mut self) -> Snapshot {
        let now = Instant::now();
        self.seq += 1;

        let mut snapshot = Snapshot {
            seq: self.seq,
            uptime_secs: now.duration_since(self.started).as_secs(),
            platform_ready: self.platform_ready,
            elevated: self.elevated,
            ..Default::default()
        };

        snapshot.system = self.system_sample(now);
        self.refresh_info(now);
        snapshot.info = Arc::clone(&self.info);
        self.sample_processes();

        snapshot.roblox = self.roblox_state();
        snapshot.apps = self.apps.clone();
        snapshot.fps = self.fps.update(
            self.logs_dir().as_deref(),
            snapshot.roblox.running,
            crate::now_unix(),
        );
        snapshot.cpu_history = self.cpu_history.iter().copied().collect();
        snapshot.ram_history = self.ram_history.iter().copied().collect();

        self.refresh_installs(now);
        snapshot.installs = self.installs.clone();

        self.refresh_items(now, snapshot.roblox.priority);
        snapshot.items = self.items.clone();

        snapshot
    }

    fn logs_dir(&self) -> Option<PathBuf> {
        self.local_app_data
            .as_ref()
            .map(|dir| crate::roblox::logs_dir(dir))
    }

    fn system_sample(&mut self, now: Instant) -> SystemSnapshot {
        let mut system = SystemSnapshot::default();

        if let Some(times) = platform::cpu_times() {
            if let Some(previous) = self.prev_system {
                let total = times.total().saturating_sub(previous.total());
                let busy = times.busy().saturating_sub(previous.busy());
                if total > 0 {
                    system.cpu_percent = (busy as f64 / total as f64 * 100.0) as f32;
                }
            }
            self.prev_system = Some(times);
        }
        system.cpu_percent = system.cpu_percent.clamp(0.0, 100.0);

        if let Some(info) = platform::memory() {
            system.ram_total = info.total;
            system.ram_used = info.used();
            system.ram_percent = info.percent();
        }

        push(&mut self.cpu_history, system.cpu_percent);
        push(&mut self.ram_history, system.ram_percent);

        let _ = now;
        system
    }

    /// Read the process list once and derive both Roblox state and app usage.
    fn sample_processes(&mut self) {
        let processes = platform::list_processes().unwrap_or_default();
        let now = Instant::now();
        let window = self
            .prev_at
            .map(|at| now.duration_since(at).as_secs_f64() * 10_000_000.0)
            .filter(|value| *value > 0.0);
        let cores = self.cores.max(1.0);

        let mut usage = Vec::with_capacity(128);
        let mut next_cpu = HashMap::with_capacity(processes.len());
        let mut roblox = RobloxState::default();

        for process in &processes {
            let percent = match (window, self.prev_processes.get(&process.pid)) {
                (Some(window), Some(previous)) if process.cpu_time >= *previous => {
                    ((process.cpu_time - previous) as f64 / window * 100.0 / cores as f64) as f32
                }
                _ => 0.0,
            };
            next_cpu.insert(process.pid, process.cpu_time);

            if process.memory > 64_000_000 || percent > 0.5 {
                usage.push(AppUsage {
                    pid: process.pid,
                    name: process.name.clone(),
                    memory: process.memory,
                    cpu_percent: percent,
                });
            }

            if crate::roblox::is_player_process(&process.name) {
                roblox.instances += 1;
                if roblox.pid.is_none() || process.memory > roblox.memory {
                    roblox.pid = Some(process.pid);
                    roblox.memory = process.memory;
                    roblox.threads = process.threads;
                    roblox.cpu_percent = percent;
                    roblox.uptime_secs = process
                        .created_unix
                        .map(|created| crate::now_unix().saturating_sub(created));
                }
            }
        }

        self.prev_processes = next_cpu;
        self.prev_at = Some(now);
        self.roblox = roblox;
        self.apps = crate::recommend::recommendations(&usage);
    }

    fn roblox_state(&mut self) -> RobloxState {
        let mut roblox = std::mem::take(&mut self.roblox);
        roblox.running = roblox.pid.is_some();

        if let Some(pid) = roblox.pid {
            roblox.priority = platform::priority(pid);
            roblox.exe = platform::process_path(pid);
        } else {
            roblox.cpu_percent = 0.0;
            roblox.memory = 0;
        }

        roblox
    }

    /// Machine facts change rarely, so the static part is re-read every five
    /// minutes; the active power plan name is fresh on every sample.
    fn refresh_info(&mut self, now: Instant) {
        let due = self
            .info_at
            .map(|at| now.duration_since(at) >= MACHINE_RESCAN)
            .unwrap_or(true);
        if due {
            self.info_at = Some(now);
            self.info = Arc::new(platform::system_info());
        }
        if let Some(name) = platform::active_power_plan_name() {
            Arc::make_mut(&mut self.info).power_plan = name;
        }
    }

    fn refresh_installs(&mut self, now: Instant) {
        let due = self
            .installs_at
            .map(|at| now.duration_since(at) >= INSTALL_RESCAN)
            .unwrap_or(true);
        if !due {
            return;
        }
        self.installs_at = Some(now);
        self.installs = match &self.local_app_data {
            Some(dir) => crate::roblox::installs(dir),
            None => Vec::new(),
        };
    }

    fn refresh_items(&mut self, now: Instant, roblox_priority: Option<Priority>) {
        let due = self
            .verify_at
            .map(|at| now.duration_since(at) >= VERIFY_INTERVAL)
            .unwrap_or(true);
        if !due {
            return;
        }
        self.verify_at = Some(now);
        self.items = crate::optimize::verify(roblox_priority, platform::active_power_scheme());
    }
}

fn push(history: &mut VecDeque<f32>, value: f32) {
    if history.len() == HISTORY_LEN {
        history.pop_front();
    }
    history.push_back(value);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn history_is_bounded() {
        let mut history = VecDeque::new();
        for value in 0..200 {
            push(&mut history, value as f32);
        }
        assert_eq!(history.len(), HISTORY_LEN);
        assert_eq!(history.front().copied(), Some(110.0));
        assert_eq!(history.back().copied(), Some(199.0));
    }
}
