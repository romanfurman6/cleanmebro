//! Disk Map screens: choose what to map (with live progress while it runs)
//! → drill-down treemap with a "To remove" list → review and move to Trash.

use super::scan::{self, Node};
use super::Lens as App;
use crate::core::tool::Cx;
use crate::kit::glass::*;
use egui::{Align2, Color32, CornerRadius, Pos2, Rect, Sense, Stroke, StrokeKind, Ui, Vec2};
use std::path::{Path, PathBuf};
use std::sync::atomic::Ordering;

/// Block colours, one per top-level folder (biggest first); the rest share
/// the last, neutral one.
const HUES: [Color32; 9] = [
    Color32::from_rgb(0x2E, 0x9E, 0x8F),
    Color32::from_rgb(0x2F, 0x6F, 0xD0),
    Color32::from_rgb(0xC7, 0x78, 0x2A),
    Color32::from_rgb(0x81, 0x59, 0xC9),
    Color32::from_rgb(0xB8, 0x4C, 0x80),
    Color32::from_rgb(0x4C, 0x8A, 0x3F),
    Color32::from_rgb(0xA8, 0x82, 0x3A),
    Color32::from_rgb(0x5A, 0x5A, 0x60),
    Color32::from_rgb(0x48, 0x48, 0x4C),
];
const OTHER_HUE: Color32 = Color32::from_rgb(0x48, 0x48, 0x4C);
/// Zoom animation length, seconds.
const MORPH_SECS: f64 = 0.35;
const HEADER_H: f32 = 57.0;
const FOOTER_H: f32 = 42.0;
const ASIDE_W: f32 = 260.0;

pub fn page(app: &mut App, ui: &mut Ui, r: Rect, cx: &mut Cx) {
    let shared = app.lens.clone();
    let now = ui.ctx().input(|i| i.time);
    if shared.running.load(Ordering::SeqCst) {
        // the map grows while mapping; rebuild it a few times a second
        if app.lens_live.as_ref().is_none_or(|l| now - l.0 > 0.25) {
            app.lens_live = scan::partial(&shared).map(|(n, pending)| (now, n, pending));
        }
        ui.ctx().request_repaint_after(std::time::Duration::from_millis(120));
        match app.lens_live.take() {
            Some(live) => {
                let act = results(app, ui, r, &live.1, Some(&live.2));
                app.lens_live = Some(live);
                if act == Act::Stop {
                    shared.stop.store(true, Ordering::SeqCst);
                }
            }
            None => start(app, ui, r, true),
        }
        return;
    }
    app.lens_live = None;
    let guard = shared.result.lock().unwrap();
    let Some(root) = guard.as_ref() else {
        drop(guard);
        start(app, ui, r, false);
        return;
    };
    let act = results(app, ui, r, root, None);
    drop(guard);
    if act == Act::NewMap {
        *shared.result.lock().unwrap() = None;
        app.lens_selected.clear();
        app.lens_morph = None;
        app.lens_anim.clear();
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

fn start(app: &mut App, ui: &mut Ui, r: Rect, running: bool) {
    let ctx = ui.ctx().clone();
    let p = ui.painter().clone();
    page_header(&p, r, "Disk Map", "See what takes space and remove it by hand", TEXT_DIM);
    let top = r.top() + PAGE_HEADER_H + 28.0;
    let col_w = (r.width() - 48.0 - 24.0) / 2.0;
    let lx = r.left() + 24.0;
    let rx = lx + col_w + 24.0;

    // left: choose what to map
    section_header(&p, Pos2::new(lx, top), Align2::LEFT_TOP, "START · CHOOSE WHAT TO MAP");
    let h = home();
    let places = [
        (Icon::Home, "Home folder", h.clone()),
        (Icon::Disk, "Macintosh HD", PathBuf::from("/")),
        (Icon::Map, "Applications", PathBuf::from("/Applications")),
        (Icon::Folder, "Library", h.join("Library")),
        (Icon::Down, "Downloads", h.join("Downloads")),
    ];
    let row_h = 46.0;
    let card = Rect::from_min_size(Pos2::new(lx, top + 22.0), Vec2::new(col_w, places.len() as f32 * row_h + 8.0));
    glass_card(&p, card, R_CARD, 0.0, false);
    let mut label = "Home folder";
    for (k, (glyph, name, path)) in places.iter().enumerate() {
        let rr = Rect::from_min_size(Pos2::new(card.left() + 4.0, card.top() + 4.0 + k as f32 * row_h), Vec2::new(card.width() - 8.0, row_h));
        let on = app.lens_root == *path;
        if on {
            label = name;
        }
        let resp = ui.interact(rr, ui.scope_id().with(("place", k)), if running { Sense::hover() } else { Sense::click() });
        if on {
            p.rect_filled(rr, R_BUTTON, with_alpha(BLUE, 0.16));
        } else if resp.hovered() && !running {
            p.rect_filled(rr, R_BUTTON, white(10));
        }
        let rc = Pos2::new(rr.left() + 20.0, rr.center().y);
        if on {
            p.circle_filled(rc, 8.0, BLUE);
            p.circle_filled(rc, 3.0, Color32::WHITE);
        } else {
            p.circle_stroke(rc, 7.25, Stroke::new(1.5, white(76)));
        }
        icon(&p, *glyph, Rect::from_center_size(Pos2::new(rr.left() + 48.0, rr.center().y), Vec2::splat(17.0)), ACCENT);
        p.text(Pos2::new(rr.left() + 68.0, rr.top() + 7.0), Align2::LEFT_TOP, *name, font(13.0), TEXT);
        p.text(Pos2::new(rr.left() + 68.0, rr.top() + 25.0), Align2::LEFT_TOP, tilde(path), mono(11.0), TEXT_FAINT);
        if resp.clicked() && !running {
            app.lens_root = path.clone();
        }
    }
    let mut y = card.bottom() + 12.0;
    let note = if app.lens_root == Path::new("/") {
        "Mapping the whole disk takes a few minutes. Give CleanMeBro Full Disk Access in System Settings for complete results."
    } else {
        "Folders the system needs can be browsed but not selected."
    };
    y += paragraph(&p, Pos2::new(lx, y), note, font(12.0), TEXT_FAINT, col_w) + 14.0;
    let bl = format!("Map {}", label);
    let bw = button_width(&p, &bl, BTN_PRIMARY).max(140.0);
    if pill_button(ui, Rect::from_min_size(Pos2::new(lx, y), Vec2::new(bw, 34.0)), &bl, BTN_PRIMARY, !running).clicked() {
        app.lens_path = app.lens_root.clone();
        app.lens_selected.clear();
        app.lens_msg = None;
        app.lens_morph = None;
        scan::start(app.lens.clone(), app.lens_root.clone(), ctx.clone());
    }
    if let Some(msg) = &app.lens_msg {
        text_fit(&p, Pos2::new(lx, y + 48.0), Align2::LEFT_TOP, msg, font(12.0), GREEN, col_w);
    }

    // right: progress while mapping
    section_header(&p, Pos2::new(rx, top), Align2::LEFT_TOP, "WHILE MAPPING");
    let pr = Rect::from_min_size(Pos2::new(rx, top + 22.0), Vec2::new(col_w, if running { 150.0 } else { 86.0 }));
    glass_card(&p, pr, R_CARD, 0.0, false);
    if running {
        let bytes = app.lens.bytes.load(Ordering::Relaxed);
        let files = app.lens.files.load(Ordering::Relaxed);
        let g = p.layout_no_wrap(human_bytes(bytes), bold(26.0), TEXT);
        let gw = g.size().x;
        p.galley(Pos2::new(pr.left() + 16.0, pr.top() + 14.0), g, TEXT);
        p.text(Pos2::new(pr.left() + 24.0 + gw, pr.top() + 33.0), Align2::LEFT_CENTER, format!("measured · {} files", group_digits(files)), font(13.0), TEXT_DIM);
        // indeterminate bar: a segment sliding across the track
        let track = Rect::from_min_size(Pos2::new(pr.left() + 16.0, pr.top() + 62.0), Vec2::new(pr.width() - 32.0, 6.0));
        p.rect_filled(track, 3.0, white(20));
        let t = ctx.input(|i| i.time);
        let f = ((t * 0.6).fract() as f32) * 1.35 - 0.35;
        let seg = Rect::from_min_max(Pos2::new(track.left() + track.width() * f.max(0.0), track.top()), Pos2::new(track.left() + track.width() * (f + 0.35).min(1.0), track.bottom()));
        p.with_clip_rect(track).rect_filled(seg, 3.0, tint::DISK_MAP);
        let cur = app.lens.current.lock().unwrap().clone();
        text_fit(&p, Pos2::new(pr.left() + 16.0, pr.top() + 80.0), Align2::LEFT_TOP, &tilde(Path::new(&cur)), mono(11.0), TEXT_FAINT, pr.width() - 32.0);
        let sw = button_width(&p, "Stop", BTN_SECONDARY);
        if pill_button(ui, Rect::from_min_size(Pos2::new(pr.left() + 16.0, pr.bottom() - 44.0), Vec2::new(sw, 28.0)), "Stop", BTN_SECONDARY, true).clicked() {
            app.lens.stop.store(true, Ordering::SeqCst);
        }
        ctx.request_repaint();
    } else {
        p.text(Pos2::new(pr.left() + 16.0, pr.top() + 16.0), Align2::LEFT_TOP, "Not started", bold(13.0), TEXT_DIM);
        paragraph(&p, Pos2::new(pr.left() + 16.0, pr.top() + 38.0), "The map shows every folder as a block sized by what it holds. Click a folder to zoom in, tick a corner to pick it for removal.", font(12.0), TEXT_FAINT, pr.width() - 32.0);
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
    HUES[i.min(HUES.len() - 1)]
}

/// Where a block is drawn this frame: it glides from where it was towards
/// `target`; a new block grows from its centre.
fn glide(app: &mut App, key: &Path, target: Rect, snap: bool, dt: f32, moving: &mut bool) -> Rect {
    let prev = if snap { None } else { app.lens_anim.get(key).copied() };
    let from = prev.unwrap_or_else(|| if snap { target } else { Rect::from_center_size(target.center(), Vec2::ZERO) });
    let k = 1.0 - (-dt * 10.0).exp();
    let shown = lerp_rect(from, target, k);
    if (shown.min - target.min).length() + (shown.max - target.max).length() > 0.5 {
        *moving = true;
    } else {
        return target;
    }
    shown
}

fn pct(part: u64, whole: u64) -> u64 {
    if whole == 0 {
        0
    } else {
        ((part as f64 / whole as f64) * 100.0).round() as u64
    }
}

/// Map a rect laid out in `area` so that `from` lands on `to`.
fn affine(b: Rect, from: Rect, to: Rect) -> Rect {
    let sx = to.width() / from.width().max(1.0);
    let sy = to.height() / from.height().max(1.0);
    Rect::from_min_max(
        Pos2::new(to.left() + (b.left() - from.left()) * sx, to.top() + (b.top() - from.top()) * sy),
        Pos2::new(to.left() + (b.right() - from.left()) * sx, to.top() + (b.bottom() - from.top()) * sy),
    )
}

fn lerp_rect(a: Rect, b: Rect, t: f32) -> Rect {
    Rect::from_min_max(a.min + (b.min - a.min) * t, a.max + (b.max - a.max) * t)
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
    let mut go_to: Option<PathBuf> = None;
    let root_label = if root.path == home() { "Home".to_string() } else { root.name.clone() };

    // header: up, breadcrumbs, size, then the hint and New map, or progress and Stop
    let hr = Rect::from_min_size(r.min, Vec2::new(r.width(), HEADER_H));
    hairline(&p, hr.left_bottom(), hr.right_bottom());
    let up = Rect::from_min_size(Pos2::new(hr.left() + 20.0, hr.center().y - 14.0), Vec2::splat(28.0));
    let can_up = node.path != root.path;
    let ur = ui.interact(up, ui.scope_id().with("map-up"), if can_up { Sense::click() } else { Sense::hover() });
    p.rect_filled(up, R_SMALL, white(if !can_up { 8 } else if ur.hovered() { 32 } else { 20 }));
    icon(&p, Icon::Back, up.shrink(7.0), if can_up { TEXT_SOFT } else { TEXT_MUTED });
    if ur.on_hover_text("Up one level").clicked() && can_up {
        go_to = node.path.parent().map(Path::to_path_buf);
    }
    let mut crumbs: Vec<(String, PathBuf)> = vec![(root_label.clone(), root.path.clone())];
    if let Ok(rel) = node.path.strip_prefix(&root.path) {
        let mut acc = root.path.clone();
        for c in rel.components() {
            acc.push(c);
            crumbs.push((c.as_os_str().to_string_lossy().into_owned(), acc.clone()));
        }
    }
    let (btn, kind) = if live { ("Stop", BTN_SECONDARY) } else { ("New map", BTN_SECONDARY) };
    let nb = button_width(&p, btn, kind);
    let new_r = Rect::from_min_size(Pos2::new(hr.right() - 20.0 - nb, hr.center().y - 15.0), Vec2::new(nb, 30.0));
    let hint_text = if live {
        format!("Mapping · {} · {} files", human_bytes(app.lens.bytes.load(Ordering::Relaxed)), group_digits(app.lens.files.load(Ordering::Relaxed)))
    } else {
        "Click a folder to zoom in · tick a corner to select".to_string()
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
    p.text(Pos2::new(x + 8.0, hr.center().y), Align2::LEFT_CENTER, human_bytes(node.bytes), font(13.0), TEXT_FAINT);
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

    // treemap
    let fr = Rect::from_min_max(Pos2::new(r.left(), r.bottom() - FOOTER_H), r.max);
    let body = Rect::from_min_max(Pos2::new(r.left() + 20.0, hr.bottom() + 16.0), Pos2::new(r.right() - 20.0, fr.top() - 12.0));
    let area = Rect::from_min_max(body.min, Pos2::new(body.right() - ASIDE_W - 16.0, body.bottom()));
    let mut vals: Vec<f64> = node.children.iter().map(|c| c.bytes as f64).collect();
    if node.other > 0 {
        vals.push(node.other as f64);
    }
    let rects = scan::squarify(&vals, area);
    let now = ctx.input(|i| i.time);
    let morph = app.lens_morph.clone().and_then(|(into, focus, start)| {
        let t = ((now - start) / MORPH_SECS).clamp(0.0, 1.0) as f32;
        if t >= 1.0 {
            return None;
        }
        let ease = 1.0 - (1.0 - t).powi(3);
        let (from, to) = match into {
            Some(clicked) => (area, clicked),
            None => {
                let k = node.children.iter().position(|c| focus.starts_with(&c.path))?;
                (rects[k], area)
            }
        };
        Some((from, to, ease))
    });
    if morph.is_some() {
        ctx.request_repaint();
    } else {
        app.lens_morph = None;
    }
    // blocks glide to new sizes (the map growing while mapping); jump
    // straight there when zooming, on the first draw and when resized
    let snap = morph.is_some() || app.lens_anim.is_empty() || app.lens_anim_area != area;
    let dt = ctx.input(|i| i.stable_dt).clamp(0.0, 0.1);
    let mut moving = false;
    let mut drawn: std::collections::HashMap<PathBuf, Rect> = Default::default();
    let mp = p.with_clip_rect(area.expand(1.0));
    let depth = node.path.strip_prefix(&root.path).map(|x| x.components().count()).unwrap_or(0);
    let n = vals.len();
    let mut hovered: Option<(String, Color32, u64, Option<usize>)> = None;
    let mut toggle: Option<(PathBuf, u64)> = None;
    for k in 0..n {
        let base = rects[k];
        let child = node.children.get(k);
        let key = child.map(|c| c.path.clone()).unwrap_or_else(|| node.path.join("\u{0}other"));
        let target = match morph {
            Some((from, to, e)) => lerp_rect(affine(base, from, to), base, e),
            None => base,
        };
        let shown = glide(app, &key, target, snap, dt, &mut moving);
        drawn.insert(key, shown);
        let br = shown.shrink(2.0);
        if br.width() < 1.0 || br.height() < 1.0 {
            continue;
        }
        let (name, bytes, path) = match child {
            Some(c) => (c.name.clone(), c.bytes, Some(c.path.clone())),
            None => ("Other items".to_string(), node.other, None),
        };
        let measuring = path.as_ref().is_some_and(|pa| pending.is_some_and(|q| q.contains(pa)));
        let hue = match &path {
            Some(pa) => hue_of(app, &root.path, pa),
            None => OTHER_HUE,
        };
        let shade = if depth == 0 { 0.75 } else { 0.45 + 0.4 * (1.0 - k as f32 / (n.max(2) - 1) as f32) };
        let id = ui.scope_id().with(("blk", &name, k));
        let kids = child.map(|c| c.children.len()).unwrap_or(0);
        let clickable = path.is_some() && morph.is_none() && !measuring;
        let resp = ui.interact(br, id, if clickable { Sense::click() } else { Sense::hover() });
        let hov = resp.hovered();
        let mut fill = lerp_color(BG, hue, if measuring { shade * 0.55 } else { shade });
        if hov && clickable {
            fill = lerp_color(fill, Color32::WHITE, 0.14);
        }
        mp.rect_filled(br, 8.0, fill);
        if measuring {
            // a soft sheen sweeping across blocks still being measured
            let w = br.width().max(br.height());
            let f = ((now * 0.7 + k as f64 * 0.13).fract() as f32) * (br.width() + w) - w * 0.5;
            let sheen = Rect::from_min_size(Pos2::new(br.left() + f - w * 0.25, br.top()), Vec2::new(w * 0.5, br.height()));
            mp.with_clip_rect(br.intersect(area.expand(1.0))).rect_filled(sheen, 0.0, white(14));
            mp.rect_stroke(br, 8.0, Stroke::new(1.0, with_alpha(hue, 0.6)), StrokeKind::Inside);
        }
        let selectable = !live && path.as_ref().is_some_and(|pa| !scan::is_essential(pa));
        let on = path.as_ref().is_some_and(|pa| app.lens_selected.iter().any(|(q, _)| q == pa));
        if on {
            mp.rect_stroke(br, 8.0, Stroke::new(3.0, Color32::WHITE), StrokeKind::Inside);
        }
        if br.width() > 64.0 && br.height() > 46.0 {
            let fs = if br.width() > 220.0 && br.height() > 120.0 { 16.0 } else { 12.0 };
            let tw = br.width() - 20.0 - if selectable { 22.0 } else { 0.0 };
            let mut ty = br.top() + 10.0;
            ty += text_fit(&mp, Pos2::new(br.left() + 10.0, ty), Align2::LEFT_TOP, &name, bold(fs), Color32::WHITE, tw).height() + 3.0;
            let size = if measuring { format!("{} so far", human_bytes(bytes)) } else { format!("{} · {}%", human_bytes(bytes), pct(bytes, node.bytes)) };
            ty += text_fit(&mp, Pos2::new(br.left() + 10.0, ty), Align2::LEFT_TOP, &size, font(12.0), white(217), br.width() - 20.0).height() + 3.0;
            let more = if measuring {
                Some("measuring…".to_string())
            } else if kids > 0 {
                Some(format!("{kids} items · click to open"))
            } else {
                None
            };
            if let Some(more) = more.filter(|_| ty + 14.0 < br.bottom()) {
                text_fit(&mp, Pos2::new(br.left() + 10.0, ty), Align2::LEFT_TOP, &more, font(11.0), white(166), br.width() - 20.0);
            }
        }
        // selection box in the corner
        let mut box_clicked = false;
        if selectable && morph.is_none() && br.width() > 30.0 && br.height() > 30.0 {
            let bx = Rect::from_min_size(Pos2::new(br.right() - 24.0, br.top() + 6.0), Vec2::splat(18.0));
            let bresp = ui.interact(bx, id.with("box"), Sense::click());
            if on {
                mp.rect_filled(bx, 5.0, BLUE);
                mp.rect_stroke(bx, 5.0, Stroke::new(1.5, Color32::WHITE), StrokeKind::Inside);
                icon(&mp, Icon::Check, bx.shrink(4.0), Color32::WHITE);
            } else {
                mp.rect_filled(bx, 5.0, Color32::from_black_alpha(64));
                mp.rect_stroke(bx, 5.0, Stroke::new(1.5, white(if hov || bresp.hovered() { 230 } else { 115 })), StrokeKind::Inside);
            }
            box_clicked = bresp.on_hover_text(if on { "Remove from the list" } else { "Add to the list" }).clicked();
        }
        if hov || (morph.is_none() && ui.rect_contains_pointer(br)) {
            hovered = Some((path.as_ref().map(|pa| tilde(pa)).unwrap_or_else(|| format!("{} · small files", tilde(&node.path))), hue, bytes, (kids > 0 && !measuring).then_some(kids)));
        }
        if let Some(pa) = &path {
            if box_clicked {
                toggle = Some((pa.clone(), bytes));
            } else if resp.clicked() {
                if kids > 0 {
                    app.lens_morph = Some((Some(base), PathBuf::new(), now));
                    go_to = Some(pa.clone());
                } else if selectable {
                    toggle = Some((pa.clone(), bytes));
                }
            }
            if hov && clickable && kids > 0 {
                ctx.set_cursor_icon(egui::CursorIcon::ZoomIn);
            } else if hov && selectable {
                ctx.set_cursor_icon(egui::CursorIcon::PointingHand);
            }
            if hov && measuring {
                resp.on_hover_text("Still measuring this folder.");
            } else if hov && !live && !selectable {
                resp.on_hover_text("Your Mac needs this folder, so it can't be removed.");
            }
        }
    }
    app.lens_anim = drawn;
    app.lens_anim_area = area;
    if moving {
        ctx.request_repaint();
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
        if app.lens_morph.as_ref().is_none_or(|m| m.0.is_none()) {
            // going up: zoom out from the folder we were in
            app.lens_morph = Some((None, app.lens_path.clone(), now));
        }
        app.lens_path = target;
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
        paragraph(&p, Pos2::new(ar.left(), list_top), "Nothing yet. Tick the corner of a block to add it here.", font(12.0), TEXT_FAINT, ar.width());
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
            let parent = if depth == 0 { root_label.clone() } else { node.name.clone() };
            let info = format!("{} · {}% of {}{}", human_bytes(bytes), pct(bytes, node.bytes), parent, kids.map(|k| format!(" · {k} items")).unwrap_or_default());
            let ir = p.text(Pos2::new(fr.right() - 20.0, fr.center().y), Align2::RIGHT_CENTER, info, font(13.0), TEXT_DIM);
            text_fit(&p, Pos2::new(fr.left() + 40.0, fr.center().y), Align2::LEFT_CENTER, &path, mono(12.0), TEXT_SOFT, ir.left() - fr.left() - 56.0);
        }
        None if live => {
            let cur = app.lens.current.lock().unwrap().clone();
            p.text(Pos2::new(fr.left() + 20.0, fr.center().y), Align2::LEFT_CENTER, "Measuring", font(12.0), TEXT_FAINT);
            text_fit(&p, Pos2::new(fr.left() + 92.0, fr.center().y), Align2::LEFT_CENTER, &tilde(Path::new(&cur)), mono(12.0), TEXT_SOFT, fr.width() - 112.0);
        }
        None => {
            p.text(Pos2::new(fr.left() + 40.0, fr.center().y), Align2::LEFT_CENTER, "Hover a block to see details", mono(12.0), TEXT_FAINT);
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
            ("Projects", 64.2, vec![("web-app", 22.0), ("cleanmebro", 18.0), ("api-server", 14.0), ("notes", 10.2)]),
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
            scan::LiveChild { name: c.name.clone(), path: c.path.clone(), bytes: Arc::new(AtomicU64::new(part)), done: done.then(|| c.clone()) }
        })
        .collect();
    app.lens.bytes.store(live.iter().map(|l| l.bytes.load(Ordering::Relaxed)).sum(), Ordering::Relaxed);
    app.lens.files.store(1_284_311, Ordering::Relaxed);
    *app.lens.current.lock().unwrap() = root.path.join("Library/Containers/com.apple.mail/Data").display().to_string();
    *app.lens.live.lock().unwrap() = Some((root.path.clone(), live));
    app.lens.running.store(true, Ordering::SeqCst);
    app.lens_selected.clear();
}
