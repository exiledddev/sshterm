//! Hand-drawn vector icons.
//!
//! Everything is painted with egui primitives so the application needs no
//! external icon font or image assets and stays crisp at any DPI.

use eframe::egui::{self, Color32, CornerRadius, Painter, Pos2, Rect, Shape, Stroke, Vec2};

/// Signature shared by every icon painter.
pub type Icon = fn(&Painter, Rect, Color32, f32);

fn p(r: Rect, x: f32, y: f32) -> Pos2 {
    Pos2::new(r.min.x + r.width() * x, r.min.y + r.height() * y)
}

fn line(painter: &Painter, pts: Vec<Pos2>, c: Color32, w: f32) {
    painter.add(Shape::line(pts, Stroke::new(w, c)));
}

fn poly(painter: &Painter, pts: Vec<Pos2>, c: Color32, w: f32) {
    let mut pts = pts;
    if let Some(first) = pts.first().copied() {
        pts.push(first);
    }
    painter.add(Shape::line(pts, Stroke::new(w, c)));
}

/// `+` — New connection.
pub fn plus(painter: &Painter, r: Rect, c: Color32, w: f32) {
    line(painter, vec![p(r, 0.5, 0.16), p(r, 0.5, 0.84)], c, w);
    line(painter, vec![p(r, 0.16, 0.5), p(r, 0.84, 0.5)], c, w);
}

/// Power symbol — Terminate session.
pub fn power(painter: &Painter, r: Rect, c: Color32, w: f32) {
    let center = p(r, 0.5, 0.56);
    let radius = r.width() * 0.34;
    // Circle with a gap at the top.
    let mut pts = Vec::new();
    let start = -std::f32::consts::FRAC_PI_2 + 0.62;
    let end = start + std::f32::consts::TAU - 1.24;
    let steps = 28;
    for i in 0..=steps {
        let a = start + (end - start) * i as f32 / steps as f32;
        pts.push(center + Vec2::new(a.cos(), a.sin()) * radius);
    }
    line(painter, pts, c, w);
    line(painter, vec![p(r, 0.5, 0.1), p(r, 0.5, 0.46)], c, w);
}

/// Folder — Open connections folder.
pub fn folder(painter: &Painter, r: Rect, c: Color32, w: f32) {
    poly(
        painter,
        vec![
            p(r, 0.1, 0.78),
            p(r, 0.1, 0.24),
            p(r, 0.42, 0.24),
            p(r, 0.52, 0.38),
            p(r, 0.9, 0.38),
            p(r, 0.9, 0.78),
        ],
        c,
        w,
    );
}

/// Left panel toggle.
pub fn panel_left(painter: &Painter, r: Rect, c: Color32, w: f32) {
    let body = Rect::from_min_max(p(r, 0.06, 0.16), p(r, 0.94, 0.84));
    painter.rect_stroke(
        body,
        CornerRadius::same(2),
        Stroke::new(w, c),
        egui::StrokeKind::Middle,
    );
    let split = body.min.x + body.width() * 0.38;
    line(
        painter,
        vec![
            Pos2::new(split, body.min.y),
            Pos2::new(split, body.max.y),
        ],
        c,
        w,
    );
    painter.rect_filled(
        Rect::from_min_max(body.min, Pos2::new(split, body.max.y)),
        CornerRadius {
            nw: 2,
            sw: 2,
            ne: 0,
            se: 0,
        },
        c.gamma_multiply(0.5),
    );
}

/// Right panel toggle.
pub fn panel_right(painter: &Painter, r: Rect, c: Color32, w: f32) {
    let body = Rect::from_min_max(p(r, 0.06, 0.16), p(r, 0.94, 0.84));
    painter.rect_stroke(
        body,
        CornerRadius::same(2),
        Stroke::new(w, c),
        egui::StrokeKind::Middle,
    );
    let split = body.min.x + body.width() * 0.62;
    line(
        painter,
        vec![
            Pos2::new(split, body.min.y),
            Pos2::new(split, body.max.y),
        ],
        c,
        w,
    );
    painter.rect_filled(
        Rect::from_min_max(Pos2::new(split, body.min.y), body.max),
        CornerRadius {
            nw: 0,
            sw: 0,
            ne: 2,
            se: 2,
        },
        c.gamma_multiply(0.5),
    );
}

/// Filled play triangle — Start a saved connection.
pub fn play(painter: &Painter, r: Rect, c: Color32, _w: f32) {
    painter.add(Shape::convex_polygon(
        vec![p(r, 0.3, 0.16), p(r, 0.82, 0.5), p(r, 0.3, 0.84)],
        c,
        Stroke::NONE,
    ));
}

/// Pencil — Edit a saved connection.
pub fn pencil(painter: &Painter, r: Rect, c: Color32, w: f32) {
    line(painter, vec![p(r, 0.22, 0.78), p(r, 0.72, 0.2)], c, w);
    line(painter, vec![p(r, 0.5, 0.14), p(r, 0.82, 0.36)], c, w);
    line(painter, vec![p(r, 0.72, 0.2), p(r, 0.82, 0.36)], c, w);
    line(painter, vec![p(r, 0.22, 0.78), p(r, 0.16, 0.86)], c, w);
    line(painter, vec![p(r, 0.16, 0.86), p(r, 0.3, 0.84)], c, w);
}

/// Key — private key field.
pub fn key(painter: &Painter, r: Rect, c: Color32, w: f32) {
    // Bow on the left, shaft running right, two teeth hanging off the end.
    painter.circle_stroke(p(r, 0.27, 0.5), r.width() * 0.16, Stroke::new(w, c));
    line(painter, vec![p(r, 0.43, 0.5), p(r, 0.9, 0.5)], c, w);
    line(painter, vec![p(r, 0.66, 0.5), p(r, 0.66, 0.71)], c, w);
    line(painter, vec![p(r, 0.81, 0.5), p(r, 0.81, 0.66)], c, w);
}

/// Server stack — remote host.
pub fn server(painter: &Painter, r: Rect, c: Color32, w: f32) {
    for (top, bottom) in [(0.14_f32, 0.42_f32), (0.58, 0.86)] {
        painter.rect_stroke(
            Rect::from_min_max(p(r, 0.12, top), p(r, 0.88, bottom)),
            CornerRadius::same(3),
            Stroke::new(w, c),
            egui::StrokeKind::Middle,
        );
        painter.circle_filled(p(r, 0.24, (top + bottom) / 2.0), w * 0.85, c);
    }
}

/// Terminal chevron `>_` — used for the local shell and the app mark.
pub fn terminal(painter: &Painter, r: Rect, c: Color32, w: f32) {
    line(
        painter,
        vec![p(r, 0.2, 0.28), p(r, 0.46, 0.5), p(r, 0.2, 0.72)],
        c,
        w,
    );
    line(painter, vec![p(r, 0.56, 0.74), p(r, 0.82, 0.74)], c, w);
}

/// Circular arrow — refresh.
pub fn refresh(painter: &Painter, r: Rect, c: Color32, w: f32) {
    let center = p(r, 0.5, 0.5);
    let radius = r.width() * 0.32;
    let mut pts = Vec::new();
    let start = -0.5;
    let end = start + std::f32::consts::TAU - 1.1;
    for i in 0..=26 {
        let a = start + (end - start) * i as f32 / 26.0;
        pts.push(center + Vec2::new(a.cos(), a.sin()) * radius);
    }
    let tip = *pts.last().unwrap();
    line(painter, pts, c, w);
    painter.add(Shape::convex_polygon(
        vec![
            tip + Vec2::new(-radius * 0.34, -radius * 0.1),
            tip + Vec2::new(radius * 0.16, -radius * 0.42),
            tip + Vec2::new(radius * 0.2, radius * 0.24),
        ],
        c,
        Stroke::NONE,
    ));
}

/// Shield — host key / fingerprint.
pub fn shield(painter: &Painter, r: Rect, c: Color32, w: f32) {
    poly(
        painter,
        vec![
            p(r, 0.5, 0.12),
            p(r, 0.84, 0.26),
            p(r, 0.78, 0.62),
            p(r, 0.5, 0.88),
            p(r, 0.22, 0.62),
            p(r, 0.16, 0.26),
        ],
        c,
        w,
    );
}

/// Info circle.
pub fn info(painter: &Painter, r: Rect, c: Color32, w: f32) {
    painter.circle_stroke(p(r, 0.5, 0.5), r.width() * 0.36, Stroke::new(w, c));
    painter.circle_filled(p(r, 0.5, 0.3), w * 0.8, c);
    line(painter, vec![p(r, 0.5, 0.45), p(r, 0.5, 0.72)], c, w);
}

/// Window minimise.
pub fn win_min(painter: &Painter, r: Rect, c: Color32, w: f32) {
    line(painter, vec![p(r, 0.24, 0.55), p(r, 0.76, 0.55)], c, w);
}

/// Window maximise / restore.
pub fn win_max(painter: &Painter, r: Rect, c: Color32, w: f32) {
    painter.rect_stroke(
        Rect::from_min_max(p(r, 0.26, 0.26), p(r, 0.74, 0.74)),
        CornerRadius::same(2),
        Stroke::new(w, c),
        egui::StrokeKind::Middle,
    );
}

/// Window close.
pub fn win_close(painter: &Painter, r: Rect, c: Color32, w: f32) {
    line(painter, vec![p(r, 0.28, 0.28), p(r, 0.72, 0.72)], c, w);
    line(painter, vec![p(r, 0.72, 0.28), p(r, 0.28, 0.72)], c, w);
}

/// Draws the application mark: a monochrome rounded tile carrying a `>_`
/// prompt. Deliberately greyscale, so it sits quietly in a task bar or title
/// bar next to whatever else is there.
pub fn app_mark(painter: &Painter, rect: Rect) {
    let cr = CornerRadius::same((rect.width() * 0.26) as u8);
    painter.rect_filled(rect, cr, Color32::from_rgb(0x1C, 0x1D, 0x24));

    // A faint light falloff from the top, so the tile is not flat.
    let bands = 8;
    for i in 0..bands {
        let t = i as f32 / bands as f32;
        let band = Rect::from_min_max(
            Pos2::new(rect.min.x, rect.min.y + rect.height() * t),
            Pos2::new(
                rect.max.x,
                rect.min.y + rect.height() * (t + 1.0 / bands as f32),
            ),
        );
        painter.rect_filled(
            band,
            if i == 0 { cr } else { CornerRadius::ZERO },
            Color32::from_rgba_unmultiplied(0xFF, 0xFF, 0xFF, ((1.0 - t) * 16.0) as u8),
        );
    }

    painter.rect_stroke(
        rect,
        cr,
        Stroke::new(1.0, Color32::from_rgba_unmultiplied(0xFF, 0xFF, 0xFF, 0x5A)),
        egui::StrokeKind::Inside,
    );
    terminal(
        painter,
        rect.shrink(rect.width() * 0.16),
        Color32::from_rgb(0xEC, 0xEE, 0xF5),
        (rect.width() * 0.085).max(1.2),
    );
}

/// An eye with a stroke through it — the censor toggle.
pub fn eye_off(painter: &Painter, r: Rect, c: Color32, w: f32) {
    // The eye outline, drawn as two arcs meeting at the corners.
    let (left, right) = (p(r, 0.12, 0.5), p(r, 0.88, 0.5));
    for dir in [-1.0_f32, 1.0] {
        let mut pts = Vec::new();
        for i in 0..=14 {
            let t = i as f32 / 14.0;
            let x = left.x + (right.x - left.x) * t;
            let y = left.y + dir * (t * std::f32::consts::PI).sin() * r.height() * 0.26;
            pts.push(egui::Pos2::new(x, y));
        }
        line(painter, pts, c, w);
    }
    painter.circle_stroke(p(r, 0.5, 0.5), r.width() * 0.13, Stroke::new(w, c));
    line(painter, vec![p(r, 0.18, 0.84), p(r, 0.82, 0.16)], c, w * 1.1);
}

/// Renders the application icon as RGBA pixels, so no binary asset is needed.
///
/// Monochrome by design: a grey rounded tile with a light rim and a white
/// `>_` prompt, which sits quietly in a task bar next to anything else.
pub fn app_icon_rgba(size: u32) -> Vec<u8> {
    let s = size as f32;
    let radius = s * 0.24;
    let mut rgba = vec![0u8; (size * size * 4) as usize];

    /// One stroke of the icon glyph: two endpoints and a half-width, all in
    /// unit space so the icon can be rendered at any size.
    struct Stroke {
        a: (f32, f32),
        b: (f32, f32),
        half_width: f32,
    }

    // The `>` chevron and the `_` bar.
    let strokes = [
        Stroke { a: (0.30, 0.32), b: (0.48, 0.50), half_width: 0.055 },
        Stroke { a: (0.48, 0.50), b: (0.30, 0.68), half_width: 0.055 },
        Stroke { a: (0.56, 0.70), b: (0.74, 0.70), half_width: 0.055 },
    ];

    for y in 0..size {
        for x in 0..size {
            let px = x as f32 + 0.5;
            let py = y as f32 + 0.5;

            // Rounded-rectangle coverage.
            let dx = (px - s / 2.0).abs() - (s / 2.0 - radius);
            let dy = (py - s / 2.0).abs() - (s / 2.0 - radius);
            let outside = (dx.max(0.0).powi(2) + dy.max(0.0).powi(2)).sqrt()
                + dx.max(dy).min(0.0)
                - radius;
            let tile = (0.5 - outside).clamp(0.0, 1.0);
            if tile <= 0.0 {
                continue;
            }

            // Vertical grey gradient, lighter at the top.
            let t = py / s;
            let shade = icon_lerp(0x2A as f32, 0x14 as f32, t);
            let mut base = [shade, shade + 1.0, shade + 6.0];

            // A light rim just inside the edge, to lift it off dark panels.
            let rim = ((outside + 2.0) / 2.0).clamp(0.0, 1.0) * tile;
            for c in &mut base {
                *c = icon_lerp(*c, 0x8A as f32, rim * 0.55);
            }

            // Glyph coverage.
            let mut glyph = 0.0f32;
            for stroke in &strokes {
                let d = dist_to_segment(px / s, py / s, stroke.a, stroke.b);
                glyph = glyph.max(((stroke.half_width - d) / (1.5 / s)).clamp(0.0, 1.0));
            }

            let color = [
                icon_lerp(base[0], 0xF2 as f32, glyph),
                icon_lerp(base[1], 0xF4 as f32, glyph),
                icon_lerp(base[2], 0xFA as f32, glyph),
            ];

            let i = ((y * size + x) * 4) as usize;
            rgba[i] = color[0] as u8;
            rgba[i + 1] = color[1] as u8;
            rgba[i + 2] = color[2] as u8;
            rgba[i + 3] = (tile * 255.0) as u8;
        }
    }

    rgba
}

fn icon_lerp(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t.clamp(0.0, 1.0)
}

fn dist_to_segment(px: f32, py: f32, (ax, ay): (f32, f32), (bx, by): (f32, f32)) -> f32 {
    let (vx, vy) = (bx - ax, by - ay);
    let (wx, wy) = (px - ax, py - ay);
    let len2 = vx * vx + vy * vy;
    let t = if len2 <= f32::EPSILON {
        0.0
    } else {
        ((wx * vx + wy * vy) / len2).clamp(0.0, 1.0)
    };
    let (cx, cy) = (ax + vx * t, ay + vy * t);
    ((px - cx).powi(2) + (py - cy).powi(2)).sqrt()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_app_icon_is_monochrome() {
        let size = 64u32;
        let rgba = app_icon_rgba(size);
        assert_eq!(rgba.len(), (size * size * 4) as usize);

        let mut opaque = 0usize;
        let mut brightest = 0u8;
        for px in rgba.chunks_exact(4) {
            let (r, g, b, a) = (px[0], px[1], px[2], px[3]);
            if a == 0 {
                continue;
            }
            opaque += 1;
            // Every visible pixel is a grey: the channels stay within a hair
            // of each other, so the icon can never drift back to a colour.
            let spread = r.max(g).max(b) - r.min(g).min(b);
            assert!(spread <= 12, "coloured pixel {r},{g},{b} (spread {spread})");
            brightest = brightest.max(r);
        }

        // The rounded corners are cut away, and the glyph is nearly white.
        assert!(opaque > 0 && opaque < (size * size) as usize, "{opaque} opaque");
        assert!(brightest > 0xE0, "the >_ glyph should be near white, got {brightest}");
    }
}
