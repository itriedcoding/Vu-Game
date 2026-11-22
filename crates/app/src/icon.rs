//! Vector iconography. Everything is painted with the egui painter, so the UI has
//! no image assets to load and stays crisp at any DPI.

use eframe::egui::{
    self, pos2, Color32, CornerRadius, Painter, Pos2, Rect, Shape, Stroke, StrokeKind,
};

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Glyph {
    Gauge,
    Bolt,
    Sparkles,
    Chip,
    Check,
    Close,
}

/// The bolt outline, normalised to a 0..1 box.
pub const BOLT: [(f32, f32); 7] = [
    (0.60, 0.04),
    (0.24, 0.56),
    (0.46, 0.56),
    (0.38, 0.96),
    (0.76, 0.44),
    (0.53, 0.44),
    (0.60, 0.04),
];

pub fn bolt_shape(rect: Rect) -> Vec<Pos2> {
    BOLT.iter()
        .map(|(x, y)| {
            pos2(
                rect.left() + rect.width() * x,
                rect.top() + rect.height() * y,
            )
        })
        .collect()
}

pub fn paint(painter: &Painter, rect: Rect, glyph: Glyph, color: Color32, weight: f32) {
    let p = |x: f32, y: f32| {
        pos2(
            rect.left() + rect.width() * x,
            rect.top() + rect.height() * y,
        )
    };
    let stroke = Stroke::new(weight, color);

    match glyph {
        Glyph::Bolt => {
            painter.add(Shape::convex_polygon(bolt_shape(rect), color, Stroke::NONE));
        }
        Glyph::Gauge => {
            let center = p(0.5, 0.58);
            let radius = rect.width() * 0.36;
            let mut arc = Vec::new();
            for step in 0..=20 {
                let angle = std::f32::consts::PI * (0.78 + 1.44 * step as f32 / 20.0);
                arc.push(pos2(
                    center.x + radius * angle.cos(),
                    center.y - radius * angle.sin(),
                ));
            }
            painter.add(Shape::line(arc, stroke));
            painter.line_segment([center, p(0.74, 0.32)], stroke);
            painter.circle_filled(center, weight * 1.7, color);
        }
        Glyph::Sparkles => {
            let star = |center: Pos2, size: f32| {
                painter.line_segment(
                    [
                        pos2(center.x - size, center.y),
                        pos2(center.x + size, center.y),
                    ],
                    stroke,
                );
                painter.line_segment(
                    [
                        pos2(center.x, center.y - size),
                        pos2(center.x, center.y + size),
                    ],
                    stroke,
                );
            };
            star(p(0.42, 0.60), rect.width() * 0.20);
            star(p(0.74, 0.30), rect.width() * 0.11);
            painter.circle_filled(p(0.28, 0.30), weight * 0.9, color);
        }
        Glyph::Chip => {
            let pad = rect.width() * 0.24;
            let body = rect.shrink(pad);
            painter.rect_stroke(
                body,
                CornerRadius::same(2),
                Stroke::new(weight, color),
                StrokeKind::Inside,
            );
            painter.rect_filled(
                body.shrink(rect.width() * 0.07),
                CornerRadius::same(1),
                color,
            );
            for step in 1..4 {
                let x = body.left() + body.width() * step as f32 / 4.0;
                painter.line_segment(
                    [pos2(x, body.top() - pad * 0.62), pos2(x, body.top())],
                    stroke,
                );
                painter.line_segment(
                    [pos2(x, body.bottom()), pos2(x, body.bottom() + pad * 0.62)],
                    stroke,
                );
                let y = body.top() + body.height() * step as f32 / 4.0;
                painter.line_segment(
                    [pos2(body.left() - pad * 0.62, y), pos2(body.left(), y)],
                    stroke,
                );
                painter.line_segment(
                    [pos2(body.right(), y), pos2(body.right() + pad * 0.62, y)],
                    stroke,
                );
            }
        }
        Glyph::Check => {
            painter.add(Shape::line(
                vec![p(0.22, 0.52), p(0.42, 0.72), p(0.78, 0.28)],
                stroke,
            ));
        }
        Glyph::Close => {
            painter.line_segment([p(0.28, 0.28), p(0.72, 0.72)], stroke);
            painter.line_segment([p(0.72, 0.28), p(0.28, 0.72)], stroke);
        }
    }
}

/// Window/taskbar icon drawn from the same bolt geometry.
pub fn window_icon() -> egui::IconData {
    const SIZE: u32 = 64;
    let mut rgba = vec![0u8; (SIZE * SIZE * 4) as usize];

    let bolt = [
        (0.60f32, 0.06f32),
        (0.24, 0.56),
        (0.46, 0.56),
        (0.38, 0.94),
        (0.76, 0.44),
        (0.53, 0.44),
    ];

    for y in 0..SIZE {
        for x in 0..SIZE {
            let fx = (x as f32 + 0.5) / SIZE as f32;
            let fy = (y as f32 + 0.5) / SIZE as f32;

            // Rounded square background with a vertical accent gradient.
            let corner = 0.22f32;
            let inside = rounded_square(fx, fy, corner);
            if !inside {
                continue;
            }
            let t = fy;
            let r = lerp(0x1B as f32, 0x4F as f32, t);
            let g = lerp(0x4F as f32, 0x2F as f32, t);
            let b = lerp(0xC4 as f32, 0x8F as f32, t);

            let mut color = (r, g, b, 255.0);
            if point_in_polygon(fx, fy, &bolt) {
                color = (245.0, 250.0, 255.0, 255.0);
            }

            let index = ((y * SIZE + x) * 4) as usize;
            rgba[index] = color.0 as u8;
            rgba[index + 1] = color.1 as u8;
            rgba[index + 2] = color.2 as u8;
            rgba[index + 3] = color.3 as u8;
        }
    }

    egui::IconData {
        rgba,
        width: SIZE,
        height: SIZE,
    }
}

fn lerp(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t
}

fn rounded_square(x: f32, y: f32, radius: f32) -> bool {
    let cx = x.clamp(radius, 1.0 - radius);
    let cy = y.clamp(radius, 1.0 - radius);
    let dx = x - cx;
    let dy = y - cy;
    dx * dx + dy * dy <= radius * radius
}

fn point_in_polygon(x: f32, y: f32, polygon: &[(f32, f32)]) -> bool {
    let mut inside = false;
    let mut j = polygon.len() - 1;
    for i in 0..polygon.len() {
        let (xi, yi) = polygon[i];
        let (xj, yj) = polygon[j];
        if (yi > y) != (yj > y) && x < (xj - xi) * (y - yi) / (yj - yi) + xi {
            inside = !inside;
        }
        j = i;
    }
    inside
}

/// Small bolt glyph used next to the wordmark.
pub fn logo(painter: &Painter, rect: Rect) {
    painter.rect_filled(
        rect,
        egui::CornerRadius::same(11),
        Color32::from_rgb(0x11, 0x1E, 0x33),
    );
    painter.rect_stroke(
        rect,
        egui::CornerRadius::same(11),
        Stroke::new(
            1.0_f32,
            Color32::from_rgba_unmultiplied(0x5A, 0xA2, 0xFF, 60),
        ),
        egui::StrokeKind::Inside,
    );
    let glyph = rect.shrink(rect.width() * 0.26);
    paint(
        painter,
        glyph,
        Glyph::Bolt,
        Color32::from_rgb(0x6F, 0xB4, 0xFF),
        1.4,
    );
}
