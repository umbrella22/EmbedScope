mod app;
mod fonts;
mod i18n;
mod icons;
mod png_save;
mod theme;
mod worker;

fn main() -> eframe::Result {
    let endpoint = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "tcp://127.0.0.1:9337".into());
    eframe::run_native(
        "EmbedScope Inspector",
        eframe::NativeOptions {
            viewport: eframe::egui::ViewportBuilder::default()
                .with_app_id("io.github.umbrella22.embedscope.inspector")
                .with_inner_size([1380.0, 900.0])
                .with_min_inner_size([900.0, 620.0]),
            ..Default::default()
        },
        Box::new(move |cc| Ok(Box::new(app::Inspector::new(cc, endpoint)))),
    )
}
