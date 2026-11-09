//! The menu-bar panel (design A, "Control Center"): a title line with the
//! window and settings buttons, the user's widgets on a 4-column grid of
//! square cells (scrolling when they don't fit), an "Edit widgets…" link,
//! and the sheet that opens beside the panel. In edit mode the widgets
//! wobble: drag to move, "−" to remove, tap to pick a size; "+" opens the
//! gallery (see `gallery.rs`).

use super::editor::Drag;
use super::layout;
use super::settings::{add_entry, hide_entry, place_entry, set_size};
use super::shell::{App, Gallery, RECOMMENDATION};
use super::tool::{Cx, Recommendation, Size, GRID_COLUMNS};
use crate::kit::glass::*;
use crate::kit::mechanics as m;
use crate::kit::tile;
use egui::{Align2, Color32, Id, LayerId, Order, Pos2, Rect, ScrollArea, Sense, Stroke, StrokeKind, Ui, UiBuilder, Vec2};

/// Height of the title line above the widgets.
pub const HEADER: f32 = 60.0;
/// Space below the widgets: a gap plus the "Edit widgets…" link.
pub const FOOTER: f32 = 40.0;
/// In edit mode: the size bar of the selected widget.
pub const EDIT_FOOTER: f32 = 72.0;
/// Side padding of the widget grid.
pub const PAD: f32 = 12.0;
/// Gap between grid cells.
pub const GAP: f32 = 8.0;

pub fn inner_width() -> f32 {
    m::PANEL_WIDTH - 2.0 * PAD
}

/// Side of one square grid cell for a grid `width` wide.
pub fn cell(width: f32) -> f32 {
    ((width - GAP * (GRID_COLUMNS as f32 - 1.0)) / GRID_COLUMNS as f32).max(1.0)
}

/// Pixel size of a widget of `size` on a grid `width` wide.
pub fn size_px(size: Size, width: f32) -> Vec2 {
    let c = cell(width);
    let (r, k) = size.dims();
    Vec2::new(k as f32 * c + (k as f32 - 1.0) * GAP, r as f32 * c + (r as f32 - 1.0) * GAP)
}

pub fn footer_height(app: &App) -> f32 {
    if app.editing { EDIT_FOOTER } else { FOOTER }
}

/// A shown widget and where it goes, relative to the grid's origin.
pub struct Placed {
    /// Index in `app.widgets`.
    pub entry: usize,
    /// Owner tool index; `None` = the shell.
    pub owner: Option<usize>,
    pub id: &'static str,
    pub size: Size,
    pub rect: Rect,
}

/// Lay out the shown widgets for a grid `width` wide: the widgets and the
/// height used.
pub fn place(app: &App, width: f32) -> (Vec<Placed>, f32) {
    let items: Vec<(usize, Option<usize>, &'static str, Size)> = app
        .widgets
        .iter()
        .enumerate()
        .filter(|(_, e)| e.on)
        .filter_map(|(entry, e)| {
            let (owner, spec) = app.widget_spec(&e.key)?;
            if owner.is_some_and(|i| !app.tool_on(i)) {
                return None;
            }
            Some((entry, owner, spec.id, spec.fit(e.size)))
        })
        .collect();
    let sizes: Vec<(u8, f32)> = items.iter().map(|it| (it.3.cols(), size_px(it.3, width).y)).collect();
    let (rects, total) = layout::flow(&sizes, width, GAP);
    let placed = items.into_iter().zip(rects).map(|((entry, owner, id, size), rect)| Placed { entry, owner, id, size, rect }).collect();
    (placed, total)
}

/// Draw one placed widget at `r`.
pub fn draw_widget(app: &mut App, ui: &mut Ui, w: &Placed, r: Rect, cx: &mut Cx) {
    draw_at(app, ui, w.owner, w.id, w.size, r, cx);
}

/// Draw widget `id` of `owner` (`None`: the shell's) at `size` in `r`.
pub fn draw_at(app: &mut App, ui: &mut Ui, owner: Option<usize>, id: &'static str, size: Size, r: Rect, cx: &mut Cx) {
    match owner {
        Some(i) => {
            let mut child = ui.new_child(UiBuilder::new().max_rect(r).id_salt((app.tools[i].id(), id, size.label())));
            app.tools[i].widget_ui(id, size, &mut child, r, cx);
        }
        None if id == RECOMMENDATION => recommendation(app, ui, r, cx),
        None => {}
    }
}

/// Height the widget area needs.
pub fn content_height(app: &App) -> f32 {
    place(app, inner_width()).1.max(120.0)
}

pub fn panel(app: &mut App, ui: &mut Ui, rect: Rect, cx: &mut Cx) {
    if app.editing {
        edit_header(app, ui, rect);
    } else {
        header(app, ui, rect);
    }
    let area = Rect::from_min_max(Pos2::new(rect.left() + PAD, rect.top() + HEADER), Pos2::new(rect.right() - PAD, rect.bottom() - footer_height(app)));
    let need = content_height(app);
    let mut target: Option<Option<String>> = None;
    if need > area.height() + 0.5 {
        let mut child = ui.new_child(UiBuilder::new().max_rect(area).layout(egui::Layout::top_down(egui::Align::Min)));
        child.set_clip_rect(area.expand2(Vec2::new(PAD, 8.0)));
        ScrollArea::vertical().id_salt("panel-widgets").auto_shrink([false, false]).show(&mut child, |ui| {
            let (origin, _) = ui.allocate_exact_size(Vec2::new(area.width(), need), Sense::hover());
            let clip = ui.clip_rect();
            target = grid(app, ui, origin, clip, cx);
        });
    } else {
        target = grid(app, ui, Rect::from_min_size(area.min, Vec2::new(area.width(), need)), area.expand2(Vec2::new(PAD, 8.0)), cx);
    }
    if app.editing {
        size_bar(app, ui, rect);
        drop_drag(app, ui, target);
    } else {
        footer(app, ui, rect);
    }
    if app.gear_open {
        gear_menu(app, ui, rect, cx);
    }
}

fn grid(app: &mut App, ui: &mut Ui, origin: Rect, clip: Rect, cx: &mut Cx) -> Option<Option<String>> {
    if app.editing {
        edit_widgets(app, ui, origin, clip)
    } else {
        widgets(app, ui, origin, cx);
        None
    }
}

/// The title line: the first enabled tool that offers one, or the app name.
pub fn title_line(app: &App, p: &egui::Painter, x0: f32, top: f32, width: f32) {
    match app.tools.iter().enumerate().filter(|(i, _)| app.tool_on(*i)).find_map(|(_, t)| t.panel_header()) {
        Some(h) => {
            text_fit(p, Pos2::new(x0, top + 15.0), Align2::LEFT_TOP, &h.title, bold(15.0), TEXT, width);
            p.circle_filled(Pos2::new(x0 + 3.5, top + 42.0), 3.5, h.dot);
            text_fit(p, Pos2::new(x0 + 13.0, top + 42.0), Align2::LEFT_CENTER, &h.status, font(12.0), TEXT_DIM, width - 13.0);
        }
        None => {
            text_fit(p, Pos2::new(x0, top + 31.0), Align2::LEFT_CENTER, "CleanMeBro", bold(15.0), TEXT, width);
        }
    }
}

/// Title and status from the first tool that offers one, then the window
/// and settings buttons.
fn header(app: &mut App, ui: &mut Ui, rect: Rect) {
    let ctx = ui.ctx().clone();
    let p = ui.painter().clone();
    let gear = Rect::from_min_size(Pos2::new(rect.right() - 14.0 - 30.0, rect.top() + 16.0), Vec2::splat(30.0));
    let win = gear.translate(Vec2::new(-38.0, 0.0));
    let x0 = rect.left() + 16.0;
    title_line(app, &p, x0, rect.top(), win.left() - x0 - 10.0);
    let pages = app.page_tools();
    if !pages.is_empty() && icon_button(ui, win, Icon::Window, "Open CleanMeBro").clicked() {
        let page = if app.tools.get(app.page).is_some_and(|t| t.has_page()) { app.page } else { pages[0] };
        app.open_window(&ctx, page);
    }
    if icon_button(ui, gear, Icon::Gear, "Settings").clicked() {
        app.gear_open = !app.gear_open;
    }
}

fn widgets(app: &mut App, ui: &mut Ui, origin: Rect, cx: &mut Cx) {
    let (placed, _) = place(app, origin.width());
    if placed.is_empty() {
        let p = ui.painter();
        p.text(origin.center_top() + Vec2::new(0.0, 40.0), Align2::CENTER_CENTER, "No widgets shown", bold(15.0), TEXT_DIM);
        p.text(origin.center_top() + Vec2::new(0.0, 64.0), Align2::CENTER_CENTER, "Use Edit widgets… to pick some.", font(13.0), TEXT_FAINT);
        return;
    }
    for w in &placed {
        draw_widget(app, ui, w, w.rect.translate(origin.min.to_vec2()), cx);
    }
}

// ---------------------------------------------------------------- edit mode

/// "+" (gallery), the title and Done.
fn edit_header(app: &mut App, ui: &mut Ui, rect: Rect) {
    let ctx = ui.ctx().clone();
    let p = ui.painter().clone();
    let cy = rect.top() + 31.0;
    let plus_r = Rect::from_center_size(Pos2::new(rect.left() + 16.0 + 15.0, cy), Vec2::splat(30.0));
    let resp = ui.interact(plus_r, ui.scope_id().with("edit-plus"), Sense::click());
    let open = app.gallery.is_some();
    p.circle_filled(plus_r.center(), 15.0, if open { BLUE } else { white(if resp.hovered() { 46 } else { 28 }) });
    plus(&p, plus_r.center(), 6.0, TEXT);
    if resp.on_hover_text("Add a widget").clicked() {
        app.gallery = if open { None } else { Some(Gallery::default()) };
        app.reshape(&ctx);
    }
    p.text(Pos2::new(rect.center().x, cy), Align2::CENTER_CENTER, "Edit widgets", bold(15.0), TEXT);
    let dw = button_width(&p, "Done", BTN_PRIMARY).max(64.0);
    let done = Rect::from_min_size(Pos2::new(rect.right() - 14.0 - dw, cy - 15.0), Vec2::new(dw, 30.0));
    if pill_button(ui, done, "Done", BTN_PRIMARY, true).clicked() || ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
        app.stop_editing();
        app.reshape(&ctx);
    }
}

pub fn plus(p: &egui::Painter, c: Pos2, s: f32, col: Color32) {
    p.line_segment([c - Vec2::new(s, 0.0), c + Vec2::new(s, 0.0)], Stroke::new(1.8, col));
    p.line_segment([c - Vec2::new(0.0, s), c + Vec2::new(0.0, s)], Stroke::new(1.8, col));
}

/// The widgets in edit mode: drawn for real but quiet (their clicks select
/// them instead), wobbling, with a "−" badge and, on the selected one, a
/// handle that steps to the next size. Returns where a drag would drop:
/// `Some(before)` while something is dragged over the grid.
fn edit_widgets(app: &mut App, ui: &mut Ui, origin: Rect, clip: Rect) -> Option<Option<String>> {
    let ctx = ui.ctx().clone();
    let p = ui.painter().clone();
    let t = ctx.input(|i| i.time) as f32;
    let (placed, _) = place(app, origin.width());
    let mut quiet = Cx::new(None);
    let dragged = app.drag.as_ref().map(|d| d.key.clone());
    let mut keys = Vec::with_capacity(placed.len());
    let mut rects = Vec::with_capacity(placed.len());
    let (mut hide, mut select, mut grow) = (None, None, None);
    for (n, w) in placed.iter().enumerate() {
        let key = app.widgets[w.entry].key.clone();
        let base = w.rect.translate(origin.min.to_vec2());
        let phase = n as f32 * 1.7;
        let jig = if app.drag.is_some() { Vec2::ZERO } else { Vec2::new((t * 17.0 + phase).sin() * 0.7, (t * 21.0 + phase).cos() * 0.4) };
        draw_widget(app, ui, w, base.translate(jig), &mut quiet);
        let id = ui.scope_id().with(("edit", &key));
        let resp = ui.interact(base, id, Sense::click_and_drag());
        let selected = app.edit_sel.as_deref() == Some(key.as_str());
        if dragged.as_deref() == Some(key.as_str()) {
            p.rect_filled(base, tile::R_TILE, with_alpha(BG, 0.7));
        } else if selected {
            p.rect_stroke(base.expand(2.0), tile::R_TILE + 2.0, Stroke::new(2.0, ACCENT), StrokeKind::Outside);
        } else if resp.hovered() && app.drag.is_none() {
            p.rect_stroke(base, tile::R_TILE, Stroke::new(1.0, white(60)), StrokeKind::Inside);
        }
        if resp.hovered() && app.drag.is_none() {
            ctx.set_cursor_icon(egui::CursorIcon::Grab);
        }
        if resp.clicked() {
            select = Some(key.clone());
        }
        if resp.drag_started() && app.drag.is_none() {
            let label = app.widget_spec(&key).map(|(_, s)| s.title.to_string()).unwrap_or_default();
            let grab = resp.interact_pointer_pos().map(|q| q - base.min).unwrap_or(Vec2::splat(12.0));
            app.drag = Some(Drag { key: key.clone(), from_gallery: false, label, size: base.size(), grab });
        }
        if app.drag.is_none() {
            // "−" removes it from the panel
            let mb = Rect::from_center_size(base.left_top() + Vec2::new(3.0, 3.0), Vec2::splat(20.0));
            if clip.contains(mb.center()) {
                let b = ui.interact(mb, id.with("hide"), Sense::click());
                p.circle_filled(mb.center() + Vec2::new(0.0, 1.5), 10.5, Color32::from_black_alpha(90));
                p.circle_filled(mb.center(), 10.0, if b.hovered() { Color32::from_rgb(0x8E, 0x8E, 0x93) } else { Color32::from_rgb(0x58, 0x58, 0x5D) });
                p.line_segment([mb.center() - Vec2::new(4.5, 0.0), mb.center() + Vec2::new(4.5, 0.0)], Stroke::new(1.8, Color32::WHITE));
                if b.on_hover_text("Remove").clicked() {
                    hide = Some(key.clone());
                }
            }
            // the handle: next size
            if selected {
                let hb = Rect::from_center_size(base.right_bottom() - Vec2::new(2.0, 2.0), Vec2::splat(22.0));
                let b = ui.interact(hb, id.with("grow"), Sense::click());
                p.circle_filled(hb.center(), 11.0, BG);
                p.circle_filled(hb.center(), 9.0, if b.hovered() { lerp_color(ACCENT, Color32::WHITE, 0.2) } else { ACCENT });
                resize_glyph(&p, hb.center(), 4.0, Color32::WHITE);
                if b.hovered() {
                    ctx.set_cursor_icon(egui::CursorIcon::ResizeNwSe);
                }
                if b.on_hover_text("Next size").clicked() {
                    grow = Some(key.clone());
                }
            }
        }
        keys.push(key);
        rects.push(base);
    }
    if let Some(k) = hide {
        hide_entry(&mut app.widgets, &k);
        if app.edit_sel.as_deref() == Some(k.as_str()) {
            app.edit_sel = None;
        }
    } else if let Some(k) = grow {
        step_size(app, &k);
    } else if let Some(k) = select {
        app.edit_sel = if app.edit_sel.as_deref() == Some(k.as_str()) { None } else { Some(k) };
    }
    ctx.request_repaint_after(std::time::Duration::from_millis(33));

    // where a drop would land: before the nearest widget, or at the end
    let d = app.drag.as_ref()?;
    let q = ctx.pointer_latest_pos()?;
    if !clip.contains(q) {
        return None;
    }
    let others: Vec<usize> = (0..keys.len()).filter(|k| keys[*k] != d.key).collect();
    let nearest = others.iter().copied().min_by(|a, b| dist(rects[*a], q).total_cmp(&dist(rects[*b], q)));
    let Some(k) = nearest else { return Some(None) };
    let r = rects[k];
    let full = r.width() > origin.width() * 0.9;
    let before = if full { q.y < r.center().y } else { q.x < r.center().x };
    let (target, line) = if before {
        (Some(keys[k].clone()), if full { [Pos2::new(r.left(), r.top() - 4.0), Pos2::new(r.right(), r.top() - 4.0)] } else { [Pos2::new(r.left() - 4.0, r.top()), Pos2::new(r.left() - 4.0, r.bottom())] })
    } else {
        let next = others.iter().find(|j| **j > k).map(|j| keys[*j].clone());
        (next, if full { [Pos2::new(r.left(), r.bottom() + 4.0), Pos2::new(r.right(), r.bottom() + 4.0)] } else { [Pos2::new(r.right() + 4.0, r.top()), Pos2::new(r.right() + 4.0, r.bottom())] })
    };
    p.line_segment(line, Stroke::new(3.0, ACCENT));
    Some(target)
}

/// Two little arrows pointing apart, corner to corner.
fn resize_glyph(p: &egui::Painter, c: Pos2, s: f32, col: Color32) {
    let st = Stroke::new(1.6, col);
    p.line_segment([c - Vec2::splat(s), c + Vec2::splat(s)], st);
    for (tip, dir) in [(c - Vec2::splat(s), 1.0), (c + Vec2::splat(s), -1.0)] {
        p.line_segment([tip, tip + Vec2::new(3.0 * dir, 0.0)], st);
        p.line_segment([tip, tip + Vec2::new(0.0, 3.0 * dir)], st);
    }
}

fn dist(r: Rect, q: Pos2) -> f32 {
    let dx = (r.left() - q.x).max(q.x - r.right()).max(0.0);
    let dy = (r.top() - q.y).max(q.y - r.bottom()).max(0.0);
    dx * dx + dy * dy
}

/// The widget's next size it offers (wrapping around).
fn step_size(app: &mut App, key: &str) {
    let Some((_, spec)) = app.widget_spec(key) else { return };
    let Some(cur) = app.widgets.iter().find(|e| e.key == key).map(|e| spec.fit(e.size)) else { return };
    let i = spec.sizes.iter().position(|z| *z == cur).unwrap_or(0);
    set_size(&mut app.widgets, key, spec.sizes[(i + 1) % spec.sizes.len()]);
}

/// The ghost under the pointer while dragging, and the drop itself.
fn drop_drag(app: &mut App, ui: &mut Ui, target: Option<Option<String>>) {
    let ctx = ui.ctx().clone();
    let Some(d) = app.drag.clone() else { return };
    ctx.set_cursor_icon(egui::CursorIcon::Grabbing);
    if let Some(q) = ctx.pointer_latest_pos() {
        let gp = ctx.layer_painter(LayerId::new(Order::Tooltip, Id::unique("panel-drag")));
        let size = Vec2::new(d.size.x.min(220.0), 40.0);
        let gr = Rect::from_min_size(q - Vec2::new(d.grab.x.min(size.x - 12.0), 20.0), size);
        gp.rect_filled(gr.translate(Vec2::new(0.0, 8.0)).expand(2.0), tile::R_TILE + 2.0, Color32::from_black_alpha(80));
        gp.rect_filled(gr, tile::R_TILE, with_alpha(RAISED, 0.94));
        gp.rect_stroke(gr, tile::R_TILE, Stroke::new(1.0, if target.is_some() { ACCENT } else { white(40) }), StrokeKind::Inside);
        text_fit(&gp, Pos2::new(gr.left() + 12.0, gr.center().y), Align2::LEFT_CENTER, &d.label, bold(13.0), TEXT, gr.width() - 24.0);
    }
    if !ctx.input(|i| i.pointer.primary_down()) {
        if let Some(before) = target {
            if d.from_gallery {
                let size = app.gallery.as_ref().and_then(|g| g.size).or_else(|| app.widget_spec(&d.key).map(|(_, s)| s.size));
                if let Some(z) = size {
                    add_entry(&mut app.widgets, &d.key, z, before.as_deref());
                    app.edit_sel = Some(d.key.clone());
                    app.gallery = None;
                    app.reshape(&ctx);
                }
            } else if before.as_deref() != Some(d.key.as_str()) {
                place_entry(&mut app.widgets, &d.key, before.as_deref());
            }
        }
        app.drag = None;
    }
    ctx.request_repaint();
}

/// Edit mode's footer: the selected widget's sizes, or a hint.
fn size_bar(app: &mut App, ui: &mut Ui, rect: Rect) {
    let p = ui.painter().clone();
    let bar_r = Rect::from_min_max(Pos2::new(rect.left(), rect.bottom() - EDIT_FOOTER), rect.max);
    hairline(&p, bar_r.left_top() + Vec2::new(12.0, 0.0), bar_r.right_top() - Vec2::new(12.0, 0.0));
    let x0 = rect.left() + PAD;
    let x1 = rect.right() - PAD;
    let sel = app.edit_sel.clone().and_then(|k| {
        let (_, spec) = app.widget_spec(&k)?;
        let cur = spec.fit(app.widgets.iter().find(|e| e.key == k && e.on)?.size);
        Some((k, spec, cur))
    });
    let Some((key, spec, cur)) = sel else {
        text_fit(&p, bar_r.center(), Align2::CENTER_CENTER, "Tap a widget to resize it · drag to move · − to remove", font(12.0), TEXT_FAINT, bar_r.width() - 24.0);
        return;
    };
    let ty = bar_r.top() + 18.0;
    let tr = p.text(Pos2::new(x0, ty), Align2::LEFT_CENTER, spec.title, bold(12.0), TEXT);
    p.text(Pos2::new(tr.right() + 6.0, ty), Align2::LEFT_CENTER, "size", font(12.0), TEXT_FAINT);
    p.text(Pos2::new(x1, ty), Align2::RIGHT_CENTER, format!("{} · {}", cur.name(), cur.label()), font(12.0), TEXT_FAINT);
    let chips = Rect::from_min_max(Pos2::new(x0, ty + 12.0), Pos2::new(x1, ty + 42.0));
    if let Some(z) = size_chips(ui, chips, ui.scope_id().with(("sizes", &key)), spec.sizes, cur) {
        set_size(&mut app.widgets, &key, z);
    }
}

/// Five size chips; sizes the widget doesn't offer are struck out. Returns
/// a newly picked size.
pub fn size_chips(ui: &mut Ui, rect: Rect, id: Id, offered: &[Size], cur: Size) -> Option<Size> {
    let p = ui.painter().clone();
    p.rect_filled(rect, 9.0, white(18));
    let inner = rect.shrink(2.0);
    let w = inner.width() / Size::ALL.len() as f32;
    let mut out = None;
    for (i, z) in Size::ALL.into_iter().enumerate() {
        let r = Rect::from_min_size(Pos2::new(inner.left() + w * i as f32, inner.top()), Vec2::new(w, inner.height())).shrink2(Vec2::new(1.0, 0.0));
        let ok = offered.contains(&z);
        let resp = ui.interact(r, id.with(i), if ok { Sense::click() } else { Sense::hover() });
        if z == cur {
            p.rect_filled(r, 7.0, BLUE);
        } else if ok && resp.hovered() {
            p.rect_filled(r, 7.0, white(20));
        }
        let col = if z == cur { Color32::WHITE } else if ok { TEXT_SOFT } else { white(60) };
        let g = p.text(r.center(), Align2::CENTER_CENTER, z.label(), if z == cur { bold(12.0) } else { font(12.0) }, col);
        if !ok {
            p.line_segment([g.left_center(), g.right_center()], Stroke::new(1.0, white(60)));
        }
        let resp = resp.on_hover_text(if ok { z.name() } else { "Not offered for this widget" });
        if ok && resp.clicked() && z != cur {
            out = Some(z);
        }
        if ok && resp.hovered() {
            ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
        }
    }
    out
}

/// The banner with the highest-priority suggestion of any tool.
fn recommendation(app: &App, ui: &mut Ui, rect: Rect, cx: &mut Cx) {
    let Some(rec) = best_recommendation(app.tools.iter().enumerate().filter(|(i, _)| app.tool_on(*i)).flat_map(|(_, t)| t.recommendations())) else {
        let p = ui.painter();
        glass_card(p, rect, R_CARD, 0.0, false);
        icon(p, Icon::Check, Rect::from_center_size(Pos2::new(rect.left() + 21.0, rect.center().y), Vec2::splat(18.0)), GREEN);
        p.text(Pos2::new(rect.left() + 40.0, rect.top() + 10.0), Align2::LEFT_TOP, "All good", bold(13.0), TEXT);
        p.text(Pos2::new(rect.left() + 40.0, rect.top() + 29.0), Align2::LEFT_TOP, "Nothing needs your attention right now.", font(12.0), TEXT_DIM);
        return;
    };
    let resp = banner(ui, rect, ui.scope_id().with("rec"), rec.badge.icon, &rec.title, &rec.body, &rec.action).on_hover_text(&rec.body);
    if resp.clicked() {
        cx.push(rec.go);
    }
}

/// The highest priority wins; on a tie, the first offered.
pub fn best_recommendation(recs: impl Iterator<Item = Recommendation>) -> Option<Recommendation> {
    let mut best: Option<Recommendation> = None;
    for r in recs {
        if best.as_ref().is_none_or(|b| r.priority > b.priority) {
            best = Some(r);
        }
    }
    best
}

/// "Edit widgets…", centred under the widgets.
fn footer(app: &mut App, ui: &mut Ui, rect: Rect) {
    let p = ui.painter().clone();
    let g = p.layout_no_wrap("Edit widgets…".into(), font(12.0), TEXT_FAINT);
    let c = Pos2::new(rect.center().x, rect.bottom() - FOOTER / 2.0 - 2.0);
    let r = Rect::from_center_size(c, g.size() + Vec2::new(16.0, 8.0));
    let resp = ui.interact(r, ui.scope_id().with("edit-widgets"), Sense::click());
    p.galley(c - g.size() / 2.0, g, if resp.hovered() { TEXT_DIM } else { TEXT_FAINT });
    if resp.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    if resp.clicked() {
        let ctx = ui.ctx().clone();
        app.start_editing(&ctx);
    }
}

/// The settings menu under the gear: edit widgets, each tool's entries, quit.
fn gear_menu(app: &mut App, ui: &mut Ui, rect: Rect, cx: &mut Cx) {
    enum Row {
        Customize,
        Tool(usize, &'static str),
        Quit,
    }
    let mut rows: Vec<(Row, String)> = vec![(Row::Customize, "Edit widgets…".into())];
    for (i, t) in app.tools.iter().enumerate().filter(|(i, _)| app.tool_on(*i)) {
        for item in t.menu_items() {
            rows.push((Row::Tool(i, item.id), item.label));
        }
    }
    rows.push((Row::Quit, "Quit CleanMeBro".into()));
    let rh = 28.0;
    let h = rows.len() as f32 * rh + 10.0;
    let menu = Rect::from_min_size(Pos2::new(rect.right() - 14.0 - 210.0, rect.top() + 52.0), Vec2::new(210.0, h));
    let p = ui.painter().clone();
    p.rect_filled(menu.translate(Vec2::new(0.0, 4.0)).expand(2.0), R_CARD, egui::Color32::from_black_alpha(60));
    p.rect_filled(menu, R_CARD, RAISED);
    p.rect_stroke(menu, R_CARD, Stroke::new(1.0, white(HAIRLINE_STRONG)), egui::StrokeKind::Inside);
    let mut picked = None;
    for (k, (row, label)) in rows.iter().enumerate() {
        let rr = Rect::from_min_size(menu.min + Vec2::new(5.0, 5.0 + k as f32 * rh), Vec2::new(menu.width() - 10.0, rh));
        if matches!(row, Row::Quit) {
            hairline(&p, Pos2::new(rr.left() + 6.0, rr.top()), Pos2::new(rr.right() - 6.0, rr.top()));
        }
        let resp = ui.interact(rr, ui.scope_id().with(("gear", k)), Sense::click());
        if resp.hovered() {
            p.rect_filled(rr.shrink2(Vec2::new(0.0, 1.0)), R_SMALL, BLUE);
        }
        p.text(rr.left_center() + Vec2::new(10.0, 0.0), Align2::LEFT_CENTER, label, font(13.0), TEXT);
        if resp.clicked() {
            picked = Some(k);
        }
    }
    if let Some(k) = picked {
        app.gear_open = false;
        match rows[k].0 {
            Row::Customize => {
                let ctx = ui.ctx().clone();
                app.start_editing(&ctx);
            }
            Row::Tool(i, id) => app.tools[i].menu_action(id, cx),
            Row::Quit => cx.quit(),
        }
    }
}

/// The sheet beside the panel; the tool draws all of it, header included.
pub fn detail(app: &mut App, ui: &mut Ui, rect: Rect, cx: &mut Cx) {
    let Some((i, d)) = app.detail.clone() else { return };
    let mut child = ui.new_child(UiBuilder::new().max_rect(rect).id_salt(("detail", app.tools[i].id(), &d)));
    app.tools[i].detail_ui(&d, &mut child, rect, cx);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::tool::{Action, Badge};

    fn rec(priority: u32, title: &str) -> Recommendation {
        Recommendation {
            priority,
            badge: Badge { icon: Icon::Check, color: TEXT },
            title: title.into(),
            body: String::new(),
            action: String::new(),
            go: Action::CloseDetail,
        }
    }

    #[test]
    fn highest_priority_recommendation_wins() {
        assert!(best_recommendation(std::iter::empty()).is_none());
        let best = best_recommendation(vec![rec(5, "a"), rec(100, "b"), rec(100, "c"), rec(0, "d")].into_iter()).unwrap();
        assert_eq!(best.title, "b");
    }
}
