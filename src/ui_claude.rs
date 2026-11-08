//! The Claude page in the full window: limits, local activity and the Claude
//! Code projects on this Mac (which ones count as "in use" and are protected).

use crate::app::{App, Page};
use crate::claude;
use crate::glass::*;
use egui::{Align2, Color32, Pos2, Rect, ScrollArea, Sense, Ui, UiBuilder, Vec2};

pub fn page(app: &mut App, ui: &mut Ui, r: Rect) {
    let left = Rect::from_min_size(r.min, Vec2::new(430.0, r.height()));
    crate::ui_panel::claude_detail(app, ui, left);
    let cl = app.claude.lock().unwrap().clone();
    let p = ui.painter().clone();
    let rr = Rect::from_min_max(Pos2::new(left.right() + 18.0, r.top()), r.max);
    glass_card(&p, rr, 20.0, 0.0, false);
    p.text(rr.min + Vec2::new(20.0, 16.0), Align2::LEFT_TOP, "Claude Code projects", bold(20.0), TEXT);
    let used = cl.projects.iter().filter(|p| p.in_use()).count();
    let sub = format!(
        "{} projects · {} in use (kept by Cleanup) · all Claude data {}",
        cl.projects.len(),
        used,
        human_bytes(cl.data_bytes)
    );
    p.text(rr.min + Vec2::new(20.0, 46.0), Align2::LEFT_TOP, sub, font(13.5), TEXT_DIM);
    let list = Rect::from_min_max(rr.min + Vec2::new(10.0, 78.0), rr.max - Vec2::new(10.0, 64.0));
    let mut child = ui.new_child(UiBuilder::new().max_rect(list).layout(egui::Layout::top_down(egui::Align::Min)));
    child.set_clip_rect(list);
    ScrollArea::vertical().id_salt("claude-projects").auto_shrink([false, false]).show(&mut child, |ui| {
        if cl.projects.is_empty() {
            ui.label(egui::RichText::new(if app.demo { "Projects appear here." } else { "Looking for Claude Code projects…" }).color(TEXT_DIM));
        }
        for pr in &cl.projects {
            let (row, _) = ui.allocate_exact_size(Vec2::new(list.width() - 6.0, 52.0), Sense::hover());
            let pp = ui.painter();
            let (c, label) = if pr.in_use() {
                (GREEN, "In use")
            } else if !pr.cwd_exists {
                (AMBER, "Folder gone")
            } else {
                (Color32::from_rgb(170, 170, 210), "Idle")
            };
            pp.circle_filled(row.min + Vec2::new(14.0, 26.0), 5.0, c);
            text_fit(pp, row.min + Vec2::new(30.0, 8.0), Align2::LEFT_TOP, &pr.display_name(), bold(15.0), TEXT, row.width() - 260.0);
            let home = dirs::home_dir().unwrap_or_default().display().to_string();
            let path = pr.cwd.as_ref().map(|c| c.display().to_string().replacen(&home, "~", 1)).unwrap_or_else(|| "unknown folder".into());
            text_fit(pp, row.min + Vec2::new(30.0, 29.0), Align2::LEFT_TOP, &middle_ellipsis(&path, 60), font(12.0), TEXT_FAINT, row.width() - 260.0);
            let idle = pr.days_idle();
            let when = if idle < 1.0 { "today".to_string() } else { format!("{} days ago", idle as u64) };
            pp.text(Pos2::new(row.right() - 170.0, row.center().y), Align2::LEFT_CENTER, label, font(13.0), c);
            pp.text(Pos2::new(row.right() - 90.0, row.top() + 16.0), Align2::LEFT_CENTER, human_bytes(pr.bytes), bold(13.5), TEXT);
            pp.text(Pos2::new(row.right() - 90.0, row.top() + 36.0), Align2::LEFT_CENTER, when, font(12.0), TEXT_FAINT);
        }
    });
    let br = Rect::from_min_size(Pos2::new(rr.right() - 230.0, rr.bottom() - 50.0), Vec2::new(210.0, 34.0));
    if pill_button(ui, br, "Clean old Claude data", 1, true).clicked() {
        app.clean_group = Some(crate::cleanup::Group::Claude);
        app.page = Page::Cleanup;
    }
    let rf = Rect::from_min_size(Pos2::new(rr.left() + 20.0, rr.bottom() - 50.0), Vec2::new(150.0, 34.0));
    if pill_button(ui, rf, "Refresh limits", 0, true).clicked() {
        claude::request_refresh();
        app.toast = Some(("Asking Claude for your limits…".into(), std::time::Instant::now()));
    }
}
