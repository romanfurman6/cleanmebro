//! The Claude page in the full window: plan limits, local token use and the
//! Claude Code projects on this Mac (which ones count as "in use" and are
//! protected by Cleanup).

use super::backend as claude;
use super::ui::CLEANUP;
use crate::core::tool::Cx;
use crate::kit::claude_code::{self as cc, Availability};
use crate::kit::glass::*;
use egui::{Align2, Color32, Pos2, Rect, ScrollArea, Sense, Ui, UiBuilder, Vec2};

/// Bar colour for a usage percentage: Claude's tint, amber from 70%, red from 90%.
pub fn usage_color(pct: f32) -> Color32 {
    if pct >= 90.0 {
        RED
    } else if pct >= 70.0 {
        AMBER
    } else {
        CLAUDE
    }
}

/// Seven daily token bars; today (the last) is solid, the rest faded.
pub fn day_bars(p: &egui::Painter, r: Rect, daily: &[u64], gap: f32) {
    let maxd = daily.iter().copied().max().unwrap_or(1).max(1) as f32;
    let n = daily.len().max(1) as f32;
    let bw = (r.width() - gap * (n - 1.0)) / n;
    for (i, v) in daily.iter().enumerate() {
        let h = (r.height() * (*v as f32 / maxd)).max(2.0);
        let x = r.left() + i as f32 * (bw + gap);
        let br = Rect::from_min_max(Pos2::new(x, r.bottom() - h), Pos2::new(x + bw, r.bottom()));
        let last = i + 1 == daily.len();
        p.rect_filled(br, 2.0, if last { CLAUDE } else { with_alpha(CLAUDE, 0.45) });
    }
}

/// The page when Claude Code is missing or signed out: what's wrong, how to
/// turn it on, and that the rest of the app works without it.
fn not_set_up(a: Availability, ui: &mut Ui, r: Rect, cx: &mut Cx) {
    let p = ui.painter().clone();
    page_header(&p, r, "Claude", a.short(), TEXT_DIM);
    let colw = 480.0f32.min(r.width() - 48.0);
    let x = r.center().x - colw / 2.0;
    let mut y = r.top() + PAGE_HEADER_H + 56.0;
    tool_chip(&p, Rect::from_center_size(Pos2::new(r.center().x, y + 28.0), Vec2::splat(56.0)), Icon::Sparkle, tint::CLAUDE);
    y += 56.0 + 20.0;
    p.text(Pos2::new(r.center().x, y), Align2::CENTER_TOP, a.title(), bold(20.0), TEXT);
    y += 32.0;
    let intro = "Set it up to see your plan limits, token use and Claude Code projects here, and to let Cleanup ask Claude about each path.";
    let g = p.layout(intro.into(), font(13.0), TEXT_DIM, colw);
    let gh = g.size().y;
    p.galley(Pos2::new(r.center().x - g.size().x / 2.0, y), g, TEXT_DIM);
    y += gh + 20.0;

    let steps: &[&str] = match a {
        Availability::SignedOut => &["Open Terminal and run claude", "Sign in with your Claude account", "Come back and press Check again"],
        _ => &["Install Claude Code from claude.com/claude-code", "Open Terminal and run claude once to sign in", "Come back and press Check again"],
    };
    let card = Rect::from_min_size(Pos2::new(x, y), Vec2::new(colw, 12.0 + steps.len() as f32 * 34.0));
    glass_card(&p, card, R_CARD, 0.0, false);
    for (i, s) in steps.iter().enumerate() {
        let cy = card.top() + 6.0 + 17.0 + i as f32 * 34.0;
        let c = Pos2::new(card.left() + 24.0, cy);
        p.circle_filled(c, 10.0, white(18));
        p.text(c, Align2::CENTER_CENTER, format!("{}", i + 1), bold(11.0), TEXT);
        text_fit(&p, Pos2::new(card.left() + 44.0, cy), Align2::LEFT_CENTER, s, font(13.0), TEXT_SOFT, colw - 58.0);
    }
    y = card.bottom() + 20.0;

    // buttons, centred
    let primary = "Check again";
    let second = if a == Availability::SignedOut { "Open Terminal" } else { "Get Claude Code" };
    let pw = button_width(&p, primary, BTN_PRIMARY);
    let sw = button_width(&p, second, BTN_SECONDARY);
    let left = r.center().x - (pw + 8.0 + sw) / 2.0;
    let sr = Rect::from_min_size(Pos2::new(left, y), Vec2::new(sw, 32.0));
    let pr = Rect::from_min_size(Pos2::new(sr.right() + 8.0, y), Vec2::new(pw, 32.0));
    if pill_button(ui, sr, second, BTN_SECONDARY, true).clicked() {
        if a == Availability::SignedOut {
            cc::open_terminal();
        } else {
            cc::open_download_page();
        }
    }
    if pill_button(ui, pr, primary, BTN_PRIMARY, true).clicked() {
        claude::request_refresh();
        cx.toast("Looking for Claude Code…");
    }
    y += 32.0 + 18.0;
    p.text(Pos2::new(r.center().x, y), Align2::CENTER_TOP, "Cleanup and the rest of the app work without it.", font(12.0), TEXT_FAINT);
}

pub fn page(info: &claude::Shared, demo: bool, ui: &mut Ui, r: Rect, cx: &mut Cx) {
    let cl = info.lock().unwrap().clone();
    if let Some(a) = cl.unavailable() {
        not_set_up(a, ui, r, cx);
        return;
    }
    let p = ui.painter().clone();
    let mut sub = cl.plan_title();
    match (&cl.fetched, cl.loading) {
        (_, true) => sub.push_str(" · updating limits…"),
        (Some(f), _) => sub.push_str(&format!(" · limits updated {}", f.format("%H:%M"))),
        _ => {}
    }
    if cl.data_bytes > 0 {
        sub.push_str(&format!(" · all Claude data on this Mac {}", human_bytes(cl.data_bytes)));
    }
    let btns = page_header(&p, r, "Claude", &sub, TEXT_DIM);
    let w = button_width(&p, "Refresh limits", BTN_SECONDARY);
    let rb = Rect::from_min_max(Pos2::new(btns.right() - w, btns.top()), btns.max);
    if pill_button(ui, rb, "Refresh limits", BTN_SECONDARY, !cl.loading).clicked() {
        claude::request_refresh();
        cx.toast("Asking Claude for your limits…");
    }

    let x0 = r.left() + 24.0;
    let x1 = r.right() - 24.0;
    let mut y = r.top() + PAGE_HEADER_H + 18.0;

    // limit cards + local tokens, 4 columns
    let gap = 12.0;
    let cw = (x1 - x0 - 3.0 * gap) / 4.0;
    let ch = 104.0;
    let mut col = 0;
    if cl.limits.is_empty() {
        let lr = Rect::from_min_size(Pos2::new(x0, y), Vec2::new(cw * 3.0 + gap * 2.0, ch));
        glass_card(&p, lr, R_CARD, 0.0, false);
        let msg = cl.error.clone().unwrap_or_else(|| if cl.loading { "Asking Claude Code for your limits…".into() } else { "No limits yet.".into() });
        p.text(lr.min + Vec2::new(12.0, 12.0), Align2::LEFT_TOP, "Plan limits", font(12.0), TEXT_DIM);
        paragraph(&p, lr.min + Vec2::new(12.0, 34.0), &msg, font(13.0), TEXT_SOFT, lr.width() - 24.0);
        if cl.loading {
            spinner(&p, Pos2::new(lr.right() - 20.0, lr.top() + 20.0), 6.0, ui.ctx().input(|i| i.time), TEXT_DIM);
            ui.ctx().request_repaint();
        }
        col = 3;
    }
    for l in cl.limits.iter().take(3) {
        let cr = Rect::from_min_size(Pos2::new(x0 + col as f32 * (cw + gap), y), Vec2::new(cw, ch));
        glass_card(&p, cr, R_CARD, 0.0, false);
        text_fit(&p, cr.min + Vec2::new(12.0, 12.0), Align2::LEFT_TOP, &claude::short_label(&l.label), font(12.0), TEXT_DIM, cw - 24.0);
        p.text(cr.min + Vec2::new(12.0, 30.0), Align2::LEFT_TOP, format!("{:.0}%", l.pct), bold(22.0), TEXT);
        bar(&p, Rect::from_min_size(cr.min + Vec2::new(12.0, 64.0), Vec2::new(cw - 24.0, 6.0)), (l.pct / 100.0).max(0.01), usage_color(l.pct));
        if !l.resets.is_empty() {
            text_fit(&p, cr.min + Vec2::new(12.0, 78.0), Align2::LEFT_TOP, &format!("Resets {}", l.resets.split(" (").next().unwrap_or(&l.resets)), font(12.0), TEXT_FAINT, cw - 24.0);
        }
        col += 1;
    }
    let tr = Rect::from_min_size(Pos2::new(x0 + 3.0 * (cw + gap), y), Vec2::new(cw, ch));
    glass_card(&p, tr, R_CARD, 0.0, false);
    p.text(tr.min + Vec2::new(12.0, 12.0), Align2::LEFT_TOP, "Tokens on this Mac", font(12.0), TEXT_DIM);
    let g = p.layout_no_wrap(claude::human_tokens(cl.local.tokens_today), bold(22.0), TEXT);
    let gw = g.size().x;
    p.galley(tr.min + Vec2::new(12.0, 30.0), g, TEXT);
    p.text(tr.min + Vec2::new(12.0 + gw + 5.0, 50.0), Align2::LEFT_BOTTOM, "today", font(12.0), TEXT_FAINT);
    day_bars(&p, Rect::from_min_size(tr.min + Vec2::new(12.0 + gw + 50.0, 32.0), Vec2::new((cw - gw - 74.0).max(30.0), 22.0)), &cl.local.daily, 3.0);
    p.text(tr.min + Vec2::new(12.0, 78.0), Align2::LEFT_TOP, format!("{} in 7 days", claude::human_tokens(cl.local.tokens_7d)), font(12.0), TEXT_FAINT);
    y += ch + 16.0;

    // projects
    // the card fits its rows (no tall empty box), up to the footer
    let max_bottom = r.bottom() - PAGE_FOOTER_H - 18.0;
    let rows_h = if cl.projects.is_empty() { 64.0 } else { cl.projects.len() as f32 * 44.0 };
    let pr = Rect::from_min_max(Pos2::new(x0, y), Pos2::new(x1, (y + 72.0 + rows_h + 6.0).min(max_bottom)));
    glass_card(&p, pr, R_CARD, 0.0, false);
    p.text(pr.min + Vec2::new(14.0, 13.0), Align2::LEFT_TOP, "Claude Code projects", bold(13.0), TEXT);
    let used = cl.projects.iter().filter(|p| p.in_use()).count();
    p.text(Pos2::new(pr.right() - 14.0, pr.top() + 14.0), Align2::RIGHT_TOP, format!("{} projects · {} in use, kept by Cleanup", cl.projects.len(), used), font(12.0), TEXT_FAINT);
    hairline(&p, Pos2::new(pr.left(), pr.top() + 40.0), Pos2::new(pr.right(), pr.top() + 40.0));
    let c_status = pr.right() - 14.0 - 100.0 - 12.0 - 90.0 - 12.0 - 120.0;
    let c_size = pr.right() - 14.0 - 100.0 - 12.0;
    let c_when = pr.right() - 14.0;
    let hy = pr.top() + 50.0;
    section_header(&p, Pos2::new(pr.left() + 14.0, hy), Align2::LEFT_TOP, "PROJECT");
    section_header(&p, Pos2::new(c_status, hy), Align2::LEFT_TOP, "STATUS");
    section_header(&p, Pos2::new(c_size, hy), Align2::RIGHT_TOP, "SIZE");
    section_header(&p, Pos2::new(c_when, hy), Align2::RIGHT_TOP, "LAST USED");
    let list = Rect::from_min_max(Pos2::new(pr.left(), hy + 22.0), pr.max - Vec2::new(0.0, 6.0));
    let mut child = ui.new_child(UiBuilder::new().max_rect(list).layout(egui::Layout::top_down(egui::Align::Min)));
    child.set_clip_rect(list);
    let home = dirs::home_dir().unwrap_or_default().display().to_string();
    ScrollArea::vertical().id_salt("claude-projects").auto_shrink([false, false]).show(&mut child, |ui| {
        ui.spacing_mut().item_spacing.y = 0.0;
        if cl.projects.is_empty() {
            let (row, _) = ui.allocate_exact_size(Vec2::new(list.width(), 64.0), Sense::hover());
            let pp = ui.painter();
            pp.line_segment([Pos2::new(row.left() + 14.0, row.top()), Pos2::new(row.right() - 14.0, row.top())], egui::Stroke::new(1.0, white(10)));
            let msg = if demo { "No Claude Code projects on this Mac yet." } else { "Looking for Claude Code projects…" };
            pp.text(Pos2::new(row.left() + 14.0, row.top() + 14.0), Align2::LEFT_TOP, msg, font(13.0), TEXT_SOFT);
            pp.text(Pos2::new(row.left() + 14.0, row.top() + 34.0), Align2::LEFT_TOP, "Each folder where you run Claude Code shows up here with its size and last use.", font(12.0), TEXT_FAINT);
        }
        for pj in &cl.projects {
            let (row, _) = ui.allocate_exact_size(Vec2::new(list.width(), 44.0), Sense::hover());
            let pp = ui.painter();
            pp.line_segment([Pos2::new(row.left() + 14.0, row.top()), Pos2::new(row.right() - 14.0, row.top())], egui::Stroke::new(1.0, white(10)));
            let (dot, label, lc) = if pj.in_use() {
                (GREEN, "In use", Color32::from_rgb(0x5B, 0xE0, 0x7F))
            } else if !pj.cwd_exists {
                (AMBER, "Folder gone", Color32::from_rgb(0xFF, 0xC2, 0x66))
            } else {
                (TEXT_FAINT, "Idle", TEXT_DIM)
            };
            let name_w = c_status - row.left() - 26.0;
            text_fit(pp, Pos2::new(row.left() + 14.0, row.top() + 7.0), Align2::LEFT_TOP, &pj.display_name(), bold(13.0), TEXT, name_w);
            let path = pj.cwd.as_ref().map(|c| c.display().to_string().replacen(&home, "~", 1)).unwrap_or_else(|| "unknown folder".into());
            text_fit(pp, Pos2::new(row.left() + 14.0, row.top() + 25.0), Align2::LEFT_TOP, &middle_ellipsis(&path, 80), mono(11.0), TEXT_FAINT, name_w);
            pp.circle_filled(Pos2::new(c_status + 4.0, row.center().y), 4.0, dot);
            pp.text(Pos2::new(c_status + 14.0, row.center().y), Align2::LEFT_CENTER, label, font(13.0), lc);
            pp.text(Pos2::new(c_size, row.center().y), Align2::RIGHT_CENTER, human_bytes(pj.bytes), font(13.0), TEXT);
            let idle = pj.days_idle();
            let when = if idle < 1.0 { "today".to_string() } else if idle < 2.0 { "yesterday".to_string() } else { format!("{} days ago", idle as u64) };
            pp.text(Pos2::new(c_when, row.center().y), Align2::RIGHT_CENTER, when, font(13.0), TEXT_DIM);
        }
    });

    // footer
    let f = page_footer(&p, r, 12.0);
    let bl = "Review in Cleanup";
    let bw = button_width(&p, bl, BTN_PRIMARY);
    let br = Rect::from_min_size(Pos2::new(f.right() - 24.0 - bw, f.center().y - 16.0), Vec2::new(bw, 32.0));
    text_fit(&p, Pos2::new(f.left() + 24.0, f.center().y), Align2::LEFT_CENTER, "Cleanup finds old Claude sessions, caches and logs and never touches projects in use.", font(13.0), TEXT_DIM, br.left() - f.left() - 40.0);
    if pill_button(ui, br, bl, BTN_PRIMARY, true).clicked() {
        cx.open_page(CLEANUP, "claude");
    }
}
