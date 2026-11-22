//! Custom widgets.
//!
//! Volt paints its own cards, buttons, pills and charts so the interface has one
//! consistent look instead of default egui chrome. Everything here is immediate
//! mode and cheap: no textures are uploaded after startup.

use crate::icon::{self, Glyph};
use crate::theme;
use eframe::egui::{
    self, pos2, vec2, Align, Align2, Color32, CornerRadius, FontId, Layout, Margin, Mesh, Rect,
    Response, RichText, Sense, Shape, Stroke, StrokeKind, Ui, Vec2,
};

pub const CARD_PAD: f32 = 18.0;
const RADIUS: u8 = 10;

/// A full-width card with the Volt surface treatment.
pub fn card(ui: &mut Ui, add: impl FnOnce(&mut Ui)) -> Response {
    let available = ui.available_width();
    let inner_width = (available - 2.0 * CARD_PAD - 2.0).max(120.0);
    let frame = egui::Frame::new()
        .fill(theme::CARD)
        .stroke(Stroke::new(1.0_f32, theme::BORDER))
        .corner_radius(CornerRadius::same(theme::CARD_RADIUS))
        .inner_margin(Margin::same(CARD_PAD as i8));

    let response = frame
        .show(ui, |ui| {
            ui.set_min_width(inner_width);
            add(ui);
        })
        .response;

    let rect = response.rect;
    ui.painter().line_segment(
        [
            rect.left_top() + vec2(18.0, 0.5),
            rect.right_top() + vec2(-18.0, 0.5),
        ],
        Stroke::new(
            1.0_f32,
            Color32::from_rgba_unmultiplied(0x5A, 0xA2, 0xFF, 26),
        ),
    );
    response
}

/// A card tinted slightly darker, used for nested panels.
pub fn soft_card(ui: &mut Ui, add: impl FnOnce(&mut Ui)) -> Response {
    let frame = egui::Frame::new()
        .fill(theme::CARD_SOFT)
        .stroke(Stroke::new(1.0_f32, theme::BORDER_SOFT))
        .corner_radius(CornerRadius::same(10))
        .inner_margin(Margin::same(12));
    frame.show(ui, add).response
}

pub fn section_title(ui: &mut Ui, title: &str, subtitle: Option<&str>) {
    ui.label(
        RichText::new(title)
            .font(theme::semibold(15.0))
            .color(theme::TEXT),
    );
    if let Some(subtitle) = subtitle {
        ui.add_space(2.0);
        ui.label(
            RichText::new(subtitle)
                .font(theme::body(12.0))
                .color(theme::MUTED),
        );
    }
}

pub fn divider(ui: &mut Ui) {
    let width = ui.available_width();
    let (rect, _) = ui.allocate_exact_size(vec2(width, 1.0), Sense::hover());
    ui.painter().line_segment(
        [rect.left_center(), rect.right_center()],
        Stroke::new(1.0_f32, theme::BORDER_SOFT),
    );
}

pub fn dot(painter: &egui::Painter, center: egui::Pos2, radius: f32, color: Color32) {
    painter.circle_filled(center, radius * 2.4, theme::alpha(color, 28));
    painter.circle_filled(center, radius, color);
}

pub fn pill(ui: &mut Ui, text: &str, color: Color32) -> Response {
    let font = theme::medium(11.0);
    let galley = ui.fonts(|fonts| fonts.layout_no_wrap(text.to_owned(), font, color));
    let size = vec2(galley.size().x + 18.0, 22.0);
    let (rect, response) = ui.allocate_exact_size(size, Sense::hover());
    let painter = ui.painter();
    painter.rect_filled(rect, CornerRadius::same(11), theme::alpha(color, 24));
    painter.rect_stroke(
        rect,
        CornerRadius::same(11),
        Stroke::new(1.0_f32, theme::alpha(color, 70)),
        StrokeKind::Inside,
    );
    painter.text(
        rect.center(),
        Align2::CENTER_CENTER,
        text,
        theme::medium(11.0),
        color,
    );
    response
}

#[derive(Clone, Copy)]
pub struct ButtonVisual {
    pub fill: Color32,
    pub hover: Color32,
    pub text: Color32,
    pub border: Color32,
    pub sheen: f32,
    pub shadow: Option<Color32>,
}

pub fn primary_visual() -> ButtonVisual {
    ButtonVisual {
        fill: theme::ACCENT_DEEP,
        hover: theme::ACCENT,
        text: Color32::WHITE,
        border: theme::ACCENT_BRIGHT,
        sheen: 10.0,
        shadow: Some(theme::ACCENT),
    }
}

pub fn ghost_visual() -> ButtonVisual {
    ButtonVisual {
        fill: theme::CARD_SOFT,
        hover: theme::CARD_HOVER,
        text: theme::MUTED,
        border: theme::BORDER,
        sheen: 0.0,
        shadow: None,
    }
}

pub fn danger_visual() -> ButtonVisual {
    ButtonVisual {
        fill: Color32::from_rgb(0x2A, 0x11, 0x18),
        hover: Color32::from_rgb(0x3C, 0x18, 0x20),
        text: theme::DANGER,
        border: theme::alpha(theme::DANGER, 120),
        sheen: 0.0,
        shadow: None,
    }
}

pub fn subtle_visual() -> ButtonVisual {
    ButtonVisual {
        fill: Color32::TRANSPARENT,
        hover: theme::CARD_HOVER,
        text: theme::MUTED,
        border: Color32::TRANSPARENT,
        sheen: 0.0,
        shadow: None,
    }
}

/// Painted button. `width` = 0 auto-sizes to the label.
pub fn button(
    ui: &mut Ui,
    text: &str,
    visual: ButtonVisual,
    width: f32,
    height: f32,
    enabled: bool,
) -> Response {
    let font: FontId = theme::medium(13.5);
    let galley = ui.fonts(|fonts| fonts.layout_no_wrap(text.to_owned(), font.clone(), visual.text));
    let desired = vec2(width.max(galley.size().x + 34.0), height);
    let sense = if enabled {
        Sense::click()
    } else {
        Sense::hover()
    };
    let (rect, response) = ui.allocate_exact_size(desired, sense);
    let hover = if enabled {
        ui.ctx()
            .animate_bool_with_time(response.id.with("hover"), response.hovered(), 0.12)
    } else {
        0.0
    };

    if ui.is_rect_visible(rect) {
        let painter = ui.painter();
        let radius = CornerRadius::same(RADIUS);
        let fill = if enabled {
            theme::mix(visual.fill, visual.hover, hover)
        } else {
            theme::CARD_SOFT
        };

        if enabled {
            if let Some(shadow) = visual.shadow {
                painter.rect_filled(
                    rect.translate(vec2(0.0, 2.0)).expand(1.0),
                    CornerRadius::same(RADIUS + 1),
                    theme::alpha(shadow, (16.0 + 30.0 * hover) as u8),
                );
            }
        }

        painter.rect_filled(rect, radius, fill);

        if visual.sheen > 0.0 {
            let sheen = Rect::from_min_max(rect.min, pos2(rect.right(), rect.center().y));
            painter.rect_filled(
                sheen,
                radius,
                Color32::from_rgba_unmultiplied(255, 255, 255, visual.sheen as u8),
            );
        }

        if enabled {
            painter.line_segment(
                [
                    rect.left_top() + vec2(12.0, 0.6),
                    rect.right_top() + vec2(-12.0, 0.6),
                ],
                Stroke::new(1.0_f32, Color32::from_rgba_unmultiplied(255, 255, 255, 34)),
            );
        }

        painter.rect_stroke(
            rect,
            radius,
            Stroke::new(
                1.0_f32,
                if enabled {
                    visual.border
                } else {
                    theme::BORDER_SOFT
                },
            ),
            StrokeKind::Inside,
        );

        painter.text(
            rect.center(),
            Align2::CENTER_CENTER,
            text,
            font,
            if enabled { visual.text } else { theme::DIM },
        );
        let _ = galley;
    }

    if enabled && response.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    response
}

pub fn checkbox(ui: &mut Ui, checked: bool) -> Response {
    let (rect, response) = ui.allocate_exact_size(Vec2::splat(20.0), Sense::click());
    let hover = ui
        .ctx()
        .animate_bool_with_time(response.id.with("check"), response.hovered(), 0.1);
    let radius = CornerRadius::same(6);
    let painter = ui.painter();

    if checked {
        painter.rect_filled(
            rect,
            radius,
            theme::mix(theme::ACCENT_DEEP, theme::ACCENT, 0.35 + 0.65 * hover),
        );
        painter.rect_stroke(
            rect,
            radius,
            Stroke::new(1.0_f32, theme::ACCENT_BRIGHT),
            StrokeKind::Inside,
        );
        let inner = rect.shrink(5.0);
        painter.add(Shape::line(
            vec![
                pos2(inner.left(), inner.center().y),
                pos2(inner.left() + inner.width() * 0.35, inner.bottom()),
                pos2(inner.right(), inner.top()),
            ],
            Stroke::new(1.8_f32, Color32::WHITE),
        ));
    } else {
        painter.rect_filled(rect, radius, theme::CARD_SOFT);
        painter.rect_stroke(
            rect,
            radius,
            Stroke::new(
                1.0_f32,
                theme::mix(theme::BORDER, theme::ACCENT, 0.4 * hover),
            ),
            StrokeKind::Inside,
        );
    }

    if response.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    response
}

/// A full-width navigation row with an icon and an active accent bar.
pub fn nav_item(ui: &mut Ui, glyph: Glyph, label: &str, active: bool) -> Response {
    let width = ui.available_width();
    let (rect, response) = ui.allocate_exact_size(vec2(width, 38.0), Sense::click());
    let hover = ui
        .ctx()
        .animate_bool_with_time(response.id.with("nav"), response.hovered(), 0.12);
    let painter = ui.painter();

    let fill = if active {
        theme::alpha(theme::ACCENT, 26)
    } else if hover > 0.0 {
        theme::alpha(theme::CARD_HOVER, (40.0 * hover) as u8)
    } else {
        Color32::TRANSPARENT
    };
    painter.rect_filled(rect, CornerRadius::same(9), fill);

    if active {
        let bar = Rect::from_min_size(
            pos2(rect.left() + 1.0, rect.center().y - 9.0),
            vec2(2.5, 18.0),
        );
        painter.rect_filled(bar, CornerRadius::same(2), theme::ACCENT_BRIGHT);
    }

    let color = if active {
        theme::TEXT
    } else {
        theme::mix(theme::MUTED, theme::TEXT, hover)
    };
    let glyph_rect =
        Rect::from_center_size(pos2(rect.left() + 24.0, rect.center().y), Vec2::splat(17.0));
    icon::paint(
        painter,
        glyph_rect,
        glyph,
        if active { theme::ACCENT_BRIGHT } else { color },
        1.5,
    );

    painter.text(
        pos2(rect.left() + 42.0, rect.center().y),
        Align2::LEFT_CENTER,
        label,
        if active {
            theme::medium(13.5)
        } else {
            theme::body(13.5)
        },
        color,
    );

    if response.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    response
}

/// Metric card used on the dashboard grid.
pub struct Metric<'a> {
    pub label: &'a str,
    pub value: String,
    pub sub: String,
    pub tone: Color32,
    pub tag: Option<&'a str>,
}

pub fn metric_card(ui: &mut Ui, width: f32, metric: &Metric<'_>) {
    ui.allocate_ui_with_layout(vec2(width, 0.0), Layout::top_down(Align::Min), |ui| {
        ui.set_width(width);
        card(ui, |ui| {
            ui.set_min_height(84.0);
            ui.horizontal(|ui| {
                ui.label(
                    RichText::new(metric.label.to_uppercase())
                        .font(theme::medium(10.0))
                        .color(theme::DIM),
                );
                if let Some(tag) = metric.tag {
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        pill(ui, tag, metric.tone);
                    });
                }
            });
            ui.add_space(8.0);
            ui.label(
                RichText::new(metric.value.as_str())
                    .font(theme::semibold(29.0))
                    .color(metric.tone),
            );
            if !metric.sub.is_empty() {
                ui.add_space(1.0);
                ui.label(
                    RichText::new(metric.sub.as_str())
                        .font(theme::body(11.5))
                        .color(theme::MUTED),
                );
            }
        });
    });
}

/// Overlaid sparklines for CPU and memory history.
pub fn sparkline(ui: &mut Ui, series: &[(&[f32], Color32)], height: f32) -> Response {
    let width = ui.available_width();
    let (rect, response) = ui.allocate_exact_size(vec2(width, height), Sense::hover());
    let painter = ui.painter();

    painter.rect_filled(
        rect,
        CornerRadius::same(9),
        Color32::from_rgb(0x07, 0x0D, 0x18),
    );
    painter.rect_stroke(
        rect,
        CornerRadius::same(9),
        Stroke::new(1.0_f32, theme::BORDER_SOFT),
        StrokeKind::Inside,
    );

    for step in 1..4 {
        let y = rect.top() + rect.height() * step as f32 / 4.0;
        painter.line_segment(
            [pos2(rect.left() + 10.0, y), pos2(rect.right() - 10.0, y)],
            Stroke::new(
                1.0_f32,
                Color32::from_rgba_unmultiplied(0x22, 0x33, 0x4A, 90),
            ),
        );
    }

    let inner = rect.shrink2(vec2(10.0, 12.0));
    for (values, color) in series {
        if values.len() < 2 {
            continue;
        }
        let points: Vec<egui::Pos2> = values
            .iter()
            .enumerate()
            .map(|(index, value)| {
                let x =
                    inner.left() + inner.width() * index as f32 / (values.len().max(2) - 1) as f32;
                let y = inner.bottom() - inner.height() * (value.clamp(0.0, 100.0) / 100.0);
                pos2(x, y)
            })
            .collect();

        let mut mesh = Mesh::default();
        for point in &points {
            mesh.colored_vertex(*point, theme::alpha(*color, 34));
        }
        let base = points.len() as u32;
        mesh.colored_vertex(pos2(inner.left(), inner.bottom()), theme::alpha(*color, 2));
        mesh.colored_vertex(pos2(inner.right(), inner.bottom()), theme::alpha(*color, 2));
        for index in 0..(points.len() as u32 - 1) {
            mesh.add_triangle(index, index + 1, base);
            mesh.add_triangle(index + 1, base + 1, base);
        }
        painter.add(Shape::mesh(mesh));
        painter.add(Shape::line(points.clone(), Stroke::new(1.5_f32, *color)));
        if let Some(last) = points.last() {
            painter.circle_filled(*last, 4.5, theme::alpha(*color, 40));
            painter.circle_filled(*last, 2.2, *color);
        }
    }

    response
}

/// Thin progress track, used for the optimisation score.
pub fn progress(ui: &mut Ui, fraction: f32, color: Color32) {
    let width = ui.available_width();
    let (rect, _) = ui.allocate_exact_size(vec2(width, 6.0), Sense::hover());
    let radius = CornerRadius::same(3);
    ui.painter().rect_filled(rect, radius, theme::CARD_SOFT);
    let filled = Rect::from_min_size(
        rect.min,
        vec2(rect.width() * fraction.clamp(0.0, 1.0), rect.height()),
    );
    if filled.width() > 1.0 {
        ui.painter().rect_filled(filled, radius, color);
    }
}

/// Rotating arc used while a job is running.
pub fn spinner(ui: &Ui, center: egui::Pos2, radius: f32, color: Color32) {
    let time = ui.ctx().input(|input| input.time) as f32;
    let start = time * 4.2;
    let mut points = Vec::new();
    for step in 0..=18 {
        let angle = start + std::f32::consts::TAU * 0.7 * step as f32 / 18.0;
        points.push(pos2(
            center.x + radius * angle.cos(),
            center.y + radius * angle.sin(),
        ));
    }
    ui.painter()
        .add(Shape::line(points, Stroke::new(1.8_f32, color)));
}

/// Keys on the left, values on the right, for detail lists.
pub fn key_value(ui: &mut Ui, label: &str, value: &str, tone: Color32) {
    ui.horizontal(|ui| {
        ui.label(
            RichText::new(label)
                .font(theme::body(12.0))
                .color(theme::DIM),
        );
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            ui.label(RichText::new(value).font(theme::medium(12.0)).color(tone));
        });
    });
}

/// Wide gradient wash painted behind the content area.
pub fn corner_glow(painter: &egui::Painter, rect: Rect) {
    let size = vec2(880.0, 460.0);
    let origin = pos2(rect.right() - size.x, rect.top() - 40.0);
    let strong = Color32::from_rgba_unmultiplied(0x1C, 0x4A, 0x9C, 46);
    let mut mesh = Mesh::default();
    mesh.colored_vertex(pos2(origin.x, origin.y), strong);
    mesh.colored_vertex(
        pos2(origin.x + size.x, origin.y),
        theme::alpha(theme::ACCENT, 26),
    );
    mesh.colored_vertex(
        pos2(origin.x + size.x, origin.y + size.y),
        Color32::TRANSPARENT,
    );
    mesh.colored_vertex(pos2(origin.x, origin.y + size.y), Color32::TRANSPARENT);
    mesh.add_triangle(0, 1, 2);
    mesh.add_triangle(0, 2, 3);
    painter.add(Shape::mesh(mesh));
}
