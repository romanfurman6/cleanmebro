//! First-run setup: pick tools, allow what they need from macOS (live
//! status), open at login, then hand over to the menu bar panel. Also the
//! permission list in Settings › General.

use super::shell::App;
use crate::kit::glass::*;
use crate::kit::perms::{self, Perm, Status};
use egui::{Align2, Color32, CornerRadius, FontId, Pos2, Rect, Sense, Stroke, StrokeKind, Ui, Vec2};
use std::collections::HashMap;

pub const SETUP_W: f32 = 820.0;
pub const SETUP_H: f32 = 600.0;
const STEPS: usize = 4;
const TOP_H: f32 = 44.0;
const BOTTOM_H: f32 = 64.0;
/// Tools that start off on a fresh install (the rest start on).
pub const OFF_AT_FIRST: [&str; 2] = ["claude", "fan"];

/// The setup window's state while it is open.
pub struct Setup {
    pub step: usize,
    /// When the current step appeared (its animations count from here).
    since: f64,
    /// Bring the window to the front on its next frame.
    pub focus: bool,
}

impl Setup {
    pub fn new(step: usize) -> Self {
        Setup { step, since: f64::NEG_INFINITY, focus: true }
    }
}

/// Marker file: setup was finished or closed once.
pub fn done_marker() -> std::path::PathBuf {
    crate::kit::fs::data_path("setup-done")
}

pub fn mark_done() {
    let m = done_marker();
    if let Some(d) = m.parent() {
        let _ = std::fs::create_dir_all(d);
    }
    let _ = std::fs::write(m, b"");
}

/// Live permission state, polled about once a second while shown.
#[derive(Default)]
pub struct PermWatch {
    status: HashMap<Perm, Status>,
    polled: f64,
    /// When a permission's button was pressed (spinner until it flips).
    asked: HashMap<Perm, f64>,
    /// When a permission turned Allowed while we watched (check pops).
    allowed_at: HashMap<Perm, f64>,
    /// Sample statuses for screenshots.
    pub demo: Option<HashMap<Perm, Status>>,
}

/// One row: a permission and the enabled tools that want it.
pub struct Row {
    pub perm: Perm,
    pub optional: bool,
    /// (tool index) of the enabled tools that need it.
    pub tools: Vec<usize>,
}

/// What the enabled tools need, required rows first, in a stable order.
pub fn rows(app: &App) -> Vec<Row> {
    let mut out: Vec<Row> = Vec::new();
    for i in 0..app.tools.len() {
        if !app.tool_on(i) {
            continue;
        }
        for n in app.tools[i].needs() {
            match out.iter_mut().find(|r| r.perm == n.perm) {
                Some(r) => {
                    r.optional &= n.optional;
                    r.tools.push(i);
                }
                None => out.push(Row { perm: n.perm, optional: n.optional, tools: vec![i] }),
            }
        }
    }
    let rank = |p: Perm| Perm::SYSTEM.iter().position(|s| *s == p).unwrap_or(Perm::SYSTEM.len());
    out.sort_by_key(|r| (r.optional, rank(r.perm)));
    out
}

impl PermWatch {
    /// Refresh statuses for `rows` (at most once a second).
    pub fn poll(&mut self, app_tools: &[Box<dyn super::tool::Tool>], rows: &[Row], now: f64) {
        if now - self.polled < 1.0 && !self.status.is_empty() {
            return;
        }
        self.polled = now;
        for r in rows {
            let s = if let Some(d) = &self.demo {
                d.get(&r.perm).copied().unwrap_or(Status::NotAsked)
            } else {
                match r.perm {
                    Perm::Own { id, .. } => r.tools.first().map(|&i| app_tools[i].own_status(id)).unwrap_or(Status::NotAsked),
                    p => p.status(),
                }
            };
            let before = self.status.insert(r.perm, s);
            if s == Status::Allowed && before.is_some_and(|b| b != Status::Allowed) {
                self.allowed_at.insert(r.perm, now);
            }
            if s != Status::NotAsked {
                // a refusal ends the wait too
                if s == Status::Allowed || self.asked.get(&r.perm).is_some_and(|t| now - t > 3.0) {
                    self.asked.remove(&r.perm);
                }
            }
        }
    }

    pub fn status(&self, p: Perm) -> Status {
        self.status.get(&p).copied().unwrap_or(Status::NotAsked)
    }

    fn waiting(&self, p: Perm, now: f64) -> bool {
        self.asked.get(&p).is_some_and(|t| now - t < 60.0) && self.status(p) != Status::Allowed
    }

    /// Something is animating or waiting: keep frames coming.
    fn busy(&self, now: f64) -> bool {
        !self.asked.is_empty() || self.allowed_at.values().any(|t| now - t < 0.6)
    }
}

/// Screen Recording was allowed after launch: capture works after a restart.
pub fn needs_restart(app: &App) -> bool {
    app.perm_watch.demo.is_none() && app.perm_watch.status(Perm::Screen) == Status::Allowed && !perms::screen_at_launch() && rows(app).iter().any(|r| r.perm == Perm::Screen)
}

// ---------------------------------------------------------------- drawing

fn ease(x: f32) -> f32 {
    let x = x.clamp(0.0, 1.0);
    1.0 - (1.0 - x).powi(3)
}

/// 0 → 1 with a small overshoot (a pop).
fn pop(x: f32) -> f32 {
    let x = x.clamp(0.0, 1.0);
    let c = 1.7;
    1.0 + (c + 1.0) * (x - 1.0).powi(3) + c * (x - 1.0).powi(2)
}

/// The whole setup screen in `full` (its own window, or the root in screenshots).
/// Returns true when the person finished or closed it.
pub fn setup_ui(app: &mut App, ui: &mut Ui, full: Rect) -> Finish {
    let ctx = ui.ctx().clone();
    let now = ctx.input(|i| i.time);
    let p = ui.painter().clone();
    let Some(step) = app.setup.as_ref().map(|s| s.step) else { return Finish::No };
    if let Some(s) = app.setup.as_mut() {
        if s.since == f64::NEG_INFINITY {
            s.since = now;
        }
    }
    let t = (now - app.setup.as_ref().map(|s| s.since).unwrap_or(now)) as f32;
    // drag the window by its top strip
    let drag = Rect::from_min_size(full.min + Vec2::new(80.0, 0.0), Vec2::new(full.width() - 80.0, TOP_H));
    if ui.interact(drag, ui.scope_id().with("setup-drag"), Sense::click_and_drag()).drag_started() {
        ctx.send_viewport_cmd(egui::ViewportCommand::StartDrag);
    }
    // step dots
    let dw = |i: usize| if i == step { 22.0 } else { 6.0 };
    let total: f32 = (0..STEPS).map(dw).sum::<f32>() + 6.0 * (STEPS as f32 - 1.0);
    let mut x = full.center().x - total / 2.0;
    for i in 0..STEPS {
        let w = egui::lerp(6.0..=dw(i), ease(t * 3.0).max(if i == step { 0.0 } else { 1.0 }));
        let r = Rect::from_min_size(Pos2::new(x, full.top() + TOP_H / 2.0 - 3.0), Vec2::new(w, 6.0));
        p.rect_filled(r, 3.0, if i <= step { BLUE } else { white(46) });
        x += w + 6.0;
    }
    let body = Rect::from_min_max(Pos2::new(full.left() + 40.0, full.top() + TOP_H), Pos2::new(full.right() - 40.0, full.bottom() - BOTTOM_H));
    // the step slides in from the right
    let slide = (1.0 - ease(t * 2.2)) * 18.0;
    let body = body.translate(Vec2::new(slide, 0.0));
    let mut next = false;
    match step {
        0 => next = welcome(app, ui, body, t),
        1 => tools(app, ui, body, t),
        2 => {
            let r = Rect::from_min_max(body.min, body.max);
            permissions_step(app, ui, r, t, now)
        }
        _ => ready(app, ui, body, t),
    }
    // bottom bar
    let bar = Rect::from_min_max(Pos2::new(full.left() + 24.0, full.bottom() - BOTTOM_H), Pos2::new(full.right() - 24.0, full.bottom()));
    let mut finish = Finish::No;
    if step > 0 {
        let r = Rect::from_min_size(Pos2::new(bar.left(), bar.center().y - 16.0), Vec2::new(70.0, 32.0));
        if ghost_button(ui, r, "Back").clicked() {
            go(app, step - 1);
        }
    }
    let label = match step {
        0 => None,
        1 | 2 => Some("Continue"),
        _ if needs_restart(app) => Some("Restart and open"),
        _ => Some("Open panel"),
    };
    if let Some(label) = label {
        let w = button_width(&p, label, BTN_PRIMARY).max(110.0);
        let r = Rect::from_min_size(Pos2::new(bar.right() - w, bar.center().y - 16.0), Vec2::new(w, 32.0));
        if pill_button(ui, r, label, BTN_PRIMARY, true).clicked() {
            if step == STEPS - 1 {
                finish = if needs_restart(app) { Finish::Restart } else { Finish::Panel };
            } else {
                next = true;
            }
        }
        if step == 2 && !all_required(app) {
            let r2 = Rect::from_min_size(Pos2::new(r.left() - 80.0, r.top()), Vec2::new(70.0, 32.0));
            if ghost_button(ui, r2, "Later").clicked() {
                next = true;
            }
        }
    }
    if next {
        go(app, step + 1);
    }
    let animating = t < 2.5 || app.perm_watch.busy(now);
    if animating {
        ctx.request_repaint();
    } else {
        ctx.request_repaint_after(std::time::Duration::from_millis(1000));
    }
    finish
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Finish {
    No,
    Panel,
    Restart,
}

fn go(app: &mut App, step: usize) {
    if let Some(s) = app.setup.as_mut() {
        s.step = step.min(STEPS - 1);
        s.since = f64::NEG_INFINITY;
    }
}

fn all_required(app: &App) -> bool {
    rows(app).iter().filter(|r| !r.optional).all(|r| app.perm_watch.status(r.perm) == Status::Allowed)
}

fn ghost_button(ui: &mut Ui, r: Rect, label: &str) -> egui::Response {
    let resp = ui.interact(r, ui.scope_id().with(("ghost", label)), Sense::click());
    let col = if resp.hovered() { TEXT } else { TEXT_DIM };
    ui.painter().text(r.center(), Align2::CENTER_CENTER, label, font(13.0), col);
    if resp.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    resp
}

/// The app's mark: the icon's dark squircle with the blue gauge arc.
fn app_mark(p: &egui::Painter, c: Pos2, size: f32, sweep: f32) {
    let r = Rect::from_center_size(c, Vec2::splat(size));
    p.rect_filled(r, size * 0.225, Color32::from_rgb(0x22, 0x22, 0x26));
    p.rect_stroke(r, size * 0.225, Stroke::new(1.0, white(36)), StrokeKind::Inside);
    let rad = size * 0.3;
    let start = -std::f32::consts::FRAC_PI_2 - 0.9;
    let track = arc_points(c, rad, start, start + 300f32.to_radians(), 48);
    p.add(egui::Shape::line(track, Stroke::new(size * 0.075, white(20))));
    let a1 = start + 300f32.to_radians() * sweep;
    if sweep > 0.01 {
        p.add(egui::Shape::line(arc_points(c, rad, start, a1, 48), Stroke::new(size * 0.075, BLUE)));
    }
    p.circle_filled(c, size * 0.06, white(200));
}

fn welcome(app: &mut App, ui: &mut Ui, body: Rect, t: f32) -> bool {
    let p = ui.painter().clone();
    let c = Pos2::new(body.center().x, body.top() + 150.0);
    // the ring draws itself, then the tools land on it one by one
    let ring = 86.0;
    p.circle_stroke(c, ring, Stroke::new(2.0, white(14)));
    let start = -std::f32::consts::FRAC_PI_2;
    let k = ease(t / 1.4);
    if k > 0.0 {
        p.add(egui::Shape::line(arc_points(c, ring, start, start + std::f32::consts::TAU * k, 96), Stroke::new(2.0, BLUE)));
    }
    let n = app.tools.len().max(1);
    for (i, tool) in app.tools.iter().enumerate() {
        let a = start + i as f32 / n as f32 * std::f32::consts::TAU;
        let at = c + Vec2::new(a.cos(), a.sin()) * ring;
        let s = pop((t - 0.2 - 0.12 * i as f32) / 0.45);
        if s <= 0.0 {
            continue;
        }
        p.circle_filled(at, 15.0 * s, Color32::from_rgb(0x1E, 0x1E, 0x20));
        p.circle_stroke(at, 15.0 * s, Stroke::new(1.0, white(36)));
        icon(&p, tool.badge().icon, Rect::from_center_size(at, Vec2::splat(14.0 * s)), TEXT_SOFT);
    }
    let bob = if t > 1.8 { ((t - 1.8) * 1.6).sin() * 3.0 } else { 0.0 };
    app_mark(&p, c + Vec2::new(0.0, bob), 96.0, ease((t - 0.3) / 1.2));
    p.text(Pos2::new(c.x, c.y + 118.0), Align2::CENTER_TOP, "CleanMeBro", bold(28.0), TEXT);
    p.text(Pos2::new(c.x, c.y + 156.0), Align2::CENTER_TOP, format!("{} tools. One menu bar.", app.tools.len()), font(14.0), TEXT_FAINT);
    let r = Rect::from_center_size(Pos2::new(c.x, c.y + 214.0), Vec2::new(128.0, 40.0));
    // breathing glow once everything has landed
    if t > 1.8 {
        let g = ((t - 1.8) * 2.4).sin() * 0.5 + 0.5;
        p.rect_filled(r.expand(6.0 * g), 14.0, with_alpha(BLUE, 0.18 * g));
    }
    pill_button(ui, r, "Set up", BTN_PRIMARY, true).clicked()
}

/// Section order for the tools grid.
fn sections(app: &App) -> Vec<&'static str> {
    let mut out: Vec<&'static str> = Vec::new();
    for want in ["Clean", "Create", "Monitor", "Utilities"] {
        if app.tools.iter().any(|t| t.section() == want) {
            out.push(want);
        }
    }
    for t in &app.tools {
        if !out.contains(&t.section()) {
            out.push(t.section());
        }
    }
    out
}

fn tools(app: &mut App, ui: &mut Ui, body: Rect, t: f32) {
    let p = ui.painter().clone();
    let on = (0..app.tools.len()).filter(|&i| app.tool_on(i)).count();
    let head = body.top() + 6.0;
    let g = p.text(Pos2::new(body.left(), head), Align2::LEFT_TOP, "Tools", bold(22.0), TEXT);
    p.text(Pos2::new(g.right() + 10.0, g.bottom() - 4.0), Align2::LEFT_BOTTOM, format!("{on}/{}", app.tools.len()), mono(13.0), TEXT_FAINT);
    p.text(Pos2::new(body.right(), g.bottom() - 4.0), Align2::RIGHT_BOTTOM, "Change any time in Settings", font(11.0), TEXT_MUTED);
    let secs = sections(app);
    let cols = secs.len().max(1) as f32;
    let gap = 12.0;
    let cw = (body.width() - gap * (cols - 1.0)) / cols;
    let top = g.bottom() + 20.0;
    let mut flip = None;
    let mut k = 0;
    for (ci, sec) in secs.iter().enumerate() {
        let x = body.left() + ci as f32 * (cw + gap);
        p.text(Pos2::new(x + 2.0, top), Align2::LEFT_TOP, sec.to_uppercase(), font(11.0), TEXT_FAINT);
        let mut y = top + 20.0;
        for i in 0..app.tools.len() {
            if app.tools[i].section() != *sec {
                continue;
            }
            let a = ease((t - 0.04 * k as f32) / 0.4);
            k += 1;
            let r = Rect::from_min_size(Pos2::new(x, y + (1.0 - a) * 8.0), Vec2::new(cw, 68.0));
            let id = ui.scope_id().with(("setup-tool", app.tools[i].id()));
            let resp = ui.interact(r, id, Sense::click());
            let o = app.tool_on(i);
            let s = ui.ctx().animate_bool_with_time(id.with("on"), o, 0.18);
            let hov = ui.ctx().animate_bool(id.with("h"), resp.hovered());
            let fill = lerp_color(SIDEBAR, Color32::from_rgb(0x14, 0x2A, 0x45), s);
            p.rect_filled(r, 10.0, lerp_color(fill, white(30), hov * 0.15).gamma_multiply(a));
            p.rect_stroke(r, 10.0, Stroke::new(1.0, lerp_color(white(12), with_alpha(BLUE, 0.7), s).gamma_multiply(a)), StrokeKind::Inside);
            let ib = Rect::from_min_size(Pos2::new(r.left() + 12.0, r.center().y - 17.0), Vec2::splat(34.0));
            p.rect_filled(ib, 9.0, lerp_color(white(16), BLUE, s));
            let b = app.tools[i].badge();
            icon(&p, b.icon, ib.shrink(8.0), lerp_color(TEXT_MUTED, Color32::WHITE, s));
            let tx = ib.right() + 10.0;
            text_fit(&p, Pos2::new(tx, r.top() + 15.0), Align2::LEFT_TOP, app.tools[i].name(), bold(13.0), lerp_color(TEXT_MUTED, TEXT, s), r.right() - tx - 24.0);
            // what it will ask for
            let mut gx = tx;
            for n in app.tools[i].needs() {
                icon(&p, n.perm.icon(), Rect::from_min_size(Pos2::new(gx, r.top() + 37.0), Vec2::splat(12.0)), TEXT_MUTED);
                gx += 16.0;
            }
            if s > 0.01 {
                let c = Pos2::new(r.right() - 14.0, r.top() + 14.0);
                let z = pop(s);
                p.circle_filled(c, 8.0 * z, BLUE);
                check(&p, c, 7.0 * z, Color32::WHITE);
            }
            if resp.clicked() {
                flip = Some((i, !o));
            }
            if resp.hovered() {
                ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
            }
            y += 76.0;
        }
    }
    if let Some((i, o)) = flip {
        app.set_tool_on(i, o);
    }
}

fn check(p: &egui::Painter, c: Pos2, s: f32, col: Color32) {
    let pts = vec![c + Vec2::new(-0.45, 0.0) * s, c + Vec2::new(-0.12, 0.33) * s, c + Vec2::new(0.45, -0.32) * s];
    p.add(egui::Shape::line(pts, Stroke::new((s * 0.22).max(1.2), col)));
}

fn permissions_step(app: &mut App, ui: &mut Ui, body: Rect, t: f32, now: f64) {
    let p = ui.painter().clone();
    let list = rows(app);
    {
        let (tools, watch) = (&app.tools, &mut app.perm_watch);
        watch.poll(tools, &list, now);
    }
    let req: Vec<&Row> = list.iter().filter(|r| !r.optional).collect();
    let done = req.iter().filter(|r| app.perm_watch.status(r.perm) == Status::Allowed).count();
    // progress ring
    let c = Pos2::new(body.left() + 22.0, body.top() + 22.0);
    p.circle_stroke(c, 18.0, Stroke::new(4.0, white(20)));
    let frac = if req.is_empty() { 1.0 } else { done as f32 / req.len() as f32 };
    let shown = ui.ctx().animate_value_with_time(ui.scope_id().with("perm-ring"), frac, 0.6);
    let full = done == req.len();
    if shown > 0.005 {
        let a0 = -std::f32::consts::FRAC_PI_2;
        p.add(egui::Shape::line(arc_points(c, 18.0, a0, a0 + std::f32::consts::TAU * shown, 48), Stroke::new(4.0, if full { GREEN } else { BLUE })));
    }
    p.text(c, Align2::CENTER_CENTER, format!("{done}/{}", req.len()), mono(11.0), TEXT);
    p.text(Pos2::new(c.x + 36.0, c.y), Align2::LEFT_CENTER, "Permissions", bold(22.0), TEXT);
    p.text(Pos2::new(body.right(), c.y), Align2::RIGHT_CENTER, "Live from System Settings", font(11.0), TEXT_MUTED);
    let area = Rect::from_min_max(Pos2::new(body.left(), body.top() + 56.0), body.max);
    perm_list(app, ui, area, &list, t, now);
}

/// The permission rows (setup and Settings › General). Returns the height used.
pub fn perm_list(app: &mut App, ui: &mut Ui, area: Rect, list: &[Row], t: f32, now: f64) -> f32 {
    let p = ui.painter().clone();
    let mut y = area.top();
    let mut opt_head = false;
    let row_h = 46.0;
    let mut ask: Option<Perm> = None;
    for (k, r) in list.iter().enumerate() {
        if r.optional && !opt_head {
            opt_head = true;
            y += 6.0;
            p.text(Pos2::new(area.left() + 2.0, y), Align2::LEFT_TOP, "OPTIONAL", font(11.0), TEXT_FAINT);
            y += 20.0;
        }
        let a = ease((t - 0.05 * k as f32) / 0.4);
        let rr = Rect::from_min_size(Pos2::new(area.left(), y - (1.0 - a) * 10.0), Vec2::new(area.width(), row_h));
        let st = app.perm_watch.status(r.perm);
        let waiting = app.perm_watch.waiting(r.perm, now);
        let ok = st == Status::Allowed;
        p.rect_filled(rr, 10.0, SIDEBAR.gamma_multiply(a));
        let ib = Rect::from_min_size(Pos2::new(rr.left() + 12.0, rr.center().y - 15.0), Vec2::splat(30.0));
        let tint = match st {
            Status::Allowed => GREEN,
            Status::Denied => Color32::from_rgb(0xFF, 0x9F, 0x0A),
            Status::NotAsked => TEXT_SOFT,
        };
        p.rect_filled(ib, 8.0, if st == Status::NotAsked { white(16) } else { with_alpha(tint, 0.16) });
        icon(&p, r.perm.icon(), ib.shrink(7.0), tint);
        p.text(Pos2::new(ib.right() + 12.0, rr.center().y), Align2::LEFT_CENTER, r.perm.name(), bold(13.0), TEXT.gamma_multiply(a));
        // the tools that want it
        let mut gx = ib.right() + 190.0;
        for &i in &r.tools {
            icon(&p, app.tools[i].badge().icon, Rect::from_min_size(Pos2::new(gx, rr.center().y - 7.0), Vec2::splat(14.0)), TEXT_MUTED);
            gx += 22.0;
        }
        // status on the right
        let right = rr.right() - 12.0;
        if ok {
            let since = app.perm_watch.allowed_at.get(&r.perm).map(|t0| (now - t0) as f32).unwrap_or(1.0);
            let z = pop(since / 0.45);
            let c = Pos2::new(right - 13.0, rr.center().y);
            p.circle_filled(c, 13.0 * z, with_alpha(GREEN, 0.18));
            check(&p, c, 12.0 * z, GREEN);
        } else if waiting {
            let c = Pos2::new(right - 8.0, rr.center().y);
            spinner(&p, c, 6.0, now, BLUE);
            let hint = if r.perm == Perm::FullDisk { "Drag CleanMeBro into the list" } else { "System Settings" };
            p.text(Pos2::new(c.x - 16.0, c.y), Align2::RIGHT_CENTER, hint, font(12.0), TEXT_FAINT);
        } else {
            let (label, kind) = if st == Status::Denied { ("Open Settings", BTN_SECONDARY) } else { (r.perm.ask_label(), BTN_SECONDARY) };
            let w = button_width(&p, label, kind);
            let b = Rect::from_min_size(Pos2::new(right - w, rr.center().y - 13.0), Vec2::new(w, 26.0));
            if pill_button(ui, b, label, kind, true).clicked() {
                ask = Some(r.perm);
            }
        }
        y += row_h + 6.0;
    }
    if let Some(perm) = ask {
        app.perm_watch.asked.insert(perm, now);
        app.perm_watch.polled = 0.0;
        if app.perm_watch.demo.is_none() {
            match perm {
                Perm::Own { id, .. } => {
                    if let Some(r) = list.iter().find(|r| r.perm == perm) {
                        if let Some(&i) = r.tools.first() {
                            app.tools[i].ask_own(id);
                        }
                    }
                }
                _ => perm.ask(),
            }
        }
    }
    y - area.top()
}

fn keycaps(p: &egui::Painter, right: Pos2, keys: &str) {
    let mut x = right.x;
    let caps: Vec<String> = {
        // modifiers are one char each; the key is the rest
        let mods: String = keys.chars().take_while(|c| "⌃⌥⇧⌘".contains(*c)).collect();
        let key: String = keys.chars().skip(mods.chars().count()).collect();
        let mut v: Vec<String> = mods.chars().map(|c| c.to_string()).collect();
        v.push(key);
        v
    };
    for c in caps.iter().rev() {
        let f: FontId = mono(12.0);
        let w = p.layout_no_wrap(c.clone(), f.clone(), TEXT).size().x.max(10.0) + 12.0;
        let r = Rect::from_min_size(Pos2::new(x - w, right.y - 11.0), Vec2::new(w, 22.0));
        p.rect_filled(r, 5.0, white(26));
        p.rect_filled(Rect::from_min_max(Pos2::new(r.left() + 1.0, r.bottom() - 1.5), Pos2::new(r.right() - 1.0, r.bottom())), CornerRadius::same(2), Color32::from_black_alpha(90));
        p.text(r.center(), Align2::CENTER_CENTER, c, f, TEXT);
        x = r.left() - 3.0;
    }
}

fn ready(app: &mut App, ui: &mut Ui, body: Rect, t: f32) {
    let p = ui.painter().clone();
    let left = Rect::from_min_max(body.min, Pos2::new(body.right() - 340.0, body.bottom()));
    let mut y = body.center().y - 120.0;
    p.text(Pos2::new(left.left(), y), Align2::LEFT_TOP, "Ready", bold(22.0), TEXT);
    y += 46.0;
    // open at login
    let row = Rect::from_min_size(Pos2::new(left.left(), y), Vec2::new(left.width(), 52.0));
    p.rect_filled(row, 10.0, SIDEBAR);
    icon(&p, Icon::Power, Rect::from_min_size(Pos2::new(row.left() + 14.0, row.center().y - 9.0), Vec2::splat(18.0)), TEXT_SOFT);
    p.text(Pos2::new(row.left() + 44.0, row.center().y), Align2::LEFT_CENTER, "Open at login", bold(13.0), TEXT);
    let on = if app.demo { true } else { crate::kit::login::enabled() };
    let sw = Rect::from_min_size(Pos2::new(row.right() - 50.0, row.center().y - 11.0), Vec2::new(38.0, 22.0));
    if switch(ui, sw, ui.scope_id().with("setup-login"), on, true).clicked() && !app.demo {
        if let Err(e) = crate::kit::login::set(!on) {
            eprintln!("[login] {e}");
        }
    }
    y += 62.0;
    // shortcuts of the enabled tools
    let keys: Vec<(Icon, &'static str, String)> = (0..app.tools.len()).filter(|&i| app.tool_on(i)).flat_map(|i| {
        let ic = app.tools[i].badge().icon;
        app.tools[i].setup_shortcuts().into_iter().map(move |(l, k)| (ic, l, k))
    }).collect();
    for (k, (ic, label, keys)) in keys.iter().enumerate() {
        let a = ease((t - 0.08 * (k + 1) as f32) / 0.4);
        let row = Rect::from_min_size(Pos2::new(left.left(), y - (1.0 - a) * 8.0), Vec2::new(left.width(), 44.0));
        p.rect_filled(row, 10.0, SIDEBAR.gamma_multiply(a));
        icon(&p, *ic, Rect::from_min_size(Pos2::new(row.left() + 14.0, row.center().y - 9.0), Vec2::splat(18.0)), TEXT_SOFT.gamma_multiply(a));
        p.text(Pos2::new(row.left() + 44.0, row.center().y), Align2::LEFT_CENTER, *label, font(13.0), TEXT.gamma_multiply(a));
        keycaps(&p, Pos2::new(row.right() - 12.0, row.center().y), keys);
        y += 52.0;
    }
    if needs_restart(app) {
        icon(&p, Icon::Refresh, Rect::from_min_size(Pos2::new(left.left(), y + 4.0), Vec2::splat(14.0)), Color32::from_rgb(0xFF, 0x9F, 0x0A));
        p.text(Pos2::new(left.left() + 22.0, y + 11.0), Align2::LEFT_CENTER, "Screen Recording needs a quick restart", font(12.0), Color32::from_rgb(0xFF, 0x9F, 0x0A));
    }
    // a little panel builds itself from the chosen tools, under the menu bar icon
    let pw = 300.0;
    let px = body.right() - pw;
    let on_tools: Vec<usize> = (0..app.tools.len()).filter(|&i| app.tool_on(i)).collect();
    let cell = (pw - 20.0 - 3.0 * 8.0) / 4.0;
    let spans: Vec<usize> = on_tools.iter().map(|&i| if matches!(app.tools[i].id(), "cleanup" | "system") { 2 } else { 1 }).collect();
    let mut rows_n = 0;
    {
        let mut col = 0;
        for s in &spans {
            if col + s > 4 {
                rows_n += 1;
                col = 0;
            }
            col += s;
        }
        if col > 0 {
            rows_n += 1;
        }
    }
    let ph = 20.0 + rows_n as f32 * 58.0 + (rows_n.max(1) as f32 - 1.0) * 8.0;
    let top = body.center().y - ph / 2.0 + 12.0;
    let bob = (t * 3.5).sin() * 2.0;
    let dot = Rect::from_center_size(Pos2::new(px + pw / 2.0, top - 16.0 + bob), Vec2::splat(13.0));
    p.rect_filled(dot.expand(4.0), 6.0, with_alpha(BLUE, 0.25));
    p.rect_filled(dot, 3.0, BLUE);
    let panel = Rect::from_min_size(Pos2::new(px, top), Vec2::new(pw, ph));
    p.rect_filled(panel, 12.0, Color32::from_rgb(0x1E, 0x1E, 0x20));
    p.rect_stroke(panel, 12.0, Stroke::new(1.0, white(36)), StrokeKind::Inside);
    let (mut col, mut row) = (0usize, 0usize);
    for (k, (&i, &s)) in on_tools.iter().zip(&spans).enumerate() {
        if col + s > 4 {
            col = 0;
            row += 1;
        }
        let a = pop((t - 0.3 - 0.09 * k as f32) / 0.45);
        let r = Rect::from_min_size(Pos2::new(panel.left() + 10.0 + col as f32 * (cell + 8.0), panel.top() + 10.0 + row as f32 * 66.0 - (1.0 - a.min(1.0)) * 14.0), Vec2::new(cell * s as f32 + 8.0 * (s as f32 - 1.0), 58.0));
        col += s;
        if a <= 0.0 {
            continue;
        }
        let blue = app.tools[i].id() == "cleanup";
        p.rect_filled(r, 9.0, (if blue { BLUE } else { SIDEBAR }).gamma_multiply(a.min(1.0)));
        icon(&p, app.tools[i].badge().icon, Rect::from_center_size(r.center(), Vec2::splat(18.0 * a)), if blue { Color32::WHITE } else { TEXT_SOFT });
    }
}
