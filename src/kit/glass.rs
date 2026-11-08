//! Liquid-glass drawing kit: translucent cards, gradients, glows, pills and the
//! small vector icons the whole app is drawn with (no bitmap assets).

use egui::{
    epaint::{Mesh, PathShape, PathStroke, Shadow},
    Align2, Color32, CornerRadius, FontFamily, FontId, Painter, Pos2, Rect, Response, Sense,
    Shape, Stroke, StrokeKind, Ui, Vec2,
};
use std::f32::consts::{PI, TAU};

// ---------------------------------------------------------------- palette

pub const TEXT: Color32 = Color32::from_rgb(250, 250, 252);
pub const TEXT_DIM: Color32 = Color32::from_rgba_premultiplied(172, 172, 176, 176);
pub const TEXT_FAINT: Color32 = Color32::from_rgba_premultiplied(112, 112, 116, 116);
// Apple system colours (dark appearance)
pub const BLUE: Color32 = Color32::from_rgb(10, 132, 255);
pub const ACCENT: Color32 = Color32::from_rgb(100, 170, 255); // links
pub const CYAN: Color32 = Color32::from_rgb(48, 209, 88); // "Good" (system green)
pub const AMBER: Color32 = Color32::from_rgb(255, 159, 10);
pub const RED: Color32 = Color32::from_rgb(255, 69, 58);
pub const GREEN: Color32 = Color32::from_rgb(48, 209, 88);
pub const TEAL: Color32 = Color32::from_rgb(100, 210, 255);
pub const MINT: Color32 = Color32::from_rgb(99, 230, 226);
pub const GRAY: Color32 = Color32::from_rgb(152, 152, 157);
pub const PINK: Color32 = AMBER;
pub const VIOLET: Color32 = BLUE;

pub fn white(a: u8) -> Color32 {
    Color32::from_white_alpha(a)
}

pub fn with_alpha(c: Color32, a: f32) -> Color32 {
    let [r, g, b, _] = c.to_srgba_unmultiplied();
    Color32::from_rgba_unmultiplied(r, g, b, (a.clamp(0.0, 1.0) * 255.0) as u8)
}

pub fn lerp_color(a: Color32, b: Color32, t: f32) -> Color32 {
    let t = t.clamp(0.0, 1.0);
    let a = a.to_srgba_unmultiplied();
    let b = b.to_srgba_unmultiplied();
    let m = |i: usize| (a[i] as f32 + (b[i] as f32 - a[i] as f32) * t).round() as u8;
    Color32::from_rgba_unmultiplied(m(0), m(1), m(2), m(3))
}

// ---------------------------------------------------------------- fonts

pub fn font(size: f32) -> FontId {
    FontId::new(size, FontFamily::Proportional)
}

pub fn bold(size: f32) -> FontId {
    FontId::new(size, FontFamily::Name("bold".into()))
}

pub fn mono(size: f32) -> FontId {
    FontId::new(size, FontFamily::Monospace)
}

// ---------------------------------------------------------------- shapes

/// Points of a rounded rectangle outline, clockwise from the top-left arc.
pub fn rounded_points(rect: Rect, r: f32) -> Vec<Pos2> {
    let r = r.min(rect.width() / 2.0).min(rect.height() / 2.0).max(0.0);
    let mut pts = Vec::with_capacity(4 * 9);
    let corners = [
        (Pos2::new(rect.left() + r, rect.top() + r), PI),
        (Pos2::new(rect.right() - r, rect.top() + r), 1.5 * PI),
        (Pos2::new(rect.right() - r, rect.bottom() - r), 0.0),
        (Pos2::new(rect.left() + r, rect.bottom() - r), 0.5 * PI),
    ];
    let steps = 8;
    for (c, start) in corners {
        for i in 0..=steps {
            let a = start + (i as f32 / steps as f32) * (PI / 2.0);
            pts.push(Pos2::new(c.x + r * a.cos(), c.y + r * a.sin()));
        }
    }
    pts
}

/// A rounded rectangle filled with a vertical gradient.
pub fn gradient_rect(painter: &Painter, rect: Rect, r: f32, top: Color32, bottom: Color32) {
    gradient_rect_dir(painter, rect, r, top, bottom, Vec2::new(0.0, 1.0));
}

/// A rounded rectangle filled with a linear gradient along `dir`.
pub fn gradient_rect_dir(
    painter: &Painter,
    rect: Rect,
    r: f32,
    from: Color32,
    to: Color32,
    dir: Vec2,
) {
    let pts = rounded_points(rect, r);
    let dir = dir.normalized();
    let half = (rect.width() * dir.x.abs() + rect.height() * dir.y.abs()) / 2.0;
    let center = rect.center();
    let t_of = |p: Pos2| ((p - center).dot(dir) / half.max(1.0) + 1.0) / 2.0;
    let mut mesh = Mesh::default();
    mesh.colored_vertex(center, lerp_color(from, to, 0.5));
    for p in &pts {
        mesh.colored_vertex(*p, lerp_color(from, to, t_of(*p)));
    }
    let n = pts.len() as u32;
    for i in 0..n {
        mesh.add_triangle(0, 1 + i, 1 + (i + 1) % n);
    }
    painter.add(Shape::mesh(mesh));
}

/// Soft radial glow (a coloured light behind the glass).
pub fn glow(painter: &Painter, center: Pos2, radius: f32, color: Color32) {
    let mut mesh = Mesh::default();
    let rings = [(0.0, 1.0), (0.35, 0.75), (0.65, 0.38), (1.0, 0.0)];
    let segs = 48u32;
    mesh.colored_vertex(center, color);
    let mut ring_start = Vec::new();
    for (k, &(rf, af)) in rings.iter().enumerate().skip(1) {
        ring_start.push(mesh.vertices.len() as u32);
        for i in 0..segs {
            let a = i as f32 / segs as f32 * TAU;
            let p = center + Vec2::new(a.cos(), a.sin()) * radius * rf;
            mesh.colored_vertex(p, with_alpha(color, color.a() as f32 / 255.0 * af));
        }
        let _ = k;
    }
    // centre fan
    let first = ring_start[0];
    for i in 0..segs {
        mesh.add_triangle(0, first + i, first + (i + 1) % segs);
    }
    for w in ring_start.windows(2) {
        let (a, b) = (w[0], w[1]);
        for i in 0..segs {
            let j = (i + 1) % segs;
            mesh.add_triangle(a + i, b + i, b + j);
            mesh.add_triangle(a + i, b + j, a + j);
        }
    }
    painter.add(Shape::mesh(mesh));
}

/// The window surface. Live, the system's Liquid Glass sits behind it, so we only
/// add a faint dark veil and the glass edge. In screenshot mode there's no
/// desktop to blur, so a blurred neutral wallpaper is painted first.
pub fn backdrop(painter: &Painter, rect: Rect, radius: f32, opaque: bool, _t: f64) {
    if opaque {
        let clip = painter.with_clip_rect(rect);
        // stand-in for a blurred desktop (soft, desaturated colour fields)
        gradient_rect_dir(painter, rect, radius, Color32::from_rgb(38, 46, 52), Color32::from_rgb(56, 60, 48), Vec2::new(0.4, 1.0));
        let w = rect.width();
        let h = rect.height();
        glow(&clip, rect.left_top() + Vec2::new(w * 0.15, h * 0.12), w.max(h) * 0.55, Color32::from_rgba_unmultiplied(70, 110, 130, 150));
        glow(&clip, rect.left_top() + Vec2::new(w * 0.9, h * 0.35), w.max(h) * 0.5, Color32::from_rgba_unmultiplied(120, 120, 80, 120));
        glow(&clip, rect.left_top() + Vec2::new(w * 0.4, h * 0.95), w.max(h) * 0.55, Color32::from_rgba_unmultiplied(40, 70, 110, 140));
        // the glass itself: a dark frosted veil
        painter.rect_filled(rect, radius, Color32::from_rgba_unmultiplied(18, 18, 22, 120));
    } else {
        painter.rect_filled(rect, radius, Color32::from_rgba_unmultiplied(0, 0, 0, 40));
    }
    rim(painter, rect, radius, 1.0);
}

/// A clear glass card. `hover` 0..1 brightens it, `selected` gives the blue ring.
pub fn glass_card(painter: &Painter, rect: Rect, r: f32, hover: f32, selected: bool) {
    let shadow = Shadow { offset: [0, 4], blur: 16, spread: 0, color: Color32::from_black_alpha(36) };
    painter.add(shadow.as_shape(rect, CornerRadius::same(r as u8)));
    gradient_rect(painter, rect, r, white((22.0 + 16.0 * hover) as u8), white((10.0 + 10.0 * hover) as u8));
    rim(painter, rect, r, 0.75 + 0.25 * hover);
    if selected {
        painter.rect_stroke(rect, r, Stroke::new(2.0, BLUE), StrokeKind::Inside);
    }
}
/// A glass card that reacts to the mouse. Returns its response.
pub fn card(ui: &mut Ui, rect: Rect, id: egui::Id, r: f32, selected: bool) -> Response {
    let resp = ui.interact(rect, id, Sense::click());
    let hover = ui.ctx().animate_bool(id.with("hover"), resp.hovered());
    glass_card(ui.painter(), rect, r, hover, selected);
    resp
}

/// Capsule button in the Liquid Glass style. `kind`: 0 = clear glass,
/// 1 = tinted with the system accent, 2 = destructive (red tint).
pub fn pill_button(ui: &mut Ui, rect: Rect, label: &str, kind: u8, enabled: bool) -> Response {
    let id = ui.id().with(("pill", label, rect.min.x as i32, rect.min.y as i32));
    let sense = if enabled { Sense::click() } else { Sense::hover() };
    let resp = ui.interact(rect, id, sense);
    let hover = ui.ctx().animate_bool(id.with("h"), resp.hovered() && enabled);
    let pressed = resp.is_pointer_button_down_on() && enabled;
    let p = ui.painter();
    let rect = if pressed { rect.shrink(1.0) } else { rect };
    let tint = match kind {
        1 => Some(BLUE),
        2 => Some(RED),
        _ => None,
    };
    liquid_capsule(p, rect, tint, hover, if enabled { 1.0 } else { 0.45 });
    let col = if enabled { TEXT } else { with_alpha(TEXT, 0.5) };
    p.text(rect.center(), Align2::CENTER_CENTER, label, bold(14.0), col);
    if enabled && resp.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    resp
}

/// A rounded glass body: translucent fill (tinted or clear), a soft light
/// pooled under the top edge, a faint caustic at the bottom and a bright rim.
pub fn liquid_capsule(p: &Painter, rect: Rect, tint: Option<Color32>, hover: f32, alpha: f32) {
    let r = rect.height().min(rect.width()) / 2.0;
    let shadow = Shadow { offset: [0, 3], blur: 12, spread: 0, color: Color32::from_black_alpha((45.0 * alpha) as u8) };
    p.add(shadow.as_shape(rect, CornerRadius::same(r.min(255.0) as u8)));
    match tint {
        Some(c) => {
            let top = lerp_color(c, Color32::WHITE, 0.10 + 0.10 * hover);
            gradient_rect(p, rect, r, with_alpha(top, 0.80 * alpha), with_alpha(lerp_color(c, Color32::BLACK, 0.08), 0.86 * alpha));
        }
        None => gradient_rect(p, rect, r, white(((26.0 + 18.0 * hover) * alpha) as u8), white(((12.0 + 12.0 * hover) * alpha) as u8)),
    }
    // light pooled under the top edge, following the shape so there are no edges
    let inner = rect.shrink(1.5);
    gradient_rect(p, inner, (r - 1.5).max(0.0), white(((tint.map(|_| 42.0).unwrap_or(26.0)) * alpha) as u8), white(0));
    rim(p, rect, r, (0.85 + 0.15 * hover) * alpha);
}

/// Text link like the original's blue "Free Up" / "Test Speed".
pub fn link(ui: &mut Ui, pos: Pos2, align: Align2, label: &str, size: f32) -> Response {
    let galley = ui.painter().layout_no_wrap(label.to_owned(), bold(size), ACCENT);
    let rect = align.anchor_size(pos, galley.size());
    let id = ui.id().with(("link", label, pos.x as i32, pos.y as i32));
    let resp = ui.interact(rect.expand(4.0), id, Sense::click());
    let col = if resp.hovered() { lerp_color(ACCENT, Color32::WHITE, 0.35) } else { ACCENT };
    ui.painter().galley(rect.min, galley, col);
    if resp.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    resp
}

/// Thin capsule progress bar.
pub fn bar(painter: &Painter, rect: Rect, frac: f32, color: Color32) {
    let r = rect.height() / 2.0;
    painter.rect_filled(rect, r, white(26));
    let f = frac.clamp(0.0, 1.0);
    if f > 0.0 {
        let w = (rect.width() * f).max(rect.height());
        let fill = Rect::from_min_size(rect.min, Vec2::new(w, rect.height()));
        gradient_rect_dir(painter, fill, r, lerp_color(color, Color32::WHITE, 0.25), color, Vec2::new(1.0, 0.0));
    }
}

/// Ring (donut) chart. Segments are (fraction, colour); remainder is a faint track.
pub fn ring(painter: &Painter, center: Pos2, radius: f32, width: f32, segments: &[(f32, Color32)]) {
    let track = PathShape::line(arc_points(center, radius, 0.0, TAU, 96), PathStroke::new(width, white(22)));
    painter.add(track);
    let mut start = -PI / 2.0;
    for (f, c) in segments {
        let sweep = (f.clamp(0.0, 1.0)) * TAU;
        if sweep < 0.002 {
            continue;
        }
        let n = ((sweep / TAU) * 120.0).max(3.0) as usize;
        painter.add(PathShape::line(arc_points(center, radius, start, start + sweep, n), PathStroke::new(width, *c)));
        start += sweep;
    }
}

pub fn arc_points(center: Pos2, radius: f32, a0: f32, a1: f32, n: usize) -> Vec<Pos2> {
    (0..=n)
        .map(|i| {
            let a = a0 + (a1 - a0) * i as f32 / n as f32;
            center + Vec2::new(a.cos(), a.sin()) * radius
        })
        .collect()
}

/// A spinning arc used while something is working.
pub fn spinner(painter: &Painter, center: Pos2, radius: f32, t: f64, color: Color32) {
    let a0 = (t * 4.0) as f32;
    painter.add(PathShape::line(arc_points(center, radius, a0, a0 + 4.2, 40), PathStroke::new(2.5, color)));
}

pub fn checkbox(ui: &mut Ui, rect: Rect, id: egui::Id, on: bool, enabled: bool) -> Response {
    let resp = ui.interact(rect.expand(3.0), id, if enabled { Sense::click() } else { Sense::hover() });
    let p = ui.painter();
    let r = 5.0;
    if on && enabled {
        gradient_rect(p, rect, r, lerp_color(BLUE, Color32::WHITE, 0.2), BLUE);
        let a = rect.left_top() + Vec2::new(rect.width() * 0.25, rect.height() * 0.52);
        let b = rect.left_top() + Vec2::new(rect.width() * 0.43, rect.height() * 0.70);
        let c = rect.left_top() + Vec2::new(rect.width() * 0.76, rect.height() * 0.32);
        p.add(PathShape::line(vec![a, b, c], PathStroke::new(2.0, Color32::WHITE)));
    } else {
        p.rect_filled(rect, r, white(if enabled { 18 } else { 6 }));
        p.rect_stroke(rect, r, Stroke::new(1.0, white(if enabled { 90 } else { 30 })), StrokeKind::Inside);
    }
    resp
}

/// Text that is cut with an ellipsis to fit `max_w`.
pub fn text_fit(painter: &Painter, pos: Pos2, align: Align2, text: &str, font: FontId, color: Color32, max_w: f32) -> Rect {
    let mut s = text.to_owned();
    let mut g = painter.layout_no_wrap(s.clone(), font.clone(), color);
    if g.size().x > max_w {
        let chars: Vec<char> = text.chars().collect();
        let mut n = chars.len();
        while n > 1 && g.size().x > max_w {
            n -= 1;
            s = chars[..n].iter().collect::<String>() + "…";
            g = painter.layout_no_wrap(s.clone(), font.clone(), color);
        }
    }
    let rect = align.anchor_size(pos, g.size());
    painter.galley(rect.min, g, color);
    rect
}

/// Middle-ellipsis for long paths: "/Users/me/Library/…/Cache".
pub fn middle_ellipsis(s: &str, max_chars: usize) -> String {
    let chars: Vec<char> = s.chars().collect();
    if chars.len() <= max_chars || max_chars < 8 {
        return s.to_owned();
    }
    let head = max_chars * 2 / 5;
    let tail = max_chars - head - 1;
    let a: String = chars[..head].iter().collect();
    let b: String = chars[chars.len() - tail..].iter().collect();
    format!("{a}…{b}")
}

/// Wrapped paragraph; returns the height used.
pub fn paragraph(painter: &Painter, pos: Pos2, text: &str, font: FontId, color: Color32, width: f32) -> f32 {
    let g = painter.layout(text.to_owned(), font, color, width);
    let h = g.size().y;
    painter.galley(pos, g, color);
    h
}

// ---------------------------------------------------------------- icons

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Icon {
    Disk,
    Memory,
    Battery(u8),
    Cpu,
    Wifi,
    Bluetooth,
    Headphones,
    Shield,
    Sparkle,
    Broom,
    Gear,
    Trash,
    Folder,
    Lens,
    Laptop,
    Heart,
    Thermo,
    Clock,
    Up,
    Down,
    Back,
    Forward,
    Info,
    Check,
    Question,
    Lock,
    Monitor,
    Chart,
    Power,
}

/// Draw a small line icon inside `rect` (roughly square).
/// The SF Symbol behind each icon.
pub fn symbol_name(icon: Icon) -> Option<&'static str> {
    Some(match icon {
        Icon::Disk => "internaldrive.fill",
        Icon::Memory => "memorychip.fill",
        Icon::Battery(l) => match l {
            88..=255 => "battery.100percent",
            63..=87 => "battery.75percent",
            38..=62 => "battery.50percent",
            13..=37 => "battery.25percent",
            _ => "battery.0percent",
        },
        Icon::Cpu => "cpu.fill",
        Icon::Wifi => "wifi",
        Icon::Bluetooth => "dot.radiowaves.left.and.right",
        Icon::Headphones => "headphones",
        Icon::Shield => "checkmark.shield.fill",
        Icon::Sparkle => "sparkles",
        Icon::Broom => "bubbles.and.sparkles.fill",
        Icon::Gear => "gearshape.fill",
        Icon::Trash => "trash.fill",
        Icon::Folder => "folder.fill",
        Icon::Lens => "magnifyingglass",
        Icon::Laptop => "laptopcomputer",
        Icon::Heart => "heart.fill",
        Icon::Thermo => "thermometer.medium",
        Icon::Clock => "clock.fill",
        Icon::Up => "arrow.up",
        Icon::Down => "arrow.down",
        Icon::Back => "chevron.left",
        Icon::Forward => "chevron.right",
        Icon::Info => "info.circle",
        Icon::Check => "checkmark",
        Icon::Question => "questionmark",
        Icon::Lock => "lock.fill",
        Icon::Monitor => "menubar.rectangle",
        Icon::Chart => "chart.bar.fill",
        Icon::Power => "power",
    })
}

thread_local! {
    static SYMBOLS: std::cell::RefCell<std::collections::HashMap<(String, u32), Option<(egui::TextureHandle, Vec2)>>> = std::cell::RefCell::new(std::collections::HashMap::new());
}

/// Draw an SF Symbol tinted `color`, fitted (aspect kept) inside `rect`.
/// Returns false when the symbol isn't available (then a drawn icon is used).
pub fn symbol(p: &Painter, name: &str, rect: Rect, color: Color32) -> bool {
    let ppp = p.ctx().pixels_per_point();
    let px = (rect.height() * ppp * 0.92).round().max(8.0) as u32;
    let key = (name.to_string(), px);
    let entry = SYMBOLS.with(|m| m.borrow().get(&key).cloned());
    let entry = match entry {
        Some(e) => e,
        None => {
            let made = crate::kit::native::render_symbol(name, px as f64, crate::kit::native::weight_semibold()).map(|(w, h, rgba)| {
                let img = egui::ColorImage::from_rgba_unmultiplied([w, h], &rgba);
                let tex = p.ctx().load_texture(format!("sf:{name}:{px}"), img, egui::TextureOptions::LINEAR);
                (tex, Vec2::new(w as f32 / ppp, h as f32 / ppp))
            });
            SYMBOLS.with(|m| m.borrow_mut().insert(key, made.clone()));
            made
        }
    };
    let Some((tex, size)) = entry else { return false };
    let scale = (rect.width() / size.x).min(rect.height() / size.y).min(1.15);
    let r = Rect::from_center_size(rect.center(), size * scale);
    p.image(tex.id(), r, Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)), color);
    true
}

/// Draw a small icon: Apple's SF Symbol when available, otherwise a drawn one.
pub fn icon(p: &Painter, icon: Icon, rect: Rect, color: Color32) {
    if let Some(name) = symbol_name(icon) {
        if symbol(p, name, rect, color) {
            return;
        }
    }
    icon_drawn(p, icon, rect, color);
}

fn icon_drawn(p: &Painter, icon: Icon, rect: Rect, color: Color32) {
    let c = rect.center();
    let s = rect.width().min(rect.height());
    let u = s / 24.0; // design grid of 24
    let pt = |x: f32, y: f32| c + Vec2::new((x - 12.0) * u, (y - 12.0) * u);
    let st = Stroke::new(1.7 * u.max(0.8), color);
    let line = |pts: Vec<Pos2>| p.add(PathShape::line(pts, PathStroke::new(st.width, color)));
    match icon {
        Icon::Disk => {
            let body = Rect::from_min_max(pt(3.0, 6.0), pt(21.0, 18.0));
            gradient_rect(p, body, 3.5 * u, lerp_color(color, Color32::WHITE, 0.2), with_alpha(color, 0.75));
            p.line_segment([pt(3.5, 13.0), pt(20.5, 13.0)], Stroke::new(1.0 * u, Color32::from_black_alpha(80)));
            p.circle_filled(pt(17.5, 15.5), 1.1 * u, Color32::from_black_alpha(120));
        }
        Icon::Memory => {
            let body = Rect::from_min_max(pt(3.0, 7.0), pt(21.0, 16.0));
            p.rect_stroke(body, 2.0 * u, st, StrokeKind::Middle);
            for i in 0..4 {
                let x = 6.0 + i as f32 * 4.0;
                p.rect_filled(Rect::from_min_max(pt(x, 9.5), pt(x + 2.4, 13.5)), 0.6 * u, color);
            }
            for i in 0..7 {
                let x = 4.5 + i as f32 * 2.5;
                p.line_segment([pt(x, 16.5), pt(x, 19.0)], Stroke::new(1.2 * u, color));
            }
        }
        Icon::Battery(level) => {
            let body = Rect::from_min_max(pt(2.5, 8.0), pt(19.5, 16.0));
            p.rect_stroke(body, 2.5 * u, st, StrokeKind::Middle);
            p.rect_filled(Rect::from_min_max(pt(20.5, 10.5), pt(22.0, 13.5)), 0.8 * u, color);
            let f = level.min(100) as f32 / 100.0;
            let inner = Rect::from_min_max(pt(4.3, 9.8), pt(4.3 + 13.4 * f, 14.2));
            let fill = if level <= 20 { RED } else { color };
            p.rect_filled(inner, 1.2 * u, fill);
        }
        Icon::Cpu => {
            let body = Rect::from_min_max(pt(5.0, 5.0), pt(19.0, 19.0));
            p.rect_stroke(body, 3.0 * u, st, StrokeKind::Middle);
            p.rect_stroke(Rect::from_min_max(pt(9.0, 9.0), pt(15.0, 15.0)), 1.5 * u, st, StrokeKind::Middle);
            for i in 0..3 {
                let o = 8.5 + i as f32 * 3.5;
                p.line_segment([pt(o, 2.0), pt(o, 5.0)], st);
                p.line_segment([pt(o, 19.0), pt(o, 22.0)], st);
                p.line_segment([pt(2.0, o), pt(5.0, o)], st);
                p.line_segment([pt(19.0, o), pt(22.0, o)], st);
            }
        }
        Icon::Wifi => {
            let base = pt(12.0, 19.0);
            for (i, r) in [5.5f32, 10.0, 14.5].iter().enumerate() {
                let pts = arc_points(base, r * u, -PI * 0.78, -PI * 0.22, 20);
                p.add(PathShape::line(pts, PathStroke::new(2.0 * u, with_alpha(color, 1.0 - i as f32 * 0.05))));
            }
            p.circle_filled(base, 1.8 * u, color);
        }
        Icon::Bluetooth => {
            line(vec![pt(7.0, 7.5), pt(17.0, 16.0), pt(12.0, 20.5), pt(12.0, 3.5), pt(17.0, 8.0), pt(7.0, 16.5)]);
        }
        Icon::Headphones => {
            p.add(PathShape::line(arc_points(pt(12.0, 13.0), 8.0 * u, PI, TAU, 24), PathStroke::new(st.width, color)));
            p.rect_filled(Rect::from_min_max(pt(3.0, 13.0), pt(7.5, 20.0)), 2.0 * u, color);
            p.rect_filled(Rect::from_min_max(pt(16.5, 13.0), pt(21.0, 20.0)), 2.0 * u, color);
        }
        Icon::Shield => {
            let pts = vec![pt(12.0, 2.5), pt(20.0, 5.5), pt(19.0, 13.0), pt(12.0, 21.5), pt(5.0, 13.0), pt(4.0, 5.5)];
            p.add(Shape::convex_polygon(pts, with_alpha(color, 0.9), Stroke::NONE));
            line(vec![pt(8.0, 11.5), pt(11.0, 14.5), pt(16.5, 8.5)]);
            p.add(PathShape::line(vec![pt(8.0, 11.5), pt(11.0, 14.5), pt(16.5, 8.5)], PathStroke::new(2.2 * u, Color32::WHITE)));
        }
        Icon::Sparkle => {
            // a four-point star with a small companion star
            let star = |cx: f32, cy: f32, r: f32| {
                let k = 0.28;
                vec![
                    pt(cx, cy - r),
                    pt(cx + r * k, cy - r * k),
                    pt(cx + r, cy),
                    pt(cx + r * k, cy + r * k),
                    pt(cx, cy + r),
                    pt(cx - r * k, cy + r * k),
                    pt(cx - r, cy),
                    pt(cx - r * k, cy - r * k),
                ]
            };
            p.add(Shape::Path(PathShape { points: star(10.5, 13.0, 8.5), closed: true, fill: color, stroke: PathStroke::NONE }));
            p.add(Shape::Path(PathShape { points: star(18.5, 5.5, 3.6), closed: true, fill: color, stroke: PathStroke::NONE }));
        }
        Icon::Broom => {
            line(vec![pt(17.5, 3.0), pt(12.5, 11.0)]);
            let pts = vec![pt(9.0, 10.0), pt(16.0, 13.5), pt(13.0, 21.0), pt(4.0, 17.0)];
            p.add(Shape::convex_polygon(pts, color, Stroke::NONE));
            p.line_segment([pt(7.0, 18.5), pt(9.5, 14.0)], Stroke::new(1.0 * u, Color32::from_black_alpha(90)));
            p.line_segment([pt(10.5, 19.8), pt(12.5, 15.0)], Stroke::new(1.0 * u, Color32::from_black_alpha(90)));
        }
        Icon::Gear => {
            let center = pt(12.0, 12.0);
            let mut pts = Vec::new();
            for i in 0..48 {
                let a = i as f32 / 48.0 * TAU;
                let tooth = ((a * 8.0).sin() > 0.0) as i32 as f32;
                let r = (7.0 + 2.2 * tooth) * u;
                pts.push(center + Vec2::new(a.cos(), a.sin()) * r);
            }
            p.add(Shape::Path(PathShape { points: pts, closed: true, fill: color, stroke: PathStroke::NONE }));
            p.circle_filled(center, 3.2 * u, Color32::from_rgb(60, 40, 110));
        }
        Icon::Trash => {
            p.rect_filled(Rect::from_min_max(pt(5.0, 5.0), pt(19.0, 7.5)), 1.0 * u, color);
            p.rect_filled(Rect::from_min_max(pt(9.5, 3.0), pt(14.5, 5.0)), 1.0 * u, color);
            let pts = vec![pt(6.5, 8.5), pt(17.5, 8.5), pt(16.5, 21.0), pt(7.5, 21.0)];
            p.add(Shape::convex_polygon(pts, with_alpha(color, 0.85), Stroke::NONE));
        }
        Icon::Folder => {
            let tab = vec![pt(2.5, 5.0), pt(9.5, 5.0), pt(11.5, 7.5), pt(2.5, 7.5)];
            p.add(Shape::convex_polygon(tab, lerp_color(color, Color32::BLACK, 0.15), Stroke::NONE));
            let body = Rect::from_min_max(pt(2.5, 7.0), pt(21.5, 19.5));
            gradient_rect(p, body, 2.2 * u, lerp_color(color, Color32::WHITE, 0.18), color);
        }
        Icon::Lens => {
            p.circle_stroke(pt(10.5, 10.5), 6.5 * u, Stroke::new(2.2 * u, color));
            p.line_segment([pt(15.5, 15.5), pt(20.5, 20.5)], Stroke::new(2.8 * u, color));
        }
        Icon::Laptop => {
            let screen = Rect::from_min_max(pt(4.0, 4.0), pt(20.0, 16.0));
            gradient_rect(p, screen, 1.5 * u, Color32::from_rgb(160, 250, 245), Color32::from_rgb(90, 200, 230));
            p.rect_stroke(screen, 1.5 * u, Stroke::new(0.8 * u, Color32::from_rgb(40, 120, 160)), StrokeKind::Inside);
            let base = Rect::from_min_max(pt(1.5, 16.5), pt(22.5, 18.5));
            gradient_rect(p, base, 1.0 * u, Color32::from_rgb(140, 170, 255), Color32::from_rgb(80, 90, 210));
        }
        Icon::Heart => {
            let mut pts = Vec::new();
            for i in 0..60 {
                let t = i as f32 / 60.0 * TAU;
                let x = 16.0 * t.sin().powi(3);
                let y = -(13.0 * t.cos() - 5.0 * (2.0 * t).cos() - 2.0 * (3.0 * t).cos() - (4.0 * t).cos());
                pts.push(pt(12.0 + x * 0.55, 12.5 + y * 0.55));
            }
            p.add(Shape::Path(PathShape { points: pts, closed: true, fill: color, stroke: PathStroke::NONE }));
        }
        Icon::Thermo => {
            p.rect_stroke(Rect::from_min_max(pt(9.5, 2.5), pt(14.5, 16.0)), 2.5 * u, st, StrokeKind::Middle);
            p.circle_filled(pt(12.0, 18.0), 3.6 * u, color);
            p.line_segment([pt(12.0, 8.0), pt(12.0, 17.0)], Stroke::new(2.2 * u, color));
            for y in [5.0, 8.0, 11.0] {
                p.line_segment([pt(16.5, y), pt(19.0, y)], Stroke::new(1.2 * u, color));
            }
        }
        Icon::Clock => {
            p.circle_stroke(pt(12.0, 12.0), 8.5 * u, st);
            line(vec![pt(12.0, 7.0), pt(12.0, 12.0), pt(15.5, 14.0)]);
        }
        Icon::Up => {
            line(vec![pt(12.0, 20.0), pt(12.0, 4.5)]);
            line(vec![pt(6.5, 10.0), pt(12.0, 4.5), pt(17.5, 10.0)]);
        }
        Icon::Down => {
            line(vec![pt(12.0, 4.0), pt(12.0, 19.5)]);
            line(vec![pt(6.5, 14.0), pt(12.0, 19.5), pt(17.5, 14.0)]);
        }
        Icon::Back => {
            line(vec![pt(15.0, 4.5), pt(8.0, 12.0), pt(15.0, 19.5)]);
        }
        Icon::Forward => {
            line(vec![pt(9.0, 4.5), pt(16.0, 12.0), pt(9.0, 19.5)]);
        }
        Icon::Info => {
            p.circle_stroke(pt(12.0, 12.0), 9.0 * u, st);
            p.circle_filled(pt(12.0, 7.6), 1.3 * u, color);
            p.line_segment([pt(12.0, 10.5), pt(12.0, 17.0)], Stroke::new(2.0 * u, color));
        }
        Icon::Check => {
            line(vec![pt(5.0, 12.5), pt(10.0, 17.5), pt(19.0, 7.0)]);
        }
        Icon::Question => {
            p.add(PathShape::line(arc_points(pt(12.0, 9.0), 4.5 * u, PI * 1.05, PI * 2.45, 20), PathStroke::new(2.0 * u, color)));
            line(vec![pt(13.6, 13.2), pt(12.0, 14.5), pt(12.0, 16.0)]);
            p.circle_filled(pt(12.0, 19.5), 1.3 * u, color);
        }
        Icon::Lock => {
            p.add(PathShape::line(arc_points(pt(12.0, 10.0), 4.5 * u, PI, TAU, 16), PathStroke::new(2.0 * u, color)));
            p.line_segment([pt(7.5, 10.0), pt(7.5, 11.0)], Stroke::new(2.0 * u, color));
            p.line_segment([pt(16.5, 10.0), pt(16.5, 11.0)], Stroke::new(2.0 * u, color));
            p.rect_filled(Rect::from_min_max(pt(5.0, 11.0), pt(19.0, 21.0)), 2.5 * u, color);
        }
        Icon::Monitor => {
            let screen = Rect::from_min_max(pt(3.0, 4.0), pt(21.0, 16.0));
            p.rect_stroke(screen, 2.0 * u, st, StrokeKind::Middle);
            p.line_segment([pt(12.0, 16.0), pt(12.0, 20.0)], st);
            p.line_segment([pt(8.0, 20.0), pt(16.0, 20.0)], st);
            line(vec![pt(7.0, 13.0), pt(10.0, 9.5), pt(13.0, 11.5), pt(17.0, 7.0)]);
        }
        Icon::Chart => {
            for (i, h) in [6.0f32, 11.0, 8.0, 14.0].iter().enumerate() {
                let x = 4.5 + i as f32 * 4.5;
                p.rect_filled(Rect::from_min_max(pt(x, 20.0 - h), pt(x + 3.0, 20.0)), 1.0 * u, color);
            }
        }
        Icon::Power => {
            p.add(PathShape::line(arc_points(pt(12.0, 13.0), 7.5 * u, -PI * 0.3, PI * 1.3, 30), PathStroke::new(2.0 * u, color)));
            p.line_segment([pt(12.0, 3.0), pt(12.0, 12.0)], Stroke::new(2.0 * u, color));
        }
    }
}

/// Big clear-glass hero object (intro / scanning): a glass squircle with a
/// tinted inner light and a large SF Symbol. `phase` drifts the light.
pub fn orb(p: &Painter, center: Pos2, radius: f32, base: Color32, glyph: Icon, phase: f64) {
    let rect = Rect::from_center_size(center, Vec2::splat(radius * 2.0));
    let r = radius * 0.46;
    let shadow = Shadow { offset: [0, 18], blur: 48, spread: 0, color: Color32::from_black_alpha(80) };
    p.add(shadow.as_shape(rect, CornerRadius::same(r.min(255.0) as u8)));
    gradient_rect_dir(p, rect, r, white(34), white(8), Vec2::new(0.25, 1.0));
    let drift = (phase * 0.5).sin() as f32 * radius * 0.08;
    glow(p, center + Vec2::new(drift, radius * 0.15), radius * 0.78, with_alpha(base, 0.42));
    glow(p, center + Vec2::new(0.0, -radius * 0.42), radius * 0.62, white(28));
    rim(p, rect, r, 1.0);
    rim(p, rect.shrink(radius * 0.05), r * 0.92, 0.25);
    let g = Rect::from_center_size(center, Vec2::splat(radius * 0.92));
    icon(&p.with_clip_rect(rect), glyph, g.translate(Vec2::new(0.0, radius * 0.02)), Color32::from_black_alpha(60));
    icon(p, glyph, g, lerp_color(base, Color32::WHITE, 0.62));
}

pub fn human_bytes(b: u64) -> String {
    const UNITS: [&str; 5] = ["B", "KB", "MB", "GB", "TB"];
    let mut v = b as f64;
    let mut i = 0;
    while v >= 1000.0 && i < UNITS.len() - 1 {
        v /= 1000.0;
        i += 1;
    }
    if i == 0 {
        format!("{} B", b)
    } else if v >= 100.0 {
        format!("{:.0} {}", v, UNITS[i])
    } else if v >= 10.0 {
        format!("{:.1} {}", v, UNITS[i])
    } else {
        format!("{:.2} {}", v, UNITS[i])
    }
}

pub fn human_rate(bps: f64) -> String {
    format!("{}/sec", human_bytes(bps.max(0.0) as u64))
}

pub fn human_duration(secs: u64) -> String {
    let d = secs / 86_400;
    let h = (secs % 86_400) / 3600;
    let m = (secs % 3600) / 60;
    if d > 0 {
        format!("{d}d {h}h")
    } else if h > 0 {
        format!("{h}h {m}m")
    } else {
        format!("{m}m")
    }
}

/// Liquid-glass edge: a hairline that catches light at the top-left and,
/// more softly, at the bottom-right.
pub fn rim(p: &Painter, rect: Rect, r: f32, strength: f32) {
    let pts = rounded_points(rect.shrink(0.5), r);
    let n = pts.len();
    let light = Vec2::new(-0.55, -0.83);
    for i in 0..n {
        let a = pts[i];
        let b = pts[(i + 1) % n];
        let mid = a + (b - a) * 0.5;
        let normal = (mid - rect.center()).normalized();
        let d = normal.dot(light);
        let k = 0.16 + 0.70 * d.max(0.0).powf(1.6) + 0.30 * (-d).max(0.0).powf(2.0);
        p.line_segment([a, b], Stroke::new(1.0, white((k * 255.0 * strength).min(255.0) as u8)));
    }
}

/// A rounded app-style icon: tinted glass square with a white glyph.
pub fn tile_icon(p: &Painter, rect: Rect, glyph: Icon, top: Color32, bottom: Color32) {
    let r = rect.width() * 0.27;
    let shadow = Shadow { offset: [0, 2], blur: 8, spread: 0, color: Color32::from_black_alpha(50) };
    p.add(shadow.as_shape(rect, CornerRadius::same(r as u8)));
    gradient_rect(p, rect, r, with_alpha(top, 0.95), with_alpha(bottom, 0.95));
    let sheen = Rect::from_min_max(rect.min + Vec2::new(1.5, 1.0), Pos2::new(rect.max.x - 1.5, rect.center().y));
    gradient_rect(p, sheen, r * 0.85, white(60), white(0));
    rim(p, rect, r, 0.9);
    icon(p, glyph, rect.shrink(rect.width() * 0.22), Color32::WHITE);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn byte_sizes() {
        assert_eq!(human_bytes(0), "0 B");
        assert_eq!(human_bytes(999), "999 B");
        assert_eq!(human_bytes(1_500), "1.50 KB");
        assert_eq!(human_bytes(46_100_000), "46.1 MB");
        assert_eq!(human_bytes(2_000_000_000_000), "2.00 TB");
        assert_eq!(human_bytes(u64::MAX), format!("{:.0} TB", u64::MAX as f64 / 1e12));
    }

    #[test]
    fn middle_ellipsis_keeps_ends() {
        assert_eq!(middle_ellipsis("short", 20), "short");
        let s = middle_ellipsis("/Users/me/Library/Caches/com.example.app/Cache", 20);
        assert_eq!(s.chars().count(), 20);
        assert!(s.starts_with("/Users/m") && s.ends_with("app/Cache") && s.contains('…'));
        // multi-byte text is cut on characters
        assert_eq!(middle_ellipsis("ааааааааааббббббббб", 10).chars().count(), 10);
    }
}
