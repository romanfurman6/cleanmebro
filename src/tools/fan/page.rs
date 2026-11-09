//! The Fan Control page in the full window: a big dial per fan with
//! spinning blades, a heat map of the chip, the fan mode and the last
//! 10 minutes of speed and temperature.

use super::data::{heat, heat_text, rpm_text, Snapshot, HISTORY, SAMPLE_SECS};
use super::ui::{blades, dial, fan_name, heat_pos, heat_scale, hottest_text, is_block, keep_spinning, plural, sparkline, spin, thermal_hint, BLADE, HOT_LINE, LINE};
use crate::core::tool::Cx;
use crate::kit::glass::*;
use egui::{Align2, Color32, Pos2, Rect, Stroke, Ui, Vec2};

const GAP: f32 = 12.0;
const ROW1_H: f32 = 222.0;

pub fn page(s: &Snapshot, ui: &mut Ui, r: Rect, _cx: &mut Cx) {
    let p = ui.painter().clone();
    let t = ui.ctx().input(|i| i.time);
    let mut sub = Vec::new();
    if !s.chip.is_empty() {
        sub.push(s.chip.clone());
    }
    sub.push(if s.fans.is_empty() && s.loaded { "no fans".into() } else { plural(s.fans.len(), "fan") });
    if let Some((g, c)) = hottest_text(s) {
        sub.push(format!("hottest {g} {c:.0} °C"));
    }
    let br = page_header(&p, r, "Fan Control", &if s.loaded { sub.join(" · ") } else { "Reading sensors…".into() }, TEXT_DIM);
    if s.loaded {
        let (_, col) = thermal_hint(s.thermal);
        let label = format!("Thermal pressure: {}", s.thermal_word().to_lowercase());
        let w = p.layout_no_wrap(label.clone(), font(12.0), col).size().x + 30.0;
        let cr = Rect::from_min_size(Pos2::new(br.right() - w, br.center().y - 13.0), Vec2::new(w, 26.0));
        p.rect_filled(cr, 13.0, with_alpha(col, 0.13));
        p.circle_filled(Pos2::new(cr.left() + 13.0, cr.center().y), 3.5, col);
        p.text(Pos2::new(cr.left() + 22.0, cr.center().y), Align2::LEFT_CENTER, label, font(12.0), col);
    }

    let x0 = r.left() + 24.0;
    let x1 = r.right() - 24.0;
    let mut y = r.top() + PAGE_HEADER_H + 16.0;

    // row 1: fans + heat map
    let n = s.fans.len().min(2);
    let fan_w = 200.0;
    let mut hx = x0;
    if n > 0 {
        for (i, f) in s.fans.iter().take(n).enumerate() {
            let fr = Rect::from_min_size(Pos2::new(x0 + i as f32 * (fan_w + GAP), y), Vec2::new(fan_w, ROW1_H));
            glass_card(&p, fr, 14.0, 0.0, false);
            p.text(fr.min + Vec2::new(12.0, 12.0), Align2::LEFT_TOP, format!("{} fan", fan_name(i, s.fans.len())).replace("Fan fan", "Fan"), bold(13.0), TEXT);
            p.text(Pos2::new(fr.right() - 12.0, fr.top() + 13.0), Align2::RIGHT_TOP, format!("{:.0}% of max", f.frac() * 100.0), font(12.0), TEXT_FAINT);
            let c = Pos2::new(fr.center().x, fr.top() + 100.0);
            dial(&p, c, 58.0, 7.0, f.frac(), tint::FAN);
            p.circle_filled(c, 44.0, BG);
            p.circle_stroke(c, 44.0, Stroke::new(1.0, white(15)));
            blades(&p, c, 40.0, spin(t, f.rpm), BLADE, RAISED);
            let g = p.layout_no_wrap(rpm_text(f.rpm), bold(26.0), TEXT);
            let u = p.layout_no_wrap("rpm".into(), font(13.0), TEXT_DIM);
            let tw = g.size().x + 4.0 + u.size().x;
            let base = fr.top() + 170.0;
            let gx = fr.center().x - tw / 2.0;
            let gh = g.size().y;
            p.galley(Pos2::new(gx, base), g.clone(), TEXT);
            p.galley(Pos2::new(gx + g.size().x + 4.0, base + gh * 0.80 - u.size().y * 0.85), u, TEXT_DIM);
            p.text(Pos2::new(fr.center().x, fr.bottom() - 14.0), Align2::CENTER_BOTTOM, format!("min {} · max {}", rpm_text(f.min), rpm_text(f.max)), font(11.0), TEXT_FAINT);
        }
        hx = x0 + n as f32 * (fan_w + GAP);
        keep_spinning(ui, s);
    } else if s.loaded {
        let fr = Rect::from_min_size(Pos2::new(x0, y), Vec2::new(fan_w * 2.0 + GAP, ROW1_H));
        glass_card(&p, fr, 14.0, 0.0, false);
        let c = Pos2::new(fr.left() + 56.0, fr.top() + 64.0);
        p.circle_stroke(c, 32.0, Stroke::new(1.5, white(30)));
        blades(&p, c, 24.0, 0.3, white(40), CARD);
        let title = if s.has_smc { "This Mac has no fans" } else { "Fan speed isn't available" };
        p.text(Pos2::new(fr.left() + 104.0, c.y - 10.0), Align2::LEFT_CENTER, title, bold(15.0), TEXT);
        p.text(Pos2::new(fr.left() + 104.0, c.y + 10.0), Align2::LEFT_CENTER, "Cools passively", font(12.0), TEXT_DIM);
        paragraph(&p, Pos2::new(fr.left() + 20.0, fr.top() + 118.0), "Nothing to control: it slows down a little when it gets hot. The temperatures on the right still show how warm it runs and what heats it up.", font(13.0), TEXT_DIM, fr.width() - 40.0);
        hx = fr.right() + GAP;
    }
    let hm = Rect::from_min_max(Pos2::new(hx, y), Pos2::new(x1, y + ROW1_H));
    heat_map(&p, hm, s);
    y += ROW1_H + 14.0;

    // row 2: mode
    let mr = Rect::from_min_size(Pos2::new(x0, y), Vec2::new(x1 - x0, 64.0));
    glass_card(&p, mr, 14.0, 0.0, false);
    p.text(Pos2::new(mr.left() + 14.0, mr.top() + 20.0), Align2::LEFT_CENTER, "Mode", bold(13.0), TEXT);
    let chip = Rect::from_min_size(Pos2::new(mr.left() + 62.0, mr.top() + 8.0), Vec2::new(64.0, 24.0));
    p.rect_filled(chip, 7.0, if s.fans.is_empty() { white(16) } else { tint::FAN });
    p.text(chip.center(), Align2::CENTER_CENTER, if s.fans.is_empty() { "Passive" } else { "Auto" }, bold(12.0), Color32::WHITE);
    let hint = if s.fans.is_empty() {
        "macOS manages heat on its own. There is no fan to set."
    } else {
        "macOS sets the speed and spins the fans up when the chip gets warm. Quiet, Fixed, Curve and Max arrive with the fan helper."
    };
    text_fit(&p, Pos2::new(mr.left() + 14.0, mr.top() + 46.0), Align2::LEFT_CENTER, hint, font(12.0), TEXT_DIM, mr.width() - 28.0);
    y = mr.bottom() + 14.0;

    // row 3: history
    let hr = Rect::from_min_max(Pos2::new(x0, y), Pos2::new(x1, r.bottom() - 18.0));
    if hr.height() > 90.0 {
        history(&p, hr, s);
    }
}

/// The chip heat map: one block of cells per core group (P-cores, E-cores,
/// GPU), each cell a sensor coloured on the heat scale, plus the other
/// parts as small rows and the scale itself.
fn heat_map(p: &egui::Painter, r: Rect, s: &Snapshot) {
    glass_card(p, r, 14.0, 0.0, false);
    p.text(r.min + Vec2::new(12.0, 12.0), Align2::LEFT_TOP, "Chip heat map", bold(13.0), TEXT);
    p.text(Pos2::new(r.right() - 12.0, r.top() + 13.0), Align2::RIGHT_TOP, "live · per sensor", font(12.0), TEXT_FAINT);
    let groups = s.groups();
    if groups.is_empty() {
        let msg = if s.loaded { "No temperature sensors found." } else { "Reading sensors…" };
        p.text(r.center(), Align2::CENTER_CENTER, msg, font(13.0), TEXT_DIM);
        return;
    }
    let blocks: Vec<_> = groups.iter().filter(|g| is_block(g.group)).take(4).collect();
    let rest: Vec<_> = groups.iter().filter(|g| !is_block(g.group)).take(4).collect();
    let inner = Rect::from_min_max(Pos2::new(r.left() + 12.0, r.top() + 36.0), Pos2::new(r.right() - 12.0, r.bottom() - 30.0));
    let side_w = if rest.is_empty() { 0.0 } else { 118.0 };
    let die = Rect::from_min_max(inner.min, Pos2::new(inner.right() - if side_w > 0.0 { side_w + 10.0 } else { 0.0 }, inner.bottom()));
    p.rect_filled(die, 10.0, Color32::from_rgb(0x18, 0x18, 0x1A));
    p.rect_stroke(die, 10.0, Stroke::new(1.0, white(26)), egui::StrokeKind::Inside);
    if !blocks.is_empty() {
        let cols = if blocks.len() == 1 { 1 } else { 2 };
        let rows = blocks.len().div_ceil(cols);
        let pad = 8.0;
        let bw = (die.width() - pad * (cols as f32 + 1.0)) / cols as f32;
        let bh = (die.height() - pad * (rows as f32 + 1.0)) / rows as f32;
        for (k, g) in blocks.iter().enumerate() {
            let (cx, cy) = (k % cols, k / cols);
            let b = Rect::from_min_size(Pos2::new(die.left() + pad + cx as f32 * (bw + pad), die.top() + pad + cy as f32 * (bh + pad)), Vec2::new(bw, bh));
            p.rect_filled(b, 7.0, white(8));
            p.text(b.min + Vec2::new(6.0, 5.0), Align2::LEFT_TOP, g.group.label(), bold(10.0), TEXT_SOFT);
            p.text(Pos2::new(b.right() - 6.0, b.top() + 5.0), Align2::RIGHT_TOP, format!("{:.0}°", g.max), bold(11.0), heat_text(g.max));
            let cells = Rect::from_min_max(Pos2::new(b.left() + 6.0, b.top() + 21.0), Pos2::new(b.right() - 6.0, b.bottom() - 6.0));
            let nc = g.cells.len().max(1);
            let ccols = ((nc as f32 * cells.width() / cells.height().max(1.0)).sqrt().ceil() as usize).clamp(1, nc);
            let crows = nc.div_ceil(ccols);
            let gap = 3.0;
            let cw = (cells.width() - gap * (ccols as f32 - 1.0)) / ccols as f32;
            let ch = (cells.height() - gap * (crows as f32 - 1.0)) / crows as f32;
            for (i, c) in g.cells.iter().enumerate() {
                let cr = Rect::from_min_size(Pos2::new(cells.left() + (i % ccols) as f32 * (cw + gap), cells.top() + (i / ccols) as f32 * (ch + gap)), Vec2::new(cw, ch));
                p.rect_filled(cr, 3.0, heat(*c));
            }
        }
    } else {
        p.text(die.center(), Align2::CENTER_CENTER, "This chip doesn't report per-core sensors.", font(12.0), TEXT_DIM);
    }
    if side_w > 0.0 {
        let sx = inner.right() - side_w;
        for (i, g) in rest.iter().enumerate() {
            let rr = Rect::from_min_size(Pos2::new(sx, inner.top() + i as f32 * 40.0), Vec2::new(side_w, 34.0));
            p.rect_filled(rr, 8.0, white(10));
            p.text(Pos2::new(rr.left() + 8.0, rr.top() + 11.0), Align2::LEFT_CENTER, g.group.label(), font(11.0), TEXT_DIM);
            p.text(Pos2::new(rr.right() - 8.0, rr.top() + 11.0), Align2::RIGHT_CENTER, format!("{:.0}°", g.max), bold(11.0), heat_text(g.max));
            bar(p, Rect::from_min_max(Pos2::new(rr.left() + 8.0, rr.bottom() - 9.0), Pos2::new(rr.right() - 8.0, rr.bottom() - 6.0)), heat_pos(g.max).max(0.03), heat(g.max));
        }
    }
    let ly = r.bottom() - 15.0;
    p.text(Pos2::new(r.left() + 12.0, ly), Align2::LEFT_CENTER, "30°", font(11.0), TEXT_FAINT);
    p.text(Pos2::new(r.right() - 12.0, ly), Align2::RIGHT_CENTER, "100°", font(11.0), TEXT_FAINT);
    let sr = Rect::from_min_max(Pos2::new(r.left() + 40.0, ly - 2.5), Pos2::new(r.right() - 46.0, ly + 2.5));
    heat_scale(p, sr);
}

/// Last 10 minutes: average fan speed (filled) and the hottest sensor.
fn history(p: &egui::Painter, r: Rect, s: &Snapshot) {
    glass_card(p, r, 14.0, 0.0, false);
    let ty = r.top() + 20.0;
    p.text(Pos2::new(r.left() + 14.0, ty), Align2::LEFT_CENTER, "Last 10 minutes", bold(13.0), TEXT);
    let mut lx = r.right() - 14.0;
    let legend: Vec<(&str, Color32)> = if s.fans.is_empty() { vec![("Hottest sensor", HOT_LINE)] } else { vec![("Hottest sensor", HOT_LINE), ("Fan speed (avg)", LINE)] };
    for (label, c) in legend {
        let tr = p.text(Pos2::new(lx, ty), Align2::RIGHT_CENTER, label, font(12.0), TEXT_DIM);
        p.rect_filled(Rect::from_center_size(Pos2::new(tr.left() - 12.0, ty), Vec2::new(12.0, 3.0)), 1.5, c);
        lx = tr.left() - 30.0;
    }
    let cr = Rect::from_min_max(Pos2::new(r.left() + 14.0, r.top() + 40.0), Pos2::new(r.right() - 14.0, r.bottom() - 28.0));
    for k in 1..4 {
        let yy = cr.top() + cr.height() * k as f32 / 4.0;
        p.line_segment([Pos2::new(cr.left(), yy), Pos2::new(cr.right(), yy)], Stroke::new(1.0, white(10)));
    }
    p.line_segment([Pos2::new(cr.left(), cr.bottom() + 0.5), Pos2::new(cr.right(), cr.bottom() + 0.5)], Stroke::new(1.0, white(26)));
    // the line fills from the right as samples arrive
    let have = s.history.len();
    let frac = have as f32 / HISTORY as f32;
    let lr = Rect::from_min_max(Pos2::new(cr.right() - cr.width() * frac.max(0.01), cr.top()), cr.max);
    if have >= 2 {
        if !s.fans.is_empty() {
            let lo = s.fans.iter().map(|f| f.min).fold(f32::MAX, f32::min).min(s.history.iter().map(|h| h.0).fold(f32::MAX, f32::min));
            let hi = s.fans.iter().map(|f| f.max).fold(0.0, f32::max);
            let rpm: Vec<f32> = s.history.iter().map(|h| h.0).collect();
            sparkline(p, lr, &rpm, lo * 0.9, hi, LINE, Some(with_alpha(LINE, 0.22)));
        }
        let hot: Vec<f32> = s.history.iter().map(|h| h.1).collect();
        sparkline(p, lr, &hot, 30.0, 100.0, HOT_LINE, None);
    } else {
        p.text(cr.center(), Align2::CENTER_CENTER, "Collecting…", font(12.0), TEXT_FAINT);
    }
    let by = r.bottom() - 14.0;
    p.text(Pos2::new(cr.left(), by), Align2::LEFT_CENTER, "10 min ago", font(11.0), TEXT_FAINT);
    p.text(Pos2::new(cr.right(), by), Align2::RIGHT_CENTER, "now", font(11.0), TEXT_FAINT);
    if let Some((i, (_, c))) = s.history.iter().enumerate().max_by(|a, b| (a.1).1.total_cmp(&(b.1).1)) {
        let ago = (have - 1 - i) as u64 * SAMPLE_SECS;
        let when = if ago < 60 { "just now".to_string() } else { format!("{} min ago", ago / 60) };
        p.text(Pos2::new(cr.center().x, by), Align2::CENTER_CENTER, format!("Peak {c:.0} °C, {when}"), font(11.0), TEXT_FAINT);
    }
}
