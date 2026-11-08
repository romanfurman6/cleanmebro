//! The menu-bar panel: the user's widgets in a grid (scrolling when they don't
//! fit), a footer with the tools' pages and a gear menu, the "Customize
//! widgets" editor, and the detail sheet that opens beside the panel.

use super::layout;
use super::settings::move_entry;
use super::shell::App;
use super::tool::{Cx, Recommendation};
use crate::kit::glass::*;
use crate::kit::mechanics as m;
use egui::{Align2, Color32, Pos2, Rect, ScrollArea, Sense, Stroke, Ui, UiBuilder, Vec2};

/// Space above the widgets.
pub const PAD: f32 = 18.0;
/// Space below the widgets: a gap plus the footer.
pub const FOOTER: f32 = 64.0;
const GAP: f32 = 12.0;
const RECOMMENDATION_HEIGHT: f32 = 168.0;
const CUSTOMIZE_HEAD: f32 = 44.0;
const CUSTOMIZE_ROW: f32 = 40.0;

fn inner_width() -> f32 {
    m::PANEL_WIDTH - 2.0 * PAD
}

/// The widgets to draw: (owner tool index or None for the shell, widget id, span, height).
fn visible(app: &App, width: f32) -> Vec<(Option<usize>, &'static str, u8, f32)> {
    app.widgets
        .iter()
        .filter(|e| e.on)
        .filter_map(|e| {
            let (owner, spec) = app.widget_spec(&e.key)?;
            let h = match owner {
                Some(i) => app.tools[i].widget_height(spec.id, width * spec.span as f32 / super::tool::GRID_COLUMNS as f32),
                None => RECOMMENDATION_HEIGHT,
            };
            Some((owner, spec.id, spec.span, h))
        })
        .collect()
}

/// Height the widget area needs.
pub fn content_height(app: &App) -> f32 {
    let items: Vec<(u8, f32)> = visible(app, inner_width()).iter().map(|v| (v.2, v.3)).collect();
    let widgets = layout::flow(&items, inner_width(), GAP).1.max(120.0);
    if app.customizing {
        // keep the panel's size while editing unless the list needs more
        return (CUSTOMIZE_HEAD + app.widgets.len() as f32 * CUSTOMIZE_ROW).max(widgets);
    }
    widgets
}

pub fn panel(app: &mut App, ui: &mut Ui, rect: Rect, cx: &mut Cx) {
    let area = Rect::from_min_max(Pos2::new(rect.left() + PAD, rect.top() + PAD), Pos2::new(rect.right() - PAD, rect.bottom() - FOOTER));
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
    footer(app, ui, rect, cx);
}

fn widgets(app: &mut App, ui: &mut Ui, origin: Rect, cx: &mut Cx) {
    let list = visible(app, origin.width());
    let items: Vec<(u8, f32)> = list.iter().map(|v| (v.2, v.3)).collect();
    let (rects, _) = layout::flow(&items, origin.width(), GAP);
    if list.is_empty() {
        let p = ui.painter();
        p.text(origin.center_top() + Vec2::new(0.0, 40.0), Align2::CENTER_CENTER, "No widgets shown", bold(16.0), TEXT_DIM);
        p.text(origin.center_top() + Vec2::new(0.0, 66.0), Align2::CENTER_CENTER, "Use the gear menu to pick some.", font(13.5), TEXT_FAINT);
        return;
    }
    for ((owner, id, _, _), r) in list.into_iter().zip(rects) {
        let r = r.translate(origin.min.to_vec2());
        match owner {
            Some(i) => {
                let mut child = ui.new_child(UiBuilder::new().max_rect(r).id_salt((app.tools[i].id(), id)));
                app.tools[i].widget_ui(id, &mut child, r, cx);
            }
            None => recommendation(app, ui, r, cx),
        }
    }
}

/// "Today's Recommendation": the highest-priority suggestion of any tool.
fn recommendation(app: &App, ui: &mut Ui, rect: Rect, cx: &mut Cx) {
    let p = ui.painter().clone();
    p.text(Pos2::new(rect.left() + 4.0, rect.top()), Align2::LEFT_TOP, "Today's Recommendation", bold(17.0), TEXT_DIM);
    let r = Rect::from_min_max(Pos2::new(rect.left(), rect.top() + 30.0), rect.max);
    glass_card(&p, r, m::CARD_RADIUS, 0.0, false);
    let Some(rec) = best_recommendation(app.tools.iter().flat_map(|t| t.recommendations())) else {
        tile_icon(&p, Rect::from_min_size(r.min + Vec2::new(16.0, 16.0), Vec2::splat(38.0)), Icon::Check, Color32::from_rgb(110, 225, 140), Color32::from_rgb(30, 165, 80));
        p.text(r.min + Vec2::new(68.0, 14.0), Align2::LEFT_TOP, "All good", bold(15.5), TEXT);
        paragraph(&p, r.min + Vec2::new(68.0, 36.0), "Nothing needs your attention right now.", font(13.5), TEXT_DIM, r.width() - 86.0);
        return;
    };
    tile_icon(&p, Rect::from_min_size(r.min + Vec2::new(16.0, 16.0), Vec2::splat(38.0)), rec.badge.icon, rec.badge.top, rec.badge.bottom);
    p.text(r.min + Vec2::new(68.0, 14.0), Align2::LEFT_TOP, &rec.title, bold(15.5), TEXT);
    paragraph(&p, r.min + Vec2::new(68.0, 36.0), &rec.body, font(13.5), TEXT_DIM, r.width() - 86.0);
    let bw = p.layout_no_wrap(rec.action.clone(), bold(14.0), TEXT).size().x + 36.0;
    let br = Rect::from_min_size(Pos2::new(r.right() - 16.0 - bw, r.bottom() - 44.0), Vec2::new(bw, 32.0));
    if pill_button(ui, br, &rec.action, 0, true).clicked() {
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
    p.text(origin.min + Vec2::new(4.0, 8.0), Align2::LEFT_TOP, "Widgets", bold(22.0), TEXT);
    let done = Rect::from_min_size(Pos2::new(origin.right() - 84.0, origin.top() + 4.0), Vec2::new(84.0, 30.0));
    if pill_button(ui, done, "Done", 1, true).clicked() {
        app.customizing = false;
    }
    let mut toggle = None;
    let mut mv = None;
    let n = app.widgets.len();
    for k in 0..n {
        let key = app.widgets[k].key.clone();
        let on = app.widgets[k].on;
        let row = Rect::from_min_size(Pos2::new(origin.left(), origin.top() + CUSTOMIZE_HEAD + k as f32 * CUSTOMIZE_ROW), Vec2::new(origin.width(), CUSTOMIZE_ROW - 4.0));
        p.rect_filled(row, 10.0, white(if on { 14 } else { 6 }));
        let cb = Rect::from_min_size(row.min + Vec2::new(10.0, (row.height() - 18.0) / 2.0), Vec2::splat(18.0));
        if checkbox(ui, cb, ui.scope_id().with(("w-on", &key)), on, true).clicked() {
            toggle = Some(k);
        }
        let (owner, spec) = match app.widget_spec(&key) {
            Some(s) => s,
            None => continue,
        };
        let tool_name = owner.map(|i| app.tools[i].name()).unwrap_or("CleanMeBro");
        text_fit(&p, row.left_center() + Vec2::new(38.0, 0.0), Align2::LEFT_CENTER, spec.title, bold(14.0), if on { TEXT } else { TEXT_DIM }, row.width() - 200.0);
        p.text(row.right_center() - Vec2::new(70.0, 0.0), Align2::RIGHT_CENTER, tool_name, font(12.0), TEXT_FAINT);
        for (j, (delta, enabled)) in [(-1isize, k > 0), (1, k + 1 < n)].into_iter().enumerate() {
            let br = Rect::from_center_size(Pos2::new(row.right() - 50.0 + j as f32 * 28.0, row.center().y), Vec2::splat(24.0));
            let resp = ui.interact(br, ui.scope_id().with(("w-mv", &key, j)), if enabled { Sense::click() } else { Sense::hover() });
            if resp.hovered() && enabled {
                p.rect_filled(br, 6.0, white(30));
            }
            // a small up or down triangle
            let (c, d) = (br.center(), delta as f32 * 4.0);
            let tri = vec![c + Vec2::new(-5.0, -d), c + Vec2::new(5.0, -d), c + Vec2::new(0.0, d)];
            p.add(egui::Shape::convex_polygon(tri, if enabled { TEXT_DIM } else { white(30) }, Stroke::NONE));
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

fn footer(app: &mut App, ui: &mut Ui, rect: Rect, cx: &mut Cx) {
    let ctx = ui.ctx().clone();
    let p = ui.painter().clone();
    let fy = rect.bottom() - 52.0;
    p.line_segment([Pos2::new(rect.left() + 14.0, fy), Pos2::new(rect.right() - 14.0, fy)], Stroke::new(1.0, white(26)));
    // one button per tool page
    let pages = app.page_tools();
    let mut x = rect.left() + 14.0;
    for &i in &pages {
        let b = Rect::from_min_size(Pos2::new(x, fy + 8.0), Vec2::splat(36.0));
        let badge = app.tools[i].badge();
        if square_button(ui, b, badge.icon, app.tools[i].name()).clicked() {
            app.open_window(&ctx, i);
        }
        x += 44.0;
    }
    let gear = Rect::from_min_size(Pos2::new(rect.right() - 50.0, fy + 8.0), Vec2::splat(36.0));
    // "Open CleanMeBro" centred in the space that's left
    let g = p.layout_no_wrap("Open CleanMeBro".into(), font(16.0), TEXT);
    let free = Rect::from_min_max(Pos2::new(x, fy), Pos2::new(gear.left() - 8.0, fy + 52.0));
    if !pages.is_empty() && free.width() >= g.size().x {
        let center_r = Rect::from_center_size(free.center(), g.size() + Vec2::new(12.0, 8.0));
        let cr = ui.interact(center_r, ui.scope_id().with("open-app"), Sense::click());
        p.galley(center_r.center() - g.size() / 2.0, g, if cr.hovered() { TEXT } else { TEXT_DIM });
        if cr.clicked() {
            let page = if app.tools.get(app.page).is_some_and(|t| t.has_page()) { app.page } else { pages[0] };
            app.open_window(&ctx, page);
        }
    }
    if square_button(ui, gear, Icon::Gear, "Settings").clicked() {
        app.gear_open = !app.gear_open;
    }
    if app.gear_open {
        gear_menu(app, ui, rect, fy, cx);
    }
}

/// The gear menu: customize, each tool's entries, quit.
fn gear_menu(app: &mut App, ui: &mut Ui, rect: Rect, fy: f32, cx: &mut Cx) {
    enum Row {
        Customize,
        Tool(usize, &'static str),
        Quit,
    }
    let mut rows: Vec<(Row, String)> = vec![(Row::Customize, "Customize widgets…".into())];
    for (i, t) in app.tools.iter().enumerate() {
        for item in t.menu_items() {
            rows.push((Row::Tool(i, item.id), item.label));
        }
    }
    rows.push((Row::Quit, "Quit CleanMeBro".into()));
    let h = rows.len() as f32 * 33.0 + 13.0;
    let menu = Rect::from_min_size(Pos2::new(rect.right() - 220.0, fy - h - 6.0), Vec2::new(206.0, h));
    let p = ui.painter().clone();
    glass_card(&p, menu, 12.0, 0.4, false);
    p.rect_filled(menu, 12.0, Color32::from_rgba_unmultiplied(28, 28, 32, 235));
    let mut picked = None;
    for (k, (_, label)) in rows.iter().enumerate() {
        let rr = Rect::from_min_size(menu.min + Vec2::new(6.0, 6.0 + k as f32 * 33.0), Vec2::new(menu.width() - 12.0, 32.0));
        let resp = ui.interact(rr, ui.scope_id().with(("gear", k)), Sense::click());
        if resp.hovered() {
            p.rect_filled(rr, 7.0, with_alpha(BLUE, 0.85));
        }
        p.text(rr.left_center() + Vec2::new(10.0, 0.0), Align2::LEFT_CENTER, label, font(14.0), TEXT);
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

fn square_button(ui: &mut Ui, r: Rect, ic: Icon, tip: &str) -> egui::Response {
    let id = ui.scope_id().with(("sq", tip));
    let resp = ui.interact(r, id, Sense::click());
    let h = ui.ctx().animate_bool(id, resp.hovered());
    let p = ui.painter();
    gradient_rect(p, r, 10.0, white((26.0 + 20.0 * h) as u8), white((10.0 + 14.0 * h) as u8));
    rim(p, r, 10.0, 0.8 + 0.2 * h);
    icon(p, ic, r.shrink(9.0), TEXT);
    resp.on_hover_text(tip)
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
            badge: Badge { icon: Icon::Check, top: TEXT, bottom: TEXT },
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
