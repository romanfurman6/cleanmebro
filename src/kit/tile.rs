//! Building blocks for panel widgets at the grid sizes (see
//! `core::tool::Size`): the card, the standard Small / Wide / Row layouts,
//! the header of a Medium or Large widget, rings and sparklines.

use super::glass::*;
use egui::{Align2, Color32, Pos2, Rect, Response, Sense, Shape, Stroke, StrokeKind, Ui, Vec2};

/// Corner radius of a widget.
pub const R_TILE: f32 = 14.0;
/// Inner padding of a widget.
pub const PAD: f32 = 10.0;
/// Height of the header line of a Medium or Large widget.
pub const HEAD_H: f32 = 22.0;

/// The card behind a widget. Hover lifts it; `selected` (its sheet is
/// open) adds the blue ring. A click is the widget's main action.
pub fn card(ui: &mut Ui, rect: Rect, id: egui::Id, selected: bool) -> Response {
    let resp = ui.interact(rect, id, Sense::click());
    let h = ui.ctx().animate_bool(id.with("hover"), resp.hovered());
    paint_card(ui.painter(), rect, h, selected);
    if resp.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    resp
}

pub fn paint_card(p: &egui::Painter, rect: Rect, hover: f32, selected: bool) {
    p.rect_filled(rect, R_TILE, lerp_color(CARD, RAISED, hover));
    p.rect_stroke(rect, R_TILE, Stroke::new(1.0, white(HAIRLINE)), StrokeKind::Inside);
    if selected {
        p.rect_stroke(rect, R_TILE, Stroke::new(1.5, ACCENT), StrokeKind::Inside);
    }
}

/// A card with a soft glow of `color` (a state that matters right now:
/// awake, paused, fans at max).
pub fn glow_card(ui: &mut Ui, rect: Rect, id: egui::Id, color: Color32, selected: bool) -> Response {
    let resp = ui.interact(rect, id, Sense::click());
    let h = ui.ctx().animate_bool(id.with("hover"), resp.hovered());
    let p = ui.painter();
    p.rect_filled(rect, R_TILE, lerp_color(lerp_color(CARD, color, 0.12), RAISED, h * 0.5));
    p.rect_stroke(rect, R_TILE, Stroke::new(1.0, with_alpha(color, 0.5)), StrokeKind::Inside);
    if selected {
        p.rect_stroke(rect, R_TILE, Stroke::new(1.5, ACCENT), StrokeKind::Inside);
    }
    if resp.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    resp
}

/// Small (1:1): tint chip on top, a bold line and a dim line at the bottom.
#[allow(clippy::too_many_arguments)]
pub fn small(ui: &mut Ui, rect: Rect, id: egui::Id, glyph: Icon, color: Color32, title: &str, sub: &str, selected: bool) -> Response {
    let resp = card(ui, rect, id, selected);
    let p = ui.painter();
    tool_chip(p, Rect::from_min_size(rect.min + Vec2::splat(9.0), Vec2::splat(24.0)), glyph, color);
    small_text(p, rect, title, sub, TEXT_DIM);
    resp
}

/// The two lines at the bottom of a Small widget.
pub fn small_text(p: &egui::Painter, rect: Rect, title: &str, sub: &str, sub_col: Color32) {
    let w = rect.width() - 16.0;
    text_fit(p, Pos2::new(rect.left() + 9.0, rect.bottom() - 25.0), Align2::LEFT_BOTTOM, title, bold(12.0), TEXT, w);
    text_fit(p, Pos2::new(rect.left() + 9.0, rect.bottom() - 9.0), Align2::LEFT_BOTTOM, sub, font(11.0), sub_col, w);
}

/// Small (1:1) gauge: a ring with the glyph inside and a caption under it.
#[allow(clippy::too_many_arguments)]
pub fn small_ring(ui: &mut Ui, rect: Rect, id: egui::Id, glyph: Icon, frac: f32, color: Color32, caption: &str, selected: bool) -> Response {
    let resp = card(ui, rect, id, selected);
    let p = ui.painter();
    let c = Pos2::new(rect.center().x, rect.top() + 32.0);
    ring(p, c, 22.0, 5.5, frac, color);
    icon(p, glyph, Rect::from_center_size(c, Vec2::splat(17.0)), TEXT_SOFT);
    text_fit(p, Pos2::new(rect.center().x, rect.bottom() - 9.0), Align2::CENTER_BOTTOM, caption, font(11.0), TEXT_SOFT, rect.width() - 10.0);
    resp
}

/// Wide (1:2): small chip and label on top, a big value, then a thin bar
/// (or a dim caption).
#[allow(clippy::too_many_arguments)]
pub fn wide(ui: &mut Ui, rect: Rect, id: egui::Id, glyph: Icon, color: Color32, label: &str, value: &str, bar_or_caption: Result<(f32, Color32), &str>, selected: bool) -> Response {
    let resp = card(ui, rect, id, selected);
    let p = ui.painter();
    let x0 = rect.left() + PAD;
    let x1 = rect.right() - PAD;
    tool_chip(p, Rect::from_min_size(Pos2::new(x0, rect.top() + 9.0), Vec2::splat(18.0)), glyph, color);
    text_fit(p, Pos2::new(x0 + 24.0, rect.top() + 18.0), Align2::LEFT_CENTER, label, font(11.0), TEXT_DIM, x1 - x0 - 24.0);
    match bar_or_caption {
        Ok((f, c)) => {
            text_fit(p, Pos2::new(x0, rect.bottom() - 18.0), Align2::LEFT_BOTTOM, value, bold(16.0), TEXT, x1 - x0);
            bar(p, Rect::from_min_max(Pos2::new(x0, rect.bottom() - 13.0), Pos2::new(x1, rect.bottom() - 9.0)), f, c);
        }
        Err(cap) => {
            text_fit(p, Pos2::new(x0, rect.bottom() - 25.0), Align2::LEFT_BOTTOM, value, bold(16.0), TEXT, x1 - x0);
            text_fit(p, Pos2::new(x0, rect.bottom() - 9.0), Align2::LEFT_BOTTOM, cap, font(11.0), TEXT_FAINT, x1 - x0);
        }
    }
    resp
}

/// Row (1:4): a big chip, the title with its value on the right, a thin bar
/// (or a caption) under them.
#[allow(clippy::too_many_arguments)]
pub fn row(ui: &mut Ui, rect: Rect, id: egui::Id, glyph: Icon, color: Color32, title: &str, value: &str, bar_or_caption: Result<(f32, Color32), &str>, selected: bool) -> Response {
    let resp = card(ui, rect, id, selected);
    let p = ui.painter();
    let chip = Rect::from_min_size(Pos2::new(rect.left() + 12.0, rect.center().y - 17.0), Vec2::splat(34.0));
    tool_chip(p, chip, glyph, color);
    row_text(p, rect, chip.right() + 12.0, title, value, bar_or_caption);
    resp
}

/// The text part of a Row from `x0` to the right edge.
pub fn row_text(p: &egui::Painter, rect: Rect, x0: f32, title: &str, value: &str, bar_or_caption: Result<(f32, Color32), &str>) {
    let x1 = rect.right() - 12.0;
    let cy = rect.center().y - 7.0;
    let vr = text_fit(p, Pos2::new(x1, cy), Align2::RIGHT_CENTER, value, font(12.0), TEXT_DIM, (x1 - x0) * 0.6);
    text_fit(p, Pos2::new(x0, cy), Align2::LEFT_CENTER, title, bold(13.0), TEXT, vr.left() - x0 - 8.0);
    match bar_or_caption {
        Ok((f, c)) => bar(p, Rect::from_min_max(Pos2::new(x0, cy + 13.0), Pos2::new(x1, cy + 17.0)), f, c),
        Err(cap) => {
            text_fit(p, Pos2::new(x0, cy + 17.0), Align2::LEFT_CENTER, cap, font(11.0), TEXT_FAINT, x1 - x0);
        }
    }
}

/// The header line of a Medium or Large widget: chip, title, a dim note on
/// the right. Returns the rect left for the body.
pub fn head(p: &egui::Painter, rect: Rect, glyph: Icon, color: Color32, title: &str, note: &str) -> Rect {
    let x0 = rect.left() + PAD;
    let x1 = rect.right() - PAD;
    let cy = rect.top() + PAD + HEAD_H / 2.0;
    tool_chip(p, Rect::from_center_size(Pos2::new(x0 + 9.0, cy), Vec2::splat(18.0)), glyph, color);
    let nr = text_fit(p, Pos2::new(x1, cy), Align2::RIGHT_CENTER, note, font(11.0), TEXT_FAINT, (x1 - x0) * 0.5);
    text_fit(p, Pos2::new(x0 + 26.0, cy), Align2::LEFT_CENTER, title, bold(13.0), TEXT, nr.left() - x0 - 34.0);
    Rect::from_min_max(Pos2::new(x0, rect.top() + PAD + HEAD_H + 8.0), Pos2::new(x1, rect.bottom() - PAD))
}

/// A progress ring from 12 o'clock, clockwise, on a faint track.
pub fn ring(p: &egui::Painter, c: Pos2, r: f32, width: f32, frac: f32, color: Color32) {
    p.circle_stroke(c, r, Stroke::new(width, white(26)));
    let f = frac.clamp(0.0, 1.0);
    if f > 0.005 {
        let a0 = -std::f32::consts::FRAC_PI_2;
        let n = ((f * 64.0) as usize).max(3);
        p.add(Shape::line(arc_points(c, r, a0, a0 + f * std::f32::consts::TAU, n), Stroke::new(width, color)));
        for q in [c + Vec2::new(a0.cos(), a0.sin()) * r, c + Vec2::angled(a0 + f * std::f32::consts::TAU) * r] {
            p.circle_filled(q, width / 2.0, color);
        }
    }
}

/// A line chart of `values` scaled into `r`, with an optional fill under it.
pub fn spark(p: &egui::Painter, r: Rect, values: &[f32], color: Color32, fill: Option<Color32>) {
    if values.len() < 2 {
        p.line_segment([r.left_bottom(), r.right_bottom()], Stroke::new(1.0, white(20)));
        return;
    }
    let lo = values.iter().copied().fold(f32::MAX, f32::min);
    let hi = values.iter().copied().fold(f32::MIN, f32::max);
    let span = (hi - lo).max(1e-3);
    let pts: Vec<Pos2> = values
        .iter()
        .enumerate()
        .map(|(i, v)| Pos2::new(r.left() + r.width() * i as f32 / (values.len() - 1) as f32, r.bottom() - 2.0 - (r.height() - 4.0) * ((v - lo) / span)))
        .collect();
    if let Some(f) = fill {
        for w in pts.windows(2) {
            let quad = vec![w[0], w[1], Pos2::new(w[1].x, r.bottom()), Pos2::new(w[0].x, r.bottom())];
            p.add(Shape::convex_polygon(quad, f, Stroke::NONE));
        }
    }
    p.add(Shape::line(pts, Stroke::new(1.6, color)));
}

/// A list line inside a Medium or Large widget: label, bar, value.
pub fn bar_line(p: &egui::Painter, r: Rect, label: &str, frac: Option<(f32, Color32)>, value: &str) {
    let vw = p.layout_no_wrap(value.to_string(), font(12.0), TEXT_DIM).size().x.clamp(40.0, r.width() * 0.45);
    let bw = if frac.is_some() { (r.width() * 0.3).min(110.0) } else { 0.0 };
    text_fit(p, Pos2::new(r.right(), r.center().y), Align2::RIGHT_CENTER, value, font(12.0), TEXT_DIM, vw);
    if let Some((f, c)) = frac {
        bar(p, Rect::from_min_max(Pos2::new(r.right() - vw - 10.0 - bw, r.center().y - 2.0), Pos2::new(r.right() - vw - 10.0, r.center().y + 2.0)), f, c);
    }
    text_fit(p, Pos2::new(r.left(), r.center().y), Align2::LEFT_CENTER, label, font(12.0), TEXT_SOFT, r.width() - vw - bw - 22.0);
}

/// A dashed box with a quiet message (nothing to show yet).
pub fn empty_note(p: &egui::Painter, r: Rect, msg: &str) {
    dashed_rect(p, r, 8.0, white(40));
    text_fit(p, r.center(), Align2::CENTER_CENTER, msg, font(12.0), TEXT_FAINT, r.width() - 16.0);
}
