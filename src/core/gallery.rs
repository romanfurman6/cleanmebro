//! The widget gallery: the sheet beside the panel while editing. A list of
//! every widget by tool; picking one shows it live at each size it offers,
//! with "Add widget" (or drag the preview into the panel).

use super::editor::Drag;
use super::panel::{self, size_chips, size_px};
use super::settings::{add_entry, set_size, widget_key};
use super::shell::{App, CORE, RECOMMENDATION};
use super::tool::{Cx, Size, WidgetSpec};
use crate::kit::glass::*;
use crate::kit::tile;
use egui::{Align2, Color32, Pos2, Rect, ScrollArea, Sense, Stroke, StrokeKind, Ui, UiBuilder, Vec2};

const ROW_H: f32 = 58.0;

/// A widget in the gallery list.
struct Item {
    key: String,
    owner: Option<usize>,
    spec: WidgetSpec,
    glyph: Icon,
    color: Color32,
    tool: &'static str,
}

/// Every widget of the enabled tools, grouped by the tools' sidebar
/// sections, then the shell's own.
fn items(app: &App) -> Vec<(&'static str, Vec<Item>)> {
    let mut groups: Vec<(&'static str, Vec<Item>)> = Vec::new();
    for (i, t) in app.tools.iter().enumerate().filter(|(i, _)| app.tool_on(*i)) {
        let b = t.badge();
        for spec in t.widgets() {
            let it = Item { key: widget_key(t.id(), spec.id), owner: Some(i), spec, glyph: b.icon, color: b.color, tool: t.name() };
            match groups.iter_mut().find(|(s, _)| *s == t.section()) {
                Some((_, v)) => v.push(it),
                None => groups.push((t.section(), vec![it])),
            }
        }
    }
    let key = widget_key(CORE, RECOMMENDATION);
    if let Some((_, spec)) = app.widget_spec(&key) {
        groups.push(("Suggestions", vec![Item { key, owner: None, spec, glyph: Icon::Sparkle, color: tint::NEUTRAL, tool: "CleanMeBro" }]));
    }
    groups
}

fn in_panel(app: &App, key: &str) -> Option<Size> {
    app.widgets.iter().find(|e| e.key == key && e.on).map(|e| e.size)
}

pub fn sheet(app: &mut App, ui: &mut Ui, rect: Rect) {
    let pick = app.gallery.as_ref().and_then(|g| g.pick.clone());
    let found = pick.and_then(|k| items(app).into_iter().flat_map(|(_, v)| v).find(|it| it.key == k));
    match found {
        Some(it) => detail(app, ui, rect, it),
        None => list(app, ui, rect),
    }
}

fn close(app: &mut App, ui: &Ui) {
    app.gallery = None;
    app.reshape(ui.ctx());
}

/// The list: a row per widget with the sizes it offers.
fn list(app: &mut App, ui: &mut Ui, rect: Rect) {
    let p = ui.painter().clone();
    p.text(rect.min + Vec2::new(18.0, 20.0), Align2::LEFT_TOP, "Add a widget", bold(15.0), TEXT);
    p.text(rect.min + Vec2::new(18.0, 40.0), Align2::LEFT_TOP, "Pick one to see its sizes.", font(12.0), TEXT_DIM);
    let cr = Rect::from_center_size(Pos2::new(rect.right() - 27.0, rect.top() + 33.0), Vec2::splat(26.0));
    if round_button(ui, cr, Icon::Close, "Close").clicked() {
        close(app, ui);
        return;
    }
    let body = Rect::from_min_max(Pos2::new(rect.left() + 12.0, rect.top() + 66.0), Pos2::new(rect.right() - 12.0, rect.bottom() - 8.0));
    let groups = items(app);
    let h: f32 = groups.iter().map(|(_, v)| 26.0 + v.len() as f32 * (ROW_H + 6.0) + 8.0).sum();
    let mut picked = None;
    let mut child = ui.new_child(UiBuilder::new().max_rect(body).layout(egui::Layout::top_down(egui::Align::Min)));
    child.set_clip_rect(body.expand2(Vec2::new(4.0, 0.0)));
    ScrollArea::vertical().id_salt("gallery-list").auto_shrink([false, false]).show(&mut child, |ui| {
        let (origin, _) = ui.allocate_exact_size(Vec2::new(body.width(), h), Sense::hover());
        let p = ui.painter().clone();
        let mut y = origin.top();
        for (section, list) in &groups {
            section_header(&p, Pos2::new(origin.left() + 4.0, y + 6.0), Align2::LEFT_TOP, &section.to_uppercase());
            y += 26.0;
            for it in list {
                let r = Rect::from_min_size(Pos2::new(origin.left(), y), Vec2::new(origin.width(), ROW_H));
                if row(ui, r, it, in_panel(app, &it.key)).clicked() {
                    picked = Some((it.key.clone(), in_panel(app, &it.key).unwrap_or(it.spec.size)));
                }
                y += ROW_H + 6.0;
            }
            y += 8.0;
        }
    });
    if let Some((k, z)) = picked {
        if let Some(g) = &mut app.gallery {
            g.pick = Some(k);
            g.size = Some(z);
        }
    }
}

fn row(ui: &mut Ui, r: Rect, it: &Item, shown: Option<Size>) -> egui::Response {
    let resp = tile::card(ui, r, ui.scope_id().with(("gal-row", &it.key)), false);
    let p = ui.painter().clone();
    let chip = Rect::from_min_size(Pos2::new(r.left() + 12.0, r.center().y - 15.0), Vec2::splat(30.0));
    tool_chip(&p, chip, it.glyph, it.color);
    // the sizes it offers, as little silhouettes
    let mut x = r.right() - 12.0;
    for z in it.spec.sizes.iter().rev() {
        let (rows, cols) = z.dims();
        let unit = 4.0;
        let s = Vec2::new(cols as f32 * unit + (cols as f32 - 1.0), rows as f32 * unit + (rows as f32 - 1.0));
        let sr = Rect::from_min_size(Pos2::new(x - s.x, r.center().y - s.y / 2.0), s);
        p.rect_filled(sr, 1.5, if shown == Some(*z) { ACCENT } else { white(60) });
        x = sr.left() - 5.0;
    }
    let x0 = chip.right() + 12.0;
    let tr = text_fit(&p, Pos2::new(x0, r.center().y - 8.0), Align2::LEFT_CENTER, it.spec.title, bold(13.0), TEXT, x - x0 - 70.0);
    if shown.is_some() {
        chip_tag(&p, Pos2::new(tr.right() + 6.0, tr.center().y), "In panel");
    }
    text_fit(&p, Pos2::new(x0, r.center().y + 9.0), Align2::LEFT_CENTER, it.spec.about, font(11.0), TEXT_FAINT, x - x0 - 8.0);
    resp
}

fn chip_tag(p: &egui::Painter, left_center: Pos2, label: &str) {
    let g = p.layout_no_wrap(label.into(), font(10.0), TEXT_DIM);
    let r = Rect::from_min_size(left_center - Vec2::new(0.0, 8.0), Vec2::new(g.size().x + 10.0, 16.0));
    p.rect_filled(r, 8.0, white(22));
    p.galley(r.center() - g.size() / 2.0, g, TEXT_DIM);
}

/// One widget: live preview at the chosen size, a pager through its sizes,
/// and "Add widget".
fn detail(app: &mut App, ui: &mut Ui, rect: Rect, it: Item) {
    let ctx = ui.ctx().clone();
    let p = ui.painter().clone();
    let shown = in_panel(app, &it.key);
    let size = it.spec.fit(app.gallery.as_ref().and_then(|g| g.size).unwrap_or(it.spec.size));

    // header: back, tool chip and name, close
    let back = Rect::from_center_size(Pos2::new(rect.left() + 27.0, rect.top() + 29.0), Vec2::splat(26.0));
    if round_button(ui, back, Icon::Back, "All widgets").clicked() {
        if let Some(g) = &mut app.gallery {
            g.pick = None;
        }
        return;
    }
    let chip = Rect::from_center_size(Pos2::new(back.right() + 22.0, back.center().y), Vec2::splat(22.0));
    tool_chip(&p, chip, it.glyph, it.color);
    p.text(Pos2::new(chip.right() + 8.0, chip.center().y), Align2::LEFT_CENTER, it.tool, font(12.0), TEXT_DIM);
    let cr = Rect::from_center_size(Pos2::new(rect.right() - 27.0, rect.top() + 29.0), Vec2::splat(26.0));
    if round_button(ui, cr, Icon::Close, "Close").clicked() {
        close(app, ui);
        return;
    }
    p.text(Pos2::new(rect.center().x, rect.top() + 68.0), Align2::CENTER_CENTER, it.spec.title, bold(17.0), TEXT);
    text_fit(&p, Pos2::new(rect.center().x, rect.top() + 90.0), Align2::CENTER_CENTER, it.spec.about, font(12.0), TEXT_DIM, rect.width() - 40.0);

    // the preview, in a stage as tall as its biggest size
    let width = panel::inner_width();
    let tallest = it.spec.sizes.iter().map(|z| size_px(*z, width).y).fold(0.0, f32::max);
    let stage = Rect::from_min_size(Pos2::new(rect.center().x - width / 2.0, rect.top() + 110.0), Vec2::new(width, tallest.max(size_px(Size::S12, width).y) + 8.0));
    let ps = size_px(size, width);
    let pr = Rect::from_center_size(Pos2::new(stage.center().x, stage.top() + 4.0 + ps.y / 2.0 + (tallest - ps.y) / 2.0), ps);
    p.rect_filled(pr.translate(Vec2::new(0.0, 6.0)).expand(2.0), tile::R_TILE + 2.0, Color32::from_black_alpha(60));
    let mut quiet = Cx::new(None);
    panel::draw_at(app, ui, it.owner, it.spec.id, size, pr, &mut quiet);
    // on top of the widget's own controls: drag it into the panel
    let grab = ui.interact(pr, ui.scope_id().with(("gal-preview", &it.key)), Sense::click_and_drag());
    if grab.hovered() && app.drag.is_none() {
        ctx.set_cursor_icon(egui::CursorIcon::Grab);
        p.rect_stroke(pr, tile::R_TILE, Stroke::new(1.0, white(60)), StrokeKind::Inside);
    }
    if grab.drag_started() && app.drag.is_none() {
        let g = grab.interact_pointer_pos().map(|q| q - pr.min).unwrap_or(Vec2::splat(12.0));
        app.drag = Some(Drag { key: it.key.clone(), from_gallery: true, label: format!("{} · {}", it.spec.title, size.label()), size: pr.size(), grab: g });
    }

    // pager: ‹ name · dots › then the chips
    let mut y = stage.bottom() + 14.0;
    let idx = it.spec.sizes.iter().position(|z| *z == size).unwrap_or(0);
    let n = it.spec.sizes.len();
    let mut pick = None;
    let dots_w = n as f32 * 12.0;
    for k in 0..n {
        let c = Pos2::new(rect.center().x - dots_w / 2.0 + 6.0 + k as f32 * 12.0, y);
        let dr = Rect::from_center_size(c, Vec2::splat(12.0));
        let d = ui.interact(dr, ui.scope_id().with(("gal-dot", k)), Sense::click());
        p.circle_filled(c, 3.5, if k == idx { TEXT } else { white(if d.hovered() { 110 } else { 60 }) });
        if d.clicked() {
            pick = Some(it.spec.sizes[k]);
        }
    }
    for (dir, glyph, x) in [(-1i32, Icon::Back, rect.center().x - dots_w / 2.0 - 22.0), (1, Icon::Forward, rect.center().x + dots_w / 2.0 + 22.0)] {
        let br = Rect::from_center_size(Pos2::new(x, y), Vec2::splat(22.0));
        let j = idx as i32 + dir;
        if (0..n as i32).contains(&j) && icon_button(ui, br, glyph, if dir < 0 { "Smaller" } else { "Bigger" }).clicked() {
            pick = Some(it.spec.sizes[j as usize]);
        }
    }
    p.text(Pos2::new(rect.left() + 18.0, y), Align2::LEFT_CENTER, size.name(), bold(12.0), TEXT);
    p.text(Pos2::new(rect.right() - 18.0, y), Align2::RIGHT_CENTER, size.label(), font(12.0), TEXT_FAINT);
    y += 16.0;
    let chips = Rect::from_min_size(Pos2::new(rect.left() + 18.0, y), Vec2::new(rect.width() - 36.0, 30.0));
    if let Some(z) = size_chips(ui, chips, ui.scope_id().with(("gal-sizes", &it.key)), it.spec.sizes, size) {
        pick = Some(z);
    }
    if let Some(z) = pick {
        if let Some(g) = &mut app.gallery {
            g.size = Some(z);
        }
    }
    // ←/→ page through the sizes
    let step = ctx.input(|i| (i.key_pressed(egui::Key::ArrowRight) as i32) - (i.key_pressed(egui::Key::ArrowLeft) as i32));
    if step != 0 {
        let j = (idx as i32 + step).clamp(0, n as i32 - 1) as usize;
        if let Some(g) = &mut app.gallery {
            g.size = Some(it.spec.sizes[j]);
        }
    }

    // add (or resize what's already there)
    y = chips.bottom() + 14.0;
    let label = match shown {
        Some(z) if z == size => "In the panel",
        Some(_) => "Use this size",
        None => "Add widget",
    };
    let br = Rect::from_min_size(Pos2::new(rect.left() + 18.0, y), Vec2::new(rect.width() - 36.0, 34.0));
    if pill_button(ui, br, label, BTN_PRIMARY, shown != Some(size)).clicked() {
        if shown.is_some() {
            set_size(&mut app.widgets, &it.key, size);
        } else {
            add_entry(&mut app.widgets, &it.key, size, None);
        }
        app.edit_sel = Some(it.key.clone());
        close(app, ui);
        return;
    }
    p.text(Pos2::new(rect.center().x, br.bottom() + 16.0), Align2::CENTER_CENTER, "or drag the preview into the panel", font(11.0), TEXT_FAINT);
}
