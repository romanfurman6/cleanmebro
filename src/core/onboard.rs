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
    if !models(app).is_empty() {
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
}

impl Setup {
    pub fn new(step: usize) -> Self {
        Setup { step, since: f64::NEG_INFINITY }
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
    if let Some(s) = app.setup.as_mut() {
        if s.since == f64::NEG_INFINITY {
            s.since = now;
        }
    }
    let t = (now - app.setup.as_ref().map(|s| s.since).unwrap_or(now)) as f32;
    // setup lives in the app window: a centered column when that is wider
    let win = full;
    let full = Rect::from_center_size(win.center(), Vec2::new(win.width().min(SETUP_W + 80.0), win.height()));
    // drag the window by its top strip
    let drag = Rect::from_min_size(win.min + Vec2::new(80.0, 0.0), Vec2::new(win.width() - 80.0, TOP_H));
    if ui.interact(drag, ui.scope_id().with("setup-drag"), Sense::click_and_drag()).drag_started() {
        ctx.send_viewport_cmd(egui::ViewportCommand::StartDrag);
    }
    // step dots
    let dw = |i: usize| if i == step { 22.0 } else { 6.0 };
    let total: f32 = (0..steps).map(dw).sum::<f32>() + 6.0 * (steps as f32 - 1.0);
    let mut x = full.center().x - total / 2.0;
    for i in 0..steps {
        let w = egui::lerp(6.0..=dw(i), ease(t * 3.0).max(if i == step { 0.0 } else { 1.0 }));
        let r = Rect::from_min_size(Pos2::new(x, full.top() + TOP_H / 2.0 - 3.0), Vec2::new(w, 6.0));
        p.rect_filled(r, 3.0, if i <= step { BLUE() } else { white(46) });
        x += w + 6.0;
    }
    let body = Rect::from_min_max(Pos2::new(full.left() + 40.0, full.top() + TOP_H), Pos2::new(full.right() - 40.0, full.bottom() - BOTTOM_H));
    // the step slides in from the right
    let slide = (1.0 - ease(t * 2.2)) * 18.0;
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
    // the ring draws itself, then the tools land on it one by one
    let ring = 86.0;
    crate::kit::glass::stroke_circle(&p, c, ring, Stroke::new(2.0, white(14)));
    let start = -std::f32::consts::FRAC_PI_2;
    let k = ease(t / 1.4);
    if k > 0.0 {
        p.add(egui::Shape::line(arc_points(c, ring, start, start + std::f32::consts::TAU * k, 96), Stroke::new(2.0, BLUE())));
    }
    let n = app.tools.len().max(1);
    for (i, tool) in app.tools.iter().enumerate() {
        let a = start + i as f32 / n as f32 * std::f32::consts::TAU;
        let at = c + Vec2::new(a.cos(), a.sin()) * ring;
        let s = pop((t - 0.2 - 0.12 * i as f32) / 0.45);
        if s <= 0.0 {
            continue;
        }
        p.circle_filled(at, 15.0 * s, BG());
        crate::kit::glass::stroke_circle(&p, at, 15.0 * s, Stroke::new(1.0, white(36)));
        icon(&p, tool.badge().icon, Rect::from_center_size(at, Vec2::splat(14.0 * s)), TEXT_SOFT());
    }
    let bob = if t > 1.8 { ((t - 1.8) * 1.6).sin() * 3.0 } else { 0.0 };
    app_mark(&p, c + Vec2::new(0.0, bob), 96.0, ease((t - 0.3) / 1.2));
    let name = p.text(Pos2::new(c.x, c.y + 118.0), Align2::CENTER_TOP, "Cmd+Meow", bold(28.0), TEXT());
    p.text(Pos2::new(c.x, name.bottom() + sp::LINE_GAP), Align2::CENTER_TOP, format!("{} tools. One menu bar.", app.tools.len()), font(13.0), TEXT_FAINT());
    let r = Rect::from_center_size(Pos2::new(c.x, c.y + 214.0), Vec2::new(128.0, 40.0));
    pill_button(ui, r, "Set up", BTN_PRIMARY, true).clicked()
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
    p.text(Pos2::new(g.right() + 10.0, g.bottom() - 4.0), Align2::LEFT_BOTTOM, format!("{on}/{}", app.tools.len()), mono(13.0), TEXT_FAINT());
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
            let ib = Rect::from_min_size(Pos2::new(r.left() + 12.0, r.center().y - 17.0), Vec2::splat(34.0));
            p.rect_filled(ib, 9.0, lerp_color(white(16), BLUE(), s));
            let b = app.tools[i].badge();
            icon(&p, b.icon, ib.shrink(8.0), lerp_color(TEXT_MUTED(), ON_ACTION, s));
            let tx = ib.right() + 10.0;
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
    type Change = Box<dyn FnOnce(&mut settings::Settings)>;
    let mut change: Option<Change> = None;
    for (k, m) in ai::PROVIDERS.iter().enumerate() {
        let r = Rect::from_min_size(Pos2::new(card.left(), card.top() + k as f32 * sp::ROW_H), Vec2::new(w, sp::ROW_H));
        if k > 0 {
            hairline(&p, Pos2::new(r.left() + sp::L, r.top()), Pos2::new(r.right() - sp::L, r.top()));
        }
        let found = app.demo || ai::make(k).data_dir().is_some_and(|d| d.exists());
        let name = p.text(Pos2::new(r.left() + sp::L, r.center().y), Align2::LEFT_CENTER, m.name, ty::title(), TEXT());
        p.text(Pos2::new(name.right() + sp::S, r.center().y), Align2::LEFT_CENTER, if found { "Found" } else { "Not found" }, ty::label(), if found { GREEN() } else { TEXT_FAINT() });
        let main = set.main == m.id;
        let on = set.is_on(m.id);
        let rr = Rect::from_center_size(Pos2::new(col_main, r.center().y), Vec2::splat(16.0));
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

/// (tool index, model) for every enabled tool's downloads.
fn models(app: &App) -> Vec<(usize, crate::core::tool::SetupModel)> {
    (0..app.tools.len()).filter(|&i| app.tool_on(i)).flat_map(|i| app.tools[i].setup_models().into_iter().map(move |m| (i, m))).collect()
}

fn models_step(app: &mut App, ui: &mut Ui, body: Rect, t: f32) {
    use crate::core::tool::ModelState;
    let p = ui.painter().clone();
    let head = body.top() + 6.0;
    let g = p.text(Pos2::new(body.left(), head), Align2::LEFT_TOP, "Download models", bold(22.0), TEXT());
    let sub = text_wrap(&p, Pos2::new(body.left(), g.bottom() + sp::LINE_GAP), Align2::LEFT_TOP, "They run on this Mac. The download keeps going if you continue.", ty::body(), TEXT_DIM(), body.width().min(560.0), 3);
    let list = models(app);
    let w = body.width().min(620.0);
    let row_h = 76.0;
    let top = sub.bottom() + sp::XL;
    let mut start: Option<(usize, &'static str)> = None;
    for (k, (i, m)) in list.iter().enumerate() {
        let a = ease((t - 0.05 * k as f32) / 0.4);
        let r = Rect::from_min_size(Pos2::new(body.left(), top + k as f32 * (row_h + 8.0) + (1.0 - a) * 8.0), Vec2::new(w, row_h));
        glass_card(&p, r, R_CARD, 0.0, false);
        let ib = Rect::from_min_size(Pos2::new(r.left() + 14.0, r.top() + 14.0), Vec2::splat(30.0));
        p.rect_filled(ib, 8.0, white(16));
        icon(&p, app.tools[*i].badge().icon, ib.shrink(7.0), if m.state == ModelState::Ready { GREEN() } else { TEXT_SOFT() });
        let tx = ib.right() + 12.0;
        let title = p.text(Pos2::new(tx, ib.center().y), Align2::LEFT_CENTER, m.what, bold(13.0), TEXT());
        p.text(Pos2::new(title.right() + 8.0, ib.center().y), Align2::LEFT_CENTER, format!("{} · {}", m.name, human_bytes(m.bytes)), font(12.0), TEXT_FAINT());
        let right = r.right() - 14.0;
        // second line: the progress bar, or what happened
        let bar = Rect::from_min_max(Pos2::new(tx, r.bottom() - 22.0), Pos2::new(right, r.bottom() - 16.0));
        let mut button: Option<&str> = None;
        match &m.state {
            ModelState::Ready => {
                check(&p, Pos2::new(right - 12.0, ib.center().y), 12.0, GREEN());
                p.text(Pos2::new(tx, bar.center().y), Align2::LEFT_CENTER, "Downloaded", font(12.0), GREEN());
            }
            ModelState::Downloading { done, total } => {
                let total = if *total > 0 { *total } else { m.bytes };
                let f = if total > 0 { (*done as f32 / total as f32).clamp(0.0, 1.0) } else { 0.0 };
                p.rect_filled(bar, 3.0, white(20));
                p.rect_filled(Rect::from_min_size(bar.min, Vec2::new(bar.width() * f, bar.height())), 3.0, BLUE());
                p.text(Pos2::new(right, ib.center().y), Align2::RIGHT_CENTER, format!("{} of {} · {:.0}%", human_bytes(*done), human_bytes(total), f * 100.0), font(12.0), TEXT_DIM());
                ui.ctx().request_repaint_after(std::time::Duration::from_millis(200));
            }
            ModelState::Unpacking => {
                p.rect_filled(bar, 3.0, BLUE());
                p.text(Pos2::new(right, ib.center().y), Align2::RIGHT_CENTER, "Unpacking", font(12.0), TEXT_DIM());
                ui.ctx().request_repaint_after(std::time::Duration::from_millis(200));
            }
            ModelState::Failed(e) => {
                text_fit(&p, Pos2::new(tx, bar.center().y), Align2::LEFT_CENTER, e, font(12.0), RED(), w - 140.0);
                button = Some("Try again");
            }
            ModelState::Missing => button = Some("Download"),
        }
        if let Some(label) = button {
            let bw = button_width(&p, label, BTN_SECONDARY);
            let b = Rect::from_min_size(Pos2::new(right - bw, ib.center().y - 13.0), Vec2::new(bw, 26.0));
            if pill_button(ui, b, label, BTN_SECONDARY, true).clicked() {
                start = Some((*i, m.id));
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
    p.text(c, Align2::CENTER_CENTER, format!("{done}/{}", req.len()), mono(11.0), TEXT());
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
        let mut gx = ib.right() + 190.0;
        for &i in &r.tools {
            icon(&p, app.tools[i].badge().icon, Rect::from_min_size(Pos2::new(gx, rr.center().y - 7.0), Vec2::splat(14.0)), TEXT_MUTED());
            gx += 22.0;
        }
        // status on the right
        let right = rr.right() - 12.0;
        if ok {
            let since = app.perm_watch.allowed_at.get(&r.perm).map(|t0| (now - t0) as f32).unwrap_or(1.0);
            let z = pop(since / 0.45);
            let c = Pos2::new(right - 13.0, rr.center().y);
            check(&p, c, 12.0 * z, GREEN());
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

/// Returns true when "Restart" next to Screen Recording was clicked.
fn ready(app: &mut App, ui: &mut Ui, body: Rect, t: f32) -> bool {
    let mut restart = false;
    let p = ui.painter().clone();
    let left = Rect::from_min_max(body.min, Pos2::new(body.right() - 340.0, body.bottom()));
    let mut y = body.center().y - 120.0;
    p.text(Pos2::new(left.left(), y), Align2::LEFT_TOP, "Ready", bold(22.0), TEXT());
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

// ---------------------------------------------------------------- Full Disk Access helper

const HELP_W: f32 = 360.0;
const HELP_H: f32 = 132.0;

/// While Full Disk Access is being set up, a small card floats on top of
/// System Settings showing what to do: find the app in the list and switch
/// it on (or drag it in from Finder when it isn't listed).
pub fn fda_helper(app: &mut App, ctx: &egui::Context) {
    let now = ctx.input(|i| i.time);
    let show = app.perm_watch.waiting(Perm::FullDisk, now) && (app.demo || app.perm_watch.status(Perm::FullDisk) != Status::Allowed);
    if !show {
        return;
    }
    let screen = ctx.input(|i| i.viewport().monitor_size).unwrap_or(Vec2::new(1512.0, 982.0));
    let builder = egui::ViewportBuilder::default()
        .with_title("Full Disk Access")
        .with_inner_size([HELP_W, HELP_H])
        .with_position([screen.x - HELP_W - 24.0, screen.y - HELP_H - 90.0])
        .with_decorations(false)
        .with_transparent(true)
        .with_resizable(false)
        .with_has_shadow(true)
        .with_window_level(egui::WindowLevel::AlwaysOnTop);
    let id = egui::ViewportId::from_hash_of("cmdmeow-fda-help");
    let mut close = false;
    crate::kit::viewport::show(ctx, id, builder, |ui, _class| {
        close = help_card(ui, ui.max_rect(), now);
    });
    if close {
        app.perm_watch.asked.remove(&Perm::FullDisk);
    }
}

/// The helper's card. Returns true when its close button was clicked.
pub fn help_card(ui: &mut Ui, full: Rect, now: f64) -> bool {
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
    p.text(Pos2::new(full.left() + 16.0, y + 20.0), Align2::LEFT_TOP, "Not there? Drag it in from the Finder window.", font(12.0), TEXT_FAINT());
    let x = Rect::from_min_size(Pos2::new(full.right() - 26.0, full.bottom() - 26.0), Vec2::splat(18.0));
    let r = ui.interact(x, ui.scope_id().with("fda-help-close"), Sense::click());
    icon(&p, Icon::Close, x.shrink(3.0), if r.hovered() { TEXT() } else { TEXT_MUTED() });
    ui.ctx().request_repaint();
    r.clicked()
}

