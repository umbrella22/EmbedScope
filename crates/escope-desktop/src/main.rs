#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app;
mod brand;
mod fonts;
mod i18n;
mod icons;
mod png_save;
mod theme;
mod worker;

fn main() -> eframe::Result {
    let argument = std::env::args().nth(1);
    match argument.as_deref() {
        Some("--version" | "-V") => {
            println!("EmbedScope Inspector {}", env!("CARGO_PKG_VERSION"));
            return Ok(());
        }
        Some("--help" | "-h") => {
            println!("EmbedScope Inspector {}\nUsage: escope-desktop [ENDPOINT]\n\nConnect to a serial path or tcp://host:port.\nOptions: --version, --help", env!("CARGO_PKG_VERSION"));
            return Ok(());
        }
        _ => {}
    }
    let smoke = argument.as_deref() == Some("--smoke-test");
    let endpoint = argument
        .filter(|_| !smoke)
        .unwrap_or_else(|| "tcp://127.0.0.1:9337".into());
    eframe::run_native(
        "EmbedScope Inspector",
        eframe::NativeOptions {
            viewport: eframe::egui::ViewportBuilder::default()
                .with_app_id("io.github.umbrella22.embedscope.inspector")
                .with_icon(brand::window_icon())
                .with_inner_size([1380.0, 900.0])
                .with_min_inner_size([900.0, 620.0]),
            ..Default::default()
        },
        Box::new(move |cc| {
            let inspector = app::Inspector::new(cc, endpoint);
            if smoke {
                Ok(Box::new(SmokeInspector {
                    inspector,
                    frames: 0,
                }))
            } else {
                Ok(Box::new(inspector))
            }
        }),
    )
}

// Distribution acceptance runs the real UI for several frames, then closes it.
// Normal launches use Inspector directly, including its persistence lifecycle.
struct SmokeInspector {
    inspector: app::Inspector,
    frames: u8,
}

impl eframe::App for SmokeInspector {
    fn ui(&mut self, ui: &mut eframe::egui::Ui, frame: &mut eframe::Frame) {
        self.inspector.ui(ui, frame);
        self.frames += 1;
        if self.frames >= 3 {
            println!("embedscope-inspector-smoke-ok");
            ui.ctx()
                .send_viewport_cmd(eframe::egui::ViewportCommand::Close);
        } else {
            ui.ctx().request_repaint();
        }
    }
}
