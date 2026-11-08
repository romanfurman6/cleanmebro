//! The menu-bar panel (design A, "Control Center"): a title line with the
//! window and settings buttons, the user's widgets in a grid (scrolling when
//! they don't fit; rows of one group share a card), an "Edit widgets…" link,
//! the editor behind it, and the detail sheet that opens beside the panel.

use super::layout;
use super::settings::move_entry;
use super::shell::{App, PLANNED, RECOMMENDATION};
use super::tool::{Cx, Recommendation};
use crate::kit::glass::*;
use crate::kit::mechanics as m;
use egui::{Align2, Pos2, Rect, ScrollArea, Sense, Stroke, Ui, UiBuilder, Vec2};

/// Height of the title line above the widgets.
pub const HEADER: f32 = 60.0;
/// Space below the widgets: a gap plus the "Edit widgets…" link.
pub const FOOTER: f32 = 40.0;
/// Side padding of the widget grid.
const PAD: f32 = 12.0;
const GAP: f32 = 10.0;
/// Padding above and below the rows inside a group card.
const GROUP_PAD: f32 = 4.0;
const CUSTOMIZE_HEAD: f32 = 44.0;
const CUSTOMIZE_ROW: f32 = 40.0;
/// Planned tools shown as dashed tiles: (icon, label).
const PLANNED_TILES: [(Icon, &str); 3] = [(Icon::Cup, "Awake"), (Icon::Crop, "Capture"), (Icon::Fan, "Fans")];

fn inner_width() -> f32 {
    m::PANEL_WIDTH - 2.0 * PAD
}

/// A visible widget: owner tool index (None = the shell), id, span, height, group.
struct Item {
    owner: Option<usize>,
    id: &'static str,
    span: u8,
    h: f32,
    group: Option<&'static str>,
}

fn visible(app: &App, width: f32) -> Vec<Item> {
    app.widgets
        .iter()
        .filter(|e| e.on)
        .filter_map(|e| {
            let (owner, spec) = app.widget_spec(&e.key)?;
            let h = match owner {
                Some(i) => app.tools[i].widget_height(spec.id, width * spec.span as f32 / super::tool::GRID_COLUMNS as f32),
                None if spec.id == PLANNED => QUICK_TILE_H,
                None => BANNER_H,
            };
            Some(Item { owner, id: spec.id, span: spec.span, h, group: spec.group })
        })
        .collect()
}

/// Items in layout order, with neighbours of one group merged into a block.
/// Returns (item indexes, span, height, is_group) per block.
fn blocks(items: &[Item]) -> Vec<(Vec<usize>, u8, f32, bool)> {
    let mut out: Vec<(Vec<usize>, u8, f32, bool)> = Vec::new();
    for (k, it) in items.iter().enumerate() {
        if let (Some(g), Some(last)) = (it.group, out.last_mut()) {
            if last.3 && items[last.0[0]].group == Some(g) {
                last.0.push(k);
                last.2 += it.h;
                continue;
            }
        }
        match it.group {
            Some(_) => out.push((vec![k], super::tool::GRID_COLUMNS, it.h + 2.0 * GROUP_PAD, true)),
            None => out.push((vec![k], it.span, it.h, false)),
        }
    }
    out
}

/// Height the widget area needs.
pub fn content_height(app: &App) -> f32 {
    let items = visible(app, inner_width());
    let sizes: Vec<(u8, f32)> = blocks(&items).iter().map(|b| (b.1, b.2)).collect();
    let widgets = layout::flow(&sizes, inner_width(), GAP).1.max(120.0);
    if app.customizing {
        // keep the panel's size while editing unless the list needs more
        return (CUSTOMIZE_HEAD + app.widgets.len() as f32 * CUSTOMIZE_ROW + 8.0).max(widgets);
    }
    widgets
}

pub fn panel(app: &mut App, ui: &mut Ui, rect: Rect, cx: &mut Cx) {
    header(app, ui, rect);
    let area = Rect::from_min_max(Pos2::new(rect.left() + PAD, rect.top() + HEADER), Pos2::new(rect.right() - PAD, rect.bottom() - FOOTER));
    let need = content_height(app);
    let scroll = need > area.height() + 0.5;
    let mut body = |app: &mut App, ui: &mut Ui, origin: Rect| {
        if app.customizing {
            customize(app, ui, origin);
        } else {
            widgets(app, ui, origin, cx);
        }
    };
    if scroll {
        let mut child = ui.new_child(UiBuilder::new().max_rect(area).layout(egui::Layout::top_down(egui::Align::Min)));
        child.set_clip_rect(area.expand2(Vec2::new(PAD, 0.0)));
        ScrollArea::vertical().id_salt("panel-widgets").auto_shrink([false, false]).show(&mut child, |ui| {
            let (origin, _) = ui.allocate_exact_size(Vec2::new(area.width(), need), Sense::hover());
            body(app, ui, origin);
        });
    } else {
        body(app, ui, Rect::from_min_size(area.min, Vec2::new(area.width(), need)));
    }
    footer(app, ui, rect);
    if app.gear_open {
        gear_menu(app, ui, rect, cx);
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
    let tw = win.left() - x0 - 10.0;
    match app.tools.iter().find_map(|t| t.panel_header()) {
        Some(h) => {
            text_fit(&p, Pos2::new(x0, rect.top() + 15.0), Align2::LEFT_TOP, &h.title, bold(15.0), TEXT, tw);
            p.circle_filled(Pos2::new(x0 + 3.5, rect.top() + 42.0), 3.5, h.dot);
            text_fit(&p, Pos2::new(x0 + 13.0, rect.top() + 42.0), Align2::LEFT_CENTER, &h.status, font(12.0), TEXT_DIM, tw - 13.0);
        }
        None => {
            text_fit(&p, Pos2::new(x0, rect.top() + 31.0), Align2::LEFT_CENTER, "CleanMeBro", bold(15.0), TEXT, tw);
        }
    }
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
    let items = visible(app, origin.width());
    if items.is_empty() {
        let p = ui.painter();
        p.text(origin.center_top() + Vec2::new(0.0, 40.0), Align2::CENTER_CENTER, "No widgets shown", bold(15.0), TEXT_DIM);
        p.text(origin.center_top() + Vec2::new(0.0, 64.0), Align2::CENTER_CENTER, "Use Edit widgets… to pick some.", font(13.0), TEXT_FAINT);
        return;
    }
    let bl = blocks(&items);
    let sizes: Vec<(u8, f32)> = bl.iter().map(|b| (b.1, b.2)).collect();
    let (rects, _) = layout::flow(&sizes, origin.width(), GAP);
    for ((members, _, _, group), r) in bl.into_iter().zip(rects) {
        let r = r.translate(origin.min.to_vec2());
        if group {
            glass_card(ui.painter(), r, R_CARD, 0.0, false);
        }
        let mut y = if group { r.top() + GROUP_PAD } else { r.top() };
        for k in members {
            let it = &items[k];
            let wr = Rect::from_min_size(Pos2::new(r.left(), y), Vec2::new(r.width(), it.h));
            y += it.h;
            match it.owner {
                Some(i) => {
                    let mut child = ui.new_child(UiBuilder::new().max_rect(wr).id_salt((app.tools[i].id(), it.id)));
                    app.tools[i].widget_ui(it.id, &mut child, wr, cx);
                }
                None if it.id == PLANNED => planned(ui, wr),
                None if it.id == RECOMMENDATION => recommendation(app, ui, wr, cx),
                None => {}
            }
        }
    }
}

/// Dashed tiles for tools that are planned but not built yet.
fn planned(ui: &mut Ui, rect: Rect) {
    let n = PLANNED_TILES.len() as f32;
    let w = (rect.width() - GAP * (n - 1.0)) / n;
    for (k, (glyph, label)) in PLANNED_TILES.iter().enumerate() {
        let r = Rect::from_min_size(Pos2::new(rect.left() + k as f32 * (w + GAP), rect.top()), Vec2::new(w, rect.height()));
        quick_tile(ui, r, ui.scope_id().with(("planned", k)), *glyph, tint::NEUTRAL, label, "planned", false);
    }
}

/// The banner with the highest-priority suggestion of any tool.
fn recommendation(app: &App, ui: &mut Ui, rect: Rect, cx: &mut Cx) {
    let Some(rec) = best_recommendation(app.tools.iter().flat_map(|t| t.recommendations())) else {
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

/// Turn widgets on and off and change their order.
fn customize(app: &mut App, ui: &mut Ui, origin: Rect) {
    let p = ui.painter().clone();
    p.text(origin.min + Vec2::new(4.0, 6.0), Align2::LEFT_TOP, "Widgets", bold(15.0), TEXT);
    p.text(origin.min + Vec2::new(4.0, 25.0), Align2::LEFT_TOP, "Show, hide and reorder", font(12.0), TEXT_DIM);
    let done = Rect::from_min_size(Pos2::new(origin.right() - 72.0, origin.top() + 4.0), Vec2::new(72.0, 30.0));
    if pill_button(ui, done, "Done", BTN_PRIMARY, true).clicked() {
        app.customizing = false;
    }
    let n = app.widgets.len();
    let list = Rect::from_min_size(Pos2::new(origin.left(), origin.top() + CUSTOMIZE_HEAD), Vec2::new(origin.width(), n as f32 * CUSTOMIZE_ROW + 8.0));
    glass_card(&p, list, R_CARD, 0.0, false);
    let mut toggle = None;
    let mut mv = None;
    for k in 0..n {
        let key = app.widgets[k].key.clone();
        let on = app.widgets[k].on;
        let row = Rect::from_min_size(Pos2::new(list.left(), list.top() + 4.0 + k as f32 * CUSTOMIZE_ROW), Vec2::new(list.width(), CUSTOMIZE_ROW));
        if k > 0 {
            hairline(&p, Pos2::new(row.left() + 12.0, row.top()), Pos2::new(row.right() - 12.0, row.top()));
        }
        let (owner, spec) = match app.widget_spec(&key) {
            Some(s) => s,
            None => continue,
        };
        let (glyph, tint_c) = match owner {
            Some(i) => (app.tools[i].badge().icon, app.tools[i].badge().color),
            None if spec.id == PLANNED => (Icon::Grip, tint::NEUTRAL),
            None => (Icon::Sparkle, BLUE),
        };
        tool_chip(&p, Rect::from_center_size(Pos2::new(row.left() + 24.0, row.center().y), Vec2::splat(22.0)), glyph, if on { tint_c } else { tint::NEUTRAL });
        let tool_name = owner.map(|i| app.tools[i].name()).unwrap_or("CleanMeBro");
        text_fit(&p, Pos2::new(row.left() + 44.0, row.top() + 7.0), Align2::LEFT_TOP, spec.title, font(13.0), if on { TEXT } else { TEXT_DIM }, row.width() - 160.0);
        p.text(Pos2::new(row.left() + 44.0, row.top() + 23.0), Align2::LEFT_TOP, tool_name, font(11.0), TEXT_FAINT);
        let sw = Rect::from_min_size(Pos2::new(row.right() - 12.0 - 32.0, row.center().y - 10.0), Vec2::new(32.0, 20.0));
        if switch(ui, sw, ui.scope_id().with(("w-on", &key)), on, true).clicked() {
            toggle = Some(k);
        }
        for (j, (delta, enabled)) in [(-1isize, k > 0), (1, k + 1 < n)].into_iter().enumerate() {
            let br = Rect::from_center_size(Pos2::new(sw.left() - 52.0 + j as f32 * 24.0, row.center().y), Vec2::splat(22.0));
            let resp = ui.interact(br, ui.scope_id().with(("w-mv", &key, j)), if enabled { Sense::click() } else { Sense::hover() });
            if resp.hovered() && enabled {
                p.rect_filled(br, R_SMALL, white(24));
            }
            // a small up or down chevron
            let (c, d) = (br.center(), delta as f32 * 2.5);
            let col = if enabled { TEXT_DIM } else { white(30) };
            p.line_segment([c + Vec2::new(-4.5, -d), c + Vec2::new(0.0, d)], Stroke::new(1.6, col));
            p.line_segment([c + Vec2::new(4.5, -d), c + Vec2::new(0.0, d)], Stroke::new(1.6, col));
            if resp.clicked() && enabled {
                mv = Some((k, delta));
            }
        }
    }
    if let Some(k) = toggle {
        app.widgets[k].on = !app.widgets[k].on;
    }
    if let Some((k, d)) = mv {
        move_entry(&mut app.widgets, k, d);
    }
}

/// "Edit widgets…", centred under the widgets.
fn footer(app: &mut App, ui: &mut Ui, rect: Rect) {
    if app.customizing {
        return;
    }
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
        app.customizing = true;
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
    for (i, t) in app.tools.iter().enumerate() {
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
            Row::Customize => app.customizing = true,
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
