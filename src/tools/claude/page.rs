//! The AI usage page in the full window: a Claude / Codex switch, plan
//! limits, local token use and, for Claude, the Claude Code projects on
//! this Mac (which ones count as "in use" and are protected by Cleanup).

use super::backend as claude;
use super::ui::CLEANUP;
use crate::core::tool::Cx;
use crate::kit::ai::{self, UsageInfo};
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

/// The Claude / Codex switch, right-aligned in `area`. Returns its left edge.
fn picker(info: &claude::Shared, demo: bool, ui: &mut Ui, area: Rect) -> f32 {
    let labels: Vec<&str> = ai::PROVIDERS.iter().map(|m| m.name).collect();
    let w = 64.0 * labels.len() as f32;
    let rect = Rect::from_min_max(Pos2::new(area.right() - w, area.top()), area.max);
    if let Some(i) = segmented(ui, rect, ui.scope_id().with("ai-provider"), &labels, ai::selected()) {
        claude::select(info, demo, i);
    }
    rect.left()
}

/// The page when the assistant is missing or signed out: what's wrong, how
/// to turn it on, and that the rest of the app works without it.
fn not_set_up(a: Availability, cl: &UsageInfo, info: &claude::Shared, demo: bool, ui: &mut Ui, r: Rect, cx: &mut Cx) {
    let p = ui.painter().clone();
    let m = ai::meta(cl.provider);
    let short = if a == Availability::SignedOut { format!("{} signed out", m.name) } else { format!("{} not installed", m.name) };
    let btns = page_header(&p, r, m.name, &short, TEXT_DIM);
    picker(info, demo, ui, btns);
    let colw = 480.0f32.min(r.width() - 48.0);
    let x = r.center().x - colw / 2.0;
    let mut y = r.top() + PAGE_HEADER_H + 56.0;
    tool_chip(&p, Rect::from_center_size(Pos2::new(r.center().x, y + 28.0), Vec2::splat(56.0)), Icon::Gauge, tint::CLAUDE);
    y += 56.0 + 20.0;
    let title = if a == Availability::SignedOut { format!("{} isn't signed in", m.name) } else { format!("{} isn't installed", m.name) };
    p.text(Pos2::new(r.center().x, y), Align2::CENTER_TOP, title, bold(20.0), TEXT);
    y += 32.0;
    let intro = if m.id == "claude" {
        "Set it up to see your plan limits, token use and Claude Code projects here, and to let Cleanup ask Claude about each path.".to_string()
    } else {
        format!("Set it up to see your {} plan limits and token use here. Or switch back to the assistant you use above.", m.name)
    };
    let g = p.layout(intro, font(13.0), TEXT_DIM, colw);
    let gh = g.size().y;
    p.galley(Pos2::new(r.center().x - g.size().x / 2.0, y), g, TEXT_DIM);
    y += gh + 20.0;

    let steps: &[&str] = if a == Availability::SignedOut { m.sign_in_steps } else { m.install_steps };
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
    let get = format!("Get {}", if m.id == "claude" { "Claude Code" } else { m.name });
    let second = if a == Availability::SignedOut { "Open Terminal" } else { get.as_str() };
    let pw = button_width(&p, primary, BTN_PRIMARY);
    let sw = button_width(&p, second, BTN_SECONDARY);
    let left = r.center().x - (pw + 8.0 + sw) / 2.0;
    let sr = Rect::from_min_size(Pos2::new(left, y), Vec2::new(sw, 32.0));
    let pr = Rect::from_min_size(Pos2::new(sr.right() + 8.0, y), Vec2::new(pw, 32.0));
    if pill_button(ui, sr, second, BTN_SECONDARY, true).clicked() {
        if a == Availability::SignedOut {
            cc::open_terminal();
        } else {
            let _ = std::process::Command::new("open").arg(m.install_url).spawn();
        }
    }
    if pill_button(ui, pr, primary, BTN_PRIMARY, true).clicked() {
        claude::request_refresh();
        cx.toast(format!("Looking for {}…", m.name));
    }
    y += 32.0 + 18.0;
    p.text(Pos2::new(r.center().x, y), Align2::CENTER_TOP, "Cleanup and the rest of the app work without it.", font(12.0), TEXT_FAINT);
}

pub fn page(info: &claude::Shared, demo: bool, ui: &mut Ui, r: Rect, cx: &mut Cx) {
    let cl = info.lock().unwrap().clone();
    if let Some(a) = cl.unavailable() {
        not_set_up(a, &cl, info, demo, ui, r, cx);
        return;
    }
    let is_claude = ai::meta(cl.provider).id == "claude";
    let p = ui.painter().clone();
    let mut sub = cl.plan_title();
    match (&cl.fetched, cl.loading) {
        (_, true) => sub.push_str(" · updating limits…"),
        (Some(f), _) => sub.push_str(&format!(" · limits updated {}", f.format("%H:%M"))),
        _ => {}
    }
    if cl.data_bytes > 0 {
        sub.push_str(&format!(" · all {} data on this Mac {}", cl.provider, human_bytes(cl.data_bytes)));
    }
    let btns = page_header(&p, r, cl.provider, &sub, TEXT_DIM);
    let w = button_width(&p, "Refresh limits", BTN_SECONDARY);
    let rb = Rect::from_min_max(Pos2::new(btns.right() - w, btns.top()), btns.max);
    if pill_button(ui, rb, "Refresh limits", BTN_SECONDARY, !cl.loading).clicked() {
        claude::request_refresh();
        cx.toast(format!("Asking {} for your limits…", cl.provider));
    }
    picker(info, demo, ui, Rect::from_min_max(btns.min, Pos2::new(rb.left() - sp::M, btns.bottom())));

    let x0 = r.left() + 20.0;
    let x1 = r.right() - 20.0;
    let y = r.top() + PAGE_HEADER_H + 1.0 + sp::L;
    let gap = sp::L;

    // row 1: a ring per plan limit, then today's tokens (4 columns)
    let cw = (x1 - x0 - 3.0 * gap) / 4.0;
    let ch = 150.0;
    let mut col = 0;
    if cl.limits.is_empty() {
        let lr = Rect::from_min_size(Pos2::new(x0, y), Vec2::new(cw * 3.0 + gap * 2.0, ch));
        glass_card(&p, lr, R_CARD, 0.0, false);
        let msg = cl.error.clone().unwrap_or_else(|| if cl.loading { format!("Asking {} for your limits…", cl.provider) } else { "No limits yet.".into() });
        p.text(lr.min + Vec2::splat(sp::L), Align2::LEFT_TOP, "Plan limits", ty::title(), TEXT);
        paragraph(&p, lr.min + Vec2::new(sp::L, sp::L + 16.0 + sp::M), &msg, ty::body(), TEXT_SOFT, lr.width() - 2.0 * sp::L);
        if cl.loading {
            spinner(&p, Pos2::new(lr.right() - 24.0, lr.top() + 24.0), 6.0, ui.ctx().input(|i| i.time), TEXT_DIM);
            crate::kit::glass::animate(ui.ctx());
        }
        col = 3;
    }
    for l in cl.limits.iter().take(3) {
        let cr = Rect::from_min_size(Pos2::new(x0 + col as f32 * (cw + gap), y), Vec2::new(cw, ch));
        glass_card(&p, cr, R_CARD, 0.0, false);
        let inner = cr.shrink(sp::L);
        text_fit(&p, inner.left_top(), Align2::LEFT_TOP, &claude::short_label(&l.label), ty::title(), TEXT, inner.width());
        let rr = 36.0;
        let c = Pos2::new(inner.left() + rr + 5.0, inner.bottom() - rr - 5.0);
        crate::kit::tile::ring(&p, c, rr, 9.0, (l.pct / 100.0).max(0.01), usage_color(l.pct));
        p.text(c, Align2::CENTER_CENTER, format!("{:.0}%", l.pct), ty::lead(), TEXT);
        let tx = c.x + rr + sp::L;
        if !l.resets.is_empty() {
            p.text(Pos2::new(tx, c.y - 2.0), Align2::LEFT_BOTTOM, "Resets", ty::label(), ty::caption_color());
            text_fit(&p, Pos2::new(tx, c.y + 2.0), Align2::LEFT_TOP, l.resets.split(" (").next().unwrap_or(&l.resets), ty::lead(), TEXT, inner.right() - tx);
        }
        col += 1;
    }
    // today takes the slots no limit uses (Codex has two, Claude three)
    let tr = Rect::from_min_max(Pos2::new(x0 + col as f32 * (cw + gap), y), Pos2::new(x1, y + ch));
    glass_card(&p, tr, R_CARD, 0.0, false);
    let ti = tr.shrink(sp::L);
    p.text(ti.left_top(), Align2::LEFT_TOP, "Today", ty::title(), TEXT);
    p.text(ti.left_top() + Vec2::new(0.0, 16.0 + sp::XS), Align2::LEFT_TOP, "Tokens on this Mac", ty::caption(), ty::caption_color());
    p.text(Pos2::new(ti.left(), ti.top() + 34.0 + sp::M), Align2::LEFT_TOP, claude::human_tokens(cl.local.tokens_today), ty::hero(), TEXT);
    let req = if cl.local.requests_today > 0 { format!("{} requests", cl.local.requests_today) } else { String::new() };
    text_fit(&p, Pos2::new(ti.left(), ti.bottom()), Align2::LEFT_BOTTOM, &format!("{} in 7 days{}", claude::human_tokens(cl.local.tokens_7d), if req.is_empty() { String::new() } else { format!(" · {req}") }), ty::label(), white(166), ti.width());

    // row 2: the week as bars, and the projects
    let y2 = y + ch + gap;
    let bottom = r.bottom() - PAGE_FOOTER_H - sp::L;
    let unit = (x1 - x0 - gap) / 2.6;
    let wr = Rect::from_min_max(Pos2::new(x0, y2), Pos2::new(x0 + unit * 1.0, bottom));
    // projects take what their rows need; what's left explains the limits
    let rows_h = if cl.projects.is_empty() { 64.0 } else { cl.projects.len() as f32 * sp::ROW_H };
    let want = sp::L + 34.0 + sp::S + rows_h + sp::S;
    let contrib_min = 120.0;
    let split = bottom - y2 - want - gap >= contrib_min && !cl.contrib.is_empty();
    let pr = Rect::from_min_max(Pos2::new(wr.right() + gap, y2), Pos2::new(x1, if split { y2 + want } else { bottom }));
    glass_card(&p, wr, R_CARD, 0.0, false);
    let wi = wr.shrink(sp::L);
    p.text(wi.left_top(), Align2::LEFT_TOP, "Last 7 days", ty::title(), TEXT);
    p.text(wi.left_top() + Vec2::new(0.0, 16.0 + sp::XS), Align2::LEFT_TOP, "Tokens per day", ty::caption(), ty::caption_color());
    let br = Rect::from_min_max(Pos2::new(wi.left(), wi.top() + 34.0 + sp::M), Pos2::new(wi.right(), wi.bottom() - 18.0));
    day_bars(&p, br, &cl.local.daily, 6.0);
    p.text(Pos2::new(wi.left(), wi.bottom()), Align2::LEFT_BOTTOM, "7 days ago", ty::caption(), ty::caption_color());
    p.text(Pos2::new(wi.right(), wi.bottom()), Align2::RIGHT_BOTTOM, "today", ty::caption(), ty::caption_color());

    glass_card(&p, pr, R_CARD, 0.0, false);
    let pi = pr.shrink(sp::L);
    if !is_claude {
        // no project list for other assistants (yet): say where the numbers come from
        p.text(pi.left_top(), Align2::LEFT_TOP, "About these numbers", ty::title(), TEXT);
        let lines = [
            cl.note.clone(),
            format!("Tokens are counted from the {} session logs on this Mac: fresh input plus output, without cached input.", cl.provider),
            "Projects and Cleanup work with Claude Code only.".to_string(),
        ];
        let mut ly = pi.top() + 16.0 + sp::M;
        for l in lines.iter().filter(|l| !l.is_empty()) {
            let g = p.layout(l.clone(), ty::body(), TEXT_SOFT, pi.width());
            let h = g.size().y;
            if ly + h > pi.bottom() {
                break;
            }
            p.galley(Pos2::new(pi.left(), ly), g, TEXT_SOFT);
            ly += h + sp::S;
        }
    } else {
    p.text(pi.left_top(), Align2::LEFT_TOP, "Claude Code projects", ty::title(), TEXT);
    let used = cl.projects.iter().filter(|p| p.in_use()).count();
    p.text(pi.left_top() + Vec2::new(0.0, 16.0 + sp::XS), Align2::LEFT_TOP, format!("{} on this Mac · {} in use, kept by Cleanup", cl.projects.len(), used), ty::caption(), ty::caption_color());
    let list = Rect::from_min_max(Pos2::new(pr.left(), pi.top() + 34.0 + sp::S), Pos2::new(pr.right(), pr.bottom() - sp::S));
    let mut child = ui.new_child(UiBuilder::new().max_rect(list).layout(egui::Layout::top_down(egui::Align::Min)));
    child.set_clip_rect(list);
    let home = dirs::home_dir().unwrap_or_default().display().to_string();
    ScrollArea::vertical().id_salt("claude-projects").auto_shrink([false, false]).show(&mut child, |ui| {
        ui.spacing_mut().item_spacing.y = 0.0;
        if cl.projects.is_empty() {
            let (row, _) = ui.allocate_exact_size(Vec2::new(list.width(), 64.0), Sense::hover());
            let pp = ui.painter();
            let msg = if demo { "No Claude Code projects on this Mac yet." } else { "Looking for Claude Code projects…" };
            pp.text(Pos2::new(row.left() + sp::L, row.top() + sp::M), Align2::LEFT_TOP, msg, ty::body(), TEXT_SOFT);
            text_fit(pp, Pos2::new(row.left() + sp::L, row.top() + sp::M + 20.0), Align2::LEFT_TOP, "Each folder where you run Claude Code shows up here with its size and last use.", ty::label(), ty::caption_color(), row.width() - 2.0 * sp::L);
        }
        for (k, pj) in cl.projects.iter().enumerate() {
            let (row, _) = ui.allocate_exact_size(Vec2::new(list.width(), sp::ROW_H), Sense::hover());
            let pp = ui.painter();
            if k > 0 {
                pp.line_segment([Pos2::new(row.left() + sp::L, row.top()), Pos2::new(row.right() - sp::L, row.top())], egui::Stroke::new(1.0, white(15)));
            }
            let (dot, label) = if pj.in_use() {
                (GREEN, "in use")
            } else if !pj.cwd_exists {
                (AMBER, "folder gone")
            } else {
                (white(51), "idle")
            };
            pp.circle_filled(Pos2::new(row.left() + sp::L + 4.0, row.center().y), 4.0, dot);
            let x = row.left() + sp::L + 8.0 + sp::M;
            let sr = pp.text(Pos2::new(row.right() - sp::L, row.center().y), Align2::RIGHT_CENTER, human_bytes(pj.bytes), ty::body(), TEXT);
            let idle = pj.days_idle();
            let when = if idle < 1.0 { "today".to_string() } else if idle < 2.0 { "yesterday".to_string() } else { format!("{} days ago", idle as u64) };
            let path = pj.cwd.as_ref().map(|c| c.display().to_string().replacen(&home, "~", 1)).unwrap_or_else(|| "unknown folder".into());
            let nw = sr.left() - x - sp::L;
            text_fit(pp, Pos2::new(x, row.center().y - 1.0), Align2::LEFT_BOTTOM, &middle_ellipsis(&path, 70), mono(12.0), TEXT, nw);
            text_fit(pp, Pos2::new(x, row.center().y + 1.0), Align2::LEFT_TOP, &format!("{label} · {when}"), ty::caption(), ty::caption_color(), nw);
        }
    });
    }
    if split {
        let ur = Rect::from_min_max(Pos2::new(pr.left(), pr.bottom() + gap), Pos2::new(x1, bottom));
        glass_card(&p, ur, R_CARD, 0.0, false);
        let ui_ = ur.shrink(sp::L);
        p.text(ui_.left_top(), Align2::LEFT_TOP, "What uses your limits", ty::title(), TEXT);
        let c = &cl.contrib[0];
        p.text(ui_.left_top() + Vec2::new(0.0, 16.0 + sp::XS), Align2::LEFT_TOP, &c.title, ty::caption(), ty::caption_color());
        let mut ly = ui_.top() + 34.0 + sp::M;
        for l in &c.lines {
            let g = p.layout(l.trim().to_string(), ty::body(), TEXT_SOFT, ui_.width());
            if ly + g.size().y > ui_.bottom() {
                break;
            }
            let h = g.size().y;
            p.galley(Pos2::new(ui_.left(), ly), g, TEXT_SOFT);
            ly += h + sp::S;
        }
    }

    // footer
    let f = page_footer(&p, r, 12.0);
    if !is_claude {
        text_fit(&p, Pos2::new(f.left() + 24.0, f.center().y), Align2::LEFT_CENTER, &format!("{} limits update with each request you make in it.", cl.provider), font(13.0), TEXT_DIM, f.width() - 48.0);
        return;
    }
    let bl = "Review in Cleanup";
    let bw = button_width(&p, bl, BTN_PRIMARY);
    let br = Rect::from_min_size(Pos2::new(f.right() - 24.0 - bw, f.center().y - 16.0), Vec2::new(bw, 32.0));
    text_fit(&p, Pos2::new(f.left() + 24.0, f.center().y), Align2::LEFT_CENTER, "Cleanup finds old Claude sessions, caches and logs and never touches projects in use.", font(13.0), TEXT_DIM, br.left() - f.left() - 40.0);
    if pill_button(ui, br, bl, BTN_PRIMARY, true).clicked() {
        cx.open_page(CLEANUP, "claude");
    }
}
