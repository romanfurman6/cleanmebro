//! The AI Check column: no card, a hairline on its left, then the header
//! (sparkle, "AI Check", the agent's mark), and one state: start, checking,
//! recommendations, all good, agent missing, or (switched off) an offer.

use super::{settings, update, Check, Phase, Rec};
use crate::kit::agent::Agent;
use crate::kit::glass::*;
use egui::{Align2, Color32, Pos2, Rect, Sense, Stroke, Ui, Vec2};

/// Column width, its hairline included.
pub const WIDTH: f32 = 300.0;

/// What the person pressed.
#[derive(Clone, Debug, PartialEq)]
pub enum Event {
    /// Run the check now: the tool builds its [`super::Ask`].
    Start,
    /// A recommendation's button.
    Act(Rec),
    OpenSettings,
}

/// Whether the column shows: always when the tool's AI Check is on; when
/// off, only once the tool has something to look at (it offers a check).
pub fn shown(c: &Check, has_data: bool) -> bool {
    c.demo || settings().is_on(c.tool) || has_data
}

/// Split a page body into (content, column) with `sp::L` between.
pub fn split(body: Rect) -> (Rect, Rect) {
    let col = Rect::from_min_max(Pos2::new(body.right() - WIDTH, body.top()), body.max);
    let content = Rect::from_min_max(body.min, Pos2::new(col.left() - sp::L, body.bottom()));
    (content, col)
}

/// The agent's mark: Claude's orange burst, Codex's prompt square.
pub fn agent_mark(p: &egui::Painter, r: Rect, a: Agent, ink: Option<Color32>) {
    match a {
        Agent::Claude => {
            let c = r.center();
            let rad = r.width() * 0.46;
            let col = ink.unwrap_or(Color32::from_rgb(0xD9, 0x77, 0x57));
            for k in 0..4 {
                let ang = k as f32 * std::f32::consts::FRAC_PI_4;
                let d = Vec2::angled(ang) * rad;
                p.line_segment([c - d, c + d], Stroke::new((r.width() * 0.13).max(1.3), col));
            }
        }
        Agent::Codex => {
            p.rect_filled(r, r.width() * 0.24, ink.unwrap_or(Color32::BLACK));
            p.rect_stroke(r, r.width() * 0.24, Stroke::new(1.0, white(70)), egui::StrokeKind::Inside);
            p.text(r.center(), Align2::CENTER_CENTER, ">_", mono(r.width() * 0.5), if ink.is_some() { BG() } else { Color32::WHITE });
        }
    }
}

fn sparkle(p: &egui::Painter, c: Pos2, s: f32, col: Color32) {
    icon(p, Icon::Sparkle, Rect::from_center_size(c, Vec2::splat(s)), col);
}

/// Draw the column in `rect` (from [`split`]); `offer` is the line shown
/// while the tool's check is off ("12 leftovers to look at").
pub fn column(c: &mut Check, ui: &mut Ui, rect: Rect, offer: &str) -> Option<Event> {
    let p = ui.painter().clone();
    let ai = tint::HEALTH();
    hairline(&p, rect.left_top(), rect.left_bottom());
    let x = rect.left() + sp::L;
    let w = rect.right() - x;
    let agent = crate::kit::agent::main();
    let ready = c.demo || agent.ready();
    let on = c.demo || settings().is_on(c.tool);
    let phase = c.phase();
    let mut ev = None;

    // header
    let hy = rect.top() + 10.0;
    sparkle(&p, Pos2::new(x + 7.0, hy), 14.0, if ready { ai } else { TEXT_FAINT() });
    p.text(Pos2::new(x + 22.0, hy), Align2::LEFT_CENTER, "AI Check", ty::title(), if ready { TEXT() } else { TEXT_DIM() });
    if ready && phase != Phase::Idle {
        let name = agent.name();
        let g = p.layout_no_wrap(name.to_string(), bold(11.0), TEXT());
        let cw = g.size().x + 30.0;
        let cr = Rect::from_min_size(Pos2::new(rect.right() - cw, hy - 10.0), Vec2::new(cw, 20.0));
        p.rect_filled(cr, 6.0, white(14));
        agent_mark(&p, Rect::from_center_size(Pos2::new(cr.left() + 12.0, cr.center().y), Vec2::splat(11.0)), agent, None);
        p.galley(Pos2::new(cr.left() + 22.0, cr.center().y - g.size().y / 2.0), g, TEXT());
    }
    let mut y = hy + 10.0 + sp::M;
    let check_btn = |ui: &mut Ui, y: f32, primary: bool| -> bool {
        let label = format!("Check with {}", agent.name());
        let bw = button_width(ui.painter(), &label, BTN_PRIMARY) + 18.0;
        let r = Rect::from_min_size(Pos2::new(x, y), Vec2::new(bw.min(w), sp::CONTROL_H));
        let clicked = pill_button(ui, r, &format!("     {label}"), if primary { BTN_PRIMARY } else { BTN_SECONDARY }, true).clicked();
        let ink = if primary { Some(on_color(BLUE())) } else { None };
        let tw = ui.painter().layout_no_wrap(label, bold(13.0), TEXT()).size().x;
        agent_mark(ui.painter(), Rect::from_center_size(Pos2::new(r.center().x - tw / 2.0 - 4.0, r.center().y), Vec2::splat(12.0)), agent, ink);
        clicked
    };

    if !ready {
        let why = format!("{} — {}", agent.name(), agent.why_not());
        let g = p.layout(why, ty::label(), TEXT_DIM(), w);
        let h = g.size().y;
        p.galley(Pos2::new(x, y), g, TEXT_DIM());
        y += h + sp::M;
        let r = Rect::from_min_size(Pos2::new(x, y), Vec2::new(button_width(&p, "Open Settings", BTN_SECONDARY), sp::CONTROL_H));
        if pill_button(ui, r, "Open Settings", BTN_SECONDARY, true).clicked() {
            ev = Some(Event::OpenSettings);
        }
        return ev;
    }

    match phase {
        Phase::Idle if !on => {
            if !offer.is_empty() {
                let g = p.layout(offer.to_string(), ty::lead(), TEXT(), w);
                let h = g.size().y;
                p.galley(Pos2::new(x, y), g, TEXT());
                y += h + sp::M;
            }
            if check_btn(ui, y, true) {
                ev = Some(Event::Start);
            }
            let bw = button_width(&p, "Always on", BTN_SECONDARY);
            let r = Rect::from_min_size(Pos2::new(x, y + sp::CONTROL_H + sp::S), Vec2::new(bw, sp::CONTROL_H));
            if ghost(ui, r, "Always on") {
                let tool = c.tool.to_string();
                update(true, |s| {
                    s.on.insert(tool, true);
                });
                ev = Some(Event::Start);
            }
        }
        Phase::Idle => {
            if check_btn(ui, y, true) {
                ev = Some(Event::Start);
            }
        }
        Phase::Running => {
            let t = ui.input(|i| i.time);
            spinner(&p, Pos2::new(x + 7.0, y + 10.0), 6.0, t, ai);
            p.text(Pos2::new(x + 22.0, y + 10.0), Align2::LEFT_CENTER, "Checking…", ty::body(), TEXT_DIM());
            let r = Rect::from_min_size(Pos2::new(rect.right() - 48.0, y), Vec2::new(48.0, 20.0));
            if ghost(ui, r, "Stop") {
                c.stop();
            }
            animate(ui.ctx());
        }
        Phase::Failed(e) => {
            let g = p.layout(e, ty::label(), TEXT_DIM(), w);
            let h = g.size().y;
            p.galley(Pos2::new(x, y), g, TEXT_DIM());
            y += h + sp::M;
            if check_btn(ui, y, false) {
                ev = Some(Event::Start);
            }
        }
        Phase::Done => {
            let head = c.headline();
            if !head.is_empty() {
                let g = p.layout(head, ty::lead(), TEXT(), w);
                let h = g.size().y;
                p.galley(Pos2::new(x, y), g, TEXT());
                y += h + sp::M;
            }
            let recs = c.recs();
            if recs.is_empty() {
                p.text(Pos2::new(x + w / 2.0, y + 18.0), Align2::CENTER_CENTER, "All good here", ty::label(), TEXT_DIM());
                y += 36.0 + sp::S;
            }
            c.hovered = None;
            for r in &recs {
                let (h, e) = rec_row(c, ui, Rect::from_min_size(Pos2::new(x, y), Vec2::new(w, 0.0)), r);
                if let Some(e) = e {
                    ev = Some(e);
                }
                y += h;
            }
            let r = Rect::from_min_size(Pos2::new(x, y + sp::S), Vec2::new(button_width(&p, "Check again", BTN_SECONDARY) - 28.0, 20.0));
            if ghost(ui, r, "Check again") {
                ev = Some(Event::Start);
            }
        }
    }
    ev
}

/// Text-only button: dim, brighter on hover. True when clicked.
fn ghost(ui: &mut Ui, r: Rect, label: &str) -> bool {
    let resp = ui.interact(r, ui.scope_id().with(("ai-ghost", label, r.min.x as i32, r.min.y as i32)), Sense::click());
    let col = if resp.hovered() { TEXT() } else { TEXT_DIM() };
    ui.painter().text(Pos2::new(r.left(), r.center().y), Align2::LEFT_CENTER, label, ty::body(), col);
    if resp.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    resp.clicked()
}

/// One recommendation under a hairline: icon, title and value, why, then
/// its button and Skip. Returns (height, event).
fn rec_row(c: &mut Check, ui: &mut Ui, at: Rect, r: &Rec) -> (f32, Option<Event>) {
    let p = ui.painter().clone();
    let (x, w) = (at.left(), at.width());
    let top = at.top();
    hairline(&p, Pos2::new(x, top), Pos2::new(x + w, top));
    let ic = Rect::from_min_size(Pos2::new(x, top + sp::M), Vec2::splat(28.0));
    rec_icon(c, ui.ctx(), &p, ic, r);
    let tx = ic.right() + 10.0;
    let tw = x + w - tx;
    let vr = if r.value.is_empty() { Rect::from_min_size(Pos2::new(x + w, top + sp::M), Vec2::ZERO) } else { p.text(Pos2::new(x + w, top + sp::M), Align2::RIGHT_TOP, &r.value, ty::title(), TEXT()) };
    let t = text_fit(&p, Pos2::new(tx, top + sp::M), Align2::LEFT_TOP, &r.title, ty::title(), TEXT(), vr.left() - sp::S - tx);
    let g = p.layout(r.why.clone(), ty::label(), TEXT_DIM(), tw);
    let gh = g.size().y;
    p.galley(Pos2::new(tx, t.bottom() + sp::XS), g, TEXT_DIM());
    let by = t.bottom() + sp::XS + gh + sp::S;
    let bw = button_width(&p, &r.label, BTN_PRIMARY);
    let br = Rect::from_min_size(Pos2::new(tx, by), Vec2::new(bw, sp::CONTROL_H));
    let mut ev = None;
    let h = by + sp::CONTROL_H + sp::M - top;
    let row = Rect::from_min_size(Pos2::new(x - 6.0, top), Vec2::new(w + 12.0, h));
    if ui.rect_contains_pointer(row) {
        c.hovered = Some(r.key.clone());
    }
    if pill_button(ui, br, &r.label, BTN_PRIMARY, true).clicked() {
        ev = Some(Event::Act(r.clone()));
    }
    let sr = Rect::from_min_size(Pos2::new(br.right() + sp::M, by), Vec2::new(36.0, sp::CONTROL_H));
    if ghost(ui, sr, "Skip") {
        c.skip(r);
    }
    (h, ev)
}

fn rec_icon(c: &mut Check, ctx: &egui::Context, p: &egui::Painter, r: Rect, rec: &Rec) {
    if let Some(path) = &rec.icon {
        if !c.icons.contains_key(path) && !c.demo {
            let tex = crate::kit::macos::app_icon(path, 64).map(|(n, rgba)| ctx.load_texture(format!("aicheck-icon:{}", path.display()), egui::ColorImage::from_rgba_unmultiplied([n, n], &rgba), egui::TextureOptions::LINEAR));
            c.icons.insert(path.clone(), tex);
        }
        if let Some(Some(t)) = c.icons.get(path) {
            p.image(t.id(), r, Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)), Color32::WHITE);
            return;
        }
    }
    let name = if rec.name.is_empty() { &rec.title } else { &rec.name };
    p.rect_filled(r, 7.0, letter_color(name));
    let l: String = name.chars().next().map(|ch| ch.to_uppercase().collect()).unwrap_or_default();
    p.text(r.center(), Align2::CENTER_CENTER, l, bold(14.0), Color32::WHITE);
}

fn letter_color(name: &str) -> Color32 {
    const PALETTE: [Color32; 6] = [
        Color32::from_rgb(0x4C, 0x8B, 0xF5),
        Color32::from_rgb(0x2E, 0xA0, 0x6B),
        Color32::from_rgb(0xC0, 0x60, 0x3E),
        Color32::from_rgb(0x8A, 0x5C, 0xD6),
        Color32::from_rgb(0x2B, 0x8C, 0xA0),
        Color32::from_rgb(0xB0, 0x7A, 0x1E),
    ];
    let h = name.bytes().fold(7u32, |h, b| h.wrapping_mul(31).wrapping_add(b as u32));
    PALETTE[(h % PALETTE.len() as u32) as usize]
}
