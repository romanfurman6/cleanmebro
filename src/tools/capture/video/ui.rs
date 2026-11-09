//! The viewer's window: title bar with Save, the picture, the transport
//! row and the timeline (frames, trim handles, cuts, selection, playhead).

use super::{Drag, Out, Viewer, SPEEDS};
use crate::kit::glass::*;
use crate::tools::capture::icons::{self, Ico};
use crate::tools::capture::record::timeline::clock_label;
use crate::tools::capture::Capture;
use egui::{Align2, Color32, Key, Pos2, Rect, Sense, Stroke, StrokeKind, Ui, Vec2, ViewportBuilder, ViewportCommand, ViewportId};

const HEAD_H: f32 = 46.0;
const TRANSPORT_H: f32 = 52.0;
const TIMELINE_H: f32 = 118.0;
const PAD: f32 = 18.0;
/// Trim handles.
const TRIM: Color32 = Color32::from_rgb(0xFF, 0xC4, 0x2E);

/// The viewer window, when one is open.
pub fn show(c: &mut Capture, ctx: &egui::Context) {
    let Some(mut v) = c.video.take() else { return };
    v.tick();
    let screen = ctx.input(|i| i.viewport().monitor_size).unwrap_or(Vec2::new(1440.0, 900.0));
    let size = Vec2::new(1120.0f32.min(screen.x * 0.92), 760.0f32.min(screen.y * 0.9));
    let pos = Pos2::new((screen.x - size.x) / 2.0, ((screen.y - size.y) / 2.0).max(30.0));
    let first = !v.opened;
    let builder = ViewportBuilder::default()
        .with_title(crate::tools::capture::windows::plain("capture-video"))
        .with_inner_size(size)
        .with_position(pos)
        .with_min_inner_size([820.0, 560.0])
        .with_transparent(true)
        .with_fullsize_content_view(true)
        .with_titlebar_shown(false)
        .with_title_shown(false)
        .with_has_shadow(true)
        .with_active(true);
    let mut closed = false;
    crate::kit::viewport::show(ctx, ViewportId::from_hash_of("capture-video"), builder, |ui, _| {
        let vctx = ui.ctx().clone();
        if first {
            v.opened = true;
            vctx.send_viewport_cmd(ViewportCommand::Focus);
            crate::kit::native::activate();
        }
        if vctx.input(|i| i.viewport().close_requested()) {
            if v.edited() && !v.asking {
                vctx.send_viewport_cmd(ViewportCommand::CancelClose);
                v.asking = true;
            } else {
                closed = true;
                return;
            }
        }
        let full = ui.max_rect();
        if draw(&mut v, ui, full) {
            closed = true;
        }
    });
    for o in std::mem::take(&mut v.out) {
        match o {
            Out::Saved { path, secs, copy } => {
                if !copy {
                    if let Some(id) = c.store.video_entry(&path).map(|s| s.id) {
                        c.store.set_video(id, path, secs as f32);
                    }
                }
                c.files_at = None;
            }
        }
    }
    if !closed {
        c.video = Some(v);
    }
}

/// The whole window; true to close.
fn draw(v: &mut Viewer, ui: &mut Ui, full: Rect) -> bool {
    let p = ui.painter().clone();
    p.rect_filled(full, 12.0, BG);
    let head = Rect::from_min_size(full.min, Vec2::new(full.width(), HEAD_H));
    let mut close = header(v, ui, head);
    let tl = Rect::from_min_max(Pos2::new(full.left(), full.bottom() - TIMELINE_H), full.max);
    let tr = Rect::from_min_max(Pos2::new(full.left(), tl.top() - TRANSPORT_H), Pos2::new(full.right(), tl.top()));
    let pic = Rect::from_min_max(Pos2::new(full.left(), head.bottom()), Pos2::new(full.right(), tr.top()));
    picture(v, ui, pic);
    hairline(&p, tr.left_top(), tr.right_top());
    transport(v, ui, tr);
    timeline(v, ui, tl);
    if keys(v, ui) {
        close = true;
    }
    // frames while playing, and for a moment after a seek
    if v.playing() || v.drag.is_some() || v.kick.elapsed().as_secs_f32() < 0.6 || v.saving {
        ui.ctx().request_repaint();
    }
    if let Some((m, at)) = &v.status {
        if at.elapsed().as_secs_f32() < 2.5 {
            let g = p.layout_no_wrap(m.clone(), font(13.0), TEXT);
            let r = Rect::from_center_size(Pos2::new(pic.center().x, pic.bottom() - 30.0), g.size() + Vec2::new(28.0, 16.0));
            p.rect_filled(r, r.height() / 2.0, RAISED);
            p.rect_stroke(r, r.height() / 2.0, Stroke::new(1.0, white(30)), StrokeKind::Inside);
            p.galley(r.center() - g.size() / 2.0, g, TEXT);
            ui.ctx().request_repaint_after(std::time::Duration::from_millis(100));
        } else {
            v.status = None;
        }
    }
    close
}

fn header(v: &mut Viewer, ui: &mut Ui, r: Rect) -> bool {
    let p = ui.painter().clone();
    p.rect_filled(r, egui::CornerRadius { nw: 12, ne: 12, sw: 0, se: 0 }, FOOTER);
    hairline(&p, r.left_bottom(), r.right_bottom());
    let drag = ui.interact(Rect::from_min_max(r.min + Vec2::new(90.0, 0.0), r.max - Vec2::new(360.0, 0.0)), ui.scope_id().with("drag"), Sense::click_and_drag());
    if drag.drag_started() {
        ui.ctx().send_viewport_cmd(ViewportCommand::StartDrag);
    }
    let y = r.center().y;
    let t = p.text(Pos2::new(r.left() + 90.0, y), Align2::LEFT_CENTER, v.title(), bold(13.0), TEXT);
    let meta = if v.edited() { format!("{}  →  {}", clock_label(v.duration()), clock_label(v.edl.length())) } else { clock_label(v.duration()) };
    p.text(Pos2::new(t.right() + 8.0, y), Align2::LEFT_CENTER, meta, font(12.0), TEXT_FAINT);

    let mut close = false;
    let mut x = r.right() - 14.0;
    let mut button = |ui: &mut Ui, label: &str, kind: u8, on: bool| {
        let w = button_width(ui.painter(), label, kind);
        let br = Rect::from_min_max(Pos2::new(x - w, y - 14.0), Pos2::new(x, y + 14.0));
        x = br.left() - 8.0;
        pill_button(ui, br, label, kind, on).clicked()
    };
    if v.asking {
        if button(ui, "Save", BTN_PRIMARY, !v.saving) {
            v.asking = false;
            v.save(false);
        }
        if button(ui, "Discard", BTN_DANGER, true) {
            close = true;
        }
        if button(ui, "Keep editing", BTN_SECONDARY, true) {
            v.asking = false;
        }
        p.text(Pos2::new(x - 4.0, y), Align2::RIGHT_CENTER, "Close without saving the cuts?", font(12.0), AMBER);
        return close;
    }
    if button(ui, if v.saving { "Saving…" } else { "Save  ⌘S" }, BTN_PRIMARY, v.edited() && !v.saving) {
        v.save(false);
    }
    if button(ui, "Save copy", BTN_SECONDARY, v.player.is_some() && !v.saving) {
        v.save(true);
    }
    if v.edited() {
        chip(&p, Pos2::new(x - 2.0, y), Align2::RIGHT_CENTER, "Unsaved cuts", CHIP_ASK);
    }
    close
}

fn picture(v: &mut Viewer, ui: &mut Ui, r: Rect) {
    let p = ui.painter().clone();
    p.rect_filled(r, 0.0, Color32::from_rgb(0x0E, 0x0E, 0x10));
    if let Some(e) = &v.error {
        p.text(r.center(), Align2::CENTER_CENTER, format!("Can't play this video: {e}"), font(13.0), TEXT_DIM);
        return;
    }
    if let Some(f) = v.player.as_ref().and_then(|pl| pl.frame()) {
        let img = egui::ColorImage::from_rgba_unmultiplied([f.w, f.h], &f.rgba);
        match &mut v.tex {
            Some(t) if t.size() == [f.w, f.h] => t.set(img, egui::TextureOptions::LINEAR),
            slot => *slot = Some(ui.ctx().load_texture("capture-video-frame", img, egui::TextureOptions::LINEAR)),
        }
    }
    let area = r.shrink(16.0);
    let (w, h) = (v.size.0.max(1) as f32, v.size.1.max(1) as f32);
    let s = (area.width() / w).min(area.height() / h);
    let fit = Rect::from_center_size(area.center(), Vec2::new(w * s, h * s));
    match &v.tex {
        Some(t) => {
            p.image(t.id(), fit, Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)), Color32::WHITE);
        }
        None => {
            p.rect_filled(fit, 6.0, Color32::from_rgb(0x1A, 0x1A, 0x1E));
        }
    }
    // a cut piece shows dimmed, so it's clear it won't be in the file
    if v.player.is_some() && !v.edl.kept(v.time()) && v.time() < v.duration() - 1e-3 {
        p.rect_filled(fit, 0.0, Color32::from_black_alpha(150));
        p.text(fit.center(), Align2::CENTER_CENTER, "Cut", bold(15.0), TEXT_SOFT);
    }
    let resp = ui.interact(fit, ui.scope_id().with("pic"), Sense::click());
    if resp.clicked() {
        v.toggle_play();
    }
    if !v.playing() && resp.hovered() {
        p.circle_filled(fit.center(), 30.0, Color32::from_black_alpha(140));
        icons::draw(&p, Ico::Play, Rect::from_center_size(fit.center() + Vec2::new(2.0, 0.0), Vec2::splat(24.0)), Color32::WHITE);
    }
}

fn plain_icon(ui: &mut Ui, r: Rect, ico: Ico, tip: &str, on: bool) -> bool {
    let resp = ui.interact(r, ui.scope_id().with(("vi", tip)), if on { Sense::click() } else { Sense::hover() });
    let p = ui.painter();
    if on && resp.hovered() {
        p.rect_filled(r, 7.0, white(20));
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    icons::draw(p, ico, Rect::from_center_size(r.center(), Vec2::splat(15.0)), if on { TEXT } else { TEXT_MUTED });
    resp.on_hover_text(tip).clicked() && on
}

fn transport(v: &mut Viewer, ui: &mut Ui, r: Rect) {
    let p = ui.painter().clone();
    let y = r.center().y;
    // play / pause
    let pr = Rect::from_center_size(Pos2::new(r.left() + PAD + 17.0, y), Vec2::splat(34.0));
    let presp = ui.interact(pr, ui.scope_id().with("play"), Sense::click());
    p.circle_filled(pr.center(), 17.0, white(if presp.hovered() { 40 } else { 26 }));
    let playing = v.playing();
    icons::draw(&p, if playing { Ico::Pause } else { Ico::Play }, Rect::from_center_size(pr.center() + Vec2::new(if playing { 0.0 } else { 1.5 }, 0.0), Vec2::splat(15.0)), TEXT);
    if presp.on_hover_text(if playing { "Pause (Space)" } else { "Play (Space)" }).clicked() {
        v.toggle_play();
    }
    // time in the edited video
    let now = v.edl.to_edited(v.time()).unwrap_or_else(|| v.edl.to_edited(v.edl.playable(v.time()).unwrap_or(v.edl.end())).unwrap_or(v.edl.length()));
    let tr = p.text(Pos2::new(pr.right() + 14.0, y), Align2::LEFT_CENTER, clock_label(now), mono(13.0), TEXT);
    p.text(Pos2::new(tr.right() + 4.0, y), Align2::LEFT_CENTER, format!("/ {}", clock_label(v.edl.length())), mono(13.0), TEXT_FAINT);
    // speed
    let labels: Vec<String> = SPEEDS.iter().map(|s| format!("{s}×")).collect();
    let refs: Vec<&str> = labels.iter().map(String::as_str).collect();
    let sr = Rect::from_min_size(Pos2::new(tr.right() + 76.0, y - 14.0), Vec2::new(150.0, 28.0));
    if let Some(k) = segmented(ui, sr, ui.scope_id().with("speed"), &refs, v.speed) {
        v.set_speed(k);
    }

    // right: Cut, Keep, undo / redo
    let mut x = r.right() - PAD;
    let has_sel = v.sel.is_some();
    let cut_w = 150.0;
    let cr = Rect::from_min_max(Pos2::new(x - cut_w, y - 15.0), Pos2::new(x, y + 15.0));
    if pill_button(ui, cr, "", BTN_PRIMARY, has_sel).on_hover_text("Cut the chosen stretch out (⌫)").clicked() {
        v.cut_selection();
    }
    let col = if has_sel { Color32::WHITE } else { with_alpha(Color32::WHITE, 0.5) };
    icons::draw(&p, Ico::Scissors, Rect::from_center_size(Pos2::new(cr.left() + 22.0, y), Vec2::splat(14.0)), col);
    p.text(Pos2::new(cr.left() + 36.0, y), Align2::LEFT_CENTER, "Cut selection", bold(13.0), col);
    x = cr.left() - 8.0;
    let kw = button_width(&p, "Keep only this", BTN_SECONDARY);
    let kr = Rect::from_min_max(Pos2::new(x - kw, y - 15.0), Pos2::new(x, y + 15.0));
    if pill_button(ui, kr, "Keep only this", BTN_SECONDARY, has_sel).on_hover_text("Trim to the chosen stretch").clicked() {
        v.keep_selection();
    }
    x = kr.left() - 16.0;
    let rr = Rect::from_min_size(Pos2::new(x - 30.0, y - 15.0), Vec2::splat(30.0));
    if plain_icon(ui, rr, Ico::Redo, "Redo (⇧⌘Z)", !v.redo.is_empty()) {
        v.redo();
    }
    let ur = rr.translate(Vec2::new(-34.0, 0.0));
    if plain_icon(ui, ur, Ico::Undo, "Undo (⌘Z)", !v.undo.is_empty()) {
        v.undo();
    }
    if !has_sel && v.edl.duration > 0.0 {
        p.text(Pos2::new(ur.left() - 14.0, y), Align2::RIGHT_CENTER, "Drag on the timeline to choose a stretch", font(12.0), TEXT_FAINT);
    }
}

/// A short label for a ruler tick.
fn tick_label(t: f64) -> String {
    let s = t.round() as u64;
    format!("{}:{:02}", s / 60, s % 60)
}

/// Seconds between ruler ticks so they're at least `min_px` apart.
pub fn tick_step(duration: f64, width: f32, min_px: f32) -> f64 {
    let per_px = duration / width.max(1.0) as f64;
    [1.0, 2.0, 5.0, 10.0, 15.0, 30.0, 60.0, 120.0, 300.0, 600.0, 900.0, 1800.0, 3600.0].into_iter().find(|s| s / per_px >= min_px as f64).unwrap_or(3600.0)
}

fn timeline(v: &mut Viewer, ui: &mut Ui, r: Rect) {
    let p = ui.painter().clone();
    p.rect_filled(r, egui::CornerRadius { nw: 0, ne: 0, sw: 12, se: 12 }, FOOTER);
    hairline(&p, r.left_top(), r.right_top());
    let d = v.duration().max(0.001);
    let lane = Rect::from_min_max(Pos2::new(r.left() + PAD + 8.0, r.top() + 30.0), Pos2::new(r.right() - PAD - 8.0, r.bottom() - 22.0));
    let x_of = |t: f64| lane.left() + (t / d) as f32 * lane.width();
    let t_of = |x: f32| (((x - lane.left()) / lane.width()) as f64 * d).clamp(0.0, d);

    // ruler
    let step = tick_step(d, lane.width(), 70.0);
    let mut t = 0.0;
    while t <= d + 1e-6 {
        let x = x_of(t);
        p.line_segment([Pos2::new(x, r.top() + 20.0), Pos2::new(x, r.top() + 25.0)], Stroke::new(1.0, white(60)));
        p.text(Pos2::new(x, r.top() + 12.0), Align2::CENTER_CENTER, tick_label(t), font(10.5), TEXT_FAINT);
        t += step;
    }

    // frames
    if v.strip.is_empty() && !v.strip_imgs.is_empty() {
        let ctx = ui.ctx().clone();
        v.strip = v.strip_imgs.iter().enumerate().map(|(k, img)| ctx.load_texture(format!("capture-video-strip-{k}"), egui::ColorImage::from_rgba_unmultiplied([img.width() as usize, img.height() as usize], img.as_raw()), egui::TextureOptions::LINEAR)).collect();
    }
    p.rect_filled(lane, 6.0, Color32::from_rgb(0x26, 0x26, 0x2B));
    if !v.strip.is_empty() {
        let n = v.strip.len();
        let sw = lane.width() / n as f32;
        let clip = p.with_clip_rect(lane);
        for (k, tex) in v.strip.iter().enumerate() {
            let cell = Rect::from_min_size(Pos2::new(lane.left() + sw * k as f32, lane.top()), Vec2::new(sw + 0.5, lane.height()));
            // crop the middle of the frame to the cell's shape
            let [iw, ih] = tex.size();
            let (ia, ca) = (iw as f32 / ih.max(1) as f32, cell.width() / cell.height());
            let uv = if ia > ca {
                let f = ca / ia;
                Rect::from_min_max(Pos2::new(0.5 - f / 2.0, 0.0), Pos2::new(0.5 + f / 2.0, 1.0))
            } else {
                let f = ia / ca;
                Rect::from_min_max(Pos2::new(0.0, 0.5 - f / 2.0), Pos2::new(1.0, 0.5 + f / 2.0))
            };
            clip.image(tex.id(), cell, uv, Color32::WHITE);
        }
    }

    // cut pieces: darkened and striped
    for (a, b) in v.edl.removed() {
        let cr = Rect::from_min_max(Pos2::new(x_of(a), lane.top()), Pos2::new(x_of(b), lane.bottom()));
        p.rect_filled(cr, 0.0, Color32::from_black_alpha(185));
        let clip = p.with_clip_rect(cr.intersect(lane));
        let mut sx = cr.left() - lane.height();
        while sx < cr.right() {
            clip.line_segment([Pos2::new(sx, lane.bottom()), Pos2::new(sx + lane.height(), lane.top())], Stroke::new(1.0, white(28)));
            sx += 8.0;
        }
    }

    // what's kept: a yellow frame with handles at both ends
    let (ks, ke) = (x_of(v.edl.start()), x_of(v.edl.end()));
    let kept = Rect::from_min_max(Pos2::new(ks, lane.top()), Pos2::new(ke, lane.bottom()));
    p.rect_stroke(kept, 4.0, Stroke::new(2.0, TRIM), StrokeKind::Outside);
    for (x, left) in [(ks, true), (ke, false)] {
        let hr = if left { Rect::from_min_max(Pos2::new(x - 10.0, lane.top() - 2.0), Pos2::new(x, lane.bottom() + 2.0)) } else { Rect::from_min_max(Pos2::new(x, lane.top() - 2.0), Pos2::new(x + 10.0, lane.bottom() + 2.0)) };
        let cr = if left { egui::CornerRadius { nw: 4, sw: 4, ne: 0, se: 0 } } else { egui::CornerRadius { nw: 0, sw: 0, ne: 4, se: 4 } };
        p.rect_filled(hr, cr, TRIM);
        p.line_segment([Pos2::new(hr.center().x, hr.center().y - 7.0), Pos2::new(hr.center().x, hr.center().y + 7.0)], Stroke::new(1.5, Color32::from_black_alpha(140)));
    }

    // selection
    if let Some((a, b)) = v.sel {
        let sr = Rect::from_min_max(Pos2::new(x_of(a), lane.top()), Pos2::new(x_of(b), lane.bottom()));
        p.rect_filled(sr, 0.0, with_alpha(ACCENT, 0.28));
        p.rect_stroke(sr, 0.0, Stroke::new(1.5, ACCENT), StrokeKind::Inside);
        p.text(Pos2::new(sr.center().x, lane.bottom() + 11.0), Align2::CENTER_CENTER, format!("{:.1} s selected", b - a), font(11.0), ACCENT);
    }

    // playhead
    let px = x_of(v.time());
    p.line_segment([Pos2::new(px, r.top() + 22.0), Pos2::new(px, lane.bottom() + 4.0)], Stroke::new(2.0, Color32::WHITE));
    p.circle_filled(Pos2::new(px, r.top() + 22.0), 5.0, Color32::WHITE);

    // input
    let hit = Rect::from_min_max(Pos2::new(lane.left() - 12.0, r.top() + 4.0), Pos2::new(lane.right() + 12.0, lane.bottom() + 4.0));
    let resp = ui.interact(hit, ui.scope_id().with("timeline"), Sense::click_and_drag());
    let near = |x: f32, at: f32| (x - at).abs() <= 10.0;
    if let Some(pos) = resp.hover_pos() {
        let on_handle = pos.y >= lane.top() - 4.0 && (near(pos.x, ks - 5.0) || near(pos.x, ke + 5.0));
        ui.ctx().set_cursor_icon(if on_handle { egui::CursorIcon::ResizeHorizontal } else { egui::CursorIcon::Text });
    }
    if resp.drag_started() {
        if let Some(pos) = resp.interact_pointer_pos() {
            v.drag = Some(if pos.y < lane.top() - 4.0 {
                Drag::Scrub
            } else if near(pos.x, ks - 5.0) {
                v.begin_trim();
                Drag::Trim { end: false }
            } else if near(pos.x, ke + 5.0) {
                v.begin_trim();
                Drag::Trim { end: true }
            } else {
                Drag::Select(t_of(pos.x))
            });
            if let Some(pl) = &v.player {
                pl.pause();
            }
        }
    }
    if let (Some(drag), Some(pos)) = (v.drag, resp.interact_pointer_pos()) {
        let t = t_of(pos.x);
        match drag {
            Drag::Scrub => v.seek(t),
            Drag::Trim { end } => {
                v.trim_to(end, t);
                v.seek(if end { v.edl.end() } else { v.edl.start() });
            }
            Drag::Select(a) => {
                v.sel = (t - a).abs().ge(&0.05).then(|| (a.min(t), a.max(t)));
                v.seek(t);
            }
        }
    }
    if resp.drag_stopped() {
        if let Some(Drag::Trim { .. }) = v.drag {
            v.end_trim();
        }
        v.drag = None;
    }
    if resp.clicked() {
        if let Some(pos) = resp.interact_pointer_pos() {
            v.sel = None;
            v.seek(t_of(pos.x));
        }
    }
}

/// Keyboard: Space, arrows, ⌫, ⌘Z / ⇧⌘Z, ⌘S, Esc. True to close.
fn keys(v: &mut Viewer, ui: &mut Ui) -> bool {
    let (space, left, right, del, undo, redo, save, esc, shift) = ui.input(|i| {
        let cmd = i.modifiers.command;
        (
            i.key_pressed(Key::Space),
            i.key_pressed(Key::ArrowLeft),
            i.key_pressed(Key::ArrowRight),
            i.key_pressed(Key::Backspace) || i.key_pressed(Key::Delete),
            cmd && !i.modifiers.shift && i.key_pressed(Key::Z),
            cmd && i.modifiers.shift && i.key_pressed(Key::Z),
            cmd && i.key_pressed(Key::S),
            i.key_pressed(Key::Escape),
            i.modifiers.shift,
        )
    });
    let jump = if shift { 5.0 } else { 1.0 };
    if space {
        v.toggle_play();
    }
    if left {
        v.step(-jump);
    }
    if right {
        v.step(jump);
    }
    if del {
        v.cut_selection();
    }
    if undo {
        v.undo();
    }
    if redo {
        v.redo();
    }
    if save {
        v.save(false);
    }
    if esc {
        if v.sel.is_some() {
            v.sel = None;
        } else if v.asking {
            v.asking = false;
        } else if v.edited() {
            v.asking = true;
        } else {
            return true;
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ruler_ticks_stay_apart() {
        assert_eq!(tick_step(60.0, 900.0, 70.0), 5.0);
        assert_eq!(tick_step(3600.0, 900.0, 70.0), 300.0);
        assert_eq!(tick_step(5.0, 900.0, 70.0), 1.0);
    }
}
