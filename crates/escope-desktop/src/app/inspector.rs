use super::*;
use crate::png_save::SaveStatus;
use eframe::egui::{FontId, Stroke};

impl Inspector {
    pub(super) fn inspector(&mut self, ctx: &egui::Context) {
        let p = Palette::new(self.dark);
        if self.show_logs {
            egui::TopBottomPanel::bottom("log-drawer")
                .resizable(true)
                .default_height(160.0)
                .min_height(100.0)
                .max_height((ctx.available_rect().height() * 0.45).max(100.0))
                .frame(theme::panel(p).inner_margin(egui::Margin::symmetric(16, 10)))
                .show(ctx, |ui| {
                    self.log_contents(ui, true);
                });
        }
        self.object_tree(ctx);
        self.properties(ctx);
        self.display(ctx);
    }

    fn object_tree(&mut self, ctx: &egui::Context) {
        let p = Palette::new(self.dark);
        egui::SidePanel::left("object-tree")
            .resizable(true)
            .default_width(236.0)
            .width_range(170.0..=360.0)
            .frame(theme::panel(p))
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.strong("Objects");
                    theme::badge(ui, &self.nodes.len().to_string(), p.muted);
                });
                ui.add_space(8.0);
                ui.add(
                    egui::TextEdit::singleline(&mut self.tree_query)
                        .desired_width(f32::INFINITY)
                        .hint_text("Filter by type, name or ID")
                        .margin(egui::vec2(8.0, 8.0)),
                );
                ui.add_space(8.0);
                let query = self.tree_query.trim().to_lowercase();
                egui::ScrollArea::vertical()
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        let mut shown = 0;
                        for node in &self.nodes {
                            if !query.is_empty()
                                && !format!("{} {} #{}", node.kind, node.name, node.id)
                                    .to_lowercase()
                                    .contains(&query)
                            {
                                continue;
                            }
                            shown += 1;
                            let selected = self.selected == Some(node.id);
                            let (rect, response) = ui.allocate_exact_size(
                                egui::vec2(
                                    ui.available_width(),
                                    if node.name.is_empty() { 34.0 } else { 48.0 },
                                ),
                                egui::Sense::click(),
                            );
                            response.widget_info(|| {
                                egui::WidgetInfo::labeled(
                                    egui::WidgetType::SelectableLabel,
                                    true,
                                    format!("{} #{} {}", node.kind, node.id, node.name),
                                )
                            });
                            if selected || response.hovered() || response.has_focus() {
                                ui.painter().rect_filled(
                                    rect,
                                    6,
                                    if selected { p.accent_bg } else { p.raised },
                                );
                            }
                            if selected {
                                ui.painter().rect_filled(
                                    egui::Rect::from_min_size(
                                        rect.min + egui::vec2(0.0, 8.0),
                                        egui::vec2(2.0, rect.height() - 16.0),
                                    ),
                                    1,
                                    p.accent,
                                );
                            }
                            let depth = self.depths.get(&node.id).copied().unwrap_or(0);
                            let indent = (depth as f32 * 12.0).min((rect.width() - 100.0).max(0.0));
                            let left = rect.left() + 10.0 + indent;
                            let top = rect.top() + if node.name.is_empty() { 9.0 } else { 7.0 };
                            icons::paint(
                                ui.painter(),
                                egui::Rect::from_min_size(
                                    egui::pos2(left, top + 1.0),
                                    egui::Vec2::splat(14.0),
                                ),
                                Icon::Layers,
                                if selected { p.accent } else { p.muted },
                            );
                            let text_rect = egui::Rect::from_min_max(
                                egui::pos2(left + 22.0, top),
                                egui::pos2(rect.right() - 38.0, rect.bottom()),
                            );
                            let painter = ui
                                .painter()
                                .with_clip_rect(text_rect.intersect(ui.clip_rect()));
                            painter.text(
                                text_rect.min,
                                egui::Align2::LEFT_TOP,
                                &node.kind,
                                FontId::proportional(12.0),
                                if selected { p.accent } else { p.text },
                            );
                            if !node.name.is_empty() {
                                painter.text(
                                    text_rect.min + egui::vec2(0.0, 18.0),
                                    egui::Align2::LEFT_TOP,
                                    &node.name,
                                    FontId::proportional(11.0),
                                    p.muted,
                                );
                            }
                            ui.painter().text(
                                egui::pos2(rect.right() - 7.0, top + 1.0),
                                egui::Align2::RIGHT_TOP,
                                format!("#{}", node.id),
                                FontId::monospace(10.0),
                                p.muted,
                            );
                            if response.clicked() {
                                self.selected = Some(node.id);
                            }
                            response
                                .on_hover_cursor(egui::CursorIcon::PointingHand)
                                .on_hover_text(format!(
                                    "{} #{}\n{}\n{} × {} at {}, {}",
                                    node.kind,
                                    node.id,
                                    node.name,
                                    node.width,
                                    node.height,
                                    node.x,
                                    node.y
                                ));
                            ui.add_space(1.0);
                        }
                        if shown == 0 {
                            ui.add_space(24.0);
                            theme::empty(
                                ui,
                                Icon::Layers,
                                if self.nodes.is_empty() {
                                    "No objects yet"
                                } else {
                                    "No matching objects"
                                },
                                if self.info.is_none() {
                                    "Connect a device to explore its UI tree."
                                } else if !self.supports(escope_host::protocol::capability::UI_TREE)
                                {
                                    "This Agent does not expose a UI tree."
                                } else if !query.is_empty() {
                                    "Try another type, label or object ID."
                                } else {
                                    "Refresh to read the current UI tree."
                                },
                            );
                        }
                    });
            });
    }

    fn properties(&mut self, ctx: &egui::Context) {
        let p = Palette::new(self.dark);
        egui::SidePanel::right("object-properties")
            .resizable(true)
            .default_width(252.0)
            .width_range(210.0..=380.0)
            .frame(theme::panel(p))
            .show(ctx, |ui| {
                ui.strong("Properties");
                ui.add_space(14.0);
                egui::ScrollArea::vertical()
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        self.property_contents(ui, p);
                    });
            });
    }

    fn property_contents(&mut self, ui: &mut egui::Ui, p: Palette) {
        if let Some(node) = self
            .nodes
            .iter()
            .find(|node| Some(node.id) == self.selected)
        {
            object_properties(ui, node, p);
        } else {
            ui.add_space(18.0);
            theme::empty(ui, Icon::Inspect, "Inspect an object",
                "Choose a row in the tree or click the display to inspect its bounds and attributes.");
        }
        ui.add_space(24.0);
        ui.separator();
        ui.add_space(12.0);
        theme::section(ui, "DEVICE SNAPSHOT");
        if let Some(info) = &self.info {
            ui.label(RichText::new(&info.name).strong());
            ui.add(egui::Label::new(RichText::new(&info.model).small().color(p.muted)).wrap());
        } else {
            ui.label(RichText::new("No active session").color(p.muted));
        }
        if let Some(metrics) = &self.metrics {
            ui.add_space(10.0);
            property(ui, "Free heap", super::bytes(metrics.heap_free));
            property(
                ui,
                "Frame",
                metrics
                    .frame_ms
                    .map_or("Unavailable".into(), |v| format!("{v} ms")),
            );
            property(
                ui,
                "Queue depth",
                metrics
                    .queue_depth
                    .map_or("Unavailable".into(), |v| v.to_string()),
            );
        }
        ui.add_space(10.0);
        if theme::button(ui, Icon::Metrics, "All metrics", ButtonStyle::Quiet).clicked() {
            self.view = View::Metrics;
        }
    }

    fn display(&mut self, ctx: &egui::Context) {
        let p = Palette::new(self.dark);
        egui::CentralPanel::default()
            .frame(egui::Frame::new().fill(p.background).inner_margin(16))
            .show(ctx, |ui| self.display_contents(ui, ctx, p));
    }

    fn display_contents(&mut self, ui: &mut egui::Ui, ctx: &egui::Context, p: Palette) {
        self.display_modes(ui, p);
        ui.add_space(8.0);
        let stage_size = egui::vec2(
            ui.available_width(),
            (ui.available_height() - 104.0).max(40.0),
        );
        let (canvas, _) = ui.allocate_exact_size(stage_size, egui::Sense::hover());
        dotted_canvas(ui, canvas, p);
        if let (Some(texture), Some(capture)) = (&self.texture, &self.capture) {
            self.display_frame(ui, canvas, texture.id(), capture.meta, p);
        } else {
            self.display_empty(ui, canvas, p);
            ui.add_space(2.0);
            ui.label(RichText::new("No display frame").size(11.0).color(p.muted));
        }
        ui.add_space(6.0);
        self.export_controls(ui, ctx, p);
    }

    fn display_modes(&mut self, ui: &mut egui::Ui, p: Palette) {
        ui.horizontal_wrapped(|ui| {
            let inspect = if self.inject { ButtonStyle::Quiet } else { ButtonStyle::Selected };
            if theme::button(ui, Icon::Inspect, "Inspect", inspect)
                .on_hover_text("Click the display to select an object. No input is sent.").clicked() {
                self.inject = false;
            }
            ui.add_enabled_ui(self.supports(escope_host::protocol::capability::INPUT), |ui| {
                let mode = if self.inject { ButtonStyle::Warning } else { ButtonStyle::Quiet };
                if theme::button(ui, Icon::Tap, "Send taps", mode)
                    .on_hover_text("Send a 60 ms tap to the device, then refresh. This changes the device UI.").clicked() {
                    self.inject = true;
                }
            });
            if self.inject {
                ui.label(RichText::new("Input armed").small().color(p.warning));
            }
        });
    }

    fn display_frame(
        &mut self,
        ui: &mut egui::Ui,
        canvas: egui::Rect,
        texture: egui::TextureId,
        meta: escope_host::protocol::CaptureMeta,
        p: Palette,
    ) {
        let source = egui::vec2(meta.width as f32, meta.height as f32);
        let available = (canvas.size() - egui::Vec2::splat(40.0)).max(egui::Vec2::splat(1.0));
        let scale = (available.x / source.x)
            .min(available.y / source.y)
            .min(2.5);
        let image_rect = egui::Rect::from_center_size(canvas.center(), source * scale);
        let painter = ui.painter().with_clip_rect(canvas);
        painter.rect_filled(image_rect.expand(5.0), 5, p.panel);
        painter.rect_stroke(
            image_rect.expand(5.0),
            5,
            Stroke::new(1.0_f32, p.border),
            egui::StrokeKind::Inside,
        );
        let response = ui.place(
            image_rect,
            egui::Image::new((texture, source * scale)).sense(egui::Sense::click()),
        );
        if let Some(node) = self
            .nodes
            .iter()
            .find(|node| Some(node.id) == self.selected)
        {
            selection_outline(ui, response.rect, canvas, node, scale, p);
        }
        if response.clicked() && !self.busy {
            if let Some((x, y)) = response
                .interact_pointer_pos()
                .and_then(|pos| display_point(response.rect, source, pos))
            {
                if self.inject {
                    self.send(Command::Tap(x, y));
                } else {
                    self.selected = node_at(&self.nodes, x, y);
                }
            }
        }
        ui.add_space(2.0);
        ui.horizontal(|ui| {
            ui.label(
                RichText::new(format!("{} × {}", meta.width, meta.height))
                    .size(11.0)
                    .monospace()
                    .color(p.muted),
            );
            ui.label(
                RichText::new(format!("Frame {}", meta.frame_id))
                    .size(11.0)
                    .color(p.muted),
            );
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.label(
                    RichText::new(format!("{:.0}%", scale * 100.0))
                        .size(11.0)
                        .monospace()
                        .color(p.muted),
                );
            });
        });
    }

    fn display_empty(&self, ui: &mut egui::Ui, canvas: egui::Rect, p: Palette) {
        let connected = self.info.is_some();
        let (title, message) = if !connected {
            ("Your device, in focus.",
                "Connect an Agent to inspect its interface. For a local preview, start the software reference device.")
        } else if !self.supports(escope_host::protocol::capability::CAPTURE) {
            ("Display capture unavailable",
                "This Agent does not advertise display capture. Its other capabilities remain available.")
        } else {
            (
                "Waiting for a display snapshot",
                "Refresh to capture the current display output.",
            )
        };
        let mut child = ui.new_child(
            egui::UiBuilder::new()
                .max_rect(canvas.shrink(16.0))
                .layout(egui::Layout::top_down(egui::Align::Center)),
        );
        child.add_space((canvas.height() / 2.0 - 96.0).max(4.0));
        theme::empty(&mut child, Icon::Device, title, message);
        if !connected {
            child.add_space(16.0);
            child.code("escope simulate");
            child.label(
                RichText::new("Software reference · synthetic data")
                    .small()
                    .color(p.muted),
            );
        }
    }

    fn export_controls(&mut self, ui: &mut egui::Ui, ctx: &egui::Context, p: Palette) {
        ui.horizontal(|ui| {
            let width = (ui.available_width() - 115.0).max(40.0);
            ui.add(
                egui::TextEdit::singleline(&mut self.save_path)
                    .desired_width(width)
                    .font(egui::TextStyle::Monospace)
                    .hint_text("New PNG file path")
                    .margin(egui::vec2(8.0, 8.0)),
            )
            .on_hover_text(&self.save_path);
            let enabled = self.capture.is_some() && !self.png_save.is_saving();
            if ui
                .add_enabled_ui(enabled, |ui| {
                    theme::button(ui, Icon::Save, "Save PNG", ButtonStyle::Quiet)
                })
                .inner
                .clicked()
            {
                if let Some(capture) = &self.capture {
                    self.png_save.start(
                        Arc::clone(capture),
                        self.save_path.clone().into(),
                        ctx.clone(),
                    );
                }
            }
        });
        let (message, color) = match self.png_save.status() {
            SaveStatus::Idle => ("Export the captured frame to a new file.".into(), p.muted),
            SaveStatus::Saving(path) => (format!("Saving {}…", path.display()), p.accent),
            SaveStatus::Saved(path) => (format!("Saved {}", path.display()), p.accent),
            SaveStatus::Failed { path, error } => (
                format!("Could not save {}: {error}", path.display()),
                p.error,
            ),
        };
        ui.add(egui::Label::new(RichText::new(&message).size(10.0).color(color)).truncate())
            .on_hover_text(message);
    }
}

fn object_properties(ui: &mut egui::Ui, node: &UiNode, p: Palette) {
    ui.horizontal(|ui| {
        theme::badge(ui, &format!("#{}", node.id), p.accent);
        ui.label(
            RichText::new(format!("revision {}", node.revision))
                .small()
                .color(p.muted),
        );
    });
    ui.label(RichText::new(&node.kind).size(20.0).strong());
    if !node.name.is_empty() {
        ui.add(egui::Label::new(&node.name).wrap());
    }
    ui.add_space(22.0);
    theme::section(ui, "LAYOUT");
    let width = ui.available_width() - 24.0;
    egui::Frame::new()
        .fill(p.raised)
        .corner_radius(8)
        .inner_margin(12)
        .show(ui, |ui| {
            ui.set_width(width);
            egui::Grid::new("object-layout")
                .num_columns(2)
                .min_col_width((width / 2.0 - 10.0).max(50.0))
                .spacing([10.0, 14.0])
                .show(ui, |ui| {
                    for (index, (label, value)) in [
                        ("X", node.x.to_string()),
                        ("Y", node.y.to_string()),
                        ("Width", node.width.to_string()),
                        ("Height", node.height.to_string()),
                    ]
                    .into_iter()
                    .enumerate()
                    {
                        ui.vertical(|ui| {
                            ui.label(RichText::new(label).small().color(p.muted));
                            ui.monospace(value);
                        });
                        if index % 2 == 1 {
                            ui.end_row();
                        }
                    }
                });
        });
    ui.add_space(20.0);
    theme::section(ui, "ATTRIBUTES");
    property(
        ui,
        "Parent",
        if node.parent == 0 {
            "Root".into()
        } else {
            format!("#{}", node.parent)
        },
    );
    property(ui, "Flags", format!("0x{:08x}", node.flags));
    property(ui, "Revision", node.revision.to_string());
}

fn dotted_canvas(ui: &egui::Ui, canvas: egui::Rect, p: Palette) {
    let painter = ui.painter().with_clip_rect(canvas);
    painter.rect_filled(canvas, 8, p.background);
    let mut y = canvas.top() + 12.0;
    while y < canvas.bottom() {
        let mut x = canvas.left() + 12.0;
        while x < canvas.right() {
            painter.circle_filled(egui::pos2(x, y), 0.8, p.grid);
            x += 22.0;
        }
        y += 22.0;
    }
}

fn selection_outline(
    ui: &egui::Ui,
    image: egui::Rect,
    canvas: egui::Rect,
    node: &UiNode,
    scale: f32,
    p: Palette,
) {
    let painter = ui.painter().with_clip_rect(image.intersect(canvas));
    let rect = egui::Rect::from_min_size(
        image.min + egui::vec2(node.x as f32, node.y as f32) * scale,
        egui::vec2(node.width as f32, node.height as f32) * scale,
    )
    .intersect(image);
    if rect.is_positive() {
        painter.rect_stroke(
            rect,
            0,
            Stroke::new(1.5_f32, p.accent),
            egui::StrokeKind::Inside,
        );
        for corner in [
            rect.left_top(),
            rect.right_top(),
            rect.left_bottom(),
            rect.right_bottom(),
        ] {
            painter.rect_filled(
                egui::Rect::from_center_size(corner, egui::Vec2::splat(5.0)),
                1,
                p.accent,
            );
        }
    }
}

pub(super) fn property(ui: &mut egui::Ui, label: &str, value: String) {
    let p = Palette::new(ui.visuals().dark_mode);
    ui.horizontal(|ui| {
        ui.label(RichText::new(label).size(12.0).color(p.muted));
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.add(egui::Label::new(RichText::new(&value).monospace().size(12.0)).truncate())
                .on_hover_text(&value);
        });
    });
}

fn display_point(rect: egui::Rect, source: egui::Vec2, position: egui::Pos2) -> Option<(u32, u32)> {
    if !rect.is_positive()
        || position.x < rect.min.x
        || position.y < rect.min.y
        || position.x >= rect.max.x
        || position.y >= rect.max.y
    {
        return None;
    }
    let point = (position - rect.min) * source / rect.size();
    Some((point.x.floor() as u32, point.y.floor() as u32))
}

fn node_at(nodes: &[UiNode], x: u32, y: u32) -> Option<u32> {
    nodes
        .iter()
        .rev()
        .find(|node| {
            x as i64 >= node.x as i64
                && y as i64 >= node.y as i64
                && (x as i64) < node.x as i64 + node.width as i64
                && (y as i64) < node.y as i64 + node.height as i64
        })
        .map(|node| node.id)
}
