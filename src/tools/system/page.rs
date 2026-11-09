//! The Mac Health page in the full window (design A): a status card with the
//! most important recommendation, then the six readings (storage, memory,
//! CPU, battery, network, devices) laid out in two columns. Each section is
//! the same content as its panel sheet, so the two never drift apart.

use super::monitor::{self, Level, Snapshot};
use super::sheets::{Action as SheetAction, Section, Sheet};
use crate::core::tool::{Cx, Recommendation};
use crate::kit::glass::*;
use egui::{Align2, Color32, Pos2, Rect, ScrollArea, Sense, Ui, UiBuilder, Vec2};

const STATUS_H: f32 = 64.0;
const GAP_X: f32 = 24.0;
const GAP_Y: f32 = 28.0;
/// Below this content width the sections stack in one column.
const TWO_COLUMNS_MIN: f32 = 640.0;

fn level_color(l: Level) -> Color32 {
    match l {
        Level::Good => GREEN,
        Level::Fair => AMBER,
        Level::Critical => RED,
    }
}

pub fn page(mon: &monitor::Shared, recs: &[Recommendation], ui: &mut Ui, r: Rect, cx: &mut Cx) {
    let s = mon.lock().unwrap().clone();
    let p = ui.painter().clone();

    // header: model, chip and uptime
    let mut sub: Vec<String> = Vec::new();
    if !s.model.is_empty() {
        sub.push(s.model.clone());
    }
    if !s.chip.is_empty() {
        sub.push(s.chip.clone());
    }
    if s.uptime_secs > 0 {
        sub.push(format!("up {}", human_duration(s.uptime_secs)));
    }
    page_header(&p, r, "Mac Health", &sub.join(" · "), TEXT_DIM);

    let body = Rect::from_min_max(Pos2::new(r.left(), r.top() + PAGE_HEADER_H + 1.0), r.max);
    let x0 = body.left() + 24.0;
    let w = body.width() - 48.0;
    let two = w >= TWO_COLUMNS_MIN;
    let cw = if two { (w - GAP_X) / 2.0 } else { w };

    let order = [Sheet::Storage, Sheet::Memory, Sheet::Cpu, Sheet::Battery, Sheet::Network, Sheet::Devices];
    let sections: Vec<Section> = order.iter().map(|k| Section::new(*k, &s)).collect();
    let heights: Vec<f32> = sections.iter().map(|sec| sec.height(&p, ui.ctx(), &s, cw)).collect();
    let rows: Vec<f32> = if two { heights.chunks(2).map(|c| c.iter().copied().fold(0.0, f32::max)).collect() } else { heights.clone() };
    let total = 20.0 + STATUS_H + GAP_Y + rows.iter().sum::<f32>() + GAP_Y * (rows.len().saturating_sub(1)) as f32 + 24.0;

    let mut child = ui.new_child(UiBuilder::new().max_rect(body).id_salt("health-page"));
    let mut action = SheetAction::None;
    ScrollArea::vertical().id_salt("health-scroll").auto_shrink([false, false]).show(&mut child, |ui| {
        let (area, _) = ui.allocate_exact_size(Vec2::new(body.width(), total), Sense::hover());
        let mut y = area.top() + 20.0;

        status_card(ui, Rect::from_min_size(Pos2::new(x0, y), Vec2::new(w, STATUS_H)), &s, recs.first(), cx);
        y += STATUS_H + GAP_Y;

        for (i, sec) in sections.iter().enumerate() {
            let (col, row) = if two { (i % 2, i / 2) } else { (0, i) };
            let top = y + rows[..row].iter().sum::<f32>() + GAP_Y * row as f32;
            let sr = Rect::from_min_size(Pos2::new(x0 + col as f32 * (cw + GAP_X), top), Vec2::new(cw, heights[i]));
            let mut sui = ui.new_child(UiBuilder::new().max_rect(sr).id_salt(("health-sec", i)));
            let a = sec.show(&mut sui, sr, &s);
            if a != SheetAction::None {
                action = a;
            }
        }
    });
    match action {
        SheetAction::Open(tool) => cx.open_page(tool, ""),
        SheetAction::SpeedTest => monitor::start_speed_test(mon.clone(), ui.ctx().clone()),
        SheetAction::None | SheetAction::Close => {}
    }
}

/// The overall state: a coloured dot, one line on what matters most, and the
/// top recommendation's button when there is one.
fn status_card(ui: &mut Ui, r: Rect, s: &Snapshot, rec: Option<&Recommendation>, cx: &mut Cx) {
    let p = ui.painter().clone();
    glass_card(&p, r, R_CARD, 0.0, false);
    let (title, col) = if !s.ready {
        ("Checking your Mac…", TEXT_FAINT)
    } else {
        match s.health() {
            Level::Good if rec.is_none() => ("All good", GREEN),
            Level::Good => ("Looks good", GREEN),
            l @ Level::Fair => ("Needs a look", level_color(l)),
            l @ Level::Critical => ("Needs attention", level_color(l)),
        }
    };
    let chip = Rect::from_min_size(Pos2::new(r.left() + 14.0, r.center().y - 16.0), Vec2::splat(32.0));
    p.rect_filled(chip, 8.0, with_alpha(col, 0.16));
    p.circle_filled(chip.center(), 5.0, col);

    let mut right = r.right() - 14.0;
    if let Some(rec) = rec {
        let bw = button_width(&p, &rec.action, BTN_PRIMARY);
        let br = Rect::from_min_size(Pos2::new(right - bw, r.center().y - 15.0), Vec2::new(bw, 30.0));
        if pill_button(ui, br, &rec.action, BTN_PRIMARY, true).clicked() {
            cx.push(rec.go.clone());
        }
        right = br.left() - 16.0;
    }
    let tx = chip.right() + 12.0;
    let tw = right - tx;
    let (line1, line2) = match rec {
        Some(rec) => (format!("{title} · {}", rec.title), rec.body.clone()),
        None => (title.to_string(), "Storage, memory, CPU, battery and network look healthy.".to_string()),
    };
    text_fit(&p, Pos2::new(tx, r.top() + 13.0), Align2::LEFT_TOP, &line1, bold(15.0), TEXT, tw);
    text_fit(&p, Pos2::new(tx, r.top() + 34.0), Align2::LEFT_TOP, &line2, font(12.0), TEXT_DIM, tw);
}
