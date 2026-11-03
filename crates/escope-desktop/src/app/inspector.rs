use super::*;
use crate::png_save::SaveStatus;
use eframe::egui::{FontId, Stroke};

impl Inspector {
    pub(super) fn inspector(&mut self, root: &mut egui::Ui) {
        let p = Palette::new(self.dark);
        if self.show_logs {
            egui::Panel::bottom("log-drawer")
                .resizable(true)
                .default_size(160.0)
                .min_size(100.0)
                .max_size((root.available_height() * 0.45).max(100.0))
                .frame(theme::panel(p).inner_margin(egui::Margin::symmetric(16, 10)))
                .show(root, |ui| {
                    self.log_contents(ui, true);
                });
        }
        self.object_tree(root);
        self.properties(root);
        self.display(root);
    }

    fn object_tree(&mut self, root: &mut egui::Ui) {
        let p = Palette::new(self.dark);
        egui::Panel::left("object-tree")
            .resizable(true)
            .default_size(236.0)
            .size_range(170.0..=360.0)
            .frame(theme::panel(p))
            .show(root, |ui| {
                ui.horizontal(|ui| {
                    ui.strong(tr!(self.i18n, "objects"));
                    theme::badge(ui, self.nodes.len().to_string(), p.muted);
                });
                ui.add_space(8.0);
                ui.add(
                    theme::text_field(&mut self.tree_query)
                        .desired_width(f32::INFINITY)
                        .hint_text(tr!(self.i18n, "tree-filter")),
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
                                    if node.name.is_empty() {
                                        theme::CONTROL_HEIGHT
                                    } else {
                                        48.0
                                    },
                                ),
                                egui::Sense::click(),
                            );
                            response.widget_info(|| {
                                egui::WidgetInfo::selected(
                                    egui::Role::ListBoxOption,
                                    true,
                                    selected,
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
                            let id = format!("#{}", node.id);
                            let id_width = ui
                                .painter()
                                .layout_no_wrap(id.clone(), FontId::monospace(10.0), p.muted)
                                .size()
                                .x;
                            let depth = self.depths.get(&node.id).copied().unwrap_or(0);
                            let indent = (depth as f32 * 12.0)
                                .min((rect.width() - id_width - 80.0).max(0.0));
                            let left = rect.left() + 10.0 + indent;
                            let top = rect.top() + if node.name.is_empty() { 12.0 } else { 7.0 };
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
                                egui::pos2(rect.right() - id_width - 19.0, rect.bottom()),
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
                                egui::pos2(rect.right() - 12.0, top + 1.0),
                                egui::Align2::RIGHT_TOP,
                                id,
                                FontId::monospace(10.0),
                                p.muted,
                            );
                            if response.clicked() {
                                self.selected = Some(node.id);
                            }
                            response
                                .on_hover_cursor(egui::CursorIcon::PointingHand)
                                .on_hover_text(tr!(
                                    self.i18n,
                                    "object-bounds",
                                    kind = &node.kind,
                                    id = node.id,
                                    name = &node.name,
                                    width = node.width,
                                    height = node.height,
                                    x = node.x,
                                    y = node.y
                                ));
                            ui.add_space(1.0);
                        }
                        if shown == 0 {
                            ui.add_space(24.0);
                            theme::empty(
                                ui,
                                Icon::Layers,
                                if self.nodes.is_empty() {
                                    tr!(self.i18n, "objects-empty")
                                } else {
                                    tr!(self.i18n, "objects-no-match")
                                },
                                if self.info.is_none() {
                                    tr!(self.i18n, "objects-connect")
                                } else if !self.supports(escope_host::protocol::capability::UI_TREE)
                                {
                                    tr!(self.i18n, "objects-unsupported")
                                } else if !query.is_empty() {
                                    tr!(self.i18n, "objects-filter-help")
                                } else {
                                    tr!(self.i18n, "objects-refresh")
                                },
                            );
                        }
                    });
            });
    }

    fn properties(&mut self, root: &mut egui::Ui) {
        let p = Palette::new(self.dark);
        egui::Panel::right("object-properties")
            .resizable(true)
            .default_size(252.0)
            .size_range(210.0..=380.0)
            .frame(theme::panel(p))
            .show(root, |ui| {
                ui.strong(tr!(self.i18n, "properties"));
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
            object_properties(ui, node, p, &self.i18n);
        } else {
            ui.add_space(18.0);
            theme::empty(
                ui,
                Icon::Inspect,
                tr!(self.i18n, "inspect-object"),
                tr!(self.i18n, "inspect-object-help"),
            );
        }
        ui.add_space(24.0);
        ui.separator();
        ui.add_space(12.0);
        theme::section(ui, tr!(self.i18n, "device-snapshot"));
        if let Some(info) = &self.info {
            ui.label(RichText::new(&info.name).strong());
            ui.add(egui::Label::new(RichText::new(&info.model).small().color(p.muted)).wrap());
        } else {
            ui.label(RichText::new(tr!(self.i18n, "no-session")).color(p.muted));
        }
        if let Some(metrics) = &self.metrics {
            ui.add_space(10.0);
            property(
                ui,
                tr!(self.i18n, "heap-free"),
                super::bytes(&self.i18n, metrics.heap_free),
            );
            property(
                ui,
                tr!(self.i18n, "frame"),
                metrics
                    .frame_ms
                    .map_or(tr!(self.i18n, "unavailable"), |v| format!("{v} ms")),
            );
            property(
                ui,
                tr!(self.i18n, "queue-depth"),
                metrics
                    .queue_depth
                    .map_or(tr!(self.i18n, "unavailable"), |v| v.to_string()),
            );
        }
        ui.add_space(10.0);
        if theme::button(
            ui,
            Icon::Metrics,
            tr!(self.i18n, "all-metrics"),
            ButtonStyle::Quiet,
        )
        .clicked()
        {
            self.view = View::Metrics;
        }
    }

    fn display(&mut self, root: &mut egui::Ui) {
        let p = Palette::new(self.dark);
        egui::CentralPanel::default()
            .frame(egui::Frame::new().fill(p.background).inner_margin(16))
            .show(root, |ui| self.display_contents(ui, &ui.ctx().clone(), p));
    }

    fn display_contents(&mut self, ui: &mut egui::Ui, ctx: &egui::Context, p: Palette) {
        // Let the actual export row and wrapped status determine their own height.
        // A fixed canvas reservation clips this area when controls or translations grow.
        egui::Panel::bottom("capture-export")
            .default_size(0.0)
            .min_size(0.0)
            .frame(egui::Frame::new().inner_margin(egui::Margin {
                top: 8,
                ..Default::default()
            }))
            .show(ui, |ui| self.export_controls(ui, ctx, p));
        self.display_modes(ui, p);
        ui.add_space(8.0);
        let metadata_height = ui
            .spacing()
            .interact_size
            .y
            .max(ui.text_style_height(&egui::TextStyle::Small));
        let stage_size = egui::vec2(
            ui.available_width(),
            (ui.available_height() - metadata_height - ui.spacing().item_spacing.y * 2.0 - 2.0)
                .max(40.0),
        );
        let (canvas, _) = ui.allocate_exact_size(stage_size, egui::Sense::hover());
        dotted_canvas(ui, canvas, p);
        if let (Some(texture), Some(capture)) = (&self.texture, &self.capture) {
            self.display_frame(ui, canvas, texture.id(), capture.meta, p);
        } else {
            self.display_empty(ui, canvas, p);
            ui.add_space(2.0);
            ui.label(
                RichText::new(tr!(self.i18n, "no-display"))
                    .size(11.0)
                    .color(p.muted),
            );
        }
    }

    fn display_modes(&mut self, ui: &mut egui::Ui, p: Palette) {
        ui.horizontal_wrapped(|ui| {
            let inspect = if self.inject {
                ButtonStyle::Quiet
            } else {
                ButtonStyle::Selected
            };
            if theme::button(ui, Icon::Inspect, tr!(self.i18n, "inspect-mode"), inspect)
                .on_hover_text(tr!(self.i18n, "inspect-mode-hint"))
                .clicked()
            {
                self.inject = false;
            }
            ui.add_enabled_ui(
                self.supports(escope_host::protocol::capability::INPUT),
                |ui| {
                    let mode = if self.inject {
                        ButtonStyle::Warning
                    } else {
                        ButtonStyle::Quiet
                    };
                    if theme::button(ui, Icon::Tap, tr!(self.i18n, "tap-mode"), mode)
                        .on_hover_text(tr!(self.i18n, "tap-mode-hint"))
                        .clicked()
                    {
                        self.inject = true;
                    }
                },
            );
            if self.inject {
                ui.label(
                    RichText::new(tr!(self.i18n, "input-armed"))
                        .small()
                        .color(p.warning),
                );
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
        painter.rect_stroke(
            image_rect,
            0,
            Stroke::new(
                1.0_f32,
                if self.dark {
                    egui::Color32::from_white_alpha(26)
                } else {
                    egui::Color32::from_black_alpha(26)
                },
            ),
            egui::StrokeKind::Inside,
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
                RichText::new(tr!(
                    self.i18n,
                    "frame-number",
                    id = meta.frame_id.to_string()
                ))
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
            (
                tr!(self.i18n, "display-welcome"),
                tr!(self.i18n, "display-connect"),
            )
        } else if !self.supports(escope_host::protocol::capability::CAPTURE) {
            (
                tr!(self.i18n, "capture-unsupported"),
                tr!(self.i18n, "capture-unsupported-help"),
            )
        } else {
            (
                tr!(self.i18n, "capture-waiting"),
                tr!(self.i18n, "capture-refresh"),
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
                RichText::new(tr!(self.i18n, "synthetic-data"))
                    .small()
                    .color(p.muted),
            );
        }
    }

    fn export_controls(&mut self, ui: &mut egui::Ui, ctx: &egui::Context, p: Palette) {
        ui.horizontal(|ui| {
            let label = tr!(self.i18n, "save-png");
            let width = (ui.available_width()
                - theme::button_width(ui, &label)
                - ui.spacing().item_spacing.x
                - 24.0)
                .max(40.0);
            ui.add(
                theme::text_field(&mut self.save_path)
                    .desired_width(width)
                    .font(egui::TextStyle::Monospace)
                    .hint_text(tr!(self.i18n, "png-path-hint")),
            )
            .on_hover_text(&self.save_path);
            let enabled = self.capture.is_some() && !self.png_save.is_saving();
            if ui
                .add_enabled_ui(enabled, |ui| {
                    theme::button(ui, Icon::Save, &label, ButtonStyle::Quiet)
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
            SaveStatus::Idle => (tr!(self.i18n, "png-idle"), p.muted),
            SaveStatus::Saving(path) => (
                tr!(self.i18n, "png-saving", path = path.display().to_string()),
                p.accent,
            ),
            SaveStatus::Saved(path) => (
                tr!(self.i18n, "png-saved", path = path.display().to_string()),
                p.accent,
            ),
            SaveStatus::Failed { path, error } => (
                tr!(
                    self.i18n,
                    "png-failed",
                    path = path.display().to_string(),
                    error = error
                ),
                p.error,
            ),
        };
        ui.add(egui::Label::new(RichText::new(&message).size(10.0).color(color)).truncate())
            .on_hover_text(message);
    }
}

fn object_properties(ui: &mut egui::Ui, node: &UiNode, p: Palette, i18n: &I18n) {
    ui.horizontal(|ui| {
        theme::badge(ui, format!("#{}", node.id), p.accent);
        ui.label(
            RichText::new(tr!(i18n, "revision-number", revision = node.revision))
                .small()
                .color(p.muted),
        );
    });
    ui.label(RichText::new(&node.kind).size(20.0).strong());
    if !node.name.is_empty() {
        ui.add(egui::Label::new(&node.name).wrap());
    }
    ui.add_space(22.0);
    theme::section(ui, tr!(i18n, "layout"));
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
                        (tr!(i18n, "geometry-x"), node.x.to_string()),
                        (tr!(i18n, "geometry-y"), node.y.to_string()),
                        (tr!(i18n, "width"), node.width.to_string()),
                        (tr!(i18n, "height"), node.height.to_string()),
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
    theme::section(ui, tr!(i18n, "attributes"));
    property(
        ui,
        tr!(i18n, "parent"),
        if node.parent == 0 {
            tr!(i18n, "root")
        } else {
            format!("#{}", node.parent)
        },
    );
    property(ui, tr!(i18n, "flags"), format!("0x{:08x}", node.flags));
    property(ui, tr!(i18n, "revision"), node.revision.to_string());
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

pub(super) fn property(ui: &mut egui::Ui, label: impl AsRef<str>, value: String) {
    let label = label.as_ref();
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
