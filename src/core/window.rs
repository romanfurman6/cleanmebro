//! The full window: traffic lights, a sidebar with one entry per tool page,
//! and the selected tool's page.

use super::shell::App;
use super::tool::Cx;
use crate::kit::glass::{self, *};
use egui::{Align2, Color32, Pos2, Rect, Sense, Ui, UiBuilder, Vec2, ViewportCommand};

const SIDEBAR: f32 = 88.0;

/// `draw_lights`: paint traffic lights ourselves (screenshots; the real
/// window has native ones).
pub fn window_ui(app: &mut App, ui: &mut Ui, full: Rect, draw_lights: bool, cx: &mut Cx) {
    let ctx = ui.ctx().clone();
    let side = Rect::from_min_size(full.min, Vec2::new(SIDEBAR, full.height()));
    // drag the window from the top strip
    let drag = Rect::from_min_size(full.min + Vec2::new(SIDEBAR, 0.0), Vec2::new(full.width() - SIDEBAR, 44.0));
    let dr = ui.interact(drag, ui.scope_id().with("drag"), Sense::click_and_drag());
    if dr.drag_started() {
        ctx.send_viewport_cmd(ViewportCommand::StartDrag);
    }
    // sidebar glass strip
    let p = ui.painter().clone();
    gradient_rect_dir(&p, side, 14.0, white(10), white(4), Vec2::new(1.0, 0.0));
    p.line_segment([side.right_top() + Vec2::new(0.0, 12.0), side.right_bottom() - Vec2::new(0.0, 12.0)], egui::Stroke::new(1.0, white(22)));
    if draw_lights {
        traffic_lights(ui, &p, full);
    }
    // one entry per tool page
    let pages = app.page_tools();
    if !pages.contains(&app.page) {
        if let Some(&first) = pages.first() {
            app.page = first;
        }
    }
    let mut y = full.top() + 52.0;
    let mut section = "";
    for &i in &pages {
        let (label, badge) = (app.tools[i].name(), app.tools[i].badge());
        // a small header whenever the group changes
        if app.tools[i].section() != section {
            section = app.tools[i].section();
            p.text(Pos2::new(side.center().x, y + 8.0), Align2::CENTER_CENTER, section.to_uppercase(), bold(9.0), TEXT_FAINT);
            y += 18.0;
        }
        let r = Rect::from_center_size(Pos2::new(side.center().x, y + 32.0), Vec2::new(70.0, 70.0));
        let id = ui.scope_id().with(("nav", app.tools[i].id()));
        let resp = ui.interact(r, id, Sense::click());
        let hov = ctx.animate_bool(id, resp.hovered());
        let selected = app.page == i;
        if selected {
            glass_card(&p, r, 18.0, 0.6, false);
        } else if hov > 0.0 {
            p.rect_filled(r, 18.0, white((14.0 * hov) as u8));
        }
        let ir = Rect::from_center_size(r.center() - Vec2::new(0.0, 8.0), Vec2::splat(34.0));
        glass::tile_icon(&p, ir, badge.icon, badge.top, badge.bottom);
        p.text(Pos2::new(r.center().x, r.bottom() - 9.0), Align2::CENTER_CENTER, label, font(10.5), if selected { TEXT } else { TEXT_DIM });
        if resp.clicked() {
            app.page = i;
        }
        y += 84.0;
    }
    // back to the menu panel
    let r = Rect::from_center_size(Pos2::new(side.center().x, full.bottom() - 46.0), Vec2::splat(52.0));
    let resp = ui.interact(r, ui.scope_id().with("to-panel"), Sense::click());
    let hov = ctx.animate_bool(ui.scope_id().with("to-panel-h"), resp.hovered());
    p.rect_filled(r, 14.0, white((10.0 + 14.0 * hov) as u8));
    icon(&p, Icon::Monitor, r.shrink(14.0), TEXT_DIM);
    if resp.on_hover_text("Show the menu bar panel").clicked() {
        app.open_panel(&ctx);
    }
    let Some(tool) = app.tools.get_mut(app.page).filter(|t| t.has_page()) else {
        p.text(full.center(), Align2::CENTER_CENTER, "No tools with a page are installed", font(15.0), TEXT_DIM);
        return;
    };
    p.text(Pos2::new(full.center().x + SIDEBAR / 2.0, full.top() + 20.0), Align2::CENTER_CENTER, tool.name(), bold(14.0), TEXT_DIM);
    let content = Rect::from_min_max(Pos2::new(side.right() + 20.0, full.top() + 44.0), full.max - Vec2::new(20.0, 18.0));
    let mut child = ui.new_child(UiBuilder::new().max_rect(content).id_salt(("page", tool.id())));
    tool.page_ui(&mut child, content, cx);
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
        p.circle_stroke(center, 6.2, egui::Stroke::new(0.6, Color32::from_black_alpha(60)));
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
