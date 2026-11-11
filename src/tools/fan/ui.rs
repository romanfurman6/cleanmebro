//! Fan Control in the panel (quick tile, card) and its detail sheet, plus the
//! drawing pieces the page shares: spinning blades, speed dials, the heat
//! scale and sparklines.

use super::data::{heat, heat_text, rpm_text, Fan, Group, Snapshot};
use super::control::Mode;
use super::{Cmd, HelperState, Live, ID, SHEET};
use crate::core::tool::{Cx, Size};
use crate::kit::tile;
use crate::kit::glass::*;
use egui::epaint::Mesh;
use egui::{Align2, Color32, Pos2, Rect, Shape, Stroke, Ui, Vec2};
use std::f32::consts::{PI, TAU};

/// Height of the card's fans-and-heat part (all of Medium).
pub const CARD_H: f32 = 168.0;
/// Blades, speed rings and lines: system blue (D6 has no per-tool colours).
pub const BLADE: Color32 = crate::kit::glass::ACCENT;
pub const LINE: Color32 = crate::kit::glass::BLUE;
pub const HOT_LINE: Color32 = crate::kit::glass::AMBER;

/// Blade angle of the fan drawn as `key` at time `t`: one turn takes
/// 2400 / rpm seconds, slow enough to read while still following the real
/// speed. The angle is carried forward frame by frame, so a new speed
/// reading (every 2 s) changes how fast the blades turn, not where they are.
pub fn spin(key: &str, t: f64, rpm: f32) -> f32 {
    use std::collections::HashMap;
    use std::sync::Mutex;
    static TURNS: Mutex<Option<HashMap<String, (f64, f64)>>> = Mutex::new(None);
    let mut g = TURNS.lock().unwrap_or_else(|e| e.into_inner());
    let map = g.get_or_insert_with(HashMap::new);
    let (last, turns) = map.entry(key.to_string()).or_insert((t, 0.0));
    // a long gap (panel was hidden): carry on from where it stopped
    let dt = (t - *last).clamp(0.0, 0.1);
    *turns = (*turns + dt * rpm as f64 / 2400.0).fract();
    *last = t;
    (*turns as f32) * TAU
}

/// Ask for the next frame soon while any blade is turning.
pub fn keep_spinning(ui: &Ui, s: &Snapshot) {
    if s.fans.iter().any(|f| f.rpm > 0.0) {
        crate::kit::glass::animate(ui.ctx());
    }
}

/// Five blades around a hub, turned by `angle`.
pub fn blades(p: &egui::Painter, c: Pos2, r: f32, angle: f32, color: Color32, hub: Color32) {
    for k in 0..5 {
        let a = angle + k as f32 * TAU / 5.0;
        let dir = Vec2::angled(a);
        let center = c + dir * r * 0.52;
        // an ellipse along the blade, leaned back a little like a real blade
        let lean = a + 0.35;
        let (ax, ay) = (Vec2::angled(lean) * r * 0.46, Vec2::angled(lean + PI / 2.0) * r * 0.19);
        let pts: Vec<Pos2> = (0..16).map(|i| {
            let u = i as f32 / 16.0 * TAU;
            center + ax * u.cos() + ay * u.sin()
        }).collect();
        p.add(Shape::convex_polygon(pts, color, Stroke::NONE));
    }
    p.circle_filled(c, r * 0.24, hub);
    crate::kit::glass::stroke_circle(&p, c, r * 0.24, Stroke::new(1.0, white(46)));
}

/// A 270° dial: track plus the filled part for `frac`.
pub fn dial(p: &egui::Painter, c: Pos2, r: f32, width: f32, frac: f32, color: Color32) {
    let a0 = 0.75 * PI;
    let span = 1.5 * PI;
    let track = arc_points(c, r, a0, a0 + span, 64);
    p.add(Shape::line(track.clone(), Stroke::new(width, white(20))));
    p.circle_filled(track[0], width / 2.0, white(20));
    p.circle_filled(*track.last().unwrap(), width / 2.0, white(20));
    let f = frac.clamp(0.02, 1.0);
    let fill = arc_points(c, r, a0, a0 + span * f, 64);
    p.add(Shape::line(fill.clone(), Stroke::new(width, color)));
    p.circle_filled(fill[0], width / 2.0, color);
    p.circle_filled(*fill.last().unwrap(), width / 2.0, color);
}

/// A full ring for small fan icons: track and filled part, starting at 12 o'clock.
pub fn ring(p: &egui::Painter, c: Pos2, r: f32, width: f32, frac: f32, color: Color32) {
    crate::kit::glass::stroke_circle(&p, c, r, Stroke::new(width, white(20)));
    let a0 = -PI / 2.0;
    let pts = arc_points(c, r, a0, a0 + TAU * frac.clamp(0.02, 1.0), 48);
    p.add(Shape::line(pts, Stroke::new(width, color)));
}

/// The heat scale from 30 °C to 100 °C as a horizontal gradient.
pub fn heat_scale(p: &egui::Painter, r: Rect) {
    let mut m = Mesh::default();
    let n = 24;
    for i in 0..=n {
        let f = i as f32 / n as f32;
        let x = r.left() + r.width() * f;
        let col = heat(30.0 + 70.0 * f);
        m.colored_vertex(Pos2::new(x, r.top()), col);
        m.colored_vertex(Pos2::new(x, r.bottom()), col);
        if i > 0 {
            let b = (i as u32) * 2;
            m.add_triangle(b - 2, b - 1, b);
            m.add_triangle(b - 1, b, b + 1);
        }
    }
    p.add(Shape::mesh(m));
}

/// Where a temperature sits on [`heat_scale`], 0..1.
pub fn heat_pos(c: f32) -> f32 {
    ((c - 30.0) / 70.0).clamp(0.0, 1.0)
}

/// A line through `values` scaled between `lo` and `hi`, optionally filled below.
pub fn sparkline(p: &egui::Painter, r: Rect, values: &[f32], lo: f32, hi: f32, color: Color32, fill: Option<Color32>) {
    if values.len() < 2 {
        return;
    }
    let n = (values.len() - 1) as f32;
    let pts: Vec<Pos2> = values
        .iter()
        .enumerate()
        .map(|(i, v)| {
            let f = ((v - lo) / (hi - lo).max(1.0)).clamp(0.0, 1.0);
            Pos2::new(r.left() + r.width() * i as f32 / n, r.bottom() - 1.0 - (r.height() - 2.0) * f)
        })
        .collect();
    if let Some(fc) = fill {
        let mut m = Mesh::default();
        for (i, q) in pts.iter().enumerate() {
            m.colored_vertex(*q, fc);
            m.colored_vertex(Pos2::new(q.x, r.bottom()), with_alpha(fc, 0.0));
            if i > 0 {
                let b = (i as u32) * 2;
                m.add_triangle(b - 2, b - 1, b);
                m.add_triangle(b - 1, b, b + 1);
            }
        }
        p.add(Shape::mesh(m));
    }
    p.add(Shape::line(pts, Stroke::new(1.8, color)));
}

/// "Left" and "Right" for two fans, otherwise "Fan 1", "Fan 2" …
pub fn fan_name(i: usize, n: usize) -> String {
    match (n, i) {
        (1, _) => "Fan".into(),
        (2, 0) => "Left".into(),
        (2, 1) => "Right".into(),
        _ => format!("Fan {}", i + 1),
    }
}

/// "P-cores 71 °C" for the hottest group.
pub fn hottest_text(s: &Snapshot) -> Option<(String, f32)> {
    s.hottest().map(|(g, c)| (g.label().to_string(), c))
}

// ---------------------------------------------------------------- panel

/// The Fans widget at `size`; a click opens the sheet.
pub fn widget(s: &Snapshot, size: Size, ui: &mut Ui, rect: Rect, cx: &mut Cx) {
    if size == Size::S24 {
        return card(s, ui, rect, cx);
    }
    if size == Size::S44 {
        card(s, ui, rect, cx);
        // every part's temperature under the card's top
        let p = ui.painter();
        let x0 = rect.left() + 12.0;
        let x1 = rect.right() - 12.0;
        let y0 = rect.top() + CARD_H + 4.0;
        hairline(p, Pos2::new(x0, y0 - 6.0), Pos2::new(x1, y0 - 6.0));
        p.text(Pos2::new(x0, y0 + 4.0), Align2::LEFT_TOP, "Temperatures", font(11.0), TEXT_FAINT);
        let groups = s.groups();
        let lh = 24.0;
        let fit = ((rect.bottom() - 10.0 - (y0 + 22.0)) / lh).floor().max(0.0) as usize;
        for (i, g) in groups.iter().take(fit).enumerate() {
            let lr = Rect::from_min_size(Pos2::new(x0, y0 + 22.0 + i as f32 * lh), Vec2::new(x1 - x0, lh));
            tile::bar_line(p, lr, g.group.label(), Some((heat_pos(g.max).max(0.03), heat(g.max))), &format!("{:.0} °C", g.max));
        }
        return;
    }
    let t = ui.ctx().input(|i| i.time);
    let id = ui.scope_id().with("w-fans");
    let sel = cx.is_detail_open(ID, SHEET);
    let rpm = s.avg_rpm();
    let value = if !s.loaded {
        "…".to_string()
    } else if let Some(r) = rpm {
        format!("{} rpm", rpm_text(r))
    } else if let Some((_, c)) = s.hottest() {
        format!("{c:.0} °C")
    } else {
        "No data".into()
    };
    let frac = s.avg_frac().unwrap_or(0.0);
    let hot = hottest_text(s).map(|(g, c)| format!("{g} {c:.0} °C")).unwrap_or_default();
    let resp = match size {
        Size::S11 => {
            let resp = tile::card(ui, rect, id, sel);
            let p = ui.painter();
            let c = Pos2::new(rect.center().x, rect.top() + 32.0);
            ring(p, c, 22.0, 4.0, frac, LINE);
            blades(p, c, 16.0, spin("fan-avg", t, rpm.unwrap_or(0.0)), BLADE, CARD);
            text_fit(p, Pos2::new(rect.center().x, rect.bottom() - 9.0), Align2::CENTER_BOTTOM, &value, font(11.0), TEXT_SOFT, rect.width() - 10.0);
            resp
        }
        Size::S12 => tile::wide(ui, rect, id, Icon::Fan, tint::FAN, "Fans", &value, if rpm.is_some() { Ok((frac.max(0.02), LINE)) } else { Err(hot.as_str()) }, sel),
        _ => {
            // the hottest part is a line on the right; the left says how hard the fans work
            let load = if rpm.is_some() { format!("{:.0}% of max", frac * 100.0) } else { hot.clone() };
            let (resp, right) = tile::row_split(ui, rect, id, Icon::Fan, tint::FAN, "Fans", &value, &load, sel);
            let mut lines: Vec<(String, Option<(f32, Color32)>, String)> = s.fans.iter().take(2).enumerate().map(|(i, f)| (fan_name(i, s.fans.len()), Some((f.frac().max(0.02), LINE)), format!("{} rpm", rpm_text(f.rpm)))).collect();
            if let Some((g, c)) = hottest_text(s) {
                lines.push((g, None, format!("{c:.0} °C")));
            }
            let empty = if !s.loaded { "Reading…" } else if s.has_smc { "No fans · cools passively" } else { "Fan speed unavailable" };
            tile::lines(ui.painter(), right, &lines, empty);
            resp
        }
    };
    // spinning blades over the chip
    if let Some(r) = rpm {
        let p = ui.painter();
        // over the static glyph: the ring's centre (Wide) or the round chip (Row)
        let spot = match size {
            Size::S12 => Some((Pos2::new(rect.left() + tile::PAD + 17.0, rect.center().y), 9.0, 7.0, CARD)),
            Size::S14 => Some((tile::row_chip(rect).center(), 12.0, 10.0, lerp_color(CARD, Color32::WHITE, 14.0 / 255.0))),
            _ => None,
        };
        if let Some((c, cover, br, hub)) = spot {
            p.circle_filled(c, cover, hub);
            blades(p, c, br, spin("fan-avg", t, r), BLADE, hub);
        }
    }
    keep_spinning(ui, s);
    if resp.clicked() {
        cx.toggle_detail(ID, SHEET);
    }
}

/// The full-width card: both fans with spinning dials, the hottest part
/// on the heat scale and its last 10 minutes.
pub fn card(s: &Snapshot, ui: &mut Ui, rect: Rect, cx: &mut Cx) {
    let resp = card_resp(ui, rect, cx.is_detail_open(ID, SHEET));
    if resp.clicked() {
        cx.toggle_detail(ID, SHEET);
    }
    let p = ui.painter().clone();
    let t = ui.ctx().input(|i| i.time);
    let x0 = rect.left() + 12.0;
    let x1 = rect.right() - 12.0;
    let hy = rect.top() + 20.0;
    p.text(Pos2::new(x0, hy), Align2::LEFT_CENTER, "Fans", ty::title(), TEXT);
    let mode = if s.fanless() { "No fans" } else { "Auto" };
    p.text(Pos2::new(x1, hy), Align2::RIGHT_CENTER, mode, font(12.0), TEXT_DIM);

    // fans
    let fy = rect.top() + 38.0;
    let fh = 52.0;
    if !s.loaded {
        spinner(&p, Pos2::new(rect.center().x, fy + fh / 2.0), 8.0, t, TEXT_DIM);
        crate::kit::glass::animate(ui.ctx());
    } else if s.fans.is_empty() {
        let r = Rect::from_min_size(Pos2::new(x0, fy), Vec2::new(x1 - x0, fh));
        p.rect_filled(r, 10.0, white(10));
        let msg = if s.has_smc { "No fans. Cools passively" } else { "Fan speed isn't available on this Mac." };
        text_fit(&p, Pos2::new(r.left() + 12.0, r.center().y), Align2::LEFT_CENTER, msg, font(12.0), TEXT_DIM, r.width() - 24.0);
    } else {
        let n = s.fans.len().min(2);
        let gap = 8.0;
        let bw = (x1 - x0 - gap * (n as f32 - 1.0)) / n as f32;
        for (i, f) in s.fans.iter().take(n).enumerate() {
            let r = Rect::from_min_size(Pos2::new(x0 + i as f32 * (bw + gap), fy), Vec2::new(bw, fh));
            mini_fan(&p, r, f, &fan_name(i, s.fans.len()), t);
        }
        keep_spinning(ui, s);
    }

    // hottest part
    let ty = fy + fh + 12.0;
    match hottest_text(s) {
        Some((g, c)) => {
            p.text(Pos2::new(x0, ty + 7.0), Align2::LEFT_CENTER, format!("Hottest · {g}"), font(12.0), TEXT_DIM);
            p.text(Pos2::new(x1, ty + 7.0), Align2::RIGHT_CENTER, format!("{c:.0} °C"), bold(13.0), heat_text(c));
            let br = Rect::from_min_max(Pos2::new(x0, ty + 20.0), Pos2::new(x1, ty + 26.0));
            heat_scale(&p, br);
            let mx = br.left() + br.width() * heat_pos(c);
            let mr = Rect::from_center_size(Pos2::new(mx, br.center().y), Vec2::new(4.0, 12.0));
            p.rect_filled(mr.expand(2.0), 3.0, CARD);
            p.rect_filled(mr, 2.0, Color32::WHITE);
            let hot: Vec<f32> = s.history.iter().map(|h| h.1).collect();
            let lo = hot.iter().copied().fold(f32::MAX, f32::min).min(c) - 4.0;
            let hi = hot.iter().copied().fold(f32::MIN, f32::max).max(c) + 4.0;
            let bottom = rect.bottom().min(rect.top() + CARD_H) - 10.0;
            sparkline(&p, Rect::from_min_max(Pos2::new(x0, ty + 34.0), Pos2::new(x1, bottom)), &hot, lo, hi, HOT_LINE, None);
        }
        None if s.loaded => {
            p.text(Pos2::new(x0, ty + 7.0), Align2::LEFT_CENTER, "No temperature sensors found", font(12.0), TEXT_DIM);
        }
        None => {}
    }
}

fn card_resp(ui: &mut Ui, rect: Rect, selected: bool) -> egui::Response {
    tile::card(ui, rect, ui.scope_id().with("fan-card"), selected)
}

/// One fan in the panel card: ring with blades, name and speed.
fn mini_fan(p: &egui::Painter, r: Rect, f: &Fan, name: &str, t: f64) {
    p.rect_filled(r, 8.0, white(8));
    let c = Pos2::new(r.left() + 8.0 + 18.0, r.center().y);
    ring(p, c, 17.0, 3.0, f.frac(), LINE);
    blades(p, c, 12.0, spin(name, t, f.rpm), BLADE, CARD);
    let x = c.x + 26.0;
    p.text(Pos2::new(x, r.center().y - 9.0), Align2::LEFT_CENTER, name, font(11.0), TEXT_DIM);
    let g = p.text(Pos2::new(x, r.center().y + 8.0), Align2::LEFT_CENTER, rpm_text(f.rpm), bold(15.0), TEXT);
    p.text(Pos2::new(g.right() + 4.0, r.center().y + 9.0), Align2::LEFT_CENTER, "rpm", font(11.0), TEXT_FAINT);
}

// ---------------------------------------------------------------- sheet

/// The sheet beside the panel: fan speeds with their minimum, every part's
/// temperature, thermal pressure and a way into the page.
pub fn sheet(s: &Snapshot, l: &Live, ui: &mut Ui, rect: Rect, cx: &mut Cx) -> Vec<Cmd> {
    let mut cmds = Vec::new();
    let p = ui.painter().clone();
    let (title, sub) = if s.fans.is_empty() {
        ("Temperatures".to_string(), s.chip.clone())
    } else {
        ("Fans".to_string(), format!("{} · mode {}", plural(s.fans.len(), "fan"), mode_word(l)))
    };
    if sheet_header(ui, rect, Icon::Fan, tint::FAN, &title, &sub) {
        cx.close_detail();
    }
    let x0 = rect.left() + 16.0;
    let w = rect.width() - 32.0;
    let mut y = rect.top() + SHEET_HEADER_H + 16.0;

    if let (Some(avg), Some(frac)) = (s.avg_rpm(), s.avg_frac()) {
        y += hero_number(&p, Pos2::new(x0, y), &rpm_text(avg), "rpm", &format!("{} · {:.0}% of max", if s.fans.len() > 1 { "average" } else { "now" }, frac * 100.0)) + 12.0;
        for (i, f) in s.fans.iter().enumerate() {
            p.text(Pos2::new(x0, y), Align2::LEFT_TOP, fan_name(i, s.fans.len()), font(12.0), TEXT_DIM);
            p.text(Pos2::new(x0 + w, y), Align2::RIGHT_TOP, format!("{} of {}", rpm_text(f.rpm), rpm_text(f.max)), font(12.0), TEXT);
            let br = Rect::from_min_size(Pos2::new(x0, y + 19.0), Vec2::new(w, 8.0));
            p.rect_filled(br, 4.0, white(20));
            let fr = Rect::from_min_size(br.min, Vec2::new((br.width() * f.frac()).max(8.0), br.height()));
            p.rect_filled(fr, 4.0, LINE);
            if f.max > 0.0 {
                let mx = br.left() + br.width() * (f.min / f.max).clamp(0.0, 1.0);
                p.rect_filled(Rect::from_center_size(Pos2::new(mx, br.center().y), Vec2::new(2.0, 12.0)), 1.0, white(140));
            }
            y += 36.0;
        }
        y += 4.0;
        // quick mode switch; anything that needs more (helper, curve) opens the page
        let labels: Vec<&str> = Mode::ALL.iter().map(|m| m.label()).collect();
        if let Some(i) = segmented(ui, Rect::from_min_size(Pos2::new(x0, y), Vec2::new(w, 28.0)), ui.scope_id().with("fan-mode"), &labels, l.settings.mode.index()) {
            let mut st = l.settings.clone();
            st.mode = Mode::ALL[i];
            let open = st.mode == Mode::Curve || (st.mode != Mode::Auto && l.helper != HelperState::Ready);
            cmds.push(Cmd::Set(st, true));
            if open {
                cx.open_page(ID, "");
            }
        }
        y += 40.0;
    } else {
        let msg = if !s.loaded {
            "Reading sensors…"
        } else if s.has_smc {
            "No fans. Cools passively"
        } else {
            "Fan speed isn't available on this Mac."
        };
        let ir = Rect::from_min_size(Pos2::new(x0, y), Vec2::new(w, 56.0));
        p.rect_filled(ir, R_CARD, with_alpha(tint::FAN, 0.14));
        p.rect_stroke(ir, R_CARD, Stroke::new(1.0, with_alpha(tint::FAN, 0.4)), egui::StrokeKind::Inside);
        paragraph(&p, ir.min + Vec2::new(12.0, 10.0), msg, font(12.0), TEXT_SOFT, w - 24.0);
        y = ir.bottom() + 14.0;
        if let Some((g, c)) = hottest_text(s) {
            y += hero_number(&p, Pos2::new(x0, y), &format!("{c:.0}"), "°C", &format!("hottest · {g}")) + 14.0;
        }
    }

    // temperatures by part
    let groups = s.groups();
    let rows = groups.len().min(8);
    if rows > 0 {
        let gr = Rect::from_min_size(Pos2::new(x0, y), Vec2::new(w, rows as f32 * 30.0 + 8.0));
        glass_card(&p, gr, R_CARD, 0.0, false);
        for (i, g) in groups.iter().take(rows).enumerate() {
            let cy = gr.top() + 4.0 + 15.0 + i as f32 * 30.0;
            p.rect_filled(Rect::from_center_size(Pos2::new(gr.left() + 16.5, cy), Vec2::splat(9.0)), 3.0, heat(g.max));
            p.text(Pos2::new(gr.left() + 31.0, cy), Align2::LEFT_CENTER, g.group.label(), font(13.0), TEXT);
            let vr = p.text(Pos2::new(gr.right() - 12.0, cy), Align2::RIGHT_CENTER, format!("{:.0} °C", g.max), font(13.0), heat_text(g.max));
            let br = Rect::from_min_max(Pos2::new(vr.left() - 12.0 - 70.0, cy - 2.0), Pos2::new(vr.left() - 12.0, cy + 2.0));
            bar(&p, br, heat_pos(g.max).max(0.03), heat(g.max));
        }
        y = gr.bottom() + 12.0;
    }

    // thermal pressure
    let (hint, col) = thermal_hint(s.thermal);
    let fr = Rect::from_min_size(Pos2::new(x0, y), Vec2::new(w, 0.0));
    let need = p.layout(hint.to_string(), font(12.0), TEXT_FAINT, w - 24.0).size().y + 35.0;
    let fr = Rect::from_min_size(fr.min, Vec2::new(w, need));
    let by = rect.bottom() - 16.0 - 30.0;
    if fr.bottom() < by - 8.0 {
        glass_card(&p, fr, R_CARD, 0.0, false);
        fact_row(&p, fr, "Thermal pressure", s.thermal_word(), col, hint);
    }

    let label = "Open Fan Control";
    let bw = button_width(&p, label, BTN_PRIMARY);
    if pill_button(ui, Rect::from_min_size(Pos2::new(rect.right() - 16.0 - bw, by), Vec2::new(bw, 30.0)), label, BTN_PRIMARY, true).clicked() {
        cx.open_page(ID, "");
    }
    keep_spinning(ui, s);
    cmds
}

/// "Curve", or "Auto (hot)" while the 95 °C safety holds the fans.
pub fn mode_word(l: &Live) -> String {
    if l.settings.mode != Mode::Auto && l.guard.tripped {
        "Auto (too hot)".into()
    } else if l.needs_helper() {
        format!("{} (needs helper)", l.settings.mode.label())
    } else {
        l.settings.mode.label().into()
    }
}

/// Hint line and colour for a thermal pressure level.
pub fn thermal_hint(level: u8) -> (&'static str, Color32) {
    match level {
        0 => ("No slowdown", GREEN),
        1 => ("Slightly slowed", AMBER),
        2 => ("Slowed to cool down", AMBER),
        _ => ("Heavily slowed", RED),
    }
}

pub fn plural(n: usize, word: &str) -> String {
    if n == 1 {
        format!("1 {word}")
    } else {
        format!("{n} {word}s")
    }
}

/// Whether a group belongs in the heat map's cell blocks.
pub fn is_block(g: Group) -> bool {
    g.is_core()
}
