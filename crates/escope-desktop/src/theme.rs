use crate::icons::{self, Icon};
use eframe::egui::{self, Color32, FontId, RichText, Stroke};

pub const CONTROL_HEIGHT: f32 = 40.0;
pub const CONTROL_INSET: f32 = 12.0;

#[derive(Clone, Copy)]
pub struct Palette {
    pub background: Color32,
    pub panel: Color32,
    pub raised: Color32,
    pub border: Color32,
    pub text: Color32,
    pub muted: Color32,
    pub accent: Color32,
    pub accent_bg: Color32,
    pub warning: Color32,
    pub error: Color32,
    pub grid: Color32,
}

impl Palette {
    pub fn new(dark: bool) -> Self {
        if dark {
            Self {
                background: Color32::from_rgb(17, 18, 21),
                panel: Color32::from_rgb(23, 24, 28),
                raised: Color32::from_rgb(31, 33, 38),
                border: Color32::from_rgb(43, 46, 53),
                text: Color32::from_rgb(227, 230, 235),
                muted: Color32::from_rgb(146, 153, 166),
                accent: Color32::from_rgb(105, 219, 174),
                accent_bg: Color32::from_rgb(31, 57, 49),
                warning: Color32::from_rgb(241, 193, 111),
                error: Color32::from_rgb(248, 134, 139),
                grid: Color32::from_rgb(43, 46, 53),
            }
        } else {
            Self {
                background: Color32::from_rgb(245, 246, 248),
                panel: Color32::WHITE,
                raised: Color32::from_rgb(241, 243, 245),
                border: Color32::from_rgb(224, 227, 232),
                text: Color32::from_rgb(39, 45, 54),
                muted: Color32::from_rgb(105, 115, 130),
                accent: Color32::from_rgb(22, 127, 91),
                accent_bg: Color32::from_rgb(229, 245, 237),
                warning: Color32::from_rgb(152, 101, 23),
                error: Color32::from_rgb(186, 58, 71),
                grid: Color32::from_rgb(216, 222, 229),
            }
        }
    }
}

pub fn apply(ctx: &egui::Context, dark: bool) {
    let p = Palette::new(dark);
    let theme = if dark {
        egui::Theme::Dark
    } else {
        egui::Theme::Light
    };
    ctx.set_theme(theme);
    let mut style = (*ctx.style_of(theme)).clone();
    style.visuals = if dark {
        egui::Visuals::dark()
    } else {
        egui::Visuals::light()
    };
    let v = &mut style.visuals;
    v.panel_fill = p.panel;
    v.window_fill = p.panel;
    v.window_stroke = Stroke::new(1.0_f32, p.border);
    v.extreme_bg_color = p.background;
    v.faint_bg_color = p.raised;
    v.code_bg_color = p.raised;
    v.override_text_color = Some(p.text);
    v.weak_text_color = Some(p.muted);
    v.hyperlink_color = p.accent;
    v.warn_fg_color = p.warning;
    v.error_fg_color = p.error;
    v.selection.bg_fill = p.accent_bg;
    v.selection.stroke = Stroke::new(1.0_f32, p.accent);
    for widget in [
        &mut v.widgets.noninteractive,
        &mut v.widgets.inactive,
        &mut v.widgets.hovered,
        &mut v.widgets.active,
        &mut v.widgets.open,
    ] {
        widget.corner_radius = egui::CornerRadius::same(6);
        widget.bg_stroke = Stroke::new(1.0_f32, p.border);
        widget.fg_stroke = Stroke::new(1.0_f32, p.text);
        widget.expansion = 0.0;
    }
    v.widgets.noninteractive.bg_fill = p.panel;
    v.widgets.inactive.bg_fill = p.raised;
    v.widgets.inactive.weak_bg_fill = p.raised;
    v.widgets.hovered.bg_fill = p.border;
    v.widgets.hovered.weak_bg_fill = p.border;
    v.widgets.active.bg_fill = p.accent_bg;
    v.widgets.active.bg_stroke.color = p.accent;
    style.spacing.item_spacing = egui::vec2(8.0, 8.0);
    style.spacing.button_padding = egui::vec2(CONTROL_INSET, 9.0);
    style.spacing.interact_size = egui::Vec2::splat(CONTROL_HEIGHT);
    style.spacing.window_margin = egui::Margin::same(16);
    style.spacing.menu_margin = egui::Margin::same(8);
    style.visuals.menu_corner_radius = egui::CornerRadius::same(14);
    style
        .text_styles
        .insert(egui::TextStyle::Body, FontId::proportional(13.0));
    style
        .text_styles
        .insert(egui::TextStyle::Button, FontId::proportional(13.0));
    style
        .text_styles
        .insert(egui::TextStyle::Small, FontId::proportional(11.0));
    style
        .text_styles
        .insert(egui::TextStyle::Monospace, FontId::monospace(12.0));
    style
        .text_styles
        .insert(egui::TextStyle::Heading, FontId::proportional(22.0));
    ctx.set_style_of(theme, style);
}

pub fn panel(p: Palette) -> egui::Frame {
    egui::Frame::new().fill(p.panel).inner_margin(14)
}

/// egui applies its own compact menu style after the application theme.
/// Restore control insets here for both Popup menus and ComboBox popups.
pub fn menu_style(style: &mut egui::Style) {
    egui::containers::menu::menu_style(style);
    style.spacing.button_padding = egui::vec2(CONTROL_INSET, 9.0);
    style.spacing.interact_size = egui::Vec2::splat(CONTROL_HEIGHT);
    style.spacing.item_spacing = egui::vec2(4.0, 4.0);
}

pub fn text_field(text: &mut String) -> egui::TextEdit<'_> {
    egui::TextEdit::singleline(text)
        .margin(egui::vec2(CONTROL_INSET, 10.0))
        .min_size(egui::vec2(0.0, CONTROL_HEIGHT))
}

#[derive(Clone, Copy)]
pub enum ButtonStyle {
    Quiet,
    Primary,
    Selected,
    Warning,
}

pub fn button(
    ui: &mut egui::Ui,
    icon: Icon,
    text: impl AsRef<str>,
    kind: ButtonStyle,
) -> egui::Response {
    let text = text.as_ref();
    let p = Palette::new(ui.visuals().dark_mode);
    let galley = ui.painter().layout_no_wrap(
        text.into(),
        FontId::proportional(13.0),
        Color32::PLACEHOLDER,
    );
    let (rect, response) = ui.allocate_exact_size(
        egui::vec2(galley.size().x + 46.0, CONTROL_HEIGHT),
        egui::Sense::click(),
    );
    response.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, ui.is_enabled(), text));
    let (mut fill, foreground) = match kind {
        ButtonStyle::Primary => (
            p.accent,
            if ui.visuals().dark_mode {
                p.background
            } else {
                Color32::WHITE
            },
        ),
        ButtonStyle::Selected => (p.accent_bg, p.accent),
        ButtonStyle::Warning => (p.warning.gamma_multiply(0.13), p.warning),
        ButtonStyle::Quiet => (
            if response.hovered() {
                p.raised
            } else {
                Color32::TRANSPARENT
            },
            p.muted,
        ),
    };
    if response.is_pointer_button_down_on() {
        fill = if matches!(kind, ButtonStyle::Quiet) {
            p.border
        } else {
            fill.gamma_multiply(0.82)
        };
    } else if response.hovered() && !matches!(kind, ButtonStyle::Quiet) {
        fill = fill.lerp_to_gamma(p.text, 0.08);
    }
    let alpha = if ui.is_enabled() { 1.0 } else { 0.38 };
    ui.painter()
        .rect_filled(rect, 6, fill.gamma_multiply(alpha));
    if response.has_focus() {
        ui.painter().rect_stroke(
            rect,
            6,
            Stroke::new(1.5_f32, p.accent),
            egui::StrokeKind::Inside,
        );
    }
    icons::paint(
        ui.painter(),
        egui::Rect::from_center_size(
            egui::pos2(rect.left() + 18.0, rect.center().y),
            egui::Vec2::splat(16.0),
        ),
        icon,
        foreground.gamma_multiply(alpha),
    );
    ui.painter().galley(
        egui::pos2(rect.left() + 32.0, rect.center().y - galley.size().y / 2.0),
        galley,
        foreground.gamma_multiply(alpha),
    );
    response.on_hover_cursor(egui::CursorIcon::PointingHand)
}

pub fn icon_button(
    ui: &mut egui::Ui,
    icon: Icon,
    label: impl AsRef<str>,
    active: bool,
) -> egui::Response {
    let label = label.as_ref();
    let p = Palette::new(ui.visuals().dark_mode);
    let (rect, response) = ui.allocate_exact_size(egui::Vec2::splat(40.0), egui::Sense::click());
    response.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, ui.is_enabled(), label));
    if active || response.hovered() || response.has_focus() {
        ui.painter()
            .rect_filled(rect, 8, if active { p.accent_bg } else { p.raised });
    }
    if response.has_focus() {
        ui.painter().rect_stroke(
            rect,
            8,
            Stroke::new(1.0_f32, p.accent),
            egui::StrokeKind::Inside,
        );
    }
    icons::paint(
        ui.painter(),
        egui::Rect::from_center_size(rect.center(), egui::Vec2::splat(20.0)),
        icon,
        if active { p.accent } else { p.muted },
    );
    response
        .on_hover_cursor(egui::CursorIcon::PointingHand)
        .on_hover_text(label)
}

pub fn badge(ui: &mut egui::Ui, text: impl AsRef<str>, color: Color32) {
    let text = text.as_ref();
    egui::Frame::new()
        .fill(color.gamma_multiply(if ui.visuals().dark_mode { 0.12 } else { 0.08 }))
        .corner_radius(4)
        .inner_margin(egui::Margin::symmetric(6, 3))
        .show(ui, |ui| {
            ui.label(RichText::new(text).size(10.0).color(color));
        });
}

pub fn section(ui: &mut egui::Ui, title: impl AsRef<str>) {
    let title = title.as_ref();
    ui.label(
        RichText::new(title)
            .size(11.0)
            .strong()
            .color(Palette::new(ui.visuals().dark_mode).muted),
    );
    ui.add_space(5.0);
}

pub fn empty(ui: &mut egui::Ui, icon: Icon, title: impl AsRef<str>, message: impl AsRef<str>) {
    let title = title.as_ref();
    let message = message.as_ref();
    let p = Palette::new(ui.visuals().dark_mode);
    ui.vertical_centered(|ui| {
        let (rect, _) = ui.allocate_exact_size(egui::Vec2::splat(44.0), egui::Sense::hover());
        ui.painter().rect_filled(rect, 10, p.raised);
        icons::paint(ui.painter(), rect.shrink(11.0), icon, p.muted);
        ui.add_space(10.0);
        ui.label(RichText::new(title).size(16.0).strong());
        ui.add_space(3.0);
        ui.add(egui::Label::new(RichText::new(message).color(p.muted)).wrap());
    });
}

pub fn button_width(ui: &egui::Ui, text: &str) -> f32 {
    ui.painter()
        .layout_no_wrap(
            text.into(),
            FontId::proportional(13.0),
            Color32::PLACEHOLDER,
        )
        .size()
        .x
        + 46.0
}
