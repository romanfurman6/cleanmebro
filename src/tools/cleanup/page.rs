//! Cleanup screens: intro → scanning → Claude reviewing paths → results (with
//! "needs your OK" questions) → cleaning → done.

use super::engine::{self as cleanup, Group, Item, Phase, Verdict, Who};
use super::Cleanup as App;
use crate::kit::controls::round_button;
use crate::kit::glass::{self, *};
use egui::{Align2, Color32, Pos2, Rect, ScrollArea, Sense, Stroke, Ui, UiBuilder, Vec2};
use std::path::PathBuf;

pub fn page(app: &mut App, ui: &mut Ui, r: Rect) {
    let phase = app.clean.lock().unwrap().phase.clone();
    match phase {
        Phase::Intro => intro(app, ui, r),
        Phase::Scanning | Phase::Judging | Phase::Cleaning => working(app, ui, r, phase),
        Phase::Results => results(app, ui, r),
        Phase::Done => done(app, ui, r),
    }
}

fn intro(app: &mut App, ui: &mut Ui, r: Rect) {
    let ctx = ui.ctx().clone();
    let p = ui.painter().clone();
    let t = ctx.input(|i| i.time);
    let left = Rect::from_min_size(r.min, Vec2::new(r.width() * 0.5, r.height() - 120.0));
    orb(&p, left.center(), 130.0, Color32::from_rgb(40, 190, 110), Icon::Broom, t);
    let x = r.left() + r.width() * 0.52;
    let mut y = r.top() + 110.0;
    p.text(Pos2::new(x, y), Align2::LEFT_TOP, "Cleanup", bold(44.0), TEXT);
    y += 60.0;
    y += paragraph(&p, Pos2::new(x, y), "Clean your system to achieve maximum performance and reclaim free space.", font(17.0), TEXT_DIM, r.right() - x - 20.0) + 26.0;
    let bullets = [
        (Icon::Sparkle, "AI judge", "Claude looks at each path (never the contents) and decides what's safe."),
        (Icon::Question, "You stay in control", "Anything unclear waits for your OK. Projects you use are never touched."),
        (Icon::Broom, "Knows popular apps", "Rules for Xcode, Adobe, Spotify, Electron apps, Claude, npm, Gradle and more."),
    ];
    for (ic, title, body) in bullets {
        tile_icon(&p, Rect::from_min_size(Pos2::new(x, y), Vec2::splat(34.0)), ic, Color32::from_rgb(100, 170, 255), Color32::from_rgb(10, 110, 235));
        p.text(Pos2::new(x + 48.0, y - 1.0), Align2::LEFT_TOP, title, bold(15.5), TEXT);
        y += paragraph(&p, Pos2::new(x + 48.0, y + 21.0), body, font(14.0), TEXT_DIM, r.right() - x - 70.0) + 36.0;
    }
    // AI toggle
    let tr = Rect::from_min_size(Pos2::new(x, y + 4.0), Vec2::new(18.0, 18.0));
    if checkbox(ui, tr, ui.id().with("use-ai"), app.use_ai, true).clicked() {
        app.use_ai = !app.use_ai;
    }
    let has_claude = crate::kit::claude_code::claude_bin().is_some() || app.demo;
    p.text(Pos2::new(x + 28.0, y + 13.0), Align2::LEFT_CENTER, if has_claude { "Let Claude judge what's safe (only paths, sizes and dates are shared)" } else { "Claude Code not found — the built-in rules will be used" }, font(13.5), TEXT_DIM);
    if round_button(ui, Pos2::new(r.center().x, r.bottom() - 54.0), "Scan", None, true) {
        cleanup::start_scan(app.clean.clone(), ctx, app.use_ai && has_claude);
    }
}

fn working(app: &mut App, ui: &mut Ui, r: Rect, phase: Phase) {
    let ctx = ui.ctx().clone();
    let st = app.clean.lock().unwrap().clone();
    let p = ui.painter().clone();
    let t = ctx.input(|i| i.time);
    // big green card like the original's "Looking for junk…" tile
    let card_r = Rect::from_min_size(r.min, Vec2::new(r.width() * 0.62, r.height() - 130.0));
    glass_card(&p, card_r, 22.0, 0.3, false);
    let title = match phase {
        Phase::Judging => "Asking Claude what's safe…",
        Phase::Cleaning => "Cleaning…",
        _ => "Looking for junk…",
    };
    p.text(Pos2::new(card_r.center().x, card_r.top() + 60.0), Align2::CENTER_CENTER, title, bold(40.0), TEXT);
    let (oc, glyph) = match phase {
        Phase::Judging => (AMBER, Icon::Sparkle),
        _ => (GREEN, Icon::Broom),
    };
    orb(&p, card_r.center() + Vec2::new(0.0, 16.0), 104.0, oc, glyph, t);
    let line = match phase {
        Phase::Judging => st.ai_status.clone(),
        _ => st.current.replace(&dirs::home_dir().unwrap_or_default().display().to_string(), "~"),
    };
    text_fit(&p, Pos2::new(card_r.center().x, card_r.bottom() - 46.0), Align2::CENTER_CENTER, &middle_ellipsis(&line, 70), font(16.0), TEXT_DIM, card_r.width() - 40.0);
    if phase == Phase::Judging {
        p.text(Pos2::new(card_r.center().x, card_r.bottom() - 20.0), Align2::CENTER_CENTER, "Only paths, sizes and dates are shared — never file contents.", font(12.5), TEXT_FAINT);
    }
    // right column: what's been found so far, per group
    let col = Rect::from_min_max(Pos2::new(card_r.right() + 16.0, r.top()), Pos2::new(r.right(), card_r.bottom()));
    let found = if phase == Phase::Cleaning { 0 } else { st.items.len() };
    let mut y = col.top();
    let groups = [Group::Claude, Group::UserCache, Group::AppCaches, Group::Xcode];
    let h = (col.height() - 3.0 * 12.0) / 4.0;
    for g in groups {
        let gr = Rect::from_min_size(Pos2::new(col.left(), y), Vec2::new(col.width(), h));
        glass_card(&p, gr, 22.0, 0.0, false);
        p.text(gr.min + Vec2::new(20.0, 20.0), Align2::LEFT_TOP, g.title(), bold(17.0), TEXT);
        let (c1, c2) = g.colors();
        tile_icon(&p, Rect::from_center_size(gr.center() + Vec2::new(0.0, 14.0), Vec2::splat(54.0)), g.icon(), c1, c2);
        if found > 0 {
            p.text(gr.right_bottom() + Vec2::new(-18.0, -14.0), Align2::RIGHT_BOTTOM, human_bytes(st.group_bytes(g)), bold(15.0), TEXT);
        }
        y += h + 12.0;
    }
    let prog = if phase == Phase::Scanning && st.progress < 0.0 { None } else { Some(st.progress) };
    if round_button(ui, Pos2::new(r.center().x, r.bottom() - 54.0), "Stop", prog, phase != Phase::Cleaning) {
        app.clean.lock().unwrap().stop = true;
    }
    ctx.request_repaint();
}

fn results(app: &mut App, ui: &mut Ui, r: Rect) {
    let ctx = ui.ctx().clone();
    let p = ui.painter().clone();
    let (total, pending, ai_used, ai_error, selected) = {
        let g = app.clean.lock().unwrap();
        (g.total(), g.pending_questions(), g.ai_used, g.ai_error.clone(), g.selected_bytes())
    };
    // header
    if total == 0 {
        p.text(Pos2::new(r.center().x, r.top() + 200.0), Align2::CENTER_CENTER, "Your Mac is pure!", bold(36.0), TEXT);
        p.text(Pos2::new(r.center().x, r.top() + 244.0), Align2::CENTER_CENTER, "There's nothing to be cleaned from your system.", font(16.0), TEXT_DIM);
        if round_button(ui, Pos2::new(r.center().x, r.bottom() - 54.0), "Done", None, true) {
            app.clean.lock().unwrap().phase = Phase::Intro;
        }
        return;
    }
    let g1 = p.layout_no_wrap("There are ".into(), bold(26.0), TEXT);
    let g2 = p.layout_no_wrap(human_bytes(total), bold(26.0), CYAN);
    let g3 = p.layout_no_wrap(" of junk files on your Mac.".into(), bold(26.0), TEXT);
    let mut x = r.left() + 4.0;
    for (g, c) in [(g1, TEXT), (g2, CYAN), (g3, TEXT)] {
        let w = g.size().x;
        p.galley(Pos2::new(x, r.top() + 2.0), g, c);
        x += w;
    }
    let sub = match (&ai_error, ai_used) {
        (Some(e), _) => e.clone(),
        (None, true) => "Claude reviewed every path. Items it wasn't sure about are waiting for your OK.".into(),
        (None, false) => "Judged by the built-in rules.".into(),
    };
    p.text(Pos2::new(r.left() + 4.0, r.top() + 40.0), Align2::LEFT_TOP, sub, font(14.0), if ai_error.is_some() { AMBER } else { TEXT_DIM });

    // left: groups
    let lr = Rect::from_min_max(Pos2::new(r.left(), r.top() + 76.0), Pos2::new(r.left() + 300.0, r.bottom() - 110.0));
    let mut y = lr.top();
    let mut rows: Vec<(Option<Group>, String, u64, usize)> = vec![];
    if pending > 0 {
        rows.push((None, "Needs your OK".into(), 0, pending));
    }
    {
        let g = app.clean.lock().unwrap();
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
    for (grp, title, bytes, q) in &rows {
        let rr = Rect::from_min_size(Pos2::new(lr.left(), y), Vec2::new(lr.width(), 52.0));
        let sel = app.clean_group == *grp;
        let id = ui.id().with(("grp", title.as_str()));
        let resp = ui.interact(rr, id, Sense::click());
        let hv = ctx.animate_bool(id, resp.hovered());
        if sel {
            glass_card(&p, rr, 14.0, 0.7, false);
        } else if hv > 0.0 {
            p.rect_filled(rr, 14.0, white((14.0 * hv) as u8));
        }
        let (c1, c2) = grp.map(|g| g.colors()).unwrap_or((Color32::from_rgb(255, 210, 110), Color32::from_rgb(240, 140, 30)));
        let ic = grp.map(|g| g.icon()).unwrap_or(Icon::Question);
        tile_icon(&p, Rect::from_min_size(rr.min + Vec2::new(10.0, 10.0), Vec2::splat(32.0)), ic, c1, c2);
        p.text(rr.min + Vec2::new(54.0, 26.0), Align2::LEFT_CENTER, title, bold(15.0), TEXT);
        if grp.is_some() {
            p.text(Pos2::new(rr.right() - 12.0, rr.center().y), Align2::RIGHT_CENTER, human_bytes(*bytes), font(14.0), TEXT_DIM);
        }
        if *q > 0 {
            let badge_x = if grp.is_some() { rr.right() - 92.0 } else { rr.right() - 30.0 };
            let br = Rect::from_center_size(Pos2::new(badge_x, rr.center().y), Vec2::new(28.0, 20.0));
            p.rect_filled(br, 10.0, AMBER);
            p.text(br.center(), Align2::CENTER_CENTER, q.to_string(), bold(12.5), Color32::from_rgb(60, 30, 0));
        }
        if resp.clicked() {
            app.clean_group = *grp;
        }
        y += 56.0;
    }

    // right: items of the chosen group
    let rr = Rect::from_min_max(Pos2::new(lr.right() + 18.0, r.top() + 76.0), Pos2::new(r.right(), r.bottom() - 110.0));
    glass_card(&p, rr, 20.0, 0.0, false);
    let (title, blurb) = match app.clean_group {
        Some(g) => (g.title(), g.blurb()),
        None => ("Needs your OK", "Claude wasn't sure these are safe. Allow each one you're happy to remove, or keep it."),
    };
    p.text(rr.min + Vec2::new(20.0, 16.0), Align2::LEFT_TOP, title, bold(20.0), TEXT);
    let bh = paragraph(&p, rr.min + Vec2::new(20.0, 46.0), blurb, font(13.5), TEXT_DIM, rr.width() - 40.0);
    // answer every open question at once
    if app.clean_group.is_none() {
        let all = rr.right_top() + Vec2::new(-20.0, 20.0);
        let mut answer: Option<Verdict> = None;
        if link(ui, all, Align2::RIGHT_TOP, "Allow all", 13.5).clicked() {
            answer = Some(Verdict::Safe);
        }
        if link(ui, all + Vec2::new(-90.0, 0.0), Align2::RIGHT_TOP, "Keep all", 13.5).clicked() {
            answer = Some(Verdict::Keep);
        }
        if let Some(v) = answer {
            for it in app.clean.lock().unwrap().items.iter_mut().filter(|i| i.verdict == Verdict::Ask && i.by != Who::You) {
                it.verdict = v;
                it.by = Who::You;
                it.selected = v == Verdict::Safe;
                it.reason = if v == Verdict::Safe { "You allowed it".into() } else { "You chose to keep it".into() };
            }
        }
    }
    // select all / none
    if let Some(g) = app.clean_group {
        let all = rr.right_top() + Vec2::new(-20.0, 20.0);
        if link(ui, all, Align2::RIGHT_TOP, "Select safe", 13.5).clicked() {
            for it in app.clean.lock().unwrap().items.iter_mut().filter(|i| i.group == g) {
                it.selected = it.verdict == Verdict::Safe && !it.permanent;
            }
        }
        if link(ui, all + Vec2::new(-96.0, 0.0), Align2::RIGHT_TOP, "None", 13.5).clicked() {
            for it in app.clean.lock().unwrap().items.iter_mut().filter(|i| i.group == g) {
                it.selected = false;
            }
        }
    }
    let list = Rect::from_min_max(rr.min + Vec2::new(10.0, 56.0 + bh + 8.0), rr.max - Vec2::new(10.0, 10.0));
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
        for i in idx {
            item_row(app, ui, i, list.width() - 8.0);
        }
    });

    // bottom bar
    let by = r.bottom() - 54.0;
    p.text(Pos2::new(r.left() + 4.0, by - 12.0), Align2::LEFT_CENTER, format!("Selected: {}", human_bytes(selected)), bold(16.0), TEXT);
    let cr = Rect::from_min_size(Pos2::new(r.left() + 4.0, by + 8.0), Vec2::splat(16.0));
    if checkbox(ui, cr, ui.id().with("delete-now"), app.delete_now, true).clicked() {
        app.delete_now = !app.delete_now;
    }
    p.text(Pos2::new(r.left() + 28.0, by + 16.0), Align2::LEFT_CENTER, if app.delete_now { "Delete right away (can't be undone)" } else { "Delete right away (otherwise moved to the Trash)" }, font(13.0), TEXT_DIM);
    let br = Rect::from_min_size(Pos2::new(r.right() - 150.0, by - 16.0), Vec2::new(150.0, 34.0));
    if pill_button(ui, br, "Scan Again", 0, true).clicked() {
        cleanup::start_scan(app.clean.clone(), ctx.clone(), app.use_ai);
    }
    if round_button(ui, Pos2::new(r.center().x + 40.0, by), "Clean", None, selected > 0) {
        app.clean_confirm = true;
    }
    if app.clean_confirm {
        confirm(app, ui, r);
    }
}

fn verdict_style(it: &Item) -> (Color32, Icon) {
    match it.verdict {
        Verdict::Safe => (GREEN, Icon::Check),
        Verdict::Ask => (AMBER, Icon::Question),
        Verdict::Keep => (Color32::from_rgb(170, 170, 200), Icon::Lock),
    }
}

fn item_row(app: &mut App, ui: &mut Ui, i: usize, w: f32) {
    let it = app.clean.lock().unwrap().items[i].clone();
    let asking = it.verdict == Verdict::Ask && it.by != Who::You;
    let h = if asking { 88.0 } else { 64.0 };
    let (rect, resp) = ui.allocate_exact_size(Vec2::new(w, h), Sense::click());
    let p = ui.painter().clone();
    if resp.hovered() {
        p.rect_filled(rect, 10.0, white(10));
    }
    let can_select = it.verdict != Verdict::Keep && !it.hard_keep;
    let cb = Rect::from_min_size(rect.min + Vec2::new(8.0, 12.0), Vec2::splat(17.0));
    let cr = checkbox(ui, cb, ui.id().with(("sel", &it.path)), it.selected, can_select);
    if cr.clicked() && can_select {
        let mut g = app.clean.lock().unwrap();
        let x = &mut g.items[i];
        x.selected = !x.selected;
        if x.selected && x.verdict == Verdict::Ask {
            x.verdict = Verdict::Safe;
            x.by = Who::You;
        }
    }
    let home = dirs::home_dir().unwrap_or_default().display().to_string();
    let path = it.path.display().to_string().replacen(&home, "~", 1);
    text_fit(&p, rect.min + Vec2::new(36.0, 9.0), Align2::LEFT_TOP, &it.title, bold(14.5), TEXT, w - 300.0);
    text_fit(&p, rect.min + Vec2::new(36.0, 29.0), Align2::LEFT_TOP, &middle_ellipsis(&path, 64), font(12.0), TEXT_FAINT, w - 200.0);
    let (vc, vi) = verdict_style(&it);
    let who = match it.by {
        Who::Ai => "Claude: ",
        Who::You => "You: ",
        Who::Rule => "",
    };
    icon(&p, vi, Rect::from_min_size(rect.min + Vec2::new(36.0, 46.0), Vec2::splat(13.0)), vc);
    text_fit(&p, rect.min + Vec2::new(54.0, 45.0), Align2::LEFT_TOP, &format!("{who}{}", it.reason), font(12.5), lerp_color(vc, TEXT, 0.35), w - 150.0);
    p.text(Pos2::new(rect.right() - 10.0, rect.top() + 18.0), Align2::RIGHT_CENTER, human_bytes(it.bytes), bold(14.0), TEXT);
    // badge
    let label = if it.permanent { "Permanent" } else { it.kind };
    let g = p.layout_no_wrap(label.to_string(), font(11.0), TEXT_DIM);
    let br = Rect::from_min_size(Pos2::new(rect.right() - 10.0 - g.size().x - 14.0, rect.top() + 34.0), g.size() + Vec2::new(14.0, 6.0));
    p.rect_filled(br, br.height() / 2.0, white(18));
    p.galley(br.min + Vec2::new(7.0, 3.0), g, TEXT_DIM);
    if asking {
        let y = rect.top() + 62.0;
        let a = Rect::from_min_size(Pos2::new(rect.left() + 36.0, y), Vec2::new(84.0, 22.0));
        let k = Rect::from_min_size(Pos2::new(rect.left() + 128.0, y), Vec2::new(84.0, 22.0));
        if pill_button(ui, a, "Allow", 1, true).clicked() {
            let mut g = app.clean.lock().unwrap();
            let x = &mut g.items[i];
            x.verdict = Verdict::Safe;
            x.by = Who::You;
            x.selected = true;
            x.reason = "You allowed it".into();
        }
        if pill_button(ui, k, "Keep", 0, true).clicked() {
            let mut g = app.clean.lock().unwrap();
            let x = &mut g.items[i];
            x.verdict = Verdict::Keep;
            x.by = Who::You;
            x.selected = false;
            x.reason = "You chose to keep it".into();
        }
    }
    p.line_segment([Pos2::new(rect.left() + 36.0, rect.bottom()), Pos2::new(rect.right() - 6.0, rect.bottom())], Stroke::new(1.0, white(14)));
    if resp.double_clicked() {
        let _ = std::process::Command::new("open").arg("-R").arg(&it.path).spawn();
    }
}

fn confirm(app: &mut App, ui: &mut Ui, r: Rect) {
    let ctx = ui.ctx().clone();
    let p = ui.painter().clone();
    p.rect_filled(r.expand(30.0), 0.0, Color32::from_black_alpha(120));
    let (n, bytes, perm) = {
        let g = app.clean.lock().unwrap();
        let sel: Vec<&Item> = g.items.iter().filter(|i| i.selected).collect();
        (sel.len(), sel.iter().map(|i| i.bytes).sum::<u64>(), sel.iter().any(|i| i.permanent))
    };
    let m = Rect::from_center_size(r.center(), Vec2::new(460.0, 230.0));
    glass_card(&p, m, 22.0, 0.3, false);
    p.rect_filled(m, 22.0, Color32::from_rgba_unmultiplied(28, 28, 32, 225));
    p.text(m.min + Vec2::new(24.0, 22.0), Align2::LEFT_TOP, format!("Clean {} items ({})?", n, human_bytes(bytes)), bold(20.0), TEXT);
    let mut body = if app.delete_now { "They will be deleted right away. This can't be undone.".to_string() } else { "They'll be moved to the Trash, so you can put anything back.".to_string() };
    if perm {
        body.push_str(" Emptying the Trash is permanent.");
    }
    body.push_str(" Quit the apps involved first for the best result.");
    paragraph(&p, m.min + Vec2::new(24.0, 60.0), &body, font(14.5), TEXT_DIM, m.width() - 48.0);
    let cancel = Rect::from_min_size(Pos2::new(m.right() - 260.0, m.bottom() - 56.0), Vec2::new(110.0, 34.0));
    let ok = Rect::from_min_size(Pos2::new(m.right() - 136.0, m.bottom() - 56.0), Vec2::new(112.0, 34.0));
    if pill_button(ui, cancel, "Cancel", 0, true).clicked() {
        app.clean_confirm = false;
    }
    if pill_button(ui, ok, "Clean", if app.delete_now || perm { 2 } else { 1 }, true).clicked() {
        app.clean_confirm = false;
        cleanup::start_clean(app.clean.clone(), ctx, app.delete_now);
    }
}

fn done(app: &mut App, ui: &mut Ui, r: Rect) {
    let ctx = ui.ctx().clone();
    let p = ui.painter().clone();
    let st = app.clean.lock().unwrap().clone();
    let t = ctx.input(|i| i.time);
    let ok = st.failed.is_empty();
    orb(&p, Pos2::new(r.center().x, r.top() + 170.0), 110.0, if ok { Color32::from_rgb(30, 180, 90) } else { Color32::from_rgb(220, 140, 40) }, if ok { Icon::Check } else { Icon::Question }, t);
    p.text(Pos2::new(r.center().x, r.top() + 330.0), Align2::CENTER_CENTER, if ok { "Cleanup complete!" } else { "Cleanup finished with some issues" }, bold(36.0), TEXT);
    p.text(Pos2::new(r.center().x, r.top() + 374.0), Align2::CENTER_CENTER, format!("{} successfully cleaned · {} items", human_bytes(st.cleaned_bytes), st.cleaned_count), font(17.0), TEXT_DIM);
    if !ok {
        let txt = format!("{} items were not removed. Please make sure you quit all of the related applications and try again.", st.failed.len());
        paragraph(&p, Pos2::new(r.center().x - 260.0, r.top() + 404.0), &txt, font(14.0), AMBER, 520.0);
    }
    if !app.delete_now && st.cleaned_bytes > 0 {
        paragraph(&p, Pos2::new(r.center().x - 260.0, r.top() + 450.0), "Removed items are in the Trash. Empty it to get the space back.", font(14.0), TEXT_DIM, 520.0);
    }
    let lr = Rect::from_center_size(Pos2::new(r.center().x - 130.0, r.bottom() - 54.0), Vec2::new(150.0, 34.0));
    if pill_button(ui, lr, if app.clean_log { "Hide Log" } else { "Cleanup Log" }, 0, true).clicked() {
        app.clean_log = !app.clean_log;
    }
    if round_button(ui, Pos2::new(r.center().x + 60.0, r.bottom() - 54.0), "Done", None, true) {
        app.clean.lock().unwrap().phase = Phase::Intro;
        app.clean_log = false;
    }
    if app.clean_log {
        let lr = Rect::from_min_max(Pos2::new(r.left() + 40.0, r.top() + 480.0), Pos2::new(r.right() - 40.0, r.bottom() - 100.0));
        glass_card(&p, lr, 14.0, 0.0, false);
        let mut child = ui.new_child(UiBuilder::new().max_rect(lr.shrink(12.0)).layout(egui::Layout::top_down(egui::Align::Min)));
        ScrollArea::vertical().id_salt("log").auto_shrink([false, false]).show(&mut child, |ui| {
            for l in &st.log {
                ui.label(egui::RichText::new(l).font(glass::mono(11.5)).color(TEXT_DIM));
            }
        });
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
        mk(Group::Claude, "Claude Desktop Cache", "~/Library/Application Support/Claude/Cache", 678_000_000, "Caches", Verdict::Safe, "Web view cache; rebuilt automatically", Who::Ai),
        mk(Group::Claude, "Claude Desktop VM image", "~/Library/Application Support/Claude/vm_bundles", 9_000_000_000, "Downloads", Verdict::Ask, "Agent sandbox image; large to download again", Who::Ai),
        mk(Group::Claude, "Project · old-experiment", "~/.claude/projects/-Users-me-old-experiment", 412_000_000, "Old Sessions", Verdict::Ask, "Chat history for a folder that no longer exists", Who::Ai),
        mk(Group::Claude, "Project · cleanmebro", "~/.claude/projects/-Users-me-Projects-cleanmebro", 96_000_000, "Projects", Verdict::Keep, "You used this project today — kept", Who::Rule),
        mk(Group::Claude, "Claude Desktop logs", "~/Library/Logs/Claude", 42_000_000, "Logs", Verdict::Safe, "Claude Desktop logs", Who::Ai),
        mk(Group::Claude, "Claude CLI logs", "~/Library/Caches/claude-cli-nodejs", 18_000_000, "Logs", Verdict::Safe, "Claude Code's own error/MCP logs", Who::Ai),
        mk(Group::UserCache, "Spotify", "~/Library/Caches/com.spotify.client", 1_200_000_000, "Caches", Verdict::Safe, "Streaming cache; rebuilt automatically", Who::Ai),
        mk(Group::UserCache, "Google Chrome", "~/Library/Caches/Google", 820_000_000, "Caches", Verdict::Safe, "Browser cache; rebuilt automatically", Who::Ai),
        mk(Group::Xcode, "Derived Data · MyApp", "~/Library/Developer/Xcode/DerivedData/MyApp-abc", 3_400_000_000, "Build Data", Verdict::Safe, "Build products and index; Xcode rebuilds them", Who::Rule),
        mk(Group::AppCaches, "Adobe Media Cache", "~/Library/Application Support/Adobe/Common/Media Cache Files", 2_100_000_000, "Caches", Verdict::Safe, "Premiere/After Effects media cache", Who::Rule),
        mk(Group::Developer, "npm cache", "~/.npm/_cacache", 640_000_000, "Caches", Verdict::Safe, "npm downloads; fetched again when needed", Who::Ai),
    ];
    cleanup::State { phase: Phase::Results, items, ai_used: true, ..Default::default() }
}
