//! Application shell: state, background job worker, navigation and toasts.

use crate::icon::{self, Glyph};
use crate::theme;
use crate::widgets;
use eframe::egui::{
    self, pos2, vec2, Align, Align2, Color32, CornerRadius, Layout, Margin, RichText, Sense,
    Stroke, StrokeKind, Ui, Vec2,
};
use std::collections::BTreeSet;
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::mpsc::{self, Receiver, Sender};
use std::time::{Duration, Instant, SystemTime};
use volt_core::cleanup::{self, CleanupReport, CleanupScan};
use volt_core::format;
use volt_core::model::Snapshot;
use volt_core::monitor::Monitor;
use volt_core::optimize::{self, ApplyReport, RestoreReport, ITEMS};
use volt_core::platform;
use volt_core::report;
use volt_core::roblox;
use volt_core::state::{self, SavedState};

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum View {
    Dashboard,
    Optimize,
    Cleanup,
    System,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum ToastTone {
    Success,
    Info,
    Error,
}

pub struct Toast {
    pub text: String,
    pub tone: ToastTone,
    pub born: Instant,
}

pub(crate) enum Job {
    Apply(Vec<String>),
    Restore,
    Scan,
    Clean(Box<CleanupScan>),
    EndTasks(Vec<u32>, String),
}

pub(crate) enum JobUpdate {
    Apply(ApplyReport, SavedState),
    Restore(RestoreReport, SavedState),
    Scan(CleanupScan),
    Clean(CleanupReport, SavedState),
    EndTasks {
        label: String,
        closed: usize,
        errors: Vec<String>,
    },
}

/// Sends work to one long-lived thread so the interface never blocks on registry
/// or file-system calls.
pub struct Jobs {
    tx: Sender<Job>,
    rx: Receiver<JobUpdate>,
    pub(crate) inflight: bool,
}

impl Jobs {
    fn new() -> Self {
        let (job_tx, job_rx) = mpsc::channel::<Job>();
        let (update_tx, update_rx) = mpsc::channel::<JobUpdate>();

        let _ = std::thread::Builder::new()
            .name("volt-jobs".to_owned())
            .spawn(move || worker(job_rx, update_tx));

        Self {
            tx: job_tx,
            rx: update_rx,
            inflight: false,
        }
    }

    pub(crate) fn send(&mut self, job: Job) {
        if self.tx.send(job).is_ok() {
            self.inflight = true;
        }
    }

    pub(crate) fn scan(&mut self) {
        self.send(Job::Scan);
    }
}

fn worker(jobs: Receiver<Job>, updates: Sender<JobUpdate>) {
    let mut saved = state::load();

    while let Ok(job) = jobs.recv() {
        let update = match job {
            Job::Apply(ids) => {
                let report = optimize::apply(&ids, roblox::primary_player_pid(), &mut saved);
                let _ = state::save(&saved);
                JobUpdate::Apply(report, saved.clone())
            }
            Job::Restore => {
                let report = optimize::restore(&mut saved);
                let _ = state::save(&saved);
                JobUpdate::Restore(report, saved.clone())
            }
            Job::Scan => {
                let scan = match (platform::local_app_data(), platform::temp_dir()) {
                    (Some(local), Some(temp)) => cleanup::scan(&local, &temp, SystemTime::now()),
                    _ => CleanupScan::default(),
                };
                JobUpdate::Scan(scan)
            }
            Job::Clean(scan) => {
                let report = cleanup::clean(&scan);
                saved.record_cleanup(report.files, report.bytes);
                let _ = state::save(&saved);
                JobUpdate::Clean(report, saved.clone())
            }
            Job::EndTasks(pids, label) => {
                let mut closed = 0usize;
                let mut errors = Vec::new();
                for pid in pids {
                    match platform::terminate(pid) {
                        Ok(()) => closed += 1,
                        Err(err) => errors.push(err.to_string()),
                    }
                }
                JobUpdate::EndTasks {
                    label,
                    closed,
                    errors,
                }
            }
        };

        if updates.send(update).is_err() {
            break;
        }
    }
}

pub struct VoltApp {
    pub(crate) monitor: Monitor,
    pub(crate) snapshot: Snapshot,
    pub(crate) saved: SavedState,
    pub(crate) view: View,
    pub(crate) selected: BTreeSet<String>,
    pub(crate) scan: Option<Rc<CleanupScan>>,
    pub(crate) scanning: bool,
    pub(crate) busy: Option<&'static str>,
    pub(crate) toasts: Vec<Toast>,
    pub(crate) apply_report: Option<ApplyReport>,
    pub(crate) restore_report: Option<RestoreReport>,
    pub(crate) cleanup_report: Option<CleanupReport>,
    pub(crate) confirm_restore: bool,
    pub(crate) confirm_end: Option<String>,
    pub(crate) jobs: Jobs,
    /// Persisted: re-apply the selection when a new Roblox process appears.
    pub(crate) auto_apply: bool,
    /// Last Roblox Player PID automatic application ran for.
    auto_pid: Option<u32>,
    /// True while the in-flight apply job was started automatically.
    auto_running: bool,
    /// Where the last saved diagnostics report went.
    pub(crate) report_path: Option<PathBuf>,
}

impl VoltApp {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        theme::install(&cc.egui_ctx);

        let mut jobs = Jobs::new();
        jobs.scan();

        let saved = state::load();
        let auto_apply = saved.auto_apply;

        Self {
            monitor: Monitor::start(),
            snapshot: Snapshot {
                platform_ready: platform::is_supported(),
                ..Default::default()
            },
            saved,
            view: View::Dashboard,
            selected: ITEMS.iter().map(|item| item.id.to_owned()).collect(),
            scan: None,
            scanning: true,
            busy: None,
            toasts: Vec::new(),
            apply_report: None,
            restore_report: None,
            cleanup_report: None,
            confirm_restore: false,
            confirm_end: None,
            jobs,
            auto_apply,
            auto_pid: None,
            auto_running: false,
            report_path: None,
        }
    }

    pub(crate) fn any_busy(&self) -> bool {
        self.jobs.inflight
    }

    pub(crate) fn push_toast(&mut self, text: String, tone: ToastTone) {
        self.toasts.push(Toast {
            text,
            tone,
            born: Instant::now(),
        });
        if self.toasts.len() > 4 {
            self.toasts.remove(0);
        }
    }

    pub(crate) fn start_optimize(&mut self) {
        if self.selected.is_empty() {
            self.push_toast(
                "Select at least one optimization first.".to_owned(),
                ToastTone::Info,
            );
            return;
        }
        if self.any_busy() {
            return;
        }
        self.busy = Some("optimize");
        self.jobs
            .send(Job::Apply(self.selected.iter().cloned().collect()));
        self.monitor.wake();
    }

    /// Turn the launch watcher on or off and persist the choice.
    pub(crate) fn set_auto_apply(&mut self, enabled: bool) {
        self.auto_apply = enabled;
        self.saved.auto_apply = enabled;
        if !enabled {
            self.auto_pid = None;
        }
        match state::save(&self.saved) {
            Ok(()) => self.push_toast(
                if enabled {
                    "Volt will apply your selected tweaks whenever Roblox starts.".to_owned()
                } else {
                    "Automatic application is off.".to_owned()
                },
                ToastTone::Info,
            ),
            Err(err) => {
                self.push_toast(format!("Preference not saved: {err}"), ToastTone::Error);
            }
        }
    }

    /// Apply the selection once per new Roblox Player process, but only for items
    /// that are not already active. Runs on the same worker thread as a manual click.
    fn maybe_auto_apply(&mut self) {
        if !self.auto_apply {
            self.auto_pid = self.snapshot.roblox.pid;
            return;
        }

        let Some(pid) = self.snapshot.roblox.pid else {
            self.auto_pid = None;
            return;
        };

        if self.auto_pid == Some(pid) || self.any_busy() || self.selected.is_empty() {
            return;
        }

        let pending: Vec<String> = self
            .selected
            .iter()
            .filter(|id| {
                self.snapshot
                    .items
                    .iter()
                    .any(|state| state.id == id.as_str() && !state.active)
            })
            .cloned()
            .collect();

        self.auto_pid = Some(pid);
        if pending.is_empty() {
            return;
        }

        self.auto_running = true;
        self.busy = Some("auto");
        self.jobs.send(Job::Apply(pending));
        self.monitor.wake();
    }

    /// Write the diagnostics report next to the saved state.
    pub(crate) fn save_report(&mut self) {
        let Some(directory) = platform::app_data().map(|dir| dir.join("Volt").join("reports"))
        else {
            self.push_toast(
                "The Windows profile folder is unavailable.".to_owned(),
                ToastTone::Error,
            );
            return;
        };

        let text = report::build(&self.snapshot, &self.saved);
        let name = format!(
            "volt-report-{}.txt",
            report::file_stamp(volt_core::now_unix())
        );
        let path = directory.join(name);

        let result = std::fs::create_dir_all(&directory).and_then(|_| std::fs::write(&path, text));
        match result {
            Ok(()) => {
                self.push_toast(
                    format!("Report saved to {}", path.display()),
                    ToastTone::Success,
                );
                self.report_path = Some(path);
            }
            Err(err) => self.push_toast(
                format!("Could not save the report: {err}"),
                ToastTone::Error,
            ),
        }
    }

    pub(crate) fn start_restore(&mut self) {
        if self.any_busy() {
            return;
        }
        self.confirm_restore = false;
        self.busy = Some("restore");
        self.jobs.send(Job::Restore);
        self.monitor.wake();
    }

    fn poll_jobs(&mut self) {
        let mut received = false;
        let mut rescan = false;

        while let Ok(update) = self.jobs.rx.try_recv() {
            received = true;
            match update {
                JobUpdate::Apply(report, saved) => {
                    self.saved = saved;
                    let automatic = std::mem::take(&mut self.auto_running);
                    let failures = report.failures();
                    let applied = report.applied();
                    if failures > 0 {
                        self.push_toast(
                            if automatic {
                                format!(
                                    "Roblox started, but {failures} automatic change(s) failed. See Optimize for the reason."
                                )
                            } else {
                                format!(
                                    "Optimization finished with {failures} issue(s). Review the details."
                                )
                            },
                            ToastTone::Error,
                        );
                    } else if applied == 0 {
                        self.push_toast(
                            if automatic {
                                "Roblox started — every selected tweak was already active."
                                    .to_owned()
                            } else {
                                "Nothing needed changing — everything selected was already active."
                                    .to_owned()
                            },
                            ToastTone::Info,
                        );
                    } else {
                        self.push_toast(
                            if automatic {
                                format!(
                                    "Roblox started — {applied} tweak{} applied automatically.",
                                    if applied == 1 { "" } else { "s" }
                                )
                            } else {
                                format!(
                                    "{applied} optimization{} applied. Changes are reversible.",
                                    if applied == 1 { "" } else { "s" }
                                )
                            },
                            ToastTone::Success,
                        );
                    }
                    self.apply_report = Some(report);
                    self.monitor.wake();
                }
                JobUpdate::Restore(report, saved) => {
                    self.saved = saved;
                    let failures = report.failures();
                    let restored = report.restored;
                    if failures > 0 {
                        self.push_toast(
                            format!("Restore finished with {failures} issue(s). They stay recorded so you can retry."),
                            ToastTone::Error,
                        );
                    } else if report.restored == 0 {
                        self.push_toast(
                            "There was nothing recorded to restore.".to_owned(),
                            ToastTone::Info,
                        );
                    } else {
                        self.push_toast(
                            format!(
                                "Defaults restored — {restored} setting{} back to their original state.",
                                if report.restored == 1 { "" } else { "s" }
                            ),
                            ToastTone::Success,
                        );
                    }
                    self.restore_report = Some(report);
                    self.monitor.wake();
                }
                JobUpdate::Scan(scan) => {
                    self.scanning = false;
                    self.scan = Some(Rc::new(scan));
                }
                JobUpdate::Clean(report, saved) => {
                    self.saved = saved;
                    let freed = report.bytes;
                    let failed = report.failed;
                    if freed > 0 {
                        self.push_toast(
                            format!(
                                "Freed {} across {} file{}.",
                                format::bytes(freed),
                                report.files,
                                if report.files == 1 { "" } else { "s" }
                            ),
                            ToastTone::Success,
                        );
                    } else if failed > 0 {
                        self.push_toast(
                            "Nothing could be removed — Roblox is using those files.".to_owned(),
                            ToastTone::Error,
                        );
                    } else {
                        self.push_toast("Nothing to clean up.".to_owned(), ToastTone::Info);
                    }
                    self.cleanup_report = Some(report);
                    self.monitor.wake();
                    self.scan = None;
                    self.scanning = true;
                    rescan = true;
                }
                JobUpdate::EndTasks {
                    label,
                    closed,
                    errors,
                } => {
                    if errors.is_empty() {
                        self.push_toast(
                            format!(
                                "Closed {closed} {label} process{}.",
                                if closed == 1 { "" } else { "es" }
                            ),
                            ToastTone::Success,
                        );
                    } else {
                        self.push_toast(
                            format!(
                                "Could not close {label}: {}",
                                errors.first().cloned().unwrap_or_default()
                            ),
                            ToastTone::Error,
                        );
                    }
                    self.monitor.wake();
                }
            }
        }

        if received {
            self.jobs.inflight = false;
            self.busy = None;
        }
        if rescan {
            // Queue the follow-up scan after clearing the flag for the finished job.
            self.jobs.scan();
        }
    }

    fn prune_toasts(&mut self) {
        self.toasts
            .retain(|toast| toast.born.elapsed() < Duration::from_secs(7));
    }

    fn sidebar(&mut self, ctx: &egui::Context) {
        egui::SidePanel::left("volt-sidebar")
            .exact_width(236.0)
            .resizable(false)
            .frame(
                egui::Frame::new()
                    .fill(theme::SIDEBAR)
                    .inner_margin(Margin {
                        left: 16,
                        right: 16,
                        top: 20,
                        bottom: 18,
                    }),
            )
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    let (rect, _) = ui.allocate_exact_size(Vec2::splat(40.0), Sense::hover());
                    icon::logo(ui.painter(), rect);
                    ui.add_space(4.0);
                    ui.vertical(|ui| {
                        ui.add_space(2.0);
                        ui.label(
                            RichText::new("VOLT")
                                .font(theme::semibold(19.0))
                                .color(theme::TEXT),
                        );
                        ui.label(
                            RichText::new("ROBLOX PERFORMANCE")
                                .font(theme::medium(8.5))
                                .color(theme::DIM),
                        );
                    });
                });

                ui.add_space(30.0);

                if widgets::nav_item(ui, Glyph::Gauge, "Dashboard", self.view == View::Dashboard)
                    .clicked()
                {
                    self.view = View::Dashboard;
                }
                ui.add_space(3.0);
                if widgets::nav_item(ui, Glyph::Bolt, "Optimize", self.view == View::Optimize)
                    .clicked()
                {
                    self.view = View::Optimize;
                }
                ui.add_space(3.0);
                if widgets::nav_item(ui, Glyph::Sparkles, "Cleanup", self.view == View::Cleanup)
                    .clicked()
                {
                    self.view = View::Cleanup;
                }
                ui.add_space(3.0);
                if widgets::nav_item(ui, Glyph::Chip, "System", self.view == View::System).clicked()
                {
                    self.view = View::System;
                }

                ui.add_space(24.0);
                widgets::divider(ui);
                ui.add_space(14.0);

                let running = self.snapshot.roblox.running;
                ui.horizontal(|ui| {
                    let (rect, _) = ui.allocate_exact_size(Vec2::splat(8.0), Sense::hover());
                    widgets::dot(
                        ui.painter(),
                        rect.center(),
                        3.0,
                        if running { theme::SUCCESS } else { theme::DIM },
                    );
                    ui.add_space(2.0);
                    ui.label(
                        RichText::new(if running {
                            "Roblox detected"
                        } else {
                            "Roblox not running"
                        })
                        .font(theme::body(11.5))
                        .color(if running {
                            theme::MUTED
                        } else {
                            theme::DIM
                        }),
                    );
                });

                ui.with_layout(Layout::bottom_up(Align::Min), |ui| {
                    ui.label(
                        RichText::new("Not affiliated with Roblox Corporation.")
                            .font(theme::body(9.5))
                            .color(theme::DIM),
                    );
                    ui.add_space(8.0);
                    ui.horizontal(|ui| {
                        widgets::pill(
                            ui,
                            &format!("v{}", volt_core::APP_VERSION),
                            theme::ACCENT_BRIGHT,
                        );
                        ui.add_space(2.0);
                        widgets::pill(ui, "WINDOWS", theme::DIM);
                    });
                    ui.add_space(6.0);
                    ui.label(
                        RichText::new("Runs locally · no telemetry · no service")
                            .font(theme::body(10.0))
                            .color(theme::DIM),
                    );
                });
            });
    }

    fn header(&mut self, ui: &mut Ui) {
        let (title, subtitle) = match self.view {
            View::Dashboard => (
                "Dashboard",
                "Live measurements from this PC, and one button to optimise Roblox.",
            ),
            View::Optimize => (
                "Optimize",
                "Reversible tweaks, ranked background load and memory guidance.",
            ),
            View::Cleanup => (
                "Cleanup",
                "Roblox logs, crash dumps and temporary files — never game content.",
            ),
            View::System => (
                "System",
                "Hardware, Windows build, privileges and every change Volt has recorded.",
            ),
        };

        ui.horizontal(|ui| {
            ui.vertical(|ui| {
                ui.label(
                    RichText::new(title)
                        .font(theme::semibold(24.0))
                        .color(theme::TEXT),
                );
                ui.add_space(2.0);
                ui.label(
                    RichText::new(subtitle)
                        .font(theme::body(12.5))
                        .color(theme::MUTED),
                );
            });

            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                let running = self.snapshot.roblox.running;
                status_pill(
                    ui,
                    if running {
                        "Roblox detected"
                    } else {
                        "Roblox not running"
                    },
                    if running { theme::SUCCESS } else { theme::DIM },
                );

                ui.add_space(6.0);
                let elevated = self.snapshot.elevated;
                status_pill(
                    ui,
                    if elevated {
                        "Administrator"
                    } else {
                        "Standard user"
                    },
                    if elevated {
                        theme::ACCENT_BRIGHT
                    } else {
                        theme::DIM
                    },
                );

                if self.any_busy() {
                    let (rect, _) = ui.allocate_exact_size(vec2(26.0, 26.0), Sense::hover());
                    widgets::spinner(
                        ui,
                        rect.center(),
                        7.0,
                        theme::alpha(theme::ACCENT_BRIGHT, 220),
                    );
                }
            });
        });
    }

    fn central(&mut self, ctx: &egui::Context) {
        egui::CentralPanel::default()
            .frame(egui::Frame::new().fill(theme::BG).inner_margin(Margin {
                left: 26,
                right: 26,
                top: 20,
                bottom: 14,
            }))
            .show(ctx, |ui| {
                widgets::corner_glow(ui.painter(), ui.max_rect());
                self.header(ui);
                ui.add_space(16.0);

                egui::ScrollArea::vertical()
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        ui.set_width(ui.available_width());
                        match self.view {
                            View::Dashboard => self.dashboard(ui),
                            View::Optimize => self.optimize(ui),
                            View::Cleanup => self.cleanup(ui),
                            View::System => self.system(ui),
                        }

                        ui.add_space(20.0);
                        ui.label(
                            RichText::new(
                                "All values are measured on this PC. Volt never estimates frame rate or gains.",
                            )
                            .font(theme::body(10.5))
                            .color(theme::DIM),
                        );
                        ui.add_space(6.0);
                    });
            });
    }

    fn toast_layer(&mut self, ctx: &egui::Context) {
        if self.toasts.is_empty() {
            return;
        }

        egui::Area::new(egui::Id::new("volt-toasts"))
            .anchor(Align2::RIGHT_BOTTOM, vec2(-22.0, -20.0))
            .order(egui::Order::Foreground)
            .interactable(false)
            .show(ctx, |ui| {
                ui.set_max_width(360.0);
                for toast in self.toasts.iter().rev() {
                    let color = match toast.tone {
                        ToastTone::Success => theme::SUCCESS,
                        ToastTone::Info => theme::ACCENT_BRIGHT,
                        ToastTone::Error => theme::DANGER,
                    };
                    egui::Frame::new()
                        .fill(theme::CARD)
                        .stroke(Stroke::new(1.0_f32, theme::alpha(color, 90)))
                        .corner_radius(CornerRadius::same(10))
                        .inner_margin(Margin::symmetric(14, 10))
                        .show(ui, |ui| {
                            ui.horizontal(|ui| {
                                let (rect, _) =
                                    ui.allocate_exact_size(Vec2::splat(10.0), Sense::hover());
                                widgets::dot(ui.painter(), rect.center(), 3.2, color);
                                ui.add_space(2.0);
                                ui.label(
                                    RichText::new(&toast.text)
                                        .font(theme::body(12.0))
                                        .color(theme::TEXT),
                                );
                            });
                        });
                    ui.add_space(6.0);
                }
            });
    }
}

fn status_pill(ui: &mut Ui, text: &str, color: Color32) {
    let font = theme::medium(11.5);
    let galley =
        ui.fonts(|fonts| fonts.layout_no_wrap(text.to_owned(), font.clone(), theme::MUTED));
    let size = Vec2::new(galley.size().x + 34.0, 26.0);
    let (rect, _) = ui.allocate_exact_size(size, Sense::hover());
    let painter = ui.painter();
    painter.rect_filled(rect, CornerRadius::same(13), theme::CARD);
    painter.rect_stroke(
        rect,
        CornerRadius::same(13),
        Stroke::new(1.0_f32, theme::BORDER),
        StrokeKind::Inside,
    );
    widgets::dot(
        painter,
        pos2(rect.left() + 14.0, rect.center().y),
        3.2,
        color,
    );
    painter.text(
        pos2(rect.left() + 25.0, rect.center().y),
        Align2::LEFT_CENTER,
        text,
        font,
        theme::MUTED,
    );
}

impl eframe::App for VoltApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.poll_jobs();
        if let Some(snapshot) = self.monitor.latest() {
            self.snapshot = snapshot;
        }
        self.maybe_auto_apply();
        self.prune_toasts();

        if ctx.input(|input| input.key_pressed(egui::Key::Escape)) {
            self.confirm_restore = false;
            self.confirm_end = None;
        }

        self.sidebar(ctx);
        self.central(ctx);
        self.toast_layer(ctx);

        let busy = self.any_busy() || self.scanning;
        ctx.request_repaint_after(Duration::from_millis(if busy { 90 } else { 1_000 }));
        if !self.toasts.is_empty() {
            ctx.request_repaint_after(Duration::from_millis(150));
        }
    }
}
