//! Space Lens screens: pick a place → "Visualizing your storage space…" →
//! list + bubble map with back/forward → review and move to Trash.

use super::scan::{self as spacelens, Node};
use super::Lens as App;
use crate::core::tool::Cx;
use crate::kit::controls::action_button;
use crate::kit::glass::*;
use egui::{Align2, Color32, Pos2, Rect, ScrollArea, Sense, Stroke, Ui, UiBuilder, Vec2};
use std::path::{Path, PathBuf};
use std::sync::atomic::Ordering;

pub fn page(app: &mut App, ui: &mut Ui, r: Rect, cx: &mut Cx) {
    let running = app.lens.running.load(Ordering::SeqCst);
    let has = app.lens.result.lock().unwrap().is_some();
    if running {
        scanning(app, ui, r);
    } else if has {
        results(app, ui, r, cx);
    } else {
        intro(app, ui, r);
    }
}

fn intro(app: &mut App, ui: &mut Ui, r: Rect) {
    let ctx = ui.ctx().clone();
    let p = ui.painter().clone();
    let t = ctx.input(|i| i.time);
    orb(&p, Pos2::new(r.center().x, r.top() + 190.0), 140.0, BLUE, Icon::Lens, t);
    p.text(Pos2::new(r.center().x, r.top() + 380.0), Align2::CENTER_CENTER, "Space Lens", bold(44.0), TEXT);
    p.text(Pos2::new(r.center().x, r.top() + 424.0), Align2::CENTER_CENTER, "Visualize what's taking up the most disk space and clean up your storage quickly.", font(17.0), TEXT_DIM);
    let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("/"));
    let places = [("Home folder", home.clone()), ("Macintosh HD", PathBuf::from("/")), ("Applications", PathBuf::from("/Applications")), ("Library", home.join("Library")), ("Downloads", home.join("Downloads"))];
    let total_w: f32 = places.len() as f32 * 138.0 - 10.0;
    let mut x = r.center().x - total_w / 2.0;
    for (label, path) in places {
        let br = Rect::from_min_size(Pos2::new(x, r.top() + 470.0), Vec2::new(128.0, 34.0));
        let sel = app.lens_root == path;
        if pill_button(ui, br, label, if sel { 1 } else { 0 }, true).clicked() {
            app.lens_root = path.clone();
        }
        x += 138.0;
    }
    if app.lens_root == Path::new("/") {
        p.text(Pos2::new(r.center().x, r.top() + 528.0), Align2::CENTER_CENTER, "Scanning the whole disk takes a few minutes. Grant Full Disk Access in System Settings for complete results.", font(13.0), TEXT_FAINT);
    }
    if let Some(msg) = &app.lens_msg {
        p.text(Pos2::new(r.center().x, r.top() + 552.0), Align2::CENTER_CENTER, msg, font(13.5), GREEN);
    }
    if action_button(ui, Pos2::new(r.center().x, r.bottom() - 54.0), "Scan", None, true) {
        app.lens_path = app.lens_root.clone();
        app.lens_back.clear();
        app.lens_fwd.clear();
        app.lens_selected.clear();
        app.lens_msg = None;
        spacelens::start(app.lens.clone(), app.lens_root.clone(), ctx);
    }
}

fn scanning(app: &mut App, ui: &mut Ui, r: Rect) {
    let ctx = ui.ctx().clone();
    let p = ui.painter().clone();
    let t = ctx.input(|i| i.time);
    orb(&p, Pos2::new(r.center().x, r.top() + 230.0), 170.0, BLUE, Icon::Lens, t);
    p.text(Pos2::new(r.center().x, r.top() + 470.0), Align2::CENTER_CENTER, "Visualizing your storage space...", font(40.0), TEXT);
    let cur = app.lens.current.lock().unwrap().clone();
    text_fit(&p, Pos2::new(r.center().x, r.top() + 520.0), Align2::CENTER_CENTER, &middle_ellipsis(&cur, 80), font(18.0), TEXT_DIM, r.width() - 80.0);
    let files = app.lens.files.load(Ordering::Relaxed);
    let bytes = app.lens.bytes.load(Ordering::Relaxed);
    p.text(Pos2::new(r.center().x, r.top() + 552.0), Align2::CENTER_CENTER, format!("{} files · {}", files, human_bytes(bytes)), font(14.0), TEXT_FAINT);
    if action_button(ui, Pos2::new(r.center().x, r.bottom() - 54.0), "Stop", None, true) {
        app.lens.stop.store(true, Ordering::SeqCst);
    }
    ctx.request_repaint();
}

fn results(app: &mut App, ui: &mut Ui, r: Rect, cx: &mut Cx) {
    let ctx = ui.ctx().clone();
    let p = ui.painter().clone();
    let root = app.lens.result.lock().unwrap().clone().unwrap();
    let node = spacelens::find(&root, &app.lens_path).cloned().unwrap_or_else(|| root.clone());
    // top bar with back/forward and the place name
    let bar_r = Rect::from_min_size(r.min, Vec2::new(r.width(), 52.0));
    glass_card(&p, bar_r, 16.0, 0.0, false);
    let back = Rect::from_min_size(bar_r.min + Vec2::new(12.0, 10.0), Vec2::splat(32.0));
    let fwd = back.translate(Vec2::new(38.0, 0.0));
    for (k, (br, ic, enabled)) in [(back, Icon::Back, !app.lens_back.is_empty()), (fwd, Icon::Forward, !app.lens_fwd.is_empty())].into_iter().enumerate() {
        let resp = ui.interact(br, ui.id().with(("nav", k)), if enabled { Sense::click() } else { Sense::hover() });
        p.rect_filled(br, 8.0, white(if resp.hovered() && enabled { 40 } else { 18 }));
        icon(&p, ic, br.shrink(8.0), if enabled { TEXT } else { TEXT_FAINT });
        if resp.clicked() {
            if k == 0 {
                if let Some(prev) = app.lens_back.pop() {
                    app.lens_fwd.push(app.lens_path.clone());
                    app.lens_path = prev;
                }
            } else if let Some(next) = app.lens_fwd.pop() {
                app.lens_back.push(app.lens_path.clone());
                app.lens_path = next;
            }
        }
    }
    icon(&p, Icon::Folder, Rect::from_center_size(Pos2::new(bar_r.center().x - 90.0, bar_r.center().y), Vec2::splat(20.0)), Color32::from_rgb(90, 175, 255));
    text_fit(&p, Pos2::new(bar_r.center().x - 74.0, bar_r.center().y), Align2::LEFT_CENTER, &node.name, bold(16.0), TEXT, 400.0);
    let so = Rect::from_min_size(Pos2::new(bar_r.right() - 140.0, bar_r.top() + 10.0), Vec2::new(128.0, 32.0));
    if pill_button(ui, so, "Start Over", 0, true).clicked() {
        *app.lens.result.lock().unwrap() = None;
        app.lens_selected.clear();
        return;
    }

    // left list
    let lr = Rect::from_min_max(Pos2::new(r.left(), bar_r.bottom() + 16.0), Pos2::new(r.left() + 400.0, r.bottom() - 62.0));
    glass_card(&p, lr, 20.0, 0.0, false);
    gradient_rect(&p, Rect::from_min_size(lr.min + Vec2::new(18.0, 18.0), Vec2::new(34.0, 42.0)), 5.0, Color32::from_rgb(235, 235, 245), Color32::from_rgb(170, 170, 190));
    text_fit(&p, lr.min + Vec2::new(66.0, 16.0), Align2::LEFT_TOP, &node.name, bold(19.0), TEXT, lr.width() - 90.0);
    p.text(lr.min + Vec2::new(66.0, 44.0), Align2::LEFT_TOP, format!("{}  |  {} items", human_bytes(node.bytes), human_count(node.items)), font(14.0), TEXT_DIM);
    let nsel = app.lens_selected.len();
    p.text(lr.min + Vec2::new(18.0, 78.0), Align2::LEFT_TOP, "Select:", bold(14.0), TEXT);
    p.text(lr.min + Vec2::new(72.0, 78.0), Align2::LEFT_TOP, if nsel == 0 { "None".to_string() } else { format!("{nsel} items") }, font(14.0), TEXT_DIM);
    let list = Rect::from_min_max(lr.min + Vec2::new(8.0, 106.0), lr.max - Vec2::new(8.0, 10.0));
    let mut child = ui.new_child(UiBuilder::new().max_rect(list).layout(egui::Layout::top_down(egui::Align::Min)));
    child.set_clip_rect(list);
    let mut go_into: Option<PathBuf> = None;
    let mut toggle: Option<(PathBuf, u64)> = None;
    ScrollArea::vertical().id_salt(("lens", node.path.display().to_string())).auto_shrink([false, false]).show(&mut child, |ui| {
        for c in node.children.iter().take(300) {
            let (rr, resp) = ui.allocate_exact_size(Vec2::new(list.width() - 6.0, 44.0), Sense::click());
            let pp = ui.painter();
            if resp.hovered() {
                pp.rect_filled(rr, 10.0, white(12));
            }
            let essential = spacelens::is_essential(&c.path);
            let on = app.lens_selected.iter().any(|(p, _)| p == &c.path);
            let cb = Rect::from_min_size(rr.min + Vec2::new(8.0, 13.0), Vec2::splat(17.0));
            let cr = checkbox(ui, cb, ui.id().with(("ls", &c.path)), on, !essential);
            if essential {
                cr.on_hover_text("This is an essential system item and it cannot be deleted.");
            } else if cr.clicked() {
                toggle = Some((c.path.clone(), c.bytes));
            }
            let pp = ui.painter();
            let ic_r = Rect::from_min_size(rr.min + Vec2::new(36.0, 9.0), Vec2::new(30.0, 26.0));
            if c.is_dir {
                icon(pp, Icon::Folder, ic_r, Color32::from_rgb(90, 175, 255));
            } else {
                gradient_rect(pp, ic_r.shrink2(Vec2::new(5.0, 0.0)), 3.0, Color32::from_rgb(235, 235, 245), Color32::from_rgb(180, 180, 200));
            }
            text_fit(pp, rr.min + Vec2::new(78.0, 22.0), Align2::LEFT_CENTER, &c.name, font(15.5), TEXT, rr.width() - 180.0);
            pp.text(Pos2::new(rr.right() - 12.0, rr.center().y), Align2::RIGHT_CENTER, human_bytes(c.bytes), bold(14.0), TEXT_DIM);
            if resp.clicked() && c.is_dir && !c.children.is_empty() {
                go_into = Some(c.path.clone());
            }
        }
        if node.other > 0 {
            let (rr, _) = ui.allocate_exact_size(Vec2::new(list.width() - 6.0, 40.0), Sense::hover());
            let pp = ui.painter();
            pp.circle_filled(rr.min + Vec2::new(51.0, 20.0), 9.0, white(60));
            pp.text(rr.min + Vec2::new(78.0, 20.0), Align2::LEFT_CENTER, "Other items", font(15.0), TEXT_DIM);
            pp.text(Pos2::new(rr.right() - 12.0, rr.center().y), Align2::RIGHT_CENTER, human_bytes(node.other), bold(14.0), TEXT_FAINT);
        }
    });
    if let Some((path, b)) = toggle {
        if let Some(i) = app.lens_selected.iter().position(|(p, _)| p == &path) {
            app.lens_selected.remove(i);
        } else {
            app.lens_selected.retain(|(p, _)| !p.starts_with(&path));
            app.lens_selected.push((path, b));
        }
    }

    // bubbles
    let br = Rect::from_min_max(Pos2::new(lr.right() + 20.0, lr.top()), Pos2::new(r.right(), lr.bottom()));
    let sizes: Vec<u64> = node.children.iter().map(|c| c.bytes).collect();
    let packed = spacelens::pack(&sizes, br.shrink(10.0), 11);
    for (i, c, rad) in packed {
        let ch = &node.children[i];
        let id = ui.id().with(("bubble", &ch.path));
        let resp = ui.interact(Rect::from_center_size(c, Vec2::splat(rad * 1.6)), id, Sense::click());
        let hv = ctx.animate_bool(id, resp.hovered());
        let rr = rad + 3.0 * hv;
        bubble(&p, c, rr, hv, app.lens_selected.iter().any(|(p, _)| p == &ch.path));
        let isz = (rr * 0.55).min(110.0);
        if ch.is_dir {
            icon(&p, Icon::Folder, Rect::from_center_size(c - Vec2::new(0.0, if rr > 40.0 { rr * 0.2 } else { 0.0 }), Vec2::new(isz, isz * 0.85)), Color32::from_rgb(90, 175, 255));
        } else {
            gradient_rect(&p, Rect::from_center_size(c - Vec2::new(0.0, rr * 0.2), Vec2::new(isz * 0.6, isz * 0.8)), 4.0, Color32::from_rgb(235, 235, 245), Color32::from_rgb(180, 180, 200));
        }
        if rr > 40.0 {
            let fs = (rr * 0.16).clamp(11.5, 26.0);
            text_fit(&p, c + Vec2::new(0.0, rr * 0.36), Align2::CENTER_CENTER, &ch.name, bold(fs), TEXT, rr * 1.6);
            p.text(c + Vec2::new(0.0, rr * 0.36 + fs + 2.0), Align2::CENTER_CENTER, human_bytes(ch.bytes), bold((fs * 0.92).max(11.0)), TEXT);
        }
        if resp.hovered() {
            let tip = format!("{}\nSize: {}{}", ch.name, human_bytes(ch.bytes), ch.modified.map(|m| format!("\nModified: {}", chrono::DateTime::<chrono::Local>::from(m).format("%b %e, %Y"))).unwrap_or_default());
            resp.clone().on_hover_text(tip);
        }
        if resp.clicked() && ch.is_dir && !ch.children.is_empty() {
            go_into = Some(ch.path.clone());
        }
    }
    if let Some(path) = go_into {
        app.lens_back.push(app.lens_path.clone());
        app.lens_fwd.clear();
        app.lens_path = path;
    }

    // bottom: disk bar, selection and Review
    let (volume, disk_total, disk_free) = app.disk();
    let by = r.bottom() - 30.0;
    p.text(Pos2::new(r.left() + 4.0, by - 10.0), Align2::LEFT_CENTER, &volume, bold(16.0), TEXT);
    let used = disk_total.saturating_sub(disk_free);
    p.text(Pos2::new(lr.right(), by - 10.0), Align2::RIGHT_CENTER, format!("{} of {} used", human_bytes(used), human_bytes(disk_total)), font(14.0), TEXT_DIM);
    bar(&p, Rect::from_min_size(Pos2::new(r.left(), by + 12.0), Vec2::new(lr.width(), 6.0)), used as f32 / disk_total.max(1) as f32, Color32::from_rgb(220, 220, 255));
    let sel_bytes: u64 = app.lens_selected.iter().map(|x| x.1).sum();
    icon(&p, Icon::Info, Rect::from_center_size(Pos2::new(br.left() + 120.0, by), Vec2::splat(20.0)), TEXT_DIM);
    let txt = if app.lens_selected.is_empty() { "No Items selected".to_string() } else { format!("{} items selected", app.lens_selected.len()) };
    p.text(Pos2::new(br.left() + 140.0, by), Align2::LEFT_CENTER, format!("{txt}   |   {}", human_bytes(sel_bytes)), bold(15.0), TEXT);
    let rb = Rect::from_min_size(Pos2::new(r.right() - 200.0, by - 20.0), Vec2::new(200.0, 40.0));
    if pill_button(ui, rb, "Review and Remove", 1, !app.lens_selected.is_empty()).clicked() {
        app.lens_review = true;
    }
    if app.lens_review {
        review(app, ui, r, cx);
    }
}

fn bubble(p: &egui::Painter, c: Pos2, r: f32, hover: f32, selected: bool) {
    // a clear glass bubble
    let rect = Rect::from_center_size(c, Vec2::splat(r * 2.0));
    let shadow = egui::epaint::Shadow { offset: [0, 6], blur: 20, spread: 0, color: Color32::from_black_alpha(50) };
    p.add(shadow.as_shape(rect, egui::CornerRadius::same(r.min(255.0) as u8)));
    gradient_rect_dir(p, rect, r, white((30.0 + 18.0 * hover) as u8), white((8.0 + 10.0 * hover) as u8), Vec2::new(0.3, 1.0));
    rim(p, rect, r, 0.9 + 0.1 * hover);
    if selected {
        p.circle_stroke(c, r + 2.0, Stroke::new(2.5, BLUE));
    }
}

fn review(app: &mut App, ui: &mut Ui, r: Rect, cx: &mut Cx) {
    let p = ui.painter().clone();
    p.rect_filled(r.expand(30.0), 0.0, Color32::from_black_alpha(120));
    let m = Rect::from_center_size(r.center(), Vec2::new(560.0, 420.0));
    glass_card(&p, m, 22.0, 0.3, false);
    p.rect_filled(m, 22.0, Color32::from_rgba_unmultiplied(28, 28, 32, 225));
    p.text(m.min + Vec2::new(24.0, 22.0), Align2::LEFT_TOP, "Review Items Before Removal", bold(20.0), TEXT);
    p.text(m.min + Vec2::new(24.0, 52.0), Align2::LEFT_TOP, "They'll be moved to the Trash, so you can put them back.", font(14.0), TEXT_DIM);
    let list = Rect::from_min_max(m.min + Vec2::new(16.0, 84.0), m.max - Vec2::new(16.0, 72.0));
    let mut child = ui.new_child(UiBuilder::new().max_rect(list).layout(egui::Layout::top_down(egui::Align::Min)));
    child.set_clip_rect(list);
    let mut drop: Option<usize> = None;
    let home = dirs::home_dir().unwrap_or_default().display().to_string();
    ScrollArea::vertical().id_salt("review").auto_shrink([false, false]).show(&mut child, |ui| {
        for (i, (path, b)) in app.lens_selected.iter().enumerate() {
            let (rr, _) = ui.allocate_exact_size(Vec2::new(list.width() - 6.0, 34.0), Sense::hover());
            let pp = ui.painter();
            text_fit(pp, rr.min + Vec2::new(8.0, 17.0), Align2::LEFT_CENTER, &middle_ellipsis(&path.display().to_string().replacen(&home, "~", 1), 60), font(14.0), TEXT, rr.width() - 160.0);
            pp.text(Pos2::new(rr.right() - 40.0, rr.center().y), Align2::RIGHT_CENTER, human_bytes(*b), bold(14.0), TEXT_DIM);
            let xr = Rect::from_center_size(Pos2::new(rr.right() - 16.0, rr.center().y), Vec2::splat(20.0));
            let resp = ui.interact(xr, ui.id().with(("rm", i)), Sense::click());
            ui.painter().text(xr.center(), Align2::CENTER_CENTER, "×", font(18.0), if resp.hovered() { TEXT } else { TEXT_FAINT });
            if resp.clicked() {
                drop = Some(i);
            }
        }
    });
    if let Some(i) = drop {
        app.lens_selected.remove(i);
    }
    let cancel = Rect::from_min_size(Pos2::new(m.right() - 256.0, m.bottom() - 54.0), Vec2::new(110.0, 34.0));
    let ok = Rect::from_min_size(Pos2::new(m.right() - 134.0, m.bottom() - 54.0), Vec2::new(110.0, 34.0));
    if pill_button(ui, cancel, "Cancel", 0, true).clicked() {
        app.lens_review = false;
    }
    if pill_button(ui, ok, "Remove", 2, !app.lens_selected.is_empty()).clicked() {
        let mut removed = vec![];
        let mut failed = 0;
        let mut freed = 0;
        for (path, b) in app.lens_selected.drain(..) {
            if spacelens::is_essential(&path) || crate::kit::fs::is_protected(&path).is_some() && path.join(".git").exists() {
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
            spacelens::prune(root, &removed);
        }
        app.lens_review = false;
        let msg = if failed == 0 { format!("Moved {} to the Trash", human_bytes(freed)) } else { format!("Moved {} to the Trash · {failed} items couldn't be removed", human_bytes(freed)) };
        cx.toast(msg);
    }
}

fn human_count(n: u64) -> String {
    if n >= 1_000_000 {
        format!("{:.0}M", n as f64 / 1e6)
    } else if n >= 1_000 {
        format!("{:.0}K", n as f64 / 1e3)
    } else {
        n.to_string()
    }
}

/// A made-up tree for screenshots (no personal paths).
pub fn demo_tree(app: &mut App) {
    let mk = |name: &str, bytes: u64| Node { name: name.into(), path: PathBuf::from(format!("/{name}")), bytes, items: bytes / 400_000, is_dir: true, modified: None, children: vec![Node::default()], other: 0 };
    let children = vec![mk("Users", 365_000_000_000), mk("Applications", 65_600_000_000), mk("System", 36_500_000_000), mk("opt", 34_700_000_000), mk("private", 18_600_000_000), mk("Library", 11_700_000_000), mk("usr", 3_300_000_000), mk("bin", 7_200_000), mk("sbin", 3_200_000)];
    let total = children.iter().map(|c| c.bytes).sum();
    let root = Node { name: "Macintosh HD".into(), path: PathBuf::from("/"), bytes: total, items: 6_000_000, is_dir: true, modified: None, children, other: 120_000_000 };
    app.lens_path = PathBuf::from("/");
    *app.lens.result.lock().unwrap() = Some(root);
}
