//! Startup on screen: the window page (suggestions, Undo, the list by
//! where things come from) and the panel widget at 1:1, 1:2 and 2:4.

use super::items::{Job, LoginApp, Scope, Why};
use super::{plural, Filter, Startup};
use crate::core::tool::{Cx, Size};
use crate::kit::glass::*;
use crate::kit::{macos, tile};
use egui::{Align2, Color32, Pos2, Rect, ScrollArea, Sense, Stroke, StrokeKind, Ui, UiBuilder, Vec2};
use std::path::{Path, PathBuf};

const ROW_H: f32 = 44.0;
const ROW_GAP: f32 = 2.0;
const HEAD_H: f32 = 34.0;
/// Icons are loaded a few per frame so a long list doesn't stall.
const ICONS_PER_FRAME: usize = 4;
/// Faint red behind suggested rows and the suggestion card.
fn red_wash(a: f32) -> Color32 {
    with_alpha(tint::STARTUP, a)
}
const SUGGEST_TEXT: Color32 = Color32::from_rgb(0xFF, 0x8A, 0x80);
const ORPHAN_TEXT: Color32 = Color32::from_rgb(0xF2, 0xC4, 0x6B);

fn letter_color(name: &str) -> Color32 {
    const PALETTE: [Color32; 8] = [
        Color32::from_rgb(0x1F, 0x7A, 0xE0),
        Color32::from_rgb(0x8E, 0x44, 0xAD),
        Color32::from_rgb(0x24, 0x8A, 0x3D),
        Color32::from_rgb(0xC7, 0x80, 0x1F),
        Color32::from_rgb(0xC0, 0x39, 0x2B),
        Color32::from_rgb(0x2E, 0x9E, 0x8F),
        Color32::from_rgb(0x5B, 0x2C, 0x6F),
        Color32::from_rgb(0x48, 0x48, 0x4E),
    ];
    let h = name.bytes().fold(5381u32, |h, b| h.wrapping_mul(33) ^ b as u32);
    PALETTE[(h % PALETTE.len() as u32) as usize]
}

/// The owning app's icon (macOS), else a colored square with a letter.
#[allow(clippy::too_many_arguments)]
fn app_icon(s: &mut Startup, ctx: &egui::Context, p: &egui::Painter, r: Rect, path: Option<&Path>, name: &str, dim: bool, budget: &mut usize) {
    let a = if dim { 0.5 } else { 1.0 };
    if let Some(path) = path {
        if !s.icons.contains_key(path) && *budget > 0 && !s.demo {
            *budget -= 1;
            let tex = macos::app_icon(path, 64).map(|(n, rgba)| ctx.load_texture(format!("startup-icon:{}", path.display()), egui::ColorImage::from_rgba_unmultiplied([n, n], &rgba), egui::TextureOptions::LINEAR));
            s.icons.insert(path.to_path_buf(), tex);
            ctx.request_repaint();
        }
        if let Some(Some(t)) = s.icons.get(path) {
            p.image(t.id(), r, Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)), with_alpha(Color32::WHITE, a));
            return;
        }
    }
    p.rect_filled(r, (r.width() * 0.27).round(), with_alpha(letter_color(name), a));
    let l: String = name.chars().next().map(|c| c.to_uppercase().collect()).unwrap_or_default();
    p.text(r.center(), Align2::CENTER_CENTER, l, bold(r.width() * 0.5), with_alpha(Color32::WHITE, a));
}

fn outline_chip(p: &egui::Painter, pos: Pos2, align: Align2, label: &str, col: Color32) -> Rect {
    let g = p.layout_no_wrap(label.to_string(), font(10.0), col);
    let r = align.anchor_size(pos, g.size() + Vec2::new(10.0, 2.0));
    p.rect_stroke(r, 4.0, Stroke::new(1.0, with_alpha(col, 0.45)), StrokeKind::Inside);
    p.galley(r.center() - g.size() / 2.0, g, col);
    r
}

fn why_color(w: Why) -> Color32 {
    if w == Why::Removed { ORPHAN_TEXT } else { SUGGEST_TEXT }
}

/// An app in /Applications that the Uninstaller can show.
fn uninstallable(owner: &Option<PathBuf>) -> Option<&PathBuf> {
    owner.as_ref().filter(|o| {
        let home_apps = crate::kit::fs::home().join("Applications");
        o.starts_with("/Applications") || o.starts_with(&home_apps)
    })
}

// ---------------------------------------------------------------- page

enum Row {
    Head(&'static str, &'static str, bool),
    App(LoginApp),
    Job(Job),
    Note(String),
}

pub fn page(s: &mut Startup, ui: &mut Ui, r: Rect, cx: &mut Cx) {
    let ctx = ui.ctx().clone();
    let p = ui.painter().clone();
    s.load_apps();
    let (loaded, jobs, apps, apps_loaded, apps_err, new_labels, undo, busy, notify) = {
        let g = s.state();
        (g.jobs_loaded, g.jobs.clone(), g.apps.clone(), g.apps_loaded, g.apps_error.clone(), g.new_labels.clone(), g.undoable(), g.busy.clone(), g.settings.notify_new)
    };
    let total = jobs.len() + apps.len();
    let running = jobs.iter().filter(|j| j.pid.is_some()).count();
    let suggested: Vec<&Job> = jobs.iter().filter(|j| j.suggested()).collect();
    let off = jobs.iter().filter(|j| !j.on).count() + apps.iter().filter(|a| !a.on).count();
    let sub = if !loaded {
        "Reading what starts with your Mac…".to_string()
    } else {
        format!("{} start with your Mac · {running} running now", plural(total, "thing"))
    };
    let btns = page_header(&p, r, "Startup", &sub, TEXT_DIM);

    // filter + search on the right of the header
    let labels_owned = [format!("All {total}"), format!("Suggested {}", suggested.len()), format!("Off {off}")];
    let labels: Vec<&str> = labels_owned.iter().map(|x| x.as_str()).collect();
    let tr = Rect::from_min_max(Pos2::new(btns.right() - 300.0, btns.top()), btns.max);
    let cur = match s.filter {
        Filter::All => 0,
        Filter::Suggested => 1,
        Filter::Off => 2,
    };
    if let Some(i) = segmented(ui, tr, ui.scope_id().with("su-filter"), &labels, cur) {
        s.filter = [Filter::All, Filter::Suggested, Filter::Off][i];
    }
    let sr = Rect::from_min_max(Pos2::new(tr.left() - 12.0 - 190.0, tr.top()), Pos2::new(tr.left() - 12.0, tr.bottom()));
    let mut q = std::mem::take(&mut s.search);
    search_field(ui, sr, &mut q, "Search", false, 13.0);
    s.search = q;

    let body = Rect::from_min_max(Pos2::new(r.left() + 24.0, r.top() + PAGE_HEADER_H + 1.0), Pos2::new(r.right() - 24.0, r.bottom()));
    if !loaded {
        spinner(&p, Pos2::new(body.left() + 8.0, body.top() + 30.0), 7.0, ctx.input(|i| i.time), ACCENT);
        p.text(Pos2::new(body.left() + 24.0, body.top() + 30.0), Align2::LEFT_CENTER, "Reading what starts with your Mac…", font(13.0), TEXT_DIM);
        crate::kit::glass::animate(&ctx);
        return;
    }

    // what passes the filter and the search
    let needle = s.search.trim().to_lowercase();
    let hit = |name: &str, label: &str| needle.is_empty() || name.to_lowercase().contains(&needle) || label.to_lowercase().contains(&needle);
    let job_ok = |j: &Job| {
        hit(&j.name, &j.label)
            && match s.filter {
                Filter::All => true,
                Filter::Suggested => j.suggested(),
                Filter::Off => !j.on,
            }
    };
    let app_ok = |a: &LoginApp| {
        hit(&a.name, &a.path.to_string_lossy())
            && match s.filter {
                Filter::All => true,
                Filter::Suggested => false,
                Filter::Off => !a.on,
            }
    };
    let mut rows = Vec::new();
    if s.filter != Filter::Suggested {
        let a: Vec<&LoginApp> = apps.iter().filter(|a| app_ok(a)).collect();
        if !a.is_empty() || (s.filter == Filter::All && needle.is_empty()) {
            rows.push(Row::Head("OPEN AT LOGIN", "Apps macOS opens when you log in", true));
            match (&apps_err, apps_loaded) {
                (Some(e), _) => rows.push(Row::Note(format!("Couldn't read the list ({e}). Allow CleanMeBro to control System Events in System Settings › Privacy & Security › Automation."))),
                (None, false) => rows.push(Row::Note("Reading…".into())),
                (None, true) if a.is_empty() => rows.push(Row::Note("No apps open at login.".into())),
                _ => rows.extend(a.into_iter().cloned().map(Row::App)),
            }
        }
    }
    for (scopes, title, hint) in [
        (&[Scope::User, Scope::AllUsers][..], "BACKGROUND AGENTS", "~/Library/LaunchAgents and /Library/LaunchAgents"),
        (&[Scope::System][..], "SYSTEM DAEMONS", "/Library/LaunchDaemons · run as admin, ask your password"),
    ] {
        let js: Vec<&Job> = jobs.iter().filter(|j| scopes.contains(&j.scope) && job_ok(j)).collect();
        if js.is_empty() {
            continue;
        }
        rows.push(Row::Head(title, hint, false));
        rows.extend(js.into_iter().cloned().map(Row::Job));
    }
    if rows.is_empty() {
        rows.push(Row::Note(match s.filter {
            Filter::Suggested => "Nothing to turn off. Everything that starts with your Mac looks needed.".into(),
            Filter::Off => "Nothing is turned off.".into(),
            Filter::All => "Nothing matches.".into(),
        }));
    }

    let top_h = if busy.is_some() || undo.is_some() || (!suggested.is_empty() && s.filter != Filter::Off) { 70.0 } else { 0.0 };
    let list_h: f32 = rows.iter().map(|r| match r {
        Row::Head(..) => HEAD_H,
        Row::Note(_) => 40.0,
        _ => ROW_H + ROW_GAP,
    }).sum::<f32>()
        + 56.0;
    let area = Rect::from_min_max(Pos2::new(body.left(), body.top() + 12.0), Pos2::new(body.right(), body.bottom() - 12.0));
    let mut child = ui.new_child(UiBuilder::new().max_rect(area));
    child.set_clip_rect(area);
    let mut flip_app: Option<LoginApp> = None;
    let mut flip_job: Option<Job> = None;
    let mut trash: Option<Job> = None;
    let mut turn_off_all = false;
    let mut do_undo = false;
    let mut set_notify: Option<bool> = None;
    let mut budget = ICONS_PER_FRAME;
    let w = area.width() - 12.0;
    ScrollArea::vertical().id_salt("su-list").auto_shrink([false, false]).show(&mut child, |ui| {
        let (a, _) = ui.allocate_exact_size(Vec2::new(w, top_h + list_h), Sense::hover());
        let p = ui.painter().clone();
        let mut y = a.top();

        // the card on top: working / Undo / suggestion
        if top_h > 0.0 {
            let cr = Rect::from_min_size(Pos2::new(a.left(), y), Vec2::new(a.width(), 58.0));
            if let Some(line) = &busy {
                glass_card(&p, cr, R_CARD, 0.0, false);
                spinner(&p, Pos2::new(cr.left() + 24.0, cr.center().y), 7.0, ctx.input(|i| i.time), ACCENT);
                p.text(Pos2::new(cr.left() + 42.0, cr.center().y), Align2::LEFT_CENTER, line, font(13.0), TEXT_SOFT);
            } else if let Some(n) = undo {
                p.rect_filled(cr, R_CARD, with_alpha(GREEN, 0.08));
                p.rect_stroke(cr, R_CARD, Stroke::new(1.0, with_alpha(GREEN, 0.35)), StrokeKind::Inside);
                let c = Pos2::new(cr.left() + 30.0, cr.center().y);
                p.circle_filled(c, 17.0, with_alpha(GREEN, 0.16));
                icon(&p, Icon::Check, Rect::from_center_size(c, Vec2::splat(18.0)), GREEN);
                let br = Rect::from_min_size(Pos2::new(cr.right() - 14.0 - 76.0, cr.center().y - 15.0), Vec2::new(76.0, 30.0));
                if pill_button(ui, br, "Undo", BTN_SECONDARY, true).clicked() {
                    do_undo = true;
                }
                p.text(Pos2::new(cr.left() + 58.0, cr.center().y - 9.0), Align2::LEFT_CENTER, format!("{} turned off", plural(n, "item")), bold(13.0), TEXT);
                text_fit(&p, Pos2::new(cr.left() + 58.0, cr.center().y + 9.0), Align2::LEFT_CENTER, "Nothing was removed. The apps still work when you open them.", font(12.0), TEXT_DIM, br.left() - cr.left() - 70.0);
            } else {
                p.rect_filled(cr, R_CARD, red_wash(0.10));
                p.rect_stroke(cr, R_CARD, Stroke::new(1.0, red_wash(0.35)), StrokeKind::Inside);
                tool_chip(&p, Rect::from_center_size(Pos2::new(cr.left() + 30.0, cr.center().y), Vec2::splat(34.0)), Icon::Power, tint::STARTUP);
                let label = format!("Turn off {}", suggested.len());
                let bw = button_width(&p, &label, BTN_PRIMARY);
                let br = Rect::from_min_size(Pos2::new(cr.right() - 14.0 - bw, cr.center().y - 15.0), Vec2::new(bw, 30.0));
                if pill_button(ui, br, &label, BTN_PRIMARY, true).clicked() {
                    turn_off_all = true;
                }
                let title = if suggested.len() == 1 { "1 item you likely don't need".to_string() } else { format!("{} items you likely don't need", suggested.len()) };
                p.text(Pos2::new(cr.left() + 58.0, cr.center().y - 9.0), Align2::LEFT_CENTER, title, bold(13.0), TEXT);
                text_fit(&p, Pos2::new(cr.left() + 58.0, cr.center().y + 9.0), Align2::LEFT_CENTER, &why_line(&suggested), font(12.0), TEXT_SOFT, br.left() - cr.left() - 70.0);
            }
            y += top_h;
        }

        for row in &rows {
            match row {
                Row::Head(t, hint, settings) => {
                    let tr = section_header(&p, Pos2::new(a.left() + 4.0, y + 22.0), Align2::LEFT_CENTER, t);
                    p.text(Pos2::new(tr.right() + 8.0, y + 22.0), Align2::LEFT_CENTER, *hint, font(11.0), TEXT_FAINT);
                    if *settings && link(ui, Pos2::new(a.right() - 4.0, y + 22.0), Align2::RIGHT_CENTER, "Open in System Settings", 12.0).clicked() {
                        super::items::open_settings();
                    }
                    y += HEAD_H;
                }
                Row::Note(t) => {
                    let r = Rect::from_min_size(Pos2::new(a.left(), y), Vec2::new(a.width(), 36.0));
                    glass_card(&p, r, 10.0, 0.0, false);
                    text_fit(&p, Pos2::new(r.left() + 14.0, r.center().y), Align2::LEFT_CENTER, t, font(12.0), TEXT_DIM, r.width() - 28.0);
                    y += 40.0;
                }
                Row::App(la) => {
                    let r = Rect::from_min_size(Pos2::new(a.left(), y), Vec2::new(a.width(), ROW_H));
                    glass_card(&p, r, 10.0, 0.0, false);
                    let ir = Rect::from_center_size(Pos2::new(r.left() + 27.0, r.center().y), Vec2::splat(26.0));
                    app_icon(s, &ctx, &p, ir, Some(&la.path), &la.name, !la.on, &mut budget);
                    let cols = columns(r);
                    text_fit(&p, Pos2::new(ir.right() + 10.0, r.center().y - 8.0), Align2::LEFT_CENTER, &la.name, bold(13.0), if la.on { TEXT } else { TEXT_DIM }, cols.when - ir.right() - 24.0);
                    p.text(Pos2::new(ir.right() + 10.0, r.center().y + 9.0), Align2::LEFT_CENTER, if la.hidden { "Opens hidden" } else { "Opens its window" }, font(11.0), TEXT_FAINT);
                    when_cell(&p, cols.when, r.center().y, Icon::Power, "at login");
                    if !la.on {
                        state_cell(&p, cols.state, r.center().y, Some("off"), false);
                    }
                    let sw = Rect::from_center_size(Pos2::new(r.right() - 30.0, r.center().y), Vec2::new(36.0, 20.0));
                    if switch(ui, sw, ui.scope_id().with(("su-app", &la.path)), la.on, busy.is_none()).on_hover_text(if la.on { "Don't open at login" } else { "Open at login" }).clicked() {
                        flip_app = Some(la.clone());
                    }
                    y += ROW_H + ROW_GAP;
                }
                Row::Job(j) => {
                    let r = Rect::from_min_size(Pos2::new(a.left(), y), Vec2::new(a.width(), ROW_H));
                    let why = j.why().filter(|_| j.on);
                    glass_card(&p, r, 10.0, 0.0, false);
                    if why.is_some() {
                        p.rect_filled(r, 10.0, red_wash(0.05));
                    }
                    let ir = Rect::from_center_size(Pos2::new(r.left() + 27.0, r.center().y), Vec2::splat(26.0));
                    app_icon(s, &ctx, &p, ir, j.owner.as_deref(), &j.name, !j.on, &mut budget);
                    let cols = columns(r);
                    let sw = Rect::from_center_size(Pos2::new(r.right() - 30.0, r.center().y), Vec2::new(36.0, 20.0));
                    let more = Rect::from_center_size(Pos2::new(sw.left() - 18.0, r.center().y), Vec2::splat(24.0));
                    let mut right_btn = more.left() - 6.0;
                    if j.orphan {
                        let rb = Rect::from_min_max(Pos2::new(right_btn - 74.0, r.center().y - 12.0), Pos2::new(right_btn, r.center().y + 12.0));
                        if pill_button(ui, rb, "Remove", BTN_SECONDARY, busy.is_none()).on_hover_text("Stop it and move its file to the Trash").clicked() {
                            trash = Some(j.clone());
                        }
                        right_btn = rb.left() - 8.0;
                    }
                    let name_max = cols.when - ir.right() - 24.0;
                    let nr = text_fit(&p, Pos2::new(ir.right() + 10.0, r.center().y - 8.0), Align2::LEFT_CENTER, &j.name, bold(13.0), if j.on { TEXT } else { TEXT_DIM }, name_max * 0.6);
                    let mut cx0 = nr.right() + 6.0;
                    let mut chip = |label: &str, col: Color32| {
                        let cr = outline_chip(&p, Pos2::new(cx0, r.center().y - 8.0), Align2::LEFT_CENTER, label, col);
                        cx0 = cr.right() + 4.0;
                    };
                    if let Some(w) = why {
                        chip(if w == Why::Removed { "app removed" } else { "suggested" }, why_color(w));
                    } else if j.orphan {
                        chip("app removed", ORPHAN_TEXT);
                    }
                    if new_labels.contains(&j.label) {
                        chip("new", ACCENT);
                    }
                    if j.admin() {
                        chip("admin", TEXT_DIM);
                    }
                    let detail = match (j.owner_name(), why) {
                        (_, Some(w)) if w != Why::Removed => format!("{} · {}", j.label, w.short()),
                        (Some(o), _) if o != j.name => format!("{} · {o}", j.label),
                        _ => j.label.clone(),
                    };
                    let dr = text_fit(&p, Pos2::new(ir.right() + 10.0, r.center().y + 9.0), Align2::LEFT_CENTER, &detail, font(11.0), TEXT_FAINT, name_max);
                    let tip = match &j.program {
                        Some(pr) => format!("{}\nRuns {}", j.plist.display(), pr.display()),
                        None => j.plist.display().to_string(),
                    };
                    ui.interact(dr, ui.scope_id().with(("su-tip", &j.label, j.scope as u8)), Sense::hover()).on_hover_text(tip);
                    if cols.state < right_btn {
                        when_cell(&p, cols.when, r.center().y, when_icon(j), &j.when.label(j.scope));
                        state_cell(&p, cols.state, r.center().y, if !j.on { Some("off") } else if j.pid.is_some() { None } else { Some("idle") }, j.on && j.pid.is_some());
                    }
                    let mr = icon_dots(ui, more, ui.scope_id().with(("su-more", &j.label, j.scope as u8)));
                    egui::Popup::menu(&mr).show(|ui| {
                        ui.set_min_width(220.0);
                        if let Some(app) = uninstallable(&j.owner) {
                            let n = app.file_stem().map(|x| x.to_string_lossy().into_owned()).unwrap_or_default();
                            if ui.button(format!("Open {n} in Uninstaller")).clicked() {
                                cx.open_page("uninstaller", &format!("app:{}", app.display()));
                            }
                        }
                        if ui.button("Show file in Finder").clicked() {
                            macos::reveal(&j.plist);
                        }
                        if ui.button("Copy label").clicked() {
                            ui.ctx().copy_text(j.label.clone());
                        }
                        ui.separator();
                        if ui.add_enabled(busy.is_none(), egui::Button::new(egui::RichText::new("Move file to Trash…").color(SUGGEST_TEXT))).clicked() {
                            trash = Some(j.clone());
                        }
                    });
                    if switch(ui, sw, ui.scope_id().with(("su-job", &j.label, j.scope as u8)), j.on, busy.is_none()).on_hover_text(if j.on { "Turn off" } else { "Turn on" }).clicked() {
                        flip_job = Some(j.clone());
                    }
                    y += ROW_H + ROW_GAP;
                }
            }
        }

        // the notification switch
        y += 10.0;
        let nr = Rect::from_min_size(Pos2::new(a.left(), y), Vec2::new(a.width(), 40.0));
        p.text(Pos2::new(nr.left() + 4.0, nr.center().y), Align2::LEFT_CENTER, "Tell me when an app adds itself to startup", font(13.0), TEXT_SOFT);
        let sw = Rect::from_center_size(Pos2::new(nr.right() - 30.0, nr.center().y), Vec2::new(36.0, 20.0));
        if switch(ui, sw, ui.scope_id().with("su-notify"), notify, true).clicked() {
            set_notify = Some(!notify);
        }
    });
    if turn_off_all {
        s.turn_off_suggested();
    }
    if do_undo {
        s.undo();
    }
    if let Some(la) = flip_app {
        s.toggle_app(&la);
    }
    if let Some(j) = flip_job {
        s.toggle_job(&j);
    }
    if let Some(j) = trash {
        s.trash(&j);
    }
    if let Some(on) = set_notify {
        s.set_notify(on);
    }
}

/// "2 updaters that check in the background, Steam Cleaner, and 1 agent of an app you removed."
fn why_line(list: &[&Job]) -> String {
    let n = |w: Why| list.iter().filter(|j| j.why() == Some(w)).count();
    let mut parts = Vec::new();
    match n(Why::Updater) {
        0 => {}
        1 => parts.push("1 updater that checks in the background".to_string()),
        k => parts.push(format!("{k} updaters that check in the background")),
    }
    let extra: Vec<String> = list.iter().filter(|j| j.why() == Some(Why::NotNeeded)).map(|j| j.name.clone()).collect();
    if !extra.is_empty() {
        parts.push(extra.join(", "));
    }
    match n(Why::Removed) {
        0 => {}
        1 => parts.push("1 agent of an app you removed".to_string()),
        k => parts.push(format!("{k} agents of apps you removed")),
    }
    let s = match parts.len() {
        0 => String::new(),
        1 => parts[0].clone(),
        _ => format!("{}, and {}", parts[..parts.len() - 1].join(", "), parts[parts.len() - 1]),
    };
    let mut c = s.chars();
    let s: String = c.next().map(|f| f.to_uppercase().chain(c).collect()).unwrap_or_default();
    format!("{s}. Apps still work when you open them.")
}

struct Cols {
    when: f32,
    state: f32,
}

fn columns(r: Rect) -> Cols {
    let state = r.right() - 64.0 - 84.0 - 74.0;
    Cols { when: state - 130.0, state }
}

fn when_icon(j: &Job) -> Icon {
    use super::items::When;
    match j.when {
        When::KeepsRunning => Icon::Refresh,
        When::Every(_) | When::Scheduled => Icon::Clock,
        When::AtLogin => Icon::Power,
        When::OnDemand => Icon::Forward,
    }
}

fn when_cell(p: &egui::Painter, x: f32, cy: f32, ic: Icon, label: &str) {
    icon(p, ic, Rect::from_center_size(Pos2::new(x + 6.0, cy), Vec2::splat(12.0)), TEXT_DIM);
    text_fit(p, Pos2::new(x + 17.0, cy), Align2::LEFT_CENTER, label, font(12.0), TEXT_DIM, 110.0);
}

/// `label` None = running (green dot).
fn state_cell(p: &egui::Painter, x: f32, cy: f32, label: Option<&str>, running: bool) {
    p.circle_filled(Pos2::new(x + 4.0, cy), 3.5, if running { GREEN } else { white(56) });
    p.text(Pos2::new(x + 13.0, cy), Align2::LEFT_CENTER, label.unwrap_or("running"), font(12.0), TEXT_DIM);
}

/// The ⋯ button of a row.
fn icon_dots(ui: &mut Ui, r: Rect, id: egui::Id) -> egui::Response {
    let resp = ui.interact(r, id, Sense::click());
    let h = ui.ctx().animate_bool(id.with("h"), resp.hovered());
    let p = ui.painter();
    if h > 0.0 {
        p.rect_filled(r, 6.0, white((18.0 * h) as u8));
    }
    for dx in [-5.0, 0.0, 5.0] {
        p.circle_filled(r.center() + Vec2::new(dx, 0.0), 1.6, TEXT_FAINT);
    }
    resp.on_hover_text("More")
}

// ---------------------------------------------------------------- panel widget

pub fn widget(s: &mut Startup, size: Size, ui: &mut Ui, rect: Rect, cx: &mut Cx) {
    let id = ui.scope_id().with("w-startup");
    let (loaded, total, running, suggested, undo, busy) = {
        let g = s.state();
        (g.jobs_loaded, g.total(), g.running(), g.suggested(), g.undoable(), g.busy.is_some())
    };
    let n = suggested.len();
    match size {
        Size::S11 => {
            let resp = tile::card(ui, rect, id, false);
            let p = ui.painter();
            tool_chip(p, Rect::from_min_size(rect.min + Vec2::splat(9.0), Vec2::splat(24.0)), Icon::Power, tint::STARTUP);
            if n > 0 {
                count_badge(p, Pos2::new(rect.right() - 9.0, rect.top() + 9.0), n);
            }
            tile::small_text(p, rect, &if loaded { total.to_string() } else { "…".into() }, "at startup", TEXT_DIM);
            if resp.clicked() {
                cx.open_page(super::ID, if n > 0 { "suggested" } else { "" });
            }
        }
        Size::S12 => {
            let resp = tile::card(ui, rect, id, false);
            let p = ui.painter().clone();
            let (x0, x1) = (rect.left() + tile::PAD, rect.right() - tile::PAD);
            tool_chip(&p, Rect::from_min_size(Pos2::new(x0, rect.top() + 9.0), Vec2::splat(18.0)), Icon::Power, tint::STARTUP);
            p.text(Pos2::new(x0 + 24.0, rect.top() + 18.0), Align2::LEFT_CENTER, "Startup", bold(12.0), TEXT);
            p.text(Pos2::new(x1, rect.top() + 18.0), Align2::RIGHT_CENTER, total.to_string(), font(11.0), TEXT_DIM);
            let cy = rect.bottom() - 21.0;
            if let Some(k) = undo {
                p.text(Pos2::new(x0, cy), Align2::LEFT_CENTER, format!("{k} off"), font(12.0), GREEN);
                let br = Rect::from_min_max(Pos2::new(x1 - 54.0, cy - 12.0), Pos2::new(x1, cy + 12.0));
                if pill_button(ui, br, "Undo", BTN_SECONDARY, !busy).clicked() {
                    s.undo();
                }
            } else if n > 0 {
                p.text(Pos2::new(x0, cy), Align2::LEFT_CENTER, format!("{n} can go"), font(12.0), SUGGEST_TEXT);
                let br = Rect::from_min_max(Pos2::new(x1 - 64.0, cy - 12.0), Pos2::new(x1, cy + 12.0));
                if pill_button(ui, br, "Turn off", BTN_PRIMARY, !busy).clicked() {
                    s.turn_off_suggested();
                }
            } else {
                p.text(Pos2::new(x0, cy), Align2::LEFT_CENTER, format!("{running} running · all needed"), font(12.0), TEXT_DIM);
            }
            if resp.clicked() {
                cx.open_page(super::ID, "");
            }
        }
        _ => {
            let resp = tile::card(ui, rect, id, false);
            let p = ui.painter().clone();
            let note = if loaded { format!("{} · {running} running", plural(total, "item")) } else { "Reading…".into() };
            let (x0, x1) = (rect.left() + tile::PAD, rect.right() - tile::PAD);
            let cy = rect.top() + tile::PAD + tile::HEAD_H / 2.0;
            tool_chip(&p, Rect::from_center_size(Pos2::new(x0 + 9.0, cy), Vec2::splat(18.0)), Icon::Power, tint::STARTUP);
            let tr = p.text(Pos2::new(x0 + 26.0, cy), Align2::LEFT_CENTER, "Startup", bold(13.0), TEXT);
            let mut right = x1;
            if undo.is_none() && n > 0 {
                let label = format!("Turn off {n}");
                let bw = button_width(&p, &label, BTN_PRIMARY) - 6.0;
                let br = Rect::from_min_max(Pos2::new(x1 - bw, cy - 12.0), Pos2::new(x1, cy + 12.0));
                if pill_button(ui, br, &label, BTN_PRIMARY, !busy).clicked() {
                    s.turn_off_suggested();
                }
                right = br.left() - 8.0;
            }
            text_fit(&p, Pos2::new(tr.right() + 8.0, cy), Align2::LEFT_CENTER, &note, font(11.0), TEXT_FAINT, right - tr.right() - 8.0);
            let body = Rect::from_min_max(Pos2::new(x0, rect.top() + tile::PAD + tile::HEAD_H + 6.0), Pos2::new(x1, rect.bottom() - tile::PAD));
            if let Some(k) = undo {
                let c = Pos2::new(body.left() + 20.0, body.center().y);
                p.circle_filled(c, 18.0, with_alpha(GREEN, 0.16));
                icon(&p, Icon::Check, Rect::from_center_size(c, Vec2::splat(18.0)), GREEN);
                let br = Rect::from_min_max(Pos2::new(body.right() - 64.0, c.y - 13.0), Pos2::new(body.right(), c.y + 13.0));
                if pill_button(ui, br, "Undo", BTN_SECONDARY, !busy).clicked() {
                    s.undo();
                }
                p.text(Pos2::new(c.x + 30.0, c.y - 9.0), Align2::LEFT_CENTER, format!("{} turned off", plural(k, "item")), bold(13.0), TEXT);
                text_fit(&p, Pos2::new(c.x + 30.0, c.y + 9.0), Align2::LEFT_CENTER, "Nothing removed. The apps still open.", font(12.0), TEXT_DIM, br.left() - c.x - 40.0);
            } else if n == 0 {
                let msg = if loaded { "Everything that starts with your Mac looks needed." } else { "Reading what starts with your Mac…" };
                tile::empty_note(&p, body.shrink2(Vec2::new(0.0, 6.0)), msg);
            } else {
                let lh = 28.0;
                let fit = ((body.height() - 14.0) / lh).floor().max(1.0) as usize;
                let show = if n > fit { fit.saturating_sub(0).min(n) } else { n };
                let mut budget = ICONS_PER_FRAME;
                let ctx = ui.ctx().clone();
                for (k, j) in suggested.iter().take(show.min(3)).enumerate() {
                    let r = Rect::from_min_size(Pos2::new(body.left(), body.top() + k as f32 * lh), Vec2::new(body.width(), lh));
                    let ir = Rect::from_center_size(Pos2::new(r.left() + 10.0, r.center().y), Vec2::splat(20.0));
                    app_icon(s, &ctx, &p, ir, j.owner.as_deref(), &j.name, false, &mut budget);
                    let why = j.why().map(|w| w.short()).unwrap_or("");
                    let wr = p.text(Pos2::new(r.right() - 42.0, r.center().y), Align2::RIGHT_CENTER, why, font(11.0), TEXT_FAINT);
                    text_fit(&p, Pos2::new(ir.right() + 8.0, r.center().y), Align2::LEFT_CENTER, &j.name, font(12.5), TEXT, wr.left() - ir.right() - 16.0);
                    let sw = Rect::from_center_size(Pos2::new(r.right() - 15.0, r.center().y), Vec2::new(30.0, 18.0));
                    if switch(ui, sw, id.with(("w-sw", &j.label)), true, !busy).clicked() {
                        s.toggle_job(j);
                    }
                }
                if n > 3 {
                    p.text(Pos2::new(body.left(), body.bottom() - 4.0), Align2::LEFT_BOTTOM, format!("+{} more · open Startup", n - 3), font(11.0), TEXT_FAINT);
                }
            }
            if resp.clicked() {
                cx.open_page(super::ID, if n > 0 { "suggested" } else { "" });
            }
        }
    }
}

/// A red count in the corner of the 1:1 widget.
fn count_badge(p: &egui::Painter, pos: Pos2, n: usize) {
    let g = p.layout_no_wrap(n.to_string(), bold(11.0), Color32::WHITE);
    let r = Align2::RIGHT_TOP.anchor_size(pos, Vec2::new((g.size().x + 10.0).max(18.0), 18.0));
    p.rect_filled(r, 9.0, tint::STARTUP);
    p.galley(r.center() - g.size() / 2.0, g, Color32::WHITE);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn why_line_reads_well() {
        let mut g = super::super::State::default();
        super::super::demo(&mut g, false);
        let list = g.suggested();
        let refs: Vec<&Job> = list.iter().collect();
        assert_eq!(why_line(&refs), "2 updaters that check in the background, Steam Cleaner, and 1 agent of an app you removed. Apps still work when you open them.");
    }

    /// Every demo view renders without a panic.
    #[test]
    fn demo_views_draw() {
        use crate::core::tool::{Tool, ToolEnv};
        let ctx = egui::Context::default();
        let mut s = Startup::new(&ToolEnv { ctx: ctx.clone(), demo: true });
        for v in ["startup", "startup-done", "startup-suggested"] {
            assert!(s.demo_view(v).is_some());
            ctx.run_ui(egui::RawInput { screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(1100.0, 700.0))), ..Default::default() }, |ui| {
                let mut cx = Cx::new(None);
                page(&mut s, ui, Rect::from_min_size(Pos2::new(220.0, 0.0), Vec2::new(880.0, 700.0)), &mut cx);
                for z in [Size::S11, Size::S12, Size::S24] {
                    widget(&mut s, z, ui, Rect::from_min_size(Pos2::ZERO, Vec2::new(356.0, 174.0)), &mut cx);
                }
            })
            .textures_delta
            .clear();
        }
    }
}
