//! System view: hardware, Windows build, privileges, the change ledger and the
//! diagnostics report.
//!
//! Everything on this page is read from Windows or from Volt's own state file.
//! The ledger exists so a user can see exactly what Restore defaults will undo
//! before pressing it.

use crate::app::{ToastTone, VoltApp};
use crate::theme;
use crate::widgets;
use eframe::egui::{self, vec2, Align, Color32, Layout, RichText, Ui};
use volt_core::format;
use volt_core::platform;
use volt_core::report;

impl VoltApp {
    pub(crate) fn system(&mut self, ui: &mut Ui) {
        self.hardware_card(ui);
        ui.add_space(12.0);
        self.privileges_card(ui);
        ui.add_space(12.0);
        self.records_card(ui);
        ui.add_space(12.0);
        self.diagnostics_card(ui);
    }

    fn hardware_card(&self, ui: &mut Ui) {
        let info = &self.snapshot.info;
        let system = &self.snapshot.system;

        widgets::card(ui, |ui| {
            card_header(
                ui,
                "This PC",
                "Read from Windows on this machine, not from a database",
                Some(("LIVE", theme::ACCENT_BRIGHT)),
            );
            ui.add_space(14.0);

            let rows = [
                ("Processor", reported(&info.cpu)),
                ("Graphics", reported(&info.gpu)),
                (
                    "Graphics driver",
                    if info.gpu_driver.is_empty() {
                        "Not reported".to_owned()
                    } else {
                        info.gpu_driver.clone()
                    },
                ),
                (
                    "Memory",
                    format!(
                        "{} · {} in use",
                        format::bytes(system.ram_total),
                        format::percent(system.ram_percent)
                    ),
                ),
                ("Windows", reported(&info.os)),
                (
                    "Logical processors",
                    if info.logical_cores == 0 {
                        "Not reported".to_owned()
                    } else {
                        info.logical_cores.to_string()
                    },
                ),
                ("Power plan", reported(&info.power_plan)),
                (
                    "Elevation",
                    if self.snapshot.elevated {
                        "Administrator".to_owned()
                    } else {
                        "Standard user".to_owned()
                    },
                ),
            ];

            let gap = 18.0;
            let width = (ui.available_width() - gap) / 2.0;
            for pair in rows.chunks(2) {
                ui.horizontal_top(|ui| {
                    ui.spacing_mut().item_spacing.x = gap;
                    for (label, value) in pair {
                        ui.allocate_ui_with_layout(
                            vec2(width, 0.0),
                            Layout::top_down(Align::Min),
                            |ui| {
                                ui.set_width(width);
                                widgets::key_value(ui, label, value, theme::TEXT);
                            },
                        );
                    }
                });
                ui.add_space(7.0);
            }
        });
    }

    fn privileges_card(&mut self, ui: &mut Ui) {
        let elevated = self.snapshot.elevated;
        let mut relaunch = false;

        widgets::card(ui, |ui| {
            card_header(
                ui,
                "Administrator access",
                "What the elevated token actually changes",
                Some(if elevated {
                    ("ADMINISTRATOR", theme::SUCCESS)
                } else {
                    ("STANDARD USER", theme::WARN)
                }),
            );
            ui.add_space(12.0);

            ui.label(
                RichText::new(if elevated {
                    "Volt started with administrator rights, so machine-wide settings such as the active power plan can be changed and restored without a second prompt. Roblox is still only read: no injection, no file edits, no anti-cheat interaction."
                } else {
                    "Volt is running without administrator rights. Roblox priority and your own Game Bar settings still work, but Windows may refuse the power plan change and some background apps cannot be closed."
                })
                .font(theme::body(12.0))
                .color(theme::MUTED),
            );

            if !elevated {
                ui.add_space(12.0);
                widgets::soft_card(ui, |ui| {
                    ui.horizontal(|ui| {
                        let width = (ui.available_width() - 232.0).max(180.0);
                        ui.allocate_ui_with_layout(
                            vec2(width, 0.0),
                            Layout::top_down(Align::Min),
                            |ui| {
                                ui.set_width(width);
                                ui.label(
                                    RichText::new("Restart Volt as administrator")
                                        .font(theme::medium(12.5))
                                        .color(theme::TEXT),
                                );
                                ui.add_space(2.0);
                                ui.label(
                                    RichText::new(
                                        "Windows shows one User Account Control prompt. This window closes once the elevated copy starts, and every recorded change comes with it.",
                                    )
                                    .font(theme::body(11.0))
                                    .color(theme::MUTED),
                                );
                            },
                        );
                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                            relaunch = widgets::button(
                                ui,
                                "Relaunch elevated",
                                widgets::primary_visual(),
                                208.0,
                                36.0,
                                true,
                            )
                            .clicked();
                        });
                    });
                });
            }
        });

        if relaunch {
            match platform::relaunch_elevated() {
                Ok(()) => {
                    self.push_toast(
                        "Windows is asking for permission. Closing this window…".to_owned(),
                        ToastTone::Info,
                    );
                    ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close);
                }
                Err(err) => self.push_toast(err.to_string(), ToastTone::Error),
            }
        }
    }

    fn records_card(&mut self, ui: &mut Ui) {
        let backups = self.saved.backups.clone();
        let previous_power = self.saved.power_scheme.clone();
        let pids = self.saved.priority_pids.len();
        let total = backups.len() + usize::from(previous_power.is_some()) + pids;
        let busy = self.any_busy();
        let confirm = self.confirm_restore;

        let mut ask = false;
        let mut do_restore = false;
        let mut cancel = false;

        widgets::card(ui, |ui| {
            ui.horizontal(|ui| {
                ui.vertical(|ui| {
                    ui.label(
                        RichText::new("Recorded changes")
                            .font(theme::semibold(14.0))
                            .color(theme::TEXT),
                    );
                    ui.label(
                        RichText::new(
                            "Exactly what Restore defaults puts back, and where it lives",
                        )
                        .font(theme::body(10.5))
                        .color(theme::DIM),
                    );
                });
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    ask = widgets::button(
                        ui,
                        "Restore defaults",
                        widgets::ghost_visual(),
                        156.0,
                        34.0,
                        !busy && total > 0,
                    )
                    .clicked();
                    ui.add_space(8.0);
                    widgets::pill(
                        ui,
                        &format!("{total} RECORDED"),
                        if total > 0 { theme::WARN } else { theme::DIM },
                    );
                });
            });

            ui.add_space(12.0);

            if total == 0 {
                widgets::soft_card(ui, |ui| {
                    ui.label(
                        RichText::new(
                            "Nothing is recorded yet. Every change Volt makes is written here first, together with the value it replaced.",
                        )
                        .font(theme::body(12.0))
                        .color(theme::MUTED),
                    );
                });
                return;
            }

            for backup in &backups {
                let original = backup
                    .original
                    .map(|value| value.to_string())
                    .unwrap_or_else(|| "not set".to_owned());
                ledger_row(
                    ui,
                    &backup.name,
                    &format!("HKEY_CURRENT_USER\\{}", backup.path),
                    &format!("{original} → {}", backup.applied),
                );
            }

            if let Some(previous) = &previous_power {
                ledger_row(
                    ui,
                    "Active power plan",
                    "Machine-wide, restored through powrprof",
                    &format!("back to {previous}"),
                );
            }

            if pids > 0 {
                ledger_row(
                    ui,
                    "Roblox process priority",
                    &format!(
                        "{pids} process id{} recorded",
                        if pids == 1 { "" } else { "s" }
                    ),
                    "back to Normal",
                );
            }

            if confirm {
                ui.add_space(6.0);
                widgets::soft_card(ui, |ui| {
                    ui.horizontal(|ui| {
                        let width = (ui.available_width() - 262.0).max(170.0);
                        ui.allocate_ui_with_layout(
                            vec2(width, 0.0),
                            Layout::top_down(Align::Min),
                            |ui| {
                                ui.set_width(width);
                                ui.label(
                                    RichText::new("Undo every recorded change?")
                                        .font(theme::medium(12.5))
                                        .color(theme::TEXT),
                                );
                                ui.add_space(2.0);
                                ui.label(
                                    RichText::new(
                                        "Values that did not exist before Volt created them are removed again.",
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

        if ask {
            self.confirm_restore = true;
        }
        if do_restore {
            self.start_restore();
        }
        if cancel {
            self.confirm_restore = false;
        }
    }

    fn diagnostics_card(&mut self, ui: &mut Ui) {
        let busy = self.any_busy();
        let saved_path = self.report_path.clone();
        let mut copy = false;
        let mut save = false;
        let mut open = false;

        widgets::card(ui, |ui| {
            card_header(
                ui,
                "Diagnostics report",
                "Hardware, Roblox state, tweaks and the ledger as plain text",
                Some(("LOCAL ONLY", theme::ACCENT_BRIGHT)),
            );
            ui.add_space(12.0);
            ui.label(
                RichText::new(
                    "Useful when something behaves unexpectedly: it repeats exactly what this app shows and marks measured values as measured. Nothing is uploaded anywhere.",
                )
                .font(theme::body(12.0))
                .color(theme::MUTED),
            );
            ui.add_space(12.0);

            ui.horizontal(|ui| {
                save = widgets::button(
                    ui,
                    "Save report",
                    widgets::primary_visual(),
                    142.0,
                    34.0,
                    !busy,
                )
                .clicked();
                ui.add_space(8.0);
                copy = widgets::button(
                    ui,
                    "Copy to clipboard",
                    widgets::ghost_visual(),
                    152.0,
                    34.0,
                    true,
                )
                .clicked();
            });

            if let Some(path) = &saved_path {
                ui.add_space(12.0);
                ui.horizontal(|ui| {
                    let width = (ui.available_width() - 118.0).max(160.0);
                    ui.allocate_ui_with_layout(
                        vec2(width, 0.0),
                        Layout::top_down(Align::Min),
                        |ui| {
                            ui.set_width(width);
                            ui.label(
                                RichText::new("Last saved")
                                    .font(theme::body(10.5))
                                    .color(theme::DIM),
                            );
                            ui.label(
                                RichText::new(path.display().to_string())
                                    .font(egui::FontId::new(10.5, egui::FontFamily::Monospace))
                                    .color(theme::MUTED),
                            );
                        },
                    );
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        open = widgets::button(
                            ui,
                            "Open folder",
                            widgets::subtle_visual(),
                            104.0,
                            30.0,
                            true,
                        )
                        .clicked();
                    });
                });
            }
        });

        if copy {
            let text = report::build(&self.snapshot, &self.saved);
            ui.ctx().copy_text(text);
            self.push_toast(
                "Report copied to the clipboard.".to_owned(),
                ToastTone::Success,
            );
        }
        if save {
            self.save_report();
        }
        if open {
            if let Some(directory) = saved_path.as_ref().and_then(|path| path.parent()) {
                if let Err(err) = platform::open_folder(directory) {
                    self.push_toast(err.to_string(), ToastTone::Error);
                }
            }
        }
    }
}

fn card_header(ui: &mut Ui, title: &str, subtitle: &str, tag: Option<(&str, Color32)>) {
    ui.horizontal(|ui| {
        ui.vertical(|ui| {
            ui.label(
                RichText::new(title)
                    .font(theme::semibold(14.0))
                    .color(theme::TEXT),
            );
            ui.label(
                RichText::new(subtitle)
                    .font(theme::body(10.5))
                    .color(theme::DIM),
            );
        });
        if let Some((text, color)) = tag {
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                widgets::pill(ui, text, color);
            });
        }
    });
}

fn ledger_row(ui: &mut Ui, title: &str, detail: &str, meta: &str) {
    widgets::soft_card(ui, |ui| {
        ui.horizontal(|ui| {
            let width = (ui.available_width() - 132.0).max(150.0);
            ui.allocate_ui_with_layout(vec2(width, 0.0), Layout::top_down(Align::Min), |ui| {
                ui.set_width(width);
                ui.label(
                    RichText::new(title)
                        .font(theme::medium(12.0))
                        .color(theme::TEXT),
                );
                ui.add_space(1.0);
                ui.label(
                    RichText::new(detail)
                        .font(egui::FontId::new(10.5, egui::FontFamily::Monospace))
                        .color(theme::DIM),
                );
            });
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                ui.label(
                    RichText::new(meta)
                        .font(theme::medium(11.0))
                        .color(theme::MUTED),
                );
            });
        });
    });
    ui.add_space(6.0);
}

fn reported(value: &str) -> String {
    if value.is_empty() {
        "Not reported".to_owned()
    } else {
        value.to_owned()
    }
}
