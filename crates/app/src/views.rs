//! The three Volt views: Dashboard, Optimize and Cleanup.

use crate::app::{Job, ToastTone, VoltApp};
use crate::icon::{self, Glyph};
use crate::theme;
use crate::widgets::{self, Metric};
use eframe::egui::{self, vec2, Align, Color32, Layout, RichText, Sense, Ui, Vec2};
use std::path::PathBuf;
use volt_core::cleanup::TargetScan;
use volt_core::format;
use volt_core::model::FpsStatus;
use volt_core::optimize::{Step, StepStatus, ITEMS};
use volt_core::platform;
use volt_core::recommend::Tone;
use volt_core::roblox;

impl VoltApp {
    fn recorded_changes(&self) -> usize {
        self.saved.backups.len()
            + usize::from(self.saved.power_scheme.is_some())
            + self.saved.priority_pids.len()
    }

    // ---------------------------------------------------------------- dashboard

    pub(crate) fn dashboard(&mut self, ui: &mut Ui) {
        let available = ui.available_width();
        let columns: usize = if available >= 940.0 {
            4
        } else if available >= 620.0 {
            2
        } else {
            1
        };
        let gap = 12.0;
        let card_width =
            ((available - gap * (columns as f32 - 1.0)) / columns as f32 - 2.0).max(148.0);

        let metrics = self.metrics();
        for chunk in metrics.chunks(columns) {
            ui.horizontal_top(|ui| {
                ui.spacing_mut().item_spacing.x = gap;
                for metric in chunk {
                    widgets::metric_card(ui, card_width, metric);
                }
            });
            ui.add_space(gap);
        }

        self.action_card(ui);
        ui.add_space(gap);

        if available >= 860.0 {
            let chart_width = (available - gap) * 0.60;
            let status_width = available - gap - chart_width;
            ui.horizontal_top(|ui| {
                ui.spacing_mut().item_spacing.x = gap;
                ui.allocate_ui_with_layout(
                    vec2(chart_width, 0.0),
                    Layout::top_down(Align::Min),
                    |ui| {
                        ui.set_width(chart_width);
                        self.overview_card(ui);
                    },
                );
                ui.allocate_ui_with_layout(
                    vec2(status_width, 0.0),
                    Layout::top_down(Align::Min),
                    |ui| {
                        ui.set_width(status_width);
                        self.status_card(ui);
                    },
                );
            });
        } else {
            self.overview_card(ui);
            ui.add_space(gap);
            self.status_card(ui);
        }

        ui.add_space(gap);
        self.client_card(ui);
    }

    fn metrics(&self) -> Vec<Metric<'static>> {
        let snapshot = &self.snapshot;
        let system = &snapshot.system;
        let roblox = &snapshot.roblox;

        let (fps_value, fps_sub, fps_tone, fps_tag) =
            match (snapshot.fps.measured.as_ref(), snapshot.fps.status) {
                (Some(measured), _) => (
                    format::fps(measured.fps),
                    format!("Roblox log · {}s ago", measured.age_secs),
                    theme::ACCENT_BRIGHT,
                    Some("MEASURED"),
                ),
                (None, FpsStatus::NotRunning) => (
                    "—".to_owned(),
                    "Roblox is not running".to_owned(),
                    theme::DIM,
                    None,
                ),
                (None, FpsStatus::Waiting) => (
                    "—".to_owned(),
                    "Waiting for Roblox output".to_owned(),
                    theme::DIM,
                    None,
                ),
                (None, _) => (
                    "—".to_owned(),
                    "Not exposed by Roblox · never estimated".to_owned(),
                    theme::DIM,
                    Some("N/A"),
                ),
            };

        let roblox_memory_value = if roblox.running {
            format::bytes(roblox.memory)
        } else {
            "—".to_owned()
        };
        let roblox_memory_sub = if roblox.running {
            format!(
                "PID {} · {} priority",
                roblox.pid.unwrap_or_default(),
                roblox
                    .priority
                    .map(|priority| priority.label())
                    .unwrap_or("Normal")
            )
        } else {
            "Client not detected".to_owned()
        };

        vec![
            Metric {
                label: "Frame rate",
                value: fps_value,
                sub: fps_sub,
                tone: fps_tone,
                tag: fps_tag,
            },
            Metric {
                label: "CPU usage",
                value: format::percent(system.cpu_percent),
                sub: format!("System-wide · {} processes", system.processes),
                tone: theme::TEXT,
                tag: Some("LIVE"),
            },
            Metric {
                label: "Memory in use",
                value: format::percent(system.ram_percent),
                sub: format!(
                    "{} of {}",
                    format::bytes(system.ram_used),
                    format::bytes(system.ram_total)
                ),
                tone: theme::TEXT,
                tag: Some("LIVE"),
            },
            Metric {
                label: "Roblox memory",
                value: roblox_memory_value,
                sub: roblox_memory_sub,
                tone: if roblox.running {
                    theme::TEXT
                } else {
                    theme::DIM
                },
                tag: if roblox.running { Some("CLIENT") } else { None },
            },
        ]
    }

    fn action_card(&mut self, ui: &mut Ui) {
        let total = ITEMS.len();
        let active = self.snapshot.active_items();
        let running = self.snapshot.roblox.running;
        let busy = self.any_busy();
        let recorded = self.recorded_changes();
        let busy_label = self.busy;

        let mut optimize = false;
        let mut ask_restore = false;
        let mut do_restore = false;
        let mut cancel = false;

        widgets::card(ui, |ui| {
            ui.horizontal(|ui| {
                let text_width = (ui.available_width() - 392.0).max(190.0);
                ui.allocate_ui_with_layout(
                    vec2(text_width, 0.0),
                    Layout::top_down(Align::Min),
                    |ui| {
                        ui.set_width(text_width);
                        ui.label(
                            RichText::new(if active == total {
                                "FULLY OPTIMIZED"
                            } else if active > 0 {
                                "PARTIALLY OPTIMIZED"
                            } else {
                                "READY WHEN YOU ARE"
                            })
                            .font(theme::medium(10.0))
                            .color(theme::ACCENT_BRIGHT),
                        );
                        ui.add_space(5.0);
                        ui.label(
                            RichText::new(if running {
                                "Roblox is running"
                            } else {
                                "Optimize Roblox"
                            })
                            .font(theme::semibold(19.0))
                            .color(theme::TEXT),
                        );
                        ui.add_space(3.0);
                        let mut detail = format!("{active} of {total} tweaks active right now");
                        if recorded > 0 {
                            detail.push_str(&format!(
                                " · {recorded} reversible change{} recorded",
                                if recorded == 1 { "" } else { "s" }
                            ));
                        }
                        ui.label(
                            RichText::new(detail)
                                .font(theme::body(12.0))
                                .color(theme::MUTED),
                        );
                    },
                );

                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    let label = if busy && busy_label == Some("optimize") {
                        "Optimizing…"
                    } else {
                        "Optimize Roblox"
                    };
                    optimize =
                        widgets::button(ui, label, widgets::primary_visual(), 186.0, 44.0, !busy)
                            .clicked();
                    ui.add_space(8.0);
                    ask_restore = widgets::button(
                        ui,
                        "Restore defaults",
                        widgets::ghost_visual(),
                        156.0,
                        40.0,
                        !busy && recorded > 0,
                    )
                    .clicked();
                });
            });

            if self.confirm_restore {
                ui.add_space(14.0);
                widgets::soft_card(ui, |ui| {
                    ui.horizontal(|ui| {
                        let width = (ui.available_width() - 250.0).max(180.0);
                        ui.allocate_ui_with_layout(
                            vec2(width, 0.0),
                            Layout::top_down(Align::Min),
                            |ui| {
                                ui.set_width(width);
                                ui.label(
                                    RichText::new("Restore every recorded change?")
                                        .font(theme::medium(12.5))
                                        .color(theme::TEXT),
                                );
                                ui.add_space(2.0);
                                ui.label(
                                    RichText::new(
                                        "Registry values, the power plan and Roblox priority return to what they were before Volt changed them.",
                                    )
                                    .font(theme::body(11.0))
                                    .color(theme::MUTED),
                                );
                            },
                        );
                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                            do_restore = widgets::button(
                                ui,
                                "Yes, restore",
                                widgets::danger_visual(),
                                120.0,
                                32.0,
                                true,
                            )
                            .clicked();
                            ui.add_space(6.0);
                            cancel = widgets::button(
                                ui,
                                "Cancel",
                                widgets::subtle_visual(),
                                82.0,
                                32.0,
                                true,
                            )
                            .clicked();
                        });
                    });
                });
            }
        });

        if optimize {
            self.start_optimize();
        }
        if ask_restore {
            self.confirm_restore = true;
        }
        if do_restore {
            self.start_restore();
        }
        if cancel {
            self.confirm_restore = false;
        }
    }

    fn overview_card(&self, ui: &mut Ui) {
        let system = &self.snapshot.system;
        widgets::card(ui, |ui| {
            ui.horizontal(|ui| {
                ui.vertical(|ui| {
                    ui.label(
                        RichText::new("Performance overview")
                            .font(theme::semibold(14.0))
                            .color(theme::TEXT),
                    );
                    ui.label(
                        RichText::new("Last 90 seconds, sampled every second on this PC")
                            .font(theme::body(10.5))
                            .color(theme::DIM),
                    );
                });
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    legend(
                        ui,
                        "RAM",
                        theme::ACCENT,
                        format::percent(system.ram_percent),
                    );
                    ui.add_space(12.0);
                    legend(
                        ui,
                        "CPU",
                        theme::ACCENT_BRIGHT,
                        format::percent(system.cpu_percent),
                    );
                });
            });
            ui.add_space(12.0);
            let series = [
                (self.snapshot.cpu_history.as_slice(), theme::ACCENT_BRIGHT),
                (self.snapshot.ram_history.as_slice(), theme::ACCENT),
            ];
            widgets::sparkline(ui, &series, 118.0);
        });
    }

    fn status_card(&self, ui: &mut Ui) {
        let total = ITEMS.len();
        let active = self.snapshot.active_items();
        let color = if active == total {
            theme::SUCCESS
        } else if active > 0 {
            theme::WARN
        } else {
            theme::DIM
        };

        widgets::card(ui, |ui| {
            ui.horizontal(|ui| {
                ui.vertical(|ui| {
                    ui.label(
                        RichText::new("Optimization status")
                            .font(theme::semibold(14.0))
                            .color(theme::TEXT),
                    );
                    ui.label(
                        RichText::new("Checked live against Windows every 5 seconds")
                            .font(theme::body(10.5))
                            .color(theme::DIM),
                    );
                });
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    widgets::pill(ui, &format!("{active}/{total}"), color);
                });
            });

            ui.add_space(13.0);
            widgets::progress(ui, active as f32 / total as f32, color);
            ui.add_space(14.0);

            for item in ITEMS {
                let is_active = self
                    .snapshot
                    .items
                    .iter()
                    .any(|state| state.id == item.id && state.active);
                ui.horizontal(|ui| {
                    let (rect, _) = ui.allocate_exact_size(Vec2::splat(10.0), Sense::hover());
                    widgets::dot(
                        ui.painter(),
                        rect.center(),
                        3.0,
                        if is_active {
                            theme::SUCCESS
                        } else {
                            theme::DIM
                        },
                    );
                    ui.add_space(2.0);
                    ui.label(
                        RichText::new(item.title)
                            .font(theme::body(12.0))
                            .color(if is_active { theme::TEXT } else { theme::MUTED }),
                    );
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        ui.label(
                            RichText::new(if is_active { "Active" } else { "Off" })
                                .font(theme::medium(10.5))
                                .color(if is_active {
                                    theme::SUCCESS
                                } else {
                                    theme::DIM
                                }),
                        );
                    });
                });
                ui.add_space(4.0);
            }
        });
    }

    fn client_card(&self, ui: &mut Ui) {
        let roblox_state = &self.snapshot.roblox;
        let installs = &self.snapshot.installs;

        widgets::card(ui, |ui| {
            ui.horizontal(|ui| {
                ui.vertical(|ui| {
                    ui.label(
                        RichText::new("Roblox client")
                            .font(theme::semibold(14.0))
                            .color(theme::TEXT),
                    );
                    ui.label(
                        RichText::new("Read from the running process and your installation folder")
                            .font(theme::body(10.5))
                            .color(theme::DIM),
                    );
                });
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    widgets::pill(
                        ui,
                        if roblox_state.running {
                            "DETECTED"
                        } else {
                            "NOT RUNNING"
                        },
                        if roblox_state.running {
                            theme::SUCCESS
                        } else {
                            theme::DIM
                        },
                    );
                });
            });

            ui.add_space(12.0);

            if roblox_state.running {
                let pairs: Vec<(&str, String)> = vec![
                    (
                        "Process ID",
                        roblox_state
                            .pid
                            .map(|pid| pid.to_string())
                            .unwrap_or_else(|| "—".to_owned()),
                    ),
                    (
                        "Priority",
                        roblox_state
                            .priority
                            .map(|priority| priority.label().to_owned())
                            .unwrap_or_else(|| "—".to_owned()),
                    ),
                    ("Memory", format::bytes(roblox_state.memory)),
                    ("CPU share", format::percent(roblox_state.cpu_percent)),
                    ("Threads", roblox_state.threads.to_string()),
                    (
                        "Session",
                        roblox_state
                            .uptime_secs
                            .map(format::duration)
                            .unwrap_or_else(|| "—".to_owned()),
                    ),
                    ("Clients", roblox_state.instances.to_string()),
                    (
                        "Client build",
                        installs
                            .first()
                            .map(|install| install.build.clone())
                            .unwrap_or_else(|| "—".to_owned()),
                    ),
                ];

                ui.columns(2, |columns| {
                    for (index, (label, value)) in pairs.iter().enumerate() {
                        let column = &mut columns[index % 2];
                        widgets::key_value(column, label, value, theme::TEXT);
                        column.add_space(5.0);
                    }
                });

                if let Some(exe) = roblox_state.exe.as_ref() {
                    ui.add_space(6.0);
                    ui.label(
                        RichText::new("Executable")
                            .font(theme::body(10.5))
                            .color(theme::DIM),
                    );
                    ui.label(
                        RichText::new(shorten(&exe.display().to_string(), 92))
                            .font(egui::FontId::new(11.0, egui::FontFamily::Monospace))
                            .color(theme::MUTED),
                    );
                }
            } else {
                widgets::soft_card(ui, |ui| {
                    ui.label(
                        RichText::new("Launch Roblox Player and Volt picks it up within a second.")
                            .font(theme::body(12.0))
                            .color(theme::MUTED),
                    );
                    ui.add_space(3.0);
                    ui.label(
                        RichText::new(if installs.is_empty() {
                            "No Roblox Player installation was found in %LOCALAPPDATA%\\Roblox\\Versions."
                                .to_owned()
                        } else {
                            format!(
                                "{} Roblox installation{} found on disk.",
                                installs.len(),
                                if installs.len() == 1 { "" } else { "s" }
                            )
                        })
                        .font(theme::body(11.0))
                        .color(theme::DIM),
                    );
                });
            }
        });
    }

    // ----------------------------------------------------------------- optimize

    pub(crate) fn optimize(&mut self, ui: &mut Ui) {
        let total = ITEMS.len();
        let active = self.snapshot.active_items();
        let selected = self.selected.len();
        let running = self.snapshot.roblox.running;
        let busy = self.any_busy();
        let recorded = self.recorded_changes();

        let mut apply = false;
        let mut ask_restore = false;
        let mut do_restore = false;
        let mut cancel = false;

        widgets::card(ui, |ui| {
            ui.horizontal(|ui| {
                let width = (ui.available_width() - 330.0).max(190.0);
                ui.allocate_ui_with_layout(
                    vec2(width, 0.0),
                    Layout::top_down(Align::Min),
                    |ui| {
                        ui.set_width(width);
                        ui.label(
                            RichText::new("Reversible optimizations")
                                .font(theme::semibold(15.0))
                                .color(theme::TEXT),
                        );
                        ui.add_space(2.0);
                        ui.label(
                            RichText::new(format!(
                                "{active} of {total} active · {recorded} change{} recorded for Restore Defaults",
                                if recorded == 1 { "" } else { "s" }
                            ))
                            .font(theme::body(11.5))
                            .color(theme::MUTED),
                        );
                    },
                );
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    apply = widgets::button(
                        ui,
                        if busy { "Applying…" } else { "Apply selected" },
                        widgets::primary_visual(),
                        150.0,
                        38.0,
                        !busy && selected > 0,
                    )
                    .clicked();
                    ui.add_space(8.0);
                    ask_restore = widgets::button(
                        ui,
                        "Restore defaults",
                        widgets::ghost_visual(),
                        148.0,
                        36.0,
                        !busy && recorded > 0,
                    )
                    .clicked();
                });
            });

            if self.confirm_restore {
                ui.add_space(12.0);
                widgets::soft_card(ui, |ui| {
                    ui.horizontal(|ui| {
                        let width = (ui.available_width() - 250.0).max(180.0);
                        ui.allocate_ui_with_layout(
                            vec2(width, 0.0),
                            Layout::top_down(Align::Min),
                            |ui| {
                                ui.set_width(width);
                                ui.label(
                                    RichText::new("Undo every change Volt made?")
                                        .font(theme::medium(12.5))
                                        .color(theme::TEXT),
                                );
                                ui.label(
                                    RichText::new(
                                        "Values that did not exist before will be removed again.",
                                    )
                                    .font(theme::body(11.0))
                                    .color(theme::MUTED),
                                );
                            },
                        );
                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                            do_restore = widgets::button(
                                ui,
                                "Yes, restore",
                                widgets::danger_visual(),
                                120.0,
                                32.0,
                                true,
                            )
                            .clicked();
                            ui.add_space(6.0);
                            cancel = widgets::button(
                                ui,
                                "Cancel",
                                widgets::subtle_visual(),
                                82.0,
                                32.0,
                                true,
                            )
                            .clicked();
                        });
                    });
                });
            }
        });

        if apply {
            self.start_optimize();
        }
        if ask_restore {
            self.confirm_restore = true;
        }
        if do_restore {
            self.start_restore();
        }
        if cancel {
            self.confirm_restore = false;
        }

        ui.add_space(12.0);

        for item in ITEMS {
            let is_active = self
                .snapshot
                .items
                .iter()
                .any(|state| state.id == item.id && state.active);
            let is_selected = self.selected.contains(item.id);
            let needs_roblox = item.needs_roblox && !running;
            let mut toggled = false;

            widgets::soft_card(ui, |ui| {
                ui.horizontal(|ui| {
                    toggled = widgets::checkbox(ui, is_selected).clicked();
                    ui.add_space(8.0);
                    let width = (ui.available_width() - 150.0).max(170.0);
                    ui.allocate_ui_with_layout(
                        vec2(width, 0.0),
                        Layout::top_down(Align::Min),
                        |ui| {
                            ui.set_width(width);
                            ui.horizontal(|ui| {
                                ui.label(
                                    RichText::new(item.title)
                                        .font(theme::medium(13.0))
                                        .color(theme::TEXT),
                                );
                                if is_active {
                                    widgets::pill(ui, "ACTIVE", theme::SUCCESS);
                                } else if needs_roblox {
                                    widgets::pill(ui, "NEEDS ROBLOX", theme::WARN);
                                }
                            });
                            ui.add_space(2.0);
                            ui.label(
                                RichText::new(item.summary)
                                    .font(theme::body(11.5))
                                    .color(theme::MUTED),
                            );
                        },
                    );
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        widgets::pill(
                            ui,
                            if is_selected { "SELECTED" } else { "SKIPPED" },
                            if is_selected {
                                theme::ACCENT_BRIGHT
                            } else {
                                theme::DIM
                            },
                        );
                    });
                });
            })
            .on_hover_text(item.note);

            if toggled {
                if is_selected {
                    self.selected.remove(item.id);
                } else {
                    self.selected.insert(item.id.to_owned());
                }
            }
            ui.add_space(8.0);
        }

        if let Some(report) = self.apply_report.clone() {
            ui.add_space(4.0);
            self.steps_card(ui, "Last optimization run", &report.steps);
            ui.add_space(12.0);
        }
        if let Some(report) = self.restore_report.clone() {
            self.steps_card(ui, "Last restore run", &report.steps);
            ui.add_space(12.0);
        }

        self.background_card(ui);
        ui.add_space(12.0);
        self.guidance_card(ui);
        ui.add_space(12.0);
        self.network_card(ui);
        ui.add_space(12.0);
        self.installs_card(ui);
    }

    fn steps_card(&self, ui: &mut Ui, title: &str, steps: &[Step]) {
        if steps.is_empty() {
            return;
        }
        widgets::card(ui, |ui| {
            ui.label(
                RichText::new(title)
                    .font(theme::semibold(14.0))
                    .color(theme::TEXT),
            );
            ui.add_space(10.0);
            for step in steps {
                let color = match step.status {
                    StepStatus::Applied | StepStatus::Restored | StepStatus::AlreadyActive => {
                        theme::SUCCESS
                    }
                    StepStatus::Failed => theme::DANGER,
                    StepStatus::Skipped | StepStatus::NothingToRestore => theme::DIM,
                };
                ui.horizontal(|ui| {
                    let (rect, _) = ui.allocate_exact_size(Vec2::splat(10.0), Sense::hover());
                    widgets::dot(ui.painter(), rect.center(), 3.0, color);
                    ui.add_space(2.0);
                    let width = (ui.available_width() - 132.0).max(180.0);
                    ui.allocate_ui_with_layout(
                        vec2(width, 0.0),
                        Layout::top_down(Align::Min),
                        |ui| {
                            ui.set_width(width);
                            ui.label(
                                RichText::new(step.title)
                                    .font(theme::medium(12.0))
                                    .color(theme::TEXT),
                            );
                            if !step.detail.is_empty() {
                                ui.label(
                                    RichText::new(&step.detail)
                                        .font(theme::body(11.0))
                                        .color(theme::MUTED),
                                );
                            }
                        },
                    );
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        ui.label(
                            RichText::new(step.status.label())
                                .font(theme::medium(10.5))
                                .color(color),
                        );
                    });
                });
                ui.add_space(8.0);
            }
        });
    }

    fn background_card(&mut self, ui: &mut Ui) {
        let apps = self.snapshot.apps.clone();
        let current_confirm = self.confirm_end.clone();
        let mut request_confirm: Option<String> = None;
        let mut cancel = false;
        let mut end: Option<(Vec<u32>, String)> = None;

        widgets::card(ui, |ui| {
            ui.horizontal(|ui| {
                ui.vertical(|ui| {
                    ui.label(
                        RichText::new("Background load")
                            .font(theme::semibold(14.0))
                            .color(theme::TEXT),
                    );
                    ui.label(
                        RichText::new(
                            "Ranked by measured CPU and memory. Volt never closes anything by itself.",
                        )
                        .font(theme::body(10.5))
                        .color(theme::DIM),
                    );
                });
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    widgets::pill(
                        ui,
                        &format!(
                            "{} APP{}",
                            apps.len(),
                            if apps.len() == 1 { "" } else { "S" }
                        ),
                        if apps.is_empty() { theme::DIM } else { theme::WARN },
                    );
                });
            });

            ui.add_space(12.0);

            if apps.is_empty() {
                widgets::soft_card(ui, |ui| {
                    ui.label(
                        RichText::new(
                            "No heavy background programs are competing with Roblox right now.",
                        )
                        .font(theme::body(12.0))
                        .color(theme::MUTED),
                    );
                });
                return;
            }

            for app in &apps {
                let confirming = current_confirm.as_deref() == Some(app.exe.as_str());
                widgets::soft_card(ui, |ui| {
                    ui.horizontal(|ui| {
                        let width = (ui.available_width() - 300.0).max(150.0);
                        ui.allocate_ui_with_layout(
                            vec2(width, 0.0),
                            Layout::top_down(Align::Min),
                            |ui| {
                                ui.set_width(width);
                                ui.horizontal(|ui| {
                                    ui.label(
                                        RichText::new(&app.title)
                                            .font(theme::medium(12.5))
                                            .color(theme::TEXT),
                                    );
                                    if app.instances > 1 {
                                        widgets::pill(
                                            ui,
                                            &format!("{} PROCESSES", app.instances),
                                            theme::DIM,
                                        );
                                    }
                                });
                                ui.add_space(1.0);
                                ui.label(
                                    RichText::new(&app.note)
                                        .font(theme::body(11.0))
                                        .color(theme::MUTED),
                                );
                            },
                        );
                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                            if confirming {
                                if widgets::button(
                                    ui,
                                    "Confirm",
                                    widgets::danger_visual(),
                                    84.0,
                                    28.0,
                                    true,
                                )
                                .clicked()
                                {
                                    end = Some((app.pids.clone(), app.title.clone()));
                                }
                                ui.add_space(4.0);
                                if widgets::button(
                                    ui,
                                    "Cancel",
                                    widgets::subtle_visual(),
                                    70.0,
                                    28.0,
                                    true,
                                )
                                .clicked()
                                {
                                    cancel = true;
                                }
                            } else if widgets::button(
                                ui,
                                "End task",
                                widgets::ghost_visual(),
                                84.0,
                                28.0,
                                true,
                            )
                            .clicked()
                            {
                                request_confirm = Some(app.exe.clone());
                            }
                            ui.add_space(10.0);
                            ui.label(
                                RichText::new(format::bytes(app.memory))
                                    .font(theme::medium(11.0))
                                    .color(theme::MUTED),
                            );
                            ui.add_space(6.0);
                            ui.label(
                                RichText::new(format!("{} CPU", format::percent(app.cpu_percent)))
                                    .font(theme::body(11.0))
                                    .color(theme::DIM),
                            );
                        });
                    });
                });
                ui.add_space(6.0);
            }
        });

        if let Some(exe) = request_confirm {
            self.confirm_end = Some(exe);
        }
        if cancel {
            self.confirm_end = None;
        }
        if let Some((pids, label)) = end {
            self.confirm_end = None;
            self.jobs.send(Job::EndTasks(pids, label));
        }
    }

    fn guidance_card(&self, ui: &mut Ui) {
        let tips = volt_core::recommend::memory_tips(&self.snapshot.system, &self.snapshot.roblox);

        widgets::card(ui, |ui| {
            widgets::section_title(
                ui,
                "Memory guidance",
                Some("Derived from the numbers above. No promised gains."),
            );
            ui.add_space(12.0);

            for tip in &tips {
                let color = match tip.tone {
                    Tone::Good => theme::SUCCESS,
                    Tone::Info => theme::ACCENT_BRIGHT,
                    Tone::Warn => theme::WARN,
                };
                ui.horizontal(|ui| {
                    let (rect, _) = ui.allocate_exact_size(Vec2::new(3.0, 34.0), Sense::hover());
                    ui.painter().rect_filled(
                        rect,
                        egui::CornerRadius::same(2),
                        theme::alpha(color, 200),
                    );
                    ui.add_space(8.0);
                    ui.vertical(|ui| {
                        ui.label(
                            RichText::new(&tip.title)
                                .font(theme::medium(12.0))
                                .color(theme::TEXT),
                        );
                        ui.label(
                            RichText::new(&tip.body)
                                .font(theme::body(11.0))
                                .color(theme::MUTED),
                        );
                    });
                });
                ui.add_space(10.0);
            }
        });
    }

    fn network_card(&self, ui: &mut Ui) {
        let tips = volt_core::recommend::network_tips();

        widgets::card(ui, |ui| {
            widgets::section_title(
                ui,
                "Network guidance",
                Some("Recommendations only — Volt changes no network settings and makes no ping claims."),
            );
            ui.add_space(12.0);

            for tip in &tips {
                ui.horizontal(|ui| {
                    let (rect, _) = ui.allocate_exact_size(Vec2::new(3.0, 30.0), Sense::hover());
                    ui.painter().rect_filled(
                        rect,
                        egui::CornerRadius::same(2),
                        theme::alpha(theme::ACCENT, 180),
                    );
                    ui.add_space(8.0);
                    ui.vertical(|ui| {
                        ui.label(
                            RichText::new(&tip.title)
                                .font(theme::medium(12.0))
                                .color(theme::TEXT),
                        );
                        ui.label(
                            RichText::new(&tip.body)
                                .font(theme::body(11.0))
                                .color(theme::MUTED),
                        );
                    });
                });
                ui.add_space(9.0);
            }
        });
    }

    fn installs_card(&mut self, ui: &mut Ui) {
        let installs = self.snapshot.installs.clone();
        let logs = platform::local_app_data().map(|dir| roblox::logs_dir(&dir));
        let mut reveal: Option<PathBuf> = None;
        let mut open_logs = false;

        widgets::card(ui, |ui| {
            ui.horizontal(|ui| {
                ui.vertical(|ui| {
                    ui.label(
                        RichText::new("Roblox Player installations")
                            .font(theme::semibold(14.0))
                            .color(theme::TEXT),
                    );
                    ui.label(
                        RichText::new("Detected in %LOCALAPPDATA%\\Roblox\\Versions")
                            .font(theme::body(10.5))
                            .color(theme::DIM),
                    );
                });
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    open_logs = widgets::button(
                        ui,
                        "Open logs folder",
                        widgets::ghost_visual(),
                        138.0,
                        30.0,
                        true,
                    )
                    .clicked();
                });
            });

            ui.add_space(12.0);

            if installs.is_empty() {
                ui.label(
                    RichText::new("No installation folders found on this account.")
                        .font(theme::body(11.5))
                        .color(theme::MUTED),
                );
                return;
            }

            for install in &installs {
                widgets::soft_card(ui, |ui| {
                    ui.horizontal(|ui| {
                        let width = (ui.available_width() - 120.0).max(160.0);
                        ui.allocate_ui_with_layout(
                            vec2(width, 0.0),
                            Layout::top_down(Align::Min),
                            |ui| {
                                ui.set_width(width);
                                ui.label(
                                    RichText::new(&install.build)
                                        .font(theme::medium(12.0))
                                        .color(theme::TEXT),
                                );
                                ui.label(
                                    RichText::new(shorten(&install.dir.display().to_string(), 74))
                                        .font(egui::FontId::new(10.5, egui::FontFamily::Monospace))
                                        .color(theme::DIM),
                                );
                            },
                        );
                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                            if widgets::button(
                                ui,
                                "Open folder",
                                widgets::ghost_visual(),
                                104.0,
                                28.0,
                                true,
                            )
                            .clicked()
                            {
                                reveal = Some(install.dir.clone());
                            }
                        });
                    });
                });
                ui.add_space(6.0);
            }
        });

        if let Some(path) = reveal {
            if let Err(err) = platform::reveal_path(&path) {
                self.push_toast(err.to_string(), ToastTone::Error);
            }
        }
        if open_logs {
            if let Some(logs) = logs {
                if let Err(err) = platform::open_folder(&logs) {
                    self.push_toast(err.to_string(), ToastTone::Error);
                }
            }
        }
    }

    // ------------------------------------------------------------------ cleanup

    pub(crate) fn cleanup(&mut self, ui: &mut Ui) {
        let scan = self.scan.clone();
        let files = scan.as_ref().map(|scan| scan.files()).unwrap_or(0);
        let bytes = scan.as_ref().map(|scan| scan.bytes()).unwrap_or(0);
        let busy = self.any_busy();
        let scanning = self.scanning;

        let mut clean = false;
        let mut rescan = false;

        widgets::card(ui, |ui| {
            ui.horizontal(|ui| {
                let width = (ui.available_width() - 290.0).max(190.0);
                ui.allocate_ui_with_layout(vec2(width, 0.0), Layout::top_down(Align::Min), |ui| {
                    ui.set_width(width);
                    ui.label(
                        RichText::new("RECLAIMABLE NOW")
                            .font(theme::medium(10.0))
                            .color(theme::DIM),
                    );
                    ui.add_space(4.0);
                    ui.label(
                        RichText::new(if scanning && files == 0 {
                            "Scanning…".to_owned()
                        } else {
                            format::bytes(bytes)
                        })
                        .font(theme::semibold(31.0))
                        .color(if bytes > 0 {
                            theme::TEXT
                        } else {
                            theme::MUTED
                        }),
                    );
                    ui.add_space(1.0);
                    ui.label(
                        RichText::new(format!(
                            "{files} file{} in Roblox diagnostics and temporary folders",
                            if files == 1 { "" } else { "s" }
                        ))
                        .font(theme::body(11.5))
                        .color(theme::MUTED),
                    );
                });
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    clean = widgets::button(
                        ui,
                        "Clean now",
                        widgets::primary_visual(),
                        138.0,
                        42.0,
                        !busy && files > 0,
                    )
                    .clicked();
                    ui.add_space(8.0);
                    rescan =
                        widgets::button(ui, "Rescan", widgets::ghost_visual(), 96.0, 36.0, !busy)
                            .clicked();
                });
            });
        });

        if clean {
            if let Some(scan) = scan.as_ref() {
                self.jobs.send(Job::Clean(Box::new(scan.as_ref().clone())));
            }
        }
        if rescan {
            self.scanning = true;
            self.jobs.scan();
        }

        ui.add_space(12.0);

        if let Some(scan) = scan.as_ref() {
            for target in &scan.targets {
                target_row(ui, target);
                ui.add_space(8.0);
            }
        }

        if let Some(report) = self.cleanup_report.clone() {
            widgets::card(ui, |ui| {
                widgets::section_title(ui, "Last run", None);
                ui.add_space(10.0);
                ui.label(
                    RichText::new(format!(
                        "Removed {} file{} · freed {}",
                        report.files,
                        if report.files == 1 { "" } else { "s" },
                        format::bytes(report.bytes)
                    ))
                    .font(theme::body(12.0))
                    .color(theme::TEXT),
                );
                if report.failed > 0 {
                    ui.add_space(6.0);
                    ui.label(
                        RichText::new(format!(
                            "{} file{} could not be removed because Roblox was using {}.",
                            report.failed,
                            if report.failed == 1 { "" } else { "s" },
                            if report.failed == 1 { "it" } else { "them" }
                        ))
                        .font(theme::body(11.0))
                        .color(theme::WARN),
                    );
                    for sample in report.samples.iter().take(3) {
                        ui.label(
                            RichText::new(sample)
                                .font(egui::FontId::new(10.5, egui::FontFamily::Monospace))
                                .color(theme::DIM),
                        );
                    }
                }
            });
            ui.add_space(12.0);
        }

        if let Some(last) = self.saved.last_cleanup.clone() {
            widgets::card(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.vertical(|ui| {
                        widgets::section_title(ui, "Cleanup history", None);
                        ui.add_space(3.0);
                        ui.label(
                            RichText::new(format!(
                                "Last cleanup {} · {} freed",
                                ago(last.at_unix),
                                format::bytes(last.bytes)
                            ))
                            .font(theme::body(11.5))
                            .color(theme::MUTED),
                        );
                    });
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        widgets::pill(
                            ui,
                            &format!("{} TOTAL", format::bytes(self.saved.lifetime_cleaned_bytes)),
                            theme::ACCENT_BRIGHT,
                        );
                    });
                });
            });
            ui.add_space(12.0);
        }

        widgets::card(ui, |ui| {
            widgets::section_title(ui, "What Volt cleans — and what it never touches", None);
            ui.add_space(10.0);
            bullet(
                ui,
                true,
                "Roblox Player session logs, MicroProfiler captures and crash dumps older than two minutes.",
            );
            bullet(
                ui,
                true,
                "Leftover Roblox entries in %TEMP%; anything in use is skipped, never forced.",
            );
            bullet(
                ui,
                false,
                "Installed client versions, assets, game content, settings and account data.",
            );
            bullet(
                ui,
                false,
                "Roblox game files, anti-cheat components or Windows services.",
            );
        });
    }
}

fn target_row(ui: &mut Ui, target: &TargetScan) {
    widgets::soft_card(ui, |ui| {
        ui.horizontal(|ui| {
            let width = (ui.available_width() - 130.0).max(150.0);
            ui.allocate_ui_with_layout(vec2(width, 0.0), Layout::top_down(Align::Min), |ui| {
                ui.set_width(width);
                ui.label(
                    RichText::new(target.label)
                        .font(theme::medium(12.5))
                        .color(theme::TEXT),
                );
                ui.add_space(1.0);
                ui.label(
                    RichText::new(target.summary)
                        .font(theme::body(11.0))
                        .color(theme::MUTED),
                );
                if target.skipped > 0 {
                    ui.add_space(2.0);
                    ui.label(
                        RichText::new(format!(
                            "{} file{} skipped (in use or too recent)",
                            target.skipped,
                            if target.skipped == 1 { "" } else { "s" }
                        ))
                        .font(theme::body(10.5))
                        .color(theme::DIM),
                    );
                }
            });
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                ui.label(
                    RichText::new(format::bytes(target.bytes()))
                        .font(theme::semibold(15.0))
                        .color(if target.bytes() > 0 {
                            theme::TEXT
                        } else {
                            theme::DIM
                        }),
                );
                ui.add_space(8.0);
                ui.label(
                    RichText::new(format!(
                        "{} file{}",
                        target.count(),
                        if target.count() == 1 { "" } else { "s" }
                    ))
                    .font(theme::body(10.5))
                    .color(theme::DIM),
                );
            });
        });
    });
}

fn bullet(ui: &mut Ui, positive: bool, text: &str) {
    ui.horizontal(|ui| {
        let (rect, _) = ui.allocate_exact_size(Vec2::splat(14.0), Sense::hover());
        icon::paint(
            ui.painter(),
            rect,
            if positive { Glyph::Check } else { Glyph::Close },
            if positive {
                theme::SUCCESS
            } else {
                theme::DANGER
            },
            1.5,
        );
        ui.add_space(6.0);
        ui.label(
            RichText::new(text)
                .font(theme::body(11.5))
                .color(theme::MUTED),
        );
    });
    ui.add_space(6.0);
}

fn legend(ui: &mut Ui, label: &str, color: Color32, value: String) {
    ui.horizontal(|ui| {
        let (rect, _) = ui.allocate_exact_size(Vec2::splat(9.0), Sense::hover());
        widgets::dot(ui.painter(), rect.center(), 3.0, color);
        ui.add_space(2.0);
        ui.label(
            RichText::new(label)
                .font(theme::body(10.5))
                .color(theme::DIM),
        );
        ui.label(
            RichText::new(value)
                .font(theme::medium(12.0))
                .color(theme::TEXT),
        );
    });
}

fn shorten(text: &str, limit: usize) -> String {
    let length = text.chars().count();
    if length <= limit {
        return text.to_owned();
    }
    let tail: String = text.chars().skip(length - (limit - 1)).collect();
    format!("…{tail}")
}

fn ago(unix: u64) -> String {
    let now = volt_core::now_unix();
    if unix == 0 || now <= unix {
        return "just now".to_owned();
    }
    let elapsed = now - unix;
    if elapsed < 60 {
        "just now".to_owned()
    } else if elapsed < 3_600 {
        format!("{}m ago", elapsed / 60)
    } else {
        format!("{}h ago", elapsed / 3_600)
    }
}
