//! Settings › AI Usage: which providers to read, where Claude's limits come
//! from (the status-line hook), notifications, and what the menu bar shows.

use crate::kit::ai::{self, settings, statusline};
use crate::kit::glass::*;
use egui::{Align2, Pos2, Rect, Ui, Vec2};

const ROW_H: f32 = sp::SETTINGS_ROW_H;
const THRESHOLDS: [u8; 3] = [50, 80, 95];
const MENU: [(&str, &str); 4] = [("closest", "Closest to limit"), ("claude", "Claude"), ("codex", "Codex"), ("off", "Off")];

/// A card of `n` rows under a section title. Returns the card and moves `y`
/// past it.
fn section(p: &egui::Painter, x: f32, w: f32, y: &mut f32, title: &str, n: usize) -> Rect {
    p.text(Pos2::new(x, *y), Align2::LEFT_TOP, title, bold(15.0), TEXT());
    *y += 28.0;
    let card = Rect::from_min_size(Pos2::new(x, *y), Vec2::new(w, n as f32 * ROW_H));
    glass_card(p, card, R_CARD, 0.0, false);
    for k in 1..n {
        let t = card.top() + k as f32 * ROW_H;
        hairline(p, Pos2::new(card.left() + 16.0, t), Pos2::new(card.right() - 16.0, t));
    }
    *y = card.bottom() + 28.0;
    card
}

fn row(card: Rect, k: usize) -> Rect {
    Rect::from_min_size(Pos2::new(card.left(), card.top() + k as f32 * ROW_H), Vec2::new(card.width(), ROW_H))
}

fn switch_at(r: Rect) -> Rect {
    Rect::from_min_size(Pos2::new(r.right() - 42.0, r.center().y - 9.0), Vec2::new(30.0, 18.0))
}

fn label(p: &egui::Painter, r: Rect, s: &str) {
    p.text(Pos2::new(r.left() + 16.0, r.center().y), Align2::LEFT_CENTER, s, font(13.0), TEXT());
}

/// Draws the tab into `body`; returns the bottom of what it drew.
pub fn list(ui: &mut Ui, body: Rect, demo: bool) -> f32 {
    let p = ui.painter().clone();
    let set = settings::get();
    let w = 600.0f32.min(body.width() - 48.0);
    let x = body.center().x - w / 2.0;
    let mut y = body.top() + 24.0;
    let mut change: Option<Box<dyn FnOnce(&mut settings::Settings)>> = None;

    // providers
    let card = section(&p, x, w, &mut y, "Providers", ai::PROVIDERS.len());
    for (k, m) in ai::PROVIDERS.iter().enumerate() {
        let r = row(card, k);
        let found = demo || ai::make(k).data_dir().is_some_and(|d| d.exists());
        p.circle_filled(Pos2::new(r.left() + 20.0, r.center().y), 4.0, if found { GREEN() } else { white(51) });
        p.text(Pos2::new(r.left() + 32.0, r.center().y), Align2::LEFT_CENTER, m.name, font(13.0), TEXT());
        let source = match m.id {
            "claude" if statusline::installed() => "Status line + logs",
            "claude" => "CLI + logs",
            _ if m.limits => "Local files",
            _ => "Tokens only",
        };
        let state = if found { source } else { "Not found" };
        p.text(Pos2::new(r.right() - 58.0, r.center().y), Align2::RIGHT_CENTER, state, ty::label(), TEXT_DIM());
        let on = set.is_on(m.id);
        if switch(ui, switch_at(r), ui.scope_id().with(("ai-on", m.id)), on, true).clicked() {
            let id = m.id.to_string();
            change = Some(Box::new(move |s| {
                s.off.retain(|x| *x != id);
                if on {
                    s.off.push(id);
                }
            }));
        }
    }

    // Claude's limits
    let card = section(&p, x, w, &mut y, "Claude limits", 2);
    let r0 = row(card, 0);
    label(&p, r0, "Source");
    let seg = Rect::from_min_size(Pos2::new(r0.right() - 16.0 - 200.0, r0.center().y - 13.0), Vec2::new(200.0, 26.0));
    let cur = usize::from(set.claude_source != "statusline");
    if let Some(i) = segmented(ui, seg, ui.scope_id().with("ai-claude-src"), &["Status line", "CLI"], cur) {
        change = Some(Box::new(move |s| s.claude_source = if i == 0 { "statusline".into() } else { "cli".into() }));
    }
    let r1 = row(card, 1);
    let installed = statusline::installed();
    label(&p, r1, "Status line hook");
    let (txt, col) = if installed { ("Installed", GREEN()) } else { ("Off", TEXT_DIM()) };
    let btn = if installed { "Remove" } else { "Install" };
    let bw = button_width(&p, btn, BTN_SECONDARY);
    let br = Rect::from_min_size(Pos2::new(r1.right() - 16.0 - bw, r1.center().y - 14.0), Vec2::new(bw, 28.0));
    p.text(Pos2::new(br.left() - sp::M, r1.center().y), Align2::RIGHT_CENTER, txt, ty::label(), col);
    if pill_button(ui, br, btn, BTN_SECONDARY, !demo).clicked() {
        let res = if installed { statusline::remove() } else { statusline::install() };
        if let Err(e) = res {
            eprintln!("[ai] status line: {e}");
        }
    }

    // notifications
    let card = section(&p, x, w, &mut y, "Notifications", 3);
    let r0 = row(card, 0);
    label(&p, r0, "Limit reaches");
    let cw = 56.0;
    for (k, t) in THRESHOLDS.iter().enumerate() {
        let cr = Rect::from_min_size(Pos2::new(r0.right() - 16.0 - (THRESHOLDS.len() - k) as f32 * (cw + 6.0) + 6.0, r0.center().y - 13.0), Vec2::new(cw, 26.0));
        let on = set.notify_at.contains(t);
        let resp = ui.interact(cr, ui.scope_id().with(("ai-th", *t)), egui::Sense::click());
        p.rect_filled(cr, R_SMALL, if on { with_alpha(BLUE(), 0.22) } else { white(15) });
        p.text(cr.center(), Align2::CENTER_CENTER, format!("{t}%"), font(12.0), if on { ACCENT() } else { TEXT_DIM() });
        if resp.clicked() {
            let t = *t;
            change = Some(Box::new(move |s| {
                if on {
                    s.notify_at.retain(|x| *x != t);
                } else {
                    s.notify_at.push(t);
                    s.notify_at.sort_unstable();
                }
            }));
        }
    }
    for (k, (name, on, f)) in [
        ("Limit reset", set.notify_reset, (|s: &mut settings::Settings| s.notify_reset = !s.notify_reset) as fn(&mut settings::Settings)),
        ("Weekly unused, day before reset", set.notify_unused, |s: &mut settings::Settings| s.notify_unused = !s.notify_unused),
    ]
    .into_iter()
    .enumerate()
    {
        let r = row(card, k + 1);
        label(&p, r, name);
        if switch(ui, switch_at(r), ui.scope_id().with(("ai-n", k)), on, true).clicked() {
            change = Some(Box::new(f));
        }
    }

    // display
    let card = section(&p, x, w, &mut y, "Display", 3);
    let r0 = row(card, 0);
    label(&p, r0, "Menu bar");
    let labels: Vec<&str> = MENU.iter().map(|m| m.1).collect();
    let seg = Rect::from_min_size(Pos2::new(r0.right() - 16.0 - 380.0, r0.center().y - 13.0), Vec2::new(380.0, 26.0));
    let cur = MENU.iter().position(|m| m.0 == set.menu_bar).unwrap_or(0);
    if let Some(i) = segmented(ui, seg, ui.scope_id().with("ai-menu"), &labels, cur) {
        change = Some(Box::new(move |s| s.menu_bar = MENU[i].0.into()));
    }
    for (k, (name, on, f)) in [
        ("Cost in $", set.show_cost, (|s: &mut settings::Settings| s.show_cost = !s.show_cost) as fn(&mut settings::Settings)),
        ("Provider outages", set.show_outages, |s: &mut settings::Settings| s.show_outages = !s.show_outages),
    ]
    .into_iter()
    .enumerate()
    {
        let r = row(card, k + 1);
        label(&p, r, name);
        if switch(ui, switch_at(r), ui.scope_id().with(("ai-d", k)), on, true).clicked() {
            change = Some(Box::new(f));
        }
    }

    if let Some(f) = change {
        settings::update(!demo, f);
        crate::kit::claude_code::request_recheck();
    }
    y - 4.0
}
