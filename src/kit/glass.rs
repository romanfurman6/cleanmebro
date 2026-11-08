//! Drawing kit for the "Control Center" look: near-opaque dark surfaces,
//! flat cards with hairlines, one blue accent, standard 28–34 pt buttons and
//! small monochrome icons (no bitmap assets).

use egui::{
    epaint::{PathShape, PathStroke},
    Align2, Color32, CornerRadius, FontFamily, FontId, Painter, Pos2, Rect, Response, Sense,
    Shape, Stroke, StrokeKind, Ui, Vec2,
};
use std::f32::consts::{PI, TAU};

// ---------------------------------------------------------------- palette

/// Window / panel background.
pub const BG: Color32 = Color32::from_rgb(0x1C, 0x1C, 0x1F);
/// Card surface on top of `BG`.
pub const CARD: Color32 = Color32::from_rgb(0x24, 0x24, 0x28);
/// Raised tile (quick actions) and selected rows.
pub const RAISED: Color32 = Color32::from_rgb(0x2A, 0x2A, 0x2E);
/// Window sidebar.
pub const SIDEBAR: Color32 = Color32::from_rgb(0x16, 0x16, 0x18);
/// Window footer strip.
pub const FOOTER: Color32 = Color32::from_rgb(0x19, 0x19, 0x1C);

pub const TEXT: Color32 = Color32::from_rgb(0xF5, 0xF5, 0xF7);
/// Secondary text on dark surfaces.
pub const TEXT_SOFT: Color32 = Color32::from_rgb(0xD1, 0xD1, 0xD6);
pub const TEXT_DIM: Color32 = Color32::from_rgb(0xA1, 0xA1, 0xA6);
pub const TEXT_FAINT: Color32 = Color32::from_rgb(0x8E, 0x8E, 0x93);
/// Chevrons, drag handles.
pub const TEXT_MUTED: Color32 = Color32::from_rgb(0x6E, 0x6E, 0x73);

/// The one accent: primary buttons, selection, checkboxes (white on it passes 4.5:1).
pub const BLUE: Color32 = Color32::from_rgb(0x0B, 0x6B, 0xD8);
/// Links, storage bars, sparklines.
pub const ACCENT: Color32 = Color32::from_rgb(0x4D, 0xA3, 0xFF);
pub const GREEN: Color32 = Color32::from_rgb(0x30, 0xD1, 0x58);
pub const AMBER: Color32 = Color32::from_rgb(0xFF, 0xB3, 0x40);
pub const RED: Color32 = Color32::from_rgb(0xFF, 0x45, 0x3A);
/// Claude's tint (bars, sparkle).
pub const CLAUDE: Color32 = Color32::from_rgb(0xE8, 0x87, 0x5F);
pub const TEAL: Color32 = Color32::from_rgb(0x64, 0xD2, 0xFF);
pub const PURPLE: Color32 = Color32::from_rgb(0xBF, 0x8C, 0xFF);
pub const MINT: Color32 = Color32::from_rgb(99, 230, 226);
pub const GRAY: Color32 = Color32::from_rgb(152, 152, 157);
pub const PINK: Color32 = AMBER;

/// One small tint chip per tool (white glyph on it).
pub mod tint {
    use egui::Color32;
    pub const CLEANUP: Color32 = super::BLUE;
    pub const DISK_MAP: Color32 = Color32::from_rgb(0x2E, 0x9E, 0x8F);
    pub const HEALTH: Color32 = Color32::from_rgb(0x30, 0xA1, 0x4E);
    pub const CLAUDE: Color32 = Color32::from_rgb(0xC9, 0x64, 0x3E);
    pub const NEUTRAL: Color32 = Color32::from_rgb(0x63, 0x63, 0x66);
}

/// Status chip colours: (background, text).
pub const CHIP_SAFE: (Color32, Color32) = (Color32::from_rgba_premultiplied(7, 29, 12, 36), Color32::from_rgb(0x5B, 0xE0, 0x7F));
pub const CHIP_ASK: (Color32, Color32) = (Color32::from_rgba_premultiplied(41, 29, 10, 41), Color32::from_rgb(0xFF, 0xC2, 0x66));
pub const CHIP_KEPT: (Color32, Color32) = (Color32::from_rgba_premultiplied(20, 20, 20, 20), TEXT_DIM);
pub const CHIP_RISK: (Color32, Color32) = (Color32::from_rgba_premultiplied(41, 11, 9, 41), Color32::from_rgb(0xFF, 0x7A, 0x70));

/// Radii and spacing on the 8 pt grid.
pub const R_CARD: f32 = 12.0;
pub const R_BUTTON: f32 = 8.0;
pub const R_SMALL: f32 = 7.0;
pub const HAIRLINE: u8 = 15; // white at ~6%
pub const HAIRLINE_STRONG: u8 = 26; // white at ~10%

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

/// The window surface: near-opaque dark material with a 1 pt hairline edge.
/// Live, the system material behind it only shows through a few percent, so
/// text stays readable over busy windows. `opaque` (screenshots) paints it solid.
pub fn backdrop(painter: &Painter, rect: Rect, radius: f32, opaque: bool, _t: f64) {
    let fill = if opaque { BG } else { with_alpha(BG, 0.97) };
    painter.rect_filled(rect, radius, fill);
    painter.rect_stroke(rect, radius, Stroke::new(1.0, white(HAIRLINE_STRONG)), StrokeKind::Inside);
}

/// A flat card: `CARD` fill with a hairline. `hover` 0..1 lifts it a little,
/// `selected` adds the blue ring.
pub fn glass_card(painter: &Painter, rect: Rect, r: f32, hover: f32, selected: bool) {
    painter.rect_filled(rect, r, lerp_color(CARD, RAISED, hover));
    painter.rect_stroke(rect, r, Stroke::new(1.0, white(HAIRLINE)), StrokeKind::Inside);
    if selected {
        painter.rect_stroke(rect, r, Stroke::new(1.5, ACCENT), StrokeKind::Inside);
    }
}

/// A card that reacts to the mouse. Returns its response.
pub fn card(ui: &mut Ui, rect: Rect, id: egui::Id, r: f32, selected: bool) -> Response {
    let resp = ui.interact(rect, id, Sense::click());
    let hover = ui.ctx().animate_bool(id.with("hover"), resp.hovered());
    glass_card(ui.painter(), rect, r, hover, selected);
    resp
}

/// Button kinds for [`pill_button`].
pub const BTN_SECONDARY: u8 = 0;
pub const BTN_PRIMARY: u8 = 1;
pub const BTN_DANGER: u8 = 2;

/// Standard push button (28–34 pt tall, radius 8). `kind`: 0 = secondary
/// (quiet fill + hairline), 1 = primary (blue), 2 = destructive (red).
pub fn pill_button(ui: &mut Ui, rect: Rect, label: &str, kind: u8, enabled: bool) -> Response {
    let id = ui.id().with(("pill", label, rect.min.x as i32, rect.min.y as i32));
    let sense = if enabled { Sense::click() } else { Sense::hover() };
    let resp = ui.interact(rect, id, sense);
    let hover = ui.ctx().animate_bool(id.with("h"), resp.hovered() && enabled);
    let pressed = resp.is_pointer_button_down_on() && enabled;
    let p = ui.painter();
    let tint = match kind {
        BTN_PRIMARY => Some(BLUE),
        BTN_DANGER => Some(RED),
        _ => None,
    };
    let a = if enabled { 1.0 } else { 0.45 };
    liquid_capsule(p, rect, tint, if pressed { 1.6 } else { hover }, a);
    let (f, col) = match tint {
        Some(_) => (bold(13.0), Color32::WHITE),
        None => (font(13.0), TEXT),
    };
    let col = if enabled { col } else { with_alpha(col, 0.5) };
    text_fit(p, rect.center(), Align2::CENTER_CENTER, label, f, col, rect.width() - 12.0);
    if enabled && resp.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    resp
}

/// Width a [`pill_button`] needs for `label` (14 pt padding each side).
pub fn button_width(p: &Painter, label: &str, kind: u8) -> f32 {
    let f = if kind == BTN_SECONDARY { font(13.0) } else { bold(13.0) };
    p.layout_no_wrap(label.to_owned(), f, TEXT).size().x + 28.0
}

/// The body of a standard button: flat tint (or quiet white) with a hairline.
/// `hover` 0..1 lightens it; values above 1 read as pressed.
pub fn liquid_capsule(p: &Painter, rect: Rect, tint: Option<Color32>, hover: f32, alpha: f32) {
    let r = R_BUTTON.min(rect.height() / 2.0);
    match tint {
        Some(c) => {
            let fill = if hover > 1.0 { lerp_color(c, Color32::BLACK, 0.15) } else { lerp_color(c, Color32::WHITE, 0.10 * hover) };
            p.rect_filled(rect, r, with_alpha(fill, alpha));
        }
        None => {
            let a = if hover > 1.0 { 26.0 } else { 10.0 + 10.0 * hover };
            p.rect_filled(rect, r, white((a * alpha) as u8));
            p.rect_stroke(rect, r, Stroke::new(1.0, white((36.0 * alpha) as u8)), StrokeKind::Inside);
        }
    }
}

/// A 30 pt square icon button (panel header, sheet actions).
pub fn icon_button(ui: &mut Ui, rect: Rect, ic: Icon, tip: &str) -> Response {
    let id = ui.id().with(("icon-btn", tip, rect.min.x as i32, rect.min.y as i32));
    let resp = ui.interact(rect, id, Sense::click());
    let h = ui.ctx().animate_bool(id, resp.hovered());
    let p = ui.painter();
    p.rect_filled(rect, R_BUTTON, white((15.0 + 13.0 * h) as u8));
    icon(p, ic, Rect::from_center_size(rect.center(), Vec2::splat(rect.height() * 0.53)), TEXT_SOFT);
    if resp.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    resp.on_hover_text(tip)
}

/// The round 26 pt close / refresh button used in sheet headers.
pub fn round_button(ui: &mut Ui, rect: Rect, ic: Icon, tip: &str) -> Response {
    let id = ui.id().with(("round-btn", tip, rect.min.x as i32, rect.min.y as i32));
    let resp = ui.interact(rect, id, Sense::click());
    let h = ui.ctx().animate_bool(id, resp.hovered());
    let p = ui.painter();
    p.circle_filled(rect.center(), rect.height() / 2.0, white((20.0 + 18.0 * h) as u8));
    icon(p, ic, Rect::from_center_size(rect.center(), Vec2::splat(rect.height() * 0.46)), TEXT_SOFT);
    resp.on_hover_text(tip)
}

/// Text link in the accent blue.
pub fn link(ui: &mut Ui, pos: Pos2, align: Align2, label: &str, size: f32) -> Response {
    let galley = ui.painter().layout_no_wrap(label.to_owned(), bold(size), ACCENT);
    let rect = align.anchor_size(pos, galley.size());
    let id = ui.id().with(("link", label, pos.x as i32, pos.y as i32));
    let resp = ui.interact(rect.expand(4.0), id, Sense::click());
    let col = if resp.hovered() { Color32::from_rgb(0x7D, 0xBB, 0xFF) } else { ACCENT };
    ui.painter().galley(rect.min, galley, col);
    if resp.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    resp
}

/// Thin flat progress bar on a faint track.
pub fn bar(painter: &Painter, rect: Rect, frac: f32, color: Color32) {
    let r = rect.height() / 2.0;
    painter.rect_filled(rect, r, white(20));
    let f = frac.clamp(0.0, 1.0);
    if f > 0.0 {
        let w = (rect.width() * f).max(rect.height());
        painter.rect_filled(Rect::from_min_size(rect.min, Vec2::new(w, rect.height())), r, color);
    }
}

/// A stacked bar: segments are (fraction of the whole, colour), 2 pt apart,
/// the rest is the faint track.
pub fn stacked_bar(painter: &Painter, rect: Rect, segments: &[(f32, Color32)]) {
    let r = rect.height() / 2.0;
    painter.rect_filled(rect, r, white(20));
    let clip = painter.with_clip_rect(rect.intersect(painter.clip_rect()));
    let mut x = rect.left();
    let mut shapes = Vec::new();
    for (f, c) in segments {
        let w = rect.width() * f.clamp(0.0, 1.0);
        if w < 0.5 {
            continue;
        }
        let seg = Rect::from_min_max(Pos2::new(x, rect.top()), Pos2::new((x + w).min(rect.right()), rect.bottom()));
        shapes.push((seg, *c));
        x += w + 2.0;
    }
    // round only the outer ends: draw the run as one rounded mask, then cut gaps
    for (seg, c) in shapes {
        let cr = CornerRadius {
            nw: if seg.left() <= rect.left() + 0.5 { r as u8 } else { 0 },
            sw: if seg.left() <= rect.left() + 0.5 { r as u8 } else { 0 },
            ne: if seg.right() >= rect.right() - 0.5 { r as u8 } else { 0 },
            se: if seg.right() >= rect.right() - 0.5 { r as u8 } else { 0 },
        };
        clip.rect_filled(seg, cr, c);
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
    let r = 4.0;
    if on && enabled {
        p.rect_filled(rect, r, BLUE);
        let a = rect.left_top() + Vec2::new(rect.width() * 0.24, rect.height() * 0.52);
        let b = rect.left_top() + Vec2::new(rect.width() * 0.43, rect.height() * 0.71);
        let c = rect.left_top() + Vec2::new(rect.width() * 0.78, rect.height() * 0.31);
        p.add(PathShape::line(vec![a, b, c], PathStroke::new(2.0, Color32::WHITE)));
    } else {
        p.rect_stroke(rect, r, Stroke::new(1.5, white(if enabled { 90 } else { 30 })), StrokeKind::Inside);
    }
    resp
}

/// A small rounded status chip ("Safe", "Ask", "Kept"). `colors` = (fill, text).
/// Returns the chip's rect.
pub fn chip(p: &Painter, pos: Pos2, align: Align2, label: &str, colors: (Color32, Color32)) -> Rect {
    let g = p.layout_no_wrap(label.to_owned(), bold(11.0), colors.1);
    let r = align.anchor_size(pos, g.size() + Vec2::new(16.0, 5.0));
    p.rect_filled(r, 6.0, colors.0);
    p.galley(r.center() - g.size() / 2.0, g, colors.1);
    r
}

/// An amber count badge (e.g. "2" next to "Needs your OK").
pub fn badge(p: &Painter, pos: Pos2, align: Align2, label: &str) -> Rect {
    let g = p.layout_no_wrap(label.to_owned(), bold(11.0), BG);
    let r = align.anchor_size(pos, Vec2::new((g.size().x + 14.0).max(20.0), 18.0));
    p.rect_filled(r, 9.0, AMBER);
    p.galley(r.center() - g.size() / 2.0, g, BG);
    r
}

/// Two-or-more option segmented control. Returns the index clicked this frame.
pub fn segmented(ui: &mut Ui, rect: Rect, id: egui::Id, labels: &[&str], selected: usize) -> Option<usize> {
    let p = ui.painter().clone();
    p.rect_filled(rect, R_SMALL, white(15));
    let inner = rect.shrink(2.0);
    let w = inner.width() / labels.len().max(1) as f32;
    let mut out = None;
    for (i, l) in labels.iter().enumerate() {
        let r = Rect::from_min_size(Pos2::new(inner.left() + w * i as f32, inner.top()), Vec2::new(w, inner.height()));
        let resp = ui.interact(r, id.with(i), Sense::click());
        if i == selected {
            p.rect_filled(r, 6.0, Color32::from_rgb(0x3A, 0x3A, 0x3F));
        }
        let col = if i == selected || resp.hovered() { TEXT } else { TEXT_DIM };
        text_fit(&p, r.center(), Align2::CENTER_CENTER, l, font(12.0), col, r.width() - 8.0);
        if resp.clicked() && i != selected {
            out = Some(i);
        }
    }
    out
}

/// An on/off switch (30×18).
pub fn switch(ui: &mut Ui, rect: Rect, id: egui::Id, on: bool, enabled: bool) -> Response {
    let resp = ui.interact(rect, id, if enabled { Sense::click() } else { Sense::hover() });
    let p = ui.painter();
    if !enabled {
        dashed_rect(p, rect, rect.height() / 2.0, white(64));
        return resp;
    }
    let t = ui.ctx().animate_bool(id.with("t"), on);
    p.rect_filled(rect, rect.height() / 2.0, lerp_color(white(41), BLUE, t));
    let d = rect.height() - 4.0;
    let x = rect.left() + 2.0 + d / 2.0 + (rect.width() - 4.0 - d) * t;
    p.circle_filled(Pos2::new(x, rect.center().y), d / 2.0, Color32::WHITE);
    resp
}

/// A dashed rounded outline: the "planned, not built yet" look.
pub fn dashed_rect(p: &Painter, rect: Rect, r: f32, color: Color32) {
    let pts = rounded_points(rect.shrink(0.5), r);
    let mut path = pts.clone();
    path.push(pts[0]);
    p.extend(Shape::dashed_line(&path, Stroke::new(1.0, color), 3.0, 2.5));
}

/// One tool's tint chip: a rounded square in `color` with a white glyph.
pub fn tool_chip(p: &Painter, rect: Rect, glyph: Icon, color: Color32) {
    let r = (rect.width() * 0.27).round();
    p.rect_filled(rect, r, color);
    icon(p, glyph, Rect::from_center_size(rect.center(), rect.size() * 0.6), Color32::WHITE);
}

/// The dashed chip of a planned tool.
pub fn planned_chip(p: &Painter, rect: Rect, glyph: Icon) {
    let r = (rect.width() * 0.27).round();
    dashed_rect(p, rect, r, white(72));
    icon(p, glyph, Rect::from_center_size(rect.center(), rect.size() * 0.6), TEXT_FAINT);
}

/// 1 pt separator line.
pub fn hairline(p: &Painter, a: Pos2, b: Pos2) {
    p.line_segment([a, b], Stroke::new(1.0, white(HAIRLINE)));
}

/// Small section header ("Clean", "Monitor", table column titles).
pub fn section_header(p: &Painter, pos: Pos2, align: Align2, label: &str) -> Rect {
    p.text(pos, align, label, bold(11.0), TEXT_FAINT)
}

/// The header of a detail sheet: tint chip, title, subtitle, close button.
/// Returns true when the close button was clicked.
pub fn sheet_header(ui: &mut Ui, rect: Rect, glyph: Icon, color: Color32, title: &str, sub: &str) -> bool {
    let p = ui.painter().clone();
    tool_chip(&p, Rect::from_min_size(rect.min + Vec2::new(16.0, 14.0), Vec2::splat(30.0)), glyph, color);
    text_fit(&p, rect.min + Vec2::new(56.0, 12.0), Align2::LEFT_TOP, title, bold(15.0), TEXT, rect.width() - 110.0);
    text_fit(&p, rect.min + Vec2::new(56.0, 31.0), Align2::LEFT_TOP, sub, font(12.0), TEXT_DIM, rect.width() - 110.0);
    hairline(&p, Pos2::new(rect.left(), rect.top() + 57.0), Pos2::new(rect.right(), rect.top() + 57.0));
    let cr = Rect::from_center_size(Pos2::new(rect.right() - 27.0, rect.top() + 29.0), Vec2::splat(26.0));
    round_button(ui, cr, Icon::Close, "Close").clicked()
}

// ---------------------------------------------------------------- panel widgets

/// A quarter-width quick-action tile: tint chip, label, one line under it.
/// Planned tiles are dashed and don't react.
pub fn quick_tile(ui: &mut Ui, rect: Rect, id: egui::Id, glyph: Icon, color: Color32, label: &str, sub: &str, live: bool) -> Response {
    let resp = ui.interact(rect, id, if live { Sense::click() } else { Sense::hover() });
    let h = ui.ctx().animate_bool(id.with("h"), resp.hovered() && live);
    let p = ui.painter();
    let chip_r = Rect::from_min_size(rect.min + Vec2::new(8.0, 10.0), Vec2::splat(28.0));
    if live {
        p.rect_filled(rect, R_CARD, lerp_color(RAISED, Color32::from_rgb(0x32, 0x32, 0x37), h));
        p.rect_stroke(rect, R_CARD, Stroke::new(1.0, white(20)), StrokeKind::Inside);
        p.rect_filled(chip_r, 8.0, color);
        icon(p, glyph, Rect::from_center_size(chip_r.center(), Vec2::splat(17.0)), Color32::WHITE);
    } else {
        dashed_rect(p, rect, R_CARD, white(56));
        p.rect_filled(chip_r, 8.0, white(15));
        icon(p, glyph, Rect::from_center_size(chip_r.center(), Vec2::splat(17.0)), TEXT_DIM);
    }
    text_fit(p, Pos2::new(rect.left() + 8.0, chip_r.bottom() + 6.0), Align2::LEFT_TOP, label, bold(12.0), TEXT, rect.width() - 14.0);
    text_fit(p, Pos2::new(rect.left() + 8.0, chip_r.bottom() + 23.0), Align2::LEFT_TOP, sub, font(11.0), TEXT_DIM, rect.width() - 14.0);
    if live && resp.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    if !live {
        return resp.on_hover_text("Planned: not built yet");
    }
    resp
}

/// Height of a [`quick_tile`].
pub const QUICK_TILE_H: f32 = 84.0;
/// Height of a [`metric_row`] with a bar / without one.
pub const METRIC_ROW_H: f32 = 44.0;
pub const METRIC_ROW_H_FLAT: f32 = 34.0;

/// A compact metric row for a group card: icon, label, value on the right,
/// an optional thin bar under them and a chevron. Hover/selected tint the row.
pub fn metric_row(ui: &mut Ui, rect: Rect, id: egui::Id, glyph: Icon, label: &str, value: &str, value_col: Color32, bar_frac: Option<(f32, Color32)>, selected: bool) -> Response {
    let resp = ui.interact(rect, id, Sense::click());
    let h = ui.ctx().animate_bool(id.with("h"), resp.hovered());
    let p = ui.painter();
    if selected {
        p.rect_filled(rect.shrink2(Vec2::new(4.0, 0.0)), R_BUTTON, with_alpha(BLUE, 0.22));
    } else if h > 0.0 {
        p.rect_filled(rect.shrink2(Vec2::new(4.0, 0.0)), R_BUTTON, white((10.0 * h) as u8));
    }
    let cy = if bar_frac.is_some() { rect.top() + 15.0 } else { rect.center().y };
    icon(p, glyph, Rect::from_center_size(Pos2::new(rect.left() + 20.5, rect.center().y), Vec2::splat(17.0)), TEXT_DIM);
    let chev = Rect::from_center_size(Pos2::new(rect.right() - 19.0, rect.center().y), Vec2::splat(13.0));
    icon(p, Icon::Forward, chev, TEXT_MUTED);
    let x0 = rect.left() + 39.0;
    let x1 = chev.left() - 8.0;
    let vr = text_fit(p, Pos2::new(x1, cy), Align2::RIGHT_CENTER, value, font(12.0), value_col, (x1 - x0) * 0.62);
    text_fit(p, Pos2::new(x0, cy), Align2::LEFT_CENTER, label, font(13.0), TEXT, vr.left() - x0 - 8.0);
    if let Some((f, c)) = bar_frac {
        bar(p, Rect::from_min_max(Pos2::new(x0, cy + 11.0), Pos2::new(x1, cy + 15.0)), f, c);
    }
    if resp.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    resp
}

/// The blue recommendation banner at the bottom of the panel. Returns its response.
pub fn banner(ui: &mut Ui, rect: Rect, id: egui::Id, glyph: Icon, title: &str, sub: &str, action: &str) -> Response {
    let resp = ui.interact(rect, id, Sense::click());
    let h = ui.ctx().animate_bool(id.with("h"), resp.hovered());
    let p = ui.painter();
    p.rect_filled(rect, R_CARD, with_alpha(BLUE, 0.16 + 0.06 * h));
    p.rect_stroke(rect, R_CARD, Stroke::new(1.0, with_alpha(ACCENT, 0.30)), StrokeKind::Inside);
    icon(p, glyph, Rect::from_center_size(Pos2::new(rect.left() + 21.0, rect.center().y), Vec2::splat(18.0)), ACCENT);
    let ar = p.text(Pos2::new(rect.right() - 12.0, rect.center().y), Align2::RIGHT_CENTER, action, bold(12.0), ACCENT);
    let x0 = rect.left() + 40.0;
    let tw = ar.left() - x0 - 10.0;
    text_fit(p, Pos2::new(x0, rect.top() + 10.0), Align2::LEFT_TOP, title, bold(13.0), TEXT, tw);
    text_fit(p, Pos2::new(x0, rect.top() + 29.0), Align2::LEFT_TOP, sub, font(12.0), TEXT_DIM, tw);
    if resp.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    resp
}

/// Height of a [`banner`].
pub const BANNER_H: f32 = 56.0;

/// A sheet's big number: "1.44 TB free of 2 TB". Returns the height used.
pub fn hero_number(p: &Painter, pos: Pos2, big: &str, unit: &str, caption: &str) -> f32 {
    let g = p.layout_no_wrap(big.to_owned(), bold(34.0), TEXT);
    let (w, h) = (g.size().x, g.size().y);
    let base = pos.y + h * 0.80;
    p.galley(pos, g, TEXT);
    let u = p.text(Pos2::new(pos.x + w + 6.0, base), Align2::LEFT_BOTTOM, unit, bold(15.0), TEXT_SOFT);
    p.text(Pos2::new(u.right() + if unit.is_empty() { 0.0 } else { 8.0 }, base), Align2::LEFT_BOTTOM, caption, font(13.0), TEXT_DIM);
    h
}

/// A window sidebar row: tint chip (or dashed chip when `planned`), label and
/// a small right-aligned `meta` text. Selected rows get the blue fill.
pub fn sidebar_item(ui: &mut Ui, rect: Rect, id: egui::Id, glyph: Icon, color: Color32, label: &str, meta: &str, selected: bool, planned: bool) -> Response {
    let resp = ui.interact(rect, id, if planned { Sense::hover() } else { Sense::click() });
    let h = ui.ctx().animate_bool(id.with("h"), resp.hovered() && !planned);
    let p = ui.painter();
    if selected {
        p.rect_filled(rect, R_SMALL, with_alpha(BLUE, 0.85));
    } else if h > 0.0 {
        p.rect_filled(rect, R_SMALL, white((12.0 * h) as u8));
    }
    let chip_r = Rect::from_min_size(Pos2::new(rect.left() + 8.0, rect.center().y - 11.0), Vec2::splat(22.0));
    if planned {
        planned_chip(p, chip_r, glyph);
    } else {
        tool_chip(p, chip_r, glyph, color);
    }
    let mut right = rect.right() - 8.0;
    if !meta.is_empty() {
        right = p.text(Pos2::new(right, rect.center().y), Align2::RIGHT_CENTER, meta, font(11.0), if selected { white(210) } else { TEXT_FAINT }).left() - 6.0;
    }
    text_fit(p, Pos2::new(chip_r.right() + 8.0, rect.center().y), Align2::LEFT_CENTER, label, font(13.0), if planned { TEXT_FAINT } else { TEXT }, right - chip_r.right() - 8.0);
    if planned {
        return resp.on_hover_text("Planned: not built yet");
    }
    resp
}

/// Height of a [`sidebar_item`] row.
pub const SIDEBAR_ROW_H: f32 = 32.0;
/// Width of the window sidebar.
pub const SIDEBAR_W: f32 = 220.0;

/// Height of a window page header ([`page_header`]).
pub const PAGE_HEADER_H: f32 = 68.0;
/// Height of a window page footer ([`page_footer`]).
pub const PAGE_FOOTER_H: f32 = 60.0;

/// A window page's header: 20 pt title, one line of status under it and a
/// hairline. Returns the rect for buttons on the right (vertically centred).
pub fn page_header(p: &Painter, r: Rect, title: &str, sub: &str, sub_col: Color32) -> Rect {
    p.text(r.min + Vec2::new(24.0, 16.0), Align2::LEFT_TOP, title, bold(20.0), TEXT);
    text_fit(p, r.min + Vec2::new(24.0, 43.0), Align2::LEFT_TOP, sub, font(13.0), sub_col, r.width() - 220.0);
    hairline(p, Pos2::new(r.left(), r.top() + PAGE_HEADER_H), Pos2::new(r.right(), r.top() + PAGE_HEADER_H));
    Rect::from_min_max(Pos2::new(r.right() - 24.0, r.top() + 20.0), Pos2::new(r.right() - 24.0, r.top() + 48.0))
}

/// A window page's footer strip (darker, with a top hairline). Returns its rect.
pub fn page_footer(p: &Painter, r: Rect, window_radius: f32) -> Rect {
    let f = Rect::from_min_max(Pos2::new(r.left(), r.bottom() - PAGE_FOOTER_H), r.max);
    let rr = window_radius.round().clamp(0.0, 255.0) as u8;
    p.rect_filled(f, CornerRadius { nw: 0, ne: 0, sw: 0, se: rr }, FOOTER);
    p.line_segment([f.left_top(), f.right_top()], Stroke::new(1.0, white(20)));
    f
}

/// Height of [`sheet_header`].
pub const SHEET_HEADER_H: f32 = 58.0;

/// A label/value row inside a group card, with an optional hint line under it.
/// Returns the height used.
pub fn fact_row(p: &Painter, r: Rect, label: &str, value: &str, value_col: Color32, hint: &str) -> f32 {
    let top = r.top() + 8.0;
    let vr = p.text(Pos2::new(r.right() - 12.0, top), Align2::RIGHT_TOP, value, bold(13.0), value_col);
    text_fit(p, Pos2::new(r.left() + 12.0, top), Align2::LEFT_TOP, label, font(13.0), TEXT_DIM, (vr.left() - r.left() - 24.0).max(40.0));
    if hint.is_empty() {
        return 32.0;
    }
    let h = paragraph(p, Pos2::new(r.left() + 12.0, top + 19.0), hint, font(12.0), TEXT_FAINT, r.width() - 24.0);
    27.0 + h + 8.0
}

/// A legend row: colour square, label, value.
pub fn legend_row(p: &Painter, r: Rect, color: Color32, label: &str, value: &str, value_col: Color32) {
    let c = Pos2::new(r.left() + 16.5, r.center().y);
    p.rect_filled(Rect::from_center_size(c, Vec2::splat(9.0)), 3.0, color);
    let vr = p.text(Pos2::new(r.right() - 12.0, r.center().y), Align2::RIGHT_CENTER, value, font(13.0), value_col);
    text_fit(p, Pos2::new(r.left() + 31.0, r.center().y), Align2::LEFT_CENTER, label, font(13.0), TEXT, (vr.left() - r.left() - 40.0).max(40.0));
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
    // Control Center additions
    Map,
    Pulse,
    Crop,
    Clipboard,
    Fan,
    Cup,
    Home,
    Window,
    Close,
    Refresh,
    Search,
    Grip,
}

/// The SF Symbol behind each icon (outline variants: icons are monochrome strokes).
pub fn symbol_name(icon: Icon) -> Option<&'static str> {
    Some(match icon {
        Icon::Disk => "internaldrive",
        Icon::Memory => "memorychip",
        Icon::Battery(l) => match l {
            88..=255 => "battery.100percent",
            63..=87 => "battery.75percent",
            38..=62 => "battery.50percent",
            13..=37 => "battery.25percent",
            _ => "battery.0percent",
        },
        Icon::Cpu => "cpu",
        Icon::Wifi => "wifi",
        Icon::Bluetooth => "dot.radiowaves.left.and.right",
        Icon::Headphones => "headphones",
        Icon::Shield => "checkmark.shield",
        Icon::Sparkle => "sparkle",
        Icon::Broom => "bubbles.and.sparkles",
        Icon::Gear => "gearshape",
        Icon::Trash => "trash",
        Icon::Folder => "folder",
        Icon::Lens => "magnifyingglass",
        Icon::Laptop => "laptopcomputer",
        Icon::Heart => "heart",
        Icon::Thermo => "thermometer.medium",
        Icon::Clock => "clock",
        Icon::Up => "arrow.up",
        Icon::Down => "arrow.down",
        Icon::Back => "chevron.left",
        Icon::Forward => "chevron.right",
        Icon::Info => "info.circle",
        Icon::Check => "checkmark",
        Icon::Question => "questionmark",
        Icon::Lock => "lock",
        Icon::Monitor => "menubar.rectangle",
        Icon::Chart => "chart.bar",
        Icon::Power => "power",
        Icon::Map => "square.split.2x2",
        Icon::Pulse => "waveform.path.ecg",
        Icon::Crop => "crop",
        Icon::Clipboard => "doc.on.clipboard",
        Icon::Fan => "fan",
        Icon::Cup => "cup.and.saucer",
        Icon::Home => "house",
        Icon::Window => "macwindow",
        Icon::Close => "xmark",
        Icon::Refresh => "arrow.clockwise",
        Icon::Search => "magnifyingglass",
        Icon::Grip => "circle.grid.2x3.fill",
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

/// Stroke-drawn fallbacks on a 24-unit grid (same shapes as the design's icons).
fn icon_drawn(p: &Painter, icon: Icon, rect: Rect, color: Color32) {
    let c = rect.center();
    let s = rect.width().min(rect.height());
    let u = s / 24.0;
    let pt = |x: f32, y: f32| c + Vec2::new((x - 12.0) * u, (y - 12.0) * u);
    let w = (1.8 * u).max(1.1);
    let st = Stroke::new(w, color);
    let open = |pts: &[(f32, f32)]| {
        p.add(PathShape::line(pts.iter().map(|&(x, y)| pt(x, y)).collect(), PathStroke::new(w, color)));
    };
    let closed = |pts: &[(f32, f32)]| {
        p.add(PathShape::closed_line(pts.iter().map(|&(x, y)| pt(x, y)).collect(), PathStroke::new(w, color)));
    };
    let boxr = |x0: f32, y0: f32, x1: f32, y1: f32, r: f32| {
        p.rect_stroke(Rect::from_min_max(pt(x0, y0), pt(x1, y1)), r * u, st, StrokeKind::Middle);
    };
    let arc = |cx: f32, cy: f32, r: f32, a0: f32, a1: f32| {
        p.add(PathShape::line(arc_points(pt(cx, cy), r * u, a0, a1, 24), PathStroke::new(w, color)));
    };
    let dot = |x: f32, y: f32, r: f32| p.circle_filled(pt(x, y), r * u, color);
    match icon {
        Icon::Disk => {
            boxr(3.0, 13.0, 21.0, 19.0, 1.5);
            open(&[(3.0, 13.0), (6.0, 5.0), (18.0, 5.0), (21.0, 13.0)]);
            dot(7.0, 16.0, 1.1);
        }
        Icon::Memory => {
            boxr(4.0, 7.0, 20.0, 17.0, 1.5);
            for x in [8.0, 12.0, 16.0] {
                open(&[(x, 7.0), (x, 17.0)]);
            }
        }
        Icon::Battery(level) => {
            boxr(3.0, 8.0, 18.0, 16.0, 2.0);
            open(&[(21.0, 11.0), (21.0, 13.0)]);
            let f = level.min(100) as f32 / 100.0;
            let fill = if level <= 20 { RED } else { color };
            p.rect_filled(Rect::from_min_max(pt(5.0, 10.0), pt(5.0 + 11.0 * f, 14.0)), 1.0 * u, fill);
        }
        Icon::Cpu => {
            boxr(7.0, 7.0, 17.0, 17.0, 1.5);
            boxr(10.0, 10.0, 14.0, 14.0, 0.5);
            for o in [9.0, 15.0] {
                open(&[(o, 3.0), (o, 7.0)]);
                open(&[(o, 17.0), (o, 21.0)]);
                open(&[(3.0, o), (7.0, o)]);
                open(&[(17.0, o), (21.0, o)]);
            }
        }
        Icon::Wifi => {
            for r in [4.0f32, 8.5, 13.0] {
                arc(12.0, 19.5, r, -PI * 0.78, -PI * 0.22);
            }
            dot(12.0, 19.5, 1.3);
        }
        Icon::Bluetooth => open(&[(7.0, 7.5), (17.0, 16.0), (12.0, 20.5), (12.0, 3.5), (17.0, 8.0), (7.0, 16.5)]),
        Icon::Headphones => {
            arc(12.0, 12.0, 8.0, PI, TAU);
            open(&[(4.0, 12.0), (4.0, 15.0)]);
            open(&[(20.0, 12.0), (20.0, 15.0)]);
            boxr(4.0, 15.0, 7.0, 20.0, 1.0);
            boxr(17.0, 15.0, 20.0, 20.0, 1.0);
        }
        Icon::Shield => {
            closed(&[(12.0, 3.0), (19.5, 6.0), (18.5, 13.0), (12.0, 21.0), (5.5, 13.0), (4.5, 6.0)]);
            open(&[(8.5, 12.0), (11.0, 14.5), (15.5, 9.5)]);
        }
        Icon::Sparkle => closed(&[(12.0, 3.0), (13.8, 8.2), (19.0, 10.0), (13.8, 11.8), (12.0, 17.0), (10.2, 11.8), (5.0, 10.0), (10.2, 8.2)]),
        Icon::Broom => {
            open(&[(18.0, 3.0), (12.0, 9.0)]);
            closed(&[(9.0, 8.0), (16.0, 15.0), (13.0, 21.0), (3.0, 11.0)]);
        }
        Icon::Gear => {
            p.circle_stroke(pt(12.0, 12.0), 3.0 * u, st);
            p.circle_stroke(pt(12.0, 12.0), 7.0 * u, st);
            for i in 0..8 {
                let a = i as f32 / 8.0 * TAU;
                let d = Vec2::new(a.cos(), a.sin());
                p.line_segment([pt(12.0, 12.0) + d * 7.0 * u, pt(12.0, 12.0) + d * 9.5 * u], Stroke::new(w * 1.4, color));
            }
        }
        Icon::Trash => {
            open(&[(4.0, 6.0), (20.0, 6.0)]);
            open(&[(9.0, 6.0), (9.0, 3.5), (15.0, 3.5), (15.0, 6.0)]);
            open(&[(6.0, 6.0), (7.0, 20.5), (17.0, 20.5), (18.0, 6.0)]);
        }
        Icon::Folder => closed(&[(3.0, 6.0), (9.5, 6.0), (11.5, 8.0), (21.0, 8.0), (21.0, 19.0), (3.0, 19.0)]),
        Icon::Lens | Icon::Search => {
            p.circle_stroke(pt(11.0, 11.0), 7.0 * u, st);
            open(&[(16.0, 16.0), (21.0, 21.0)]);
        }
        Icon::Laptop => {
            boxr(5.0, 5.0, 19.0, 15.0, 1.5);
            open(&[(2.5, 18.5), (21.5, 18.5)]);
        }
        Icon::Heart => {
            let mut pts = Vec::new();
            for i in 0..48 {
                let t = i as f32 / 48.0 * TAU;
                let x = 16.0 * t.sin().powi(3);
                let y = -(13.0 * t.cos() - 5.0 * (2.0 * t).cos() - 2.0 * (3.0 * t).cos() - (4.0 * t).cos());
                pts.push(pt(12.0 + x * 0.5, 12.0 + y * 0.5));
            }
            p.add(PathShape::closed_line(pts, PathStroke::new(w, color)));
        }
        Icon::Thermo => {
            boxr(10.0, 3.0, 14.0, 15.0, 2.0);
            p.circle_stroke(pt(12.0, 18.0), 3.0 * u, st);
            open(&[(12.0, 8.0), (12.0, 16.0)]);
        }
        Icon::Clock => {
            p.circle_stroke(pt(12.0, 12.0), 9.0 * u, st);
            open(&[(12.0, 7.0), (12.0, 12.0), (15.5, 14.0)]);
        }
        Icon::Up => {
            open(&[(12.0, 20.0), (12.0, 4.5)]);
            open(&[(6.5, 10.0), (12.0, 4.5), (17.5, 10.0)]);
        }
        Icon::Down => {
            open(&[(12.0, 4.0), (12.0, 19.5)]);
            open(&[(6.5, 14.0), (12.0, 19.5), (17.5, 14.0)]);
        }
        Icon::Back => open(&[(15.0, 6.0), (9.0, 12.0), (15.0, 18.0)]),
        Icon::Forward => open(&[(9.0, 6.0), (15.0, 12.0), (9.0, 18.0)]),
        Icon::Info => {
            p.circle_stroke(pt(12.0, 12.0), 9.0 * u, st);
            dot(12.0, 7.8, 1.2);
            open(&[(12.0, 11.0), (12.0, 16.5)]);
        }
        Icon::Check => open(&[(5.0, 12.0), (10.0, 17.0), (19.0, 7.0)]),
        Icon::Question => {
            arc(12.0, 9.0, 4.0, PI * 1.05, PI * 2.45);
            open(&[(13.5, 12.8), (12.0, 14.0), (12.0, 15.5)]);
            dot(12.0, 19.0, 1.2);
        }
        Icon::Lock => {
            arc(12.0, 10.0, 4.0, PI, TAU);
            open(&[(8.0, 10.0), (8.0, 11.0)]);
            open(&[(16.0, 10.0), (16.0, 11.0)]);
            boxr(5.0, 11.0, 19.0, 20.0, 2.0);
        }
        Icon::Monitor | Icon::Window => {
            boxr(3.0, 5.0, 21.0, 19.0, 2.0);
            open(&[(3.0, 9.0), (21.0, 9.0)]);
        }
        Icon::Chart => {
            for (i, h) in [6.0f32, 11.0, 8.0, 14.0].iter().enumerate() {
                let x = 5.0 + i as f32 * 4.5;
                open(&[(x, 20.0), (x, 20.0 - h)]);
            }
        }
        Icon::Power => {
            arc(12.0, 13.0, 7.5, -PI * 0.3, PI * 1.3);
            open(&[(12.0, 3.0), (12.0, 12.0)]);
        }
        Icon::Map => {
            boxr(3.0, 4.0, 21.0, 20.0, 1.5);
            open(&[(3.0, 12.0), (12.0, 12.0)]);
            open(&[(12.0, 4.0), (12.0, 20.0)]);
            open(&[(12.0, 9.0), (21.0, 9.0)]);
        }
        Icon::Pulse => open(&[(3.0, 12.0), (7.0, 12.0), (10.0, 5.0), (14.0, 19.0), (17.0, 12.0), (21.0, 12.0)]),
        Icon::Crop => {
            open(&[(6.0, 2.0), (6.0, 18.0), (22.0, 18.0)]);
            open(&[(2.0, 6.0), (18.0, 6.0), (18.0, 22.0)]);
        }
        Icon::Clipboard => {
            boxr(9.0, 3.5, 15.0, 6.5, 0.8);
            open(&[(7.0, 5.0), (5.0, 5.0), (5.0, 21.0), (19.0, 21.0), (19.0, 5.0), (17.0, 5.0)]);
        }
        Icon::Fan => {
            p.circle_stroke(pt(12.0, 12.0), 2.0 * u, st);
            for k in 0..4 {
                let a0 = k as f32 * PI / 2.0 - PI / 2.0;
                let d = Vec2::new(a0.cos(), a0.sin());
                let n = Vec2::new(-d.y, d.x);
                let base = pt(12.0, 12.0);
                let tip = base + d * 9.0 * u;
                let side = base + d * 5.0 * u + n * 3.5 * u;
                p.add(PathShape::line(vec![base + d * 2.0 * u, side, tip, base + d * 2.5 * u + n * 0.5 * u], PathStroke::new(w, color)));
            }
        }
        Icon::Cup => {
            open(&[(4.0, 9.0), (16.0, 9.0), (16.0, 14.0)]);
            arc(11.0, 14.0, 5.0, 0.0, PI);
            open(&[(4.0, 9.0), (4.0, 14.0)]);
            arc(18.0, 13.0, 2.0, -PI / 2.0, PI / 2.0);
            open(&[(8.0, 3.0), (8.0, 6.0)]);
            open(&[(12.0, 3.0), (12.0, 6.0)]);
        }
        Icon::Home => closed(&[(3.0, 11.0), (12.0, 4.0), (21.0, 11.0), (21.0, 20.0), (3.0, 20.0)]),
        Icon::Close => {
            open(&[(6.0, 6.0), (18.0, 18.0)]);
            open(&[(18.0, 6.0), (6.0, 18.0)]);
        }
        Icon::Refresh => {
            arc(12.0, 12.0, 8.0, -PI * 0.25, PI * 1.6);
            open(&[(20.0, 4.0), (20.0, 9.0), (15.0, 9.0)]);
        }
        Icon::Grip => {
            for (x, y) in [(8.0, 6.0), (16.0, 6.0), (8.0, 12.0), (16.0, 12.0), (8.0, 18.0), (16.0, 18.0)] {
                dot(x, y, 1.6);
            }
        }
    }
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
