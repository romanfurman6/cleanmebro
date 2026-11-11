//! Cleanup screens: start → scanning → Claude reviewing paths → results (with
//! "needs your OK" questions) → cleaning → done.

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
    let phase = app.clean.lock().unwrap().phase.clone();
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

    // top: the disk in one line; middle: a round Scan button; bottom: the
    // five groups with what the last scan found in each
    let (total, free) = crate::kit::fs::volume_space("/");
    let used = if total > 0 { 1.0 - free as f32 / total as f32 } else { 0.0 };
    let disk = Rect::from_min_size(body.min, Vec2::new(body.width(), 56.0));
    glass_card(&p, disk, R_CARD, 0.0, false);
    let di = disk.shrink2(Vec2::new(sp::L, 0.0));
    icon(&p, Icon::Disk, Rect::from_center_size(Pos2::new(di.left() + 9.0, di.center().y), Vec2::splat(18.0)), TEXT_SOFT());
    let name = p.text(Pos2::new(di.left() + 18.0 + sp::M, di.center().y), Align2::LEFT_CENTER, "Macintosh HD", ty::title(), TEXT());
    let right = if total > 0 { format!("{} free of {}", human_bytes(free), human_bytes(total)) } else { String::new() };
    let rr = p.text(Pos2::new(di.right(), di.center().y), Align2::RIGHT_CENTER, &right, ty::body(), TEXT_SOFT());
    let bar_r = Rect::from_min_max(Pos2::new(name.right() + sp::XL, di.center().y - 4.0), Pos2::new(rr.left() - sp::XL, di.center().y + 4.0));
    if bar_r.width() > 40.0 {
        bar(&p, bar_r, used, BLUE());
    }

    // the groups, with the last scan's sizes (sample sizes in the demo)
    let last = if app.demo { Some(super::engine::LastScan { when: chrono::Utc::now().timestamp() - 3 * 86400, groups: vec![3_400_000_000, 1_900_000_000, 6_200_000_000, 410_000_000, 820_000_000] }) } else { super::engine::last_scan() };
    let hist = super::engine::history();
    let tiles_h = 116.0;
    let tiles = Rect::from_min_max(Pos2::new(body.left(), body.bottom() - tiles_h), body.max);
    let head_y = tiles.top() - sp::M - 16.0;
    if let Some(l) = &last {
        p.text(Pos2::new(body.left(), head_y), Align2::LEFT_TOP, format!("Last scan {}", super::engine::days_ago(l.when)), ty::label(), TEXT_DIM());
    }
    if !hist.is_empty() {
        let freed: u64 = hist.iter().map(|h| h.1).sum();
        p.text(Pos2::new(body.right(), head_y), Align2::RIGHT_TOP, format!("{} freed in {} clean{}", human_bytes(freed), hist.len(), if hist.len() == 1 { "" } else { "s" }), ty::label(), TEXT_DIM());
    }
    let n = Group::ALL.len() as f32;
    let gap = sp::M;
    let tw = (tiles.width() - gap * (n - 1.0)) / n;
    for (i, g) in Group::ALL.iter().enumerate() {
        let t = Rect::from_min_size(Pos2::new(tiles.left() + i as f32 * (tw + gap), tiles.top()), Vec2::new(tw, tiles_h));
        glass_card(&p, t, R_CARD, 0.0, false);
        let ti = t.shrink(sp::L);
        let col = junk_ramp(i);
        let chip = Rect::from_min_size(ti.min, Vec2::splat(30.0));
        p.circle_filled(chip.center(), 15.0, with_alpha(col, 0.18));
        let ir = Rect::from_center_size(chip.center(), Vec2::splat(16.0));
        let drawn = match g {
            Group::Developer => symbol(&p, "hammer", ir, col),
            _ => false,
        };
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
        let size = last.as_ref().and_then(|l| l.groups.get(i).copied());
        let name_y = if size.is_some() { ti.bottom() - 22.0 } else { ti.bottom() };
        text_fit(&p, Pos2::new(ti.left(), name_y), Align2::LEFT_BOTTOM, g.title(), ty::body(), TEXT_SOFT(), ti.width());
        if let Some(b) = size {
            text_fit(&p, Pos2::new(ti.left(), ti.bottom()), Align2::LEFT_BOTTOM, &human_bytes(b), ty::title(), TEXT(), ti.width());
        }
    }

    // the middle: a note when Claude is missing, then the Scan button
    let mut mid = Rect::from_min_max(Pos2::new(body.left(), disk.bottom() + sp::L), Pos2::new(body.right(), head_y - sp::L));
    if !has_claude {
        let h = claude_note(ui, Pos2::new(body.left(), mid.bottom() - claude_note_height(&p, body.width(), avail)), body.width(), avail, tcx);
        mid.max.y -= h + sp::L;
    }
    let below = if has_claude { 18.0 + sp::L } else { 0.0 };
    let r_btn = ((mid.height() - below) / 2.0 - sp::M).clamp(36.0, 84.0);
    let c = Pos2::new(mid.center().x, mid.center().y - below / 2.0);
    let id = ui.scope_id().with("scan-round");
    let resp = ui.interact(Rect::from_center_size(c, Vec2::splat(r_btn * 2.0)), id, Sense::click());
    let hover = ui.ctx().animate_bool(id.with("h"), resp.hovered());
    p.circle_filled(c, r_btn + 10.0 + 4.0 * hover, with_alpha(BLUE(), 0.10 + 0.06 * hover));
    p.circle_filled(c, r_btn, BLUE());
    p.text(c, Align2::CENTER_CENTER, "Scan", ty::hero(), on_color(BLUE()));
    if resp.clicked() {
        cleanup::start_scan(app.clean.clone(), ctx, app.use_ai && has_claude);
    }
    if has_claude {
        let label = "Check with AI";
        let lw = p.layout_no_wrap(label.into(), ty::body(), TEXT()).size().x;
        let total_w = 30.0 + sp::S + lw;
        let sw = Rect::from_min_size(Pos2::new(c.x - total_w / 2.0, c.y + r_btn + 14.0 + sp::L), Vec2::new(30.0, 18.0));
        if switch(ui, sw, ui.scope_id().with("use-ai"), app.use_ai, true).clicked() {
            app.use_ai = !app.use_ai;
        }
        p.text(Pos2::new(sw.right() + sp::S, sw.center().y), Align2::LEFT_CENTER, label, ty::body(), TEXT());
    }
}

// ---------------------------------------------------------------- scanning / judging / cleaning

fn working(app: &mut App, ui: &mut Ui, r: Rect, phase: Phase) {
    let ctx = ui.ctx().clone();
    let st = app.clean.lock().unwrap().clone();
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
            app.clean.lock().unwrap().stop = true;
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
        let g = app.clean.lock().unwrap();
        (g.total(), g.pending_questions(), g.ai_used, g.ai_error.clone(), g.selected_bytes())
    };
    let avail = claude_now(app);
    let mut sub = format!("{} found", human_bytes(total));
    match (&ai_error, ai_used) {
        (Some(e), _) => sub = format!("{sub} · {e}"),
        (None, true) => sub.push_str(" · Claude reviewed every path"),
        (None, false) if !avail.ready() => sub.push_str(&format!(" · judged by the built-in rules ({})", avail.short())),
        (None, false) => sub.push_str(" · judged by the built-in rules"),
    }
    if pending > 0 {
        sub.push_str(&format!(" · {pending} need your OK"));
    }
    let btns = page_header(&p, r, "Cleanup", &sub, if ai_error.is_some() { AMBER() } else { TEXT_DIM() });
    let w = button_width(&p, "Scan again", BTN_SECONDARY);
    let sr = Rect::from_min_max(Pos2::new(btns.right() - w, btns.top()), btns.max);
    if pill_button(ui, sr, "Scan again", BTN_SECONDARY, true).clicked() {
        cleanup::start_scan(app.clean.clone(), ctx.clone(), app.use_ai && avail.ready());
        return;
    }
    let hidden = app.clean.lock().unwrap().hidden_kept;
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
            app.clean.lock().unwrap().phase = Phase::Intro;
        }
        return;
    }

    let body = Rect::from_min_max(Pos2::new(r.left(), r.top() + PAGE_HEADER_H + 1.0), Pos2::new(r.right(), r.bottom() - PAGE_FOOTER_H));

    // left: groups
    let lr = Rect::from_min_max(body.min, Pos2::new(body.left() + 250.0, body.bottom()));
    hairline(&p, lr.right_top(), lr.right_bottom());
    let mut rows: Vec<(Option<Group>, String, u64, usize)> = vec![];
    {
        let g = app.clean.lock().unwrap();
        if pending > 0 {
            let b = g.items.iter().filter(|i| i.verdict == Verdict::Ask && i.by != Who::You).map(|i| i.bytes).sum();
            rows.push((None, "Needs your OK".into(), b, pending));
        }
        for grp in Group::ALL {
            let n = g.items.iter().filter(|i| i.group == grp).count();
            if n > 0 {
                let q = g.items.iter().filter(|i| i.group == grp && i.verdict == Verdict::Ask && i.by != Who::You).count();
                rows.push((Some(grp), grp.title().into(), g.group_bytes(grp), q));
            }
        }
    }
    if !rows.iter().any(|r| r.0 == app.clean_group) {
        app.clean_group = rows.first().and_then(|r| r.0);
    }
    let mut y = lr.top() + 12.0;
    for (grp, title, bytes, q) in &rows {
        let rr = Rect::from_min_size(Pos2::new(lr.left() + 12.0, y), Vec2::new(lr.width() - 24.0, 34.0));
        let sel = app.clean_group == *grp;
        let id = ui.scope_id().with(("grp", title.as_str()));
        let resp = ui.interact(rr, id, Sense::click());
        let hv = ctx.animate_bool(id, resp.hovered());
        if sel {
            p.rect_filled(rr, R_BUTTON, white(20));
        } else if hv > 0.0 {
            p.rect_filled(rr, R_BUTTON, white((10.0 * hv) as u8));
        }
        let sz = p.text(Pos2::new(rr.right() - 10.0, rr.center().y), Align2::RIGHT_CENTER, human_bytes(*bytes), font(13.0), TEXT_DIM());
        let mut right = sz.left() - 8.0;
        if *q > 0 && grp.is_none() {
            right = badge(&p, Pos2::new(right, rr.center().y), Align2::RIGHT_CENTER, &q.to_string()).left() - 8.0;
        } else if *q > 0 {
            right = badge(&p, Pos2::new(right, rr.center().y), Align2::RIGHT_CENTER, &q.to_string()).left() - 8.0;
        }
        text_fit(&p, Pos2::new(rr.left() + 10.0, rr.center().y), Align2::LEFT_CENTER, title, font(13.0), TEXT(), right - rr.left() - 14.0);
        if resp.clicked() {
            app.clean_group = *grp;
        }
        y += 36.0;
    }

    // right: items of the chosen group
    let tr = Rect::from_min_max(Pos2::new(lr.right() + 1.0, body.top()), body.max);
    let (gtitle, blurb) = match app.clean_group {
        Some(g) => (g.title(), g.blurb()),
        None => ("Needs your OK", "Check these before removing"),
    };
    let gbytes = rows.iter().find(|r| r.0 == app.clean_group).map(|r| r.2).unwrap_or(0);
    let c_name = tr.left() + 24.0 + 28.0 + 12.0;
    let c_size = tr.right() - 24.0;
    let c_verdict = c_size - 80.0 - 12.0 - 110.0;
    let hy = tr.top() + 10.0;
    section_header(&p, Pos2::new(c_name, hy), Align2::LEFT_TOP, &format!("{} · {}", gtitle, human_bytes(gbytes)));
    section_header(&p, Pos2::new(c_verdict, hy), Align2::LEFT_TOP, "Verdict");
    section_header(&p, Pos2::new(c_size, hy), Align2::RIGHT_TOP, "Size");
    // bulk actions on the right of the blurb line
    let la = Pos2::new(c_verdict - 16.0, hy + 20.0);
    let mut links_left = la.x;
    if app.clean_group.is_none() {
        let mut answer: Option<Verdict> = None;
        let a = link(ui, la, Align2::RIGHT_TOP, "Allow all", 12.0);
        if a.clicked() {
            answer = Some(Verdict::Safe);
        }
        let k = link(ui, Pos2::new(a.rect.left() - 10.0, la.y), Align2::RIGHT_TOP, "Keep all", 12.0);
        links_left = k.rect.left();
        if k.clicked() {
            answer = Some(Verdict::Keep);
        }
        if let Some(v) = answer {
            let mut kept = Vec::new();
            for it in app.clean.lock().unwrap().items.iter_mut().filter(|i| i.verdict == Verdict::Ask && i.by != Who::You) {
                if v == Verdict::Keep {
                    kept.push(it.path.clone());
                }
                it.verdict = v;
                it.by = Who::You;
                it.selected = v == Verdict::Safe;
                it.reason = if v == Verdict::Safe { "You allowed it".into() } else { "You chose to keep it".into() };
            }
            if !kept.is_empty() {
                cleanup::remember_kept(kept);
            }
        }
    } else if let Some(g) = app.clean_group {
        let a = link(ui, la, Align2::RIGHT_TOP, "None", 12.0);
        if a.clicked() {
            for it in app.clean.lock().unwrap().items.iter_mut().filter(|i| i.group == g) {
                it.selected = false;
            }
        }
        let sl = link(ui, Pos2::new(a.rect.left() - 10.0, la.y), Align2::RIGHT_TOP, "Select safe", 12.0);
        links_left = sl.rect.left();
        if sl.clicked() {
            for it in app.clean.lock().unwrap().items.iter_mut().filter(|i| i.group == g) {
                it.selected = it.verdict == Verdict::Safe && !it.permanent;
            }
        }
    }
    let br = text_fit(&p, Pos2::new(c_name, hy + 20.0), Align2::LEFT_TOP, blurb, font(12.0), TEXT_FAINT(), links_left - c_name - 16.0);
    ui.interact(br, ui.scope_id().with("blurb"), Sense::hover()).on_hover_text(blurb);
    let head_bottom = tr.top() + 48.0;
    hairline(&p, Pos2::new(tr.left(), head_bottom), Pos2::new(tr.right(), head_bottom));
    let list = Rect::from_min_max(Pos2::new(tr.left(), head_bottom + 1.0), tr.max);
    let idx: Vec<usize> = {
        let g = app.clean.lock().unwrap();
        g.items
            .iter()
            .enumerate()
            .filter(|(_, it)| match app.clean_group {
                Some(gr) => it.group == gr,
                None => it.verdict == Verdict::Ask && it.by != Who::You,
            })
            .map(|(i, _)| i)
            .collect()
    };
    let mut child = ui.new_child(UiBuilder::new().max_rect(list).layout(egui::Layout::top_down(egui::Align::Min)));
    child.set_clip_rect(list);
    ScrollArea::vertical().id_salt(("items", format!("{:?}", app.clean_group))).auto_shrink([false, false]).show(&mut child, |ui| {
        ui.spacing_mut().item_spacing.y = 0.0;
        for i in idx {
            item_row(app, ui, i, list.width(), (c_name - list.left(), c_verdict - list.left(), c_size - list.left()));
        }
    });

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

fn verdict_chip(it: &Item) -> (&'static str, (Color32, Color32)) {
    match it.verdict {
        Verdict::Safe => ("Safe", CHIP_SAFE()),
        Verdict::Ask => ("Ask", CHIP_ASK()),
        Verdict::Keep => ("Kept", CHIP_KEPT()),
    }
}

/// One results row. `cols` = x offsets of the name, verdict and size (right edge) columns.
fn item_row(app: &mut App, ui: &mut Ui, i: usize, w: f32, cols: (f32, f32, f32)) {
    let h = 66.0;
    let (rect, resp) = ui.allocate_exact_size(Vec2::new(w, h), Sense::click());
    // a group can hold hundreds of items: only those in view are drawn
    if !ui.is_rect_visible(rect) {
        return;
    }
    let it = app.clean.lock().unwrap().items[i].clone();
    let asking = it.verdict == Verdict::Ask && it.by != Who::You;
    let p = ui.painter().clone();
    if resp.hovered() {
        p.rect_filled(rect, 0.0, white(6));
    }
    let can_select = it.verdict != Verdict::Keep && !it.hard_keep;
    let cb = Rect::from_center_size(Pos2::new(rect.left() + 24.0 + 8.0, rect.center().y), Vec2::splat(16.0));
    let cr = checkbox(ui, cb, ui.scope_id().with(("sel", &it.path)), it.selected, can_select);
    if cr.clicked() && can_select {
        let mut g = app.clean.lock().unwrap();
        let x = &mut g.items[i];
        x.selected = !x.selected;
        if x.selected && x.verdict == Verdict::Ask {
            x.verdict = Verdict::Safe;
            x.by = Who::You;
        }
    }
    let (cn, cv, cs) = (rect.left() + cols.0, rect.left() + cols.1, rect.left() + cols.2);
    let name_w = cv - cn - 12.0;
    text_fit(&p, Pos2::new(cn, rect.top() + 10.0), Align2::LEFT_TOP, &it.title, bold(13.0), TEXT(), name_w);
    text_fit(&p, Pos2::new(cn, rect.top() + 28.0), Align2::LEFT_TOP, &middle_ellipsis(&short_path(&it.path), 90), mono(11.0), TEXT_FAINT(), name_w);
    let who = match it.by {
        Who::Ai => "",
        Who::You => "",
        Who::Rule => "",
    };
    text_fit(&p, Pos2::new(cn, rect.top() + 45.0), Align2::LEFT_TOP, &format!("{who}{}", it.reason), font(12.0), TEXT_DIM(), name_w);
    let (label, colors) = verdict_chip(&it);
    let top = if asking { rect.top() + 10.0 } else { rect.center().y - 9.0 };
    let chip_r = chip(&p, Pos2::new(cv, top), Align2::LEFT_TOP, label, colors);
    if it.permanent {
        chip(&p, Pos2::new(chip_r.right() + 6.0, top), Align2::LEFT_TOP, "Permanent", CHIP_RISK());
    }
    p.text(Pos2::new(cs, rect.center().y), Align2::RIGHT_CENTER, human_bytes(it.bytes), bold(13.0), TEXT());
    if asking {
        let y = chip_r.bottom() + 6.0;
        let a = Rect::from_min_size(Pos2::new(cv, y), Vec2::new(52.0, 22.0));
        let k = Rect::from_min_size(Pos2::new(cv + 56.0, y), Vec2::new(48.0, 22.0));
        if small_button(ui, a, "Allow", true) {
            let mut g = app.clean.lock().unwrap();
            let x = &mut g.items[i];
            x.verdict = Verdict::Safe;
            x.by = Who::You;
            x.selected = true;
            x.reason = "You allowed it".into();
        }
        if small_button(ui, k, "Keep", false) {
            let mut g = app.clean.lock().unwrap();
            let x = &mut g.items[i];
            x.verdict = Verdict::Keep;
            x.by = Who::You;
            x.selected = false;
            x.reason = "You chose to keep it".into();
            cleanup::remember_kept([x.path.clone()]);
        }
    }
    p.line_segment([Pos2::new(rect.left() + 24.0, rect.bottom() - 0.5), Pos2::new(rect.right() - 24.0, rect.bottom() - 0.5)], Stroke::new(1.0, white(10)));
    if resp.double_clicked() {
        let _ = std::process::Command::new("open").arg("-R").arg(&it.path).spawn();
    }
}

/// The 22 pt Allow / Keep buttons inside a row.
fn small_button(ui: &mut Ui, r: Rect, label: &str, primary: bool) -> bool {
    let id = ui.scope_id().with(("small", label, r.min.x as i32, r.min.y as i32));
    let resp = ui.interact(r, id, Sense::click());
    let h = ui.ctx().animate_bool(id, resp.hovered());
    let p = ui.painter();
    if primary {
        p.rect_filled(r, 6.0, lerp_color(BLUE(), Color32::WHITE, 0.1 * h));
        p.text(r.center(), Align2::CENTER_CENTER, label, bold(11.0), ON_ACTION);
    } else {
        p.rect_filled(r, 6.0, white((10.0 * h) as u8));
        p.rect_stroke(r, 6.0, Stroke::new(1.0, white(36)), StrokeKind::Inside);
        p.text(r.center(), Align2::CENTER_CENTER, label, font(11.0), TEXT());
    }
    resp.clicked()
}

fn confirm(app: &mut App, ui: &mut Ui, r: Rect) {
    let ctx = ui.ctx().clone();
    let p = ui.painter().clone();
    // block clicks to the page underneath
    let _ = ui.interact(r, ui.scope_id().with("confirm-scrim"), Sense::click());
    p.rect_filled(r, CornerRadius { nw: 0, sw: 0, ne: WIN_R as u8, se: WIN_R as u8 }, Color32::from_black_alpha(115));
    let (n, bytes, perm, trashed) = {
        let g = app.clean.lock().unwrap();
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
    let st = app.clean.lock().unwrap().clone();
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
        app.clean.lock().unwrap().phase = Phase::Intro;
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
