//! Disk Map screens: choose what to map (with live progress while it runs)
//! → bubbles you can zoom into folder by folder, with a list to pick from → review and move to Trash.

use super::scan::{self, Node};
use super::Lens as App;
use crate::core::tool::Cx;
use crate::kit::glass::*;
use egui::{Align2, Color32, CornerRadius, Pos2, Rect, Sense, Stroke, StrokeKind, Ui, Vec2};
use std::path::{Path, PathBuf};
use std::sync::atomic::Ordering;

const HEADER_H: f32 = 57.0;
const INFO_H: f32 = 28.0;

pub fn page(app: &mut App, ui: &mut Ui, r: Rect, cx: &mut Cx) {
    let shared = app.lens.clone();
    let now = ui.ctx().input(|i| i.time);
    if shared.running.load(Ordering::SeqCst) {
        // while mapping: a calm list of folders that grows in place; the
        // treemap is drawn once, when the sizes are final
        if app.lens_live.as_ref().is_none_or(|l| now - l.0 > 0.25) {
            app.lens_live = scan::partial(&shared).map(|(root, rows)| (now, root, rows));
        }
        match app.lens_live.take() {
            Some(live) => {
                let act = mapping(app, ui, r, &live.1, &live.2);
                app.lens_live = Some(live);
                if act == Act::Stop {
                    shared.stop.store(true, Ordering::SeqCst);
                }
            }
            None => start(app, ui, r),
        }
        return;
    }
    if app.lens_live.is_some() || !app.lens_order.is_empty() {
        // mapping just finished: the treemap starts fresh, no glide from the list
        app.lens_order.clear();
        app.lens_rows.clear();
    }
    app.lens_live = None;
    let guard = shared.result.lock().unwrap();
    let Some(root) = guard.as_ref() else {
        drop(guard);
        // show the last map if there is one (even from a previous run);
        // map the disk only the very first time
        scan::load(&shared, ui.ctx());
        if shared.loading.load(Ordering::SeqCst) {
            page_header(&ui.painter().clone(), r, "Disk Map", "Opening the last map…", TEXT_DIM());
            return;
        }
        // nothing mapped yet: mapping walks the whole disk, so it waits
        // for a click
        if shared.result.lock().unwrap().is_none() && intro(ui, r) {
            begin(app, ui.ctx());
        }
        return;
    };
    let act = results(app, ui, r, root, None);
    drop(guard);
    if act == Act::NewMap {
        *shared.result.lock().unwrap() = None;
        app.lens_selected.clear();
        app.lens_morph = None;
        begin(app, ui.ctx());
    } else if app.lens_review {
        review(app, ui, r, cx);
    }
}

#[derive(PartialEq)]
enum Act {
    None,
    NewMap,
    Stop,
}

fn home() -> PathBuf {
    dirs::home_dir().unwrap_or_else(|| PathBuf::from("/"))
}

fn tilde(p: &Path) -> String {
    let h = home().display().to_string();
    let s = p.display().to_string();
    if s == h {
        "~".into()
    } else {
        s.replacen(&(h + "/"), "~/", 1)
    }
}

// ---------------------------------------------------------------- start

/// Before the first map: what it does, and the button that starts it.
fn intro(ui: &mut Ui, r: Rect) -> bool {
    let p = ui.painter().clone();
    page_header(&p, r, "Disk Map", "Macintosh HD", TEXT_DIM());
    let w = (r.width() - 48.0).min(520.0);
    let pr = Rect::from_center_size(Pos2::new(r.center().x, r.top() + PAGE_HEADER_H + 28.0 + 50.0), Vec2::new(w, 100.0));
    glass_card(&p, pr, R_CARD, 0.0, false);
    p.text(Pos2::new(pr.left() + 16.0, pr.top() + 14.0), Align2::LEFT_TOP, "Map Macintosh HD", bold(15.0), TEXT());
    let bw = button_width(&p, "Map disk", BTN_PRIMARY) + 28.0;
    pill_button(ui, Rect::from_min_size(Pos2::new(pr.left() + 16.0, pr.bottom() - 48.0), Vec2::new(bw, 32.0)), "Map disk", BTN_PRIMARY, true).clicked()
}

/// The first moments of a map, before the disk's top folders are listed:
/// what's being measured, with Stop. There is nothing to choose: Disk Map
/// always maps the whole startup disk.
fn start(app: &mut App, ui: &mut Ui, r: Rect) {
    let ctx = ui.ctx().clone();
    let p = ui.painter().clone();
    page_header(&p, r, "Disk Map", "Macintosh HD", TEXT_DIM());
    let w = (r.width() - 48.0).min(520.0);
    let pr = Rect::from_center_size(Pos2::new(r.center().x, r.top() + PAGE_HEADER_H + 28.0 + 90.0), Vec2::new(w, 180.0));
    glass_card(&p, pr, R_CARD, 0.0, false);
    let bytes = app.lens.bytes.load(Ordering::Relaxed);
    let files = app.lens.files.load(Ordering::Relaxed);
    p.text(Pos2::new(pr.left() + 16.0, pr.top() + 14.0), Align2::LEFT_TOP, "Mapping Macintosh HD", bold(13.0), TEXT_DIM());
    let g = p.layout_no_wrap(human_bytes(bytes), bold(26.0), TEXT());
    let gw = g.size().x;
    p.galley(Pos2::new(pr.left() + 16.0, pr.top() + 36.0), g, TEXT());
    p.text(Pos2::new(pr.left() + 24.0 + gw, pr.top() + 55.0), Align2::LEFT_CENTER, format!("measured · {} files", group_digits(files)), font(13.0), TEXT_DIM());
    let track = Rect::from_min_size(Pos2::new(pr.left() + 16.0, pr.top() + 84.0), Vec2::new(pr.width() - 32.0, 6.0));
    p.rect_filled(track, 3.0, white(20));
    let t = ctx.input(|i| i.time);
    let f = ((t * 0.6).fract() as f32) * 1.35 - 0.35;
    let seg = Rect::from_min_max(Pos2::new(track.left() + track.width() * f.max(0.0), track.top()), Pos2::new(track.left() + track.width() * (f + 0.35).min(1.0), track.bottom()));
    p.with_clip_rect(track).rect_filled(seg, 3.0, tint::DISK_MAP());
    let cur = app.lens.current.lock().unwrap().clone();
    text_fit(&p, Pos2::new(pr.left() + 16.0, pr.top() + 102.0), Align2::LEFT_TOP, &tilde(Path::new(&cur)), mono(11.0), TEXT_FAINT(), pr.width() - 32.0);
    let sw = button_width(&p, "Stop", BTN_SECONDARY);
    if pill_button(ui, Rect::from_min_size(Pos2::new(pr.left() + 16.0, pr.bottom() - 44.0), Vec2::new(sw, 28.0)), "Stop", BTN_SECONDARY, true).clicked() {
        app.lens.stop.store(true, Ordering::SeqCst);
    }
    paragraph(&p, Pos2::new(pr.left(), pr.bottom() + 14.0), "Allow Full Disk Access for complete results", font(12.0), TEXT_FAINT(), pr.width());
    crate::kit::glass::animate(&ctx);
}

/// Map the whole startup disk from scratch.
fn begin(app: &mut App, ctx: &egui::Context) {
    app.lens_root = PathBuf::from("/");
    app.lens_path = app.lens_root.clone();
    app.lens_selected.clear();
    app.lens_msg = None;
    app.lens_morph = None;
    app.lens_hues.clear();
    app.lens_skip = 0;
    scan::start(app.lens.clone(), app.lens_root.clone(), ctx.clone());
}

/// "just now", "5 min ago", "3 h ago", "2 days ago".
fn ago(when: i64) -> String {
    let s = (chrono::Utc::now().timestamp() - when).max(0);
    match s {
        0..60 => "just now".into(),
        60..3600 => format!("{} min ago", s / 60),
        3600..86400 => format!("{} h ago", s / 3600),
        _ if s < 2 * 86400 => "yesterday".into(),
        _ => format!("{} days ago", s / 86400),
    }
}

fn group_digits(n: u64) -> String {
    let s = n.to_string();
    let mut out = String::new();
    for (i, ch) in s.chars().enumerate() {
        if i > 0 && (s.len() - i) % 3 == 0 {
            out.push(',');
        }
        out.push(ch);
    }
    out
}

// ---------------------------------------------------------------- results

/// Colour of the top-level folder that holds `path`. Folders get their
/// colour the first time they're drawn and keep it while the map changes.
fn hue_of(app: &mut App, root: &Path, path: &Path) -> Color32 {
    let Some(first) = path.strip_prefix(root).ok().and_then(|r| r.components().next()) else { return PURPLE() };
    let top = root.join(first);
    let i = match app.lens_hues.iter().position(|q| *q == top) {
        Some(i) => i,
        None => {
            app.lens_hues.push(top);
            app.lens_hues.len() - 1
        }
    };
    junk_ramp(i % 7)
}

fn pct(part: u64, whole: u64) -> u64 {
    if whole == 0 {
        0
    } else {
        ((part as f64 / whole as f64) * 100.0).round() as u64
    }
}

fn lerp_rect(a: Rect, b: Rect, t: f32) -> Rect {
    Rect::from_min_max(a.min + (b.min - a.min) * t, a.max + (b.max - a.max) * t)
}

/// The screen while mapping: the root's folders as rows with a size bar,
/// biggest first. Rows are re-sorted at most once a second and glide to
/// their new places; bars grow smoothly. Nothing jumps.
fn mapping(app: &mut App, ui: &mut Ui, r: Rect, root: &Path, rows: &[scan::LiveRow]) -> Act {
    let ctx = ui.ctx().clone();
    let p = ui.painter().clone();
    let now = ctx.input(|i| i.time);
    let mut act = Act::None;
    let root_label = if root == home() { "Home".to_string() } else { scan::display_name(root) };
    let measured = app.lens.bytes.load(Ordering::Relaxed);

    // header: what's mapped and how much so far; progress and Stop
    let hr = Rect::from_min_size(r.min, Vec2::new(r.width(), HEADER_H));
    hairline(&p, hr.left_bottom(), hr.right_bottom());
    let g = p.layout_no_wrap(root_label, bold(13.0), TEXT());
    let tw = g.size().x;
    p.galley(Pos2::new(hr.left() + 24.0, hr.center().y - g.size().y / 2.0), g, TEXT());
    p.text(Pos2::new(hr.left() + 32.0 + tw, hr.center().y), Align2::LEFT_CENTER, human_bytes(measured), font(13.0), TEXT_FAINT());
    let nb = button_width(&p, "Stop", BTN_SECONDARY);
    let stop_r = Rect::from_min_size(Pos2::new(hr.right() - 20.0 - nb, hr.center().y - 15.0), Vec2::new(nb, 30.0));
    let done = rows.iter().filter(|x| x.done).count();
    let hint_text = format!("Mapping · {} of {} folders done · {} files", done, rows.len(), group_digits(app.lens.files.load(Ordering::Relaxed)));
    let hint = p.text(Pos2::new(stop_r.left() - 14.0, hr.center().y), Align2::RIGHT_CENTER, hint_text, font(12.0), TEXT_DIM());
    let t = now as f32;
    let c = Pos2::new(hint.left() - 12.0, hr.center().y);
    crate::kit::glass::stroke_circle(&p, c, 5.0, Stroke::new(1.5, white(40)));
    let pts: Vec<Pos2> = (0..=12).map(|i| t * 5.0 + i as f32 * 0.15).map(|a| c + Vec2::angled(a) * 5.0).collect();
    p.add(egui::Shape::line(pts, Stroke::new(1.5, tint::DISK_MAP())));
    if pill_button(ui, stop_r, "Stop", BTN_SECONDARY, true).clicked() {
        act = Act::Stop;
    }
    let track = Rect::from_min_size(Pos2::new(hr.left(), hr.bottom() - 2.0), Vec2::new(hr.width(), 2.0));
    let f = ((now * 0.5).fract() as f32) * 1.4 - 0.4;
    let seg = Rect::from_min_max(Pos2::new(track.left() + track.width() * f.max(0.0), track.top()), Pos2::new(track.left() + track.width() * (f + 0.4).min(1.0), track.bottom()));
    p.with_clip_rect(track).rect_filled(seg, 1.0, tint::DISK_MAP());

    // order: new folders join at the end; a full re-sort by size at most once a second
    for row in rows {
        if !app.lens_order.contains(&row.path) {
            app.lens_order.push(row.path.clone());
        }
    }
    if now - app.lens_order_at > 1.0 {
        let size = |pa: &PathBuf| rows.iter().find(|x| &x.path == pa).map(|x| x.bytes).unwrap_or(0);
        app.lens_order.sort_by_key(|pa| std::cmp::Reverse(size(pa)));
        app.lens_order_at = now;
    }

    // the list
    // full width: nothing can be picked while mapping, so no side column
    let body = Rect::from_min_max(Pos2::new(r.left() + 20.0, hr.bottom() + 16.0), Pos2::new(r.right() - 20.0, r.bottom() - 16.0));
    let fr = Rect::from_min_max(Pos2::new(body.left(), body.bottom() - INFO_H), body.max);
    let area = Rect::from_min_max(body.min, Pos2::new(body.right(), fr.top() - 8.0));
    const ROW_H: f32 = 44.0;
    const GAP: f32 = 4.0;
    let fit = (((area.height() + GAP) / (ROW_H + GAP)).floor() as usize).max(1);
    let max_bytes = rows.iter().map(|x| x.bytes).max().unwrap_or(1).max(1);
    let ordered: Vec<&scan::LiveRow> = app.lens_order.iter().filter_map(|pa| rows.iter().find(|x| &x.path == pa)).collect();
    let (shown_rows, rest) = if ordered.len() > fit { (&ordered[..fit - 1], &ordered[fit - 1..]) } else { (&ordered[..], &ordered[..0]) };
    let dt = ctx.input(|i| i.stable_dt).clamp(0.0, 0.1);
    let k = 1.0 - (-dt * 8.0).exp();
    let mut drawn = std::collections::HashMap::new();
    let lp = p.with_clip_rect(area.expand(2.0));
    let name_w = (area.width() * 0.32).clamp(140.0, 240.0);
    let size_w = 90.0;
    let pointer = ctx.pointer_hover_pos();
    let mut hovered: Option<(&scan::LiveRow, Color32)> = None;
    for (i, row) in shown_rows.iter().enumerate() {
        let target = Rect::from_min_size(Pos2::new(area.left(), area.top() + i as f32 * (ROW_H + GAP)), Vec2::new(area.width(), ROW_H));
        let frac = row.bytes as f32 / max_bytes as f32;
        // glide from last frame; a new row slides in from just below with an empty bar
        let (prev, pf) = app.lens_rows.get(&row.path).copied().unwrap_or((target.translate(Vec2::new(0.0, 12.0)), 0.0));
        let rr = lerp_rect(prev, target, k);
        let fr_ = pf + (frac - pf) * k;
        drawn.insert(row.path.clone(), (rr, fr_));
        let hue = hue_of(app, root, &row.path);
        let hov = pointer.is_some_and(|q| rr.contains(q));
        p_row(&lp, rr, row, hue, fr_, hov, name_w, size_w, now, i);
        if hov {
            hovered = Some((row, hue));
        }
    }
    if !rest.is_empty() {
        let i = shown_rows.len();
        let rr = Rect::from_min_size(Pos2::new(area.left(), area.top() + i as f32 * (ROW_H + GAP)), Vec2::new(area.width(), ROW_H));
        let bytes: u64 = rest.iter().map(|x| x.bytes).sum();
        lp.rect_filled(rr, R_SMALL, white(8));
        lp.text(Pos2::new(rr.left() + 30.0, rr.center().y), Align2::LEFT_CENTER, format!("{} more folders", rest.len()), font(13.0), TEXT_DIM());
        lp.text(Pos2::new(rr.right() - 14.0, rr.center().y), Align2::RIGHT_CENTER, human_bytes(bytes), font(13.0), TEXT_FAINT());
    }
    if rows.is_empty() {
        lp.text(area.center(), Align2::CENTER_CENTER, "Looking inside…", font(13.0), TEXT_FAINT());
    }
    // keep animating only while something is still gliding
    let moving = drawn.iter().any(|(pa, (rr, f))| {
        app.lens_rows.get(pa).is_none_or(|(a, b)| (a.min - rr.min).length() > 0.3 || (b - f).abs() > 0.001)
    });
    app.lens_rows = drawn;
    if moving {
        ctx.request_repaint();
    } else {
        crate::kit::glass::animate(&ctx);
    }

    // under the list: what's under the pointer, or what's being measured
    match hovered {
        Some((row, hue)) => {
            p.rect_filled(Rect::from_center_size(Pos2::new(fr.left() + 5.0, fr.center().y), Vec2::splat(10.0)), 3.0, hue);
            let info = if row.done { format!("{} · {}% of what's measured", human_bytes(row.bytes), pct(row.bytes, measured)) } else { format!("{} so far · still measuring", human_bytes(row.bytes)) };
            let ir = p.text(Pos2::new(fr.right(), fr.center().y), Align2::RIGHT_CENTER, info, font(13.0), TEXT_DIM());
            text_fit(&p, Pos2::new(fr.left() + 18.0, fr.center().y), Align2::LEFT_CENTER, &tilde(&row.path), mono(12.0), TEXT_SOFT(), ir.left() - fr.left() - 34.0);
        }
        None => {
            let cur = app.lens.current.lock().unwrap().clone();
            text_fit(&p, Pos2::new(fr.left(), fr.center().y), Align2::LEFT_CENTER, &tilde(Path::new(&cur)), mono(12.0), TEXT_FAINT(), fr.width());
        }
    }
    act
}

/// One folder row of the live list: colour, name and state, a size bar, the size.
#[allow(clippy::too_many_arguments)]
fn p_row(p: &egui::Painter, rr: Rect, row: &scan::LiveRow, hue: Color32, frac: f32, hov: bool, name_w: f32, size_w: f32, now: f64, i: usize) {
    p.rect_filled(rr, R_SMALL, white(if hov { 18 } else { 10 }));
    p.rect_filled(Rect::from_center_size(Pos2::new(rr.left() + 16.0, rr.center().y), Vec2::splat(10.0)), 3.0, hue);
    let nx = rr.left() + 32.0;
    text_fit(p, Pos2::new(nx, rr.top() + 6.0), Align2::LEFT_TOP, &row.name, bold(13.0), TEXT(), name_w - 40.0);
    let (state, sc) = if row.done { ("done", TEXT_FAINT()) } else { ("measuring…", TEXT_MUTED()) };
    p.text(Pos2::new(nx, rr.top() + 24.0), Align2::LEFT_TOP, state, font(11.0), sc);
    let track = Rect::from_min_max(Pos2::new(rr.left() + name_w, rr.center().y - 4.0), Pos2::new(rr.right() - size_w - 14.0, rr.center().y + 4.0));
    if track.width() > 20.0 {
        p.rect_filled(track, 4.0, white(14));
        let fill = Rect::from_min_size(track.min, Vec2::new((track.width() * frac.clamp(0.0, 1.0)).max(4.0), track.height()));
        let col = if row.done { lerp_color(BG(), hue, 0.9) } else { lerp_color(BG(), hue, 0.55) };
        p.rect_filled(fill, 4.0, col);
        if !row.done {
            // a slow, faint shimmer along bars still growing
            let w = 60.0;
            let f = ((now * 0.45 + i as f64 * 0.17).fract() as f32) * (fill.width() + w) - w;
            let sheen = Rect::from_min_size(Pos2::new(fill.left() + f, fill.top()), Vec2::new(w, fill.height()));
            p.with_clip_rect(fill.intersect(p.clip_rect())).rect_filled(sheen, 0.0, white(22));
        }
    }
    let size = human_bytes(row.bytes);
    p.text(Pos2::new(rr.right() - 14.0, rr.center().y), Align2::RIGHT_CENTER, size, font(13.0), if row.done { TEXT_SOFT() } else { TEXT_DIM() });
}

/// The treemap screen. `pending` is set while mapping: those folders are
/// still being measured and the header shows progress with Stop.
/// Bubbles drawn on their own; the rest fold into one grey "Other" bubble.
const MAX_BUBBLES: usize = 18;
const LIST_W: f32 = 320.0;
const ROW_H: f32 = 32.0;
/// How long a zoom into a bubble takes, seconds.
const ZOOM_SECS: f64 = 0.35;

/// One bubble: where it sits in pack space, its radius, and what it is.
struct Bubble {
    at: Vec2,
    r: f32,
    idx: Option<usize>, // index into the folder's children; None = Other
}

/// Circles for `sizes` (biggest first), packed around the centre: each new
/// circle goes to the free spot nearest the middle that touches one already
/// placed. Area follows size, so a folder twice as big looks twice as big.
fn pack(sizes: &[u64]) -> Vec<(Vec2, f32)> {
    let max = sizes.first().copied().unwrap_or(1).max(1) as f32;
    let radii: Vec<f32> = sizes.iter().map(|b| (*b as f32 / max).sqrt().max(0.08)).collect();
    let gap = 0.03;
    let mut out: Vec<(Vec2, f32)> = Vec::with_capacity(radii.len());
    for &r in &radii {
        if out.is_empty() {
            out.push((Vec2::ZERO, r));
            continue;
        }
        let mut best: Option<(f32, Vec2)> = None;
        for &(c, rc) in &out {
            for k in 0..48 {
                let a = k as f32 / 48.0 * std::f32::consts::TAU;
                let at = c + Vec2::angled(a) * (rc + r + gap);
                let d = at.length() + r;
                if best.is_some_and(|b| b.0 <= d) {
                    continue;
                }
                if out.iter().all(|&(o, ro)| (o - at).length() >= ro + r + gap - 1e-4) {
                    best = Some((d, at));
                }
            }
        }
        out.push((best.map(|b| b.1).unwrap_or(Vec2::ZERO), r));
    }
    out
}

thread_local! {
    /// The last pack, by folder (and how many bubbles), so it isn't redone every frame.
    static PACK: std::cell::RefCell<Option<(PathBuf, Vec<u64>, Vec<(Vec2, f32)>)>> = const { std::cell::RefCell::new(None) };
}

fn pack_cached(path: &Path, sizes: &[u64]) -> Vec<(Vec2, f32)> {
    PACK.with(|c| {
        let mut c = c.borrow_mut();
        if let Some((p, s, v)) = c.as_ref() {
            if p == path && s == sizes {
                return v.clone();
            }
        }
        let v = pack(sizes);
        *c = Some((path.to_path_buf(), sizes.to_vec(), v.clone()));
        v
    })
}

fn is_folder(n: &Node) -> bool {
    !n.children.is_empty() || n.items > 1
}

fn results(app: &mut App, ui: &mut Ui, r: Rect, root: &Node, _pending: Option<&[PathBuf]>) -> Act {
    let ctx = ui.ctx().clone();
    let p = ui.painter().clone();
    let mut act = Act::None;
    if scan::find(root, &app.lens_path).is_none() {
        app.lens_path = root.path.clone();
    }
    let node = scan::find(root, &app.lens_path).unwrap_or(root);
    let mut go_to: Option<(PathBuf, Option<Rect>)> = None;
    let root_label = if root.path == home() { "Home".to_string() } else { root.name.clone() };
    let depth = node.path.strip_prefix(&root.path).map(|x| x.components().count()).unwrap_or(0);
    let here = if depth == 0 { root_label.clone() } else { node.name.clone() };

    // header: up, breadcrumbs, when it was mapped, Start over
    let hr = Rect::from_min_size(r.min, Vec2::new(r.width(), HEADER_H));
    hairline(&p, hr.left_bottom(), hr.right_bottom());
    let up = Rect::from_min_size(Pos2::new(hr.left() + 20.0, hr.center().y - 14.0), Vec2::splat(28.0));
    let can_up = node.path != root.path;
    let ur = ui.interact(up, ui.scope_id().with("map-up"), if can_up { Sense::click() } else { Sense::hover() });
    p.rect_filled(up, R_SMALL, white(if !can_up { 8 } else if ur.hovered() { 32 } else { 20 }));
    icon(&p, Icon::Back, up.shrink(7.0), if can_up { TEXT_SOFT() } else { TEXT_MUTED() });
    if ur.on_hover_text("Up one level").clicked() && can_up {
        go_to = node.path.parent().map(|x| (x.to_path_buf(), None));
    }
    let mut crumbs: Vec<(String, PathBuf)> = vec![(root_label.clone(), root.path.clone())];
    if let Ok(rel) = node.path.strip_prefix(&root.path) {
        let mut acc = root.path.clone();
        for c in rel.components() {
            acc.push(c);
            crumbs.push((c.as_os_str().to_string_lossy().into_owned(), acc.clone()));
        }
    }
    let nb = button_width(&p, "Start over", BTN_SECONDARY);
    let new_r = Rect::from_min_size(Pos2::new(hr.right() - 20.0 - nb, hr.center().y - 15.0), Vec2::new(nb, 30.0));
    let when = match *app.lens.when.lock().unwrap() {
        Some(w) => format!("Mapped {}", ago(w)),
        None => String::new(),
    };
    let hint = p.text(Pos2::new(new_r.left() - 14.0, hr.center().y), Align2::RIGHT_CENTER, when, font(12.0), TEXT_FAINT());
    let mut x = up.right() + 8.0;
    let n_crumbs = crumbs.len();
    for (k, (label, path)) in crumbs.iter().enumerate() {
        let last = k + 1 == n_crumbs;
        let g = p.layout_no_wrap(label.clone(), if last { bold(13.0) } else { font(13.0) }, TEXT());
        let br = Rect::from_min_size(Pos2::new(x, hr.center().y - 12.0), Vec2::new(g.size().x + 16.0, 24.0));
        if br.right() > hint.left() - 40.0 {
            break;
        }
        let resp = ui.interact(br, ui.scope_id().with(("crumb", k)), Sense::click());
        if last {
            p.rect_filled(br, 6.0, white(20));
        } else if resp.hovered() {
            p.rect_filled(br, 6.0, white(10));
        }
        p.galley(Pos2::new(br.left() + 8.0, br.center().y - g.size().y / 2.0), g, if last { TEXT() } else { TEXT_DIM() });
        if resp.clicked() && !last {
            go_to = Some((path.clone(), None));
        }
        x = br.right() + 2.0;
        if !last {
            p.text(Pos2::new(x + 4.0, hr.center().y), Align2::CENTER_CENTER, "›", font(13.0), TEXT_MUTED());
            x += 10.0;
        }
    }
    if pill_button(ui, new_r, "Start over", BTN_SECONDARY, true).clicked() {
        act = Act::NewMap;
    }

    let body = Rect::from_min_max(Pos2::new(r.left() + 20.0, hr.bottom() + 16.0), Pos2::new(r.right() - 20.0, r.bottom() - 16.0));
    let picked_total: u64 = app.lens_selected.iter().map(|x| x.1).sum();
    let list_w = (body.width() * 0.36).clamp(260.0, LIST_W);
    let lr = Rect::from_min_max(body.min, Pos2::new(body.left() + list_w, body.bottom()));
    let area = Rect::from_min_max(Pos2::new(lr.right() + 16.0, body.top()), body.max);
    let now = ctx.input(|i| i.time);
    let pointer = ctx.pointer_hover_pos();
    let mut hover_path: Option<PathBuf> = None;
    let mut toggle: Option<(PathBuf, u64)> = None;

    // ---- the list: this folder, everything in it, the disk
    glass_card(&p, lr, R_CARD, 0.0, false);
    let x0 = lr.left() + 14.0;
    let x1 = lr.right() - 14.0;
    let mut y = lr.top() + 14.0;
    icon(&p, Icon::Folder, Rect::from_min_size(Pos2::new(x0, y + 2.0), Vec2::splat(28.0)), ACCENT());
    text_fit(&p, Pos2::new(x0 + 38.0, y + 1.0), Align2::LEFT_TOP, &here, ty::title(), TEXT(), x1 - x0 - 38.0);
    text_fit(&p, Pos2::new(x0 + 38.0, y + 19.0), Align2::LEFT_TOP, &format!("{} · {} items", human_bytes(node.bytes), group_digits(node.items)), ty::caption(), ty::caption_color(), x1 - x0 - 38.0);
    y += 46.0;
    hairline(&p, Pos2::new(lr.left(), y), Pos2::new(lr.right(), y));
    y += 6.0;
    let disk_h = 46.0;
    let list_bottom = lr.bottom() - disk_h - 6.0;
    let fit = (((list_bottom - y) / ROW_H).floor().max(0.0)) as usize;
    let kids = &node.children;
    let shown = if kids.len() > fit { fit.saturating_sub(1) } else { kids.len() };
    let lp = p.with_clip_rect(Rect::from_min_max(Pos2::new(lr.left(), y), Pos2::new(lr.right(), list_bottom)));
    for (i, k) in kids.iter().take(shown).enumerate() {
        let rr = Rect::from_min_size(Pos2::new(lr.left() + 6.0, y), Vec2::new(lr.width() - 12.0, ROW_H));
        let resp = ui.interact(rr, ui.scope_id().with(("lens-row", i)), Sense::click());
        let selectable = !scan::is_essential(&k.path);
        let on = app.lens_selected.iter().any(|(q, _)| q == &k.path);
        if resp.hovered() {
            lp.rect_filled(rr, R_SMALL, white(12));
            hover_path = Some(k.path.clone());
        }
        let cb = Rect::from_center_size(Pos2::new(rr.left() + 14.0, rr.center().y), Vec2::splat(15.0));
        if selectable && checkbox(ui, cb, ui.scope_id().with(("lens-cb", i)), on, true).clicked() {
            toggle = Some((k.path.clone(), k.bytes));
        }
        let folder = is_folder(k);
        icon(&lp, if folder { Icon::Folder } else { Icon::Doc }, Rect::from_center_size(Pos2::new(rr.left() + 40.0, rr.center().y), Vec2::splat(16.0)), if folder { ACCENT() } else { TEXT_DIM() });
        let sr = lp.text(Pos2::new(rr.right() - 8.0, rr.center().y), Align2::RIGHT_CENTER, human_bytes(k.bytes), font(12.0), TEXT_SOFT());
        text_fit(&lp, Pos2::new(rr.left() + 56.0, rr.center().y), Align2::LEFT_CENTER, &k.name, font(13.0), TEXT(), sr.left() - rr.left() - 66.0);
        if resp.clicked() && !k.children.is_empty() {
            go_to = Some((k.path.clone(), None));
        }
        if resp.hovered() && !k.children.is_empty() {
            ctx.set_cursor_icon(egui::CursorIcon::PointingHand);
        }
        y += ROW_H;
    }
    let rest: u64 = kids.iter().skip(shown).map(|k| k.bytes).sum::<u64>() + node.other;
    if rest > 0 && y + ROW_H <= list_bottom + 1.0 {
        let rr = Rect::from_min_size(Pos2::new(lr.left() + 6.0, y), Vec2::new(lr.width() - 12.0, ROW_H));
        let n = kids.len() - shown;
        let label = if n > 0 { format!("{n} more and small files") } else { "Small files".to_string() };
        let sr = lp.text(Pos2::new(rr.right() - 8.0, rr.center().y), Align2::RIGHT_CENTER, human_bytes(rest), font(12.0), TEXT_FAINT());
        text_fit(&lp, Pos2::new(rr.left() + 56.0, rr.center().y), Align2::LEFT_CENTER, &label, font(13.0), TEXT_DIM(), sr.left() - rr.left() - 66.0);
    }
    // the disk this map is on
    let (disk_total, disk_free) = crate::kit::fs::volume_space(&root.path.to_string_lossy());
    if disk_total > 0 {
        let used = disk_total.saturating_sub(disk_free);
        let dy = lr.bottom() - disk_h;
        hairline(&p, Pos2::new(lr.left(), dy), Pos2::new(lr.right(), dy));
        p.text(Pos2::new(x0, dy + 16.0), Align2::LEFT_CENTER, "Macintosh HD", font(12.0), TEXT_DIM());
        p.text(Pos2::new(x1, dy + 16.0), Align2::RIGHT_CENTER, format!("{} of {} used", human_bytes(used), human_bytes(disk_total)), font(12.0), TEXT_DIM());
        bar(&p, Rect::from_min_size(Pos2::new(x0, dy + 28.0), Vec2::new(x1 - x0, 4.0)), (used as f32 / disk_total as f32).clamp(0.01, 1.0), ACCENT());
    }

    // ---- the bubbles: this folder's contents, one level, area by size
    let sel_h = if app.lens_selected.is_empty() { 0.0 } else { 46.0 };
    let pa = Rect::from_min_max(area.min, Pos2::new(area.right(), area.bottom() - sel_h));
    let keep = kids.len().min(MAX_BUBBLES);
    let other = kids[keep..].iter().map(|k| k.bytes).sum::<u64>() + node.other;
    let mut sizes: Vec<u64> = kids[..keep].iter().map(|k| k.bytes.max(1)).collect();
    let mut bubbles: Vec<Bubble> = Vec::new();
    if other > 0 {
        sizes.push(other);
    }
    // packing wants biggest first; Other can be bigger than some folders
    let mut order: Vec<usize> = (0..sizes.len()).collect();
    order.sort_by_key(|&i| std::cmp::Reverse(sizes[i]));
    let sorted: Vec<u64> = order.iter().map(|&i| sizes[i]).collect();
    for (j, (at, rad)) in pack_cached(&node.path, &sorted).into_iter().enumerate() {
        let i = order[j];
        bubbles.push(Bubble { at, r: rad, idx: (i < keep).then_some(i) });
    }
    // fit the pack into the area
    let (mut lo, mut hi) = (Vec2::splat(f32::MAX), Vec2::splat(f32::MIN));
    for b in &bubbles {
        lo = lo.min(b.at - Vec2::splat(b.r));
        hi = hi.max(b.at + Vec2::splat(b.r));
    }
    let span = (hi - lo).max(Vec2::splat(0.01));
    let s = ((pa.width() - 24.0) / span.x).min((pa.height() - 24.0) / span.y);
    let mid = (lo + hi) / 2.0;
    let place = |b: &Bubble| (pa.center() + (b.at - mid) * s, b.r * s);
    // zoom: the new level grows out of the bubble that was clicked
    if app.lens_morph.as_ref().is_none_or(|m| m.1 != node.path) {
        app.lens_morph = Some((None, node.path.clone(), now));
    }
    let (from, start) = app.lens_morph.as_ref().map(|m| (m.0, m.2)).unwrap_or((None, now));
    let t = ((now - start) / ZOOM_SECS).clamp(0.0, 1.0) as f32;
    let k = 1.0 - (1.0 - t).powi(3);
    if t < 1.0 {
        ctx.request_repaint();
    }
    let fit_r = pa.width().min(pa.height()) / 2.0;
    let morph = |c: Pos2, rad: f32| -> (Pos2, f32) {
        match from {
            Some(fr) => {
                let z = fr.width() / 2.0 / fit_r;
                let c0 = fr.center() + (c - pa.center()) * z;
                (c0 + (c - c0) * k, rad * (z + (1.0 - z) * k))
            }
            None => (pa.center() + (c - pa.center()) * (0.94 + 0.06 * k), rad * (0.94 + 0.06 * k)),
        }
    };
    let bp = p.with_clip_rect(pa.expand(4.0));
    let alpha = if from.is_some() { k.max(0.15) } else { k };
    let mut tip: Option<(Pos2, String, String)> = None;
    for (n, b) in bubbles.iter().enumerate() {
        let (c, rad) = place(b);
        let (c, rad) = morph(c, rad);
        let kid = b.idx.map(|i| &kids[i]);
        let hot = t >= 1.0 && pointer.is_some_and(|q| (q - c).length() <= rad);
        let linked = kid.is_some_and(|kd| hover_path.as_ref() == Some(&kd.path));
        let on = kid.is_some_and(|kd| app.lens_selected.iter().any(|(q, _)| q == &kd.path));
        let base = if kid.is_some() { BLUE() } else { PURPLE() };
        let fill = with_alpha(base, (if hot || linked { 0.34 } else { 0.2 }) * alpha);
        bp.circle_filled(c, rad, fill);
        crate::kit::glass::stroke_circle(&bp, c, rad, Stroke::new(if on { 2.0 } else { 1.0 }, if on { Color32::WHITE } else { with_alpha(base, 0.6 * alpha) }));
        // label by room: icon, name and size; icon only; nothing
        let txt = with_alpha(TEXT(), alpha);
        match kid {
            Some(kd) if rad >= 46.0 => {
                let isz = (rad * 0.42).min(56.0);
                icon(&bp, if is_folder(kd) { Icon::Folder } else { Icon::Doc }, Rect::from_center_size(c - Vec2::new(0.0, isz * 0.45), Vec2::splat(isz)), with_alpha(ACCENT(), alpha));
                text_fit(&bp, c + Vec2::new(0.0, isz * 0.15), Align2::CENTER_TOP, &kd.name, bold(12.0), txt, rad * 1.6);
                text_fit(&bp, c + Vec2::new(0.0, isz * 0.15 + 16.0), Align2::CENTER_TOP, &human_bytes(kd.bytes), font(11.0), with_alpha(TEXT_SOFT(), alpha), rad * 1.6);
            }
            Some(kd) if rad >= 30.0 => {
                text_fit(&bp, c - Vec2::new(0.0, 1.0), Align2::CENTER_BOTTOM, &kd.name, bold(11.0), txt, rad * 1.7);
                text_fit(&bp, c + Vec2::new(0.0, 1.0), Align2::CENTER_TOP, &human_bytes(kd.bytes), font(10.0), with_alpha(TEXT_SOFT(), alpha), rad * 1.7);
            }
            Some(kd) if rad >= 16.0 => {
                let isz = (rad * 0.8).min(24.0);
                icon(&bp, if is_folder(kd) { Icon::Folder } else { Icon::Doc }, Rect::from_center_size(c, Vec2::splat(isz)), with_alpha(ACCENT(), alpha));
            }
            None if rad >= 30.0 => {
                text_fit(&bp, c - Vec2::new(0.0, 1.0), Align2::CENTER_BOTTOM, "Other", bold(12.0), with_alpha(TEXT_DIM(), alpha), rad * 1.6);
                text_fit(&bp, c + Vec2::new(0.0, 1.0), Align2::CENTER_TOP, &human_bytes(other), font(11.0), with_alpha(TEXT_FAINT(), alpha), rad * 1.6);
            }
            _ => {}
        }
        if hot {
            match kid {
                Some(kd) => {
                    hover_path = Some(kd.path.clone());
                    tip = Some((c, kd.name.clone(), format!("{} · {} items · {}% of {here}", human_bytes(kd.bytes), group_digits(kd.items), pct(kd.bytes, node.bytes))));
                    if !kd.children.is_empty() {
                        ctx.set_cursor_icon(egui::CursorIcon::PointingHand);
                    }
                    let resp = ui.interact(Rect::from_center_size(c, Vec2::splat(rad * 2.0)), ui.scope_id().with(("bubble", n)), Sense::click());
                    if resp.clicked() && !kd.children.is_empty() {
                        go_to = Some((kd.path.clone(), Some(Rect::from_center_size(c, Vec2::splat(rad * 2.0)))));
                    }
                }
                None => tip = Some((c, "Other".into(), format!("{} · smaller items and loose files", human_bytes(other)))),
            }
        }
    }
    // hover card: name and numbers, beside the pointer
    if let (Some((_, name, info)), Some(q)) = (tip, pointer) {
        let w = p.layout_no_wrap(info.clone(), font(12.0), TEXT()).size().x.max(p.layout_no_wrap(name.clone(), bold(13.0), TEXT()).size().x) + 24.0;
        let mut tr = Rect::from_min_size(q + Vec2::new(14.0, 14.0), Vec2::new(w, 50.0));
        if tr.right() > pa.right() {
            tr = tr.translate(Vec2::new(-w - 28.0, 0.0));
        }
        if tr.bottom() > pa.bottom() {
            tr = tr.translate(Vec2::new(0.0, -78.0));
        }
        p.rect_filled(tr, 10.0, RAISED());
        p.rect_stroke(tr, 10.0, Stroke::new(1.0, white(31)), StrokeKind::Inside);
        p.text(tr.min + Vec2::new(12.0, 9.0), Align2::LEFT_TOP, name, bold(13.0), TEXT());
        p.text(tr.min + Vec2::new(12.0, 28.0), Align2::LEFT_TOP, info, font(12.0), TEXT_DIM());
    }

    // ---- the selection bar, only while something is picked
    if !app.lens_selected.is_empty() {
        let sr = Rect::from_min_max(Pos2::new(area.left(), area.bottom() - 34.0), area.max);
        let n = app.lens_selected.len();
        let g = p.text(Pos2::new(sr.left(), sr.center().y), Align2::LEFT_CENTER, human_bytes(picked_total), bold(15.0), TEXT());
        p.text(Pos2::new(g.right() + 8.0, sr.center().y), Align2::LEFT_CENTER, format!("{n} item{} picked", if n == 1 { "" } else { "s" }), font(13.0), TEXT_DIM());
        let bw = button_width(&p, "Review and remove", BTN_PRIMARY);
        let rb = Rect::from_min_size(Pos2::new(sr.right() - bw, sr.top()), Vec2::new(bw, 34.0));
        if pill_button(ui, rb, "Review and remove", BTN_PRIMARY, true).clicked() {
            app.lens_review = true;
        }
        let cw = button_width(&p, "Clear", BTN_SECONDARY);
        let cr = Rect::from_min_size(Pos2::new(rb.left() - 8.0 - cw, sr.top()), Vec2::new(cw, 34.0));
        if pill_button(ui, cr, "Clear", BTN_SECONDARY, true).clicked() {
            app.lens_selected.clear();
        }
    }

    if let Some((path, b)) = toggle {
        if let Some(i) = app.lens_selected.iter().position(|(q, _)| q == &path) {
            app.lens_selected.remove(i);
        } else {
            // a folder replaces anything picked inside it
            app.lens_selected.retain(|(q, _)| !q.starts_with(&path));
            if !app.lens_selected.iter().any(|(q, _)| path.starts_with(q)) {
                app.lens_selected.push((path, b));
            }
        }
    }
    if let Some((target, from)) = go_to {
        app.lens_path = target.clone();
        app.lens_morph = Some((from, target, now));
        ctx.request_repaint();
    }
    act
}

fn review(app: &mut App, ui: &mut Ui, r: Rect, cx: &mut Cx) {
    let p = ui.painter().clone();
    // swallow clicks behind the dialog
    let _ = ui.interact(r, ui.scope_id().with("review-scrim"), Sense::click());
    p.rect_filled(r, CornerRadius { ne: 12, se: 12, ..Default::default() }, Color32::from_black_alpha(115));
    let n = app.lens_selected.len();
    let rows = n.min(8);
    let h = 22.0 + 46.0 + 14.0 + rows as f32 * 34.0 + 8.0 + 18.0 + 30.0 + 22.0;
    let m = Rect::from_center_size(r.center(), Vec2::new(520.0, h));
    p.rect_filled(m.translate(Vec2::new(0.0, 10.0)).expand(6.0), 18.0, Color32::from_black_alpha(70));
    p.rect_filled(m, 14.0, RAISED());
    p.rect_stroke(m, 14.0, Stroke::new(1.0, white(31)), StrokeKind::Inside);
    let total: u64 = app.lens_selected.iter().map(|x| x.1).sum();
    let title = format!("Move {} item{} ({}) to the Trash?", n, if n == 1 { "" } else { "s" }, human_bytes(total));
    text_fit(&p, m.min + Vec2::new(22.0, 22.0), Align2::LEFT_TOP, &title, bold(17.0), TEXT(), m.width() - 44.0);
    text_fit(&p, m.min + Vec2::new(22.0, 48.0), Align2::LEFT_TOP, "System folders and code projects are skipped", font(13.0), TEXT_DIM(), m.width() - 44.0);
    let list = Rect::from_min_size(m.min + Vec2::new(22.0, 82.0), Vec2::new(m.width() - 44.0, rows as f32 * 34.0 + 8.0));
    p.rect_filled(list, 10.0, Color32::from_black_alpha(46));
    let mut drop: Option<usize> = None;
    for (i, (path, b)) in app.lens_selected.iter().enumerate().take(rows) {
        let rr = Rect::from_min_size(Pos2::new(list.left(), list.top() + 4.0 + i as f32 * 34.0), Vec2::new(list.width(), 34.0));
        icon(&p, Icon::Folder, Rect::from_center_size(Pos2::new(rr.left() + 20.0, rr.center().y), Vec2::splat(16.0)), ACCENT());
        let xr = Rect::from_center_size(Pos2::new(rr.right() - 20.0, rr.center().y), Vec2::splat(22.0));
        let sr = p.text(Pos2::new(xr.left() - 8.0, rr.center().y), Align2::RIGHT_CENTER, human_bytes(*b), font(13.0), TEXT_SOFT());
        text_fit(&p, Pos2::new(rr.left() + 38.0, rr.center().y), Align2::LEFT_CENTER, &tilde(path), mono(12.0), TEXT(), sr.left() - rr.left() - 48.0);
        if round_button(ui, xr, Icon::Close, "Remove from the list").clicked() {
            drop = Some(i);
        }
    }
    if let Some(i) = drop {
        app.lens_selected.remove(i);
    }
    let by = m.bottom() - 22.0 - 30.0;
    let ow = button_width(&p, "Move to Trash", BTN_PRIMARY);
    let ok = Rect::from_min_size(Pos2::new(m.right() - 22.0 - ow, by), Vec2::new(ow, 30.0));
    let cw = button_width(&p, "Cancel", BTN_SECONDARY);
    let cancel = Rect::from_min_size(Pos2::new(ok.left() - 8.0 - cw, by), Vec2::new(cw, 30.0));
    if pill_button(ui, cancel, "Cancel", BTN_SECONDARY, true).clicked() || app.lens_selected.is_empty() {
        app.lens_review = false;
    }
    if pill_button(ui, ok, "Move to Trash", BTN_PRIMARY, !app.lens_selected.is_empty()).clicked() {
        let mut removed = vec![];
        let mut failed = 0;
        let mut freed = 0;
        for (path, b) in app.lens_selected.drain(..) {
            if scan::is_essential(&path) || crate::kit::fs::is_protected(&path).is_some() && path.join(".git").exists() {
                failed += 1;
                continue;
            }
            match crate::kit::fs::move_to_trash(&path) {
                Ok(()) => {
                    freed += b;
                    removed.push(path)
                }
                Err(_) => failed += 1,
            }
        }
        if let Some(root) = app.lens.result.lock().unwrap().as_mut() {
            scan::prune(root, &removed);
        }
        scan::save_later(&app.lens);
        app.lens_review = false;
        let msg = if failed == 0 { format!("Moved {} to the Trash", human_bytes(freed)) } else { format!("Moved {} to the Trash · {failed} items couldn't be removed", human_bytes(freed)) };
        cx.toast(msg);
    }
}

/// A made-up home folder for screenshots (no personal paths).
pub fn demo_tree(app: &mut App) {
    const GB: f64 = 1_000_000_000.0;
    fn mk(parent: &Path, name: &str, gb: f64, kids: Vec<(&str, f64, Vec<(&str, f64)>)>) -> Node {
        let path = parent.join(name);
        let children: Vec<Node> = kids
            .into_iter()
            .map(|(n, g, sub)| mk(&path, n, g, sub.into_iter().map(|(a, b)| (a, b, vec![])).collect()))
            .collect();
        Node { name: name.into(), path: path.clone(), bytes: (gb * GB) as u64, items: (gb * 2400.0) as u64, children, other: 0 }
    }
    let base = PathBuf::from("/Users");
    let root = mk(
        &base,
        "me",
        365.0,
        vec![
            ("Library", 182.0, vec![("Containers", 61.0), ("Application Support", 48.0), ("Caches", 34.0), ("Developer", 27.0), ("Mail", 7.0), ("Logs", 5.0)]),
            ("Projects", 64.2, vec![("web-app", 22.0), ("cmdmeow", 18.0), ("api-server", 14.0), ("notes", 10.2)]),
            ("Downloads", 38.4, vec![("Xcode_16.xip", 12.0), ("old-movies.zip", 9.4), ("Installers", 8.0), ("Archive", 9.0)]),
            ("Movies", 31.0, vec![("Screen recordings", 19.0), ("Final Cut", 12.0)]),
            ("Pictures", 22.5, vec![("Photos Library", 21.0), ("Wallpapers", 1.5)]),
            ("Documents", 14.1, vec![]),
            ("Music", 6.3, vec![]),
            ("Desktop", 2.1, vec![]),
        ],
    );
    let mut root = root;
    root.other = (4.4 * GB) as u64;
    app.lens_root = root.path.clone();
    app.lens_path = root.path.clone();
    app.lens_focus = Some(root.path.join("Movies"));
    app.lens_selected = vec![(root.path.join("Downloads/Xcode_16.xip"), (12.0 * GB) as u64), (root.path.join("Movies"), (31.0 * GB) as u64)];
    *app.lens.result.lock().unwrap() = Some(root);
}

/// Mapping in progress, for screenshots: some folders done, others still
/// counting.
pub fn demo_live(app: &mut App) {
    use std::sync::atomic::AtomicU64;
    use std::sync::Arc;
    demo_tree(app);
    let root = app.lens.result.lock().unwrap().take().unwrap();
    let live: Vec<scan::LiveChild> = root
        .children
        .iter()
        .enumerate()
        .map(|(k, c)| {
            let done = matches!(k, 1 | 3 | 5 | 7);
            let part = if done { c.bytes } else { (c.bytes as f64 * [0.55, 0.0, 0.8, 0.0, 0.35, 0.0, 0.6][k.min(6)]) as u64 };
            scan::LiveChild { name: c.name.clone(), path: c.path.clone(), bytes: Arc::new(AtomicU64::new(part)), done }
        })
        .collect();
    app.lens.bytes.store(live.iter().map(|l| l.bytes.load(Ordering::Relaxed)).sum(), Ordering::Relaxed);
    app.lens.files.store(1_284_311, Ordering::Relaxed);
    *app.lens.current.lock().unwrap() = root.path.join("Library/Containers/com.apple.mail/Data").display().to_string();
    *app.lens.live.lock().unwrap() = Some((root.path.clone(), live));
    app.lens.running.store(true, Ordering::SeqCst);
    app.lens_selected.clear();
}
