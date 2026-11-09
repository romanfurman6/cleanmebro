//! Drawing for Keep Awake: the cup-in-a-ring scene, the panel tile and card,
//! and the window page.
//!
//! The scene tells the state at a glance: a moon when the Mac may sleep,
//! a steaming cup when it's kept awake, Claude's spark riding the ring while
//! Claude Code works, and a draining amber ring while it winds down.

use super::engine::{Engine, Span, State, GRACE_CHOICES, WORKING_SECS};
use super::{KeepAwake, ID};
use crate::kit::glass::*;
use egui::epaint::{PathShape, PathStroke};
use egui::{Align2, Color32, Mesh, Painter, Pos2, Rect, Sense, Shape, Stroke, StrokeKind, Ui, Vec2};
use std::f32::consts::{PI, TAU};

const HERO_H: f32 = 120.0;
const ROW_H: f32 = 34.0;
const SESSION_H: f32 = 24.0;
/// Most working sessions listed in the panel card.
const CARD_SESSIONS: usize = 3;

/// Colors of one state.
struct Look {
    /// Ring, accents.
    ring: Color32,
    /// Cup, moon.
    body: Color32,
    /// Steam, second text line.
    soft: Color32,
    /// Hero background.
    bg: Color32,
}

fn look(st: State) -> Look {
    match st {
        State::Off => Look { ring: Color32::from_rgb(0x6E, 0x82, 0xB4), body: Color32::from_rgb(0xA9, 0xB4, 0xCF), soft: Color32::from_rgb(0xC7, 0xCE, 0xDD), bg: Color32::from_rgb(0x1B, 0x1D, 0x24) },
        State::Manual => Look { ring: tint::KEEP_AWAKE, body: Color32::from_rgb(0xF2, 0xC6, 0xD9), soft: Color32::from_rgb(0xF5, 0xD0, 0xE0), bg: Color32::from_rgb(0x24, 0x1A, 0x21) },
        State::Claude => Look { ring: tint::CLAUDE, body: Color32::from_rgb(0xF2, 0xD3, 0xC4), soft: Color32::from_rgb(0xF3, 0xD3, 0xC4), bg: Color32::from_rgb(0x25, 0x1C, 0x18) },
        State::WindDown { .. } => Look { ring: AMBER, body: Color32::from_rgb(0xE8, 0xD6, 0xB0), soft: Color32::from_rgb(0xF0, 0xDD, 0xB5), bg: Color32::from_rgb(0x22, 0x1F, 0x18) },
    }
}

/// One word for the tile and the sidebar.
pub fn short_state(st: State) -> String {
    match st {
        State::Off => "Off".into(),
        State::Manual => "On".into(),
        State::Claude => "Claude".into(),
        State::WindDown { left } => clock(left),
    }
}

/// 4:12
fn clock(secs: i64) -> String {
    let s = secs.max(0);
    format!("{}:{:02}", s / 60, s % 60)
}

/// 1 h 12 m, 12 m, 40 s
fn span_text(secs: i64) -> String {
    let s = secs.max(0);
    let (h, m) = (s / 3600, (s % 3600) / 60);
    if h > 0 {
        format!("{h} h {m:02} m")
    } else if m > 0 {
        format!("{m} m")
    } else {
        format!("{s} s")
    }
}

fn ago(secs: i64) -> String {
    let s = secs.max(0);
    if s < 60 {
        format!("{s} s ago")
    } else if s < 3600 {
        format!("{} min ago", s / 60)
    } else {
        format!("{} h ago", s / 3600)
    }
}

fn time_of_day(t: i64) -> String {
    use chrono::{Local, TimeZone};
    Local.timestamp_opt(t, 0).single().map(|d| d.format("%H:%M").to_string()).unwrap_or_default()
}

/// How far around the ring is filled.
fn ring_frac(e: &Engine, st: State, now: i64) -> f32 {
    match st {
        State::Off => 0.0,
        State::Manual => 1.0,
        // fills once an hour
        State::Claude => {
            let mins = e.awake_since.map(|s| (now - s) as f32 / 60.0).unwrap_or(0.0);
            (0.06 + (mins % 60.0) / 60.0).min(1.0)
        }
        State::WindDown { left } => (left as f32 / (e.settings.grace_min.max(1) as f32 * 60.0)).clamp(0.02, 1.0),
    }
}

// ---------------------------------------------------------------- the scene

/// A soft round glow behind the scene (a radial fade), clipped to `clip`.
fn glow(p: &Painter, clip: Rect, center: Pos2, radius: f32, color: Color32) {
    let p = p.with_clip_rect(clip.intersect(p.clip_rect()));
    let mut mesh = Mesh::default();
    let n = 64u32;
    // centre, a mid ring and the rim: a softer falloff than one linear step
    mesh.colored_vertex(center, with_alpha(color, 0.42));
    for (k, (rf, a)) in [(0.45f32, 0.20f32), (1.0, 0.0)].into_iter().enumerate() {
        for i in 0..n {
            let ang = TAU * i as f32 / n as f32;
            mesh.colored_vertex(center + Vec2::new(ang.cos(), ang.sin()) * radius * rf, with_alpha(color, a));
        }
        let ring = 1 + k as u32 * n;
        for i in 0..n {
            let j = (i + 1) % n;
            if k == 0 {
                mesh.add_triangle(0, ring + i, ring + j);
            } else {
                let inner = ring - n;
                mesh.add_triangle(inner + i, ring + i, ring + j);
                mesh.add_triangle(inner + i, ring + j, inner + j);
            }
        }
    }
    p.add(Shape::mesh(mesh));
}

/// A crescent: the part of circle (c1, r1) outside circle (c2, r2).
fn crescent(p: &Painter, c1: Pos2, r1: f32, c2: Pos2, r2: f32, color: Color32) {
    let dv = c2 - c1;
    let d = dv.length();
    if d <= 0.0 || d >= r1 + r2 || d <= (r1 - r2).abs() {
        p.circle_filled(c1, r1, color);
        return;
    }
    let a = (r1 * r1 - r2 * r2 + d * d) / (2.0 * d);
    let h = (r1 * r1 - a * a).max(0.0).sqrt();
    let base = c1 + dv / d * a;
    let perp = Vec2::new(-dv.y, dv.x) / d;
    let (ta, tb) = (base + perp * h, base - perp * h);
    // both arcs run from tip A to tip B through the direction away from c2
    let away = (c1 - c2).angle();
    let arc = |c: Pos2, r: f32, n: usize| -> Vec<Pos2> {
        let s = (ta - c).angle();
        let e = (tb - c).angle();
        let ccw = (e - s).rem_euclid(TAU);
        let sweep = if (away - s).rem_euclid(TAU) < ccw { ccw } else { ccw - TAU };
        (0..=n).map(|i| c + Vec2::angled(s + sweep * i as f32 / n as f32) * r).collect()
    };
    let n = 40;
    let (outer, inner) = (arc(c1, r1, n), arc(c2, r2, n));
    let mut mesh = Mesh::default();
    for (o, i) in outer.iter().zip(&inner) {
        mesh.colored_vertex(*o, color);
        mesh.colored_vertex(*i, color);
    }
    for i in 0..n as u32 {
        let k = i * 2;
        mesh.add_triangle(k, k + 1, k + 2);
        mesh.add_triangle(k + 1, k + 3, k + 2);
    }
    p.add(Shape::mesh(mesh));
    // meshes have no anti-aliasing; a hairline along both edges smooths them
    p.add(PathShape::line(outer, PathStroke::new(1.0, color)));
    p.add(PathShape::line(inner, PathStroke::new(1.0, color)));
}

/// The cup-in-a-ring scene drawn in the square `rect` on a `look.bg` ground.
fn scene(p: &Painter, rect: Rect, st: State, frac: f32, t: f64) {
    let lk = look(st);
    let s = rect.width() / 96.0;
    let at = |x: f32, y: f32| rect.min + Vec2::new(x * s, y * s);
    let c = at(48.0, 48.0);
    let rr = 42.0 * s;
    p.circle_stroke(c, rr, Stroke::new(4.0 * s, white(26)));
    let a0 = -PI / 2.0;
    let a1 = a0 + TAU * frac;
    if frac >= 0.999 {
        p.circle_stroke(c, rr, Stroke::new(4.0 * s, lk.ring));
    } else if frac > 0.0 {
        p.add(PathShape::line(arc_points(c, rr, a0, a1, 72), PathStroke::new(4.0 * s, lk.ring)));
        for a in [a0, a1] {
            p.circle_filled(c + Vec2::new(a.cos(), a.sin()) * rr, 2.0 * s, lk.ring);
        }
    }

    if st == State::Off {
        // crescent moon, twinkling stars, two z's
        crescent(p, at(52.0, 50.0), 22.0 * s, at(62.0, 41.0), 18.5 * s, lk.body);
        for (k, (x, y, r)) in [(26.0, 30.0, 1.8), (72.0, 26.0, 1.4), (28.0, 68.0, 1.4)].iter().enumerate() {
            let tw = 0.25 + 0.75 * (0.5 + 0.5 * ((t / 3.0 * TAU as f64) + k as f64 * 2.1).sin()) as f32;
            p.circle_filled(at(*x, *y), *r * s, with_alpha(Color32::from_rgb(0xDD, 0xE3, 0xF0), tw));
        }
        p.text(at(66.0, 46.0), Align2::LEFT_BOTTOM, "z", bold(9.0 * s), lk.ring);
        p.text(at(73.0, 37.0), Align2::LEFT_BOTTOM, "z", bold(7.0 * s), lk.ring);
        return;
    }

    // steam: three wisps that rise and fade in turn; one faint wisp while cooling
    let wisps: &[(f32, f32)] = if matches!(st, State::WindDown { .. }) { &[(46.0, 36.0)] } else { &[(39.0, 38.0), (46.0, 36.0), (53.0, 38.0)] };
    for (k, (x0, y0)) in wisps.iter().enumerate() {
        let (alpha, dy) = if matches!(st, State::WindDown { .. }) {
            (0.35, 0.0)
        } else {
            let ph = ((t / 2.4) + k as f64 / 3.0).fract() as f32;
            let a = if ph < 0.4 { ph / 0.4 } else { (1.0 - ph) / 0.6 };
            (a, 3.0 - 7.0 * ph)
        };
        let pts: Vec<Pos2> = (0..=12)
            .map(|i| {
                let u = i as f32 / 12.0;
                at(x0 + 2.2 * (u * PI * 1.6).sin(), y0 + dy - 11.0 * u)
            })
            .collect();
        p.add(PathShape::line(pts, PathStroke::new(2.5 * s, with_alpha(lk.soft, alpha * 0.9))));
    }

    // cup: straight top, rounded bottom
    let mut body = vec![at(31.0, 44.0), at(61.0, 44.0), at(61.0, 55.0)];
    body.extend(arc_points(at(47.0, 55.0), 14.0 * s, 0.0, PI / 2.0, 10));
    body.extend(arc_points(at(45.0, 55.0), 14.0 * s, PI / 2.0, PI, 10));
    p.add(Shape::convex_polygon(body, lk.body, Stroke::NONE));
    let mut handle = vec![at(60.0, 48.0)];
    handle.extend(arc_points(at(64.0, 54.0), 6.0 * s, -PI / 2.0, PI / 2.0, 14));
    handle.push(at(60.0, 60.0));
    p.add(PathShape::line(handle, PathStroke::new(4.0 * s, lk.body)));
    p.line_segment([at(27.0, 74.0), at(65.0, 74.0)], Stroke::new(3.0 * s, lk.body));
    p.circle_filled(at(27.0, 74.0), 1.5 * s, lk.body);
    p.circle_filled(at(65.0, 74.0), 1.5 * s, lk.body);

    if st == State::Claude {
        // Claude's spark rides the end of the ring
        let pos = c + Vec2::new(a1.cos(), a1.sin()) * rr;
        p.circle_filled(pos, 9.0 * s, tint::CLAUDE);
        let (lo, hi) = (1.6 * s, 6.0 * s);
        for (dx, dy) in [(lo, hi), (hi, lo)] {
            let pts = vec![pos + Vec2::new(0.0, -dy), pos + Vec2::new(dx, 0.0), pos + Vec2::new(0.0, dy), pos + Vec2::new(-dx, 0.0)];
            p.add(Shape::convex_polygon(pts, Color32::WHITE, Stroke::NONE));
        }
    }
}

/// Lines of text beside the scene: title, reason, detail.
fn hero_text(e: &Engine, st: State, now: i64) -> (String, String, String) {
    let since = e.awake_since.map(|s| format!("since {} · {}", time_of_day(s), span_text(now - s))).unwrap_or_default();
    match st {
        State::Off => (
            "Off".into(),
            "Mac sleeps as usual".into(),
            if e.settings.claude { "Watching Claude Code".into() } else { "Claude trigger is off".into() },
        ),
        State::Manual => {
            let screen = if e.settings.manual_screen { "screen on" } else { "screen can sleep" };
            ("Awake".into(), "Until you turn it off".into(), format!("{since} · {screen}"))
        }
        State::Claude => {
            let n = e.working(now).len();
            let screen = if e.settings.claude_screen { "screen on" } else { "screen can sleep" };
            (
                "Awake".into(),
                format!("Claude is working · {n} session{}", if n == 1 { "" } else { "s" }),
                format!("{} · {screen}", e.awake_since.map(|s| span_text(now - s)).unwrap_or_default()),
            )
        }
        State::WindDown { left } => (clock(left), "Claude finished · sleep in".into(), String::new()),
    }
}

/// The scene card: tinted ground, glow, scene on the left, text beside it.
/// Returns the rect of the "Keep awake" button while winding down.
fn hero(p: &Painter, r: Rect, e: &Engine, st: State, now: i64, t: f64, big: bool) -> Option<Rect> {
    let lk = look(st);
    let radius = if big { 16.0 } else { R_CARD };
    p.rect_filled(r, radius, lk.bg);
    let size = if big { 150.0 } else { 96.0 };
    let pad = if big { 28.0 } else { 14.0 };
    let sr = Rect::from_min_size(Pos2::new(r.left() + pad, r.center().y - size / 2.0), Vec2::splat(size));
    glow(p, r.shrink(1.0), sr.center(), size * 1.3, lk.ring);
    p.rect_stroke(r, radius, Stroke::new(1.0, with_alpha(lk.ring, 0.38)), StrokeKind::Inside);
    scene(p, sr, st, ring_frac(e, st, now), t);

    let (title, reason, detail) = hero_text(e, st, now);
    let x = sr.right() + if big { 28.0 } else { 14.0 };
    let w = r.right() - x - 12.0;
    let (ts, ls) = if big { (34.0, 15.0) } else { (22.0, 12.0) };
    let block = ts + 6.0 + ls + 6.0 + 16.0;
    let mut y = r.center().y - block / 2.0;
    text_fit(p, Pos2::new(x, y), Align2::LEFT_TOP, &title, bold(ts), TEXT, w);
    y += ts + 6.0;
    text_fit(p, Pos2::new(x, y), Align2::LEFT_TOP, &reason, font(ls), lk.soft, w);
    y += ls + 8.0;
    if matches!(st, State::WindDown { .. }) {
        let br = Rect::from_min_size(Pos2::new(x, y - 2.0), Vec2::new(96.0, 24.0));
        return Some(br);
    }
    if !big {
        text_fit(p, Pos2::new(x, y), Align2::LEFT_TOP, &detail, font(12.0), TEXT_DIM, w);
    }
    None
}

// ---------------------------------------------------------------- panel

pub fn tile(k: &mut KeepAwake, ui: &mut Ui, rect: Rect) {
    let st = k.state();
    let resp = quick_tile(ui, rect, ui.scope_id().with("q-awake"), Icon::Cup, tint::KEEP_AWAKE, "Awake", &short_state(st), true);
    if st.awake() {
        ui.painter().rect_stroke(rect, R_CARD, Stroke::new(1.5, with_alpha(look(st).ring, 0.75)), StrokeKind::Inside);
    }
    let tip = if k.engine.lock().unwrap().manual_since.is_some() { "Let the Mac sleep" } else { "Keep the Mac awake" };
    if resp.on_hover_text(tip).clicked() {
        k.toggle();
    }
}

fn shown_sessions(e: &Engine, now: i64) -> usize {
    if !e.settings.claude {
        return 0;
    }
    e.working(now).len().clamp(1, CARD_SESSIONS)
}

pub fn card_height(e: &Engine, now: i64) -> f32 {
    10.0 + 30.0 + HERO_H + 8.0 + ROW_H + shown_sessions(e, now) as f32 * SESSION_H + 4.0 + ROW_H + 6.0
}

pub fn card(k: &mut KeepAwake, ui: &mut Ui, rect: Rect, cx: &mut crate::core::tool::Cx) {
    let now = k.now();
    let t = ui.ctx().input(|i| i.time);
    let p = ui.painter().clone();
    let id = ui.scope_id().with("awake-card");
    glass_card(&p, rect, R_CARD, 0.0, false);
    let x0 = rect.left() + 12.0;
    let x1 = rect.right() - 12.0;
    let mut y = rect.top() + 10.0;

    let mut e = k.engine.lock().unwrap();
    let st = e.state(now);
    let manual = e.manual_since.is_some();

    // header: chip, name (opens the page), the switch
    tool_chip(&p, Rect::from_min_size(Pos2::new(x0, y), Vec2::splat(20.0)), Icon::Cup, tint::KEEP_AWAKE);
    let name = p.text(Pos2::new(x0 + 28.0, y + 10.0), Align2::LEFT_CENTER, "Keep Awake", bold(13.0), TEXT);
    if ui.interact(name, id.with("name"), Sense::click()).on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
        cx.open_page(ID, "");
    }
    let sw = Rect::from_min_size(Pos2::new(x1 - 38.0, y - 1.0), Vec2::new(38.0, 22.0));
    p.text(Pos2::new(sw.left() - 8.0, sw.center().y), Align2::RIGHT_CENTER, if manual { "On" } else { "Off" }, font(12.0), TEXT_DIM);
    if switch(ui, sw, id.with("manual"), manual, true).on_hover_text("Keep the Mac awake until you turn this off").clicked() {
        e.set_manual(!manual, now);
    }
    y += 30.0;

    let hr = Rect::from_min_max(Pos2::new(x0, y), Pos2::new(x1, y + HERO_H));
    if let Some(br) = hero(&p, hr, &e, st, now, t, false) {
        if pill_button(ui, br, "Keep awake", BTN_SECONDARY, true).clicked() {
            e.set_manual(true, now);
        }
    }
    y += HERO_H + 8.0;

    // Claude Code
    let row = |y: f32| Rect::from_min_max(Pos2::new(x0, y), Pos2::new(x1, y + ROW_H));
    let r = row(y);
    icon(&p, Icon::Sparkle, Rect::from_center_size(Pos2::new(x0 + 8.0, r.center().y), Vec2::splat(16.0)), TEXT_DIM);
    p.text(Pos2::new(x0 + 26.0, r.center().y), Align2::LEFT_CENTER, "While Claude Code works", font(13.0), TEXT);
    let claude = e.settings.claude;
    if switch(ui, Rect::from_min_size(Pos2::new(x1 - 30.0, r.center().y - 9.0), Vec2::new(30.0, 18.0)), id.with("claude"), claude, true).clicked() {
        e.change(now, |s| s.claude = !claude);
    }
    y += ROW_H;
    if e.settings.claude {
        let working = e.working(now);
        if working.is_empty() {
            p.text(Pos2::new(x0 + 26.0, y + SESSION_H / 2.0), Align2::LEFT_CENTER, "No session working right now", font(12.0), TEXT_FAINT);
            y += SESSION_H;
        } else {
            for s in working.iter().take(CARD_SESSIONS) {
                let cy = y + SESSION_H / 2.0;
                p.circle_filled(Pos2::new(x0 + 29.0, cy), 3.0, GREEN);
                text_fit(&p, Pos2::new(x0 + 40.0, cy), Align2::LEFT_CENTER, &s.project, font(12.0), TEXT_SOFT, x1 - x0 - 170.0);
                p.text(Pos2::new(x1, cy), Align2::RIGHT_CENTER, format!("working · {}", ago(now - s.last_write)), font(12.0), TEXT_FAINT);
                y += SESSION_H;
            }
        }
    }
    y += 4.0;
    hairline(&p, Pos2::new(x0, y), Pos2::new(x1, y));

    // screen: the setting that applies to what's keeping the Mac up
    let r = row(y);
    icon(&p, Icon::Monitor, Rect::from_center_size(Pos2::new(x0 + 8.0, r.center().y), Vec2::splat(16.0)), TEXT_DIM);
    let for_claude = matches!(st, State::Claude | State::WindDown { .. });
    let label = if for_claude { "Keep screen on while Claude works" } else { "Keep screen on" };
    p.text(Pos2::new(x0 + 26.0, r.center().y), Align2::LEFT_CENTER, label, font(13.0), TEXT);
    let on = if for_claude { e.settings.claude_screen } else { e.settings.manual_screen };
    if switch(ui, Rect::from_min_size(Pos2::new(x1 - 30.0, r.center().y - 9.0), Vec2::new(30.0, 18.0)), id.with("screen"), on, true).clicked() {
        e.change(now, |s| if for_claude { s.claude_screen = !on } else { s.manual_screen = !on });
    }
}

// ---------------------------------------------------------------- page

pub fn page(k: &mut KeepAwake, ui: &mut Ui, r: Rect) {
    let now = k.now();
    let t = ui.ctx().input(|i| i.time);
    let p = ui.painter().clone();
    let id = ui.scope_id().with("awake-page");
    let mut e = k.engine.lock().unwrap();
    let st = e.state(now);
    let x0 = r.left() + 24.0;
    let x1 = r.right() - 24.0;
    let mut y = r.top() + 20.0;

    // hero with the big switch
    let hr = Rect::from_min_max(Pos2::new(x0, y), Pos2::new(x1, y + 194.0));
    if let Some(br) = hero(&p, hr, &e, st, now, t, true) {
        let br = Rect::from_min_size(br.min + Vec2::new(0.0, 4.0), Vec2::new(110.0, 28.0));
        if pill_button(ui, br, "Keep awake", BTN_SECONDARY, true).clicked() {
            e.set_manual(true, now);
        }
    } else {
        // status dots under the reason line
        let x = hr.left() + 28.0 + 150.0 + 28.0;
        let mut cx = x;
        let cy = hr.center().y + (34.0 + 6.0 + 15.0 + 6.0 + 16.0) / 2.0 - 8.0;
        let held = e.held();
        let dot = |label: &str, on: bool, cx: &mut f32| {
            p.circle_filled(Pos2::new(*cx + 3.5, cy), 3.5, if on { GREEN } else { TEXT_FAINT });
            let tr = p.text(Pos2::new(*cx + 13.0, cy), Align2::LEFT_CENTER, label, font(12.0), TEXT_DIM);
            *cx = tr.right() + 18.0;
        };
        dot(if held.system { "Mac won't sleep" } else { "Mac can sleep" }, held.system, &mut cx);
        dot(if held.display { "Screen stays on" } else { "Screen can sleep" }, held.display, &mut cx);
        if let Some(s) = e.awake_since {
            p.text(Pos2::new(cx, cy), Align2::LEFT_CENTER, format!("since {} · {}", time_of_day(s), span_text(now - s)), font(12.0), TEXT_DIM);
        }
    }
    let manual = e.manual_since.is_some();
    let sw = Rect::from_min_size(Pos2::new(hr.right() - 28.0 - 64.0, hr.center().y - 26.0), Vec2::new(64.0, 36.0));
    if switch(ui, sw, id.with("manual"), manual, true).on_hover_text("Keep the Mac awake until you turn this off").clicked() {
        e.set_manual(!manual, now);
    }
    p.text(Pos2::new(sw.center().x, sw.bottom() + 10.0), Align2::CENTER_TOP, "By hand", font(11.0), TEXT_DIM);
    y = hr.bottom() + 18.0;

    // today
    let spans = e.today(now);
    let total: i64 = spans.iter().map(|s| s.end - s.start).sum();
    let claude: i64 = spans.iter().filter(|s| s.claude).map(|s| s.end - s.start).sum();
    p.text(Pos2::new(x0, y), Align2::LEFT_TOP, "Today", bold(15.0), TEXT);
    let summary = if total == 0 { "Not kept awake yet".to_string() } else { format!("Awake {} · {} of it for Claude", span_text(total), span_text(claude)) };
    p.text(Pos2::new(x1, y + 2.0), Align2::RIGHT_TOP, summary, font(12.0), TEXT_DIM);
    y += 26.0;
    let strip = Rect::from_min_max(Pos2::new(x0, y), Pos2::new(x1, y + 28.0));
    today_strip(&p, strip, &spans, now);
    y = strip.bottom() + 6.0;
    for (i, l) in ["00:00", "06:00", "12:00", "18:00", "24:00"].iter().enumerate() {
        let x = strip.left() + strip.width() * i as f32 / 4.0;
        let align = match i {
            0 => Align2::LEFT_TOP,
            4 => Align2::RIGHT_TOP,
            _ => Align2::CENTER_TOP,
        };
        p.text(Pos2::new(x, y), align, *l, font(11.0), TEXT_FAINT);
    }
    y += 20.0;
    let mut lx = x0;
    for (label, c) in [("By hand", tint::KEEP_AWAKE), ("Claude working", tint::CLAUDE)] {
        p.rect_filled(Rect::from_min_size(Pos2::new(lx, y + 1.0), Vec2::splat(10.0)), 3.0, c);
        let tr = p.text(Pos2::new(lx + 16.0, y + 6.0), Align2::LEFT_CENTER, label, font(12.0), TEXT_DIM);
        lx = tr.right() + 16.0;
    }
    y += 30.0;

    // two columns: Claude Code, Screen
    let gap = 20.0;
    let lw = (x1 - x0 - gap) * 0.55;
    let left = Rect::from_min_max(Pos2::new(x0, y), Pos2::new(x0 + lw, r.bottom() - 20.0));
    let right = Rect::from_min_max(Pos2::new(left.right() + gap, y), Pos2::new(x1, r.bottom() - 20.0));
    claude_section(ui, &p, left, &mut e, now, id);
    screen_section(ui, &p, right, &mut e, now, id);
}

fn today_strip(p: &Painter, r: Rect, spans: &[Span], now: i64) {
    p.rect_filled(r, 8.0, CARD);
    p.rect_stroke(r, 8.0, Stroke::new(1.0, white(HAIRLINE)), StrokeKind::Inside);
    let day = super::engine::today_start(now);
    let x_of = |t: i64| r.left() + r.width() * ((t - day) as f32 / 86_400.0).clamp(0.0, 1.0);
    for s in spans {
        let (a, b) = (x_of(s.start), x_of(s.end).max(x_of(s.start) + 3.0));
        let c = if s.claude { tint::CLAUDE } else { tint::KEEP_AWAKE };
        p.rect_filled(Rect::from_min_max(Pos2::new(a, r.top() + 4.0), Pos2::new(b, r.bottom() - 4.0)), 4.0, c);
    }
    let nx = x_of(now);
    p.line_segment([Pos2::new(nx, r.top()), Pos2::new(nx, r.bottom())], Stroke::new(2.0, TEXT));
}

fn section_card(p: &Painter, r: Rect) {
    p.rect_filled(r, R_CARD, CARD);
    p.rect_stroke(r, R_CARD, Stroke::new(1.0, white(HAIRLINE)), StrokeKind::Inside);
}

/// A switch row with a hint under the label. Returns true when clicked.
fn switch_row(ui: &mut Ui, p: &Painter, r: Rect, id: egui::Id, label: &str, hint: &str, on: bool) -> bool {
    p.text(Pos2::new(r.left() + 12.0, r.center().y - 8.0), Align2::LEFT_CENTER, label, font(13.0), TEXT);
    text_fit(p, Pos2::new(r.left() + 12.0, r.center().y + 9.0), Align2::LEFT_CENTER, hint, font(11.0), TEXT_FAINT, r.width() - 70.0);
    switch(ui, Rect::from_min_size(Pos2::new(r.right() - 42.0, r.center().y - 9.0), Vec2::new(30.0, 18.0)), id, on, true).clicked()
}

fn claude_section(ui: &mut Ui, p: &Painter, r: Rect, e: &mut Engine, now: i64, id: egui::Id) {
    p.text(r.min, Align2::LEFT_TOP, "Claude Code", bold(15.0), TEXT);
    let shown: Vec<_> = e.sessions.iter().take(4).cloned().collect();
    let rows = shown.len().max(1) as f32;
    let card = Rect::from_min_size(r.min + Vec2::new(0.0, 26.0), Vec2::new(r.width(), 44.0 + 36.0 + 28.0 + rows * 26.0 + 8.0));
    section_card(p, card);
    let mut y = card.top() + 2.0;
    let claude = e.settings.claude;
    let row = Rect::from_min_max(Pos2::new(card.left(), y), Pos2::new(card.right(), y + 44.0));
    let hint = format!("Working = a session wrote to its log in the last {} min", WORKING_SECS / 60);
    if switch_row(ui, p, row, id.with("claude"), "Stay awake while Claude works", &hint, claude) {
        e.change(now, |s| s.claude = !claude);
    }
    y += 44.0;
    hairline(p, Pos2::new(card.left() + 12.0, y), Pos2::new(card.right(), y));
    p.text(Pos2::new(card.left() + 12.0, y + 18.0), Align2::LEFT_CENTER, "Wait before sleeping", font(13.0), TEXT);
    let labels: Vec<String> = GRACE_CHOICES.iter().map(|m| format!("{m} min")).collect();
    let refs: Vec<&str> = labels.iter().map(|s| s.as_str()).collect();
    let sel = GRACE_CHOICES.iter().position(|m| *m == e.settings.grace_min).unwrap_or(1);
    let seg = Rect::from_min_size(Pos2::new(card.right() - 12.0 - 168.0, y + 6.0), Vec2::new(168.0, 24.0));
    if let Some(i) = segmented(ui, seg, id.with("grace"), &refs, sel) {
        e.change(now, |s| s.grace_min = GRACE_CHOICES[i]);
    }
    y += 36.0;
    hairline(p, Pos2::new(card.left() + 12.0, y), Pos2::new(card.right(), y));
    section_header(p, Pos2::new(card.left() + 12.0, y + 10.0), Align2::LEFT_TOP, "SESSIONS");
    y += 28.0;
    if shown.is_empty() {
        p.text(Pos2::new(card.left() + 12.0, y + 13.0), Align2::LEFT_CENTER, "No Claude Code session in the last day", font(12.0), TEXT_FAINT);
    }
    for s in &shown {
        let cy = y + 13.0;
        let working = now - s.last_write <= WORKING_SECS;
        p.circle_filled(Pos2::new(card.left() + 15.0, cy), 3.0, if working { GREEN } else { Color32::from_rgb(0x63, 0x63, 0x66) });
        text_fit(p, Pos2::new(card.left() + 26.0, cy), Align2::LEFT_CENTER, &s.project, mono(11.5), TEXT_SOFT, card.width() - 180.0);
        let when = if working { format!("working · {}", ago(now - s.last_write)) } else { format!("idle · {}", ago(now - s.last_write).trim_end_matches(" ago")) };
        p.text(Pos2::new(card.right() - 12.0, cy), Align2::RIGHT_CENTER, when, font(12.0), TEXT_FAINT);
        y += 26.0;
    }
}

fn screen_section(ui: &mut Ui, p: &Painter, r: Rect, e: &mut Engine, now: i64, id: egui::Id) {
    p.text(r.min, Align2::LEFT_TOP, "Screen", bold(15.0), TEXT);
    let card = Rect::from_min_size(r.min + Vec2::new(0.0, 26.0), Vec2::new(r.width(), 2.0 * 44.0 + 4.0));
    section_card(p, card);
    let y = card.top() + 2.0;
    let (m, c) = (e.settings.manual_screen, e.settings.claude_screen);
    let row1 = Rect::from_min_max(Pos2::new(card.left(), y), Pos2::new(card.right(), y + 44.0));
    if switch_row(ui, p, row1, id.with("mscreen"), "Keep screen on when on by hand", "Like a presentation or a long read", m) {
        e.change(now, |s| s.manual_screen = !m);
    }
    hairline(p, Pos2::new(card.left() + 12.0, y + 44.0), Pos2::new(card.right(), y + 44.0));
    let row2 = Rect::from_min_max(Pos2::new(card.left(), y + 44.0), Pos2::new(card.right(), y + 88.0));
    if switch_row(ui, p, row2, id.with("cscreen"), "Keep screen on while Claude works", "Off: the Mac works, the screen rests", c) {
        e.change(now, |s| s.claude_screen = !c);
    }
    paragraph(
        p,
        Pos2::new(card.left() + 2.0, card.bottom() + 10.0),
        "Closing the lid still puts the Mac to sleep unless it's on power with an external display. That's a macOS rule.",
        font(11.0),
        TEXT_FAINT,
        card.width() - 4.0,
    );
}
