//! The AI Check strip: one row across the top of a tool's page (sparkle,
//! "AI Check", what's going on, one button), and under it, once checked,
//! one row per recommendation. No card; a hairline closes it. It shows only
//! when the tool has found something and the main agent can be used.

use super::{Check, Phase, Rec};
use crate::kit::agent::Agent;
use crate::kit::glass::*;
use egui::{Align2, Color32, Pos2, Rect, Sense, Stroke, Ui, Vec2};

const HEAD_H: f32 = 40.0;
const ROW_H: f32 = 44.0;

/// What the person pressed.
#[derive(Clone, Debug, PartialEq)]
pub enum Event {
    /// Run the check now: the tool builds its [`super::Ask`].
    Start,
    /// A recommendation's button.
    Act(Rec),
}

/// Whether the strip shows: the tool found something to look at and the
/// main agent can be used (screenshots: always).
pub fn shown(c: &Check, has_data: bool) -> bool {
    c.demo || (has_data && crate::kit::agent::main().ready())
}

/// Draw the strip at the top of `body` when it shows. Returns the height it
/// took (0 when hidden), the gap under it included, and what was pressed.
/// `offer` says what there is to check ("12 removed apps").
pub fn strip(c: &mut Check, ui: &mut Ui, body: Rect, offer: &str, has_data: bool) -> (f32, Option<Event>) {
    if !shown(c, has_data) {
        c.hovered = None;
        return (0.0, None);
    }
    let p = ui.painter().clone();
    let ai = tint::HEALTH();
    let agent = if c.demo { Agent::Claude } else { crate::kit::agent::main() };
    let phase = c.phase();
    let recs = if phase == Phase::Done { c.recs() } else { vec![] };
    let (x0, x1) = (body.left(), body.right());
    let head = Rect::from_min_size(body.min, Vec2::new(body.width(), HEAD_H));
    let cy = head.center().y;
    let mut ev = None;

    sparkle(&p, Pos2::new(x0 + 7.0, cy), 14.0, ai);
    let t = p.text(Pos2::new(x0 + 22.0, cy), Align2::LEFT_CENTER, "AI Check", ty::title(), TEXT());

    // the one control on the right
    let right;
    match &phase {
        Phase::Idle | Phase::Failed(_) => {
            let label = format!("Check with {}", agent.name());
            let bw = button_width(&p, &label, BTN_SECONDARY) + 18.0;
            let r = Rect::from_min_size(Pos2::new(x1 - bw, cy - sp::CONTROL_H / 2.0), Vec2::new(bw, sp::CONTROL_H));
            if pill_button(ui, r, &format!("     {label}"), BTN_SECONDARY, true).clicked() {
                ev = Some(Event::Start);
            }
            let tw = p.layout_no_wrap(label, bold(13.0), TEXT()).size().x;
            agent_mark(&p, Rect::from_center_size(Pos2::new(r.center().x - tw / 2.0 - 4.0, cy), Vec2::splat(12.0)), agent, None);
            right = r.left();
        }
        Phase::Running => {
            let w = p.layout_no_wrap("Stop".into(), ty::body(), TEXT()).size().x;
            let r = Rect::from_min_size(Pos2::new(x1 - w, cy - 10.0), Vec2::new(w, 20.0));
            if ghost(ui, r, "Stop") {
                c.stop();
            }
            right = r.left();
        }
        Phase::Done => {
            let w = p.layout_no_wrap("Check again".into(), ty::body(), TEXT()).size().x;
            let r = Rect::from_min_size(Pos2::new(x1 - w, cy - 10.0), Vec2::new(w, 20.0));
            if ghost(ui, r, "Check again") {
                ev = Some(Event::Start);
            }
            right = r.left();
        }
    }

    // what's going on, between the title and the control
    let mut mx = t.right() + sp::M;
    let (line, col) = match &phase {
        Phase::Idle => (offer.to_string(), TEXT_DIM()),
        Phase::Running => {
            spinner(&p, Pos2::new(mx + 6.0, cy), 6.0, ui.input(|i| i.time), ai);
            mx += 20.0;
            animate(ui.ctx());
            (format!("{} is checking…", agent.name()), TEXT_DIM())
        }
        Phase::Failed(e) => (e.clone(), AMBER()),
        Phase::Done => {
            let h = c.headline();
            (if recs.is_empty() && h.is_empty() { "All good here".to_string() } else { h }, TEXT_DIM())
        }
    };
    if !line.is_empty() && right - sp::M > mx {
        text_fit(&p, Pos2::new(mx, cy), Align2::LEFT_CENTER, &line, ty::body(), col, right - sp::M - mx);
    }

    // recommendations, one row each
    c.hovered = None;
    let mut y = head.bottom();
    for r in &recs {
        let rr = Rect::from_min_size(Pos2::new(x0, y), Vec2::new(body.width(), ROW_H));
        if let Some(e) = rec_row(c, ui, rr, r) {
            ev = Some(e);
        }
        y += ROW_H;
    }
    hairline(&p, Pos2::new(x0, y), Pos2::new(x1, y));
    (y - body.top() + sp::L, ev)
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

/// One recommendation on one line under a hairline: icon, title, why
/// (dim), value, its button and Skip.
fn rec_row(c: &mut Check, ui: &mut Ui, rr: Rect, r: &Rec) -> Option<Event> {
    let p = ui.painter().clone();
    let cy = rr.center().y;
    hairline(&p, rr.left_top(), rr.right_top());
    if ui.rect_contains_pointer(rr) {
        c.hovered = Some(r.key.clone());
        p.rect_filled(rr.shrink2(Vec2::new(0.0, 1.0)), R_SMALL, white(6));
    }
    let ic = Rect::from_center_size(Pos2::new(rr.left() + 12.0 + 4.0, cy), Vec2::splat(24.0));
    rec_icon(c, ui.ctx(), &p, ic, r);
    // right to left: Skip, the button, the value
    let sw = p.layout_no_wrap("Skip".into(), ty::body(), TEXT()).size().x;
    let sr = Rect::from_min_size(Pos2::new(rr.right() - sp::S - sw, cy - 10.0), Vec2::new(sw, 20.0));
    if ghost(ui, sr, "Skip") {
        c.skip(r);
    }
    let bw = button_width(&p, &r.label, BTN_PRIMARY);
    let br = Rect::from_min_size(Pos2::new(sr.left() - sp::M - bw, cy - sp::CONTROL_H / 2.0), Vec2::new(bw, sp::CONTROL_H));
    let mut ev = None;
    if pill_button(ui, br, &r.label, BTN_PRIMARY, true).clicked() {
        ev = Some(Event::Act(r.clone()));
    }
    let mut right = br.left() - sp::L;
    if !r.value.is_empty() {
        let v = p.text(Pos2::new(right, cy), Align2::RIGHT_CENTER, &r.value, ty::body(), TEXT());
        right = v.left() - sp::L;
    }
    let tx = ic.right() + sp::M;
    let t = text_fit(&p, Pos2::new(tx, cy), Align2::LEFT_CENTER, &r.title, ty::title(), TEXT(), (right - tx).min(260.0));
    if right > t.right() + sp::M + 40.0 {
        let wr = text_fit(&p, Pos2::new(t.right() + sp::M, cy), Align2::LEFT_CENTER, &r.why, ty::label(), TEXT_DIM(), right - t.right() - sp::M);
        if p.layout_no_wrap(r.why.clone(), ty::label(), TEXT_DIM()).size().x > right - t.right() - sp::M {
            let _ = ui.interact(wr, ui.scope_id().with(("ai-why", &r.key)), Sense::hover()).on_hover_text(&r.why);
        }
    }
    ev
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
