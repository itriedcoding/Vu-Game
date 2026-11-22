//! Design tokens and egui style for Volt.
//!
//! Palette: near-black blue canvas, dark navy surfaces, electric blue accents and
//! white text. One accent hue, used sparingly, with green/amber/red reserved for
//! status only.

use eframe::egui::{
    self, Color32, Context, CornerRadius, FontData, FontDefinitions, FontFamily, FontId, Stroke,
    TextStyle, Visuals,
};
use std::sync::Arc;

pub const BG: Color32 = Color32::from_rgb(0x06, 0x0A, 0x12);
pub const SIDEBAR: Color32 = Color32::from_rgb(0x08, 0x0D, 0x17);
pub const CARD: Color32 = Color32::from_rgb(0x0B, 0x13, 0x21);
pub const CARD_SOFT: Color32 = Color32::from_rgb(0x09, 0x10, 0x1C);
pub const CARD_HOVER: Color32 = Color32::from_rgb(0x10, 0x1A, 0x2B);
pub const BORDER: Color32 = Color32::from_rgb(0x17, 0x25, 0x3B);
pub const BORDER_SOFT: Color32 = Color32::from_rgb(0x13, 0x1F, 0x32);
pub const TEXT: Color32 = Color32::from_rgb(0xE9, 0xF0, 0xFB);
pub const MUTED: Color32 = Color32::from_rgb(0x93, 0xA7, 0xC4);
pub const DIM: Color32 = Color32::from_rgb(0x5E, 0x73, 0x92);
pub const ACCENT: Color32 = Color32::from_rgb(0x2F, 0x7D, 0xFF);
pub const ACCENT_BRIGHT: Color32 = Color32::from_rgb(0x5A, 0xA2, 0xFF);
pub const ACCENT_DEEP: Color32 = Color32::from_rgb(0x1B, 0x4F, 0xB8);
pub const SUCCESS: Color32 = Color32::from_rgb(0x35, 0xD0, 0x8A);
pub const WARN: Color32 = Color32::from_rgb(0xFF, 0xB4, 0x3D);
pub const DANGER: Color32 = Color32::from_rgb(0xFF, 0x60, 0x70);

pub const CARD_RADIUS: u8 = 14;

pub fn semibold(size: f32) -> FontId {
    FontId::new(size, FontFamily::Name("Volt Semibold".into()))
}

pub fn medium(size: f32) -> FontId {
    FontId::new(size, FontFamily::Name("Volt Medium".into()))
}

pub fn body(size: f32) -> FontId {
    FontId::new(size, FontFamily::Proportional)
}

/// Blend two colours (`t` = 0 gives `a`, 1 gives `b`).
pub fn mix(a: Color32, b: Color32, t: f32) -> Color32 {
    let t = t.clamp(0.0, 1.0);
    let channel = |x: u8, y: u8| (x as f32 + (y as f32 - x as f32) * t).round() as u8;
    Color32::from_rgba_unmultiplied(
        channel(a.r(), b.r()),
        channel(a.g(), b.g()),
        channel(a.b(), b.b()),
        channel(a.a(), b.a()),
    )
}

pub fn alpha(color: Color32, alpha: u8) -> Color32 {
    Color32::from_rgba_unmultiplied(color.r(), color.g(), color.b(), alpha)
}

pub fn install(ctx: &Context) {
    install_fonts(ctx);

    let mut style = (*ctx.style()).clone();
    style.visuals = visuals();
    style.spacing.item_spacing = egui::vec2(10.0, 8.0);
    style.spacing.button_padding = egui::vec2(14.0, 8.0);
    style.spacing.interact_size = egui::vec2(0.0, 30.0);
    style.spacing.indent = 16.0;
    style.text_styles = [
        (TextStyle::Heading, semibold(21.0)),
        (TextStyle::Body, body(14.0)),
        (TextStyle::Button, medium(14.0)),
        (TextStyle::Small, body(12.0)),
        (
            TextStyle::Monospace,
            FontId::new(12.5, FontFamily::Monospace),
        ),
    ]
    .into();
    ctx.set_style(style);
}

fn install_fonts(ctx: &Context) {
    let mut fonts = FontDefinitions::default();

    fonts.font_data.insert(
        "barlow".to_owned(),
        Arc::new(FontData::from_static(include_bytes!(
            "../../../assets/fonts/Barlow-Regular.ttf"
        ))),
    );
    fonts.font_data.insert(
        "barlow-medium".to_owned(),
        Arc::new(FontData::from_static(include_bytes!(
            "../../../assets/fonts/Barlow-Medium.ttf"
        ))),
    );
    fonts.font_data.insert(
        "barlow-semibold".to_owned(),
        Arc::new(FontData::from_static(include_bytes!(
            "../../../assets/fonts/Barlow-SemiBold.ttf"
        ))),
    );

    // Keep egui's bundled fonts as glyph fallbacks (arrows, symbols, emoji).
    let fallback = fonts
        .families
        .get(&FontFamily::Proportional)
        .cloned()
        .unwrap_or_default();

    let mut proportional = vec!["barlow".to_owned()];
    proportional.extend(fallback.iter().cloned());
    fonts
        .families
        .insert(FontFamily::Proportional, proportional);

    let mut semibold = vec!["barlow-semibold".to_owned(), "barlow".to_owned()];
    semibold.extend(fallback.iter().cloned());
    fonts
        .families
        .insert(FontFamily::Name("Volt Semibold".into()), semibold);

    let mut medium = vec!["barlow-medium".to_owned(), "barlow".to_owned()];
    medium.extend(fallback.iter().cloned());
    fonts
        .families
        .insert(FontFamily::Name("Volt Medium".into()), medium);

    ctx.set_fonts(fonts);
}

fn visuals() -> Visuals {
    let mut visuals = Visuals::dark();
    visuals.override_text_color = Some(TEXT);
    visuals.panel_fill = BG;
    visuals.window_fill = CARD;
    visuals.extreme_bg_color = Color32::from_rgb(0x07, 0x0C, 0x16);
    visuals.faint_bg_color = CARD_SOFT;
    visuals.hyperlink_color = ACCENT_BRIGHT;
    visuals.selection.bg_fill = alpha(ACCENT, 60);
    visuals.selection.stroke = Stroke::new(1.0_f32, ACCENT_BRIGHT);
    visuals.window_corner_radius = CornerRadius::same(14);

    visuals.widgets.noninteractive.bg_fill = CARD;
    visuals.widgets.noninteractive.weak_bg_fill = CARD;
    visuals.widgets.noninteractive.bg_stroke = Stroke::new(1.0_f32, BORDER);
    visuals.widgets.noninteractive.fg_stroke = Stroke::new(1.0_f32, TEXT);
    visuals.widgets.noninteractive.corner_radius = CornerRadius::same(8);

    visuals.widgets.inactive.bg_fill = Color32::from_rgb(0x10, 0x1A, 0x2A);
    visuals.widgets.inactive.weak_bg_fill = Color32::from_rgb(0x10, 0x1A, 0x2A);
    visuals.widgets.inactive.bg_stroke = Stroke::new(1.0_f32, BORDER);
    visuals.widgets.inactive.fg_stroke = Stroke::new(1.0_f32, MUTED);
    visuals.widgets.inactive.corner_radius = CornerRadius::same(8);

    visuals.widgets.hovered.bg_fill = CARD_HOVER;
    visuals.widgets.hovered.weak_bg_fill = CARD_HOVER;
    visuals.widgets.hovered.bg_stroke = Stroke::new(1.0_f32, Color32::from_rgb(0x25, 0x3B, 0x5A));
    visuals.widgets.hovered.fg_stroke = Stroke::new(1.0_f32, TEXT);
    visuals.widgets.hovered.corner_radius = CornerRadius::same(8);

    visuals.widgets.active.bg_fill = Color32::from_rgb(0x16, 0x24, 0x3A);
    visuals.widgets.active.weak_bg_fill = Color32::from_rgb(0x16, 0x24, 0x3A);
    visuals.widgets.active.bg_stroke = Stroke::new(1.0_f32, ACCENT);
    visuals.widgets.active.fg_stroke = Stroke::new(1.0_f32, TEXT);
    visuals.widgets.active.corner_radius = CornerRadius::same(8);

    visuals
}
