//! Brand images are rasterized from assets/brand/*.svg and embedded in the binary.
use eframe::egui::{self, Color32, Pos2, Rect};

pub fn window_icon() -> egui::IconData {
    eframe::icon_data::from_png_bytes(include_bytes!("../assets/brand/app-icon.png"))
        .expect("embedded application icon is a valid PNG")
}

pub fn paint(painter: &egui::Painter, center: Pos2, size: f32, color: Color32) {
    let ctx = painter.ctx();
    let id = egui::Id::unique("embedscope-brand-mark");
    let texture = ctx
        .data(|data| data.get_temp::<egui::TextureHandle>(id))
        .unwrap_or_else(|| {
            let mask =
                eframe::icon_data::from_png_bytes(include_bytes!("../assets/brand/mark-mask.png"))
                    .expect("embedded brand mask is a valid PNG");
            let image = egui::ColorImage::from_rgba_unmultiplied(
                [mask.width as usize, mask.height as usize],
                &mask.rgba,
            );
            let texture =
                ctx.load_texture("embedscope-brand-mark", image, egui::TextureOptions::LINEAR);
            ctx.data_mut(|data| data.insert_temp(id, texture.clone()));
            texture
        });
    painter.image(
        texture.id(),
        Rect::from_center_size(center, egui::Vec2::splat(size)),
        Rect::from_min_max(Pos2::ZERO, egui::pos2(1.0, 1.0)),
        color,
    );
}
