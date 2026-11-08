//! Capture's own floating windows: the preview after a capture, pinned
//! captures, the editor, the countdown, short messages, the Screen Recording
//! card and the full-screen color picker / ruler. All are egui viewports
//! titled "plain-…" so the shell puts no glass behind them.

use super::editor::{self, Editor, Request};
use super::grab::Grab;
use super::icons::{self, Ico};
use super::{mac, render, Capture, Measure, Purpose};
use crate::core::tool::Cx;
use crate::kit::glass::*;
use egui::{Align2, Color32, Pos2, Rect, Sense, Stroke, StrokeKind, Vec2, ViewportBuilder, ViewportCommand, ViewportId};
use image::RgbaImage;
use std::time::Instant;

pub struct Countdown {
    pub until: Instant,
    pub total: u32,
    pub grab: Grab,
    pub purpose: Purpose,
}

pub struct Preview {
    id: Option<u64>,
    img: RgbaImage,
    note: String,
    /// Names its window; `born` moves while the mouse is over it.
    key: Instant,
    born: Instant,
    tex: Option<egui::TextureHandle>,
    frames: u32,
}

impl Preview {
    pub fn new(id: Option<u64>, img: RgbaImage, note: String) -> Preview {
        Preview { id, img, note, key: Instant::now(), born: Instant::now(), tex: None, frames: 0 }
    }
}

pub struct Pin {
    n: u64,
    id: Option<u64>,
    img: RgbaImage,
    tex: Option<egui::TextureHandle>,
    /// Size in points at scale 1 (true size).
    base: Vec2,
    scale: f32,
    offset: f32,
    /// Where and how big it opened; later moves and resizes are the window's own.
    start: Option<(Pos2, Vec2)>,
    frames: u32,
}

impl Pin {
    pub fn new(n: u64, id: Option<u64>, img: RgbaImage, ppp: f32, offset: f32) -> Pin {
        let base = Vec2::new(img.width() as f32, img.height() as f32) / ppp.max(1.0);
        Pin { n, id, img, tex: None, base, scale: 1.0, offset, start: None, frames: 0 }
    }
}

pub struct EditorWin {
    pub ed: Editor,
    opened: bool,
}

impl EditorWin {
    pub fn new(ed: Editor) -> EditorWin {
        EditorWin { ed, opened: false }
    }
}

pub struct Overlay {
    mode: Measure,
    img: RgbaImage,
    tex: Option<egui::TextureHandle>,
    drag: Option<(Pos2, Pos2)>,
    frames: u32,
}

impl Overlay {
    pub fn new(mode: Measure, img: RgbaImage) -> Overlay {
        Overlay { mode, img, tex: None, drag: None, frames: 0 }
    }
}

fn texture(ctx: &egui::Context, slot: &mut Option<egui::TextureHandle>, name: &str, img: &RgbaImage) -> egui::TextureId {
    if slot.is_none() {
        let ci = egui::ColorImage::from_rgba_unmultiplied([img.width() as usize, img.height() as usize], img.as_raw());
        *slot = Some(ctx.load_texture(name, ci, egui::TextureOptions::LINEAR));
    }
    slot.as_ref().unwrap().id()
}

/// A window title that keeps the shell's glass away.
fn plain(name: &str) -> String {
    format!("{}{name}", crate::kit::native::PLAIN_WINDOW)
}

fn floating(title: &str, pos: Pos2, size: Vec2) -> ViewportBuilder {
    ViewportBuilder::default()
        .with_title(title)
        .with_decorations(false)
        .with_transparent(true)
        .with_resizable(false)
        .with_always_on_top()
        .with_taskbar(false)
        .with_has_shadow(true)
        .with_position(pos)
        .with_inner_size(size)
}

fn screen_size(ctx: &egui::Context) -> Vec2 {
    ctx.input(|i| i.viewport().monitor_size).filter(|s| s.x > 100.0).unwrap_or(Vec2::new(1512.0, 982.0))
}

/// Everything Capture shows outside the shell, once per ui pass.
pub fn show(c: &mut Capture, ctx: &egui::Context, _cx: &mut Cx) {
    let screen = screen_size(ctx);
    countdown(c, ctx, screen);
    permission(c, ctx, screen);
    preview(c, ctx, screen);
    pins(c, ctx, screen);
    editor_window(c, ctx, screen);
    overlay(c, ctx, screen);
    hud(c, ctx, screen);
}

// ---------------------------------------------------------------- countdown

fn countdown(c: &mut Capture, ctx: &egui::Context, screen: Vec2) {
    let Some(cd) = &c.countdown else { return };
    let left = cd.until.saturating_duration_since(Instant::now()).as_secs_f32();
    let total = cd.total as f32;
    let what = match (cd.grab, cd.purpose) {
        (Grab::Area, _) => "Then drag out an area",
        (Grab::Window, _) => "Then click a window",
        (Grab::Screen, _) => "Capturing the screen",
    };
    let size = Vec2::new(200.0, 200.0);
    let pos = Pos2::new((screen.x - size.x) / 2.0, (screen.y - size.y) / 2.0);
    let title = &plain("capture-countdown");
    let mut cancel = false;
    ctx.show_viewport_immediate(ViewportId::from_hash_of(title), floating(title, pos, size).with_active(false), |ui, _| {
        let r = ui.max_rect();
        let p = ui.painter();
        let card = Rect::from_center_size(r.center(), Vec2::splat(180.0));
        p.rect_filled(card, 24.0, Color32::from_rgba_unmultiplied(28, 28, 31, 235));
        p.rect_stroke(card, 24.0, Stroke::new(1.0, white(30)), StrokeKind::Inside);
        let c0 = card.center() - Vec2::new(0.0, 16.0);
        p.circle_stroke(c0, 44.0, Stroke::new(4.0, white(30)));
        let frac = (left / total.max(0.1)).clamp(0.0, 1.0);
        let a0 = -std::f32::consts::FRAC_PI_2;
        p.add(egui::epaint::PathShape::line(arc_points(c0, 44.0, a0, a0 + frac * std::f32::consts::TAU, 64), egui::epaint::PathStroke::new(4.0, tint::CAPTURE)));
        p.text(c0, Align2::CENTER_CENTER, format!("{}", left.ceil().max(1.0) as u32), bold(40.0), TEXT);
        p.text(Pos2::new(card.center().x, card.bottom() - 40.0), Align2::CENTER_CENTER, what, font(12.0), TEXT_SOFT);
        p.text(Pos2::new(card.center().x, card.bottom() - 22.0), Align2::CENTER_CENTER, "Click to cancel", font(11.0), TEXT_FAINT);
        if ui.interact(card, ui.scope_id().with("cancel"), Sense::click()).clicked() {
            cancel = true;
        }
    });
    if cancel {
        c.countdown = None;
    }
}

// ---------------------------------------------------------------- permission

fn permission(c: &mut Capture, ctx: &egui::Context, screen: Vec2) {
    if c.perm_card.is_none() {
        return;
    }
    let size = Vec2::new(400.0, 176.0);
    let pos = Pos2::new((screen.x - size.x) / 2.0, screen.y * 0.3);
    let title = &plain("capture-permission");
    let mut choice = None;
    ctx.show_viewport_immediate(ViewportId::from_hash_of(title), floating(title, pos, size).with_active(true), |ui, _| {
        let r = ui.max_rect();
        let p = ui.painter().clone();
        p.rect_filled(r, 14.0, BG);
        p.rect_stroke(r, 14.0, Stroke::new(1.0, white(30)), StrokeKind::Inside);
        tool_chip(&p, Rect::from_min_size(r.min + Vec2::new(18.0, 18.0), Vec2::splat(32.0)), Icon::Crop, tint::CAPTURE);
        p.text(r.min + Vec2::new(62.0, 20.0), Align2::LEFT_TOP, "Capture needs Screen Recording", bold(15.0), TEXT);
        paragraph(&p, r.min + Vec2::new(62.0, 44.0), "Without it, captures show only the desktop. Turn on cleanmebro in System Settings, then try again.", font(13.0), TEXT_DIM, r.width() - 80.0);
        let y = r.bottom() - 46.0;
        let ow = button_width(&p, "Open System Settings", BTN_PRIMARY);
        let or = Rect::from_min_size(Pos2::new(r.right() - 18.0 - ow, y), Vec2::new(ow, 30.0));
        let nw = button_width(&p, "Not now", BTN_SECONDARY);
        let nr = Rect::from_min_size(Pos2::new(or.left() - 10.0 - nw, y), Vec2::new(nw, 30.0));
        if pill_button(ui, nr, "Not now", BTN_SECONDARY, true).clicked() || ui.input(|i| i.key_pressed(egui::Key::Escape)) {
            choice = Some(false);
        }
        if pill_button(ui, or, "Open System Settings", BTN_PRIMARY, true).clicked() {
            choice = Some(true);
        }
        if ui.ctx().input(|i| i.viewport().close_requested()) {
            choice = Some(false);
        }
    });
    match choice {
        Some(true) => {
            mac::ask_to_record_screen();
            mac::open_privacy_settings();
            c.perm_card = None;
        }
        Some(false) => {
            c.perm_card = None;
            c.perm_skip = true;
        }
        None => {}
    }
}

// ---------------------------------------------------------------- preview

#[derive(Clone, Copy, PartialEq)]
enum PreviewAct {
    Copy,
    Save,
    Edit,
    Pin,
    Text,
    Close,
}

fn preview(c: &mut Capture, ctx: &egui::Context, screen: Vec2) {
    let Some(pv) = &mut c.preview else { return };
    let w = 264.0;
    let (iw, ih) = pv.img.dimensions();
    let th = ((w - 16.0) * ih as f32 / iw.max(1) as f32).clamp(70.0, 170.0);
    let size = Vec2::new(w, 8.0 + 26.0 + th + 8.0 + 40.0 + 8.0);
    let pos = Pos2::new(20.0, screen.y - size.y - 20.0);
    let title = &plain("capture-preview");
    let vid = ViewportId::from_hash_of(("capture-preview", pv.key));
    let hide_after = c.settings.preview_secs;
    let mut act = None;
    // the native window may appear a frame late: lift it for a few frames
    let lift = pv.frames < 5;
    pv.frames += 1;
    ctx.show_viewport_immediate(vid, floating(title, pos, size).with_active(false), |ui, _| {
        if lift {
            mac::float_window(title, false);
        }
        let vctx = ui.ctx().clone();
        let r = ui.max_rect();
        let p = ui.painter().clone();
        p.rect_filled(r, 14.0, Color32::from_rgba_unmultiplied(28, 28, 31, 245));
        p.rect_stroke(r, 14.0, Stroke::new(1.0, white(30)), StrokeKind::Inside);
        let hovered = ui.rect_contains_pointer(r);
        // header
        let hy = r.top() + 8.0 + 13.0;
        icons::draw(&p, Ico::Check, Rect::from_center_size(Pos2::new(r.left() + 18.0, hy), Vec2::splat(13.0)), GREEN);
        p.text(Pos2::new(r.left() + 30.0, hy), Align2::LEFT_CENTER, &pv.note, bold(12.0), TEXT);
        if hovered {
            let cr = Rect::from_center_size(Pos2::new(r.right() - 18.0, hy), Vec2::splat(20.0));
            let b = ui.interact(cr, ui.scope_id().with("close"), Sense::click());
            p.circle_filled(cr.center(), 10.0, white(if b.hovered() { 40 } else { 22 }));
            icons::draw(&p, Ico::Close, cr.shrink(5.0), TEXT_SOFT);
            if b.on_hover_text("Close").clicked() {
                act = Some(PreviewAct::Close);
            }
        } else {
            p.text(Pos2::new(r.right() - 12.0, hy), Align2::RIGHT_CENTER, format!("{iw}×{ih}"), font(11.0), TEXT_FAINT);
        }
        // picture: drag moves the preview, click opens the editor
        let tr = Rect::from_min_size(Pos2::new(r.left() + 8.0, r.top() + 34.0), Vec2::new(w - 16.0, th));
        let tex = texture(&vctx, &mut pv.tex, "capture-preview", &pv.img);
        super::ui::thumb(&p, tr, Some(tex), (iw, ih), 8.0);
        let tresp = ui.interact(tr, ui.scope_id().with("pic"), Sense::click_and_drag());
        p.rect_stroke(tr, 8.0, Stroke::new(1.0, if tresp.hovered() { ACCENT } else { white(26) }), StrokeKind::Inside);
        if tresp.drag_started() {
            vctx.send_viewport_cmd(ViewportCommand::StartDrag);
        }
        if tresp.on_hover_text("Click to edit").clicked() {
            act = Some(PreviewAct::Edit);
        }
        // actions
        let items = [(Ico::Copy, "Copy", PreviewAct::Copy), (Ico::Save, "Save", PreviewAct::Save), (Ico::Edit, "Edit", PreviewAct::Edit), (Ico::Pin, "Pin", PreviewAct::Pin), (Ico::ScanText, "Text", PreviewAct::Text)];
        let ay = tr.bottom() + 8.0;
        let bw = (w - 16.0 - 4.0 * 4.0) / 5.0;
        for (k, (ico, label, a)) in items.iter().enumerate() {
            let br = Rect::from_min_size(Pos2::new(r.left() + 8.0 + k as f32 * (bw + 4.0), ay), Vec2::new(bw, 40.0));
            let b = ui.interact(br, ui.scope_id().with(("act", k)), Sense::click());
            p.rect_filled(br, 8.0, white(if b.hovered() { 34 } else { 16 }));
            icons::draw(&p, *ico, Rect::from_center_size(Pos2::new(br.center().x, br.top() + 14.0), Vec2::splat(15.0)), TEXT);
            p.text(Pos2::new(br.center().x, br.top() + 31.0), Align2::CENTER_CENTER, *label, font(11.0), TEXT_SOFT);
            if b.clicked() {
                act = Some(*a);
            }
        }
        if hovered {
            pv.born = Instant::now();
        }
    });
    let expired = hide_after.is_some_and(|s| pv.born.elapsed().as_secs_f32() > s as f32);
    let (id, img) = (pv.id, pv.img.clone());
    match act {
        Some(PreviewAct::Copy) => {
            if c.copy_image(&img) {
                c.hud("Copied");
            }
            c.preview = None;
        }
        Some(PreviewAct::Save) => {
            if let Some(p) = c.save_image(id, &img) {
                c.hud(format!("Saved to {}", short_path(&p)));
            }
            c.preview = None;
        }
        Some(PreviewAct::Edit) => {
            c.preview = None;
            c.open_editor(id, img);
        }
        Some(PreviewAct::Pin) => {
            c.preview = None;
            c.pin(id, img);
        }
        Some(PreviewAct::Text) => {
            c.preview = None;
            c.spawn_ocr(img);
        }
        Some(PreviewAct::Close) => c.preview = None,
        None if expired => c.preview = None,
        None => {}
    }
}

fn short_path(p: &std::path::Path) -> String {
    let home = dirs::home_dir().map(|h| h.display().to_string()).unwrap_or_default();
    let s = p.display().to_string();
    if !home.is_empty() && s.starts_with(&home) {
        s.replacen(&home, "~", 1)
    } else {
        s
    }
}

// ---------------------------------------------------------------- pins

fn pins(c: &mut Capture, ctx: &egui::Context, screen: Vec2) {
    let mut close = Vec::new();
    let mut edit = Vec::new();
    for (k, pin) in c.pins.iter_mut().enumerate() {
        let (pos, size) = *pin.start.get_or_insert_with(|| {
            // open at most 60% of the screen, centered
            pin.scale = (screen.x * 0.6 / pin.base.x).min(screen.y * 0.6 / pin.base.y).min(1.0);
            let size = pin.base * pin.scale;
            (Pos2::new((screen.x - size.x) / 2.0 + pin.offset, (screen.y - size.y) / 2.0 + pin.offset), size)
        });
        let title = plain(&format!("capture-pin-{}", pin.n));
        let lift = pin.frames < 5;
        pin.frames += 1;
        let vid = ViewportId::from_hash_of(("capture-pin", pin.n));
        let builder = floating(&title, pos, size).with_has_shadow(true);
        ctx.show_viewport_immediate(vid, builder, |ui, _| {
            let vctx = ui.ctx().clone();
            if lift {
                mac::float_window(&title, false);
            }
            let r = ui.max_rect();
            let p = ui.painter().clone();
            let tex = texture(&vctx, &mut pin.tex, &title, &pin.img);
            p.image(tex, r, Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)), Color32::WHITE);
            let resp = ui.interact(r, ui.scope_id().with("pin"), Sense::click_and_drag());
            let hovered = ui.rect_contains_pointer(r);
            p.rect_stroke(r, 0.0, Stroke::new(1.0, if hovered { with_alpha(ACCENT, 0.8) } else { white(40) }), StrokeKind::Inside);
            if resp.drag_started() {
                vctx.send_viewport_cmd(ViewportCommand::StartDrag);
            }
            if resp.double_clicked() {
                edit.push(k);
            }
            if hovered {
                let scroll = ui.input(|i| i.smooth_scroll_delta.y);
                if scroll.abs() > 0.1 {
                    pin.scale = (pin.scale * (1.0 + scroll * 0.003)).clamp(0.1, 4.0);
                    vctx.send_viewport_cmd(ViewportCommand::InnerSize(pin.base * pin.scale));
                }
                let cr = Rect::from_center_size(Pos2::new(r.right() - 16.0, r.top() + 16.0), Vec2::splat(22.0));
                let b = ui.interact(cr, ui.scope_id().with("close"), Sense::click());
                p.circle_filled(cr.center(), 11.0, Color32::from_black_alpha(if b.hovered() { 220 } else { 160 }));
                icons::draw(&p, Ico::Close, cr.shrink(6.0), Color32::WHITE);
                if b.on_hover_text("Unpin").clicked() {
                    close.push(k);
                }
                let label = format!("{:.0}%", pin.scale * 100.0);
                let g = p.layout_no_wrap(label, font(11.0), Color32::WHITE);
                let tr = Rect::from_min_size(r.left_top() + Vec2::new(8.0, 8.0), g.size() + Vec2::new(12.0, 6.0));
                p.rect_filled(tr, 6.0, Color32::from_black_alpha(160));
                p.galley(tr.min + Vec2::new(6.0, 3.0), g, Color32::WHITE);
            }
            if ui.input(|i| i.key_pressed(egui::Key::Escape)) || vctx.input(|i| i.viewport().close_requested()) {
                close.push(k);
            }
        });
    }
    for k in edit {
        if let Some(pin) = c.pins.get(k) {
            let (id, img) = (pin.id, pin.img.clone());
            c.open_editor(id, img);
        }
    }
    close.sort();
    close.dedup();
    for k in close.into_iter().rev() {
        let pin = c.pins.remove(k);
        if let Some(id) = pin.id {
            if !c.pins.iter().any(|p| p.id == Some(id)) {
                c.store.set_pinned(id, false);
            }
        }
    }
}

// ---------------------------------------------------------------- editor

fn editor_window(c: &mut Capture, ctx: &egui::Context, screen: Vec2) {
    let Some(mut ew) = c.editor.take() else { return };
    let (iw, ih) = ew.ed.image_size();
    let chrome = Vec2::new(48.0, 42.0 + 46.0 + 36.0 + 32.0 + 48.0);
    let want = Vec2::new(iw as f32, ih as f32) / c.ppp.max(1.0) + chrome;
    let size = Vec2::new(want.x.clamp(860.0, screen.x * 0.92), want.y.clamp(520.0, screen.y * 0.9));
    let pos = Pos2::new((screen.x - size.x) / 2.0, ((screen.y - size.y) / 2.0).max(30.0));
    let title = &plain("capture-editor");
    let first = !ew.opened;
    ew.opened = true;
    let builder = ViewportBuilder::default()
        .with_title(title)
        .with_inner_size(size)
        .with_position(pos)
        .with_min_inner_size([760.0, 460.0])
        .with_transparent(true)
        .with_fullsize_content_view(true)
        .with_titlebar_shown(false)
        .with_title_shown(false)
        .with_has_shadow(true)
        .with_active(true);
    let mut closed = false;
    ctx.show_viewport_immediate(ViewportId::from_hash_of("capture-editor"), builder, |ui, _| {
        let vctx = ui.ctx().clone();
        if first {
            vctx.send_viewport_cmd(ViewportCommand::Focus);
            crate::kit::native::activate();
        }
        if vctx.input(|i| i.viewport().close_requested()) {
            closed = true;
            return;
        }
        let full = ui.max_rect();
        ew.ed.ui(ui, full, false);
    });
    for r in std::mem::take(&mut ew.ed.requests) {
        match r {
            Request::Copy => {
                let img = ew.ed.rendered();
                let ok = c.copy_image(&img);
                ew.ed.flash(if ok { "Copied" } else { "Couldn't copy" });
            }
            Request::Save => {
                let id = c.commit_edit(&mut ew.ed);
                let img = ew.ed.rendered();
                if let Some(p) = c.save_image(id, &img) {
                    ew.ed.flash(format!("Saved to {}", short_path(&p)));
                }
            }
            Request::Pin => {
                let id = c.commit_edit(&mut ew.ed);
                let img = ew.ed.rendered();
                c.pin(id, img);
                ew.ed.flash("Pinned on screen");
            }
            Request::CopyText(t) => c.copy_text(&t),
            Request::Close => closed = true,
        }
    }
    if closed {
        c.commit_edit(&mut ew.ed);
        let s = &mut c.settings;
        if (s.blur, s.cover, s.keep_breaks) != (ew.ed.blur, ew.ed.cover, ew.ed.keep_breaks) {
            s.blur = ew.ed.blur;
            s.cover = ew.ed.cover;
            s.keep_breaks = ew.ed.keep_breaks;
            c.save_settings();
        }
    } else {
        c.editor = Some(ew);
    }
}

// ---------------------------------------------------------------- picker / ruler

fn overlay(c: &mut Capture, ctx: &egui::Context, screen: Vec2) {
    let Some(ov) = &mut c.overlay else { return };
    let title = &plain("capture-overlay");
    let first = ov.frames == 0;
    let lift = ov.frames < 5;
    ov.frames += 1;
    let builder = ViewportBuilder::default()
        .with_title(title)
        .with_decorations(false)
        .with_position(Pos2::ZERO)
        .with_inner_size(screen)
        .with_resizable(false)
        .with_always_on_top()
        .with_taskbar(false)
        .with_active(true);
    let mut done: Option<Option<String>> = None;
    ctx.show_viewport_immediate(ViewportId::from_hash_of(title), builder, |ui, _| {
        let vctx = ui.ctx().clone();
        if lift {
            mac::float_window(title, true);
        }
        if first {
            vctx.send_viewport_cmd(ViewportCommand::Focus);
            crate::kit::native::activate();
        }
        let r = ui.max_rect();
        let p = ui.painter().clone();
        let tex = texture(&vctx, &mut ov.tex, "capture-overlay", &ov.img);
        p.image(tex, r, Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)), Color32::WHITE);
        // image pixels per point on this screen
        let k = ov.img.width() as f32 / r.width().max(1.0);
        let to_img = |s: Pos2| Pos2::new((s.x - r.left()) * k, (s.y - r.top()) * k);
        let to_screen = |q: Pos2| Pos2::new(r.left() + q.x / k, r.top() + q.y / k);
        let resp = ui.interact(r, ui.scope_id().with("overlay"), Sense::click_and_drag());
        vctx.set_cursor_icon(egui::CursorIcon::Crosshair);
        let hint = match ov.mode {
            Measure::Color => "Pick a color: click to copy its hex code · Esc to close",
            Measure::Ruler => "Ruler: snaps to edges · drag to measure a box · click copies the size · Esc to close",
        };
        let g = p.layout_no_wrap(hint.to_string(), font(12.0), TEXT);
        let hr = Rect::from_center_size(Pos2::new(r.center().x, r.top() + 44.0), g.size() + Vec2::new(28.0, 14.0));
        p.rect_filled(hr, hr.height() / 2.0, Color32::from_rgba_unmultiplied(28, 28, 31, 235));
        p.galley(hr.center() - g.size() / 2.0, g, TEXT);
        let hover = resp.hover_pos();
        match ov.mode {
            Measure::Color => {
                if let Some(h) = hover {
                    let q = to_img(h);
                    if let Some(px) = render::pixel(&ov.img, q.x as i64, q.y as i64) {
                        editor::loupe(&p, &ov.img, q, h, px);
                        if resp.clicked() {
                            done = Some(Some(render::hex(px)));
                        }
                    }
                }
            }
            Measure::Ruler => {
                let st = Stroke::new(1.5, Color32::from_rgb(255, 214, 10));
                if resp.drag_started() {
                    if let Some(s) = resp.interact_pointer_pos() {
                        ov.drag = Some((s, s));
                    }
                }
                if let (Some((a, _)), Some(s)) = (ov.drag, resp.interact_pointer_pos()) {
                    if resp.dragged() {
                        ov.drag = Some((a, s));
                    }
                }
                if let Some((a, b)) = ov.drag {
                    let br = Rect::from_two_pos(a, b);
                    p.rect_filled(br, 0.0, Color32::from_rgba_unmultiplied(255, 214, 10, 30));
                    p.rect_stroke(br, 0.0, st, StrokeKind::Middle);
                    let label = format!("{:.0} × {:.0} pt", br.width(), br.height());
                    editor::tag(&p, br.right_bottom() + Vec2::new(8.0, 8.0), &label);
                    if resp.drag_stopped() {
                        done = Some(Some(format!("{:.0} × {:.0}", br.width(), br.height())));
                    }
                } else if let Some(h) = hover {
                    let q = to_img(h);
                    if let Some((x0, x1, y0, y1)) = render::edge_span(&ov.img, q.x as i64, q.y as i64, 10) {
                        let a = to_screen(Pos2::new(x0 as f32, q.y));
                        let b = to_screen(Pos2::new(x1 as f32 + 1.0, q.y));
                        let c2 = to_screen(Pos2::new(q.x, y0 as f32));
                        let d = to_screen(Pos2::new(q.x, y1 as f32 + 1.0));
                        p.line_segment([a, b], st);
                        p.line_segment([c2, d], st);
                        let (wp, hp) = (b.x - a.x, d.y - c2.y);
                        editor::tag(&p, h + Vec2::new(14.0, 14.0), &format!("{wp:.0} × {hp:.0} pt"));
                        if resp.clicked() {
                            done = Some(Some(format!("{wp:.0} × {hp:.0}")));
                        }
                    }
                }
            }
        }
        if ui.input(|i| i.key_pressed(egui::Key::Escape)) || vctx.input(|i| i.viewport().close_requested()) {
            done = Some(None);
        }
        vctx.request_repaint();
    });
    if let Some(copied) = done {
        c.overlay = None;
        if let Some(t) = copied {
            c.copy_text(&t);
            c.hud(format!("Copied {t}"));
        }
    }
}

// ---------------------------------------------------------------- messages

fn hud(c: &mut Capture, ctx: &egui::Context, screen: Vec2) {
    let Some((msg, at)) = &c.hud else { return };
    if at.elapsed().as_secs_f32() > 2.2 {
        c.hud = None;
        return;
    }
    let msg = msg.clone();
    let w = (msg.chars().count() as f32 * 7.4 + 56.0).clamp(160.0, 520.0);
    let size = Vec2::new(w, 40.0);
    let pos = Pos2::new((screen.x - size.x) / 2.0, 44.0);
    let title = &plain("capture-hud");
    let vid = ViewportId::from_hash_of(("capture-hud", at));
    ctx.show_viewport_immediate(vid, floating(title, pos, size).with_active(false).with_mouse_passthrough(true), |ui, _| {
        let r = ui.max_rect();
        let p = ui.painter();
        p.rect_filled(r, r.height() / 2.0, Color32::from_rgba_unmultiplied(28, 28, 31, 240));
        p.rect_stroke(r, r.height() / 2.0, Stroke::new(1.0, white(30)), StrokeKind::Inside);
        text_fit(p, r.center(), Align2::CENTER_CENTER, &msg, font(13.0), TEXT, r.width() - 24.0);
    });
}

