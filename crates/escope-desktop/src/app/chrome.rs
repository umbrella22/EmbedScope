use super::*;

impl Inspector {
    pub(super) fn navigation(&mut self, root: &mut egui::Ui) {
        let p = Palette::new(self.dark);
        egui::Panel::left("navigation")
            .exact_size(64.0)
            .resizable(false)
            .frame(egui::Frame::new().fill(p.panel).inner_margin(12))
            .show(root, |ui| {
                let (logo, _) =
                    ui.allocate_exact_size(egui::vec2(40.0, 48.0), egui::Sense::hover());
                crate::brand::paint(ui.painter(), logo.center(), 30.0, p.accent);
                ui.add_space(16.0);
                for (view, icon, label) in [
                    (
                        View::Inspector,
                        Icon::Inspect,
                        tr!(self.i18n, "nav-inspector"),
                    ),
                    (View::Logs, Icon::Logs, tr!(self.i18n, "nav-logs")),
                    (View::Metrics, Icon::Metrics, tr!(self.i18n, "nav-metrics")),
                    (
                        View::Device,
                        Icon::Device,
                        tr!(self.i18n, "nav-device-hint"),
                    ),
                ] {
                    if theme::icon_button(ui, icon, label, self.view == view).clicked() {
                        self.view = view;
                    }
                    ui.add_space(4.0);
                }
                ui.with_layout(egui::Layout::bottom_up(egui::Align::Center), |ui| {
                    if theme::icon_button(
                        ui,
                        if self.dark { Icon::Sun } else { Icon::Moon },
                        if self.dark {
                            tr!(self.i18n, "theme-light")
                        } else {
                            tr!(self.i18n, "theme-dark")
                        },
                        false,
                    )
                    .clicked()
                    {
                        self.dark = !self.dark;
                        theme::apply(ui.ctx(), self.dark);
                    }
                    if theme::icon_button(ui, Icon::Book, tr!(self.i18n, "open-docs"), false)
                        .clicked()
                    {
                        ui.ctx()
                            .open_url(egui::OpenUrl::new_tab(self.i18n.docs_url("inspector")));
                    }
                    self.language_menu(ui);
                });
            });
    }

    fn language_menu(&mut self, ui: &mut egui::Ui) {
        let response =
            theme::icon_button(ui, Icon::Language, tr!(self.i18n, "language-menu"), false);
        egui::Popup::menu(&response)
            .style(theme::menu_style)
            .show(|ui| {
                ui.set_min_width(216.0);
                egui::Frame::new()
                    .inner_margin(egui::Margin::symmetric(12, 6))
                    .show(ui, |ui| {
                        ui.strong(tr!(self.i18n, "language-title"));
                    });
                ui.separator();
                let mut preference = self.i18n.preference();
                ui.selectable_value(
                    &mut preference,
                    Preference::System,
                    tr!(self.i18n, "language-system"),
                );
                for (index, locale) in LOCALES.iter().enumerate() {
                    ui.selectable_value(&mut preference, Preference::Language(index), locale.name);
                }
                ui.separator();
                egui::Frame::new()
                    .inner_margin(egui::Margin::symmetric(12, 6))
                    .show(ui, |ui| {
                        ui.add(
                            egui::Label::new(
                                RichText::new(tr!(
                                    self.i18n,
                                    "language-current",
                                    language = self.i18n.locale().name
                                ))
                                .small()
                                .weak(),
                            )
                            .wrap(),
                        );
                    });
                if preference != self.i18n.preference() {
                    self.i18n.set_preference(preference);
                    ui.ctx().request_repaint();
                }
            });
    }

    pub(super) fn header(&mut self, root: &mut egui::Ui) {
        let p = Palette::new(self.dark);
        egui::Panel::top("workspace-header")
            .frame(theme::panel(p).inner_margin(egui::Margin::symmetric(20, 14)))
            .show(root, |ui| {
                ui.horizontal(|ui| {
                    ui.label(RichText::new("EmbedScope").size(15.0).strong());
                    ui.label(RichText::new("/").color(p.border));
                    ui.label(RichText::new(tr!(self.i18n, "workspace")).color(p.muted));
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        let connected = self.info.is_some();
                        let connect = ui
                            .add_enabled_ui(connected || !self.busy, |ui| {
                                theme::button(
                                    ui,
                                    if connected {
                                        Icon::Disconnect
                                    } else {
                                        Icon::Connect
                                    },
                                    if connected {
                                        tr!(self.i18n, "disconnect")
                                    } else {
                                        tr!(self.i18n, "connect")
                                    },
                                    if connected {
                                        ButtonStyle::Quiet
                                    } else {
                                        ButtonStyle::Primary
                                    },
                                )
                            })
                            .inner;
                        if connect.clicked() {
                            self.error = None;
                            self.send(if connected {
                                Command::Disconnect
                            } else {
                                Command::Connect(self.endpoint.clone(), self.dtr)
                            });
                        }
                        ui.add_enabled(
                            !connected && !self.busy,
                            egui::Checkbox::new(&mut self.dtr, "DTR"),
                        )
                        .on_hover_text(tr!(self.i18n, "dtr-hint"));
                        let width = (ui.available_width() - 24.0).clamp(150.0, 320.0);
                        ui.add_enabled(
                            !connected && !self.busy,
                            theme::text_field(&mut self.endpoint)
                                .font(egui::TextStyle::Monospace)
                                .desired_width(width)
                                .hint_text(tr!(self.i18n, "endpoint-hint")),
                        );
                    });
                });
                ui.add_space(14.0);
                ui.horizontal(|ui| {
                    ui.vertical(|ui| {
                        ui.label(
                            RichText::new(self.view.title(&self.i18n))
                                .size(23.0)
                                .strong(),
                        );
                        ui.label(
                            RichText::new(self.view.description(&self.i18n))
                                .size(12.0)
                                .color(p.muted),
                        );
                    });
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if self.view == View::Inspector
                            && theme::button(
                                ui,
                                Icon::Logs,
                                tr!(self.i18n, "toggle-logs"),
                                if self.show_logs {
                                    ButtonStyle::Selected
                                } else {
                                    ButtonStyle::Quiet
                                },
                            )
                            .clicked()
                        {
                            self.show_logs = !self.show_logs;
                        }
                        ui.add_enabled_ui(self.info.is_some() && !self.busy, |ui| {
                            if theme::button(
                                ui,
                                if self.auto { Icon::Pause } else { Icon::Play },
                                tr!(self.i18n, "live"),
                                if self.auto {
                                    ButtonStyle::Selected
                                } else {
                                    ButtonStyle::Quiet
                                },
                            )
                            .on_hover_text(tr!(self.i18n, "live-hint"))
                            .clicked()
                            {
                                self.auto = !self.auto;
                                self.send(Command::Auto(self.auto));
                            }
                            if theme::button(
                                ui,
                                Icon::Refresh,
                                tr!(self.i18n, "refresh"),
                                ButtonStyle::Quiet,
                            )
                            .clicked()
                            {
                                self.send(Command::Refresh);
                            }
                        });
                        if self.busy {
                            ui.spinner();
                        }
                    });
                });
            });
        if let Some(error) = self.error.clone() {
            egui::Panel::top("connection-error")
                .frame(
                    egui::Frame::new()
                        .fill(p.error.gamma_multiply(0.08))
                        .inner_margin(egui::Margin::symmetric(20, 8)),
                )
                .show(root, |ui| {
                    ui.horizontal_wrapped(|ui| {
                        if ui.small_button(tr!(self.i18n, "dismiss")).clicked() {
                            self.error = None;
                        }
                        ui.colored_label(p.error, tr!(self.i18n, "error-context", error = error));
                    });
                });
        }
    }

    pub(super) fn status(&self, root: &mut egui::Ui) {
        let p = Palette::new(self.dark);
        egui::Panel::bottom("session-status")
            .exact_size(30.0)
            .frame(
                egui::Frame::new()
                    .fill(p.panel)
                    .inner_margin(egui::Margin::symmetric(16, 5)),
            )
            .show(root, |ui| {
                // This is a read-only status line inside a 30-point panel, not a control row.
                ui.spacing_mut().interact_size.y = 18.0;
                ui.horizontal(|ui| {
                    let (dot, _) =
                        ui.allocate_exact_size(egui::vec2(7.0, 14.0), egui::Sense::hover());
                    ui.painter().circle_filled(
                        dot.center(),
                        3.0,
                        if self.info.is_some() {
                            p.accent
                        } else {
                            p.muted
                        },
                    );
                    ui.label(
                        RichText::new(if self.info.is_some() {
                            tr!(self.i18n, "connected")
                        } else if self.busy {
                            tr!(self.i18n, "connecting")
                        } else {
                            tr!(self.i18n, "disconnected")
                        })
                        .size(11.0),
                    );
                    if let Some(info) = &self.info {
                        ui.label(RichText::new("/").color(p.border));
                        ui.add(
                            egui::Label::new(
                                RichText::new(format!("{} · {}", info.name, info.model))
                                    .size(11.0)
                                    .color(p.muted),
                            )
                            .truncate(),
                        );
                    }
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.label(
                            RichText::new(concat!("v", env!("CARGO_PKG_VERSION")))
                                .size(10.0)
                                .color(p.muted),
                        );
                        if let Some(hello) = &self.hello {
                            ui.label(
                                RichText::new(tr!(
                                    self.i18n,
                                    "protocol-version",
                                    version = hello.protocol
                                ))
                                .size(10.0)
                                .color(p.muted),
                            );
                        }
                        if let Some(last) = self.last_snapshot {
                            ui.label(
                                RichText::new(tr!(
                                    self.i18n,
                                    "snapshot-age",
                                    seconds = last.elapsed().as_secs()
                                ))
                                .size(10.0)
                                .color(p.muted),
                            );
                        }
                    });
                });
            });
    }
}
