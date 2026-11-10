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
fn claude_note(ui: &mut Ui, pos: Pos2, width: f32, a: Availability, cx: &mut Cx) -> f32 {
    let p = ui.painter().clone();
    let x = pos.x + 40.0;
    let w = width - 54.0;
    let body = format!("Cleanup still works with its built-in rules. {} Then Claude double-checks each path too.", a.how());
    let text_h = p.layout(body.clone(), font(12.0), TEXT_DIM, w).size().y;
    let r = Rect::from_min_size(pos, Vec2::new(width, 32.0 + text_h + 12.0 + 26.0 + 14.0));
    glass_card(&p, r, R_CARD, 0.0, false);
    icon(&p, Icon::Sparkle, Rect::from_min_size(r.min + Vec2::new(14.0, 14.0), Vec2::splat(16.0)), TEXT_FAINT);
    text_fit(&p, Pos2::new(x, r.top() + 13.0), Align2::LEFT_TOP, a.title(), bold(13.0), TEXT, w);
    paragraph(&p, Pos2::new(x, r.top() + 32.0), &body, font(12.0), TEXT_DIM, w);
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
    let p = ui.painter().clone();
    page_header(&p, r, "Cleanup", "Not scanned yet", TEXT_DIM);
    let colw = 560.0f32.min(r.width() - 48.0);
    let x = r.center().x - colw / 2.0;
    let mut y = r.top() + PAGE_HEADER_H + 40.0;

    // hero line
    tool_chip(&p, Rect::from_min_size(Pos2::new(x, y), Vec2::splat(56.0)), Icon::Broom, tint::CLEANUP);
    p.text(Pos2::new(x + 72.0, y + 2.0), Align2::LEFT_TOP, "Find junk you can safely remove", bold(22.0), TEXT);
    let h = paragraph(&p, Pos2::new(x + 72.0, y + 32.0), "Caches, logs and leftovers from Xcode, Claude, Electron apps, npm, Gradle and more.", font(13.0), TEXT_DIM, colw - 72.0);
    y += (32.0 + h).max(56.0) + 22.0;

    // what happens
    let first = if avail.ready() {
        (Icon::Sparkle, "Claude checks every path", "Each item gets a verdict: safe, ask you, or keep.")
    } else {
        (Icon::Check, "Built-in rules check every path", "Each item gets a verdict: safe, ask you, or keep.")
    };
    let points = [
        first,
        (Icon::Question, "You decide on anything unclear", "Unsure items wait for your OK. Projects you use are never touched."),
        (Icon::Trash, "Goes to the Trash first", "You can put anything back until you empty the Trash."),
    ];
    let card_r = Rect::from_min_size(Pos2::new(x, y), Vec2::new(colw, 8.0 + points.len() as f32 * 52.0));
    glass_card(&p, card_r, R_CARD, 0.0, false);
    let mut py = card_r.top() + 4.0;
    for (ic, title, body) in points {
        icon(&p, ic, Rect::from_min_size(Pos2::new(x + 14.0, py + 10.0), Vec2::splat(18.0)), ACCENT);
        p.text(Pos2::new(x + 44.0, py + 10.0), Align2::LEFT_TOP, title, bold(13.0), TEXT);
        text_fit(&p, Pos2::new(x + 44.0, py + 28.0), Align2::LEFT_TOP, body, font(13.0), TEXT_DIM, colw - 58.0);
        py += 52.0;
    }
    y = card_r.bottom() + 22.0;

    // where it looks
    section_header(&p, Pos2::new(x, y), Align2::LEFT_TOP, "LOOKS IN");
    y += 22.0;
    let mut cx = x;
    for g in Group::ALL {
        let gl = p.layout_no_wrap(g.title().to_string(), font(12.0), TEXT_SOFT);
        let w = gl.size().x + 20.0;
        if cx + w > x + colw {
            cx = x;
            y += 30.0;
        }
        let pr = Rect::from_min_size(Pos2::new(cx, y), Vec2::new(w, 24.0));
        p.rect_filled(pr, 12.0, white(15));
        p.galley(pr.center() - gl.size() / 2.0, gl, TEXT_SOFT);
        cx += w + 6.0;
    }
    y += 24.0 + 22.0;

    // AI switch, or how to turn Claude on
    let has_claude = avail.ready();
    if has_claude {
        let sw = Rect::from_min_size(Pos2::new(x, y), Vec2::new(30.0, 18.0));
        if switch(ui, sw, ui.id().with("use-ai"), app.use_ai, true).clicked() {
            app.use_ai = !app.use_ai;
        }
        let (a, b) = ("Let Claude judge what's safe ", "(it sees paths, sizes and dates, never file contents)");
        let ga = p.layout_no_wrap(a.into(), font(13.0), TEXT);
        let wa = ga.size().x;
        p.galley(Pos2::new(x + 40.0, sw.center().y - ga.size().y / 2.0), ga, TEXT);
        text_fit(&p, Pos2::new(x + 40.0 + wa, sw.center().y), Align2::LEFT_CENTER, b, font(13.0), TEXT_FAINT, colw - 40.0 - wa);
        y += 18.0 + 22.0;
    } else {
        y += claude_note(ui, Pos2::new(x, y), colw, avail, tcx) + 22.0;
    }

    // scan
    let br = Rect::from_min_size(Pos2::new(x, y), Vec2::new(button_width(&p, "Scan", BTN_PRIMARY) + 28.0, 34.0));
    if pill_button(ui, br, "Scan", BTN_PRIMARY, true).clicked() {
        cleanup::start_scan(app.clean.clone(), ctx, app.use_ai && has_claude);
    }
    text_fit(&p, Pos2::new(br.right() + 12.0, br.center().y), Align2::LEFT_CENTER, "Usually takes under a minute. Nothing is removed without your OK.", font(12.0), TEXT_FAINT, colw - br.width() - 12.0);
}

// ---------------------------------------------------------------- scanning / judging / cleaning

fn working(app: &mut App, ui: &mut Ui, r: Rect, phase: Phase) {
    let ctx = ui.ctx().clone();
    let st = app.clean.lock().unwrap().clone();
    let p = ui.painter().clone();
    let t = ctx.input(|i| i.time);
    let sub = match phase {
        Phase::Judging => "Step 2 of 2 · Asking Claude what's safe",
        Phase::Cleaning => "Cleaning…",
        _ if !st.with_ai => "Looking for junk",
        _ => "Step 1 of 2 · Looking for junk",
    };
    let btns = page_header(&p, r, "Cleanup", sub, TEXT_DIM);
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
    let g = p.layout_no_wrap(big, bold(28.0), TEXT);
    let gw = g.size().x;
    p.galley(Pos2::new(x0, y), g, TEXT);
    p.text(Pos2::new(x0 + gw + 10.0, y + 22.0), Align2::LEFT_BOTTOM, caption, font(13.0), TEXT_DIM);
    if st.progress >= 0.0 {
        p.text(Pos2::new(x1, y + 22.0), Align2::RIGHT_BOTTOM, format!("{:.0}%", st.progress.clamp(0.0, 1.0) * 100.0), font(13.0), TEXT_DIM);
    }
    y += 40.0;
    // progress bar (an indeterminate sweep while the scan can't count)
    let track = Rect::from_min_size(Pos2::new(x0, y), Vec2::new(x1 - x0, 6.0));
    if st.progress >= 0.0 {
        bar(&p, track, st.progress, BLUE);
    } else {
        p.rect_filled(track, 3.0, white(20));
        let w = track.width() * 0.25;
        let f = ((t * 0.6).fract() as f32) * (track.width() + w) - w;
        let seg = Rect::from_min_size(Pos2::new(track.left() + f, track.top()), Vec2::new(w, 6.0)).intersect(track);
        if seg.width() > 0.0 {
            p.rect_filled(seg, 3.0, BLUE);
        }
    }
    y += 16.0;
    // what it's doing right now
    match phase {
        Phase::Judging => {
            icon(&p, Icon::Sparkle, Rect::from_min_size(Pos2::new(x0, y), Vec2::splat(14.0)), CLAUDE);
            let line = if st.ai_status.is_empty() { "Asking Claude…".to_string() } else { st.ai_status.clone() };
            text_fit(&p, Pos2::new(x0 + 22.0, y + 7.0), Align2::LEFT_CENTER, &format!("{line} · only paths, sizes and dates are sent"), font(12.0), TEXT_FAINT, x1 - x0 - 22.0);
        }
        _ => {
            let line = st.current.replacen(&home_str(), "~", 1);
            text_fit(&p, Pos2::new(x0, y + 7.0), Align2::LEFT_CENTER, &middle_ellipsis(&line, 110), mono(11.0), TEXT_FAINT, x1 - x0);
        }
    }
    y += 34.0;

    // per-group table
    let cols = [x0 + 12.0, x0 + 48.0, x1 - 250.0, x1 - 12.0];
    section_header(&p, Pos2::new(cols[1], y), Align2::LEFT_TOP, "GROUP");
    section_header(&p, Pos2::new(cols[2], y), Align2::LEFT_TOP, "ITEMS");
    section_header(&p, Pos2::new(cols[3], y), Align2::RIGHT_TOP, if phase == Phase::Cleaning { "SELECTED" } else { "FOUND" });
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
            p.circle_filled(mark, 8.0, tint::HEALTH);
            icon(&p, Icon::Check, Rect::from_center_size(mark, Vec2::splat(11.0)), Color32::WHITE);
        } else {
            p.circle_stroke(mark, 7.5, Stroke::new(1.5, white(64)));
        }
        p.text(Pos2::new(cols[1], row.center().y), Align2::LEFT_CENTER, grp.title(), font(13.0), TEXT);
        let (status, sc) = if n > 0 { (format!("{n} found"), Color32::from_rgb(0x5B, 0xE0, 0x7F)) } else { ("—".to_string(), TEXT_FAINT) };
        p.text(Pos2::new(cols[2], row.center().y), Align2::LEFT_CENTER, status, font(12.0), sc);
        p.text(Pos2::new(cols[3], row.center().y), Align2::RIGHT_CENTER, if n > 0 { human_bytes(bytes) } else { "—".into() }, font(13.0), if n > 0 { TEXT } else { TEXT_FAINT });
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
    let btns = page_header(&p, r, "Cleanup", &sub, if ai_error.is_some() { AMBER } else { TEXT_DIM });
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
        p.circle_filled(c, 28.0, tint::HEALTH);
        icon(&p, Icon::Check, Rect::from_center_size(c, Vec2::splat(28.0)), Color32::WHITE);
        p.text(c + Vec2::new(0.0, 56.0), Align2::CENTER_CENTER, "Nothing to clean", bold(22.0), TEXT);
        p.text(c + Vec2::new(0.0, 84.0), Align2::CENTER_CENTER, "No caches, logs or leftovers worth removing were found.", font(13.0), TEXT_DIM);
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
        let sz = p.text(Pos2::new(rr.right() - 10.0, rr.center().y), Align2::RIGHT_CENTER, human_bytes(*bytes), font(13.0), TEXT_DIM);
        let mut right = sz.left() - 8.0;
        if *q > 0 && grp.is_none() {
            right = badge(&p, Pos2::new(right, rr.center().y), Align2::RIGHT_CENTER, &q.to_string()).left() - 8.0;
        } else if *q > 0 {
            right = badge(&p, Pos2::new(right, rr.center().y), Align2::RIGHT_CENTER, &q.to_string()).left() - 8.0;
        }
        text_fit(&p, Pos2::new(rr.left() + 10.0, rr.center().y), Align2::LEFT_CENTER, title, font(13.0), TEXT, right - rr.left() - 14.0);
        if resp.clicked() {
            app.clean_group = *grp;
        }
        y += 36.0;
    }

    // right: items of the chosen group
    let tr = Rect::from_min_max(Pos2::new(lr.right() + 1.0, body.top()), body.max);
    let (gtitle, blurb) = match app.clean_group {
        Some(g) => (g.title(), g.blurb()),
        None => ("Needs your OK", "Claude wasn't sure these are safe. Allow each one you're happy to remove, or keep it."),
    };
    let gbytes = rows.iter().find(|r| r.0 == app.clean_group).map(|r| r.2).unwrap_or(0);
    let c_name = tr.left() + 24.0 + 28.0 + 12.0;
    let c_size = tr.right() - 24.0;
    let c_verdict = c_size - 80.0 - 12.0 - 110.0;
    let hy = tr.top() + 10.0;
    section_header(&p, Pos2::new(c_name, hy), Align2::LEFT_TOP, &format!("{} · {}", gtitle.to_uppercase(), human_bytes(gbytes)));
    section_header(&p, Pos2::new(c_verdict, hy), Align2::LEFT_TOP, "VERDICT");
    section_header(&p, Pos2::new(c_size, hy), Align2::RIGHT_TOP, "SIZE");
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
    let br = text_fit(&p, Pos2::new(c_name, hy + 20.0), Align2::LEFT_TOP, blurb, font(12.0), TEXT_FAINT, links_left - c_name - 16.0);
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
    p.text(Pos2::new(f.left() + 24.0, fy - 9.0), Align2::LEFT_CENTER, format!("{} selected", human_bytes(selected)), bold(13.0), TEXT);
    p.text(
        Pos2::new(f.left() + 24.0, fy + 10.0),
        Align2::LEFT_CENTER,
        if app.delete_now { "Deleted right away. This can't be undone." } else { "Goes to the Trash. You can put it back." },
        font(12.0),
        if app.delete_now { AMBER } else { TEXT_FAINT },
    );
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
        Verdict::Safe => ("Safe", CHIP_SAFE),
        Verdict::Ask => ("Ask", CHIP_ASK),
        Verdict::Keep => ("Kept", CHIP_KEPT),
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
    text_fit(&p, Pos2::new(cn, rect.top() + 10.0), Align2::LEFT_TOP, &it.title, bold(13.0), TEXT, name_w);
    text_fit(&p, Pos2::new(cn, rect.top() + 28.0), Align2::LEFT_TOP, &middle_ellipsis(&short_path(&it.path), 90), mono(11.0), TEXT_FAINT, name_w);
    let who = match it.by {
        Who::Ai => "Claude: ",
        Who::You => "",
        Who::Rule => "",
    };
    text_fit(&p, Pos2::new(cn, rect.top() + 45.0), Align2::LEFT_TOP, &format!("{who}{}", it.reason), font(12.0), TEXT_DIM, name_w);
    let (label, colors) = verdict_chip(&it);
    let top = if asking { rect.top() + 10.0 } else { rect.center().y - 9.0 };
    let chip_r = chip(&p, Pos2::new(cv, top), Align2::LEFT_TOP, label, colors);
    if it.permanent {
        chip(&p, Pos2::new(chip_r.right() + 6.0, top), Align2::LEFT_TOP, "Permanent", CHIP_RISK);
    }
    p.text(Pos2::new(cs, rect.center().y), Align2::RIGHT_CENTER, human_bytes(it.bytes), bold(13.0), TEXT);
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
        p.rect_filled(r, 6.0, lerp_color(BLUE, Color32::WHITE, 0.1 * h));
        p.text(r.center(), Align2::CENTER_CENTER, label, bold(11.0), Color32::WHITE);
    } else {
        p.rect_filled(r, 6.0, white((10.0 * h) as u8));
        p.rect_stroke(r, 6.0, Stroke::new(1.0, white(36)), StrokeKind::Inside);
        p.text(r.center(), Align2::CENTER_CENTER, label, font(11.0), TEXT);
    }
    resp.clicked()
}

fn confirm(app: &mut App, ui: &mut Ui, r: Rect) {
    let ctx = ui.ctx().clone();
    let p = ui.painter().clone();
    // block clicks to the page underneath
    let _ = ui.interact(r, ui.scope_id().with("confirm-scrim"), Sense::click());
    p.rect_filled(r, CornerRadius { nw: 0, sw: 0, ne: WIN_R as u8, se: WIN_R as u8 }, Color32::from_black_alpha(115));
    let (n, bytes, perm) = {
        let g = app.clean.lock().unwrap();
        let sel: Vec<&Item> = g.items.iter().filter(|i| i.selected).collect();
        (sel.len(), sel.iter().map(|i| i.bytes).sum::<u64>(), sel.iter().any(|i| i.permanent))
    };
    let mut body = if app.delete_now { "They'll be deleted right away. This can't be undone.".to_string() } else { "They go to the Trash, so you can put anything back.".to_string() };
    if perm {
        body.push_str(" Emptying the Trash is permanent.");
    }
    body.push_str(" Quit the apps involved first for the best result.");
    let bw = 440.0 - 44.0;
    let body_h = p.layout(body.clone(), font(13.0), TEXT_SOFT, bw).size().y;
    let m = Rect::from_center_size(r.center(), Vec2::new(440.0, 22.0 + 22.0 + 14.0 + body_h + 18.0 + 30.0 + 22.0));
    let shadow = egui::epaint::Shadow { offset: [0, 20], blur: 60, spread: 0, color: Color32::from_black_alpha(128) };
    p.add(shadow.as_shape(m, CornerRadius::same(14)));
    p.rect_filled(m, 14.0, RAISED);
    p.rect_stroke(m, 14.0, Stroke::new(1.0, white(31)), StrokeKind::Inside);
    p.text(m.min + Vec2::new(22.0, 22.0), Align2::LEFT_TOP, format!("Clean {} items ({})?", n, human_bytes(bytes)), bold(17.0), TEXT);
    paragraph(&p, m.min + Vec2::new(22.0, 58.0), &body, font(13.0), TEXT_SOFT, bw);
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
    page_header(&p, r, "Cleanup", "Finished", TEXT_DIM);
    let ok = st.failed.is_empty();
    let colw = 600.0f32.min(r.width() - 48.0);
    let x = r.center().x - colw / 2.0;
    let mut y = r.top() + PAGE_HEADER_H + 36.0;

    let c = Pos2::new(x + 28.0, y + 28.0);
    p.circle_filled(c, 28.0, if ok { tint::HEALTH } else { Color32::from_rgb(0xC9, 0x87, 0x1E) });
    icon(&p, if ok { Icon::Check } else { Icon::Info }, Rect::from_center_size(c, Vec2::splat(28.0)), Color32::WHITE);
    let title = if app.delete_now { format!("{} deleted", human_bytes(st.cleaned_bytes)) } else { format!("{} moved to the Trash", human_bytes(st.cleaned_bytes)) };
    p.text(Pos2::new(x + 72.0, y + 8.0), Align2::LEFT_TOP, title, bold(22.0), TEXT);
    let total = st.cleaned_count + st.failed.len();
    let mut sub = format!("{} of {} items cleaned.", st.cleaned_count, total);
    if !app.delete_now && st.cleaned_bytes > 0 {
        sub.push_str(" Empty the Trash to get the space back.");
    }
    text_fit(&p, Pos2::new(x + 72.0, y + 36.0), Align2::LEFT_TOP, &sub, font(13.0), TEXT_DIM, colw - 72.0);
    y += 56.0 + 18.0;

    if !ok {
        let lines = st.failed.len().min(3);
        let fr = Rect::from_min_size(Pos2::new(x, y), Vec2::new(colw, 52.0 + lines as f32 * 18.0));
        p.rect_filled(fr, R_CARD, with_alpha(AMBER, 0.10));
        p.rect_stroke(fr, R_CARD, Stroke::new(1.0, with_alpha(AMBER, 0.30)), StrokeKind::Inside);
        let n = st.failed.len();
        p.text(fr.min + Vec2::new(14.0, 12.0), Align2::LEFT_TOP, format!("{n} item{} couldn't be removed", if n == 1 { "" } else { "s" }), bold(13.0), Color32::from_rgb(0xFF, 0xC2, 0x66));
        for (k, (name, err)) in st.failed.iter().take(3).enumerate() {
            text_fit(&p, fr.min + Vec2::new(14.0, 32.0 + k as f32 * 18.0), Align2::LEFT_TOP, &format!("{name}: {err}"), mono(11.0), TEXT_SOFT, colw - 28.0);
        }
        text_fit(&p, Pos2::new(fr.left() + 14.0, fr.bottom() - 10.0), Align2::LEFT_BOTTOM, "Quit the apps involved and scan again.", font(12.0), TEXT_DIM, colw - 28.0);
        y = fr.bottom() + 18.0;
    }

    let btn_y = (r.bottom() - 32.0 - 36.0).min(if app.clean_log { r.bottom() } else { y + 8.0 });
    if app.clean_log {
        let lr = Rect::from_min_max(Pos2::new(x, y), Pos2::new(x + colw, r.bottom() - 32.0 - 36.0 - 18.0));
        glass_card(&p, lr, R_CARD, 0.0, false);
        p.text(lr.min + Vec2::new(14.0, 12.0), Align2::LEFT_TOP, "Cleanup log", bold(13.0), TEXT);
        p.text(Pos2::new(lr.right() - 14.0, lr.top() + 12.0), Align2::RIGHT_TOP, format!("{} entries", st.log.len()), font(12.0), TEXT_FAINT);
        hairline(&p, Pos2::new(lr.left(), lr.top() + 38.0), Pos2::new(lr.right(), lr.top() + 38.0));
        let inner = Rect::from_min_max(lr.min + Vec2::new(14.0, 46.0), lr.max - Vec2::new(10.0, 8.0));
        let mut child = ui.new_child(UiBuilder::new().max_rect(inner).layout(egui::Layout::top_down(egui::Align::Min)));
        child.set_clip_rect(inner);
        ScrollArea::vertical().id_salt("log").auto_shrink([false, false]).show(&mut child, |ui| {
            for l in &st.log {
                let col = if l.starts_with("Couldn't") { Color32::from_rgb(0xFF, 0xC2, 0x66) } else { TEXT_DIM };
                ui.label(egui::RichText::new(l.replacen(&home_str(), "~", 1)).font(glass::mono(11.0)).color(col));
            }
        });
    }
    let lw = button_width(&p, "Show log", BTN_SECONDARY);
    let lb = Rect::from_min_size(Pos2::new(x, btn_y), Vec2::new(lw, 32.0));
    if pill_button(ui, lb, if app.clean_log { "Hide log" } else { "Show log" }, BTN_SECONDARY, true).clicked() {
        app.clean_log = !app.clean_log;
    }
    let db = Rect::from_min_size(Pos2::new(x + colw - 88.0, btn_y), Vec2::new(88.0, 32.0));
    if pill_button(ui, db, "Done", BTN_PRIMARY, true).clicked() {
        app.clean.lock().unwrap().phase = Phase::Intro;
        app.clean_log = false;
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
        by,
        selected: verdict == Verdict::Safe,
        hard_keep: false,
        permanent: false,
        app_running: false,
    };
    let items = vec![
        mk(Group::Claude, "Claude Desktop cache", "~/Library/Application Support/Claude/Cache", 678_000_000, "Caches", Verdict::Safe, "Web view cache, rebuilt automatically", Who::Ai),
        mk(Group::Claude, "Claude Desktop VM image", "~/Library/Application Support/Claude/vm_bundles", 9_000_000_000, "Downloads", Verdict::Ask, "Agent sandbox image, large to download again", Who::Ai),
        mk(Group::Claude, "Project · old-experiment", "~/.claude/projects/-Users-me-old-experiment", 412_000_000, "Old Sessions", Verdict::Ask, "Chat history for a folder that no longer exists", Who::Ai),
        mk(Group::Claude, "Project · cmdmeow", "~/.claude/projects/-Users-me-Projects-cmdmeow", 96_000_000, "Projects", Verdict::Keep, "You used this project today", Who::Rule),
        mk(Group::Claude, "Claude Desktop logs", "~/Library/Logs/Claude", 42_000_000, "Logs", Verdict::Safe, "Old app logs", Who::Ai),
        mk(Group::Claude, "Claude CLI logs", "~/Library/Caches/claude-cli-nodejs", 18_000_000, "Logs", Verdict::Safe, "Old CLI logs", Who::Ai),
        mk(Group::UserCache, "Spotify", "~/Library/Caches/com.spotify.client", 1_200_000_000, "Caches", Verdict::Safe, "Streaming cache, rebuilt automatically", Who::Ai),
        mk(Group::UserCache, "Google Chrome", "~/Library/Caches/Google", 820_000_000, "Caches", Verdict::Safe, "Browser cache, rebuilt automatically", Who::Ai),
        mk(Group::Xcode, "Derived Data · MyApp", "~/Library/Developer/Xcode/DerivedData/MyApp-abc", 3_400_000_000, "Build Data", Verdict::Safe, "Build products and index, Xcode rebuilds them", Who::Rule),
        mk(Group::AppCaches, "Adobe Media Cache", "~/Library/Application Support/Adobe/Common/Media Cache Files", 2_100_000_000, "Caches", Verdict::Safe, "Premiere/After Effects media cache", Who::Rule),
        mk(Group::Developer, "npm cache", "~/.npm/_cacache", 640_000_000, "Caches", Verdict::Safe, "npm downloads, fetched again when needed", Who::Ai),
    ];
    cleanup::State { phase: Phase::Results, items, ai_used: true, ..Default::default() }
}
