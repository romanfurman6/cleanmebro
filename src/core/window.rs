//! The full window: traffic lights, a sidebar with one entry per tool page,
//! and the selected tool's page.

use super::shell::App;
use super::tool::Cx;
use crate::kit::glass::{self, *};
use egui::{Align2, Color32, Pos2, Rect, Sense, Ui, UiBuilder, Vec2, ViewportCommand};

/// Tools that are planned but not built yet: shown dashed and disabled so
/// it's clear where they will live. An installed tool with the same name hides
/// its placeholder.
const PLANNED: [(Icon, &str); 4] = [(Icon::Crop, "Capture"), (Icon::Clipboard, "Clipboard"), (Icon::Fan, "Fan Control"), (Icon::Cup, "Keep Awake")];

/// `draw_lights`: paint traffic lights ourselves (screenshots; the real
/// window has native ones).
pub fn window_ui(app: &mut App, ui: &mut Ui, full: Rect, draw_lights: bool, cx: &mut Cx) {
    let ctx = ui.ctx().clone();
    let side = Rect::from_min_size(full.min, Vec2::new(SIDEBAR_W, full.height()));
    // drag the window from the top strip of the page
    let drag = Rect::from_min_size(full.min + Vec2::new(SIDEBAR_W, 0.0), Vec2::new(full.width() - SIDEBAR_W, 16.0));
    let dr = ui.interact(drag, ui.scope_id().with("drag"), Sense::click_and_drag());
    if dr.drag_started() {
        ctx.send_viewport_cmd(ViewportCommand::StartDrag);
    }
    let p = ui.painter().clone();
    p.rect_filled(side, egui::CornerRadius { nw: 12, sw: 12, ne: 0, se: 0 }, glass::SIDEBAR());
    hairline(&p, side.right_top(), side.right_bottom());
    if draw_lights {
        traffic_lights(ui, &p, full);
    }
    // one entry per tool page, grouped by section
    let mut pages = app.page_tools();
    // group by section in the sidebar's order (tools keep their order inside)
    pages.sort_by_key(|&i| section_rank(app.tools[i].section()));
    if !pages.contains(&app.page) {
        if let Some(&first) = pages.first() {
            app.page = first;
        }
    }
    let x = side.left() + 8.0;
    let w = side.width() - 16.0;
    // app mark under the traffic lights
    let logo_y = full.top() + 44.0;
    if let Some(tex) = crate::kit::brand::app_icon(&ctx) {
        let r = Rect::from_min_size(Pos2::new(side.left() + 16.0, logo_y), Vec2::splat(22.0));
        p.image(tex.id(), r, Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)), Color32::WHITE);
    }
    p.text(Pos2::new(side.left() + 46.0, logo_y + 11.0), Align2::LEFT_CENTER, "Cmd+Meow", bold(13.0), TEXT());
    let mut y = logo_y + 22.0 + 4.0;
    let mut section = "";
    for &i in &pages {
        if app.tools[i].section() != section {
            section = app.tools[i].section();
            y += 12.0;
            group_header(&p, Pos2::new(x + 8.0, y), section);
            y += 18.0;
        }
        let t = &app.tools[i];
        let (label, badge, meta, id) = (t.name(), t.badge(), t.sidebar_meta(), t.id());
        let r = Rect::from_min_size(Pos2::new(x, y), Vec2::new(w, SIDEBAR_ROW_H));
        if sidebar_item(ui, r, ui.scope_id().with(("nav", id)), badge.icon, badge.color, label, &meta, app.page == i && !app.settings_open, false).clicked() {
            app.page = i;
            app.settings_open = false;
            app.drag = None;
        }
        y += SIDEBAR_ROW_H + 2.0;
    }
    let planned: Vec<_> = PLANNED.iter().filter(|(_, n)| !app.tools.iter().any(|t| t.name() == *n)).collect();
    if !planned.is_empty() {
        y += 12.0;
        section_header(&p, Pos2::new(x + 8.0, y), Align2::LEFT_TOP, "Planned tools");
        y += 18.0;
        for (ic, label) in planned {
            let r = Rect::from_min_size(Pos2::new(x, y), Vec2::new(w, SIDEBAR_ROW_H));
            sidebar_item(ui, r, ui.scope_id().with(("planned", *label)), *ic, Color32::TRANSPARENT, label, "", false, true);
            y += SIDEBAR_ROW_H + 2.0;
        }
    }
    // settings: widgets and tools
    let r = Rect::from_min_size(Pos2::new(x, full.bottom() - 12.0 - 30.0 - 6.0 - SIDEBAR_ROW_H), Vec2::new(w, SIDEBAR_ROW_H));
    if sidebar_item(ui, r, ui.scope_id().with("nav-settings"), Icon::Gear, tint::NEUTRAL(), "Settings", "", app.settings_open, false).clicked() {
        app.settings_open = true;
    }
    // back to the menu bar panel
    let r = Rect::from_min_size(Pos2::new(x, full.bottom() - 12.0 - 30.0), Vec2::new(w, 30.0));
    let resp = ui.interact(r, ui.scope_id().with("to-panel"), Sense::click());
    let hov = ctx.animate_bool(ui.scope_id().with("to-panel-h"), resp.hovered());
    if hov > 0.0 {
        p.rect_filled(r, R_SMALL, white((12.0 * hov) as u8));
    }
    icon(&p, Icon::Monitor, Rect::from_min_size(Pos2::new(r.left() + 10.0, r.center().y - 8.0), Vec2::splat(16.0)), TEXT_DIM());
    p.text(Pos2::new(r.left() + 34.0, r.center().y), Align2::LEFT_CENTER, "Menu bar panel", font(13.0), TEXT_DIM());
    if resp.on_hover_text("Show the menu bar panel").clicked() {
        app.open_panel(&ctx);
    }
    let content = Rect::from_min_max(Pos2::new(side.right() + 1.0, full.top()), full.max);
    if app.settings_open {
        let mut child = ui.new_child(UiBuilder::new().max_rect(content).id_salt("settings"));
        super::editor::settings_page(app, &mut child, content);
        return;
    }
    let Some(tool) = app.tools.get_mut(app.page).filter(|t| t.has_page()) else {
        p.text(content.center(), Align2::CENTER_CENTER, "No tools with a page are installed", font(15.0), TEXT_DIM());
        return;
    };
    let mut child = ui.new_child(UiBuilder::new().max_rect(content).id_salt(("page", tool.id())));
    tool.page_ui(&mut child, content, cx);
}

/// Sidebar order of the tool sections; unknown sections go last.
fn section_rank(section: &str) -> usize {
    ["Clean", "Monitor", "Create"].iter().position(|s| *s == section).unwrap_or(3)
}

fn traffic_lights(ui: &mut Ui, p: &egui::Painter, full: Rect) {
    let ctx = ui.ctx().clone();
    let lights = [(Color32::from_rgb(255, 95, 87), 0), (Color32::from_rgb(254, 188, 46), 1), (Color32::from_rgb(40, 200, 64), 2)];
    let any_hover = ui.rect_contains_pointer(Rect::from_min_size(full.min + Vec2::new(10.0, 8.0), Vec2::new(70.0, 24.0)));
    for (c, k) in lights {
        let center = full.min + Vec2::new(22.0 + k as f32 * 20.0, 20.0);
        let r = Rect::from_center_size(center, Vec2::splat(13.0));
        let resp = ui.interact(r, ui.scope_id().with(("light", k)), Sense::click());
        p.circle_filled(center, 6.2, c);
        crate::kit::glass::stroke_circle(&p, center, 6.2, egui::Stroke::new(0.6, Color32::from_black_alpha(60)));
        if any_hover {
            let s = egui::Stroke::new(1.2, Color32::from_black_alpha(150));
            match k {
                0 => {
                    p.line_segment([center + Vec2::new(-2.5, -2.5), center + Vec2::new(2.5, 2.5)], s);
                    p.line_segment([center + Vec2::new(-2.5, 2.5), center + Vec2::new(2.5, -2.5)], s);
                }
                1 => {
                    p.line_segment([center + Vec2::new(-3.0, 0.0), center + Vec2::new(3.0, 0.0)], s);
                }
                _ => {
                    p.line_segment([center + Vec2::new(-2.5, 2.5), center + Vec2::new(2.5, -2.5)], s);
                }
            }
        }
        if resp.clicked() {
            match k {
                0 => ctx.send_viewport_cmd(ViewportCommand::Close),
                1 => ctx.send_viewport_cmd(ViewportCommand::Minimized(true)),
                _ => {}
            }
        }
    }
}
