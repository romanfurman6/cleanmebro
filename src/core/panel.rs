//! The menu-bar panel (design A, "Control Center"): a title line with the
//! window and settings buttons, the user's widgets in a grid (scrolling when
//! they don't fit; rows of one group share a card), an "Edit widgets…" link
//! that opens the editor in the window, and the detail sheet that opens beside
//! the panel. The editor draws its live preview with [`place`] and
//! [`draw_widget`].

use super::layout;
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
pub const PAD: f32 = 12.0;
pub const GAP: f32 = 10.0;
/// Padding above and below the rows inside a group card.
const GROUP_PAD: f32 = 4.0;
/// Planned tools shown as dashed tiles: (icon, label, tool name). A tile
/// disappears once a tool of that name is installed.
const PLANNED_TILES: [(Icon, &str, &str); 2] = [(Icon::Cup, "Awake", "Keep Awake"), (Icon::Fan, "Fans", "Fan Control")];

pub fn inner_width() -> f32 {
    m::PANEL_WIDTH - 2.0 * PAD
}

/// A shown widget and where it goes, relative to the grid's origin.
pub struct Placed {
    /// Index in `app.widgets`.
    pub entry: usize,
    /// Owner tool index; `None` = the shell.
    pub owner: Option<usize>,
    pub id: &'static str,
    pub rect: Rect,
}

/// Lay out the shown widgets for a grid `width` wide: the widgets, the cards
/// behind grouped rows, and the height used.
pub fn place(app: &App, width: f32) -> (Vec<Placed>, Vec<Rect>, f32) {
    struct Item {
        entry: usize,
        owner: Option<usize>,
        id: &'static str,
        span: u8,
        h: f32,
        group: Option<&'static str>,
    }
    let items: Vec<Item> = app
        .widgets
        .iter()
        .enumerate()
        .filter(|(_, e)| e.on)
        .filter_map(|(entry, e)| {
            let (owner, spec) = app.widget_spec(&e.key)?;
            if owner.is_some_and(|i| !app.tool_on(i)) {
                return None;
            }
            let h = match owner {
                Some(i) => app.tools[i].widget_height(spec.id, width * spec.span as f32 / super::tool::GRID_COLUMNS as f32),
                None if spec.id == PLANNED => QUICK_TILE_H,
                None => BANNER_H,
            };
            Some(Item { entry, owner, id: spec.id, span: spec.span, h, group: spec.group })
        })
        .collect();
    // neighbours of one group become one block: (item indexes, span, height, is_group)
    let mut blocks: Vec<(Vec<usize>, u8, f32, bool)> = Vec::new();
    for (k, it) in items.iter().enumerate() {
        if let (Some(g), Some(last)) = (it.group, blocks.last_mut()) {
            if last.3 && items[last.0[0]].group == Some(g) {
                last.0.push(k);
                last.2 += it.h;
                continue;
            }
        }
        match it.group {
            Some(_) => blocks.push((vec![k], super::tool::GRID_COLUMNS, it.h + 2.0 * GROUP_PAD, true)),
            None => blocks.push((vec![k], it.span, it.h, false)),
        }
    }
    let sizes: Vec<(u8, f32)> = blocks.iter().map(|b| (b.1, b.2)).collect();
    let (rects, total) = layout::flow(&sizes, width, GAP);
    let mut placed = Vec::with_capacity(items.len());
    let mut cards = Vec::new();
    for ((members, _, _, group), r) in blocks.into_iter().zip(rects) {
        if group {
            cards.push(r);
        }
        let mut y = if group { r.top() + GROUP_PAD } else { r.top() };
        for k in members {
            let it = &items[k];
            placed.push(Placed { entry: it.entry, owner: it.owner, id: it.id, rect: Rect::from_min_size(Pos2::new(r.left(), y), Vec2::new(r.width(), it.h)) });
            y += it.h;
        }
    }
    (placed, cards, total)
}

/// Draw one placed widget at `r`.
pub fn draw_widget(app: &mut App, ui: &mut Ui, w: &Placed, r: Rect, cx: &mut Cx) {
    match w.owner {
        Some(i) => {
            let mut child = ui.new_child(UiBuilder::new().max_rect(r).id_salt((app.tools[i].id(), w.id)));
            app.tools[i].widget_ui(w.id, &mut child, r, cx);
        }
        None if w.id == PLANNED => planned(app, ui, r),
        None if w.id == RECOMMENDATION => recommendation(app, ui, r, cx),
        None => {}
    }
}

/// Height the widget area needs.
pub fn content_height(app: &App) -> f32 {
    place(app, inner_width()).2.max(120.0)
}

pub fn panel(app: &mut App, ui: &mut Ui, rect: Rect, cx: &mut Cx) {
    header(app, ui, rect);
    let area = Rect::from_min_max(Pos2::new(rect.left() + PAD, rect.top() + HEADER), Pos2::new(rect.right() - PAD, rect.bottom() - FOOTER));
    let need = content_height(app);
    if need > area.height() + 0.5 {
        let mut child = ui.new_child(UiBuilder::new().max_rect(area).layout(egui::Layout::top_down(egui::Align::Min)));
        child.set_clip_rect(area.expand2(Vec2::new(PAD, 0.0)));
        ScrollArea::vertical().id_salt("panel-widgets").auto_shrink([false, false]).show(&mut child, |ui| {
            let (origin, _) = ui.allocate_exact_size(Vec2::new(area.width(), need), Sense::hover());
            widgets(app, ui, origin, cx);
        });
    } else {
        widgets(app, ui, Rect::from_min_size(area.min, Vec2::new(area.width(), need)), cx);
    }
    footer(app, ui, rect);
    if app.gear_open {
        gear_menu(app, ui, rect, cx);
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
    let (placed, cards, _) = place(app, origin.width());
    if placed.is_empty() {
        let p = ui.painter();
        p.text(origin.center_top() + Vec2::new(0.0, 40.0), Align2::CENTER_CENTER, "No widgets shown", bold(15.0), TEXT_DIM);
        p.text(origin.center_top() + Vec2::new(0.0, 64.0), Align2::CENTER_CENTER, "Use Edit widgets… to pick some.", font(13.0), TEXT_FAINT);
        return;
    }
    for c in cards {
        glass_card(ui.painter(), c.translate(origin.min.to_vec2()), R_CARD, 0.0, false);
    }
    for w in &placed {
        draw_widget(app, ui, w, w.rect.translate(origin.min.to_vec2()), cx);
    }
}

/// Planned tiles whose tool isn't installed yet.
fn planned_left(app: &App) -> Vec<(Icon, &'static str)> {
    PLANNED_TILES.iter().filter(|(_, _, tool)| !app.tools.iter().any(|t| t.name() == *tool)).map(|(g, l, _)| (*g, *l)).collect()
}

/// How many planned tiles are left (the planned widget's span; 0 hides it).
pub fn planned_count(app: &App) -> usize {
    planned_left(app).len()
}

/// Dashed tiles for tools that are planned but not built yet.
fn planned(app: &App, ui: &mut Ui, rect: Rect) {
    let left = planned_left(app);
    let n = left.len().max(1) as f32;
    let w = (rect.width() - GAP * (n - 1.0)) / n;
    for (k, (glyph, label)) in left.iter().enumerate() {
        let r = Rect::from_min_size(Pos2::new(rect.left() + k as f32 * (w + GAP), rect.top()), Vec2::new(w, rect.height()));
        quick_tile(ui, r, ui.scope_id().with(("planned", k)), *glyph, tint::NEUTRAL, label, "planned", false);
    }
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
        app.open_settings(&ctx, false);
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
                app.open_settings(&ctx, false);
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
