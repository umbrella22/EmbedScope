use super::*;
use escope_host::protocol::capability;

impl Inspector {
    pub(super) fn log_page(&mut self, root: &mut egui::Ui) {
        let p = Palette::new(self.dark);
        egui::CentralPanel::default()
            .frame(theme::panel(p).inner_margin(20))
            .show(root, |ui| {
                self.log_contents(ui, false);
            });
    }

    pub(super) fn log_contents(&mut self, ui: &mut egui::Ui, compact: bool) {
        let p = Palette::new(self.dark);
        ui.horizontal(|ui| {
            ui.strong(if compact {
                tr!(self.i18n, "nav-logs")
            } else {
                tr!(self.i18n, "events")
            });
            theme::badge(ui, format!("{} / 512", self.logs.len()), p.muted);
            if self.info.is_none() && !self.logs.is_empty() {
                theme::badge(ui, tr!(self.i18n, "logs-retained"), p.warning);
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if theme::button(ui, Icon::Close, tr!(self.i18n, "clear"), ButtonStyle::Quiet)
                    .on_hover_text(tr!(self.i18n, "clear-hint"))
                    .clicked()
                {
                    self.logs.clear();
                }
                ui.checkbox(&mut self.follow_logs, tr!(self.i18n, "follow"))
                    .on_hover_text(tr!(self.i18n, "follow-hint"));
                egui::ComboBox::from_id_salt(if compact {
                    "drawer-level"
                } else {
                    "page-level"
                })
                .selected_text(level_filter(&self.i18n, self.log_level))
                .width(90.0)
                .show_ui(ui, |ui| {
                    for value in (1..=5).rev() {
                        ui.selectable_value(
                            &mut self.log_level,
                            value,
                            level_filter(&self.i18n, value),
                        );
                    }
                });
                let width = (ui.available_width() - 12.0).clamp(100.0, 240.0);
                ui.add(
                    egui::TextEdit::singleline(&mut self.log_query)
                        .desired_width(width)
                        .hint_text(tr!(self.i18n, "log-filter"))
                        .margin(egui::vec2(8.0, 7.0)),
                );
            });
        });
        if self.host_skipped_logs > 0 {
            ui.colored_label(
                p.warning,
                tr!(
                    self.i18n,
                    "host-skipped",
                    count = self.host_skipped_logs.to_string()
                ),
            );
        }
        ui.add_space(4.0);
        ui.separator();
        ui.add_space(4.0);
        let query = self.log_query.trim().to_lowercase();
        egui::ScrollArea::vertical()
            .id_salt(if compact {
                "drawer-events"
            } else {
                "page-events"
            })
            .stick_to_bottom(self.follow_logs)
            .auto_shrink([false, false])
            .show(ui, |ui| {
                let mut shown = 0;
                for log in &self.logs {
                    if self.log_level < 5 && log.level > self.log_level {
                        continue;
                    }
                    if !query.is_empty()
                        && !format!("{} {}", log.target, log.message)
                            .to_lowercase()
                            .contains(&query)
                    {
                        continue;
                    }
                    log_row(ui, log, compact, shown % 2 == 0, p, &self.i18n);
                    shown += 1;
                }
                if shown == 0 {
                    self.log_empty_state(ui, compact, p);
                }
            });
    }

    fn log_empty_state(&self, ui: &mut egui::Ui, compact: bool, p: Palette) {
        ui.add_space(if compact { 3.0 } else { 65.0 });
        if compact {
            let message = if self.logs.is_empty() {
                tr!(self.i18n, "logs-waiting")
            } else {
                tr!(self.i18n, "logs-no-match-hint")
            };
            ui.label(RichText::new(message).size(12.0).color(p.muted));
        } else {
            let title = if self.logs.is_empty() {
                tr!(self.i18n, "logs-empty")
            } else {
                tr!(self.i18n, "logs-no-match")
            };
            let message = if self.info.is_none() {
                tr!(self.i18n, "logs-connect")
            } else if !self.supports(capability::LOGS) {
                tr!(self.i18n, "logs-unsupported")
            } else if self.logs.is_empty() {
                tr!(self.i18n, "logs-collecting")
            } else {
                tr!(self.i18n, "logs-filter-help")
            };
            theme::empty(ui, Icon::Logs, title, message);
        }
    }

    pub(super) fn metrics_page(&mut self, root: &mut egui::Ui) {
        let p = Palette::new(self.dark);
        egui::CentralPanel::default()
            .frame(egui::Frame::new().fill(p.background).inner_margin(24))
            .show(root, |ui| {
                egui::ScrollArea::vertical().show(ui, |ui| {
                    if let Some(metrics) = &self.metrics {
                        metrics_snapshot(ui, metrics, p, &self.i18n);
                    } else {
                        ui.add_space(90.0);
                        let message = if self.info.is_none() {
                            tr!(self.i18n, "metrics-connect")
                        } else if !self.supports(capability::METRICS) {
                            tr!(self.i18n, "metrics-unsupported")
                        } else {
                            tr!(self.i18n, "metrics-refresh")
                        };
                        theme::empty(ui, Icon::Metrics, tr!(self.i18n, "metrics-empty"), message);
                    }
                });
            });
    }

    pub(super) fn device_page(&mut self, root: &mut egui::Ui) {
        let p = Palette::new(self.dark);
        egui::CentralPanel::default()
            .frame(egui::Frame::new().fill(p.background).inner_margin(24))
            .show(root, |ui| {
                egui::ScrollArea::vertical().show(ui, |ui| {
                    if let Some(info) = &self.info {
                        ui.horizontal(|ui| {
                            ui.heading(&info.name);
                            theme::badge(ui, tr!(self.i18n, "connected"), p.accent);
                        });
                        ui.label(RichText::new(&info.model).color(p.muted));
                        ui.add_space(24.0);
                        device_identity(
                            ui,
                            info,
                            &self.endpoint,
                            self.hello.as_ref(),
                            p,
                            &self.i18n,
                        );
                        ui.add_space(24.0);
                        self.device_capabilities(ui, p);
                    } else {
                        disconnected_device(ui, p, &self.i18n);
                    }
                });
            });
    }

    fn device_capabilities(&self, ui: &mut egui::Ui, p: Palette) {
        theme::section(ui, tr!(self.i18n, "capabilities"));
        for (bit, title, description) in [
            (
                capability::UI_TREE,
                tr!(self.i18n, "capability-tree"),
                tr!(self.i18n, "capability-tree-help"),
            ),
            (
                capability::CAPTURE,
                tr!(self.i18n, "capability-capture"),
                tr!(self.i18n, "capability-capture-help"),
            ),
            (
                capability::INPUT,
                tr!(self.i18n, "capability-input"),
                tr!(self.i18n, "capability-input-help"),
            ),
            (
                capability::LOGS,
                tr!(self.i18n, "capability-logs"),
                tr!(self.i18n, "capability-logs-help"),
            ),
            (
                capability::METRICS,
                tr!(self.i18n, "nav-metrics"),
                tr!(self.i18n, "capability-metrics-help"),
            ),
            (
                capability::CRASH,
                tr!(self.i18n, "capability-crash"),
                tr!(self.i18n, "capability-crash-help"),
            ),
        ] {
            capability_row(ui, self.supports(bit), title, description, p, &self.i18n);
            ui.add_space(6.0);
            ui.separator();
        }
    }
}

fn log_row(
    ui: &mut egui::Ui,
    log: &LogRecord,
    compact: bool,
    shaded: bool,
    p: Palette,
    i18n: &I18n,
) {
    let color = match log.level {
        1 => p.error,
        2 => p.warning,
        3 => p.accent,
        _ => p.muted,
    };
    let background = if shaded {
        p.raised.gamma_multiply(0.35)
    } else {
        egui::Color32::TRANSPARENT
    };
    egui::Frame::new()
        .fill(background)
        .inner_margin(egui::Margin::symmetric(6, 6))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal_top(|ui| {
                ui.add_sized(
                    [88.0, 16.0],
                    egui::Label::new(
                        RichText::new(format!("{} ms", log.timestamp_ms))
                            .monospace()
                            .size(11.0)
                            .color(p.muted),
                    ),
                );
                ui.add_sized(
                    [44.0, 16.0],
                    egui::Label::new(
                        RichText::new(level_name(log.level))
                            .monospace()
                            .size(10.0)
                            .color(color),
                    ),
                );
                let target_width = if compact { 150.0 } else { 190.0 };
                ui.add_sized(
                    [target_width, 16.0],
                    egui::Label::new(
                        RichText::new(&log.target)
                            .monospace()
                            .size(11.0)
                            .color(p.muted),
                    )
                    .truncate(),
                )
                .on_hover_text(&log.target);
                ui.vertical(|ui| {
                    ui.add(egui::Label::new(RichText::new(&log.message).size(12.0)).wrap());
                    log_loss_markers(ui, log, p, i18n);
                });
            });
        });
}

fn log_loss_markers(ui: &mut egui::Ui, log: &LogRecord, p: Palette, i18n: &I18n) {
    if log.dropped > 0 || log.truncated {
        ui.horizontal_wrapped(|ui| {
            if log.dropped > 0 {
                ui.label(
                    RichText::new(tr!(i18n, "device-dropped", count = log.dropped.to_string()))
                        .size(10.0)
                        .color(p.warning),
                );
            }
            if log.truncated {
                ui.label(
                    RichText::new(tr!(i18n, "log-truncated"))
                        .size(10.0)
                        .color(p.warning),
                );
            }
        });
    }
}

fn metrics_snapshot(ui: &mut egui::Ui, metrics: &Metrics, p: Palette, i18n: &I18n) {
    theme::section(ui, tr!(i18n, "latest-snapshot"));
    ui.columns(3, |columns| {
        for (column, (label, value, caption)) in columns.iter_mut().zip([
            (
                tr!(i18n, "internal-heap"),
                super::bytes(i18n, metrics.heap_free),
                tr!(i18n, "currently-free"),
            ),
            (
                tr!(i18n, "psram"),
                super::bytes(i18n, metrics.psram_free),
                tr!(i18n, "external-memory-free"),
            ),
            (
                tr!(i18n, "frame-duration"),
                metrics
                    .frame_ms
                    .map_or(tr!(i18n, "unavailable"), |v| format!("{v} ms")),
                tr!(i18n, "backend-reported"),
            ),
        ]) {
            metric_tile(column, label, value, caption, p);
        }
    });
    ui.add_space(26.0);
    ui.heading(tr!(i18n, "runtime-measurements"));
    ui.add_space(6.0);
    ui.label(RichText::new(tr!(i18n, "metrics-help")).color(p.muted));
    ui.add_space(16.0);
    for (label, value, description) in [
        (
            tr!(i18n, "uptime"),
            format!("{:.1} s", metrics.uptime_ms as f64 / 1000.0),
            tr!(i18n, "uptime-help"),
        ),
        (
            tr!(i18n, "heap-free"),
            super::bytes(i18n, metrics.heap_free),
            tr!(i18n, "heap-free-help"),
        ),
        (
            tr!(i18n, "heap-min-free"),
            super::bytes(i18n, metrics.heap_min_free),
            tr!(i18n, "heap-min-free-help"),
        ),
        (
            tr!(i18n, "heap-largest-free"),
            super::bytes(i18n, metrics.heap_largest_free),
            tr!(i18n, "heap-largest-free-help"),
        ),
        (
            tr!(i18n, "psram-free"),
            super::bytes(i18n, metrics.psram_free),
            tr!(i18n, "psram-free-help"),
        ),
        (
            tr!(i18n, "frame-duration"),
            metrics
                .frame_ms
                .map_or(tr!(i18n, "unavailable"), |v| format!("{v} ms")),
            tr!(i18n, "frame-duration-help"),
        ),
        (
            tr!(i18n, "queue-depth"),
            metrics
                .queue_depth
                .map_or(tr!(i18n, "unavailable"), |v| v.to_string()),
            tr!(i18n, "queue-depth-help"),
        ),
        (
            tr!(i18n, "dropped-logs"),
            metrics.dropped_logs.to_string(),
            tr!(i18n, "dropped-logs-help"),
        ),
    ] {
        metric_row(ui, label, value, description, p);
        ui.separator();
    }
}

fn metric_tile(
    ui: &mut egui::Ui,
    label: impl AsRef<str>,
    value: String,
    caption: impl AsRef<str>,
    p: Palette,
) {
    let label = label.as_ref();
    let caption = caption.as_ref();
    egui::Frame::new()
        .fill(p.panel)
        .stroke(egui::Stroke::new(1.0_f32, p.border))
        .corner_radius(8)
        .inner_margin(18)
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.label(RichText::new(label).size(12.0).color(p.muted));
            ui.add_space(8.0);
            ui.label(RichText::new(value).monospace().size(22.0));
            ui.add_space(4.0);
            ui.label(RichText::new(caption).size(11.0).color(p.muted));
        });
}

fn metric_row(
    ui: &mut egui::Ui,
    label: impl AsRef<str>,
    value: String,
    description: impl AsRef<str>,
    p: Palette,
) {
    let label = label.as_ref();
    let description = description.as_ref();
    ui.horizontal_wrapped(|ui| {
        fixed_width_label(ui, [160.0, 24.0], label);
        fixed_width_label(ui, [140.0, 24.0], RichText::new(value).monospace());
        ui.label(RichText::new(description).size(12.0).color(p.muted));
    });
}

fn device_identity(
    ui: &mut egui::Ui,
    info: &DeviceInfo,
    endpoint: &str,
    hello: Option<&Hello>,
    p: Palette,
    i18n: &I18n,
) {
    theme::section(ui, tr!(i18n, "identity"));
    egui::Frame::new()
        .fill(p.panel)
        .corner_radius(8)
        .inner_margin(18)
        .show(ui, |ui| {
            for (label, value) in [
                (tr!(i18n, "endpoint"), endpoint.to_owned()),
                (tr!(i18n, "firmware"), info.firmware.clone()),
                (tr!(i18n, "build"), info.build_id.clone()),
                (tr!(i18n, "boot-id"), info.boot_id.to_string()),
            ] {
                identity_row(ui, label, value, p);
            }
            if let Some(hello) = hello {
                ui.separator();
                identity_row(ui, tr!(i18n, "protocol"), hello.protocol.to_string(), p);
                identity_row(ui, tr!(i18n, "session"), hello.session.to_string(), p);
                identity_row(
                    ui,
                    tr!(i18n, "max-payload"),
                    tr!(i18n, "payload-bytes", count = hello.max_payload),
                    p,
                );
            }
        });
}

fn identity_row(ui: &mut egui::Ui, label: impl AsRef<str>, value: String, p: Palette) {
    let label = label.as_ref();
    ui.horizontal_top(|ui| {
        fixed_width_label(ui, [132.0, 20.0], RichText::new(label).color(p.muted));
        ui.add(egui::Label::new(RichText::new(value).monospace()).wrap());
    });
}

fn fixed_width_label(ui: &mut egui::Ui, size: [f32; 2], text: impl Into<egui::WidgetText>) {
    ui.allocate_ui_with_layout(
        size.into(),
        egui::Layout::left_to_right(egui::Align::Center),
        |ui| {
            ui.set_min_size(size.into());
            ui.label(text);
        },
    );
}

fn capability_row(
    ui: &mut egui::Ui,
    enabled: bool,
    title: String,
    description: String,
    p: Palette,
    i18n: &I18n,
) {
    ui.horizontal(|ui| {
        let (rect, _) = ui.allocate_exact_size(egui::Vec2::splat(22.0), egui::Sense::hover());
        if enabled {
            icons::paint(ui.painter(), rect.shrink(2.0), Icon::Check, p.accent);
        } else {
            ui.painter().line_segment(
                [
                    rect.left_center() + egui::vec2(6.0, 0.0),
                    rect.right_center() - egui::vec2(6.0, 0.0),
                ],
                egui::Stroke::new(1.5_f32, p.muted),
            );
        }
        ui.vertical(|ui| {
            ui.label(
                RichText::new(title)
                    .strong()
                    .color(if enabled { p.text } else { p.muted }),
            );
            ui.label(RichText::new(description).size(12.0).color(p.muted));
        });
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.label(
                RichText::new(if enabled {
                    tr!(i18n, "available")
                } else {
                    tr!(i18n, "not-advertised")
                })
                .size(11.0)
                .color(if enabled { p.accent } else { p.muted }),
            );
        });
    });
}

fn disconnected_device(ui: &mut egui::Ui, p: Palette, i18n: &I18n) {
    ui.add_space(70.0);
    theme::empty(
        ui,
        Icon::Connect,
        tr!(i18n, "connect-device"),
        tr!(i18n, "connect-device-help"),
    );
    ui.add_space(22.0);
    ui.vertical_centered(|ui| {
        ui.code("escope simulate");
        ui.label(RichText::new(tr!(i18n, "simulate-help")).color(p.muted));
        ui.add_space(14.0);
        ui.hyperlink_to(tr!(i18n, "connection-guide"), i18n.docs_url("connection"));
    });
}

fn level_name(level: u8) -> &'static str {
    match level {
        1 => "ERROR",
        2 => "WARN",
        3 => "INFO",
        4 => "DEBUG",
        5 => "TRACE",
        _ => "OTHER",
    }
}

fn level_filter(i18n: &I18n, level: u8) -> String {
    match level {
        1 => tr!(i18n, "level-error"),
        2 => tr!(i18n, "level-warn"),
        3 => tr!(i18n, "level-info"),
        4 => tr!(i18n, "level-debug"),
        _ => tr!(i18n, "level-all"),
    }
}
