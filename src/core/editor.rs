//! Settings in the full window. Widgets: the one place to edit the menu-bar
//! panel. A searchable gallery of every widget, grouped by tool, next to a
//! live preview of the panel; drag widgets into the panel to add them,
//! around it to reorder, and back to the gallery to hide them; click one to
//! pick its size. Tools: turn whole tools on or off.

use super::panel::{self, Placed};
use super::settings::{hide_entry, place_entry, set_size, widget_key};
use super::shell::{App, CORE, RECOMMENDATION};
use super::tool::{Cx, Size};
use crate::kit::glass::*;
use crate::kit::mechanics as m;
use egui::{Align2, Color32, CornerRadius, Id, LayerId, Order, Pos2, Rect, ScrollArea, Sense, Stroke, StrokeKind, Ui, UiBuilder, Vec2};

/// A widget being dragged in the editor.
#[derive(Clone, Debug)]
pub struct Drag {
    pub key: String,
    /// Dragged out of the gallery (adding) rather than inside the panel.
    pub from_gallery: bool,
    pub label: String,
    pub size: Vec2,
    /// Where the pointer holds the dragged block, from its top-left corner.
    pub grab: Vec2,
    /// From the gallery: the size it is added at.
    pub as_size: Option<Size>,
}

const TOP_H: f32 = 52.0;
/// The list of widgets on the left of Settings › Widgets.
const LIST_W: f32 = 184.0;
const LIST_ROW_H: f32 = 30.0;
const END_SLOT_H: f32 = 40.0;
/// Under the panel preview: the selected widget's sizes.
const SIZE_BAR_H: f32 = 64.0;

/// What a drop would do right now.
#[derive(Clone, Debug, PartialEq)]
enum Target {
    /// Put the widget before this one (`None`: at the end).
    Insert(Option<String>),
    Hide,
    Nothing,
}

pub fn settings_page(app: &mut App, ui: &mut Ui, r: Rect) {
    let p = ui.painter().clone();
    let top = Rect::from_min_size(r.min, Vec2::new(r.width(), TOP_H));
    hairline(&p, top.left_bottom(), top.right_bottom());
    p.text(Pos2::new(top.left() + 24.0, top.center().y), Align2::LEFT_CENTER, "Settings", bold(15.0), TEXT);
    let tabs = Rect::from_center_size(top.center(), Vec2::new(180.0, 28.0));
    if let Some(t) = segmented(ui, tabs, ui.scope_id().with("settings-tabs"), &["Widgets", "Tools"], app.settings_tab) {
        app.settings_tab = t;
        app.drag = None;
    }
    let body = Rect::from_min_max(Pos2::new(r.left(), top.bottom() + 1.0), r.max);
    if app.settings_tab == 0 {
        widgets_tab(app, ui, body);
    } else {
        tools_tab(app, ui, body);
    }
}

// ---------------------------------------------------------------- widgets

struct Item {
    /// `None`: a planned tool's widget, which can't be added yet.
    key: Option<String>,
    title: String,
    /// The tool's name and the widget's one-liner, for search.
    words: String,
    /// How many sizes it comes in.
    sizes: usize,
    glyph: Icon,
    color: Color32,
    added: bool,
}

/// Whether `query` (any case, every word) matches the widget.
fn matches(query: &str, title: &str, words: &str) -> bool {
    let hay = format!("{title} {words}").to_lowercase();
    query.to_lowercase().split_whitespace().all(|w| hay.contains(w))
}

/// The gallery's groups, filtered by `query`: each enabled tool's widgets,
/// then the shell's. Groups with nothing left are dropped.
fn gallery(app: &App, query: &str) -> Vec<(String, Vec<Item>)> {
    let added = |k: &str| app.widgets.iter().any(|e| e.key == k && e.on);
    let mut groups = Vec::new();
    for (_, t) in app.tools.iter().enumerate().filter(|(i, _)| app.tool_on(*i)) {
        let b = t.badge();
        let items: Vec<Item> = t
            .widgets()
            .into_iter()
            .map(|w| {
                let key = widget_key(t.id(), w.id);
                Item { added: added(&key), key: Some(key), title: w.title.into(), words: format!("{} {}", t.name(), w.about), sizes: w.sizes.len(), glyph: b.icon, color: b.color }
            })
            .filter(|it| matches(query, &it.title, &it.words))
            .collect();
        if !items.is_empty() {
            groups.push((t.name().to_uppercase(), items));
        }
    }
    let core = [(RECOMMENDATION, "Recommendation", Icon::Sparkle)]
        .into_iter()
        .map(|(id, title, glyph)| {
            let key = widget_key(CORE, id);
            Item { added: added(&key), key: Some(key), title: title.into(), words: "suggestion tip today".into(), sizes: 1, glyph, color: tint::NEUTRAL }
        })
        .filter(|it| matches(query, &it.title, &it.words))
        .collect::<Vec<_>>();
    if !core.is_empty() {
        groups.push(("CORE".into(), core));
    }
    groups
}

fn widgets_tab(app: &mut App, ui: &mut Ui, body: Rect) {
    let ctx = ui.ctx().clone();
    let p = ui.painter().clone();
    let pointer = ctx.pointer_latest_pos();
    let lr = Rect::from_min_size(body.min, Vec2::new(LIST_W, body.height()));
    let vr = Rect::from_min_size(Pos2::new(lr.right() + 1.0, body.top()), Vec2::new(panel::inner_width() + 2.0 * sp::L, body.height()));
    let gr = Rect::from_min_max(lr.min, vr.max);
    let pr = Rect::from_min_max(Pos2::new(gr.right() + 1.0, body.top()), body.max);
    let dragging_out = app.drag.as_ref().is_some_and(|d| !d.from_gallery);
    let over_gallery = dragging_out && pointer.is_some_and(|q| gr.contains(q));

    // list and sizes: red tint while a panel widget hovers them (drop = hide)
    let gallery_bg = if over_gallery {
        lerp_color(SIDEBAR, RED, 0.12)
    } else if dragging_out {
        RAISED
    } else {
        SIDEBAR
    };
    p.rect_filled(gr, CornerRadius::ZERO, gallery_bg);
    hairline(&p, lr.right_top(), lr.right_bottom());
    hairline(&p, gr.right_top(), gr.right_bottom());

    // list: search, then each tool's widgets
    let sr = Rect::from_min_size(lr.min + Vec2::new(sp::M, sp::L), Vec2::new(LIST_W - 2.0 * sp::M, 28.0));
    let mut q = std::mem::take(&mut app.widget_search);
    let sresp = search_field(ui, sr, &mut q, "Search", false, 13.0);
    if sresp.has_focus() && ui.input(|i| i.key_pressed(egui::Key::Escape)) {
        q.clear();
    }
    app.widget_search = q;
    let query = app.widget_search.clone();
    let groups = gallery(app, &query);
    let keys: Vec<String> = groups.iter().flat_map(|(_, its)| its.iter().filter_map(|i| i.key.clone())).collect();
    if app.gallery_pick.as_ref().is_none_or(|k| !keys.contains(k)) {
        app.gallery_pick = keys.first().cloned();
    }
    let list_r = Rect::from_min_max(Pos2::new(lr.left(), sr.bottom() + sp::S), lr.max);
    let mut child = ui.new_child(UiBuilder::new().max_rect(list_r.shrink2(Vec2::new(sp::S, 0.0))).layout(egui::Layout::top_down(egui::Align::Min)));
    child.set_clip_rect(list_r);
    ScrollArea::vertical().id_salt("gallery-list").auto_shrink([false, false]).show(&mut child, |ui| {
        let w = ui.available_width();
        let h: f32 = groups.iter().map(|(_, its)| 26.0 + its.len() as f32 * LIST_ROW_H).sum::<f32>() + sp::S;
        let (origin, _) = ui.allocate_exact_size(Vec2::new(w, h.max(60.0)), Sense::hover());
        let p = ui.painter().clone();
        if groups.is_empty() {
            text_fit(&p, Pos2::new(origin.left() + 4.0, origin.top() + 16.0), Align2::LEFT_CENTER, &format!("No widgets match \"{}\"", query.trim()), font(12.0), TEXT_DIM, w - 8.0);
        }
        let mut y = origin.top();
        for (name, items) in &groups {
            section_header(&p, Pos2::new(origin.left() + 4.0, y + 12.0), Align2::LEFT_TOP, name);
            y += 26.0;
            for it in items {
                let rr = Rect::from_min_size(Pos2::new(origin.left(), y), Vec2::new(w, LIST_ROW_H - 2.0));
                y += LIST_ROW_H;
                let resp = ui.interact(rr, ui.scope_id().with(("gl", &it.title, &it.key)), Sense::click());
                let sel = it.key.is_some() && it.key == app.gallery_pick;
                if sel {
                    p.rect_filled(rr, 7.0, BLUE);
                } else if resp.hovered() {
                    p.rect_filled(rr, 7.0, white(12));
                }
                icon(&p, it.glyph, Rect::from_center_size(Pos2::new(rr.left() + 16.0, rr.center().y), Vec2::splat(14.0)), if sel { Color32::WHITE } else { lift(it.color) });
                let meta = if it.sizes == 1 { "1 size".to_string() } else { format!("{} sizes", it.sizes) };
                let mr = p.text(Pos2::new(rr.right() - 8.0, rr.center().y), Align2::RIGHT_CENTER, meta, ty::caption(), if sel { white(205) } else { ty::caption_color() });
                let mut right = mr.left() - 6.0;
                if it.added {
                    p.circle_filled(Pos2::new(right - 3.0, rr.center().y), 3.0, GREEN);
                    right -= 12.0;
                }
                text_fit(&p, Pos2::new(rr.left() + 32.0, rr.center().y), Align2::LEFT_CENTER, &it.title, ty::body(), if sel { Color32::WHITE } else { TEXT }, right - rr.left() - 36.0);
                if resp.clicked() {
                    app.gallery_pick = it.key.clone();
                }
            }
        }
    });

    // sizes: the picked widget, drawn for real at every size it comes in
    if let Some(key) = app.gallery_pick.clone() {
        if let Some(z) = variants(app, ui, vr, &key) {
            place_entry(&mut app.widgets, &key, None);
            set_size(&mut app.widgets, &key, z);
            app.edit_sel = Some(key);
        }
    }

    // preview: a hint of the menu bar, the title row, then the panel
    let p = ui.painter().clone();
    p.rect_filled(pr, CornerRadius { se: 12, ..Default::default() }, BG);
    let bar = Rect::from_min_size(pr.min, Vec2::new(pr.width(), 26.0));
    p.rect_filled(bar, CornerRadius::ZERO, white(13));
    hairline(&p, bar.left_bottom(), bar.right_bottom());
    let pw = m::PANEL_WIDTH;
    let px = pr.center().x - pw / 2.0;
    // the app's menu bar icon, highlighted, above the panel it opens
    let tray = Rect::from_center_size(Pos2::new(px + pw - 40.0, bar.center().y), Vec2::splat(15.0));
    p.rect_filled(tray, 4.0, white(220));
    p.rect_stroke(tray.expand(3.0), 6.0, Stroke::new(1.5, with_alpha(ACCENT, 0.8)), StrokeKind::Outside);
    icon(&p, Icon::Wifi, Rect::from_center_size(Pos2::new(tray.center().x + 30.0, bar.center().y), Vec2::splat(14.0)), TEXT_SOFT);
    icon(&p, Icon::Battery(80), Rect::from_center_size(Pos2::new(tray.center().x - 30.0, bar.center().y), Vec2::splat(16.0)), TEXT_SOFT);
    let title_y = bar.bottom() + 12.0;
    p.text(Pos2::new(px, title_y + 12.0), Align2::LEFT_CENTER, "Menu bar panel", bold(13.0), TEXT);
    let rw = button_width(&p, "Reset", BTN_SECONDARY);
    let reset = Rect::from_min_size(Pos2::new(px + pw - rw, title_y), Vec2::new(rw, 24.0));
    if pill_button(ui, reset, "Reset", BTN_SECONDARY, true).on_hover_text("Back to the default widgets").clicked() {
        app.reset_widgets();
    }
    let shown = panel::place(app, panel::inner_width()).0.len();
    p.text(Pos2::new(reset.left() - 10.0, title_y + 12.0), Align2::RIGHT_CENTER, format!("{shown} widget{}", if shown == 1 { "" } else { "s" }), font(12.0), TEXT_FAINT);
    let boxr = Rect::from_min_max(Pos2::new(px, title_y + 36.0), Pos2::new(px + pw, pr.bottom() - 20.0 - SIZE_BAR_H));
    p.rect_filled(boxr.translate(Vec2::new(0.0, 12.0)).expand(4.0), 18.0, Color32::from_black_alpha(70));
    p.rect_filled(boxr, 14.0, BG);
    p.rect_stroke(boxr, 14.0, Stroke::new(1.0, white(31)), StrokeKind::Inside);
    panel::title_line(app, &p, boxr.left() + 16.0, boxr.top(), pw - 32.0);

    let area = Rect::from_min_max(Pos2::new(boxr.left() + panel::PAD, boxr.top() + panel::HEADER - 8.0), Pos2::new(boxr.right() - panel::PAD, boxr.bottom() - 10.0));
    let (placed, content_h) = panel::place(app, area.width());
    let need = content_h + panel::GAP + END_SLOT_H + 8.0;
    let mut target = Target::Nothing;
    let mut hide: Option<String> = None;
    let mut pick: Option<Pick> = None;
    // the scroll area reaches into the right padding, so its bar runs beside
    // the widgets (and the selection ring and handle) rather than over them
    let scroll_r = Rect::from_min_max(area.min, Pos2::new(area.right() + panel::PAD - 2.0, area.bottom()));
    let mut child = ui.new_child(UiBuilder::new().max_rect(scroll_r).layout(egui::Layout::top_down(egui::Align::Min)));
    // a slim bar, so even hovered it stays clear of the widgets
    child.spacing_mut().scroll.bar_width = 6.0;
    child.set_clip_rect(area.expand2(Vec2::new(panel::PAD, 10.0)));
    ScrollArea::vertical().id_salt("preview").auto_shrink([false, false]).show(&mut child, |ui| {
        // room above the first row for its "−" badges
        ui.add_space(8.0);
        let (origin, _) = ui.allocate_exact_size(Vec2::new(area.width(), need.max(area.height() - 8.0)), Sense::hover());
        let clip = ui.clip_rect();
        let (t, h, k) = preview(app, ui, origin, clip, &placed, content_h, pointer);
        target = t;
        hide = h;
        pick = k;
    });
    if over_gallery {
        target = Target::Hide;
    }
    if let Some(k) = hide {
        hide_entry(&mut app.widgets, &k);
        if app.edit_sel.as_deref() == Some(k.as_str()) {
            app.edit_sel = None;
        }
    }
    match pick {
        Some(Pick::Select(k)) => app.edit_sel = if app.edit_sel.as_deref() == Some(k.as_str()) { None } else { Some(k) },
        Some(Pick::Grow(k)) => step_size(app, &k),
        None => {}
    }
    let bar = Rect::from_min_size(Pos2::new(px, boxr.bottom() + 14.0), Vec2::new(pw, SIZE_BAR_H - 8.0));
    size_bar(app, ui, bar);

    // the dragged block follows the pointer; dropping applies the target
    if let Some(d) = app.drag.clone() {
        ctx.set_cursor_icon(egui::CursorIcon::Grabbing);
        if let Some(q) = pointer {
            let gp = ctx.layer_painter(LayerId::new(Order::Tooltip, Id::unique("editor-drag")));
            // a slim tag, so the insert line under it stays visible
            let size = Vec2::new(d.size.x.min(240.0), 44.0);
            let gr = Rect::from_min_size(q - Vec2::new(d.grab.x.min(size.x - 12.0), 22.0), size);
            gp.rect_filled(gr.translate(Vec2::new(0.0, 8.0)).expand(2.0), R_CARD + 2.0, Color32::from_black_alpha(80));
            gp.rect_filled(gr, R_CARD, with_alpha(RAISED, 0.92));
            gp.rect_stroke(gr, R_CARD, Stroke::new(1.0, if target == Target::Hide { RED } else { white(40) }), StrokeKind::Inside);
            text_fit(&gp, Pos2::new(gr.left() + 12.0, gr.center().y), Align2::LEFT_CENTER, &d.label, bold(13.0), TEXT, gr.width() - 24.0);
        }
        if !ctx.input(|i| i.pointer.primary_down()) {
            match target {
                Target::Insert(before) if !d.key.is_empty() => {
                    if before.as_deref() != Some(d.key.as_str()) {
                        place_entry(&mut app.widgets, &d.key, before.as_deref());
                    }
                    if let Some(z) = d.as_size {
                        set_size(&mut app.widgets, &d.key, z);
                    }
                }
                Target::Hide => hide_entry(&mut app.widgets, &d.key),
                _ => {}
            }
            app.drag = None;
        }
        ctx.request_repaint();
    }
    // the panel's widgets jiggle while editing
    crate::kit::glass::animate(&ctx);
}

/// The picked widget at each size it comes in, drawn live at real size, the
/// two small ones side by side. Drag one into the panel, or click it to add
/// it at that size (returned).
fn variants(app: &mut App, ui: &mut Ui, r: Rect, key: &str) -> Option<Size> {
    let (owner, spec) = app.widget_spec(key)?;
    let p = ui.painter().clone();
    let x = r.left() + sp::L;
    let w = panel::inner_width();
    let mut y = r.top() + sp::L;
    text_fit(&p, Pos2::new(x, y), Align2::LEFT_TOP, spec.title, ty::title(), TEXT, w);
    y += 16.0 + sp::XS;
    let n = spec.sizes.len();
    let hint = if n == 1 { "Drag it into the panel, or click to add it.".to_string() } else { format!("{n} sizes. Drag the one you want into the panel, or click it.") };
    text_fit(&p, Pos2::new(x, y), Align2::LEFT_TOP, &hint, ty::caption(), ty::caption_color(), w);
    y += 14.0 + sp::M;
    let cur = app.widgets.iter().find(|e| e.key == key && e.on).map(|e| spec.fit(e.size));
    // rows: Small and Wide share one; the rest get their own
    let mut rows: Vec<Vec<Size>> = vec![];
    let small: Vec<Size> = spec.sizes.iter().copied().filter(|z| z.cols() <= 2).collect();
    if !small.is_empty() {
        rows.push(small);
    }
    rows.extend(spec.sizes.iter().copied().filter(|z| z.cols() > 2).map(|z| vec![z]));
    let label_h = 14.0 + sp::XS;
    let total: f32 = rows.iter().map(|row| label_h + row.iter().map(|z| panel::size_px(*z, w).y).fold(0.0, f32::max) + sp::L).sum();
    let area = Rect::from_min_max(Pos2::new(r.left(), y), r.max);
    let mut child = ui.new_child(UiBuilder::new().id_salt("gallery-sizes").max_rect(area).layout(egui::Layout::top_down(egui::Align::Min)));
    child.set_clip_rect(area);
    let mut quiet = Cx::new(None);
    let mut out = None;
    let dragging = app.drag.is_some();
    ScrollArea::vertical().id_salt(("gallery-sizes", key)).auto_shrink([false, false]).show(&mut child, |ui| {
        let (origin, _) = ui.allocate_exact_size(Vec2::new(area.width(), total + sp::S), Sense::hover());
        let p = ui.painter().clone();
        let mut y = origin.top();
        for row in &rows {
            let mut cx = x;
            let mut row_h: f32 = 0.0;
            for z in row {
                let sz = panel::size_px(*z, w);
                let here = cur == Some(*z);
                let label = if here { format!("{} · {} · in the panel", z.name(), z.label()) } else { format!("{} · {}", z.name(), z.label()) };
                text_fit(&p, Pos2::new(cx, y), Align2::LEFT_TOP, &label, ty::caption(), if here { ACCENT } else { ty::caption_color() }, sz.x.max(150.0));
                let wr = Rect::from_min_size(Pos2::new(cx, y + label_h), sz);
                panel::draw_at(app, ui, owner, spec.id, *z, wr, &mut quiet);
                let resp = ui.interact(wr, ui.scope_id().with(("var", key, z.label())), Sense::click_and_drag());
                if here {
                    p.rect_stroke(wr.expand(2.0), R_CARD + 2.0, Stroke::new(2.0, ACCENT), StrokeKind::Outside);
                } else if resp.hovered() && !dragging {
                    p.rect_stroke(wr, R_CARD, Stroke::new(1.0, white(46)), StrokeKind::Inside);
                }
                if resp.hovered() && !dragging {
                    ui.ctx().set_cursor_icon(egui::CursorIcon::Grab);
                }
                if resp.drag_started() && !dragging {
                    let grab = resp.interact_pointer_pos().map(|q| q - wr.min).unwrap_or(Vec2::splat(12.0));
                    app.drag = Some(Drag { key: key.to_string(), from_gallery: true, label: spec.title.to_string(), size: sz, grab, as_size: Some(*z) });
                } else if resp.on_hover_text(if here { "In the panel at this size" } else { "Click to add at this size, or drag into the panel" }).clicked() {
                    out = Some(*z);
                }
                cx += sz.x + panel::GAP;
                row_h = row_h.max(sz.y);
            }
            y += label_h + row_h + sp::L;
        }
    });
    out
}

/// A click on a widget in the preview.
enum Pick {
    /// Select it (or unselect it again).
    Select(String),
    /// Its corner handle: the next size.
    Grow(String),
}

/// The widget's next size it offers (wrapping around).
fn step_size(app: &mut App, key: &str) {
    let Some((_, spec)) = app.widget_spec(key) else { return };
    let Some(cur) = app.widgets.iter().find(|e| e.key == key).map(|e| spec.fit(e.size)) else { return };
    let i = spec.sizes.iter().position(|z| *z == cur).unwrap_or(0);
    set_size(&mut app.widgets, key, spec.sizes[(i + 1) % spec.sizes.len()]);
}

/// Under the panel: the selected widget's name and its size chips, or a hint.
fn size_bar(app: &mut App, ui: &mut Ui, r: Rect) {
    let p = ui.painter().clone();
    let sel = app.edit_sel.clone().and_then(|k| {
        let (_, spec) = app.widget_spec(&k)?;
        let cur = spec.fit(app.widgets.iter().find(|e| e.key == k && e.on)?.size);
        Some((k, spec, cur))
    });
    let Some((key, spec, cur)) = sel else {
        text_fit(&p, r.center(), Align2::CENTER_CENTER, "Click a widget to resize it · drag to move · − to remove", font(12.0), TEXT_FAINT, r.width());
        return;
    };
    let ty = r.top() + 8.0;
    let tr = p.text(Pos2::new(r.left(), ty), Align2::LEFT_CENTER, spec.title, bold(12.0), TEXT);
    p.text(Pos2::new(tr.right() + 6.0, ty), Align2::LEFT_CENTER, "size", font(12.0), TEXT_FAINT);
    p.text(Pos2::new(r.right(), ty), Align2::RIGHT_CENTER, format!("{} · {}", cur.name(), cur.label()), font(12.0), TEXT_FAINT);
    let chips = Rect::from_min_max(Pos2::new(r.left(), ty + 14.0), Pos2::new(r.right(), ty + 44.0));
    if let Some(z) = panel::size_chips(ui, chips, ui.scope_id().with(("sizes", &key)), spec.sizes, cur) {
        set_size(&mut app.widgets, &key, z);
    }
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

/// The panel's widgets, drawn for real but not live: clicks select them and
/// drags pick them up instead. Returns the drop target under the pointer, a
/// widget whose "−" was clicked, and a click on a widget or its handle.
#[allow(clippy::too_many_arguments)]
fn preview(app: &mut App, ui: &mut Ui, origin: Rect, clip: Rect, placed: &[Placed], content_h: f32, pointer: Option<Pos2>) -> (Target, Option<String>, Option<Pick>) {
    let ctx = ui.ctx().clone();
    let p = ui.painter().clone();
    let t = ctx.input(|i| i.time) as f32;
    let at = |r: Rect| r.translate(origin.min.to_vec2());
    let dragged = app.drag.as_ref().map(|d| d.key.clone());
    // tools' widgets may ask for things (open a sheet…): ignore it here
    let mut quiet = Cx::new(None);
    let mut hide = None;
    let mut pick = None;
    let mut keys: Vec<String> = Vec::with_capacity(placed.len());
    for (n, w) in placed.iter().enumerate() {
        let key = app.widgets[w.entry].key.clone();
        let base = at(w.rect);
        let phase = n as f32 * 1.7;
        let jig = if app.drag.is_some() { Vec2::ZERO } else { Vec2::new((t * 17.0 + phase).sin() * 0.6, (t * 21.0 + phase).cos() * 0.35) };
        let r = base.translate(jig);
        panel::draw_widget(app, ui, w, r, &mut quiet);
        let id = ui.scope_id().with(("ed", &key));
        let resp = ui.interact(base, id, Sense::click_and_drag());
        let selected = app.edit_sel.as_deref() == Some(key.as_str());
        if dragged.as_deref() == Some(key.as_str()) {
            p.rect_filled(base, R_CARD, with_alpha(BG, 0.7));
        } else if selected {
            p.rect_stroke(base.expand(2.0), R_CARD + 2.0, Stroke::new(2.0, ACCENT), StrokeKind::Outside);
        } else if resp.hovered() && app.drag.is_none() {
            p.rect_stroke(base, R_CARD, Stroke::new(1.0, white(46)), StrokeKind::Inside);
        }
        if resp.hovered() && app.drag.is_none() {
            ctx.set_cursor_icon(egui::CursorIcon::Grab);
        }
        if resp.clicked() {
            pick = Some(Pick::Select(key.clone()));
        }
        if resp.drag_started() && app.drag.is_none() {
            let title = app.widget_spec(&key).map(|(_, s)| s.title.to_string()).unwrap_or_default();
            let grab = resp.interact_pointer_pos().map(|q| q - base.min).unwrap_or(Vec2::splat(12.0));
            app.drag = Some(Drag { key: key.clone(), from_gallery: false, label: title, size: base.size(), grab, as_size: None });
        }
        // "−" hides it
        let mb = Rect::from_center_size(base.left_top() + Vec2::new(3.0, 3.0), Vec2::splat(20.0));
        if app.drag.is_none() && clip.contains(mb.center()) {
            let b = ui.interact(mb, id.with("hide"), Sense::click());
            p.circle_filled(mb.center() + Vec2::new(0.0, 1.5), 10.5, Color32::from_black_alpha(90));
            p.circle_filled(mb.center(), 10.0, if b.hovered() { Color32::from_rgb(0x8E, 0x8E, 0x93) } else { Color32::from_rgb(0x6E, 0x6E, 0x73) });
            p.line_segment([mb.center() - Vec2::new(4.5, 0.0), mb.center() + Vec2::new(4.5, 0.0)], Stroke::new(1.8, Color32::WHITE));
            if b.on_hover_text("Hide").clicked() {
                hide = Some(key.clone());
            }
        }
        // the selected one's corner handle: next size
        let hb = Rect::from_center_size(base.right_bottom() - Vec2::new(9.0, 9.0), Vec2::splat(22.0));
        if selected && app.drag.is_none() && clip.contains(hb.center()) {
            let b = ui.interact(hb, id.with("grow"), Sense::click());
            p.circle_filled(hb.center(), 11.0, BG);
            p.circle_filled(hb.center(), 9.0, if b.hovered() { lerp_color(ACCENT, Color32::WHITE, 0.2) } else { ACCENT });
            resize_glyph(&p, hb.center(), 4.0, Color32::WHITE);
            if b.hovered() {
                ctx.set_cursor_icon(egui::CursorIcon::ResizeNwSe);
            }
            if b.on_hover_text("Next size").clicked() {
                pick = Some(Pick::Grow(key.clone()));
            }
        }
        keys.push(key);
    }

    // where a drop would land
    let end = Rect::from_min_size(Pos2::new(origin.left(), origin.top() + content_h + panel::GAP), Vec2::new(origin.width(), END_SLOT_H));
    let mut target = Target::Nothing;
    let mut line: Option<[Pos2; 2]> = None;
    if let (Some(d), Some(q)) = (&app.drag, pointer) {
        if clip.contains(q) {
            target = Target::Insert(None);
            let others: Vec<(usize, Rect)> = placed.iter().enumerate().filter(|(k, _)| keys[*k] != d.key).map(|(k, w)| (k, at(w.rect))).collect();
            if q.y < end.top() {
                let nearest = others.iter().min_by(|a, b| dist(a.1, q).total_cmp(&dist(b.1, q)));
                if let Some(&(k, r)) = nearest {
                    let full = r.width() > origin.width() * 0.9;
                    let before = if full { q.y < r.center().y } else { q.x < r.center().x };
                    if before {
                        target = Target::Insert(Some(keys[k].clone()));
                        line = Some(if full { [Pos2::new(r.left(), r.top() - 3.0), Pos2::new(r.right(), r.top() - 3.0)] } else { [Pos2::new(r.left() - 5.0, r.top()), Pos2::new(r.left() - 5.0, r.bottom())] });
                    } else {
                        let next = others.iter().find(|(j, _)| *j > k).map(|(j, _)| keys[*j].clone());
                        target = Target::Insert(next);
                        line = Some(if full { [Pos2::new(r.left(), r.bottom() + 3.0), Pos2::new(r.right(), r.bottom() + 3.0)] } else { [Pos2::new(r.right() + 5.0, r.top()), Pos2::new(r.right() + 5.0, r.bottom())] });
                    }
                }
            }
        }
    }
    if let Some([a, b]) = line {
        p.line_segment([a, b], Stroke::new(3.0, ACCENT));
    }
    // the slot at the end
    let on_end = target == Target::Insert(None) && line.is_none();
    if on_end {
        p.rect_filled(end, 10.0, with_alpha(BLUE, 0.12));
    }
    dashed_rect(&p, end, 10.0, if on_end { ACCENT } else { white(40) });
    let label = if app.drag.is_some() { "Drop here to add at the end" } else { "Drag widgets here" };
    p.text(end.center(), Align2::CENTER_CENTER, label, font(12.0), if on_end { ACCENT } else { TEXT_MUTED });
    (target, hide, pick)
}

fn dist(r: Rect, q: Pos2) -> f32 {
    let dx = (r.left() - q.x).max(q.x - r.right()).max(0.0);
    let dy = (r.top() - q.y).max(q.y - r.bottom()).max(0.0);
    dx * dx + dy * dy
}

// ---------------------------------------------------------------- tools

/// "Cmd+Meow 0.2.0 (57)": the Cargo.toml version plus the build number that
/// scripts/bundle-mac.sh passes in as CMDMEOW_BUILD (commit count).
pub fn version_label() -> String {
    match option_env!("CMDMEOW_BUILD") {
        Some(b) if !b.is_empty() => format!("Cmd+Meow {} ({b})", env!("CARGO_PKG_VERSION")),
        _ => format!("Cmd+Meow {}", env!("CARGO_PKG_VERSION")),
    }
}

fn tools_tab(app: &mut App, ui: &mut Ui, body: Rect) {
    let p = ui.painter().clone();
    let w = 560.0f32.min(body.width() - 48.0);
    let x = body.center().x - w / 2.0;
    let mut y = body.top() + 24.0;
    // general
    p.text(Pos2::new(x, y), Align2::LEFT_TOP, "General", bold(15.0), TEXT);
    y += 28.0;
    let card = Rect::from_min_size(Pos2::new(x, y), Vec2::new(w, 54.0));
    glass_card(&p, card, R_CARD, 0.0, false);
    let sw = Rect::from_min_size(Pos2::new(card.right() - 42.0, card.center().y - 9.0), Vec2::new(30.0, 18.0));
    p.text(Pos2::new(card.left() + 16.0, card.top() + 11.0), Align2::LEFT_TOP, "Open at login", font(13.0), TEXT);
    p.text(Pos2::new(card.left() + 16.0, card.top() + 29.0), Align2::LEFT_TOP, "Start Cmd+Meow in the menu bar when you log in to your Mac", font(11.0), TEXT_FAINT);
    let on = crate::kit::login::enabled();
    if switch(ui, sw, ui.scope_id().with("open-at-login"), on, true).clicked() {
        if let Err(e) = crate::kit::login::set(!on) {
            eprintln!("[login] {e}");
        }
    }
    p.text(Pos2::new(x + 2.0, card.bottom() + 8.0), Align2::LEFT_TOP, version_label(), font(11.0), TEXT_FAINT);
    y = card.bottom() + 44.0;
    p.text(Pos2::new(x, y), Align2::LEFT_TOP, "Tools", bold(15.0), TEXT);
    p.text(Pos2::new(x, y + 20.0), Align2::LEFT_TOP, "Each tool is separate. Turning one off hides its widgets and its page.", font(12.0), TEXT_DIM);
    y += 50.0;
    let rows = app.tools.len();
    let row_h = 46.0;
    let card = Rect::from_min_size(Pos2::new(x, y), Vec2::new(w, rows as f32 * row_h + 8.0));
    glass_card(&p, card, R_CARD, 0.0, false);
    let mut flip: Option<(usize, bool)> = None;
    for k in 0..rows {
        let rr = Rect::from_min_size(Pos2::new(card.left() + 12.0, card.top() + 4.0 + k as f32 * row_h), Vec2::new(w - 24.0, row_h));
        if k > 0 {
            hairline(&p, Pos2::new(rr.left() + 38.0, rr.top()), Pos2::new(rr.right(), rr.top()));
        }
        let chip = Rect::from_min_size(Pos2::new(rr.left(), rr.center().y - 13.0), Vec2::splat(26.0));
        let sw = Rect::from_min_size(Pos2::new(rr.right() - 30.0, rr.center().y - 9.0), Vec2::new(30.0, 18.0));
        let tw = sw.left() - chip.right() - 24.0;
        let t = &app.tools[k];
        {
            let b = t.badge();
            tool_chip(&p, chip, b.icon, b.color);
            text_fit(&p, Pos2::new(chip.right() + 12.0, rr.top() + 8.0), Align2::LEFT_TOP, t.name(), font(13.0), TEXT, tw);
            text_fit(&p, Pos2::new(chip.right() + 12.0, rr.top() + 26.0), Align2::LEFT_TOP, t.about(), font(11.0), TEXT_FAINT, tw);
            let on = app.tool_on(k);
            let id = ui.scope_id().with(("tool-on", t.id()));
            if switch(ui, sw, id, on, true).on_hover_text(if on { "Turn off" } else { "Turn on" }).clicked() {
                flip = Some((k, !on));
            }
        }
    }
    if let Some((i, on)) = flip {
        app.set_tool_on(i, on);
    }
}
