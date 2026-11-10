//! The Claude card for the panel and the limits sheet beside it (also the
//! left half of the Claude page).

use super::backend;
use super::ID;
use crate::core::tool::{Cx, Size};
use crate::kit::tile;
use crate::kit::glass::*;
use egui::{Align2, Color32, Pos2, Rect, Ui, Vec2};

/// The Cleanup tool, linked to by id only.
pub const CLEANUP: &str = "cleanup";
const GAP: f32 = 12.0;

/// The Claude widget at `size`.
pub fn widget(ui: &mut Ui, r: Rect, size: Size, cl: &backend::ClaudeInfo, sel: bool) -> egui::Response {
    let id = ui.scope_id().with("w-claude");
    let t = ui.ctx().input(|i| i.time);
    let session = cl.session().cloned();
    let pct = session.as_ref().map(|l| l.pct).unwrap_or(0.0);
    let col = super::page::usage_color(pct);
    let value = session.as_ref().map(|_| format!("{pct:.0}%")).unwrap_or_else(|| "–".into());
    match size {
        Size::S11 => tile::small_ring(ui, r, id, Icon::Gauge, pct / 100.0, col, &format!("Session {value}"), sel),
        Size::S12 => {
            let st = status(cl);
            let bar_or = if session.is_some() { Ok(((pct / 100.0).max(0.01), col)) } else { Err(st.as_str()) };
            tile::wide(ui, r, id, Icon::Gauge, tint::CLAUDE, "Claude · session", &value, bar_or, sel)
        }
        Size::S14 => {
            let resp = tile::card(ui, r, id, sel);
            card(ui.painter(), r, cl, t);
            resp
        }
        _ => {
            let resp = tile::card(ui, r, id, sel);
            let p = ui.painter();
            let body = tile::head(p, r, Icon::Gauge, tint::CLAUDE, &cl.plan_title(), &status(cl));
            for (k, (label, l)) in [("Session", cl.session()), ("Week", cl.weekly())].into_iter().enumerate() {
                let lr = Rect::from_min_size(Pos2::new(body.left(), body.top() + k as f32 * 22.0), Vec2::new(body.width(), 22.0));
                let v = l.map(|l| format!("{:.0}% · {}", l.pct, short_reset(&l.resets).trim_start_matches("resets "))).unwrap_or_default();
                tile::bar_line(p, lr, label, l.map(|l| ((l.pct / 100.0).max(0.01), super::page::usage_color(l.pct))), &v);
            }
            let lr = Rect::from_min_max(Pos2::new(body.left(), body.top() + 52.0), body.max);
            p.text(lr.left_top(), Align2::LEFT_TOP, "Tokens · 7 days", font(11.0), TEXT_FAINT);
            super::page::day_bars(p, Rect::from_min_max(lr.left_top() + Vec2::new(0.0, 17.0), lr.max), &cl.local.daily, 5.0);
            resp
        }
    }
}

fn status(cl: &backend::ClaudeInfo) -> String {
    if cl.loading && cl.limits.is_empty() {
        "Checking…".to_string()
    } else if !cl.installed && !cl.loading {
        "Not installed".to_string()
    } else if cl.signed_out && cl.limits.is_empty() {
        "Signed out".to_string()
    } else if cl.limits.is_empty() {
        cl.error.clone().unwrap_or_else(|| "No data yet".into())
    } else {
        format!("{} tokens today", backend::human_tokens(cl.local.tokens_today))
    }
}

/// The panel card: plan, tokens today, then Session and Week bars.
pub fn card(p: &egui::Painter, r: Rect, cl: &backend::ClaudeInfo, t: f64) {
    let x0 = r.left() + 12.0;
    let x1 = r.right() - 12.0;
    let hy = r.top() + 17.0;
    let status = status(cl);
    let sw = text_fit(p, Pos2::new(x1, hy), Align2::RIGHT_CENTER, &status, ty::label(), white(166), 170.0).width();
    text_fit(p, Pos2::new(x0, hy), Align2::LEFT_CENTER, &cl.plan_title(), ty::title(), TEXT, (x1 - sw - 12.0) - x0);
    if cl.limits.is_empty() && cl.loading {
        spinner(p, Pos2::new(x1 - 8.0, r.top() + 56.0), 7.0, t, TEXT_DIM);
    }
    let rows = [("Session", cl.session().cloned()), ("Week", cl.weekly().cloned())];
    for (k, (label, l)) in rows.iter().enumerate() {
        let yy = r.top() + 42.0 + k as f32 * 22.0;
        p.text(Pos2::new(x0, yy), Align2::LEFT_CENTER, *label, font(12.0), TEXT_DIM);
        let bar_r = Rect::from_min_max(Pos2::new(x0 + 64.0, yy - 2.0), Pos2::new(x1 - 92.0 - 36.0 - 16.0, yy + 2.0));
        match l {
            Some(l) => {
                bar(p, bar_r, (l.pct / 100.0).max(0.01), super::page::usage_color(l.pct));
                p.text(Pos2::new(x1 - 92.0 - 8.0, yy), Align2::RIGHT_CENTER, format!("{:.0}%", l.pct), font(12.0), TEXT);
                text_fit(p, Pos2::new(x1, yy), Align2::RIGHT_CENTER, &short_reset(&l.resets), font(12.0), TEXT_FAINT, 92.0);
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

/// The Claude sheet beside the panel: limits, local tokens, what uses the
/// limits, and links to the page and to Cleanup.
pub fn detail(info: &backend::Shared, ui: &mut Ui, rect: Rect, cx: &mut Cx) {
    let ctx = ui.ctx().clone();
    let cl = info.lock().unwrap().clone();
    let p = ui.painter().clone();
    let t = ctx.input(|i| i.time);
    let sub = match (&cl.fetched, cl.loading) {
        (_, true) => "Updating limits…".to_string(),
        (Some(f), _) => format!("Limits updated {}", f.format("%H:%M")),
        _ => String::new(),
    };
    if sheet_header(ui, rect, Icon::Gauge, tint::CLAUDE, &cl.plan_title(), &sub) {
        cx.close_detail();
    }
    let rb = Rect::from_center_size(Pos2::new(rect.right() - 27.0 - 34.0, rect.top() + 29.0), Vec2::splat(26.0));
    if cl.loading {
        spinner(&p, rb.center(), 6.0, t, TEXT_DIM);
        crate::kit::glass::animate(&ctx);
    } else if round_button(ui, rb, Icon::Refresh, "Refresh limits").clicked() {
        backend::request_refresh();
        cx.toast("Asking Claude for your limits…");
    }
    let x0 = rect.left() + 16.0;
    let w = rect.width() - 32.0;
    let mut y = rect.top() + SHEET_HEADER_H + 16.0;

    // limits
    let n = cl.limits.len();
    let lim_h = if n == 0 { 64.0 } else { n as f32 * 58.0 + 13.0 };
    let lr = Rect::from_min_size(Pos2::new(x0, y), Vec2::new(w, lim_h));
    glass_card(&p, lr, R_CARD, 0.0, false);
    if n == 0 {
        let msg = cl.error.clone().unwrap_or_else(|| "Asking Claude Code for your limits…".into());
        paragraph(&p, lr.min + Vec2::new(12.0, 12.0), &msg, font(13.0), TEXT_DIM, w - 24.0);
    }
    for (i, l) in cl.limits.iter().enumerate() {
        let yy = lr.top() + 12.0 + i as f32 * 58.0;
        p.text(Pos2::new(lr.left() + 12.0, yy), Align2::LEFT_TOP, backend::short_label(&l.label), font(13.0), TEXT);
        p.text(Pos2::new(lr.right() - 12.0, yy), Align2::RIGHT_TOP, format!("{:.0}%", l.pct), bold(13.0), TEXT);
        bar(&p, Rect::from_min_size(Pos2::new(lr.left() + 12.0, yy + 20.0), Vec2::new(w - 24.0, 6.0)), (l.pct / 100.0).max(0.01), super::page::usage_color(l.pct));
        if !l.resets.is_empty() {
            text_fit(&p, Pos2::new(lr.left() + 12.0, yy + 32.0), Align2::LEFT_TOP, &format!("Resets {}", l.resets), font(12.0), TEXT_FAINT, w - 24.0);
        }
    }
    y = lr.bottom() + GAP;

    // on this Mac
    let ar = Rect::from_min_size(Pos2::new(x0, y), Vec2::new(w, 150.0));
    glass_card(&p, ar, R_CARD, 0.0, false);
    p.text(ar.min + Vec2::new(12.0, 12.0), Align2::LEFT_TOP, "On this Mac", bold(13.0), TEXT);
    let stats = [("last 5 hours", cl.local.tokens_5h), ("today", cl.local.tokens_today), ("7 days", cl.local.tokens_7d)];
    for (i, (k, v)) in stats.iter().enumerate() {
        let x = ar.left() + 12.0 + i as f32 * ((w - 24.0) / 3.0);
        p.text(Pos2::new(x, ar.top() + 36.0), Align2::LEFT_TOP, backend::human_tokens(*v), bold(18.0), TEXT);
        p.text(Pos2::new(x, ar.top() + 60.0), Align2::LEFT_TOP, *k, font(11.0), TEXT_FAINT);
    }
    super::page::day_bars(&p, Rect::from_min_size(Pos2::new(ar.left() + 12.0, ar.top() + 82.0), Vec2::new(w - 24.0, 40.0)), &cl.local.daily, 6.0);
    p.text(Pos2::new(ar.left() + 12.0, ar.bottom() - 10.0), Align2::LEFT_BOTTOM, "7 days ago", font(11.0), TEXT_FAINT);
    p.text(Pos2::new(ar.right() - 12.0, ar.bottom() - 10.0), Align2::RIGHT_BOTTOM, "Today", font(11.0), TEXT_FAINT);
    y = ar.bottom() + GAP;

    // what uses your limits
    let by = rect.bottom() - 16.0 - 30.0;
    let ur_max = by - GAP;
    let mut lines: Vec<(String, Color32, f32)> = Vec::new();
    match cl.contrib.first() {
        Some(c) => {
            lines.push((c.title.clone(), TEXT_FAINT, 12.0));
            for l in &c.lines {
                lines.push((l.clone(), TEXT_SOFT, 13.0));
            }
        }
        None => lines.push(("Claude Code reports this once you've used it on this Mac.".into(), TEXT_DIM, 13.0)),
    }
    let heights: Vec<f32> = lines.iter().map(|(l, c, s)| p.layout(l.clone(), font(*s), *c, w - 24.0).size().y).collect();
    let mut h = 12.0 + 18.0 + 6.0;
    let mut shown = 0;
    for hh in &heights {
        if y + h + hh + 6.0 + 12.0 > ur_max {
            break;
        }
        h += hh + 6.0;
        shown += 1;
    }
    let ur = Rect::from_min_size(Pos2::new(x0, y), Vec2::new(w, h + 6.0));
    glass_card(&p, ur, R_CARD, 0.0, false);
    p.text(ur.min + Vec2::new(12.0, 12.0), Align2::LEFT_TOP, "What uses your limits", bold(13.0), TEXT);
    let mut ly = ur.top() + 36.0;
    for ((l, c, s), hh) in lines.iter().zip(&heights).take(shown) {
        paragraph(&p, Pos2::new(ur.left() + 12.0, ly), l, font(*s), *c, w - 24.0);
        ly += hh + 6.0;
    }

    // buttons, bottom right
    let primary = "Clean Claude caches";
    let pw = button_width(&p, primary, BTN_PRIMARY);
    let pr = Rect::from_min_size(Pos2::new(rect.right() - 16.0 - pw, by), Vec2::new(pw, 30.0));
    if pill_button(ui, pr, primary, BTN_PRIMARY, true).clicked() {
        cx.open_page(CLEANUP, "claude");
    }
    let sw = button_width(&p, "Open page", BTN_SECONDARY);
    let sr = Rect::from_min_size(Pos2::new(pr.left() - 8.0 - sw, by), Vec2::new(sw, 30.0));
    if pill_button(ui, sr, "Open page", BTN_SECONDARY, true).clicked() {
        cx.open_page(ID, "");
    }
}
