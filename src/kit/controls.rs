//! Controls shared by tool pages.

use crate::kit::glass::*;
use egui::{Align2, Color32, Pos2, Rect, Sense, Ui, Vec2};

/// The big round action button at the bottom centre (like the original's Scan/Stop).
pub fn round_button(ui: &mut Ui, center: Pos2, label: &str, progress: Option<f32>, enabled: bool) -> bool {
    let d = 84.0;
    let rect = Rect::from_center_size(center, Vec2::splat(d));
    let id = ui.id().with(("round", label));
    let resp = ui.interact(rect, id, if enabled { Sense::click() } else { Sense::hover() });
    let h = ui.ctx().animate_bool(id, resp.hovered() && enabled);
    let pressed = resp.is_pointer_button_down_on() && enabled;
    let p = ui.painter();
    let body = if pressed { rect.shrink(2.0) } else { rect.expand(h * 2.0) };
    // a soft accent halo, then the tinted Liquid Glass disc
    glow(p, center + Vec2::new(0.0, 6.0), d * 0.95, with_alpha(BLUE, 0.22 + 0.12 * h));
    liquid_capsule(p, body, Some(BLUE), h, if enabled { 1.0 } else { 0.5 });
    if let Some(f) = progress {
        // progress ring hugging the glass edge
        p.add(egui::epaint::PathShape::line(arc_points(center, d / 2.0 + 6.0, 0.0, std::f32::consts::TAU, 96), egui::epaint::PathStroke::new(3.0, white(30))));
        p.add(egui::epaint::PathShape::line(arc_points(center, d / 2.0 + 6.0, -std::f32::consts::FRAC_PI_2, -std::f32::consts::FRAC_PI_2 + f.clamp(0.0, 1.0) * std::f32::consts::TAU, 96), egui::epaint::PathStroke::new(3.0, white(230))));
    }
    p.text(center + Vec2::new(0.0, 1.0), Align2::CENTER_CENTER, label, bold(17.0), Color32::from_black_alpha(50));
    p.text(center, Align2::CENTER_CENTER, label, bold(17.0), Color32::WHITE);
    if enabled && resp.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    resp.clicked() && enabled
}
