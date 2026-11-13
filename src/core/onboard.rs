//! First-run setup: pick tools, allow what they need from macOS (live
//! status), crash reports (official builds only), open at login, then hand
//! over to the menu bar panel. Also the permission list in Settings › General.

use crate::kit::sync::LockExt;
use super::shell::App;
use crate::kit::glass::*;
use crate::kit::perms::{self, Perm, Status};
use egui::{Align2, Color32, CornerRadius, FontId, Pos2, Rect, Sense, Stroke, StrokeKind, Ui, Vec2};
use std::collections::HashMap;

pub const SETUP_W: f32 = 820.0;
pub const SETUP_H: f32 = 600.0;
/// Most steps setup can have (with the crash reports step).
const MAX_STEPS: usize = 7;
const TOP_H: f32 = 44.0;
const BOTTOM_H: f32 = 64.0;
/// Tools that start off on a fresh install (the rest start on).
pub const OFF_AT_FIRST: [&str; 2] = ["claude", "fan"];

/// What a setup step shows.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Kind {
    Welcome,
    Tools,
    Agents,
    Models,
    Perms,
    Crash,
    Ready,
}

/// The steps in order. The crash reports question only exists in official
/// builds (screenshots show it too).
fn kinds(app: &App) -> Vec<Kind> {
    let mut v = vec![Kind::Welcome, Kind::Tools, Kind::Agents];
    // only when an enabled tool needs a download (Dictation's speech model)
    // or already has one; models fetched on first use don't count
    if has_models_step(app) {
        v.push(Kind::Models);
    }
    v.push(Kind::Perms);
    if app.demo || crate::kit::crash_report::available() {
        v.push(Kind::Crash);
    }
    v.push(Kind::Ready);
    v
}

/// The setup window's state while it is open.
pub struct Setup {
    pub step: usize,
    /// When the current step appeared (its animations count from here).
    since: f64,
    /// The step before this one: the cat on the step dots hops from there.
    from: usize,
}

impl Setup {
    pub fn new(step: usize) -> Self {
        Setup { step, since: f64::NEG_INFINITY, from: step }
    }
}

/// Marker file: setup was finished or closed once.
pub fn done_marker() -> std::path::PathBuf {
    crate::kit::fs::data_path("setup-done")
}

fn step_file() -> std::path::PathBuf {
    crate::kit::fs::data_path("setup-step")
}

/// The step setup was on when the app last quit: some permissions make macOS
/// restart the app, and setup picks up where it was.
pub fn saved_step() -> usize {
    std::fs::read_to_string(step_file()).ok().and_then(|s| s.trim().parse().ok()).unwrap_or(0).min(MAX_STEPS - 1)
}

/// Setup was finished (closing the window no longer counts).
pub fn is_done() -> bool {
    done_marker().exists()
}

pub fn mark_done() {
    let _ = std::fs::remove_file(step_file());
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
    /// Last error of a tool's own setup step (shown on its row).
    own_err: HashMap<Perm, String>,
    /// When a permission turned Allowed while we watched (check pops).
    allowed_at: HashMap<Perm, f64>,
    /// Sample statuses for screenshots.
    pub demo: Option<HashMap<Perm, Status>>,
    /// Open at login, as last read (None until the first read lands).
    login: Option<bool>,
    /// macOS answers slowly enough to drop frames, so reads run off the ui thread.
    bg: std::sync::Arc<std::sync::Mutex<Bg>>,
}

#[derive(Default)]
struct Bg {
    running: bool,
    result: Option<(Vec<(Perm, Status)>, bool)>,
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
    /// Start a background read of `perms` and Open at login (at most once a second).
    fn kick(&mut self, perms: Vec<Perm>, now: f64) {
        if now - self.polled < 1.0 && self.polled > 0.0 {
            return;
        }
        {
            let mut b = self.bg.lock_ok();
            if b.running {
                return;
            }
            b.running = true;
        }
        self.polled = now;
        let bg = self.bg.clone();
        std::thread::spawn(move || {
            let v: Vec<(Perm, Status)> = perms.into_iter().map(|p| (p, p.status())).collect();
            let login = crate::kit::login::enabled();
            let mut b = bg.lock_ok();
            b.result = Some((v, login));
            b.running = false;
        });
    }

    fn apply(&mut self, perm: Perm, s: Status, now: f64) {
        let before = self.status.insert(perm, s);
        if s == Status::Allowed && before.is_some_and(|b| b != Status::Allowed) {
            self.allowed_at.insert(perm, now);
        }
        // allowed, or refused a while after asking: the wait is over
        if s == Status::Allowed || (s == Status::Denied && self.asked.get(&perm).is_some_and(|t| now - t > 3.0)) {
            self.asked.remove(&perm);
        }
    }

    /// Refresh statuses for `rows`: system ones in the background, at most
    /// once a second; a tool's own ones here (they're cheap).
    pub fn poll(&mut self, app_tools: &[Box<dyn super::tool::Tool>], rows: &[Row], now: f64) {
        if let Some(d) = self.demo.clone() {
            for r in rows {
                self.apply(r.perm, d.get(&r.perm).copied().unwrap_or(Status::NotAsked), now);
            }
            return;
        }
        let landed = self.bg.lock_ok().result.take();
        if let Some((v, login)) = landed {
            self.login = Some(login);
            for (p, st) in v {
                self.apply(p, st, now);
            }
        }
        let due = now - self.polled >= 1.0 || self.polled == 0.0;
        if due {
            for r in rows {
                if let Perm::Own { id, .. } = r.perm {
                    if let Some(&i) = r.tools.first() {
                        let st = app_tools[i].own_status(id);
                        self.apply(r.perm, st, now);
                        // the tool says when its setup is over: stop the
                        // spinner then (cancelled or failed), not after a minute
                        if let Some((busy, err)) = app_tools[i].own_progress(id) {
                            if !busy {
                                self.asked.remove(&r.perm);
                            }
                            self.own_err.insert(r.perm, err);
                        }
                    }
                }
            }
        }
        let sys: Vec<Perm> = rows.iter().map(|r| r.perm).filter(|p| !matches!(p, Perm::Own { .. })).collect();
        self.kick(sys, now);
    }

    /// Open at login, read in the background (macOS is slow to answer).
    pub fn login_on(&mut self, demo: bool, now: f64) -> bool {
        if demo {
            return true;
        }
        if let Some((_, login)) = self.bg.lock_ok().result.as_ref() {
            self.login = Some(*login);
        }
        self.kick(Vec::new(), now);
        self.login.unwrap_or(false)
    }

    /// Turn Open at login on or off, and show it at once.
    pub fn set_login(&mut self, on: bool) {
        crate::kit::login::settle();
        match crate::kit::login::set(on) {
            Ok(()) => self.login = Some(on),
            Err(e) => eprintln!("[login] {e}"),
        }
    }

    pub fn status(&self, p: Perm) -> Status {
        self.status.get(&p).copied().unwrap_or(Status::NotAsked)
    }

    /// The status once a read has landed (None before that).
    pub fn known(&self, p: Perm) -> Option<Status> {
        self.status.get(&p).copied()
    }

    /// Ask for a system permission (the spinner shows until it flips).
    pub fn ask(&mut self, p: Perm, now: f64) {
        self.asked.insert(p, now);
        self.polled = 0.0;
        if self.demo.is_none() {
            p.ask();
        }
    }

    pub fn waiting(&self, p: Perm, now: f64) -> bool {
        self.asked.get(&p).is_some_and(|t| now - t < 60.0) && self.status(p) != Status::Allowed
    }

    /// Something is animating or waiting: keep frames coming.
    fn busy(&self, now: f64) -> bool {
        !self.asked.is_empty() || self.allowed_at.values().any(|t| now - t < 0.9)
    }
}

/// Screen Recording was allowed after launch: capture works after a restart
/// (once: a restarted copy that still doesn't have it won't get it from another).
pub fn needs_restart(app: &App) -> bool {
    app.perm_watch.demo.is_none() && app.perm_watch.status(Perm::Screen) == Status::Allowed && !perms::screen_at_launch() && !perms::relaunched() && rows(app).iter().any(|r| r.perm == Perm::Screen)
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

/// The mascot's colour: lime on dark, deep green on light (lime on the pale
/// light backdrop is too faint).
fn cat_color() -> Color32 {
    if is_light() { ACCENT() } else { BLUE() }
}

/// An ear flick starting at `at` (seconds): two quick outward tilts, in radians.
fn twitch(t: f32, at: f32) -> f32 {
    let u = (t - at) / 0.36;
    if !(0.0..1.0).contains(&u) {
        return 0.0;
    }
    (u * std::f32::consts::TAU).sin().abs() * 0.28 * (1.0 - 0.4 * u)
}

/// How long the cat's hop from one step dot to the next takes.
const HOP: f32 = 0.32;

/// The step dots along the top. The cat sits on the current step's dot and
/// hops there from the step before; a dot lights up when the cat lands.
fn step_dots(p: &egui::Painter, full: Rect, steps: usize, from: usize, step: usize, t: f32) {
    const D: f32 = 6.0;
    const GAP: f32 = 22.0;
    let total = steps as f32 * D + (steps as f32 - 1.0) * GAP;
    let x = |i: usize| full.center().x - total / 2.0 + D / 2.0 + i as f32 * (D + GAP);
    let y = full.top() + TOP_H / 2.0 + 9.0;
    let k = (t / HOP).clamp(0.0, 1.0);
    let moving = from != step;
    for i in 0..steps {
        let lit = i < step || (i == step && (!moving || k >= 1.0)) || (i <= from && i <= step);
        p.circle_filled(Pos2::new(x(i), y), D / 2.0, if lit { BLUE() } else { white(46) });
    }
    // the Welcome step has the big cat: back there, the small one hops
    // onto its dot and fades away
    let fade = if step == 0 { 1.0 - ((t - HOP) / 0.2).clamp(0.0, 1.0) } else { 1.0 };
    if fade <= 0.0 || (step == 0 && !moving) {
        return;
    }
    let m = k * k * (3.0 - 2.0 * k);
    let cx = egui::lerp(x(from)..=x(step), m);
    let jump = if moving { (k * std::f32::consts::PI).sin() * 8.0 } else { 0.0 };
    // a squash as it lands
    let l = ((t - HOP) / 0.2).clamp(0.0, 1.0);
    let sq = if moving && l > 0.0 && l < 1.0 { (l * std::f32::consts::PI).sin() * 0.18 } else { 0.0 };
    let size = 16.0;
    let (w, h) = (size * (1.0 + sq), size * (1.0 - sq));
    let bottom = y - D / 2.0 - 3.0 - jump;
    let r = Rect::from_min_max(Pos2::new(cx - w / 2.0, bottom - h), Pos2::new(cx + w / 2.0, bottom));
    crate::kit::brand::draw_mascot(p, r, cat_color().gamma_multiply(fade), (0.0, twitch(t, HOP + 0.15)));
}

/// A soft vertical band of `col`, brightest at `x` and `w` wide, inside `clip`.
fn glow_band(p: &egui::Painter, clip: Rect, x: f32, w: f32, col: Color32) {
    let p = p.with_clip_rect(clip.intersect(p.clip_rect()));
    let mut m = egui::Mesh::default();
    for (xx, c) in [(x - w / 2.0, Color32::TRANSPARENT), (x, col), (x + w / 2.0, Color32::TRANSPARENT)] {
        m.colored_vertex(Pos2::new(xx, clip.top()), c);
        m.colored_vertex(Pos2::new(xx, clip.bottom()), c);
    }
    m.add_triangle(0, 1, 2);
    m.add_triangle(1, 3, 2);
    m.add_triangle(2, 3, 4);
    m.add_triangle(3, 5, 4);
    p.add(m);
}

/// The whole setup screen in `full` (the app window, or the root in screenshots).
/// Says whether the person finished it.
pub fn setup_ui(app: &mut App, ui: &mut Ui, full: Rect) -> Finish {
    let ctx = ui.ctx().clone();
    let now = ctx.input(|i| i.time);
    let p = ui.painter().clone();
    let Some(step) = app.setup.as_ref().map(|s| s.step) else { return Finish::No };
    let kinds = kinds(app);
    let steps = kinds.len();
    let step = step.min(steps - 1);
    let kind = kinds[step];
    // a step's animations start when it can be seen: not while the window
    // is still hidden or covered (they would play to nobody)
    let unseen = ctx.input(|i| i.viewport().occluded == Some(true) || i.viewport().minimized == Some(true));
    if let Some(s) = app.setup.as_mut() {
        if s.since == f64::NEG_INFINITY && !unseen {
            s.since = now;
        }
    }
    let since = app.setup.as_ref().map(|s| s.since).unwrap_or(now);
    let t = if since == f64::NEG_INFINITY { 0.0 } else { (now - since) as f32 };
    // screenshots show each step settled, not mid-animation
    let t = if app.demo { t + 5.0 } else { t };
    // setup lives in the app window: a centered column when that is wider
    let win = full;
    let full = Rect::from_center_size(win.center(), Vec2::new(win.width().min(SETUP_W + 80.0), win.height()));
    // drag the window by its top strip
    let drag = Rect::from_min_size(win.min + Vec2::new(80.0, 0.0), Vec2::new(win.width() - 80.0, TOP_H));
    if ui.interact(drag, ui.scope_id().with("setup-drag"), Sense::click_and_drag()).drag_started() {
        ctx.send_viewport_cmd(egui::ViewportCommand::StartDrag);
    }
    let from = app.setup.as_ref().map(|s| s.from).unwrap_or(step).min(steps - 1);
    step_dots(&p, full, steps, from, step, t);
    let body = Rect::from_min_max(Pos2::new(full.left() + 40.0, full.top() + TOP_H), Pos2::new(full.right() - 40.0, full.bottom() - BOTTOM_H));
    // the step slides in from the side it comes from
    let side = if from > step { -1.0 } else { 1.0 };
    let slide = (1.0 - ease(t * 2.2)) * 18.0 * side;
    let body = body.translate(Vec2::new(slide, 0.0));
    let mut next = false;
    let mut restart_now = false;
    match kind {
        Kind::Welcome => next = welcome(app, ui, body, t),
        Kind::Tools => tools(app, ui, body, t),
        Kind::Agents => agents(app, ui, body, t),
        Kind::Models => models_step(app, ui, body, t),
        Kind::Perms => permissions_step(app, ui, body, t, now),
        Kind::Crash => next = crash_step(app, ui, body, t),
        Kind::Ready => restart_now = ready(app, ui, body, t),
    }
    // bottom bar
    let bar = Rect::from_min_max(Pos2::new(full.left() + 24.0, full.bottom() - BOTTOM_H), Pos2::new(full.right() - 24.0, full.bottom()));
    let mut finish = if restart_now { Finish::Restart } else { Finish::No };
    if step > 0 {
        let r = Rect::from_min_size(Pos2::new(bar.left(), bar.center().y - 16.0), Vec2::new(70.0, 32.0));
        if ghost_button(ui, r, "Back").clicked() {
            go(app, step - 1);
        }
    }
    let label = match kind {
        // these steps have their own buttons
        Kind::Welcome | Kind::Crash => None,
        Kind::Tools | Kind::Agents | Kind::Models | Kind::Perms => Some("Continue"),
        Kind::Ready if needs_restart(app) => Some("Restart and open"),
        Kind::Ready => Some("Open panel"),
    };
    if let Some(label) = label {
        let w = button_width(&p, label, BTN_PRIMARY).max(110.0);
        let r = Rect::from_min_size(Pos2::new(bar.right() - w, bar.center().y - 16.0), Vec2::new(w, 32.0));
        if pill_button(ui, r, label, BTN_PRIMARY, true).clicked() {
            if kind == Kind::Ready {
                finish = if needs_restart(app) { Finish::Restart } else { Finish::Panel };
            } else {
                next = true;
            }
        }
        if kind == Kind::Perms && !all_required(app) {
            let r2 = Rect::from_min_size(Pos2::new(r.left() - 80.0, r.top()), Vec2::new(70.0, 32.0));
            if ghost_button(ui, r2, "Later").clicked() {
                next = true;
            }
        }
    }
    if next {
        go(app, step + 1);
    }
    // setup is an immediate window drawn in the (hidden) root's pass: ask
    // the root for the frames too, or after a click the window kept showing
    // the old step until it was resized (first launch, M1)
    let animating = t < 2.5 || app.perm_watch.busy(now) || next;
    if animating {
        ctx.request_repaint();
        ctx.request_repaint_of(egui::ViewportId::ROOT);
    } else {
        ctx.request_repaint_after(std::time::Duration::from_millis(1000));
        ctx.request_repaint_after_for(std::time::Duration::from_millis(1000), egui::ViewportId::ROOT);
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
    let last = kinds(app).len() - 1;
    if let Some(s) = app.setup.as_mut() {
        s.from = s.step;
        s.step = step.min(last);
        s.since = f64::NEG_INFINITY;
        if !app.demo {
            let f = step_file();
            if let Some(d) = f.parent() {
                let _ = std::fs::create_dir_all(d);
            }
            let _ = std::fs::write(f, s.step.to_string());
        }
    }
}

fn all_required(app: &App) -> bool {
    rows(app).iter().filter(|r| !r.optional).all(|r| app.perm_watch.status(r.perm) == Status::Allowed)
}

fn ghost_button(ui: &mut Ui, r: Rect, label: &str) -> egui::Response {
    let resp = ui.interact(r, ui.scope_id().with(("ghost", label)), Sense::click());
    let col = if resp.hovered() { TEXT() } else { TEXT_DIM() };
    ui.painter().text(r.center(), Align2::CENTER_CENTER, label, font(13.0), col);
    if resp.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    resp
}

/// The app's mark: a dark squircle with the Cmd+Meow glyph, fading in with `sweep`.
fn app_mark(p: &egui::Painter, c: Pos2, size: f32, sweep: f32) {
    let r = Rect::from_center_size(c, Vec2::splat(size));
    p.rect_filled(r, size * 0.225, Color32::from_rgb(0x22, 0x22, 0x26));
    p.rect_stroke(r, size * 0.225, Stroke::new(1.0, white(36)), StrokeKind::Inside);
    let a = (0.35 + 0.65 * sweep.clamp(0.0, 1.0)) * 255.0;
    brand_glyph(p, c, size * 0.6, Color32::from_white_alpha(a as u8));
}

/// The Cmd+Meow glyph (as in the menu bar), tinted `col`.
fn brand_glyph(p: &egui::Painter, c: Pos2, size: f32, col: Color32) {
    let id = egui::Id::NULL.with("cmdmeow-brand-glyph");
    let tex = p.ctx().data_mut(|d| d.get_temp::<egui::TextureHandle>(id)).unwrap_or_else(|| {
        let n = 128;
        let mut px = crate::kit::brand::glyph_rgba(n);
        for c in px.chunks_mut(4) {
            c[0] = 255;
            c[1] = 255;
            c[2] = 255;
        }
        let img = egui::ColorImage::from_rgba_unmultiplied([n as usize, n as usize], &px);
        let t = p.ctx().load_texture("cmdmeow-brand-glyph", img, egui::TextureOptions::LINEAR);
        p.ctx().data_mut(|d| d.insert_temp(id, t.clone()));
        t
    });
    let uv = Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0));
    p.image(tex.id(), Rect::from_center_size(c, Vec2::splat(size)), uv, col);
}

fn welcome(app: &mut App, ui: &mut Ui, body: Rect, t: f32) -> bool {
    let p = ui.painter().clone();
    let c = Pos2::new(body.center().x, body.top() + 150.0);
    // the cat peeks out ears first, flicks them, then hops up; the ring
    // draws itself round it and the tools land on it one by one
    let ring = 104.0;
    crate::kit::glass::stroke_circle(&p, c, ring, Stroke::new(2.0, white(14)));
    let start = -std::f32::consts::FRAC_PI_2;
    let k = ease((t - 1.15) / 0.9);
    if k > 0.0 {
        p.add(egui::Shape::line(arc_points(c, ring, start, start + std::f32::consts::TAU * k, 96), Stroke::new(2.0, BLUE())));
    }
    let n = app.tools.len().max(1);
    for (i, tool) in app.tools.iter().enumerate() {
        let a = start + i as f32 / n as f32 * std::f32::consts::TAU;
        let at = c + Vec2::new(a.cos(), a.sin()) * ring;
        let s = pop((t - 1.25 - 0.07 * i as f32) / 0.4);
        if s <= 0.0 {
            continue;
        }
        p.circle_filled(at, 15.0 * s, BG());
        crate::kit::glass::stroke_circle(&p, at, 15.0 * s, Stroke::new(1.0, white(36)));
        icon(&p, tool.badge().icon, Rect::from_center_size(at, Vec2::splat(14.0 * s)), TEXT_SOFT());
    }
    let size = 128.0;
    // peek() is laid out for a 96 pt cat
    let (rise, sx, sy) = peek(t);
    let rise = rise * size / 96.0;
    let floor = c.y + size / 2.0;
    let (w, h) = (size * sx, size * sy);
    let cat = Rect::from_min_max(Pos2::new(c.x - w / 2.0, floor - rise - h), Pos2::new(c.x + w / 2.0, floor - rise));
    // below the floor the cat is hidden: that's where it peeks out from
    let clip = Rect::from_min_max(Pos2::new(c.x - 90.0, c.y - 130.0), Pos2::new(c.x + 90.0, floor));
    let ears = (twitch(t, 0.42) + twitch(t, 2.3), twitch(t, 0.6) + twitch(t, 1.55));
    crate::kit::brand::draw_mascot(&p.with_clip_rect(clip.intersect(p.clip_rect())), cat, cat_color(), ears);
    let name = p.text(Pos2::new(c.x, c.y + ring + 32.0), Align2::CENTER_TOP, "Cmd+Meow", bold(28.0), TEXT());
    p.text(Pos2::new(c.x, name.bottom() + sp::LINE_GAP), Align2::CENTER_TOP, format!("{} tools. One menu bar.", app.tools.len()), font(13.0), TEXT_FAINT());
    let r = Rect::from_center_size(Pos2::new(c.x, c.y + ring + 128.0), Vec2::new(128.0, 40.0));
    pill_button(ui, r, "Set up", BTN_PRIMARY, true).clicked()
}

/// The Welcome cat at `t`: how far it is lifted from where it rests (0 =
/// resting, negative = still below the floor) and its squash (width, height).
fn peek(t: f32) -> (f32, f32, f32) {
    let lerp = |a: f32, b: f32, x: f32| a + (b - a) * x.clamp(0.0, 1.0);
    match t {
        // hidden, then the ears come up over the floor
        t if t < 0.45 => (lerp(-100.0, -72.0, ease((t - 0.1) / 0.35)), 1.0, 1.0),
        // looking round
        t if t < 0.85 => (-72.0, 1.0, 1.0),
        // crouch
        t if t < 0.97 => {
            let x = ease((t - 0.85) / 0.12);
            (lerp(-72.0, -80.0, x), 1.0 + 0.04 * x, 1.0 - 0.08 * x)
        }
        // jump, stretched
        t if t < 1.2 => {
            let x = ease((t - 0.97) / 0.23);
            (lerp(-80.0, 14.0, x), lerp(1.04, 0.96, x), lerp(0.92, 1.05, x))
        }
        // fall back and land with a squash
        t if t < 1.36 => {
            let x = (t - 1.2) / 0.16;
            (lerp(14.0, 0.0, x * x), 1.0, 1.0)
        }
        t => {
            let s = pop((t - 1.36) / 0.3);
            let sq = if s < 1.0 { (1.0 - s) * 0.14 } else { 0.0 };
            (0.0, 1.0 + sq, 1.0 - sq)
        }
    }
}

/// Crash reports, asked once (official builds only). Returns true when the
/// person picked an answer: setup moves on.
fn crash_step(app: &mut App, ui: &mut Ui, body: Rect, t: f32) -> bool {
    let p = ui.painter().clone();
    let c = Pos2::new(body.center().x, body.top() + 96.0);
    let z = pop(t / 0.5).max(0.0);
    if z > 0.0 {
        let mark = Rect::from_center_size(c, Vec2::splat(80.0 * z));
        p.rect_filled(mark, 18.0 * z, CARD());
        p.rect_stroke(mark, 18.0 * z, Stroke::new(1.0, white(24)), StrokeKind::Inside);
        icon(&p, Icon::Shield, mark.shrink(20.0 * z), BLUE());
    }
    let q = p.text(Pos2::new(c.x, c.y + 62.0), Align2::CENTER_TOP, "Send crash reports?", bold(22.0), TEXT());
    p.text(Pos2::new(c.x, q.bottom() + sp::LINE_GAP), Align2::CENTER_TOP, "They help fix bugs faster. Your personal data stays on your Mac.", font(13.0), TEXT_FAINT());
    // what a report has, and what it never has
    let facts: [(bool, &str); 3] = [(true, "What went wrong and where in the app"), (true, "App and macOS version"), (false, "Files, names or IP address")];
    let w = 400.0;
    let mut y = c.y + 134.0;
    for (k, (sent, label)) in facts.iter().enumerate() {
        let a = ease((t - 0.15 - 0.08 * k as f32) / 0.4);
        let row = Rect::from_min_size(Pos2::new(c.x - w / 2.0, y - (1.0 - a) * 8.0), Vec2::new(w, 40.0));
        p.rect_filled(row, 10.0, CARD().gamma_multiply(a));
        let m = Pos2::new(row.left() + 22.0, row.center().y);
        if *sent {
            check(&p, m, 12.0, GREEN().gamma_multiply(a));
        } else {
            icon(&p, Icon::Close, Rect::from_center_size(m, Vec2::splat(12.0)), TEXT_MUTED().gamma_multiply(a));
        }
        let col = if *sent { TEXT() } else { TEXT_MUTED() };
        p.text(Pos2::new(row.left() + 44.0, row.center().y), Align2::LEFT_CENTER, *label, font(13.0), col.gamma_multiply(a));
        y += 48.0;
    }
    // the answer
    let yes = "Send reports";
    let bw = button_width(&p, yes, BTN_PRIMARY).max(140.0);
    let no_w = 96.0;
    let gap = 12.0;
    let left = c.x - (bw + gap + no_w) / 2.0;
    let by = y + 14.0;
    let mut picked = None;
    if ghost_button(ui, Rect::from_min_size(Pos2::new(left, by), Vec2::new(no_w, 36.0)), "Don't send").clicked() {
        picked = Some(false);
    }
    if pill_button(ui, Rect::from_min_size(Pos2::new(left + no_w + gap, by), Vec2::new(bw, 36.0)), yes, BTN_PRIMARY, true).clicked() {
        picked = Some(true);
    }
    match picked {
        Some(on) => {
            if !app.demo {
                crate::kit::crash_report::set_consent(on);
            }
            true
        }
        None => false,
    }
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
    let g = p.text(Pos2::new(body.left(), head), Align2::LEFT_TOP, "Tools", bold(22.0), TEXT());
    // the count rolls to its new number like an odometer
    let shown = ui.ctx().animate_value_with_time(ui.scope_id().with("setup-tools-on"), on as f32, 0.28);
    rolling_count(&p, Pos2::new(g.right() + 10.0, g.bottom() - 4.0), shown, app.tools.len());
    let secs = sections(app);
    let cols = secs.len().max(1) as f32;
    let gap = 12.0;
    let cw = (body.width() - gap * (cols - 1.0)) / cols;
    let top = g.bottom() + 20.0;
    let mut flip = None;
    let mut k = 0;
    for (ci, sec) in secs.iter().enumerate() {
        let x = body.left() + ci as f32 * (cw + gap);
        p.text(Pos2::new(x + 2.0, top), Align2::LEFT_TOP, sec, font(11.0), TEXT_FAINT());
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
            p.rect_filled(r, 10.0, lerp_color(CARD(), white(30), hov * 0.15).gamma_multiply(a));
            p.rect_stroke(r, 10.0, Stroke::new(1.0, white(12).gamma_multiply(a)), StrokeKind::Inside);
            // the tile jumps up when switched on and dips when switched off
            let jolt = ui.ctx().animate_bool_with_time(id.with("jolt"), o, 0.32);
            let bump = 4.0 * jolt * (1.0 - jolt) * if o { 0.16 } else { -0.1 };
            let ib = Rect::from_center_size(Pos2::new(r.left() + 29.0, r.center().y), Vec2::splat(34.0 * (1.0 + bump)));
            p.rect_filled(ib, 9.0 * (1.0 + bump), lerp_color(white(16), BLUE(), s));
            let b = app.tools[i].badge();
            icon(&p, b.icon, ib.shrink(8.0 * (1.0 + bump)), lerp_color(TEXT_MUTED(), ON_ACTION, s));
            let tx = r.left() + 56.0;
            // name + permission icons as one block, centered, LINE_GAP apart
            let block = if app.tools[i].needs().is_empty() { 16.0 } else { 16.0 + sp::LINE_GAP + 12.0 };
            let ny = r.center().y - block / 2.0;
            text_fit(&p, Pos2::new(tx, ny), Align2::LEFT_TOP, app.tools[i].name(), bold(13.0), lerp_color(TEXT_MUTED(), TEXT(), s), r.right() - tx - 24.0);
            // what it will ask for
            let mut gx = tx;
            for n in app.tools[i].needs() {
                icon(&p, n.perm.icon(), Rect::from_min_size(Pos2::new(gx, ny + 16.0 + sp::LINE_GAP), Vec2::splat(12.0)), TEXT_MUTED());
                gx += 16.0;
            }
            if s > 0.01 {
                let c = Pos2::new(r.right() - 14.0, r.top() + 14.0);
                let z = pop(s);
                p.circle_filled(c, 8.0 * z, BLUE());
                check(&p, c, 7.0 * z, ON_ACTION);
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

/// The AI agents the person uses: one main (Cleanup checks with it), and
/// the ones AI Usage tracks. Saved in the AI Usage settings, changed later in
/// Settings › AI Usage.
/// "`n`/`total`" where `n` rolls between whole numbers as `shown` moves:
/// the old number slides up out of view and the new one comes up from below.
fn rolling_count(p: &egui::Painter, left_bottom: Pos2, shown: f32, total: usize) {
    let f = mono(13.0);
    let lo = shown.floor();
    let frac = shown - lo;
    let lo_s = format!("{}", lo as i64);
    let hi_s = format!("{}", lo as i64 + 1);
    let w = p.layout_no_wrap(if frac > 0.0 { hi_s.clone() } else { lo_s.clone() }, f.clone(), TEXT_FAINT()).size().x.max(p.layout_no_wrap(lo_s.clone(), f.clone(), TEXT_FAINT()).size().x);
    let line = 16.0;
    let cell = Rect::from_min_max(Pos2::new(left_bottom.x, left_bottom.y - line), Pos2::new(left_bottom.x + w, left_bottom.y + 1.0));
    let clip = p.with_clip_rect(cell.intersect(p.clip_rect()));
    clip.text(left_bottom - Vec2::new(0.0, frac * line), Align2::LEFT_BOTTOM, &lo_s, f.clone(), TEXT_FAINT());
    if frac > 0.0 {
        clip.text(left_bottom + Vec2::new(0.0, (1.0 - frac) * line), Align2::LEFT_BOTTOM, &hi_s, f.clone(), TEXT_FAINT());
    }
    p.text(Pos2::new(cell.right(), left_bottom.y), Align2::LEFT_BOTTOM, format!("/{total}"), f, TEXT_FAINT());
}

fn agents(app: &mut App, ui: &mut Ui, body: Rect, t: f32) {
    use crate::kit::ai::{self, settings};
    let p = ui.painter().clone();
    let set = settings::get();
    let head = body.top() + 6.0;
    let g = p.text(Pos2::new(body.left(), head), Align2::LEFT_TOP, "AI agents", bold(22.0), TEXT());
    let sub = text_wrap(&p, Pos2::new(body.left(), g.bottom() + sp::LINE_GAP), Align2::LEFT_TOP, "Main does AI Check in Cleanup, Uninstaller, Startup, Disk Map and Mac Health. Tracked agents show in AI Usage.", ty::body(), TEXT_DIM(), body.width().min(560.0), 3);
    let w = body.width().min(560.0);
    let x = body.left();
    let col_track = x + w - sp::L - 8.0;
    let col_main = col_track - 56.0;
    let top = sub.bottom() + sp::XL;
    p.text(Pos2::new(col_main, top), Align2::CENTER_TOP, "Main", ty::caption(), TEXT_FAINT());
    p.text(Pos2::new(col_track, top), Align2::CENTER_TOP, "Track", ty::caption(), TEXT_FAINT());
    let n = ai::PROVIDERS.len();
    let a = ease(t / 0.4);
    let card = Rect::from_min_size(Pos2::new(x, top + 20.0 + (1.0 - a) * 8.0), Vec2::new(w, n as f32 * sp::ROW_H));
    glass_card(&p, card, R_CARD, 0.0, false);
    // a light sweeps across the card as the agents are looked for; each
    // "Found" shows once it has passed
    let (sweep0, sweep_t) = (0.15, 0.8);
    let band_w = 140.0;
    let travel = card.width() + band_w;
    let u = (t - sweep0) / sweep_t;
    if (0.0..1.0).contains(&u) {
        glow_band(&p, card, card.left() - band_w / 2.0 + travel * u, band_w, with_alpha(BLUE(), 0.12));
    }
    let seen_at = |x: f32| sweep0 + sweep_t * (x - card.left() + band_w / 2.0) / travel;
    // a ping round the main agent's dot when it changes
    let now = ui.ctx().input(|i| i.time);
    let ping_id = ui.scope_id().with("setup-main-ping");
    let (was, since) = ui.ctx().data(|d| d.get_temp::<(String, f64)>(ping_id)).unwrap_or_else(|| (set.main.clone(), f64::NEG_INFINITY));
    let since = if was != set.main { now } else { since };
    ui.ctx().data_mut(|d| d.insert_temp(ping_id, (set.main.clone(), since)));
    let ping = ((now - since) / 0.55) as f32;
    type Change = Box<dyn FnOnce(&mut settings::Settings)>;
    let mut change: Option<Change> = None;
    for (k, m) in ai::PROVIDERS.iter().enumerate() {
        let r = Rect::from_min_size(Pos2::new(card.left(), card.top() + k as f32 * sp::ROW_H), Vec2::new(w, sp::ROW_H));
        if k > 0 {
            hairline(&p, Pos2::new(r.left() + sp::L, r.top()), Pos2::new(r.right() - sp::L, r.top()));
        }
        let found = app.demo || ai::make(k).data_dir().is_some_and(|d| d.exists());
        let name = p.text(Pos2::new(r.left() + sp::L, r.center().y), Align2::LEFT_CENTER, m.name, ty::title(), TEXT());
        let sx = name.right() + sp::S;
        let sa = ease((t - seen_at(sx) - 0.04 * k as f32) / 0.25);
        p.text(Pos2::new(sx - (1.0 - sa) * 6.0, r.center().y), Align2::LEFT_CENTER, if found { "Found" } else { "Not found" }, ty::label(), (if found { GREEN() } else { TEXT_FAINT() }).gamma_multiply(sa));
        let main = set.main == m.id;
        let on = set.is_on(m.id);
        let rr = Rect::from_center_size(Pos2::new(col_main, r.center().y), Vec2::splat(16.0));
        if main && (0.0..1.0).contains(&ping) {
            let e = ease(ping);
            crate::kit::glass::stroke_circle(&p, rr.center(), 8.0 + 13.0 * e, Stroke::new(1.5, with_alpha(BLUE(), 0.7 * (1.0 - e))));
            ui.ctx().request_repaint();
        }
        if radio(ui, rr, ui.scope_id().with(("setup-main", m.id)), main, found || main).clicked() && !main && found {
            let id = m.id.to_string();
            change = Some(Box::new(move |s| {
                s.off.retain(|x| *x != id);
                s.provider = id.clone();
                s.main = id;
            }));
        }
        // the main agent is always tracked
        let cb = Rect::from_center_size(Pos2::new(col_track, r.center().y), Vec2::splat(16.0));
        let track = checkbox(ui, cb, ui.scope_id().with(("setup-track", m.id)), on || main, true);
        let track = if main { track.on_hover_text("The main agent is always tracked") } else { track };
        if track.clicked() && !main {
            let id = m.id.to_string();
            change = Some(Box::new(move |s| {
                s.off.retain(|x| *x != id);
                if on {
                    s.off.push(id);
                }
            }));
        }
    }
    if let Some(f) = change {
        settings::update(!app.demo, f);
    }
}

/// One model on the Download models step and the tools it's for: (tool
/// index, what that tool uses it for). Several tools can share one file.
struct ModelRow {
    tools: Vec<(usize, &'static str)>,
    model: crate::core::tool::SetupModel,
}

/// Every enabled tool's models, one row per model (shared ones merged; the
/// tool that is downloading it, or needs it now, comes first).
fn models(app: &App) -> Vec<ModelRow> {
    use crate::core::tool::ModelState;
    let mut rows: Vec<ModelRow> = Vec::new();
    for i in (0..app.tools.len()).filter(|&i| app.tool_on(i)) {
        for m in app.tools[i].setup_models() {
            match rows.iter_mut().find(|r| r.model.name == m.name) {
                Some(r) if r.model.state == ModelState::OnFirstUse && m.state != ModelState::OnFirstUse => {
                    r.tools.insert(0, (i, m.what));
                    r.model = m;
                }
                Some(r) => r.tools.push((i, m.what)),
                None => rows.push(ModelRow { tools: vec![(i, m.what)], model: m }),
            }
        }
    }
    rows
}

/// The step is there when a model needs a download now or is already here.
fn has_models_step(app: &App) -> bool {
    models(app).iter().any(|r| r.model.state != crate::core::tool::ModelState::OnFirstUse)
}

fn models_step(app: &mut App, ui: &mut Ui, body: Rect, t: f32) {
    use crate::core::tool::ModelState;
    let p = ui.painter().clone();
    let head = body.top() + 6.0;
    let g = p.text(Pos2::new(body.left(), head), Align2::LEFT_TOP, "Download models", bold(22.0), TEXT());
    let sub = text_wrap(&p, Pos2::new(body.left(), g.bottom() + sp::LINE_GAP), Align2::LEFT_TOP, "They run on this Mac. Each one says which tool uses it. The download keeps going if you continue.", ty::body(), TEXT_DIM(), body.width().min(560.0), 3);
    let list = models(app);
    let w = body.width().min(620.0);
    let mut y = sub.bottom() + sp::XL;
    let mut start: Option<(usize, &'static str)> = None;
    for (k, row) in list.iter().enumerate() {
        let (m, i) = (&row.model, row.tools[0].0);
        let a = ease((t - 0.05 * k as f32) / 0.4);
        // two lines (the model, then the tools it's for), a third for the bar or an error
        let third = matches!(m.state, ModelState::Downloading { .. } | ModelState::Unpacking | ModelState::Failed(_));
        let row_h = if third { 86.0 } else { 66.0 };
        let r = Rect::from_min_size(Pos2::new(body.left(), y + (1.0 - a) * 8.0), Vec2::new(w, row_h));
        y += row_h + sp::S;
        glass_card(&p, r, R_CARD, 0.0, false);
        let ib = Rect::from_min_size(Pos2::new(r.left() + 14.0, r.top() + 18.0), Vec2::splat(30.0));
        p.rect_filled(ib, 8.0, white(16));
        icon(&p, app.tools[i].badge().icon, ib.shrink(7.0), if m.state == ModelState::Ready { GREEN() } else { TEXT_SOFT() });
        let tx = ib.right() + 12.0;
        let right = r.right() - 14.0;
        let line1 = r.top() + 22.0;
        let line2 = r.top() + 44.0;
        let title = p.text(Pos2::new(tx, line1), Align2::LEFT_CENTER, m.name, bold(13.0), TEXT());
        p.text(Pos2::new(title.right() + 8.0, line1), Align2::LEFT_CENTER, human_bytes(m.bytes), font(12.0), TEXT_FAINT());
        // which tools use it, and for what
        let uses = row.tools.iter().map(|(ti, what)| format!("{}: {}", app.tools[*ti].name(), what.to_lowercase())).collect::<Vec<_>>().join(" · ");
        let status_w = 150.0;
        text_fit(&p, Pos2::new(tx, line2), Align2::LEFT_CENTER, &uses, font(12.0), TEXT_DIM(), right - tx - status_w);
        let bar = Rect::from_min_max(Pos2::new(tx, r.bottom() - 16.0), Pos2::new(right, r.bottom() - 10.0));
        let mut button: Option<&str> = None;
        match &m.state {
            ModelState::Ready => {
                check(&p, Pos2::new(right - 12.0, line1), 12.0, GREEN());
                p.text(Pos2::new(right, line2), Align2::RIGHT_CENTER, "On this Mac", font(12.0), GREEN());
            }
            ModelState::OnFirstUse => {
                p.text(Pos2::new(right, line2), Align2::RIGHT_CENTER, "Downloads on first use", font(12.0), TEXT_FAINT());
            }
            ModelState::Downloading { done, total } => {
                let total = if *total > 0 { *total } else { m.bytes };
                let f = if total > 0 { (*done as f32 / total as f32).clamp(0.0, 1.0) } else { 0.0 };
                p.rect_filled(bar, 3.0, white(20));
                // the bar fills smoothly between progress reports
                let f = ui.ctx().animate_value_with_time(ui.scope_id().with(("setup-model", m.id)), f, 0.25);
                p.rect_filled(Rect::from_min_size(bar.min, Vec2::new(bar.width() * f, bar.height())), 3.0, BLUE());
                p.text(Pos2::new(right, line1), Align2::RIGHT_CENTER, format!("{} of {} · {:.0}%", human_bytes(*done), human_bytes(total), f * 100.0), font(12.0), TEXT_DIM());
                // the cat trots along on the front of the fill
                let now = ui.ctx().input(|i| i.time);
                let hop = ((now * 9.0).sin().abs() * 2.5) as f32;
                let cx = bar.left() + bar.width() * f;
                let cat = Rect::from_min_max(Pos2::new(cx - 6.0, bar.top() - 13.0 - hop), Pos2::new(cx + 6.0, bar.top() - 1.0 - hop));
                crate::kit::brand::draw_mascot(&p, cat, cat_color(), (0.0, 0.0));
                crate::kit::glass::animate(ui.ctx());
            }
            ModelState::Unpacking => {
                p.rect_filled(bar, 3.0, BLUE());
                p.text(Pos2::new(right, line1), Align2::RIGHT_CENTER, "Unpacking", font(12.0), TEXT_DIM());
                ui.ctx().request_repaint_after(std::time::Duration::from_millis(200));
            }
            ModelState::Failed(e) => {
                text_fit(&p, Pos2::new(tx, bar.center().y), Align2::LEFT_CENTER, e, font(12.0), RED(), right - tx);
                button = Some("Try again");
            }
            ModelState::Missing => button = Some("Download"),
        }
        if let Some(label) = button {
            let bw = button_width(&p, label, BTN_SECONDARY);
            let b = Rect::from_min_size(Pos2::new(right - bw, line1 - 13.0), Vec2::new(bw, 26.0));
            if pill_button(ui, b, label, BTN_SECONDARY, true).clicked() {
                start = Some((i, m.id));
            }
        }
    }
    if let Some((i, id)) = start {
        if !app.demo {
            app.tools[i].download_model(id);
        }
    }
}

fn check(p: &egui::Painter, c: Pos2, s: f32, col: Color32) {
    let pts = vec![c + Vec2::new(-0.45, 0.0) * s, c + Vec2::new(-0.12, 0.33) * s, c + Vec2::new(0.45, -0.32) * s];
    p.add(egui::Shape::line(pts, Stroke::new((s * 0.22).max(1.2), col)));
}

/// [`check`] drawn up to `k` (0 → nothing, 1 → whole), short stroke first.
fn check_drawn(p: &egui::Painter, c: Pos2, s: f32, col: Color32, k: f32) {
    if k <= 0.0 {
        return;
    }
    let pts = [c + Vec2::new(-0.45, 0.0) * s, c + Vec2::new(-0.12, 0.33) * s, c + Vec2::new(0.45, -0.32) * s];
    let (l0, l1) = (pts[0].distance(pts[1]), pts[1].distance(pts[2]));
    let mut d = (l0 + l1) * k.min(1.0);
    let mut out = vec![pts[0]];
    for (a, b, l) in [(pts[0], pts[1], l0), (pts[1], pts[2], l1)] {
        if d >= l {
            out.push(b);
            d -= l;
        } else {
            out.push(a + (b - a) * (d / l));
            break;
        }
    }
    p.add(egui::Shape::line(out, Stroke::new((s * 0.22).max(1.2), col)));
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
    crate::kit::glass::stroke_circle(&p, c, 18.0, Stroke::new(4.0, white(20)));
    let frac = if req.is_empty() { 1.0 } else { done as f32 / req.len() as f32 };
    let shown = ui.ctx().animate_value_with_time(ui.scope_id().with("perm-ring"), frac, 0.6);
    let full = done == req.len();
    if shown > 0.005 {
        let a0 = -std::f32::consts::FRAC_PI_2;
        p.add(egui::Shape::line(arc_points(c, 18.0, a0, a0 + std::f32::consts::TAU * shown, 48), Stroke::new(4.0, if full { GREEN() } else { BLUE() })));
    }
    // all required ones allowed: the count turns into a check and the ring
    // grows soft cat ears (it was full already when the step opened: no show)
    let full_id = ui.scope_id().with("perm-full");
    let full_at = if full {
        let at = ui.ctx().data(|d| d.get_temp::<f64>(full_id)).unwrap_or(if t < 0.1 { now - 10.0 } else { now });
        ui.ctx().data_mut(|d| d.insert_temp(full_id, at));
        Some(at)
    } else {
        ui.ctx().data_mut(|d| d.remove::<f64>(full_id));
        None
    };
    match full_at {
        Some(at) => {
            let since = (now - at) as f32;
            check(&p, c, 14.0 * pop(since / 0.35).max(0.0), GREEN());
            let z = pop((since - 0.25) / 0.4).max(0.0);
            if z > 0.0 {
                ring_ears(&p, c, z, GREEN());
            }
            if since < 0.8 {
                ui.ctx().request_repaint();
            }
        }
        None => {
            p.text(c, Align2::CENTER_CENTER, format!("{done}/{}", req.len()), mono(11.0), TEXT());
        }
    }
    p.text(Pos2::new(c.x + 36.0, c.y), Align2::LEFT_CENTER, "Permissions", bold(22.0), TEXT());
    let area = Rect::from_min_max(Pos2::new(body.left(), body.top() + 56.0), body.max);
    // rows at their smallest; past that the list scrolls (it never runs
    // under the Back / Continue bar)
    let head = if list.iter().any(|r| r.optional) { 26.0 } else { 0.0 };
    let need = list.len() as f32 * (38.0 + 6.0) + head;
    if need <= area.height() {
        perm_list(app, ui, area, &list, t, now);
    } else {
        let mut child = ui.new_child(egui::UiBuilder::new().max_rect(area));
        child.set_clip_rect(area);
        egui::ScrollArea::vertical().id_salt("setup-perms").auto_shrink([false, false]).show(&mut child, |ui| {
            let (r, _) = ui.allocate_exact_size(Vec2::new(area.width() - 10.0, need), Sense::hover());
            perm_list(app, ui, r, &list, t, now);
        });
    }
}

/// Two soft cat ears on top of the permissions ring (radius 18 round `c`),
/// grown to `z` (0 → none, 1 → full; a bit over 1 overshoots). Wide bases
/// and a rounded tip, so they read as ears, not horns.
fn ring_ears(p: &egui::Painter, c: Pos2, z: f32, col: Color32) {
    for side in [-1.0f32, 1.0] {
        let pt = |deg: f32, r: f32| {
            let a = deg.to_radians();
            c + Vec2::new(a.cos() * side, a.sin()) * r
        };
        // base sunk into the ring, tip out and up
        let (b0, b1, tip) = (pt(-150.0, 17.0), pt(-98.0, 17.0), pt(-123.0, 30.0));
        let mid = b0 + (b1 - b0) * 0.5;
        let tip = mid + (tip - mid) * z;
        // the tip's corner rounded off: a curve between points `round` from it
        let round = 8.0 * z.min(1.0);
        let (t1, t2) = (tip + (b0 - tip).normalized() * round, tip + (b1 - tip).normalized() * round);
        let mut pts = vec![b0];
        pts.extend((0..=8).map(|i| {
            let u = i as f32 / 8.0;
            let v = 1.0 - u;
            Pos2::new(v * v * t1.x + 2.0 * u * v * tip.x + u * u * t2.x, v * v * t1.y + 2.0 * u * v * tip.y + u * u * t2.y)
        }));
        pts.push(b1);
        // same winding on both sides
        if side > 0.0 {
            pts.reverse();
        }
        p.add(egui::Shape::convex_polygon(pts, col, Stroke::NONE));
    }
}

/// The permission rows (setup and Settings › General). Returns the height used.
pub fn perm_list(app: &mut App, ui: &mut Ui, area: Rect, list: &[Row], t: f32, now: f64) -> f32 {
    let p = ui.painter().clone();
    let mut y = area.top();
    let mut opt_head = false;
    // rows shrink a little when the list wouldn't fit (8 permissions + Optional)
    let n = list.len().max(1) as f32;
    let head = if list.iter().any(|r| r.optional) { 26.0 } else { 0.0 };
    let row_h = ((area.height() - head - 6.0 * n) / n).clamp(38.0, 46.0);
    let mut ask: Option<Perm> = None;
    // the reasons line up in one column, after the longest row of tool icons
    let max_tools = list.iter().map(|r| r.tools.len()).max().unwrap_or(0) as f32;
    for (k, r) in list.iter().enumerate() {
        if r.optional && !opt_head {
            opt_head = true;
            y += 6.0;
            p.text(Pos2::new(area.left() + 2.0, y), Align2::LEFT_TOP, "Optional", font(11.0), TEXT_FAINT());
            y += 20.0;
        }
        let a = ease((t - 0.05 * k as f32) / 0.4);
        let rr = Rect::from_min_size(Pos2::new(area.left(), y - (1.0 - a) * 10.0), Vec2::new(area.width(), row_h));
        let st = app.perm_watch.status(r.perm);
        let waiting = app.perm_watch.waiting(r.perm, now);
        let ok = st == Status::Allowed;
        p.rect_filled(rr, 10.0, CARD().gamma_multiply(a));
        // just allowed: the row flashes green once and its check draws itself
        let since = app.perm_watch.allowed_at.get(&r.perm).map(|t0| (now - t0) as f32).unwrap_or(10.0);
        if since < 0.8 {
            p.rect_filled(rr, 10.0, with_alpha(GREEN(), 0.2 * (1.0 - ease(since / 0.8))));
        }
        let ib = Rect::from_min_size(Pos2::new(rr.left() + 12.0, rr.center().y - 15.0), Vec2::splat(30.0));
        let tint = match st {
            Status::Allowed => GREEN(),
            Status::Denied => AMBER(),
            Status::NotAsked => TEXT_SOFT(),
        };
        p.rect_filled(ib, 8.0, white(16));
        icon(&p, r.perm.icon(), ib.shrink(7.0), tint);
        p.text(Pos2::new(ib.right() + 12.0, rr.center().y), Align2::LEFT_CENTER, r.perm.name(), bold(13.0), TEXT().gamma_multiply(a));
        // the tools that want it
        for (n, &i) in r.tools.iter().enumerate() {
            let gx = ib.right() + 190.0 + n as f32 * 22.0;
            icon(&p, app.tools[i].badge().icon, Rect::from_min_size(Pos2::new(gx, rr.center().y - 7.0), Vec2::splat(14.0)), TEXT_MUTED());
        }
        // status on the right
        let right = rr.right() - 12.0;
        // why it's asked, after the tools' icons (the status keeps its room)
        let why_x = ib.right() + 190.0 + max_tools * 22.0 + 10.0;
        let why_w = right - 210.0 - why_x;
        if why_w > 60.0 && !r.perm.why().is_empty() {
            text_fit(&p, Pos2::new(why_x, rr.center().y), Align2::LEFT_CENTER, r.perm.why(), font(12.0), TEXT_FAINT().gamma_multiply(a), why_w);
        }
        if ok {
            let c = Pos2::new(right - 13.0, rr.center().y);
            check_drawn(&p, c, 12.0, GREEN(), ease(since / 0.35));
        } else if waiting {
            let c = Pos2::new(right - 8.0, rr.center().y);
            spinner(&p, c, 6.0, now, BLUE());
            // a tool's own step (an admin prompt, a download) isn't in System Settings
            let hint = match r.perm {
                Perm::FullDisk => "Switch it on in System Settings",
                Perm::Own { .. } => "",
                _ => "System Settings",
            };
            p.text(Pos2::new(c.x - 16.0, c.y), Align2::RIGHT_CENTER, hint, font(12.0), TEXT_FAINT());
        } else {
            let err = app.perm_watch.own_err.get(&r.perm).filter(|e| !e.is_empty());
            let (label, kind) = if err.is_some() {
                ("Try again", BTN_SECONDARY)
            } else if st == Status::Denied {
                ("Open Settings", BTN_SECONDARY)
            } else {
                (r.perm.ask_label(), BTN_SECONDARY)
            };
            let w = button_width(&p, label, kind);
            let b = Rect::from_min_size(Pos2::new(right - w, rr.center().y - 13.0), Vec2::new(w, 26.0));
            if let Some(e) = err {
                text_fit(&p, Pos2::new(b.left() - 10.0, rr.center().y), Align2::RIGHT_CENTER, e, font(12.0), RED(), 260.0);
            }
            if pill_button(ui, b, label, kind, true).clicked() {
                ask = Some(r.perm);
            }
        }
        y += row_h + 6.0;
    }
    if let Some(perm) = ask {
        match perm {
            Perm::Own { id, .. } => {
                app.perm_watch.asked.insert(perm, now);
                app.perm_watch.polled = 0.0;
                if app.perm_watch.demo.is_none() {
                    if let Some(&i) = list.iter().find(|r| r.perm == perm).and_then(|r| r.tools.first()) {
                        app.tools[i].ask_own(id);
                    }
                }
            }
            _ => app.perm_watch.ask(perm, now),
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
        let f: FontId = font(13.0);
        let g = p.layout_no_wrap(c.clone(), f, TEXT());
        let w = g.size().x.max(10.0) + 12.0;
        let r = Rect::from_min_size(Pos2::new(x - w, right.y - 11.0), Vec2::new(w, 22.0));
        p.rect_filled(r, 5.0, white(26));
        p.rect_filled(Rect::from_min_max(Pos2::new(r.left() + 1.0, r.bottom() - 1.5), Pos2::new(r.right() - 1.0, r.bottom())), CornerRadius::same(2), Color32::from_black_alpha(90));
        // center the glyph's ink, not the font's line box (symbols sat low);
        // the cap's bottom edge takes 1.5 pt, so aim a hair above the middle
        let ink = g.mesh_bounds;
        let target = r.center() - Vec2::new(0.0, 0.75);
        let pos = Pos2::new(r.center().x - g.size().x / 2.0, target.y - ink.center().y);
        p.galley(pos, g, TEXT());
        x = r.left() - 3.0;
    }
}

/// The Ready step's cat, standing on `feet`: it pops in, crouches and
/// bursts the chosen tools out round it, which arc away and fall.
fn celebrate(app: &App, p: &egui::Painter, feet: Pos2, t: f32) {
    use std::f32::consts::PI;
    let size = 56.0;
    let burst = 0.5;
    let z = pop(t / 0.35).max(0.0);
    // crouch before the burst, stretch on it, then settle
    let sq = if t < burst { 0.12 * ease((t - 0.3) / (burst - 0.3)) } else { -0.08 * (1.0 - ease((t - burst) / 0.3)) };
    let (w, h) = (size * z * (1.0 + sq), size * z * (1.0 - sq));
    let cat = Rect::from_min_max(Pos2::new(feet.x - w / 2.0, feet.y - h), Pos2::new(feet.x + w / 2.0, feet.y));
    let c = Pos2::new(feet.x, feet.y - size / 2.0);
    // a ring pushed out by the burst
    let u = (t - burst) / 0.45;
    if (0.0..1.0).contains(&u) {
        crate::kit::glass::stroke_circle(p, c, 24.0 + 52.0 * ease(u), Stroke::new(2.0, with_alpha(BLUE(), 0.6 * (1.0 - u))));
    }
    let perk = if t > burst { 0.35 * (1.0 - ease((t - burst) / 0.5)) } else { 0.0 };
    if z > 0.0 {
        crate::kit::brand::draw_mascot(p, cat, cat_color(), (perk + twitch(t, 1.6), perk));
    }
    // the chosen tools fly out in a fan over the top and drop
    let on: Vec<usize> = (0..app.tools.len()).filter(|&i| app.tool_on(i)).collect();
    let n = on.len().max(1) as f32;
    let fly = (t - burst) / 1.0;
    if !(0.0..1.0).contains(&fly) {
        return;
    }
    for (k, &i) in on.iter().enumerate() {
        // from straight left over the top to straight right, a little uneven
        let a = PI + (k as f32 + 0.5) / n * PI + ((k * 7 % 5) as f32 - 2.0) * 0.06;
        let dist = 120.0 + ((k * 37) % 5) as f32 * 18.0;
        let out = ease((fly * 1.8).min(1.0));
        let at = c + Vec2::new(a.cos(), a.sin()) * dist * out + Vec2::new(0.0, 140.0 * fly * fly);
        let s = pop(fly / 0.18).max(0.0);
        let fade = 1.0 - ((fly - 0.55) / 0.45).clamp(0.0, 1.0);
        let lime = k % 2 == 0;
        let r = 12.0 * s;
        p.circle_filled(at, r, (if lime { BLUE() } else { POPOVER() }).gamma_multiply(fade));
        if !lime {
            crate::kit::glass::stroke_circle(p, at, r, Stroke::new(1.0, white(36).gamma_multiply(fade)));
        }
        icon(p, app.tools[i].badge().icon, Rect::from_center_size(at, Vec2::splat(12.0 * s)), (if lime { ON_ACTION } else { TEXT_SOFT() }).gamma_multiply(fade));
    }
}

/// Returns true when "Restart" next to Screen Recording was clicked.
fn ready(app: &mut App, ui: &mut Ui, body: Rect, t: f32) -> bool {
    let mut restart = false;
    let p = ui.painter().clone();
    let left = Rect::from_min_max(body.min, Pos2::new(body.right() - 340.0, body.bottom()));
    let mut y = body.center().y - 120.0;
    p.text(Pos2::new(left.left(), y), Align2::LEFT_TOP, "Ready", bold(22.0), TEXT());
    celebrate(app, &p, Pos2::new(left.left() + 28.0, y - 14.0), t);
    y += 46.0;
    // open at login
    let row = Rect::from_min_size(Pos2::new(left.left(), y), Vec2::new(left.width(), 52.0));
    p.rect_filled(row, 10.0, CARD());
    icon(&p, Icon::Power, Rect::from_min_size(Pos2::new(row.left() + 14.0, row.center().y - 9.0), Vec2::splat(18.0)), TEXT_SOFT());
    p.text(Pos2::new(row.left() + 44.0, row.center().y), Align2::LEFT_CENTER, "Open at login", bold(13.0), TEXT());
    let now = ui.ctx().input(|i| i.time);
    // on by default; registered when setup finishes
    let on = crate::kit::login::default_pending() || app.perm_watch.login_on(app.demo, now);
    let sw = Rect::from_min_size(Pos2::new(row.right() - 50.0, row.center().y - 11.0), Vec2::new(38.0, 22.0));
    if switch(ui, sw, ui.scope_id().with("setup-login"), on, true).clicked() && !app.demo {
        app.perm_watch.set_login(!on);
    }
    y += 62.0;
    // shortcuts of the enabled tools
    let keys: Vec<(Icon, &'static str, String)> = (0..app.tools.len()).filter(|&i| app.tool_on(i)).flat_map(|i| {
        let ic = app.tools[i].badge().icon;
        app.tools[i].setup_shortcuts().into_iter().map(move |(l, k)| (if l.starts_with("Record") { Icon::ScreenRec } else { ic }, l, k))
    }).collect();
    for (k, (ic, label, keys)) in keys.iter().enumerate() {
        let a = ease((t - 0.08 * (k + 1) as f32) / 0.4);
        let row = Rect::from_min_size(Pos2::new(left.left(), y - (1.0 - a) * 8.0), Vec2::new(left.width(), 44.0));
        p.rect_filled(row, 10.0, CARD().gamma_multiply(a));
        icon(&p, *ic, Rect::from_min_size(Pos2::new(row.left() + 14.0, row.center().y - 9.0), Vec2::splat(18.0)), TEXT_SOFT().gamma_multiply(a));
        p.text(Pos2::new(row.left() + 44.0, row.center().y), Align2::LEFT_CENTER, *label, font(13.0), TEXT().gamma_multiply(a));
        keycaps(&p, Pos2::new(row.right() - 12.0, row.center().y), keys);
        y += 52.0;
    }
    if needs_restart(app) {
        let row = Rect::from_min_size(Pos2::new(left.left(), y), Vec2::new(left.width(), 44.0));
        p.rect_filled(row, 10.0, CARD());
        icon(&p, Icon::ScreenRec, Rect::from_min_size(Pos2::new(row.left() + 14.0, row.center().y - 9.0), Vec2::splat(18.0)), AMBER());
        p.text(Pos2::new(row.left() + 44.0, row.center().y), Align2::LEFT_CENTER, "Screen Recording needs a restart", font(13.0), TEXT());
        let bw = button_width(&p, "Restart", BTN_PRIMARY);
        let br = Rect::from_min_size(Pos2::new(row.right() - 10.0 - bw, row.center().y - 13.0), Vec2::new(bw, 26.0));
        if pill_button(ui, br, "Restart", BTN_PRIMARY, true).clicked() {
            restart = true;
        }
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
    let top = body.center().y - ph / 2.0 + 24.0;
    // a strip of the menu bar with the app icon lit, the panel hanging under it
    let bar = Rect::from_min_size(Pos2::new(px, top - 40.0), Vec2::new(pw, 24.0));
    p.rect_filled(bar, 6.0, white(18));
    let mut bx = bar.right() - 12.0;
    let clock = p.text(Pos2::new(bx, bar.center().y), Align2::RIGHT_CENTER, "Tue 17:58", font(11.0), TEXT_FAINT());
    bx = clock.left() - 14.0;
    for ic in [Icon::Wifi, Icon::Battery(80)] {
        icon(&p, ic, Rect::from_center_size(Pos2::new(bx - 7.0, bar.center().y), Vec2::splat(13.0)), TEXT_FAINT());
        bx -= 24.0;
    }
    let me = Rect::from_center_size(Pos2::new(bx - 8.0, bar.center().y), Vec2::new(26.0, 20.0));
    let glow = ((t * 3.0).sin() * 0.5 + 0.5) * 0.25 + 0.2;
    p.rect_filled(me, 5.0, with_alpha(BLUE(), glow));
    brand_glyph(&p, me.center(), 15.0, Color32::WHITE);
    p.text(Pos2::new(bar.left() + 10.0, bar.center().y), Align2::LEFT_CENTER, "Click this icon", font(11.0), TEXT_FAINT());
    // a little caret from the icon to the panel
    let c = Pos2::new(me.center().x, top);
    p.add(egui::Shape::convex_polygon(vec![c + Vec2::new(-6.0, 0.5), c + Vec2::new(0.0, -6.0), c + Vec2::new(6.0, 0.5)], BG(), Stroke::NONE));
    let panel = Rect::from_min_size(Pos2::new(px, top), Vec2::new(pw, ph));
    p.rect_filled(panel, 12.0, BG());
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
        let blue = false;
        p.rect_filled(r, 9.0, (if blue { BLUE() } else { SIDEBAR() }).gamma_multiply(a.min(1.0)));
        let fg = if blue { ON_ACTION } else { TEXT_SOFT() };
        // the whole name (R10): on two lines when one is too narrow, the icon moves up
        let name = app.tools[i].name();
        let two = p.layout_no_wrap(name.to_string(), font(11.0), fg).size().x > r.width() - 8.0;
        let icon_up = if two { 14.0 } else { 8.0 };
        icon(&p, app.tools[i].badge().icon, Rect::from_center_size(r.center() - Vec2::new(0.0, icon_up), Vec2::splat(18.0 * a)), fg);
        text_wrap(&p, Pos2::new(r.center().x, r.bottom() - if two { 5.0 } else { 9.0 }), Align2::CENTER_BOTTOM, name, font(11.0), fg.gamma_multiply(0.85 * a.min(1.0)), r.width() - 8.0, 2);
    }
    restart
}

// ---------------------------------------------------------------- System Settings helper

const HELP_W: f32 = 360.0;
const HELP_H: f32 = 132.0;
/// The permissions switched on by hand in a System Settings list.
const HELPED: [Perm; 3] = [Perm::FullDisk, Perm::Accessibility, Perm::Screen];

/// While one of [`HELPED`] is being set up, a small card floats on top in
/// the middle of the screen showing what to do: find the app in the list
/// and switch it on.
pub fn perm_helper(app: &mut App, ctx: &egui::Context) {
    let now = ctx.input(|i| i.time);
    let Some(perm) = HELPED.into_iter().find(|&p| app.perm_watch.waiting(p, now) && (app.demo || app.perm_watch.status(p) != Status::Allowed)) else {
        return;
    };
    let screen = ctx.input(|i| i.viewport().monitor_size).unwrap_or(Vec2::new(1512.0, 982.0));
    let builder = egui::ViewportBuilder::default()
        .with_title(perm.name())
        .with_inner_size([HELP_W, HELP_H])
        .with_position([(screen.x - HELP_W) / 2.0, (screen.y - HELP_H) / 2.0])
        .with_decorations(false)
        .with_transparent(true)
        .with_resizable(false)
        .with_has_shadow(true)
        .with_window_level(egui::WindowLevel::AlwaysOnTop);
    let id = egui::ViewportId::from_hash_of("cmdmeow-fda-help");
    let mut close = false;
    crate::kit::viewport::show(ctx, id, builder, |ui, _class| {
        close = help_card(ui, ui.max_rect(), now, perm);
    });
    if close {
        app.perm_watch.asked.remove(&perm);
    }
}

/// The helper's card. Returns true when its close button was clicked.
pub fn help_card(ui: &mut Ui, full: Rect, now: f64, perm: Perm) -> bool {
    let p = ui.painter().clone();
    p.rect_filled(full, 12.0, BG());
    p.rect_stroke(full, 12.0, Stroke::new(1.0, white(40)), StrokeKind::Inside);
    let name = perms::app_name();
    // a copy of the Settings row, its switch flipping on and off
    let row = Rect::from_min_size(full.min + Vec2::new(14.0, 14.0), Vec2::new(full.width() - 28.0, 40.0));
    p.rect_filled(row, 8.0, white(14));
    app_mark(&p, Pos2::new(row.left() + 20.0, row.center().y), 22.0, 0.75);
    p.text(Pos2::new(row.left() + 40.0, row.center().y), Align2::LEFT_CENTER, &name, font(13.0), TEXT());
    let cycle = (now % 2.4) as f32 / 2.4;
    let on = ease(((cycle - 0.25) / 0.25).clamp(0.0, 1.0)) * (1.0 - ease(((cycle - 0.85) / 0.15).clamp(0.0, 1.0)));
    let sw = Rect::from_min_size(Pos2::new(row.right() - 50.0, row.center().y - 11.0), Vec2::new(38.0, 22.0));
    p.rect_filled(sw, 11.0, lerp_color(white(46), BLUE(), on));
    p.circle_filled(Pos2::new(sw.left() + 11.0 + 16.0 * on, sw.center().y), 9.0, Color32::WHITE);
    // a pointer that clicks the switch
    let tip = sw.center() + Vec2::new(4.0, 6.0) + Vec2::new(14.0, 12.0) * (1.0 - ease((cycle / 0.25).clamp(0.0, 1.0)));
    let arrow = vec![tip, tip + Vec2::new(0.0, 13.0), tip + Vec2::new(3.5, 9.5), tip + Vec2::new(8.5, 10.0)];
    p.add(egui::Shape::convex_polygon(arrow, Color32::WHITE, Stroke::new(1.0, Color32::BLACK)));
    let y = row.bottom() + 14.0;
    p.text(Pos2::new(full.left() + 16.0, y), Align2::LEFT_TOP, format!("Switch on {name} in the list"), bold(13.0), TEXT());
    let more = match perm {
        Perm::FullDisk => "Not there? Drag it in from the Finder window.",
        _ => "If macOS asks first, pick Open System Settings.",
    };
    p.text(Pos2::new(full.left() + 16.0, y + 20.0), Align2::LEFT_TOP, more, font(12.0), TEXT_FAINT());
    let x = Rect::from_min_size(Pos2::new(full.right() - 26.0, full.bottom() - 26.0), Vec2::splat(18.0));
    let r = ui.interact(x, ui.scope_id().with("fda-help-close"), Sense::click());
    icon(&p, Icon::Close, x.shrink(3.0), if r.hovered() { TEXT() } else { TEXT_MUTED() });
    ui.ctx().request_repaint();
    r.clicked()
}


#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn welcome_cat_rises_smoothly_and_comes_to_rest() {
        let (rise, _, _) = peek(0.0);
        assert!(rise <= -96.0, "hidden below the floor at first: {rise}");
        let (rise, _, _) = peek(0.6);
        assert!((-80.0..-60.0).contains(&rise), "only the ears show while it looks round: {rise}");
        assert_eq!(peek(2.0), (0.0, 1.0, 1.0), "at rest, unsquashed");
        let mut last = peek(0.0).0;
        for i in 1..300 {
            let r = peek(i as f32 * 0.01).0;
            assert!((r - last).abs() < 15.0, "a jump of {} at {:.2}s", r - last, i as f32 * 0.01);
            last = r;
        }
    }

    #[test]
    fn ear_flick_is_short_and_small() {
        assert_eq!(twitch(0.9, 1.0), 0.0);
        assert_eq!(twitch(1.4, 1.0), 0.0);
        let most = (0..36).map(|i| twitch(1.0 + i as f32 * 0.01, 1.0)).fold(0.0f32, f32::max);
        assert!(most > 0.1 && most <= 0.3, "{most}");
    }
}
