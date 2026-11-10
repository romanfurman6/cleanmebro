//! Disk Map screens: choose what to map (with live progress while it runs)
//! → rings you can open folder by folder, with a "To remove" list → review and move to Trash.

use super::scan::{self, Node};
use super::Lens as App;
use crate::core::tool::Cx;
use crate::kit::glass::*;
use egui::{Align2, Color32, CornerRadius, Pos2, Rect, Sense, Stroke, StrokeKind, Ui, Vec2};
use std::path::{Path, PathBuf};
use std::sync::atomic::Ordering;

/// Ring shades (from the design); the folder in focus is BLUE / ACCENT.
pub(crate) const HUES: [Color32; 4] = [
    Color32::from_rgb(0x0E, 0x63, 0xBA),
    Color32::from_rgb(0x0C, 0x56, 0xA3),
    Color32::from_rgb(0x0B, 0x4A, 0x8C),
    Color32::from_rgb(0x0A, 0x3D, 0x73),
];
const OTHER_HUE: Color32 = Color32::from_rgb(0x3A, 0x3A, 0x3E);
/// How long the rings take to sweep in, seconds.
const SWEEP_SECS: f64 = 0.45;
const HEADER_H: f32 = 57.0;
const FOOTER_H: f32 = 42.0;
const ASIDE_W: f32 = 260.0;
const DETAILS_MIN_W: f32 = 240.0;
/// Inner-ring folders drawn on their own; the rest fold into "Other".
const MAX_SEGMENTS: usize = 14;
const MIN_SPAN_DEG: f64 = 2.5;

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
            page_header(&ui.painter().clone(), r, "Disk Map", "Opening the last map…", TEXT_DIM);
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
    page_header(&p, r, "Disk Map", "See what takes space and remove it by hand", TEXT_DIM);
    let w = (r.width() - 48.0).min(520.0);
    let pr = Rect::from_center_size(Pos2::new(r.center().x, r.top() + PAGE_HEADER_H + 28.0 + 70.0), Vec2::new(w, 140.0));
    glass_card(&p, pr, R_CARD, 0.0, false);
    p.text(Pos2::new(pr.left() + 16.0, pr.top() + 14.0), Align2::LEFT_TOP, "Map Macintosh HD", bold(15.0), TEXT);
    paragraph(&p, Pos2::new(pr.left() + 16.0, pr.top() + 40.0), "Measures every folder on the disk, so it takes a few minutes and keeps the Mac busy. Nothing is removed.", font(13.0), TEXT_DIM, pr.width() - 32.0);
    let bw = button_width(&p, "Map disk", BTN_PRIMARY) + 28.0;
    pill_button(ui, Rect::from_min_size(Pos2::new(pr.left() + 16.0, pr.bottom() - 48.0), Vec2::new(bw, 32.0)), "Map disk", BTN_PRIMARY, true).clicked()
}

/// The first moments of a map, before the disk's top folders are listed:
/// what's being measured, with Stop. There is nothing to choose: Disk Map
/// always maps the whole startup disk.
fn start(app: &mut App, ui: &mut Ui, r: Rect) {
    let ctx = ui.ctx().clone();
    let p = ui.painter().clone();
    page_header(&p, r, "Disk Map", "See what takes space and remove it by hand", TEXT_DIM);
    let w = (r.width() - 48.0).min(520.0);
    let pr = Rect::from_center_size(Pos2::new(r.center().x, r.top() + PAGE_HEADER_H + 28.0 + 90.0), Vec2::new(w, 180.0));
    glass_card(&p, pr, R_CARD, 0.0, false);
    let bytes = app.lens.bytes.load(Ordering::Relaxed);
    let files = app.lens.files.load(Ordering::Relaxed);
    p.text(Pos2::new(pr.left() + 16.0, pr.top() + 14.0), Align2::LEFT_TOP, "Mapping Macintosh HD", bold(13.0), TEXT_DIM);
    let g = p.layout_no_wrap(human_bytes(bytes), bold(26.0), TEXT);
    let gw = g.size().x;
    p.galley(Pos2::new(pr.left() + 16.0, pr.top() + 36.0), g, TEXT);
    p.text(Pos2::new(pr.left() + 24.0 + gw, pr.top() + 55.0), Align2::LEFT_CENTER, format!("measured · {} files", group_digits(files)), font(13.0), TEXT_DIM);
    let track = Rect::from_min_size(Pos2::new(pr.left() + 16.0, pr.top() + 84.0), Vec2::new(pr.width() - 32.0, 6.0));
    p.rect_filled(track, 3.0, white(20));
    let t = ctx.input(|i| i.time);
    let f = ((t * 0.6).fract() as f32) * 1.35 - 0.35;
    let seg = Rect::from_min_max(Pos2::new(track.left() + track.width() * f.max(0.0), track.top()), Pos2::new(track.left() + track.width() * (f + 0.35).min(1.0), track.bottom()));
    p.with_clip_rect(track).rect_filled(seg, 3.0, tint::DISK_MAP);
    let cur = app.lens.current.lock().unwrap().clone();
    text_fit(&p, Pos2::new(pr.left() + 16.0, pr.top() + 102.0), Align2::LEFT_TOP, &tilde(Path::new(&cur)), mono(11.0), TEXT_FAINT, pr.width() - 32.0);
    let sw = button_width(&p, "Stop", BTN_SECONDARY);
    if pill_button(ui, Rect::from_min_size(Pos2::new(pr.left() + 16.0, pr.bottom() - 44.0), Vec2::new(sw, 28.0)), "Stop", BTN_SECONDARY, true).clicked() {
        app.lens.stop.store(true, Ordering::SeqCst);
    }
    paragraph(&p, Pos2::new(pr.left(), pr.bottom() + 14.0), "Mapping the whole disk takes a few minutes. Give Cmd+Meow Full Disk Access in System Settings for complete results.", font(12.0), TEXT_FAINT, pr.width());
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
    let Some(first) = path.strip_prefix(root).ok().and_then(|r| r.components().next()) else { return OTHER_HUE };
    let top = root.join(first);
    let i = match app.lens_hues.iter().position(|q| *q == top) {
        Some(i) => i,
        None => {
            app.lens_hues.push(top);
            app.lens_hues.len() - 1
        }
    };
    HUES[i % HUES.len()]
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
    let g = p.layout_no_wrap(root_label, bold(13.0), TEXT);
    let tw = g.size().x;
    p.galley(Pos2::new(hr.left() + 24.0, hr.center().y - g.size().y / 2.0), g, TEXT);
    p.text(Pos2::new(hr.left() + 32.0 + tw, hr.center().y), Align2::LEFT_CENTER, human_bytes(measured), font(13.0), TEXT_FAINT);
    let nb = button_width(&p, "Stop", BTN_SECONDARY);
    let stop_r = Rect::from_min_size(Pos2::new(hr.right() - 20.0 - nb, hr.center().y - 15.0), Vec2::new(nb, 30.0));
    let done = rows.iter().filter(|x| x.done).count();
    let hint_text = format!("Mapping · {} of {} folders done · {} files", done, rows.len(), group_digits(app.lens.files.load(Ordering::Relaxed)));
    let hint = p.text(Pos2::new(stop_r.left() - 14.0, hr.center().y), Align2::RIGHT_CENTER, hint_text, font(12.0), TEXT_DIM);
    let t = now as f32;
    let c = Pos2::new(hint.left() - 12.0, hr.center().y);
    p.circle_stroke(c, 5.0, Stroke::new(1.5, white(40)));
    let pts: Vec<Pos2> = (0..=12).map(|i| t * 5.0 + i as f32 * 0.15).map(|a| c + Vec2::angled(a) * 5.0).collect();
    p.add(egui::Shape::line(pts, Stroke::new(1.5, tint::DISK_MAP)));
    if pill_button(ui, stop_r, "Stop", BTN_SECONDARY, true).clicked() {
        act = Act::Stop;
    }
    let track = Rect::from_min_size(Pos2::new(hr.left(), hr.bottom() - 2.0), Vec2::new(hr.width(), 2.0));
    let f = ((now * 0.5).fract() as f32) * 1.4 - 0.4;
    let seg = Rect::from_min_max(Pos2::new(track.left() + track.width() * f.max(0.0), track.top()), Pos2::new(track.left() + track.width() * (f + 0.4).min(1.0), track.bottom()));
    p.with_clip_rect(track).rect_filled(seg, 1.0, tint::DISK_MAP);

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
    let fr = Rect::from_min_max(Pos2::new(r.left(), r.bottom() - FOOTER_H), r.max);
    let body = Rect::from_min_max(Pos2::new(r.left() + 20.0, hr.bottom() + 16.0), Pos2::new(r.right() - 20.0, fr.top() - 12.0));
    let area = Rect::from_min_max(body.min, Pos2::new(body.right() - ASIDE_W - 16.0, body.bottom()));
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
        lp.text(Pos2::new(rr.left() + 30.0, rr.center().y), Align2::LEFT_CENTER, format!("{} more folders", rest.len()), font(13.0), TEXT_DIM);
        lp.text(Pos2::new(rr.right() - 14.0, rr.center().y), Align2::RIGHT_CENTER, human_bytes(bytes), font(13.0), TEXT_FAINT);
    }
    if rows.is_empty() {
        lp.text(area.center(), Align2::CENTER_CENTER, "Looking inside…", font(13.0), TEXT_FAINT);
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

    // aside
    let ar = Rect::from_min_max(Pos2::new(area.right() + 16.0, body.top()), body.max);
    section_header(&p, ar.min, Align2::LEFT_TOP, "TO REMOVE");
    paragraph(&p, Pos2::new(ar.left(), ar.top() + 24.0), "The map opens when mapping finishes. Then click a folder to zoom in and tick a corner to pick it for removal.", font(12.0), TEXT_FAINT, ar.width());
    let rb = Rect::from_min_max(Pos2::new(ar.left(), ar.bottom() - 34.0), ar.max);
    pill_button(ui, rb, "Review and remove", BTN_PRIMARY, false);

    // footer: what's under the pointer, or what's being measured
    p.rect_filled(fr, CornerRadius { se: 12, ..Default::default() }, FOOTER);
    hairline(&p, fr.left_top(), fr.right_top());
    match hovered {
        Some((row, hue)) => {
            p.rect_filled(Rect::from_center_size(Pos2::new(fr.left() + 25.0, fr.center().y), Vec2::splat(10.0)), 3.0, hue);
            let info = if row.done { format!("{} · {}% of what's measured", human_bytes(row.bytes), pct(row.bytes, measured)) } else { format!("{} so far · still measuring", human_bytes(row.bytes)) };
            let ir = p.text(Pos2::new(fr.right() - 20.0, fr.center().y), Align2::RIGHT_CENTER, info, font(13.0), TEXT_DIM);
            text_fit(&p, Pos2::new(fr.left() + 40.0, fr.center().y), Align2::LEFT_CENTER, &tilde(&row.path), mono(12.0), TEXT_SOFT, ir.left() - fr.left() - 56.0);
        }
        None => {
            let cur = app.lens.current.lock().unwrap().clone();
            p.text(Pos2::new(fr.left() + 20.0, fr.center().y), Align2::LEFT_CENTER, "Measuring", font(12.0), TEXT_FAINT);
            text_fit(&p, Pos2::new(fr.left() + 92.0, fr.center().y), Align2::LEFT_CENTER, &tilde(Path::new(&cur)), mono(12.0), TEXT_SOFT, fr.width() - 112.0);
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
    text_fit(p, Pos2::new(nx, rr.top() + 6.0), Align2::LEFT_TOP, &row.name, bold(13.0), TEXT, name_w - 40.0);
    let (state, sc) = if row.done { ("done", TEXT_FAINT) } else { ("measuring…", TEXT_MUTED) };
    p.text(Pos2::new(nx, rr.top() + 24.0), Align2::LEFT_TOP, state, font(11.0), sc);
    let track = Rect::from_min_max(Pos2::new(rr.left() + name_w, rr.center().y - 4.0), Pos2::new(rr.right() - size_w - 14.0, rr.center().y + 4.0));
    if track.width() > 20.0 {
        p.rect_filled(track, 4.0, white(14));
        let fill = Rect::from_min_size(track.min, Vec2::new((track.width() * frac.clamp(0.0, 1.0)).max(4.0), track.height()));
        let col = if row.done { lerp_color(BG, hue, 0.9) } else { lerp_color(BG, hue, 0.55) };
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
    p.text(Pos2::new(rr.right() - 14.0, rr.center().y), Align2::RIGHT_CENTER, size, font(13.0), if row.done { TEXT_SOFT } else { TEXT_DIM });
}

/// The treemap screen. `pending` is set while mapping: those folders are
/// still being measured and the header shows progress with Stop.
fn results(app: &mut App, ui: &mut Ui, r: Rect, root: &Node, pending: Option<&[PathBuf]>) -> Act {
    let ctx = ui.ctx().clone();
    let p = ui.painter().clone();
    let live = pending.is_some();
    let mut act = Act::None;
    if scan::find(root, &app.lens_path).is_none() {
        app.lens_path = root.path.clone();
    }
    let node = scan::find(root, &app.lens_path).unwrap_or(root);
    // inside "Other": the folder minus its biggest items
    let skip = app.lens_skip.min(node.children.len());
    let all = &node.children[skip..];
    let total = if skip == 0 { node.bytes } else { node.other + all.iter().map(|c| c.bytes).sum::<u64>() };
    let mut go_to: Option<PathBuf> = None;
    let mut unskip = false;
    let root_label = if root.path == home() { "Home".to_string() } else { root.name.clone() };

    // header: up, breadcrumbs, size, then the hint and New map, or progress and Stop
    let hr = Rect::from_min_size(r.min, Vec2::new(r.width(), HEADER_H));
    hairline(&p, hr.left_bottom(), hr.right_bottom());
    let up = Rect::from_min_size(Pos2::new(hr.left() + 20.0, hr.center().y - 14.0), Vec2::splat(28.0));
    let can_up = node.path != root.path || skip > 0;
    let ur = ui.interact(up, ui.scope_id().with("map-up"), if can_up { Sense::click() } else { Sense::hover() });
    p.rect_filled(up, R_SMALL, white(if !can_up { 8 } else if ur.hovered() { 32 } else { 20 }));
    icon(&p, Icon::Back, up.shrink(7.0), if can_up { TEXT_SOFT } else { TEXT_MUTED });
    if ur.on_hover_text("Up one level").clicked() && can_up {
        if skip > 0 {
            unskip = true;
        } else {
            go_to = node.path.parent().map(Path::to_path_buf);
        }
    }
    let mut crumbs: Vec<(String, PathBuf)> = vec![(root_label.clone(), root.path.clone())];
    if let Ok(rel) = node.path.strip_prefix(&root.path) {
        let mut acc = root.path.clone();
        for c in rel.components() {
            acc.push(c);
            crumbs.push((c.as_os_str().to_string_lossy().into_owned(), acc.clone()));
        }
    }
    if skip > 0 {
        crumbs.push(("Other".to_string(), node.path.join("\u{0}other")));
    }
    let (btn, kind) = if live { ("Stop", BTN_SECONDARY) } else { ("Start over", BTN_SECONDARY) };
    let nb = button_width(&p, btn, kind);
    let new_r = Rect::from_min_size(Pos2::new(hr.right() - 20.0 - nb, hr.center().y - 15.0), Vec2::new(nb, 30.0));
    let hint_text = if live {
        format!("Mapping · {} · {} files", human_bytes(app.lens.bytes.load(Ordering::Relaxed)), group_digits(app.lens.files.load(Ordering::Relaxed)))
    } else {
        match *app.lens.when.lock().unwrap() {
            Some(w) => format!("Mapped {} · click for details, double-click to open", ago(w)),
            None => "Click for details · double-click to open".to_string(),
        }
    };
    let hint = p.text(Pos2::new(new_r.left() - 14.0, hr.center().y), Align2::RIGHT_CENTER, hint_text, font(12.0), if live { TEXT_DIM } else { TEXT_FAINT });
    if live {
        // a spinner dot before the progress text
        let t = ctx.input(|i| i.time) as f32;
        let c = Pos2::new(hint.left() - 12.0, hr.center().y);
        p.circle_stroke(c, 5.0, Stroke::new(1.5, white(40)));
        let a0 = t * 5.0;
        let pts: Vec<Pos2> = (0..=12).map(|i| a0 + i as f32 * 0.15).map(|a| c + Vec2::angled(a) * 5.0).collect();
        p.add(egui::Shape::line(pts, Stroke::new(1.5, tint::DISK_MAP)));
    }
    let mut x = up.right() + 8.0;
    let n_crumbs = crumbs.len();
    for (k, (label, path)) in crumbs.iter().enumerate() {
        let last = k + 1 == n_crumbs;
        let g = p.layout_no_wrap(label.clone(), if last { bold(13.0) } else { font(13.0) }, TEXT);
        let br = Rect::from_min_size(Pos2::new(x, hr.center().y - 12.0), Vec2::new(g.size().x + 16.0, 24.0));
        if br.right() > hint.left() - 100.0 {
            break;
        }
        let resp = ui.interact(br, ui.scope_id().with(("crumb", k)), Sense::click());
        if last {
            p.rect_filled(br, 6.0, white(20));
        } else if resp.hovered() {
            p.rect_filled(br, 6.0, white(10));
        }
        p.galley(Pos2::new(br.left() + 8.0, br.center().y - g.size().y / 2.0), g, if last { TEXT } else { TEXT_DIM });
        if resp.clicked() && !last {
            go_to = Some(path.clone());
        }
        x = br.right() + 2.0;
        if !last {
            p.text(Pos2::new(x + 4.0, hr.center().y), Align2::CENTER_CENTER, "›", font(13.0), TEXT_MUTED);
            x += 10.0;
        }
    }
    p.text(Pos2::new(x + 8.0, hr.center().y), Align2::LEFT_CENTER, human_bytes(total), font(13.0), TEXT_FAINT);
    if pill_button(ui, new_r, btn, kind, true).clicked() {
        act = if live { Act::Stop } else { Act::NewMap };
    }
    if live {
        // progress strip along the header's bottom edge
        let track = Rect::from_min_size(Pos2::new(hr.left(), hr.bottom() - 2.0), Vec2::new(hr.width(), 2.0));
        let t = ctx.input(|i| i.time);
        let f = ((t * 0.5).fract() as f32) * 1.4 - 0.4;
        let seg = Rect::from_min_max(Pos2::new(track.left() + track.width() * f.max(0.0), track.top()), Pos2::new(track.left() + track.width() * (f + 0.4).min(1.0), track.bottom()));
        p.with_clip_rect(track).rect_filled(seg, 1.0, tint::DISK_MAP);
    }

    // rings: the folder in the centre, its folders on the inner ring and
    // what is inside each of them on the outer ring
    let fr = Rect::from_min_max(Pos2::new(r.left(), r.bottom() - FOOTER_H), r.max);
    let body = Rect::from_min_max(Pos2::new(r.left() + 20.0, hr.bottom() + 16.0), Pos2::new(r.right() - 20.0, fr.top() - 12.0));
    let area = Rect::from_min_max(body.min, Pos2::new(body.right() - ASIDE_W - 16.0, body.bottom()));
    let now = ctx.input(|i| i.time);
    // the rings sweep in each time a folder opens
    if app.lens_morph.as_ref().is_none_or(|m| m.1 != node.path || m.0.is_some()) {
        app.lens_morph = Some((None, node.path.clone(), now));
    }
    let start = app.lens_morph.as_ref().map(|m| m.2).unwrap_or(now);
    let t = ((now - start) / SWEEP_SECS).clamp(0.0, 1.0) as f32;
    let sweep = 1.0 - (1.0 - t).powi(3);
    if t < 1.0 {
        ctx.request_repaint();
    }
    // a details card beside the rings when there is room for both
    let diam = area.height().min(560.0).min(area.width() - 16.0 - DETAILS_MIN_W);
    let (diam, card_w) = if diam >= 300.0 { (diam, (area.width() - diam - 16.0).min(340.0)) } else { (area.height().min(area.width()).min(560.0), 0.0) };
    let chart_w = area.width() - if card_w > 0.0 { card_w + 16.0 } else { 0.0 };
    let c = Pos2::new(area.left() + chart_w / 2.0, area.center().y);
    let big = diam / 2.0 - 4.0;
    let (r_hole, r1a, r1b, r2a, r2b) = (big * 0.39, big * 0.415, big * 0.757, big * 0.775, big);
    let whole = total.max(1) as f64;
    // inner ring: biggest folders first; slivers and loose files fold into "Other"
    let keep = all.iter().take(MAX_SEGMENTS).take_while(|ch| ch.bytes as f64 / whole * 360.0 >= MIN_SPAN_DEG).count();
    let folded = all.len() - keep;
    let other_bytes = node.other + all[keep..].iter().map(|ch| ch.bytes).sum::<u64>();
    let depth = node.path.strip_prefix(&root.path).map(|x| x.components().count()).unwrap_or(0);
    let mut segs: Vec<Seg> = vec![];
    let mut a = 0.0f32;
    for (i, ch) in all[..keep].iter().enumerate() {
        let span = (ch.bytes as f64 / whole * 360.0) as f32;
        segs.push(Seg { ring: 1, a0: a, a1: a + span, path: Some(ch.path.clone()), name: ch.name.clone(), bytes: ch.bytes, kids: ch.children.len(), parent: i });
        // outer ring: what's inside this folder
        let cw = ch.bytes.max(1) as f64;
        let mut b = a;
        for g in ch.children.iter().take(8) {
            let s2 = (g.bytes as f64 / cw) as f32 * span;
            if s2 < 1.2 {
                break;
            }
            segs.push(Seg { ring: 2, a0: b, a1: b + s2, path: Some(g.path.clone()), name: g.name.clone(), bytes: g.bytes, kids: g.children.len(), parent: i });
            b += s2;
        }
        a += span;
    }
    if other_bytes > 0 {
        let span = (other_bytes as f64 / whole * 360.0) as f32;
        segs.push(Seg { ring: 1, a0: a, a1: (a + span).min(360.0), path: None, name: "Other".into(), bytes: other_bytes, kids: folded, parent: usize::MAX });
    }
    // what's under the pointer
    let ptr = ctx.input(|i| i.pointer.hover_pos()).filter(|q| area.contains(*q));
    let polar = ptr.map(|q| {
        let d = q - c;
        let ang = (d.x.atan2(-d.y).to_degrees() + 360.0) % 360.0;
        (d.length(), ang)
    });
    let interact = ui.interact(Rect::from_center_size(c, Vec2::splat(big * 2.0)), ui.scope_id().with("rings"), Sense::click());
    let hit = polar.filter(|_| t >= 1.0).and_then(|(d, ang)| {
        let ring = if d >= r1a && d <= r1b { 1 } else if d >= r2a && d <= r2b { 2 } else { return None };
        segs.iter().position(|s| s.ring == ring && ang >= s.a0 && ang < s.a1)
    });
    let in_hole = polar.is_some_and(|(d, _)| d < r_hole);
    let focus = app.lens_focus.clone().filter(|f| f.starts_with(&node.path) && f != &node.path);
    let mut hovered: Option<(String, Color32, u64, Option<usize>)> = None;
    let mut toggle: Option<(PathBuf, u64)> = None;
    let mp = p.with_clip_rect(area.expand(1.0));
    for (k, s) in segs.iter().enumerate() {
        let measuring = s.path.as_ref().is_some_and(|pa| pending.is_some_and(|q| q.contains(pa)));
        let focused = s.path.is_some() && s.path == focus;
        let in_focus = focus.as_ref().is_some_and(|f| s.path.as_ref().is_some_and(|pa| pa.starts_with(f) || f.starts_with(pa)));
        let mut fill = if s.path.is_none() {
            OTHER_HUE
        } else if s.ring == 1 {
            if focused || in_focus { BLUE } else { HUES[s.parent % 4] }
        } else {
            let j = segs[..k].iter().rev().take_while(|o| o.ring == 2 && o.parent == s.parent).count();
            let base = if in_focus { ACCENT } else { HUES[(s.parent + j + 1) % 4] };
            with_alpha(base, (0.9 - j as f32 * 0.12).max(0.45))
        };
        if measuring {
            let pulse = 0.45 + 0.2 * ((now * 3.0 + k as f64).sin() as f32);
            fill = with_alpha(fill, pulse);
            ctx.request_repaint();
        }
        if hit == Some(k) {
            fill = lerp_color(fill, Color32::WHITE, 0.16);
        }
        let (ra, rb) = if s.ring == 1 { (r1a, r1b) } else { (r2a, r2b) };
        let gap = 0.5f32.min((s.a1 - s.a0) * 0.2);
        let (a0, a1) = (s.a0 * sweep + gap, s.a1 * sweep - gap);
        if a1 <= a0 {
            continue;
        }
        annulus(&mp, c, ra, rb, a0, a1, fill);
        let on = s.path.as_ref().is_some_and(|pa| app.lens_selected.iter().any(|(q, _)| q == pa));
        if on {
            annulus_outline(&mp, c, ra + 1.0, rb - 1.0, a0 + 0.3, a1 - 0.3, Stroke::new(2.0, Color32::WHITE));
        }
        // labels on wide inner segments
        if s.ring == 1 && t >= 1.0 && s.a1 - s.a0 > 18.0 {
            let mid = ((s.a0 + s.a1) / 2.0).to_radians();
            let lr = (r1a + r1b) / 2.0;
            let lp = c + Vec2::new(mid.sin(), -mid.cos()) * lr;
            let max_w = ((s.a1 - s.a0).to_radians() * lr).min(r1b - r1a + 30.0);
            text_fit(&mp, lp - Vec2::new(0.0, 1.0), Align2::CENTER_BOTTOM, &s.name, bold(11.0), Color32::WHITE, max_w);
            let size = if measuring { "measuring…".to_string() } else { human_bytes(s.bytes) };
            text_fit(&mp, lp + Vec2::new(0.0, 1.0), Align2::CENTER_TOP, &size, font(11.0), white(190), max_w);
        }
        if hit == Some(k) {
            let label = match &s.path {
                Some(pa) => tilde(pa),
                None if folded > 0 => format!("{} · {folded} smaller items and small files", tilde(&node.path)),
                None => format!("{} · small files", tilde(&node.path)),
            };
            hovered = Some((label, fill, s.bytes, (s.kids > 0 && !measuring).then_some(s.kids)));
            let opens = s.kids > 0 && !measuring;
            ctx.set_cursor_icon(if opens { egui::CursorIcon::PointingHand } else { egui::CursorIcon::Default });
            if measuring {
                interact.clone().on_hover_text("Still measuring this folder.");
            }
            if interact.double_clicked() && opens {
                match &s.path {
                    Some(pa) => go_to = Some(pa.clone()),
                    None => {
                        app.lens_skip = skip + keep;
                        app.lens_morph = None;
                    }
                }
            } else if interact.clicked() {
                match &s.path {
                    Some(pa) if s.ring == 1 && s.kids > 0 && focused => go_to = Some(pa.clone()),
                    Some(pa) => app.lens_focus = Some(pa.clone()),
                    None if folded > 0 => {
                        app.lens_skip = skip + keep;
                        app.lens_morph = None;
                    }
                    None => {}
                }
            }
        }
    }
    // the centre: this folder; clicking it goes up
    let hole_hot = in_hole && can_up;
    mp.circle_filled(c, r_hole, if hole_hot { RAISED } else { CARD });
    let name = if skip > 0 { format!("Other in {}", if depth == 0 { &root_label } else { &node.name }) } else if depth == 0 { root_label.clone() } else { node.name.clone() };
    text_fit(&mp, c - Vec2::new(0.0, 15.0), Align2::CENTER_BOTTOM, &name, ty::caption(), ty::caption_color(), r_hole * 1.6);
    mp.text(c, Align2::CENTER_CENTER, human_bytes(total), ty::number(), TEXT);
    if can_up {
        mp.text(c + Vec2::new(0.0, 15.0), Align2::CENTER_TOP, "click to go up", ty::caption(), ty::caption_color());
    }
    if hole_hot {
        ctx.set_cursor_icon(egui::CursorIcon::PointingHand);
        if interact.clicked() {
            if skip > 0 {
                unskip = true;
            } else {
                go_to = node.path.parent().map(Path::to_path_buf);
            }
        }
    }
    if in_hole || hit.is_none() && polar.is_some_and(|(d, _)| d > big) {
        if interact.clicked() && !in_hole {
            app.lens_focus = None;
        }
    }

    // details of the focused folder (or of this folder)
    if card_w > 0.0 {
        let subject = focus.as_ref().and_then(|f| scan::find(root, f)).unwrap_or(node);
        let is_node = subject.path == node.path;
        let kids: Vec<&Node> = subject.children.iter().take(4).collect();
        let selectable = !live && !is_node && !scan::is_essential(&subject.path);
        let on = app.lens_selected.iter().any(|(q, _)| q == &subject.path);
        let pad = sp::L;
        let rows_h = kids.len() as f32 * sp::LEGEND_H;
        let h = pad + 16.0 + sp::XS + 14.0 + sp::L + 34.0 + sp::XS + 14.0 + if kids.is_empty() { 0.0 } else { sp::L + rows_h } + sp::L + 30.0 + if !is_node { sp::S + 30.0 } else { 0.0 } + pad;
        let cr = Rect::from_min_size(Pos2::new(area.right() - card_w, (c.y - diam / 2.0).max(area.top())), Vec2::new(card_w, h.min(area.height())));
        glass_card(&p, cr, R_CARD, 0.0, false);
        let x = cr.left() + pad;
        let w = cr.width() - pad * 2.0;
        let mut y = cr.top() + pad;
        text_fit(&p, Pos2::new(x, y), Align2::LEFT_TOP, if is_node { &name } else { &subject.name }, ty::title(), TEXT, w);
        y += 16.0 + sp::XS;
        let sub = format!("{} · {} items", tilde(&subject.path), group_digits(subject.items));
        text_fit(&p, Pos2::new(x, y), Align2::LEFT_TOP, &sub, ty::caption(), ty::caption_color(), w);
        y += 14.0 + sp::L;
        p.text(Pos2::new(x, y), Align2::LEFT_TOP, human_bytes(subject.bytes), ty::hero(), TEXT);
        y += 34.0 + sp::XS;
        let share = if is_node {
            format!("{} folders · click one for details", subject.children.len())
        } else {
            format!("{}% of {}", pct(subject.bytes, total), name)
        };
        p.text(Pos2::new(x, y), Align2::LEFT_TOP, share, ty::caption(), ty::caption_color());
        y += 14.0;
        if !kids.is_empty() {
            y += sp::L;
            for g in &kids {
                legend_row(&p, Rect::from_min_size(Pos2::new(cr.left(), y), Vec2::new(cr.width() - 4.0, sp::LEGEND_H)), ACCENT, &g.name, &human_bytes(g.bytes), TEXT_SOFT);
                y += sp::LEGEND_H;
            }
        }
        y += sp::L;
        let half = (w - sp::S) / 2.0;
        let opens = !is_node && !subject.children.is_empty();
        let (r_a, r_b) = (Rect::from_min_size(Pos2::new(x, y), Vec2::new(half, 30.0)), Rect::from_min_size(Pos2::new(x + half + sp::S, y), Vec2::new(half, 30.0)));
        if opens {
            if pill_button(ui, r_a, "Open", BTN_SECONDARY, true).clicked() {
                go_to = Some(subject.path.clone());
            }
        }
        let finder = if opens { r_b } else { Rect::from_min_size(Pos2::new(x, y), Vec2::new(w, 30.0)) };
        if pill_button(ui, finder, "Show in Finder", BTN_SECONDARY, true).clicked() {
            crate::kit::macos::reveal(&subject.path);
        }
        if !is_node {
            y += 30.0 + sp::S;
            let br = Rect::from_min_size(Pos2::new(x, y), Vec2::new(w, 30.0));
            if selectable {
                if pill_button(ui, br, if on { "Remove from the list" } else { "Add to the list" }, if on { BTN_SECONDARY } else { BTN_PRIMARY }, true).clicked() {
                    toggle = Some((subject.path.clone(), subject.bytes));
                }
            } else {
                let why = if live { "You can pick what to remove when mapping finishes." } else { "Your Mac needs this folder, so it can't be removed." };
                text_fit(&p, Pos2::new(x, br.center().y), Align2::LEFT_CENTER, why, ty::caption(), ty::caption_color(), w);
            }
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
    if let Some(target) = go_to {
        app.lens_focus = None;
        app.lens_path = target;
        app.lens_skip = 0;
        ctx.request_repaint();
    } else if unskip {
        app.lens_skip = 0;
        app.lens_morph = None;
        ctx.request_repaint();
    }

    // aside: the list to remove
    let ar = Rect::from_min_max(Pos2::new(area.right() + 16.0, body.top()), body.max);
    section_header(&p, ar.min, Align2::LEFT_TOP, "TO REMOVE");
    let total: u64 = app.lens_selected.iter().map(|x| x.1).sum();
    let list_top = ar.top() + 24.0;
    let list_bottom = ar.bottom() - 34.0 - 12.0 - 34.0;
    if live {
        paragraph(&p, Pos2::new(ar.left(), list_top), "You can pick what to remove when mapping finishes. Finished folders open already.", font(12.0), TEXT_FAINT, ar.width());
    } else if app.lens_selected.is_empty() {
        paragraph(&p, Pos2::new(ar.left(), list_top), "Nothing yet. Click a folder, then Add to the list.", font(12.0), TEXT_FAINT, ar.width());
    }
    let mut drop: Option<usize> = None;
    let rel = |pa: &Path| -> String {
        pa.strip_prefix(&root.path).map(|x| x.components().map(|c| c.as_os_str().to_string_lossy().into_owned()).collect::<Vec<_>>().join(" › ")).unwrap_or_else(|_| tilde(pa))
    };
    let max_rows = if live { 0 } else { ((list_bottom - list_top) / 32.0).floor().max(0.0) as usize };
    let picked = app.lens_selected.clone();
    for (i, (pa, b)) in picked.iter().enumerate().take(max_rows) {
        let rr = Rect::from_min_size(Pos2::new(ar.left(), list_top + i as f32 * 32.0), Vec2::new(ar.width(), 30.0));
        p.rect_filled(rr, R_SMALL, white(13));
        p.rect_filled(Rect::from_center_size(Pos2::new(rr.left() + 12.0, rr.center().y), Vec2::splat(8.0)), 2.0, hue_of(app, &root.path, pa));
        let xr = Rect::from_center_size(Pos2::new(rr.right() - 14.0, rr.center().y), Vec2::splat(18.0));
        let sr = p.text(Pos2::new(xr.left() - 6.0, rr.center().y), Align2::RIGHT_CENTER, human_bytes(*b), font(13.0), TEXT_SOFT);
        text_fit(&p, Pos2::new(rr.left() + 24.0, rr.center().y), Align2::LEFT_CENTER, &rel(pa), font(13.0), TEXT, sr.left() - rr.left() - 32.0);
        let xresp = ui.interact(xr, ui.scope_id().with(("unpick", i)), Sense::click());
        p.text(xr.center(), Align2::CENTER_CENTER, "×", font(15.0), if xresp.hovered() { TEXT } else { TEXT_FAINT });
        if xresp.on_hover_text("Remove from the list").clicked() {
            drop = Some(i);
        }
    }
    if !live && app.lens_selected.len() > max_rows {
        p.text(Pos2::new(ar.left(), list_top + max_rows as f32 * 32.0 + 4.0), Align2::LEFT_TOP, format!("and {} more", app.lens_selected.len() - max_rows), font(12.0), TEXT_FAINT);
    }
    if let Some(i) = drop {
        app.lens_selected.remove(i);
    }
    if !live {
        let g = p.layout_no_wrap(human_bytes(total), bold(22.0), TEXT);
        let gw = g.size().x;
        let ty = ar.bottom() - 34.0 - 12.0 - g.size().y;
        p.galley(Pos2::new(ar.left(), ty), g, TEXT);
        p.text(Pos2::new(ar.left() + gw + 6.0, ty + 22.0 * 0.82), Align2::LEFT_BOTTOM, "selected", font(13.0), TEXT_FAINT);
    }
    let rb = Rect::from_min_max(Pos2::new(ar.left(), ar.bottom() - 34.0), ar.max);
    if pill_button(ui, rb, "Review and remove", BTN_PRIMARY, !live && !app.lens_selected.is_empty()).clicked() {
        app.lens_review = true;
    }

    // footer: what's under the pointer, or what's being measured
    p.rect_filled(fr, CornerRadius { se: 12, ..Default::default() }, FOOTER);
    hairline(&p, fr.left_top(), fr.right_top());
    match hovered {
        Some((path, hue, bytes, kids)) => {
            p.rect_filled(Rect::from_center_size(Pos2::new(fr.left() + 25.0, fr.center().y), Vec2::splat(10.0)), 3.0, hue);
            let mut parent = if depth == 0 { root_label.clone() } else { node.name.clone() };
            if skip > 0 {
                parent = format!("Other in {parent}");
            }
            let info = format!("{} · {}% of {}{}", human_bytes(bytes), pct(bytes, total), parent, kids.map(|k| format!(" · {k} items")).unwrap_or_default());
            let ir = p.text(Pos2::new(fr.right() - 20.0, fr.center().y), Align2::RIGHT_CENTER, info, font(13.0), TEXT_DIM);
            text_fit(&p, Pos2::new(fr.left() + 40.0, fr.center().y), Align2::LEFT_CENTER, &path, mono(12.0), TEXT_SOFT, ir.left() - fr.left() - 56.0);
        }
        None if live => {
            let cur = app.lens.current.lock().unwrap().clone();
            p.text(Pos2::new(fr.left() + 20.0, fr.center().y), Align2::LEFT_CENTER, "Measuring", font(12.0), TEXT_FAINT);
            text_fit(&p, Pos2::new(fr.left() + 92.0, fr.center().y), Align2::LEFT_CENTER, &tilde(Path::new(&cur)), mono(12.0), TEXT_SOFT, fr.width() - 112.0);
        }
        None => {
            p.text(Pos2::new(fr.left() + 40.0, fr.center().y), Align2::LEFT_CENTER, "Point at a ring to see details", mono(12.0), TEXT_FAINT);
        }
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
    p.rect_filled(m, 14.0, RAISED);
    p.rect_stroke(m, 14.0, Stroke::new(1.0, white(31)), StrokeKind::Inside);
    let total: u64 = app.lens_selected.iter().map(|x| x.1).sum();
    let title = format!("Move {} item{} ({}) to the Trash?", n, if n == 1 { "" } else { "s" }, human_bytes(total));
    text_fit(&p, m.min + Vec2::new(22.0, 22.0), Align2::LEFT_TOP, &title, bold(17.0), TEXT, m.width() - 44.0);
    text_fit(&p, m.min + Vec2::new(22.0, 48.0), Align2::LEFT_TOP, "You can put them back from the Trash. System folders and code projects are skipped.", font(13.0), TEXT_DIM, m.width() - 44.0);
    let list = Rect::from_min_size(m.min + Vec2::new(22.0, 82.0), Vec2::new(m.width() - 44.0, rows as f32 * 34.0 + 8.0));
    p.rect_filled(list, 10.0, Color32::from_black_alpha(46));
    let mut drop: Option<usize> = None;
    for (i, (path, b)) in app.lens_selected.iter().enumerate().take(rows) {
        let rr = Rect::from_min_size(Pos2::new(list.left(), list.top() + 4.0 + i as f32 * 34.0), Vec2::new(list.width(), 34.0));
        icon(&p, Icon::Folder, Rect::from_center_size(Pos2::new(rr.left() + 20.0, rr.center().y), Vec2::splat(16.0)), ACCENT);
        let xr = Rect::from_center_size(Pos2::new(rr.right() - 20.0, rr.center().y), Vec2::splat(22.0));
        let sr = p.text(Pos2::new(xr.left() - 8.0, rr.center().y), Align2::RIGHT_CENTER, human_bytes(*b), font(13.0), TEXT_SOFT);
        text_fit(&p, Pos2::new(rr.left() + 38.0, rr.center().y), Align2::LEFT_CENTER, &tilde(path), mono(12.0), TEXT, sr.left() - rr.left() - 48.0);
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

/// One segment of the rings: angles in degrees, clockwise from the top.
struct Seg {
    ring: u8,
    a0: f32,
    a1: f32,
    path: Option<PathBuf>,
    name: String,
    bytes: u64,
    kids: usize,
    /// Index of the inner-ring folder this belongs to.
    parent: usize,
}

fn ring_points(c: Pos2, r: f32, a0: f32, a1: f32) -> impl Iterator<Item = Pos2> {
    let n = ((a1 - a0) / 2.0).ceil().max(2.0) as usize;
    (0..=n).map(move |i| {
        let a = (a0 + (a1 - a0) * i as f32 / n as f32).to_radians();
        c + Vec2::new(a.sin(), -a.cos()) * r
    })
}

/// A filled ring segment between radii `ra`..`rb` and angles `a0`..`a1`.
fn annulus(p: &egui::Painter, c: Pos2, ra: f32, rb: f32, a0: f32, a1: f32, fill: Color32) {
    let mut mesh = egui::Mesh::default();
    for (pi, po) in ring_points(c, ra, a0, a1).zip(ring_points(c, rb, a0, a1)) {
        let i = mesh.vertices.len() as u32;
        mesh.colored_vertex(pi, fill);
        mesh.colored_vertex(po, fill);
        if i >= 2 {
            mesh.add_triangle(i - 2, i - 1, i);
            mesh.add_triangle(i - 1, i, i + 1);
        }
    }
    p.add(egui::Shape::mesh(mesh));
}

fn annulus_outline(p: &egui::Painter, c: Pos2, ra: f32, rb: f32, a0: f32, a1: f32, stroke: Stroke) {
    let mut pts: Vec<Pos2> = ring_points(c, rb, a0, a1).collect();
    pts.extend(ring_points(c, ra, a0, a1).collect::<Vec<_>>().into_iter().rev());
    p.add(egui::Shape::closed_line(pts, stroke));
}
