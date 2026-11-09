//! The Devices widget in every size: the emptiest device as a ring (1:1),
//! a glyph and charge per device (1:2), a ring per device (1:4), a list
//! with AirPods split into left, right and case (2:4) and every part of
//! every device (4:4).

use super::devices::{self, Device};
use super::monitor::Snapshot;
use crate::core::tool::Size;
use crate::kit::glass::*;
use crate::kit::mechanics as m;
use crate::kit::tile;
use egui::{Align2, Pos2, Rect, Response, Ui, Vec2};

fn pct_text(b: Option<u8>) -> String {
    b.map(|b| format!("{b}%")).unwrap_or_else(|| "–".into())
}

fn frac(b: Option<u8>) -> f32 {
    b.map(|b| (b as f32 / 100.0).clamp(0.02, 1.0)).unwrap_or(0.0)
}

fn col(b: Option<u8>) -> egui::Color32 {
    b.map(devices::color).unwrap_or(TEXT_FAINT)
}

fn txt_col(b: Option<u8>) -> egui::Color32 {
    b.map(devices::text_color).unwrap_or(TEXT_FAINT)
}

/// "1 low" or "3 connected".
pub fn note(devs: &[Device]) -> String {
    let low = devs.iter().filter(|d| devices::is_low(d)).count();
    if low > 0 {
        format!("{low} low")
    } else {
        format!("{} connected", devs.len())
    }
}

/// A ring with the device's glyph inside and its charge under it.
fn ring_cell(p: &egui::Painter, c: Pos2, r: f32, d: &Device, glyph_sz: f32) {
    let b = d.battery();
    tile::ring(p, c, r, (r * 0.24).max(3.5), frac(b), col(b));
    icon(p, d.kind.icon(), Rect::from_center_size(c, Vec2::splat(glyph_sz)), TEXT_SOFT);
}

pub fn widget(s: &Snapshot, size: Size, ui: &mut Ui, r: Rect, wid: egui::Id, sel: bool) -> Response {
    let devs = devices::lowest_first(&s.devices);
    if size == Size::S11 {
        return match devs.first() {
            Some(d) if d.battery().is_some() => {
                let b = d.battery();
                let resp = tile::card(ui, r, wid, sel);
                let p = ui.painter();
                let c = Pos2::new(r.center().x, r.top() + 32.0);
                ring_cell(p, c, 22.0, d, 17.0);
                text_fit(p, Pos2::new(r.center().x, r.bottom() - 9.0), Align2::CENTER_BOTTOM, &pct_text(b), bold(11.0), txt_col(b), r.width() - 10.0);
                resp
            }
            Some(d) => tile::small(ui, r, wid, d.kind.icon(), tint::HEALTH, &d.name, "Connected", sel),
            None => tile::small(ui, r, wid, Icon::Bluetooth, tint::HEALTH, "Devices", "None", sel),
        };
    }
    let resp = tile::card(ui, r, wid, sel);
    let p = ui.painter();
    match size {
        Size::S12 => {
            let x0 = r.left() + tile::PAD;
            let x1 = r.right() - tile::PAD;
            tool_chip(p, Rect::from_min_size(Pos2::new(x0, r.top() + 9.0), Vec2::splat(18.0)), Icon::Bluetooth, tint::HEALTH);
            p.text(Pos2::new(x1, r.top() + 18.0), Align2::RIGHT_CENTER, devs.len().to_string(), font(11.0), TEXT_FAINT);
            text_fit(p, Pos2::new(x0 + 24.0, r.top() + 18.0), Align2::LEFT_CENTER, "Devices", font(11.0), TEXT_DIM, x1 - x0 - 40.0);
            if devs.is_empty() {
                text_fit(p, Pos2::new(x0, r.bottom() - 12.0), Align2::LEFT_BOTTOM, "None connected", font(12.0), TEXT_FAINT, x1 - x0);
                return resp;
            }
            let n = devs.len().min(3);
            let cw = (x1 - x0) / n as f32;
            for (k, d) in devs.iter().take(n).enumerate() {
                let cx = x0 + cw * (k as f32 + 0.5);
                icon(p, d.kind.icon(), Rect::from_center_size(Pos2::new(cx, r.bottom() - 34.0), Vec2::splat(16.0)), TEXT_SOFT);
                text_fit(p, Pos2::new(cx, r.bottom() - 10.0), Align2::CENTER_BOTTOM, &pct_text(d.battery()), bold(12.0), txt_col(d.battery()), cw - 4.0);
            }
        }
        Size::S14 => {
            if devs.is_empty() {
                tool_chip(p, Rect::from_min_size(Pos2::new(r.left() + 12.0, r.center().y - 17.0), Vec2::splat(34.0)), Icon::Bluetooth, tint::HEALTH);
                let x0 = r.left() + 58.0;
                text_fit(p, Pos2::new(x0, r.center().y - 9.0), Align2::LEFT_CENTER, "No devices connected", bold(13.0), TEXT, r.right() - 12.0 - x0);
                text_fit(p, Pos2::new(x0, r.center().y + 10.0), Align2::LEFT_CENTER, "Turn on AirPods, a mouse or a keyboard", font(11.0), TEXT_FAINT, r.right() - 12.0 - x0);
                return resp;
            }
            let n = devs.len().clamp(1, 5);
            let cw = (r.width() - 2.0 * tile::PAD) / n.max(4) as f32;
            let x0 = r.left() + tile::PAD + (r.width() - 2.0 * tile::PAD - cw * n as f32) / 2.0;
            for (k, d) in devs.iter().take(n).enumerate() {
                let cx = x0 + cw * (k as f32 + 0.5);
                ring_cell(p, Pos2::new(cx, r.top() + 33.0), 19.0, d, 15.0);
                text_fit(p, Pos2::new(cx, r.bottom() - 8.0), Align2::CENTER_BOTTOM, &pct_text(d.battery()), bold(11.0), txt_col(d.battery()), cw - 4.0);
            }
        }
        Size::S24 => {
            let body = tile::head(p, r, Icon::Bluetooth, tint::HEALTH, "Devices", &note(&devs));
            if devs.is_empty() {
                tile::empty_note(p, body, "No Bluetooth devices connected");
                return resp;
            }
            let lh = (body.height() / 3.0).min(40.0);
            for (k, d) in devs.iter().take(3).enumerate() {
                let lr = Rect::from_min_size(Pos2::new(body.left(), body.top() + k as f32 * lh), Vec2::new(body.width(), lh));
                if k > 0 {
                    hairline(p, lr.left_top(), lr.right_top());
                }
                device_line(p, lr, d);
            }
        }
        _ => large(s, &devs, p, r),
    }
    resp
}

/// One device in a list: glyph, name with its parts or kind under it, a
/// bar and the charge.
fn device_line(p: &egui::Painter, lr: Rect, d: &Device) {
    let b = d.battery();
    let cy = lr.center().y;
    icon(p, d.kind.icon(), Rect::from_center_size(Pos2::new(lr.left() + 9.0, cy), Vec2::splat(17.0)), TEXT_SOFT);
    let vr = p.text(Pos2::new(lr.right(), cy), Align2::RIGHT_CENTER, pct_text(b), bold(12.5), txt_col(b));
    let bw = 64.0;
    let bx1 = vr.left() - 10.0;
    bar(p, Rect::from_min_max(Pos2::new(bx1 - bw, cy - 2.0), Pos2::new(bx1, cy + 2.0)), frac(b).max(0.01), col(b));
    let tx = lr.left() + 26.0;
    let tw = bx1 - bw - tx - 10.0;
    text_fit(p, Pos2::new(tx, cy - 1.0), Align2::LEFT_BOTTOM, &d.name, font(12.5), TEXT, tw);
    text_fit(p, Pos2::new(tx, cy + 1.0), Align2::LEFT_TOP, &d.detail(), font(11.0), TEXT_FAINT, tw);
}

/// 4:4: earbuds get a card with three rings, other devices a line each.
fn large(s: &Snapshot, devs: &[Device], p: &egui::Painter, r: Rect) {
    let body = tile::head(p, r, Icon::Bluetooth, tint::HEALTH, &format!("Devices · {} connected", devs.len()), "");
    if devs.is_empty() {
        tile::empty_note(p, body, "No Bluetooth devices connected");
        return;
    }
    let mut y = body.top();
    let foot_h = 18.0;
    let bottom = body.bottom() - foot_h - 6.0;
    // earbuds first, as a card with a ring per part
    let pods: Vec<&Device> = devs.iter().filter(|d| d.split()).collect();
    for d in pods.iter().take(1) {
        let cr = Rect::from_min_size(Pos2::new(body.left(), y), Vec2::new(body.width(), 112.0));
        p.rect_filled(cr, 10.0, white(10));
        icon(p, d.kind.icon(), Rect::from_center_size(Pos2::new(cr.left() + 19.0, cr.top() + 17.0), Vec2::splat(17.0)), TEXT_SOFT);
        text_fit(p, Pos2::new(cr.left() + 36.0, cr.top() + 17.0), Align2::LEFT_CENTER, &d.name, bold(13.0), TEXT, cr.width() - 48.0);
        let cw = cr.width() / 3.0;
        for (k, (label, v)) in d.parts().iter().enumerate() {
            let c = Pos2::new(cr.left() + cw * (k as f32 + 0.5), cr.top() + 60.0);
            tile::ring(p, c, 22.0, 5.0, frac(*v), col(*v));
            p.text(c, Align2::CENTER_CENTER, pct_text(*v), bold(12.0), txt_col(*v));
            p.text(Pos2::new(c.x, cr.bottom() - 10.0), Align2::CENTER_BOTTOM, *label, font(11.0), TEXT_DIM);
        }
        y = cr.bottom() + 8.0;
    }
    let lh = 40.0;
    for d in devs.iter().filter(|d| !pods.first().is_some_and(|f| std::ptr::eq(*f, *d))) {
        if y + lh > bottom + 0.5 {
            break;
        }
        let lr = Rect::from_min_size(Pos2::new(body.left(), y), Vec2::new(body.width(), lh));
        p.rect_filled(lr, 10.0, white(10));
        device_line(p, lr.shrink2(Vec2::new(10.0, 0.0)), d);
        y += lh + 6.0;
    }
    let low = devs.iter().find(|d| devices::is_low(d));
    let alerts = if s.bt_prefs.alerts { format!("Alerts below {}% and {}%.", m::BT_LOW_BATTERY_MAX_PCT + 1, m::BT_CRITICAL_BATTERY_MAX_PCT + 1) } else { "Low battery alerts are off.".into() };
    let line = match low {
        Some(d) => format!("{} is low. {alerts}", d.name),
        None => alerts,
    };
    text_fit(p, Pos2::new(body.left(), body.bottom()), Align2::LEFT_BOTTOM, &line, font(11.0), TEXT_DIM, body.width());
}
