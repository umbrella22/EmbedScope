use crate::{
    icons::{self, Icon},
    png_save::PngSaver,
    theme::{self, ButtonStyle, Palette},
    worker::{self, Command, Update},
};
use eframe::egui::{self, RichText};
use escope_host::{Capture, DeviceInfo, Hello, LogRecord, Metrics, UiNode};
use std::{
    collections::{HashMap, VecDeque},
    sync::{mpsc, Arc},
    time::{Duration, Instant},
};

mod diagnostics;
mod inspector;
#[cfg(test)]
mod tests;

#[derive(Clone, Copy, PartialEq, Eq)]
enum View {
    Inspector,
    Logs,
    Metrics,
    Device,
}

impl View {
    fn title(self) -> &'static str {
        match self {
            Self::Inspector => "UI Inspector",
            Self::Logs => "Event logs",
            Self::Metrics => "System metrics",
            Self::Device => "Device",
        }
    }
    fn description(self) -> &'static str {
        match self {
            Self::Inspector => "Explore objects, inspect pixels, understand your interface.",
            Self::Logs => "Structured events from the connected Agent.",
            Self::Metrics => "Memory and runtime measurements from the latest snapshot.",
            Self::Device => "Device identity, connection and supported capabilities.",
        }
    }
}

pub struct Inspector {
    endpoint: String,
    commands: tokio::sync::mpsc::Sender<Command>,
    updates: mpsc::Receiver<Update>,
    info: Option<DeviceInfo>,
    hello: Option<Hello>,
    nodes: Vec<UiNode>,
    depths: HashMap<u32, usize>,
    dtr: bool,
    selected: Option<u32>,
    metrics: Option<Metrics>,
    capture: Option<Arc<Capture>>,
    texture: Option<egui::TextureHandle>,
    logs: VecDeque<LogRecord>,
    host_skipped_logs: u64,
    error: Option<String>,
    busy: bool,
    auto: bool,
    inject: bool,
    save_path: String,
    png_save: PngSaver,
    dark: bool,
    view: View,
    tree_query: String,
    log_query: String,
    log_level: u8,
    follow_logs: bool,
    show_logs: bool,
    last_snapshot: Option<Instant>,
}

impl Inspector {
    pub fn new(cc: &eframe::CreationContext<'_>, endpoint: String) -> Self {
        crate::fonts::install_system_cjk(&cc.egui_ctx);
        theme::apply(&cc.egui_ctx, true);
        let (commands, updates) = worker::spawn();
        Self::with_channels(endpoint, commands, updates)
    }

    fn with_channels(
        endpoint: String,
        commands: tokio::sync::mpsc::Sender<Command>,
        updates: mpsc::Receiver<Update>,
    ) -> Self {
        Self {
            endpoint,
            commands,
            updates,
            info: None,
            hello: None,
            nodes: Vec::new(),
            depths: HashMap::new(),
            dtr: false,
            selected: None,
            metrics: None,
            capture: None,
            texture: None,
            logs: VecDeque::new(),
            host_skipped_logs: 0,
            error: None,
            busy: false,
            auto: false,
            inject: false,
            save_path: default_capture_path(),
            png_save: PngSaver::default(),
            dark: true,
            view: View::Inspector,
            tree_query: String::new(),
            log_query: String::new(),
            log_level: 5,
            follow_logs: true,
            show_logs: true,
            last_snapshot: None,
        }
    }

    fn supports(&self, capability: u32) -> bool {
        self.hello
            .as_ref()
            .is_some_and(|hello| hello.capabilities & capability != 0)
    }

    fn send(&mut self, command: Command) {
        match self.commands.try_send(command) {
            Ok(()) => self.busy = true,
            Err(e) => self.error = Some(e.to_string()),
        }
    }

    fn clear_device(&mut self) {
        self.info = None;
        self.hello = None;
        self.nodes.clear();
        self.depths.clear();
        self.selected = None;
        self.metrics = None;
        self.capture = None;
        self.texture = None;
        self.inject = false;
        self.auto = false;
        self.last_snapshot = None;
    }

    fn receive(&mut self, ctx: &egui::Context) {
        self.png_save.poll();
        for _ in 0..256 {
            let Ok(update) = self.updates.try_recv() else {
                break;
            };
            match update {
                Update::Connected(info, hello) => {
                    self.clear_device();
                    self.logs.clear();
                    self.host_skipped_logs = 0;
                    self.error = None;
                    self.info = Some(info);
                    self.hello = Some(hello);
                }
                Update::Snapshot(nodes, metrics, capture) => {
                    if self.selected.is_some_and(|id| {
                        !nodes.iter().any(|n| {
                            n.id == id
                                && self
                                    .nodes
                                    .iter()
                                    .any(|old| old.id == id && old.revision == n.revision)
                        })
                    }) {
                        self.selected = None;
                    }
                    self.depths = escope_host::tree_depths(&nodes).unwrap_or_default();
                    self.nodes = nodes;
                    self.metrics = metrics;
                    self.texture = capture.as_ref().map(|capture| {
                        let image = egui::ColorImage::from_rgb(
                            [capture.meta.width as usize, capture.meta.height as usize],
                            &capture.rgb,
                        );
                        ctx.load_texture("device-display", image, egui::TextureOptions::NEAREST)
                    });
                    self.capture = capture.map(Arc::new);
                    self.last_snapshot = Some(Instant::now());
                    self.error = None;
                }
                Update::Log(log) => {
                    if self.logs.len() == 512 {
                        self.logs.pop_front();
                    }
                    self.logs.push_back(log);
                }
                Update::LogGap(count) => {
                    self.host_skipped_logs = self.host_skipped_logs.saturating_add(count);
                }
                Update::Error(error) => self.error = Some(error),
                Update::Disconnected => {
                    self.clear_device();
                    self.busy = false;
                }
                Update::Busy(value) => self.busy = value,
            }
        }
    }

    fn navigation(&mut self, ctx: &egui::Context) {
        let p = Palette::new(self.dark);
        egui::SidePanel::left("navigation").exact_width(64.0).resizable(false)
            .frame(egui::Frame::new().fill(p.panel).inner_margin(12))
            .show(ctx, |ui| {
                let (logo, _) = ui.allocate_exact_size(egui::vec2(40.0, 48.0), egui::Sense::hover());
                icons::logo(ui.painter(), logo.center(), 30.0, p.accent);
                ui.add_space(16.0);
                for (view, icon, label) in [
                    (View::Inspector, Icon::Inspect, "UI Inspector"),
                    (View::Logs, Icon::Logs, "Event logs"),
                    (View::Metrics, Icon::Metrics, "System metrics"),
                    (View::Device, Icon::Device, "Device information"),
                ] {
                    if theme::icon_button(ui, icon, label, self.view == view).clicked() { self.view = view; }
                    ui.add_space(4.0);
                }
                ui.with_layout(egui::Layout::bottom_up(egui::Align::Center), |ui| {
                    if theme::icon_button(ui, if self.dark {Icon::Sun} else {Icon::Moon}, if self.dark {"Switch to light theme"} else {"Switch to dark theme"}, false).clicked() {
                        self.dark = !self.dark;
                        theme::apply(ctx, self.dark);
                    }
                    if theme::icon_button(ui, Icon::Book, "Open documentation", false).clicked() {
                        ui.ctx().open_url(egui::OpenUrl::new_tab("https://umbrella22.github.io/EmbedScope_website/guide/basic/inspector.html"));
                    }
                });
            });
    }

    fn header(&mut self, ctx: &egui::Context) {
        let p = Palette::new(self.dark);
        egui::TopBottomPanel::top("workspace-header").frame(theme::panel(p).inner_margin(egui::Margin::symmetric(20,14)))
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.label(RichText::new("EmbedScope").size(15.0).strong());
                    ui.label(RichText::new("/").color(p.border));
                    ui.label(RichText::new("Workspace").color(p.muted));
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        let connected = self.info.is_some();
                        let connect = ui.add_enabled_ui(connected || !self.busy, |ui| {
                            theme::button(ui, if connected {Icon::Disconnect} else {Icon::Connect}, if connected {"Disconnect"} else {"Connect"}, if connected {ButtonStyle::Quiet} else {ButtonStyle::Primary})
                        }).inner;
                        if connect.clicked() {
                            self.error = None;
                            self.send(if connected {Command::Disconnect} else {Command::Connect(self.endpoint.clone(),self.dtr)});
                        }
                        ui.add_enabled(!connected && !self.busy, egui::Checkbox::new(&mut self.dtr,"DTR"))
                            .on_hover_text("Assert DTR when opening a serial connection. Required by the Dango diagnostic CDC.");
                        let width = (ui.available_width()-24.0).clamp(150.0,320.0);
                        ui.add_enabled(!connected && !self.busy,egui::TextEdit::singleline(&mut self.endpoint)
                            .font(egui::TextStyle::Monospace).desired_width(width)
                            .margin(egui::vec2(10.0,8.0)).hint_text("tcp://host:port or serial path"));
                    });
                });
                ui.add_space(14.0);
                ui.horizontal(|ui| {
                    ui.vertical(|ui| {
                        ui.label(RichText::new(self.view.title()).size(23.0).strong());
                        ui.label(RichText::new(self.view.description()).size(12.0).color(p.muted));
                    });
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if self.view == View::Inspector && theme::button(ui,Icon::Logs,"Logs",if self.show_logs {ButtonStyle::Selected} else {ButtonStyle::Quiet}).clicked() {self.show_logs = !self.show_logs;}
                        ui.add_enabled_ui(self.info.is_some() && !self.busy,|ui| {
                            if theme::button(ui,if self.auto {Icon::Pause} else {Icon::Play},"Live · 1s",if self.auto {ButtonStyle::Selected} else {ButtonStyle::Quiet}).on_hover_text("Refresh the tree, display and metrics every second.").clicked() {
                                self.auto = !self.auto;
                                self.send(Command::Auto(self.auto));
                            }
                            if theme::button(ui,Icon::Refresh,"Refresh",ButtonStyle::Quiet).clicked() {self.send(Command::Refresh);}
                        });
                        if self.busy {ui.spinner();}
                    });
                });
            });
        if let Some(error) = self.error.clone() {
            egui::TopBottomPanel::top("connection-error")
                .frame(
                    egui::Frame::new()
                        .fill(p.error.gamma_multiply(0.08))
                        .inner_margin(egui::Margin::symmetric(20, 8)),
                )
                .show(ctx, |ui| {
                    ui.horizontal_wrapped(|ui| {
                        if ui.small_button("Dismiss").clicked() {
                            self.error = None;
                        }
                        ui.colored_label(p.error, error);
                    });
                });
        }
    }

    fn status(&self, ctx: &egui::Context) {
        let p = Palette::new(self.dark);
        egui::TopBottomPanel::bottom("session-status")
            .exact_height(30.0)
            .frame(
                egui::Frame::new()
                    .fill(p.panel)
                    .inner_margin(egui::Margin::symmetric(16, 5)),
            )
            .show(ctx, |ui| {
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
                            "Connected"
                        } else if self.busy {
                            "Connecting…"
                        } else {
                            "Disconnected"
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
                                RichText::new(format!("Protocol {}", hello.protocol))
                                    .size(10.0)
                                    .color(p.muted),
                            );
                        }
                        if let Some(last) = self.last_snapshot {
                            ui.label(
                                RichText::new(format!(
                                    "Snapshot {:.0}s ago",
                                    last.elapsed().as_secs_f32()
                                ))
                                .size(10.0)
                                .color(p.muted),
                            );
                        }
                    });
                });
            });
    }

    fn render(&mut self, ctx: &egui::Context) {
        self.receive(ctx);
        self.navigation(ctx);
        self.header(ctx);
        self.status(ctx);
        match self.view {
            View::Inspector => self.inspector(ctx),
            View::Logs => self.log_page(ctx),
            View::Metrics => self.metrics_page(ctx),
            View::Device => self.device_page(ctx),
        }
        ctx.request_repaint_after(Duration::from_millis(100));
    }
}

impl eframe::App for Inspector {
    fn update(&mut self, ctx: &egui::Context, _: &mut eframe::Frame) {
        self.render(ctx);
    }
}

fn bytes(value: Option<u64>) -> String {
    value.map_or("Unavailable".into(), |v| {
        format!("{:.1} KiB", v as f64 / 1024.0)
    })
}

fn default_capture_path() -> String {
    let directory = std::env::var_os("HOME")
        .map(std::path::PathBuf::from)
        .map(|base| base.join("Pictures"))
        .filter(|path| path.is_dir())
        .unwrap_or_else(std::env::temp_dir);
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis();
    directory
        .join(format!("embedscope-{stamp}.png"))
        .to_string_lossy()
        .into_owned()
}
