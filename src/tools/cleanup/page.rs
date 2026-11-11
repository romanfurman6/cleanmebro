//! Cleanup screens: start → scanning → Claude reviewing paths → results (with
//! "needs your OK" questions) → cleaning → done.

use crate::kit::sync::LockExt;
use super::engine::{self as cleanup, Group, Item, Phase, Verdict, Who};
use super::Cleanup as App;
use crate::core::tool::{Action, Cx};
use crate::kit::claude_code::{self as cc, Availability};
use crate::kit::glass::{self, *};
use egui::{Align2, Color32, CornerRadius, Pos2, Rect, ScrollArea, Sense, Stroke, StrokeKind, Ui, UiBuilder, Vec2};
use std::path::PathBuf;

/// Window corner radius (the footer rounds its bottom-right corner to match).
const WIN_R: f32 = 12.0;

pub fn page(app: &mut App, ui: &mut Ui, r: Rect, cx: &mut Cx) {
    let phase = app.clean.lock_ok().phase.clone();
    match phase {
        Phase::Intro => intro(app, ui, r, cx),
        Phase::Scanning | Phase::Judging | Phase::Cleaning => working(app, ui, r, phase),
        Phase::Results => results(app, ui, r),
        Phase::Done => done(app, ui, r),
    }
}

/// Whether Claude can judge paths now (always in the demo).
fn claude_now(app: &App) -> Availability {
    if app.demo {
        app.demo_claude.unwrap_or(Availability::Ready)
    } else {
        cc::availability()
    }
}

/// A calm note in place of the Claude switch: what's missing, how to turn it
/// on, and that the built-in rules are used meanwhile. Returns its height.
fn claude_note_height(p: &egui::Painter, width: f32, a: Availability) -> f32 {
    let text_h = p.layout(a.how().to_string(), font(12.0), TEXT_DIM(), width - 54.0).size().y;
    32.0 + text_h + 12.0 + 26.0 + 14.0
}

fn claude_note(ui: &mut Ui, pos: Pos2, width: f32, a: Availability, cx: &mut Cx) -> f32 {
    let p = ui.painter().clone();
    let x = pos.x + 40.0;
    let w = width - 54.0;
    let body = a.how().to_string();
    let r = Rect::from_min_size(pos, Vec2::new(width, claude_note_height(&p, width, a)));
    glass_card(&p, r, R_CARD, 0.0, false);
    icon(&p, Icon::Gauge, Rect::from_min_size(r.min + Vec2::new(14.0, 14.0), Vec2::splat(16.0)), TEXT_FAINT());
    text_fit(&p, Pos2::new(x, r.top() + 13.0), Align2::LEFT_TOP, a.title(), bold(13.0), TEXT(), w);
    paragraph(&p, Pos2::new(x, r.top() + 32.0), &body, font(12.0), TEXT_DIM(), w);
    // buttons, bottom left
    let by = r.bottom() - 12.0 - 26.0;
    let mut bx = x;
    let mut btn = |ui: &mut Ui, label: &str, kind: u8| {
        let bw = button_width(&p, label, kind);
        let br = Rect::from_min_size(Pos2::new(bx, by), Vec2::new(bw, 26.0));
        bx += bw + 8.0;
        pill_button(ui, br, label, kind, true).clicked()
    };
    match a {
        Availability::Off => {
            if btn(ui, "Open Settings", BTN_SECONDARY) {
                cx.push(Action::OpenSettings { tools: true });
            }
        }
        Availability::NotInstalled => {
            if btn(ui, "Get Claude Code", BTN_SECONDARY) {
                cc::open_download_page();
            }
            if btn(ui, "Check again", BTN_SECONDARY) {
                cc::request_recheck();
                cx.toast("Looking for Claude Code…");
            }
        }
        Availability::SignedOut => {
            if btn(ui, "Open Terminal", BTN_SECONDARY) {
                cc::open_terminal();
            }
            if btn(ui, "Check again", BTN_SECONDARY) {
                cc::request_recheck();
                cx.toast("Checking your Claude sign-in…");
            }
        }
        Availability::Ready => {}
    }
    r.height()
}

fn home_str() -> String {
    dirs::home_dir().unwrap_or_default().display().to_string()
}

fn short_path(p: &std::path::Path) -> String {
    p.display().to_string().replacen(&home_str(), "~", 1)
}

// ---------------------------------------------------------------- start

fn intro(app: &mut App, ui: &mut Ui, r: Rect, tcx: &mut Cx) {
    let ctx = ui.ctx().clone();
    let avail = claude_now(app);
    let has_claude = avail.ready();
    let p = ui.painter().clone();
    page_header(&p, r, "Cleanup", "", TEXT_DIM());
    let body = Rect::from_min_max(Pos2::new(r.left() + 20.0, r.top() + PAGE_HEADER_H + 1.0 + sp::L), Pos2::new(r.right() - 20.0, r.bottom() - sp::L));

    // top: one toolbar with the disk, the AI switch and Scan; below it the
    // groups as columns sized by what the last scan found in each
    let (total, free) = crate::kit::fs::volume_space("/");
    let used = if total > 0 { 1.0 - free as f32 / total as f32 } else { 0.0 };
    let bar_card = Rect::from_min_size(body.min, Vec2::new(body.width(), 56.0));
    glass_card(&p, bar_card, R_CARD, 0.0, false);
    let di = bar_card.shrink2(Vec2::new(sp::L, 0.0));
    let cy = di.center().y;
    // right to left: Scan, the switch, a divider, the free space
    let scan_w = button_width(&p, "Scan", BTN_PRIMARY).max(96.0);
    let scan_r = Rect::from_min_size(Pos2::new(di.right() + 4.0 - scan_w, cy - 17.0), Vec2::new(scan_w, 34.0));
    if pill_button(ui, scan_r, "Scan", BTN_PRIMARY, true).clicked() {
        cleanup::start_scan(app.clean.clone(), ctx.clone(), app.use_ai && has_claude);
    }
    let mut right = scan_r.left() - sp::L;
    if has_claude {
        let label = "Check with AI";
        let lr = p.text(Pos2::new(right, cy), Align2::RIGHT_CENTER, label, ty::body(), TEXT());
        let sw = Rect::from_min_size(Pos2::new(lr.left() - sp::S - 30.0, cy - 9.0), Vec2::new(30.0, 18.0));
        if switch(ui, sw, ui.scope_id().with("use-ai"), app.use_ai, true).clicked() {
            app.use_ai = !app.use_ai;
        }
        right = sw.left() - sp::L;
    }
    p.line_segment([Pos2::new(right, cy - 14.0), Pos2::new(right, cy + 14.0)], Stroke::new(1.0, CARD_EDGE()));
    right -= sp::L;
    icon(&p, Icon::Disk, Rect::from_center_size(Pos2::new(di.left() + 9.0, cy), Vec2::splat(18.0)), TEXT_SOFT());
    let name = p.text(Pos2::new(di.left() + 18.0 + sp::M, cy), Align2::LEFT_CENTER, "Macintosh HD", ty::title(), TEXT());
    let free_s = if total > 0 { format!("{} free of {}", human_bytes(free), human_bytes(total)) } else { String::new() };
    let fr = p.text(Pos2::new(right, cy), Align2::RIGHT_CENTER, &free_s, ty::body(), TEXT_SOFT());
    let bar_r = Rect::from_min_max(Pos2::new(name.right() + sp::L, cy - 3.0), Pos2::new(fr.left() - sp::L, cy + 3.0));
    if bar_r.width() > 40.0 {
        bar(&p, bar_r, used, BLUE());
    }

    // a note when Claude is missing, at the bottom
    let mut chart_bottom = body.bottom();
    if !has_claude {
        let h = claude_note_height(&p, body.width(), avail);
        claude_note(ui, Pos2::new(body.left(), body.bottom() - h), body.width(), avail, tcx);
        chart_bottom -= h + sp::M;
    }

    // the chart card
    let card = Rect::from_min_max(Pos2::new(body.left(), bar_card.bottom() + sp::M), Pos2::new(body.right(), chart_bottom));
    glass_card(&p, card, R_CARD, 0.0, false);
    let ci = card.shrink(sp::XL);
    let last = if app.demo { Some(super::engine::LastScan { when: chrono::Utc::now().timestamp() - 2 * 86400, groups: vec![14_200_000_000, 9_800_000_000, 21_600_000_000, 1_300_000_000, 3_400_000_000] }) } else { super::engine::last_scan() };
    let hist = super::engine::history();
    let sizes: Vec<u64> = Group::ALL.iter().enumerate().map(|(i, _)| last.as_ref().and_then(|l| l.groups.get(i).copied()).unwrap_or(0)).collect();
    let found: u64 = sizes.iter().sum();
    let mut top = ci.top();
    if let Some(l) = &last {
        p.text(Pos2::new(ci.left(), top), Align2::LEFT_TOP, format!("Last scan {}", super::engine::days_ago(l.when)), ty::label(), TEXT_DIM());
        p.text(Pos2::new(ci.left(), top + 20.0), Align2::LEFT_TOP, human_bytes(found), ty::hero(), TEXT());
        top += 20.0 + 34.0;
    }
    if !hist.is_empty() {
        let freed: u64 = hist.iter().map(|h| h.1).sum();
        p.text(Pos2::new(ci.right(), ci.top()), Align2::RIGHT_TOP, format!("{} freed in {} clean{}", human_bytes(freed), hist.len(), if hist.len() == 1 { "" } else { "s" }), ty::label(), TEXT_DIM());
        top = top.max(ci.top() + 20.0);
    }
    if top > ci.top() {
        top += sp::L;
    }

    // columns, biggest first; the colour follows the rank
    let mut order: Vec<usize> = (0..Group::ALL.len()).collect();
    order.sort_by_key(|&i| std::cmp::Reverse(sizes[i]));
    let max = sizes.iter().copied().max().unwrap_or(0).max(1) as f32;
    let label_h = 18.0;
    let base = ci.bottom() - label_h - sp::M;
    let col_top = top + 22.0;
    let n = order.len() as f32;
    let gap = sp::L;
    let cw = (ci.width() - gap * (n - 1.0)) / n;
    let full = (base - col_top).max(20.0);
    for (k, &i) in order.iter().enumerate() {
        let g = Group::ALL[i];
        let x = ci.left() + k as f32 * (cw + gap);
        let col = junk_ramp(k);
        let track = Rect::from_min_max(Pos2::new(x, col_top), Pos2::new(x + cw, base));
        p.rect_filled(track, CornerRadius { nw: 10, ne: 10, sw: 0, se: 0 }, with_alpha(col, 0.06));
        if sizes[i] > 0 {
            let h = (full * sizes[i] as f32 / max).max(8.0);
            let fill = Rect::from_min_max(Pos2::new(x, base - h), Pos2::new(x + cw, base));
            p.rect_filled(fill, CornerRadius { nw: 10, ne: 10, sw: 0, se: 0 }, col);
            p.text(Pos2::new(fill.center().x, fill.top() - 6.0), Align2::CENTER_BOTTOM, human_bytes(sizes[i]), ty::title(), TEXT());
        }
        // icon + name under the baseline
        let ny = ci.bottom() - label_h / 2.0;
        let nw = p.layout_no_wrap(g.title().to_string(), ty::body(), TEXT()).size().x.min(cw - 24.0);
        let lx = x + (cw - nw - 22.0) / 2.0;
        let ir = Rect::from_center_size(Pos2::new(lx + 8.0, ny), Vec2::splat(16.0));
        let drawn = matches!(g, Group::Developer) && symbol(&p, "hammer", ir, col);
        if !drawn {
            let ic = match g {
                Group::AiTools => Icon::Gauge,
                Group::AppCaches => Icon::Folder,
                Group::Developer => Icon::Gear,
                Group::Logs => Icon::Doc,
                Group::Trash => Icon::Trash,
            };
            icon(&p, ic, ir, col);
        }
        text_fit(&p, Pos2::new(lx + 22.0, ny), Align2::LEFT_CENTER, g.title(), ty::body(), TEXT_SOFT(), cw - 24.0);
    }
    p.line_segment([Pos2::new(ci.left(), base), Pos2::new(ci.right(), base)], Stroke::new(1.0, CARD_EDGE()));
}

// ---------------------------------------------------------------- scanning / judging / cleaning

fn working(app: &mut App, ui: &mut Ui, r: Rect, phase: Phase) {
    let ctx = ui.ctx().clone();
    let st = app.clean.lock_ok().clone();
    let p = ui.painter().clone();
    let t = ctx.input(|i| i.time);
    let sub = match phase {
        Phase::Judging => "Step 2 of 2 · Checking with AI",
        Phase::Cleaning => "Cleaning…",
        _ if !st.with_ai => "Looking for junk",
        _ => "Step 1 of 2 · Looking for junk",
    };
    let btns = page_header(&p, r, "Cleanup", sub, TEXT_DIM());
    if phase != Phase::Cleaning {
        let w = button_width(&p, "Stop", BTN_SECONDARY).max(64.0);
        let sr = Rect::from_min_max(Pos2::new(btns.right() - w, btns.top()), btns.max);
        if pill_button(ui, sr, "Stop", BTN_SECONDARY, true).clicked() {
            app.clean.lock_ok().stop = true;
        }
    }

    let x0 = r.left() + 24.0;
    let x1 = r.right() - 24.0;
    let mut y = r.top() + PAGE_HEADER_H + 20.0;
    // headline number
    let (big, caption) = if phase == Phase::Cleaning {
        (human_bytes(st.cleaned_bytes), format!("cleaned so far · {} items", st.cleaned_count))
    } else {
        (human_bytes(st.items.iter().filter(|i| i.verdict != Verdict::Keep).map(|i| i.bytes).sum()), format!("found so far · {} items", st.items.len()))
    };
    let g = p.layout_no_wrap(big, bold(28.0), TEXT());
    let gw = g.size().x;
    p.galley(Pos2::new(x0, y), g, TEXT());
    p.text(Pos2::new(x0 + gw + 10.0, y + 22.0), Align2::LEFT_BOTTOM, caption, font(13.0), TEXT_DIM());
    if st.progress >= 0.0 {
        p.text(Pos2::new(x1, y + 22.0), Align2::RIGHT_BOTTOM, format!("{:.0}%", st.progress.clamp(0.0, 1.0) * 100.0), font(13.0), TEXT_DIM());
    }
    y += 40.0;
    // progress bar (an indeterminate sweep while the scan can't count)
    let track = Rect::from_min_size(Pos2::new(x0, y), Vec2::new(x1 - x0, 6.0));
    if st.progress >= 0.0 {
        bar(&p, track, st.progress, BLUE());
    } else {
        p.rect_filled(track, 3.0, white(20));
        let w = track.width() * 0.25;
        let f = ((t * 0.6).fract() as f32) * (track.width() + w) - w;
        let seg = Rect::from_min_size(Pos2::new(track.left() + f, track.top()), Vec2::new(w, 6.0)).intersect(track);
        if seg.width() > 0.0 {
            p.rect_filled(seg, 3.0, BLUE());
        }
    }
    y += 16.0;
    // what it's doing right now
    match phase {
        Phase::Judging => {
            let line = if st.ai_status.is_empty() { "Asking Claude…".to_string() } else { st.ai_status.clone() };
            text_fit(&p, Pos2::new(x0, y + 7.0), Align2::LEFT_CENTER, &line, font(12.0), TEXT_FAINT(), x1 - x0);
        }
        _ => {
            let line = st.current.replacen(&home_str(), "~", 1);
            text_fit(&p, Pos2::new(x0, y + 7.0), Align2::LEFT_CENTER, &middle_ellipsis(&line, 110), mono(11.0), TEXT_FAINT(), x1 - x0);
        }
    }
    y += 34.0;

    // per-group table
    let cols = [x0 + 12.0, x0 + 48.0, x1 - 250.0, x1 - 12.0];
    section_header(&p, Pos2::new(cols[1], y), Align2::LEFT_TOP, "Group");
    section_header(&p, Pos2::new(cols[2], y), Align2::LEFT_TOP, "Items");
    section_header(&p, Pos2::new(cols[3], y), Align2::RIGHT_TOP, if phase == Phase::Cleaning { "Selected" } else { "Found" });
    y += 22.0;
    for grp in Group::ALL {
        let row = Rect::from_min_max(Pos2::new(x0, y), Pos2::new(x1, y + 38.0));
        if row.bottom() > r.bottom() - 8.0 {
            break;
        }
        let items: Vec<&Item> = st.items.iter().filter(|i| i.group == grp).collect();
        let n = items.len();
        let bytes: u64 = items.iter().filter(|i| i.verdict != Verdict::Keep).map(|i| i.bytes).sum();
        let mark = Pos2::new(cols[0] + 8.0, row.center().y);
        if n > 0 {
            p.circle_filled(mark, 8.0, tint::HEALTH());
            icon(&p, Icon::Check, Rect::from_center_size(mark, Vec2::splat(11.0)), ON_ACTION);
        } else {
            crate::kit::glass::stroke_circle(&p, mark, 7.5, Stroke::new(1.5, white(64)));
        }
        p.text(Pos2::new(cols[1], row.center().y), Align2::LEFT_CENTER, grp.title(), font(13.0), TEXT());
        let (status, sc) = if n > 0 { (format!("{n} found"), GREEN()) } else { ("—".to_string(), TEXT_FAINT()) };
        p.text(Pos2::new(cols[2], row.center().y), Align2::LEFT_CENTER, status, font(12.0), sc);
        p.text(Pos2::new(cols[3], row.center().y), Align2::RIGHT_CENTER, if n > 0 { human_bytes(bytes) } else { "—".into() }, font(13.0), if n > 0 { TEXT() } else { TEXT_FAINT() });
        y += 40.0;
    }
    ctx.request_repaint();
}

// ---------------------------------------------------------------- results

fn results(app: &mut App, ui: &mut Ui, r: Rect) {
    let ctx = ui.ctx().clone();
    let p = ui.painter().clone();
    let (total, pending, ai_used, ai_error, selected) = {
        let g = app.clean.lock_ok();
        (g.total(), g.pending_questions(), g.ai_used, g.ai_error.clone(), g.selected_bytes())
    };
    let avail = claude_now(app);
    let mut sub = format!("{} found", human_bytes(total));
    if let Some(e) = &ai_error {
        sub = format!("{sub} · {e}");
    }
    let _ = ai_used;
    let btns = page_header(&p, r, "Cleanup", &sub, if ai_error.is_some() { AMBER() } else { TEXT_DIM() });
    let w = button_width(&p, "Scan again", BTN_SECONDARY);
    let sr = Rect::from_min_max(Pos2::new(btns.right() - w, btns.top()), btns.max);
    if pill_button(ui, sr, "Scan again", BTN_SECONDARY, true).clicked() {
        cleanup::start_scan(app.clean.clone(), ctx.clone(), app.use_ai && avail.ready());
        return;
    }
    let hidden = app.clean.lock_ok().hidden_kept;
    if hidden > 0 {
        let label = format!("Show {hidden} kept");
        let kw = button_width(&p, &label, BTN_SECONDARY);
        let kr = Rect::from_min_max(Pos2::new(sr.left() - 8.0 - kw, btns.top()), Pos2::new(sr.left() - 8.0, btns.bottom()));
        if pill_button(ui, kr, &label, BTN_SECONDARY, true).on_hover_text("You chose to keep these before, so they are hidden. Show them again and scan.").clicked() {
            cleanup::forget_kept();
            cleanup::start_scan(app.clean.clone(), ctx.clone(), app.use_ai && avail.ready());
            return;
        }
    }

    if total == 0 && pending == 0 {
        let c = Pos2::new(r.center().x, r.top() + PAGE_HEADER_H + 150.0);
        p.circle_filled(c, 28.0, tint::HEALTH());
        icon(&p, Icon::Check, Rect::from_center_size(c, Vec2::splat(28.0)), ON_ACTION);
        p.text(c + Vec2::new(0.0, 56.0), Align2::CENTER_CENTER, "Nothing to clean", bold(22.0), TEXT());
        p.text(c + Vec2::new(0.0, 84.0), Align2::CENTER_CENTER, "No caches, logs or leftovers worth removing were found.", font(13.0), TEXT_DIM());
        let dr = Rect::from_center_size(c + Vec2::new(0.0, 130.0), Vec2::new(96.0, 32.0));
        if pill_button(ui, dr, "Done", BTN_PRIMARY, true).clicked() {
            app.clean.lock_ok().phase = Phase::Intro;
        }
        return;
    }

    let body = Rect::from_min_max(Pos2::new(r.left() + 20.0, r.top() + PAGE_HEADER_H + 1.0 + sp::L), Pos2::new(r.right() - 20.0, r.bottom() - PAGE_FOOTER_H - sp::L));

    // the groups as tabs: All, then each group that found something
    let mut tabs: Vec<(Option<Group>, String, u64, usize, Color32)> = vec![];
    {
        let g = app.clean.lock_ok();
        let asks = |f: &dyn Fn(&Item) -> bool| g.items.iter().filter(|i| f(i) && i.verdict == Verdict::Ask && i.by != Who::You).count();
        tabs.push((None, "All".into(), total, asks(&|_| true), TEXT_DIM()));
        for (k, grp) in Group::ALL.iter().enumerate() {
            if g.items.iter().any(|i| i.group == *grp) {
                tabs.push((Some(*grp), grp.title().into(), g.group_bytes(*grp), asks(&|i| i.group == *grp), junk_ramp(k)));
            }
        }
    }
    if !tabs.iter().any(|t| t.0 == app.clean_group) {
        app.clean_group = None;
    }
    let mut x = body.left();
    let tab_h = 30.0;
    for (grp, title, bytes, q, dot) in &tabs {
        let size = human_bytes(*bytes);
        let tw = p.layout_no_wrap(title.clone(), ty::body(), TEXT()).size().x;
        let sw = p.layout_no_wrap(size.clone(), ty::body(), TEXT()).size().x;
        let bw = if *q > 0 { (q.to_string().len() as f32 * 7.0 + 14.0).max(20.0) + sp::S } else { 0.0 };
        let w = 12.0 + 8.0 + sp::S + tw + sp::S + sw + bw + 12.0;
        let tr = Rect::from_min_size(Pos2::new(x, body.top()), Vec2::new(w, tab_h));
        let sel = app.clean_group == *grp;
        let id = ui.scope_id().with(("tab", title.as_str()));
        let resp = ui.interact(tr, id, Sense::click());
        let hv = ctx.animate_bool(id, resp.hovered());
        if sel {
            p.rect_filled(tr, R_BUTTON, with_alpha(BLUE(), 0.16));
            p.rect_stroke(tr, R_BUTTON, Stroke::new(1.0, with_alpha(BLUE(), 0.45)), StrokeKind::Inside);
        } else {
            p.rect_filled(tr, R_BUTTON, lerp_color(CARD(), RAISED(), hv));
            p.rect_stroke(tr, R_BUTTON, Stroke::new(1.0, CARD_EDGE()), StrokeKind::Inside);
        }
        let mut cx_ = tr.left() + 12.0;
        p.circle_filled(Pos2::new(cx_ + 4.0, tr.center().y), 4.0, *dot);
        cx_ += 8.0 + sp::S;
        let t = p.text(Pos2::new(cx_, tr.center().y), Align2::LEFT_CENTER, title, ty::body(), if sel { ACCENT() } else { TEXT() });
        let s = p.text(Pos2::new(t.right() + sp::S, tr.center().y), Align2::LEFT_CENTER, &size, ty::body(), TEXT_DIM());
        if *q > 0 {
            badge(&p, Pos2::new(s.right() + sp::S, tr.center().y), Align2::LEFT_CENTER, &q.to_string());
        }
        let resp = match grp {
            Some(g) => resp.on_hover_text(g.blurb()),
            None => resp,
        };
        if resp.clicked() {
            app.clean_group = *grp;
        }
        if resp.hovered() {
            ctx.set_cursor_icon(egui::CursorIcon::PointingHand);
        }
        x = tr.right() + sp::S;
    }

    // the chosen tab's items, split three ways
    let (asks, safe, kept): (Vec<usize>, Vec<usize>, Vec<usize>) = {
        let g = app.clean.lock_ok();
        let mut mine: Vec<usize> = g.items.iter().enumerate().filter(|(_, it)| app.clean_group.is_none_or(|gr| it.group == gr)).map(|(i, _)| i).collect();
        mine.sort_by_key(|&i| std::cmp::Reverse(g.items[i].bytes));
        let asks = mine.iter().copied().filter(|&i| g.items[i].verdict == Verdict::Ask && g.items[i].by != Who::You).collect();
        let safe = mine.iter().copied().filter(|&i| g.items[i].verdict == Verdict::Safe || (g.items[i].verdict == Verdict::Ask && g.items[i].by == Who::You)).collect();
        let kept = mine.iter().copied().filter(|&i| g.items[i].verdict == Verdict::Keep).collect();
        (asks, safe, kept)
    };
    let shared = app.clean.clone();
    let sum = |v: &[usize]| -> u64 {
        let g = shared.lock_ok();
        v.iter().map(|&i| g.items[i].bytes).sum()
    };
    let cols = Rect::from_min_max(Pos2::new(body.left(), body.top() + tab_h + sp::L), body.max);
    let (left, right) = if asks.is_empty() {
        (None, cols)
    } else if safe.is_empty() && kept.is_empty() {
        (Some(cols), Rect::NOTHING)
    } else {
        let lw = ((cols.width() - sp::L) * 0.56).round();
        (
            Some(Rect::from_min_size(cols.min, Vec2::new(lw, cols.height()))),
            Rect::from_min_max(Pos2::new(cols.left() + lw + sp::L, cols.top()), cols.max),
        )
    };
    const LABEL_H: f32 = 20.0;

    // left: questions, one card each
    if let Some(lc) = left {
        let lab = section_label(&p, lc.left_top(), "Needs your OK", AMBER(), asks.len(), sum(&asks));
        let mut answer: Option<Verdict> = None;
        let a = link(ui, Pos2::new(lc.right(), lab.center().y), Align2::RIGHT_CENTER, "Keep all", 12.0);
        if a.clicked() {
            answer = Some(Verdict::Keep);
        }
        if link(ui, Pos2::new(a.rect.left() - sp::M, lab.center().y), Align2::RIGHT_CENTER, "Allow all", 12.0).clicked() {
            answer = Some(Verdict::Safe);
        }
        if let Some(v) = answer {
            answer_items(app, &asks, v);
        }
        let area = Rect::from_min_max(Pos2::new(lc.left(), lc.top() + LABEL_H + sp::S), lc.max);
        scroll_rows(ui, area, format!("asks-{:?}", app.clean_group), asks.len(), 60.0, sp::S, |ui, k, rr| ask_card(app, ui, asks[k], rr));
    }

    // right: what will go (Safe) and what stays (Kept)
    if right.is_positive() {
        let row_h = 36.0;
        let need = |n: usize| if n == 0 { 0.0 } else { LABEL_H + sp::S + n as f32 * row_h + 8.0 };
        let (mut safe_h, mut kept_h) = (need(safe.len()), need(kept.len()));
        let gap = if safe_h > 0.0 && kept_h > 0.0 { sp::L } else { 0.0 };
        if safe_h + kept_h + gap > right.height() {
            kept_h = kept_h.min(if safe_h > 0.0 { right.height() * 0.4 } else { right.height() });
            safe_h = (right.height() - kept_h - gap).max(0.0);
        }
        let safe_r = Rect::from_min_size(right.min, Vec2::new(right.width(), safe_h));
        if !safe.is_empty() {
            let lab = section_label(&p, safe_r.left_top(), "Safe", ACCENT(), safe.len(), sum(&safe));
            let all_on = {
                let g = app.clean.lock_ok();
                safe.iter().all(|&i| g.items[i].selected || g.items[i].hard_keep)
            };
            if link(ui, Pos2::new(safe_r.right(), lab.center().y), Align2::RIGHT_CENTER, if all_on { "None" } else { "Select all" }, 12.0).clicked() {
                let mut g = app.clean.lock_ok();
                for &i in &safe {
                    let it = &mut g.items[i];
                    it.selected = !all_on && !it.hard_keep && !it.permanent;
                }
            }
            let card = Rect::from_min_max(Pos2::new(safe_r.left(), safe_r.top() + LABEL_H + sp::S), safe_r.max);
            glass_card(&p, card, R_CARD, 0.0, false);
            scroll_rows(ui, card.shrink2(Vec2::new(0.0, 4.0)), format!("safe-{:?}", app.clean_group), safe.len(), row_h, 0.0, |ui, k, rr| safe_row(app, ui, safe[k], rr));
        }
        if !kept.is_empty() {
            let kr = Rect::from_min_size(Pos2::new(right.left(), safe_r.bottom() + gap), Vec2::new(right.width(), kept_h));
            section_label(&p, kr.left_top(), "Kept", TEXT_DIM(), kept.len(), sum(&kept));
            let card = Rect::from_min_max(Pos2::new(kr.left(), kr.top() + LABEL_H + sp::S), kr.max);
            glass_card(&p, card, R_CARD, 0.0, false);
            scroll_rows(ui, card.shrink2(Vec2::new(0.0, 4.0)), format!("kept-{:?}", app.clean_group), kept.len(), row_h, 0.0, |ui, k, rr| kept_row(app, ui, kept[k], rr));
        }
    }

    // footer
    let f = page_footer(&p, r, WIN_R);
    let fy = f.center().y;
    // the segmented control already says where it goes; only warn when it can't come back
    let ty_ = if app.delete_now { fy - 9.0 } else { fy };
    p.text(Pos2::new(f.left() + 24.0, ty_), Align2::LEFT_CENTER, format!("{} selected", human_bytes(selected)), bold(13.0), TEXT());
    if app.delete_now {
        p.text(Pos2::new(f.left() + 24.0, fy + 10.0), Align2::LEFT_CENTER, "Can't be undone", font(12.0), AMBER());
    }
    let clean_label = format!("Clean {}", human_bytes(selected));
    let cw = button_width(&p, &clean_label, BTN_PRIMARY).max(120.0);
    let cr = Rect::from_min_size(Pos2::new(f.right() - 24.0 - cw, fy - 16.0), Vec2::new(cw, 32.0));
    if pill_button(ui, cr, &clean_label, BTN_PRIMARY, selected > 0).clicked() {
        app.clean_confirm = true;
    }
    let seg = Rect::from_min_size(Pos2::new(cr.left() - 16.0 - 200.0, fy - 14.0), Vec2::new(200.0, 28.0));
    if let Some(k) = segmented(ui, seg, ui.scope_id().with("removal"), &["Move to Trash", "Delete now"], app.delete_now as usize) {
        app.delete_now = k == 1;
    }
    if app.clean_confirm {
        confirm(app, ui, r);
    }
}

/// "Needs your OK  5 · 29.8 GB": a section title with its count and size.
fn section_label(p: &egui::Painter, at: Pos2, title: &str, col: Color32, n: usize, bytes: u64) -> Rect {
    let t = p.text(Pos2::new(at.x, at.y + 10.0), Align2::LEFT_CENTER, title, ty::title(), col);
    let s = p.text(Pos2::new(t.right() + sp::S, t.center().y), Align2::LEFT_CENTER, format!("{n} · {}", human_bytes(bytes)), ty::label(), TEXT_DIM());
    t.union(s)
}

/// A scrolling list of `n` rows of `row_h` in `area`; only the rows in view are drawn.
fn scroll_rows(ui: &mut Ui, area: Rect, salt: String, n: usize, row_h: f32, gap: f32, mut f: impl FnMut(&mut Ui, usize, Rect)) {
    if n == 0 || !area.is_positive() {
        return;
    }
    let mut child = ui.new_child(UiBuilder::new().max_rect(area).layout(egui::Layout::top_down(egui::Align::Min)));
    child.set_clip_rect(area.intersect(ui.clip_rect()));
    ScrollArea::vertical().id_salt(salt).auto_shrink([false, false]).show(&mut child, |ui| {
        ui.spacing_mut().item_spacing.y = gap;
        for k in 0..n {
            let (rect, _) = ui.allocate_exact_size(Vec2::new(area.width(), row_h), Sense::hover());
            if ui.is_rect_visible(rect) {
                f(ui, k, rect);
            }
        }
    });
}

/// Answer questions: Allow selects them, Keep remembers them as kept.
fn answer_items(app: &mut App, idx: &[usize], v: Verdict) {
    let mut kept = Vec::new();
    {
        let mut g = app.clean.lock_ok();
        for &i in idx {
            let it = &mut g.items[i];
            if v == Verdict::Keep {
                kept.push(it.path.clone());
            }
            it.verdict = v;
            it.by = Who::You;
            it.selected = v == Verdict::Safe;
            it.reason = if v == Verdict::Safe { "You allowed it".into() } else { "You chose to keep it".into() };
        }
    }
    if !kept.is_empty() {
        cleanup::remember_kept(kept);
    }
}

/// Hover: the reason; double click: show in Finder.
fn row_extras(ui: &mut Ui, rr: Rect, it: &Item) {
    let resp = ui.interact(rr, ui.scope_id().with(("row", &it.path)), Sense::click());
    if resp.hovered() {
        ui.painter().rect_filled(rr, R_SMALL, white(6));
    }
    let resp = resp.on_hover_text(format!("{}\n{}", short_path(&it.path), it.reason));
    if resp.double_clicked() {
        let _ = std::process::Command::new("open").arg("-R").arg(&it.path).spawn();
    }
}

/// One question: name and size, the path, Allow and Keep.
fn ask_card(app: &mut App, ui: &mut Ui, i: usize, rr: Rect) {
    let it = app.clean.lock_ok().items[i].clone();
    let p = ui.painter().clone();
    glass_card(&p, rr, R_CARD, 0.0, false);
    row_extras(ui, rr, &it);
    let kw = button_width(&p, "Keep", BTN_SECONDARY);
    let aw = button_width(&p, "Allow", BTN_PRIMARY);
    let kr = Rect::from_min_size(Pos2::new(rr.right() - sp::M - kw, rr.center().y - 14.0), Vec2::new(kw, 28.0));
    let ar = Rect::from_min_size(Pos2::new(kr.left() - sp::S - aw, kr.top()), Vec2::new(aw, 28.0));
    let tx = rr.left() + sp::L;
    let tw = ar.left() - sp::M - tx;
    let t = text_fit(&p, Pos2::new(tx, rr.top() + 12.0), Align2::LEFT_TOP, &it.title, ty::title(), TEXT(), tw - 70.0);
    let s = p.text(Pos2::new(t.right() + sp::S, t.top()), Align2::LEFT_TOP, human_bytes(it.bytes), ty::body(), TEXT_DIM());
    if it.permanent {
        chip(&p, Pos2::new(s.right() + sp::S, t.top() - 2.0), Align2::LEFT_TOP, "Permanent", CHIP_RISK());
    }
    text_fit(&p, Pos2::new(tx, rr.top() + 32.0), Align2::LEFT_TOP, &middle_ellipsis(&short_path(&it.path), 80), mono(11.0), TEXT_FAINT(), tw);
    if pill_button(ui, ar, "Allow", BTN_PRIMARY, true).clicked() {
        answer_items(app, &[i], Verdict::Safe);
    }
    if pill_button(ui, kr, "Keep", BTN_SECONDARY, true).clicked() {
        answer_items(app, &[i], Verdict::Keep);
    }
}

/// One safe item: checkbox, name, size.
fn safe_row(app: &mut App, ui: &mut Ui, i: usize, rr: Rect) {
    let it = app.clean.lock_ok().items[i].clone();
    let p = ui.painter().clone();
    let rr = rr.shrink2(Vec2::new(4.0, 0.0));
    row_extras(ui, rr, &it);
    let can = !it.hard_keep;
    let cb = Rect::from_center_size(Pos2::new(rr.left() + sp::M + 8.0, rr.center().y), Vec2::splat(16.0));
    if checkbox(ui, cb, ui.scope_id().with(("sel", &it.path)), it.selected, can).clicked() && can {
        let mut g = app.clean.lock_ok();
        g.items[i].selected = !g.items[i].selected;
    }
    let sz = p.text(Pos2::new(rr.right() - sp::M, rr.center().y), Align2::RIGHT_CENTER, human_bytes(it.bytes), ty::title(), TEXT());
    let mut right = sz.left() - sp::M;
    if it.permanent {
        right = chip(&p, Pos2::new(right, rr.center().y), Align2::RIGHT_CENTER, "Permanent", CHIP_RISK()).left() - sp::S;
    }
    let x = cb.right() + sp::M;
    text_fit(&p, Pos2::new(x, rr.center().y), Align2::LEFT_CENTER, &it.title, ty::body(), TEXT(), right - x);
}

/// One kept item: a lock, the name, the size, dimmed.
fn kept_row(app: &mut App, ui: &mut Ui, i: usize, rr: Rect) {
    let it = app.clean.lock_ok().items[i].clone();
    let p = ui.painter().clone();
    let rr = rr.shrink2(Vec2::new(4.0, 0.0));
    row_extras(ui, rr, &it);
    icon(&p, Icon::Lock, Rect::from_center_size(Pos2::new(rr.left() + sp::M + 8.0, rr.center().y), Vec2::splat(14.0)), TEXT_FAINT());
    let sz = p.text(Pos2::new(rr.right() - sp::M, rr.center().y), Align2::RIGHT_CENTER, human_bytes(it.bytes), ty::body(), TEXT_DIM());
    let x = rr.left() + sp::M + 16.0 + sp::M;
    text_fit(&p, Pos2::new(x, rr.center().y), Align2::LEFT_CENTER, &it.title, ty::body(), TEXT_DIM(), sz.left() - sp::M - x);
}

fn confirm(app: &mut App, ui: &mut Ui, r: Rect) {
    let ctx = ui.ctx().clone();
    let p = ui.painter().clone();
    // block clicks to the page underneath
    let _ = ui.interact(r, ui.scope_id().with("confirm-scrim"), Sense::click());
    p.rect_filled(r, CornerRadius { nw: 0, sw: 0, ne: WIN_R as u8, se: WIN_R as u8 }, Color32::from_black_alpha(115));
    let (n, bytes, perm, trashed) = {
        let g = app.clean.lock_ok();
        let sel: Vec<&Item> = g.items.iter().filter(|i| i.selected).collect();
        let trashed = sel.iter().filter(|i| !i.permanent && !i.rebuilds()).count();
        (sel.len(), sel.iter().map(|i| i.bytes).sum::<u64>(), sel.iter().any(|i| i.permanent), trashed)
    };
    let mut body = if app.delete_now {
        "They'll be deleted right away. This can't be undone.".to_string()
    } else if trashed == 0 {
        "Caches and logs are deleted; apps rebuild them.".to_string()
    } else if trashed + perm as usize == n {
        "They go to the Trash, so you can put anything back.".to_string()
    } else {
        format!("Caches and logs are deleted; apps rebuild them. The other {trashed} go to the Trash.")
    };
    if perm {
        body.push_str(" Emptying the Trash is permanent.");
    }
    body.push_str(" Quit the apps involved first for the best result.");
    let bw = 440.0 - 44.0;
    let body_h = p.layout(body.clone(), font(13.0), TEXT_SOFT(), bw).size().y;
    let m = Rect::from_center_size(r.center(), Vec2::new(440.0, 22.0 + 22.0 + 14.0 + body_h + 18.0 + 30.0 + 22.0));
    let shadow = egui::epaint::Shadow { offset: [0, 20], blur: 60, spread: 0, color: Color32::from_black_alpha(128) };
    p.add(shadow.as_shape(m, CornerRadius::same(14)));
    p.rect_filled(m, 14.0, RAISED());
    p.rect_stroke(m, 14.0, Stroke::new(1.0, white(31)), StrokeKind::Inside);
    p.text(m.min + Vec2::new(22.0, 22.0), Align2::LEFT_TOP, format!("Clean {} items ({})?", n, human_bytes(bytes)), bold(17.0), TEXT());
    paragraph(&p, m.min + Vec2::new(22.0, 58.0), &body, font(13.0), TEXT_SOFT(), bw);
    let danger = app.delete_now || perm;
    let ok_label = if app.delete_now { "Delete now" } else { "Move to Trash" };
    let okw = button_width(&p, ok_label, BTN_PRIMARY);
    let ok = Rect::from_min_size(Pos2::new(m.right() - 22.0 - okw, m.bottom() - 22.0 - 30.0), Vec2::new(okw, 30.0));
    let cw = button_width(&p, "Cancel", BTN_SECONDARY);
    let cancel = Rect::from_min_size(Pos2::new(ok.left() - 8.0 - cw, ok.top()), Vec2::new(cw, 30.0));
    if pill_button(ui, cancel, "Cancel", BTN_SECONDARY, true).clicked() {
        app.clean_confirm = false;
    }
    if pill_button(ui, ok, ok_label, if danger { BTN_DANGER } else { BTN_PRIMARY }, true).clicked() {
        app.clean_confirm = false;
        cleanup::start_clean(app.clean.clone(), ctx, app.delete_now);
    }
}

// ---------------------------------------------------------------- done

fn done(app: &mut App, ui: &mut Ui, r: Rect) {
    let p = ui.painter().clone();
    let st = app.clean.lock_ok().clone();
    page_header(&p, r, "Cleanup", "Finished", TEXT_DIM());
    let ok = st.failed.is_empty();
    // removed items leave the list; the engine keeps what each one was
    let mut rows: Vec<(Group, usize, u64)> = Group::ALL
        .iter()
        .map(|g| {
            let it: Vec<u64> = st.cleaned.iter().filter(|c| c.0 == *g).map(|c| c.1).collect();
            (*g, it.len(), it.iter().sum())
        })
        .filter(|r| r.1 > 0)
        .collect();
    rows.sort_by(|a, b| b.2.cmp(&a.2));
    // cards fit their content (no stretched empty boxes); the log gets the full height
    let body = Rect::from_min_max(Pos2::new(r.left() + 20.0, r.top() + PAGE_HEADER_H + 1.0 + sp::L), Pos2::new(r.right() - 20.0, r.bottom() - sp::L));
    let left_h = 2.0 * sp::L + 44.0 + sp::L + 34.0 + sp::XS + 16.0 + sp::XL + 34.0 + if ok { 0.0 } else { 16.0 + sp::S + st.failed.len().min(4) as f32 * 18.0 + 20.0 + sp::L };
    let right_h = 2.0 * sp::L + 34.0 + sp::S + rows.len().max(1) as f32 * sp::ROW_H;
    let h = if app.clean_log { body.height() } else { left_h.max(right_h).min(body.height()) };
    let left = Rect::from_min_size(body.min, Vec2::new(340.0f32.min(body.width() * 0.4), h));
    let right = Rect::from_min_size(Pos2::new(left.right() + sp::L, body.top()), Vec2::new(body.right() - left.right() - sp::L, h));
    glass_card(&p, left, R_CARD, 0.0, false);
    glass_card(&p, right, R_CARD, 0.0, false);

    // left: the result
    let li = left.shrink(sp::L);
    let c = Pos2::new(li.left() + 22.0, li.top() + 22.0);
    let col = if ok { GREEN() } else { AMBER() };
    crate::kit::tile::ring(&p, c, 19.0, 4.0, 1.0, col);
    icon(&p, if ok { Icon::Check } else { Icon::Info }, Rect::from_center_size(c, Vec2::splat(18.0)), col);
    let tx = c.x + 22.0 + sp::M;
    let total = st.cleaned_count + st.failed.len();
    p.text(Pos2::new(tx, c.y - 1.0), Align2::LEFT_BOTTOM, "Cleanup finished", ty::title(), TEXT());
    p.text(Pos2::new(tx, c.y + 3.0), Align2::LEFT_TOP, format!("{} of {} items", st.cleaned_count, total), ty::caption(), ty::caption_color());
    let mut y = li.top() + 44.0 + sp::L;
    let freed = st.cleaned_bytes.saturating_sub(st.trashed_bytes);
    let g = p.layout_no_wrap(human_bytes(if freed > 0 { freed } else { st.cleaned_bytes }), ty::hero(), TEXT());
    let gw = g.size().x;
    p.galley(Pos2::new(li.left(), y), g, TEXT());
    let what = if st.trashed_bytes == 0 {
        "freed".to_string()
    } else if freed == 0 {
        "to the Trash".to_string()
    } else {
        format!("freed · {} to the Trash", human_bytes(st.trashed_bytes))
    };
    p.text(Pos2::new(li.left() + gw + 6.0, y + 26.0), Align2::LEFT_BOTTOM, what, ty::body(), ty::caption_color());
    y += 34.0 + sp::XS;
    y += sp::L;
    if !ok {
        let n = st.failed.len();
        p.text(Pos2::new(li.left(), y), Align2::LEFT_TOP, format!("{n} item{} couldn't be removed", if n == 1 { "" } else { "s" }), ty::title(), AMBER());
        y += 16.0 + sp::S;
        for (name, err) in st.failed.iter().take(4) {
            text_fit(&p, Pos2::new(li.left(), y), Align2::LEFT_TOP, &format!("{name}: {err}"), mono(11.0), TEXT_SOFT(), li.width());
            y += 18.0;
        }
        text_fit(&p, Pos2::new(li.left(), y + sp::XS), Align2::LEFT_TOP, "Quit the apps and scan again", ty::caption(), ty::caption_color(), li.width());
    }
    let db = Rect::from_min_size(Pos2::new(li.left(), li.bottom() - 34.0), Vec2::new(li.width(), 34.0));
    if pill_button(ui, db, "Done", BTN_PRIMARY, true).clicked() {
        app.clean.lock_ok().phase = Phase::Intro;
        app.clean_log = false;
    }

    // right: what was cleaned, or the log
    let ri = right.shrink(sp::L);
    let log_label = if app.clean_log { "Hide log" } else { "Show log" };
    let lw = button_width(&p, log_label, BTN_SECONDARY);
    let lb = Rect::from_min_size(Pos2::new(ri.right() - lw, ri.top() - 4.0), Vec2::new(lw, 26.0));
    if pill_button(ui, lb, log_label, BTN_SECONDARY, true).clicked() {
        app.clean_log = !app.clean_log;
    }
    let list = Rect::from_min_max(Pos2::new(ri.left(), ri.top() + 16.0 + sp::M), ri.max);
    if app.clean_log {
        p.text(ri.left_top(), Align2::LEFT_TOP, "Cleanup log", ty::title(), TEXT());
        let mut child = ui.new_child(UiBuilder::new().max_rect(list).layout(egui::Layout::top_down(egui::Align::Min)));
        child.set_clip_rect(list);
        ScrollArea::vertical().id_salt("log").auto_shrink([false, false]).show(&mut child, |ui| {
            for l in &st.log {
                let col = if l.starts_with("Couldn't") { AMBER() } else { TEXT_DIM() };
                ui.label(egui::RichText::new(l.replacen(&home_str(), "~", 1)).font(glass::mono(11.0)).color(col));
            }
        });
        return;
    }
    p.text(ri.left_top(), Align2::LEFT_TOP, "What was cleaned", ty::title(), TEXT());
    for (k, (g, n, b)) in rows.iter().enumerate() {
        let row = Rect::from_min_size(Pos2::new(list.left(), list.top() + k as f32 * sp::ROW_H), Vec2::new(list.width(), sp::ROW_H));
        if row.bottom() > list.bottom() {
            break;
        }
        if k > 0 {
            p.line_segment([row.left_top(), row.right_top()], Stroke::new(1.0, white(15)));
        }
        icon(&p, Icon::Check, Rect::from_center_size(Pos2::new(row.left() + 7.0, row.center().y), Vec2::splat(14.0)), GREEN());
        let vr = p.text(Pos2::new(row.right(), row.center().y), Align2::RIGHT_CENTER, human_bytes(*b), ty::body(), white(166));
        let x = row.left() + 14.0 + sp::M;
        text_fit(&p, Pos2::new(x, row.center().y - 1.0), Align2::LEFT_BOTTOM, g.title(), ty::body(), TEXT(), vr.left() - x - sp::S);
        text_fit(&p, Pos2::new(x, row.center().y + 1.0), Align2::LEFT_TOP, &format!("{n} item{}", if *n == 1 { "" } else { "s" }), ty::caption(), ty::caption_color(), vr.left() - x - sp::S);
    }
    if rows.is_empty() {
        text_fit(&p, Pos2::new(list.left(), list.top() + sp::M), Align2::LEFT_TOP, "Nothing selected", ty::body(), TEXT_DIM(), list.width());
    }
}

/// Example results for screenshots (no personal paths).
pub fn demo_results() -> cleanup::State {
    let mk = |group, title: &str, path: &str, bytes: u64, kind, verdict, reason: &str, by| Item {
        group,
        title: title.into(),
        path: PathBuf::from(path),
        bytes,
        age_days: 12.0,
        kind,
        verdict,
        reason: reason.into(),
        rule_reason: reason.into(),
        rule_verdict: verdict,
        by,
        selected: verdict == Verdict::Safe,
        hard_keep: false,
        permanent: false,
        app_running: false,
    };
    let items = vec![
        mk(Group::AiTools, "Claude Desktop cache", "~/Library/Application Support/Claude/Cache", 678_000_000, "Caches", Verdict::Safe, "Web view cache, rebuilt automatically", Who::Ai),
        mk(Group::AiTools, "Claude Desktop VM image", "~/Library/Application Support/Claude/vm_bundles", 9_000_000_000, "Downloads", Verdict::Ask, "Agent sandbox image, large to download again", Who::Ai),
        mk(Group::AiTools, "Project · old-experiment", "~/.claude/projects/-Users-me-old-experiment", 412_000_000, "Old Sessions", Verdict::Ask, "Chat history for a folder that no longer exists", Who::Ai),
        mk(Group::AiTools, "Project · cmdmeow", "~/.claude/projects/-Users-me-Projects-cmdmeow", 96_000_000, "Projects", Verdict::Keep, "You used this project today", Who::Rule),
        mk(Group::AiTools, "Claude Desktop logs", "~/Library/Logs/Claude", 42_000_000, "Logs", Verdict::Safe, "Old app logs", Who::Ai),
        mk(Group::AiTools, "Claude CLI logs", "~/Library/Caches/claude-cli-nodejs", 18_000_000, "Logs", Verdict::Safe, "Old CLI logs", Who::Ai),
        mk(Group::AppCaches, "Spotify", "~/Library/Caches/com.spotify.client", 1_200_000_000, "Caches", Verdict::Safe, "Streaming cache, rebuilt automatically", Who::Ai),
        mk(Group::AppCaches, "Google Chrome", "~/Library/Caches/Google", 820_000_000, "Caches", Verdict::Safe, "Browser cache, rebuilt automatically", Who::Ai),
        mk(Group::Developer, "Derived Data · MyApp", "~/Library/Developer/Xcode/DerivedData/MyApp-abc", 3_400_000_000, "Build Data", Verdict::Safe, "Build products and index, Xcode rebuilds them", Who::Rule),
        mk(Group::AppCaches, "Adobe Media Cache", "~/Library/Application Support/Adobe/Common/Media Cache Files", 2_100_000_000, "Caches", Verdict::Safe, "Premiere/After Effects media cache", Who::Rule),
        mk(Group::Developer, "npm cache", "~/.npm/_cacache", 640_000_000, "Caches", Verdict::Safe, "npm downloads, fetched again when needed", Who::Ai),
    ];
    cleanup::State { phase: Phase::Results, items, ai_used: true, ..Default::default() }
}

/// The Done screen with sample results (screenshots).
pub fn demo_done() -> cleanup::State {
    let mut st = demo_results();
    st.phase = Phase::Done;
    let sel: Vec<&Item> = st.items.iter().filter(|i| i.selected).collect();
    st.cleaned_count = sel.len();
    st.cleaned_bytes = sel.iter().map(|i| i.bytes).sum();
    st.cleaned = sel.iter().map(|i| (i.group, i.bytes)).collect();
    st.trashed_bytes = sel.iter().filter(|i| !i.rebuilds()).map(|i| i.bytes).sum();
    st.log = sel.iter().map(|i| format!("Moved to Trash: {}", i.path.display())).collect();
    st
}
