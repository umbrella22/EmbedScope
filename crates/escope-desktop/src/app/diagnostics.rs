use super::*;
use escope_host::protocol::capability;

impl Inspector {
    pub(super) fn log_page(&mut self, ctx: &egui::Context) {
        let p = Palette::new(self.dark);
        egui::CentralPanel::default()
            .frame(theme::panel(p).inner_margin(20))
            .show(ctx, |ui| {
                self.log_contents(ui, false);
            });
    }

    pub(super) fn log_contents(&mut self, ui: &mut egui::Ui, compact: bool) {
        let p = Palette::new(self.dark);
        ui.horizontal(|ui| {
            ui.strong(if compact { "Event logs" } else { "Events" });
            theme::badge(ui, &format!("{} / 512", self.logs.len()), p.muted);
            if self.info.is_none() && !self.logs.is_empty() {
                theme::badge(ui, "Retained · disconnected", p.warning);
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if theme::button(ui, Icon::Close, "Clear", ButtonStyle::Quiet)
                    .on_hover_text("Clear the local event buffer. Device logging continues.")
                    .clicked()
                {
                    self.logs.clear();
                }
                ui.checkbox(&mut self.follow_logs, "Follow").on_hover_text(
                    "Keep the newest matching event visible. Collection continues when unchecked.",
                );
                egui::ComboBox::from_id_salt(if compact {
                    "drawer-level"
                } else {
                    "page-level"
                })
                .selected_text(if self.log_level == 5 {
                    "All levels"
                } else {
                    level_name(self.log_level)
                })
                .width(90.0)
                .show_ui(ui, |ui| {
                    for (value, label) in [
                        (5, "All levels"),
                        (4, "Debug +"),
                        (3, "Info +"),
                        (2, "Warn +"),
                        (1, "Error"),
                    ] {
                        ui.selectable_value(&mut self.log_level, value, label);
                    }
                });
                let width = (ui.available_width() - 12.0).clamp(100.0, 240.0);
                ui.add(
                    egui::TextEdit::singleline(&mut self.log_query)
                        .desired_width(width)
                        .hint_text("Filter target or message")
                        .margin(egui::vec2(8.0, 7.0)),
                );
            });
        });
        if self.host_skipped_logs > 0 {
            ui.colored_label(
                p.warning,
                format!(
                    "Host skipped {} events in this session. Device loss is reported separately on each record.",
                    self.host_skipped_logs
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
                    log_row(ui, log, compact, shown % 2 == 0, p);
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
                "Waiting for events. Logs will appear here once the Agent emits them."
            } else {
                "No events match these filters."
            };
            ui.label(RichText::new(message).size(12.0).color(p.muted));
        } else {
            let title = if self.logs.is_empty() {
                "No events yet"
            } else {
                "No matching events"
            };
            let message = if self.info.is_none() {
                "Connect an Agent to follow its structured event stream."
            } else if !self.supports(capability::LOGS) {
                "This Agent does not advertise structured logs."
            } else if self.logs.is_empty() {
                "Events appear as the Agent emits them. The latest 512 are retained locally."
            } else {
                "Try another filter or severity level."
            };
            theme::empty(ui, Icon::Logs, title, message);
        }
    }

    pub(super) fn metrics_page(&mut self, ctx: &egui::Context) {
        let p = Palette::new(self.dark);
        egui::CentralPanel::default()
            .frame(egui::Frame::new().fill(p.background).inner_margin(24))
            .show(ctx, |ui| {
                egui::ScrollArea::vertical().show(ui, |ui| {
                    if let Some(metrics) = &self.metrics {
                        metrics_snapshot(ui, metrics, p);
                    } else {
                        ui.add_space(90.0);
                        let message = if self.info.is_none() {
                            "Connect a device to inspect its memory and runtime measurements."
                        } else if !self.supports(capability::METRICS) {
                            "This Agent does not advertise metrics."
                        } else {
                            "Refresh to read the latest backend measurements."
                        };
                        theme::empty(ui, Icon::Metrics, "No metrics snapshot", message);
                    }
                });
            });
    }

    pub(super) fn device_page(&mut self, ctx: &egui::Context) {
        let p = Palette::new(self.dark);
        egui::CentralPanel::default()
            .frame(egui::Frame::new().fill(p.background).inner_margin(24))
            .show(ctx, |ui| {
                egui::ScrollArea::vertical().show(ui, |ui| {
                    if let Some(info) = &self.info {
                        ui.horizontal(|ui| {
                            ui.heading(&info.name);
                            theme::badge(ui, "Connected", p.accent);
                        });
                        ui.label(RichText::new(&info.model).color(p.muted));
                        ui.add_space(24.0);
                        device_identity(ui, info, &self.endpoint, self.hello.as_ref(), p);
                        ui.add_space(24.0);
                        self.device_capabilities(ui, p);
                    } else {
                        disconnected_device(ui, p);
                    }
                });
            });
    }

    fn device_capabilities(&self, ui: &mut egui::Ui, p: Palette) {
        theme::section(ui, "ADVERTISED CAPABILITIES");
        for (bit, title, description) in [
            (
                capability::UI_TREE,
                "UI tree",
                "Inspect object hierarchy, attributes and geometry",
            ),
            (
                capability::CAPTURE,
                "Display capture",
                "Read a fixed frame of display pixels",
            ),
            (
                capability::INPUT,
                "Input control",
                "Send explicit taps through the platform input route",
            ),
            (
                capability::LOGS,
                "Structured logs",
                "Subscribe to device events with severity and target",
            ),
            (
                capability::METRICS,
                "System metrics",
                "Read allocator and runtime measurements",
            ),
            (
                capability::CRASH,
                "Crash summary",
                "Read a retained crash summary with available attribution",
            ),
        ] {
            capability_row(ui, self.supports(bit), title, description, p);
            ui.add_space(6.0);
            ui.separator();
        }
    }
}

fn log_row(ui: &mut egui::Ui, log: &LogRecord, compact: bool, shaded: bool, p: Palette) {
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
                    log_loss_markers(ui, log, p);
                });
            });
        });
}

fn log_loss_markers(ui: &mut egui::Ui, log: &LogRecord, p: Palette) {
    if log.dropped > 0 || log.truncated {
        ui.horizontal_wrapped(|ui| {
            if log.dropped > 0 {
                ui.label(
                    RichText::new(format!(
                        "Device dropped: {} (subscription total)",
                        log.dropped
                    ))
                    .size(10.0)
                    .color(p.warning),
                );
            }
            if log.truncated {
                ui.label(
                    RichText::new("Record fields truncated")
                        .size(10.0)
                        .color(p.warning),
                );
            }
        });
    }
}

fn metrics_snapshot(ui: &mut egui::Ui, metrics: &Metrics, p: Palette) {
    theme::section(ui, "LATEST SNAPSHOT");
    ui.columns(3, |columns| {
        for (column, (label, value, caption)) in columns.iter_mut().zip([
            (
                "Internal heap",
                super::bytes(metrics.heap_free),
                "Currently free",
            ),
            (
                "PSRAM",
                super::bytes(metrics.psram_free),
                "External memory free",
            ),
            (
                "Frame duration",
                metrics
                    .frame_ms
                    .map_or("Unavailable".into(), |v| format!("{v} ms")),
                "Reported by the backend",
            ),
        ]) {
            metric_tile(column, label, value, caption, p);
        }
    });
    ui.add_space(26.0);
    ui.heading("Runtime measurements");
    ui.add_space(6.0);
    ui.label(
        RichText::new(
            "Unavailable fields are not reported as zero. Refresh to update this snapshot.",
        )
        .color(p.muted),
    );
    ui.add_space(16.0);
    for (label, value, description) in [
        (
            "Uptime",
            format!("{:.1} s", metrics.uptime_ms as f64 / 1000.0),
            "Device time since boot",
        ),
        (
            "Free heap",
            super::bytes(metrics.heap_free),
            "Current internal allocator free bytes",
        ),
        (
            "Minimum free heap",
            super::bytes(metrics.heap_min_free),
            "Allocator low-water mark, when provided",
        ),
        (
            "Largest free block",
            super::bytes(metrics.heap_largest_free),
            "Largest contiguous internal allocation",
        ),
        (
            "PSRAM free",
            super::bytes(metrics.psram_free),
            "Current external-memory free bytes",
        ),
        (
            "Frame duration",
            metrics
                .frame_ms
                .map_or("Unavailable".into(), |v| format!("{v} ms")),
            "Backend-defined frame work duration",
        ),
        (
            "Queue depth",
            metrics
                .queue_depth
                .map_or("Unavailable".into(), |v| v.to_string()),
            "Pending backend work",
        ),
        (
            "Dropped logs",
            metrics.dropped_logs.to_string(),
            "Loss reported by the current backend",
        ),
    ] {
        metric_row(ui, label, value, description, p);
        ui.separator();
    }
}

fn metric_tile(ui: &mut egui::Ui, label: &str, value: String, caption: &str, p: Palette) {
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

fn metric_row(ui: &mut egui::Ui, label: &str, value: String, description: &str, p: Palette) {
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
) {
    theme::section(ui, "IDENTITY");
    egui::Frame::new()
        .fill(p.panel)
        .corner_radius(8)
        .inner_margin(18)
        .show(ui, |ui| {
            for (label, value) in [
                ("Endpoint", endpoint.to_owned()),
                ("Firmware", info.firmware.clone()),
                ("Build", info.build_id.clone()),
                ("Boot ID", info.boot_id.to_string()),
            ] {
                identity_row(ui, label, value, p);
            }
            if let Some(hello) = hello {
                ui.separator();
                identity_row(ui, "Protocol", hello.protocol.to_string(), p);
                identity_row(ui, "Session", hello.session.to_string(), p);
                identity_row(
                    ui,
                    "Maximum payload",
                    format!("{} bytes", hello.max_payload),
                    p,
                );
            }
        });
}

fn identity_row(ui: &mut egui::Ui, label: &str, value: String, p: Palette) {
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

fn capability_row(ui: &mut egui::Ui, enabled: bool, title: &str, description: &str, p: Palette) {
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
                    "Available"
                } else {
                    "Not advertised"
                })
                .size(11.0)
                .color(if enabled { p.accent } else { p.muted }),
            );
        });
    });
}

fn disconnected_device(ui: &mut egui::Ui, p: Palette) {
    ui.add_space(70.0);
    theme::empty(
        ui,
        Icon::Connect,
        "Connect your device",
        "Enter a TCP endpoint or serial path in the connection bar. Enable DTR when the diagnostic transport requires it.",
    );
    ui.add_space(22.0);
    ui.vertical_centered(|ui| {
        ui.code("escope simulate");
        ui.label(
            RichText::new(
                "Start the software reference device, then connect to tcp://127.0.0.1:9337.",
            )
            .color(p.muted),
        );
        ui.add_space(14.0);
        ui.hyperlink_to(
            "Read the connection guide ↗",
            "https://umbrella22.github.io/EmbedScope_website/guide/basic/connection.html",
        );
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
