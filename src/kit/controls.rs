//! Controls shared by tool pages.

use crate::kit::glass::*;
use egui::{Pos2, Rect, Ui, Vec2};

/// A standard primary button centred on `center` (replaces the old giant
/// round Scan/Stop button). `progress` is drawn as a thin bar under it.
pub fn action_button(ui: &mut Ui, center: Pos2, label: &str, progress: Option<f32>, enabled: bool) -> bool {
    let w = button_width(ui.painter(), label, BTN_PRIMARY).max(96.0);
    let rect = Rect::from_center_size(center, Vec2::new(w, 34.0));
    let clicked = pill_button(ui, rect, label, BTN_PRIMARY, enabled).clicked();
    if let Some(f) = progress {
        bar(ui.painter(), Rect::from_min_size(Pos2::new(rect.left(), rect.bottom() + 8.0), Vec2::new(rect.width(), 4.0)), f, ACCENT);
    }
    clicked && enabled
}
