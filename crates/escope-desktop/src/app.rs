use crate::{
    i18n::{tr, I18n, Preference, LOCALES, STORAGE_KEY},
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

mod chrome;
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
    fn title(self, i18n: &I18n) -> String {
        match self {
            Self::Inspector => tr!(i18n, "nav-inspector"),
            Self::Logs => tr!(i18n, "nav-logs"),
            Self::Metrics => tr!(i18n, "nav-metrics"),
            Self::Device => tr!(i18n, "nav-device"),
        }
    }
    fn description(self, i18n: &I18n) -> String {
        match self {
            Self::Inspector => tr!(i18n, "view-inspector-description"),
            Self::Logs => tr!(i18n, "view-logs-description"),
            Self::Metrics => tr!(i18n, "view-metrics-description"),
            Self::Device => tr!(i18n, "view-device-description"),
        }
    }
}

pub struct Inspector {
    i18n: I18n,
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
        let stored = cc
            .storage
            .and_then(|storage| storage.get_string(STORAGE_KEY));
        Self::with_channels(
            endpoint,
            commands,
            updates,
            I18n::new(stored.as_deref(), sys_locale::get_locales()),
        )
    }

    fn with_channels(
        endpoint: String,
        commands: tokio::sync::mpsc::Sender<Command>,
        updates: mpsc::Receiver<Update>,
        i18n: I18n,
    ) -> Self {
        Self {
            i18n,
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

    fn render(&mut self, root: &mut egui::Ui) {
        self.receive(root.ctx());
        self.navigation(root);
        self.header(root);
        self.status(root);
        match self.view {
            View::Inspector => self.inspector(root),
            View::Logs => self.log_page(root),
            View::Metrics => self.metrics_page(root),
            View::Device => self.device_page(root),
        }
        root.ctx().request_repaint_after(Duration::from_millis(100));
    }
}

impl eframe::App for Inspector {
    fn save(&mut self, storage: &mut dyn eframe::Storage) {
        storage.set_string(STORAGE_KEY, self.i18n.preference().stored().into());
    }

    fn ui(&mut self, ui: &mut egui::Ui, _: &mut eframe::Frame) {
        self.render(ui);
    }
}

fn bytes(i18n: &I18n, value: Option<u64>) -> String {
    value.map_or(tr!(i18n, "unavailable"), |v| {
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
