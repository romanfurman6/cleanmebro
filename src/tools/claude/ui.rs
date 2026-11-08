//! The Claude card for the panel and the limits sheet beside it (also the
//! left half of the Claude page).

use super::backend;
use super::ID;
use crate::core::tool::Cx;
use crate::kit::glass::*;
use crate::kit::mechanics as m;
use egui::{Align2, Color32, Pos2, Rect, Ui, Vec2};

/// The Cleanup tool, linked to by id only.
pub const CLEANUP: &str = "cleanup";
const GAP: f32 = 12.0;

pub fn card(p: &egui::Painter, r: Rect, cl: &backend::ClaudeInfo, t: f64) {
    tile_icon(p, Rect::from_min_size(r.min + Vec2::new(16.0, 15.0), Vec2::splat(30.0)), Icon::Sparkle, Color32::from_rgb(255, 180, 140), Color32::from_rgb(210, 96, 66));
    p.text(r.min + Vec2::new(58.0, 15.0), Align2::LEFT_TOP, cl.plan_title(), bold(17.0), TEXT);
    let status = if cl.loading && cl.limits.is_empty() {
        "Checking…".to_string()
    } else if !cl.installed && !cl.loading {
        "Not installed".to_string()
    } else if cl.limits.is_empty() {
        cl.error.clone().unwrap_or_else(|| "No data yet".into())
    } else {
        format!("{} tokens today", backend::human_tokens(cl.local.tokens_today))
    };
    let st_col = if cl.limits.is_empty() { TEXT_FAINT } else { TEXT_DIM };
    if !cl.limits.is_empty() {
        p.circle_filled(r.right_top() + Vec2::new(-16.0 - p.layout_no_wrap(status.clone(), font(13.0), st_col).size().x - 10.0, 25.0), 3.5, GREEN);
    }
    text_fit(p, r.right_top() + Vec2::new(-16.0, 18.0), Align2::RIGHT_TOP, &status, font(13.0), st_col, 180.0);
    if cl.limits.is_empty() && cl.loading {
        spinner(p, r.right_top() + Vec2::new(-24.0, 60.0), 9.0, t, TEXT_DIM);
    }
    let rows = [cl.session().cloned(), cl.weekly().cloned()];
    for (k, l) in rows.iter().enumerate() {
        let yy = r.top() + 58.0 + k as f32 * 24.0;
        let label = if k == 0 { "Session" } else { "Week" };
        p.text(Pos2::new(r.left() + 18.0, yy), Align2::LEFT_CENTER, label, font(13.5), TEXT_DIM);
        let bar_r = Rect::from_min_size(Pos2::new(r.left() + 82.0, yy - 3.5), Vec2::new(r.width() - 250.0, 7.0));
        match l {
            Some(l) => {
                let c = if l.pct >= 90.0 { RED } else if l.pct >= 70.0 { AMBER } else { BLUE };
                bar(p, bar_r, l.pct / 100.0, c);
                p.text(Pos2::new(bar_r.right() + 10.0, yy), Align2::LEFT_CENTER, format!("{:.0}%", l.pct), bold(13.5), TEXT);
                text_fit(p, Pos2::new(r.right() - 16.0, yy), Align2::RIGHT_CENTER, &short_reset(&l.resets), font(12.5), TEXT_FAINT, 110.0);
            }
            None => bar(p, bar_r, 0.0, TEXT_DIM),
        }
    }
}

pub fn short_reset(s: &str) -> String {
    if s.is_empty() {
        return String::new();
    }
    let s = s.split(" (").next().unwrap_or(s);
    format!("resets {}", s)
}

pub fn detail(info: &backend::Shared, ui: &mut Ui, r: Rect, cx: &mut Cx) {
    let ctx = ui.ctx().clone();
    let cl = info.lock().unwrap().clone();
    let p = ui.painter().clone();
    let t = ctx.input(|i| i.time);
    let mut y = r.top();
    // limits
    let lim_h = 52.0 + cl.limits.len().max(2) as f32 * 62.0;
    let lr = Rect::from_min_size(Pos2::new(r.left(), y), Vec2::new(r.width(), lim_h));
    glass_card(&p, lr, m::CARD_RADIUS, 0.0, false);
    p.text(lr.min + Vec2::new(16.0, 14.0), Align2::LEFT_TOP, "Plan limits", bold(16.0), TEXT);
    let upd = match (&cl.fetched, cl.loading) {
        (_, true) => "Updating…".to_string(),
        (Some(f), _) => format!("Updated {}", f.format("%H:%M")),
        _ => String::new(),
    };
    p.text(lr.right_top() + Vec2::new(-16.0, 16.0), Align2::RIGHT_TOP, upd, font(12.5), TEXT_FAINT);
    if cl.loading {
        spinner(&p, lr.right_top() + Vec2::new(-110.0, 23.0), 6.0, t, TEXT_DIM);
        ctx.request_repaint();
    }
    if cl.limits.is_empty() {
        let msg = cl.error.clone().unwrap_or_else(|| "Asking Claude Code for your limits…".into());
        paragraph(&p, lr.min + Vec2::new(16.0, 48.0), &msg, font(14.0), TEXT_DIM, lr.width() - 32.0);
    }
    for (i, l) in cl.limits.iter().enumerate() {
        let yy = lr.top() + 48.0 + i as f32 * 62.0;
        p.text(Pos2::new(lr.left() + 16.0, yy), Align2::LEFT_TOP, backend::short_label(&l.label), font(14.0), TEXT);
        p.text(Pos2::new(lr.right() - 16.0, yy), Align2::RIGHT_TOP, format!("{:.0}%", l.pct), bold(14.0), TEXT);
        let c = if l.pct >= 90.0 { RED } else if l.pct >= 70.0 { AMBER } else { BLUE };
        bar(&p, Rect::from_min_size(Pos2::new(lr.left() + 16.0, yy + 22.0), Vec2::new(lr.width() - 32.0, 7.0)), l.pct / 100.0, c);
        if !l.resets.is_empty() {
            text_fit(&p, Pos2::new(lr.left() + 16.0, yy + 36.0), Align2::LEFT_TOP, &format!("Resets {}", l.resets), font(12.0), TEXT_FAINT, lr.width() - 32.0);
        }
    }
    y = lr.bottom() + GAP;
    // local activity
    let ar = Rect::from_min_size(Pos2::new(r.left(), y), Vec2::new(r.width(), 150.0));
    glass_card(&p, ar, m::CARD_RADIUS, 0.0, false);
    p.text(ar.min + Vec2::new(16.0, 14.0), Align2::LEFT_TOP, "On this Mac", bold(16.0), TEXT);
    let stats = [("Last 5 hours", cl.local.tokens_5h), ("Today", cl.local.tokens_today), ("7 days", cl.local.tokens_7d)];
    for (i, (k, v)) in stats.iter().enumerate() {
        let x = ar.left() + 16.0 + i as f32 * ((ar.width() - 32.0) / 3.0);
        p.text(Pos2::new(x, ar.top() + 44.0), Align2::LEFT_TOP, backend::human_tokens(*v), bold(20.0), TEXT);
        p.text(Pos2::new(x, ar.top() + 70.0), Align2::LEFT_TOP, format!("tokens · {k}"), font(12.0), TEXT_FAINT);
    }
    let maxd = cl.local.daily.iter().copied().max().unwrap_or(1).max(1) as f32;
    let bw = (ar.width() - 32.0) / 7.0;
    for (i, v) in cl.local.daily.iter().enumerate() {
        let h = 34.0 * (*v as f32 / maxd);
        let br = Rect::from_min_max(Pos2::new(ar.left() + 16.0 + i as f32 * bw + 3.0, ar.bottom() - 12.0 - h.max(2.0)), Pos2::new(ar.left() + 16.0 + (i + 1) as f32 * bw - 3.0, ar.bottom() - 12.0));
        gradient_rect(&p, br, 3.0, Color32::from_rgb(255, 180, 140), Color32::from_rgb(210, 96, 66));
    }
    y = ar.bottom() + GAP;
    // what's driving usage
    let rest = Rect::from_min_max(Pos2::new(r.left(), y), r.max);
    glass_card(&p, rest, m::CARD_RADIUS, 0.0, false);
    p.text(rest.min + Vec2::new(16.0, 14.0), Align2::LEFT_TOP, "What uses your limits", bold(16.0), TEXT);
    let mut yy = rest.top() + 42.0;
    if let Some(c) = cl.contrib.first() {
        p.text(Pos2::new(rest.left() + 16.0, yy), Align2::LEFT_TOP, &c.title, font(13.0), TEXT_FAINT);
        yy += 22.0;
        for line in &c.lines {
            if yy > rest.bottom() - 60.0 {
                break;
            }
            p.circle_filled(Pos2::new(rest.left() + 20.0, yy + 8.0), 2.5, Color32::from_rgb(255, 170, 130));
            yy += paragraph(&p, Pos2::new(rest.left() + 30.0, yy), line, font(13.0), TEXT_DIM, rest.width() - 46.0) + 6.0;
        }
    } else {
        paragraph(&p, Pos2::new(rest.left() + 16.0, yy), "Claude Code reports this once you've used it on this Mac.", font(13.0), TEXT_DIM, rest.width() - 32.0);
    }
    let label = "Clean Claude caches";
    let bw = p.layout_no_wrap(label.to_string(), bold(14.0), TEXT).size().x + 36.0;
    let br = Rect::from_min_size(Pos2::new(rest.right() - 16.0 - bw, rest.bottom() - 46.0), Vec2::new(bw, 32.0));
    if pill_button(ui, br, label, 1, true).clicked() {
        cx.open_page(CLEANUP, "claude");
    }
    let br2 = Rect::from_min_size(Pos2::new(rest.left() + 16.0, rest.bottom() - 46.0), Vec2::new(110.0, 32.0));
    if pill_button(ui, br2, "Details", 0, true).clicked() {
        cx.open_page(ID, "");
    }
}
