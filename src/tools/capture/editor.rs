//! The editor: one capture with a toolbar of drawing tools (arrow, box,
//! label, highlighter, blur, eraser, numbered steps, crop), Copy text (drag a
//! box, the text is copied), color picker and ruler, undo/redo, and Pin /
//! Copy / Save. It draws into any rect, so it serves both its own window and
//! the `--view capture-editor` screenshot.

use super::icons::{self, Ico};
use super::mac::{self, TextLine};
use super::render::{self, Doc, Frame, Mark, Ratio, Rgba8};
use super::settings::{BlurStyle, Cover};
use crate::kit::glass::*;
use egui::{Align2, Color32, Key, Pos2, Rect, Sense, Stroke, StrokeKind, Ui, Vec2};
use image::RgbaImage;
use std::sync::mpsc::{channel, Receiver, Sender};
use std::time::Instant;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tool {
    Select,
    Arrow,
    Rect,
    Ellipse,
    Pen,
    Text,
    Highlight,
    Blur,
    Eraser,
    Steps,
    Crop,
    CopyText,
    Picker,
    Ruler,
    Frame,
}

impl Tool {
    const DRAW: [Tool; 12] = [Tool::Select, Tool::Arrow, Tool::Rect, Tool::Ellipse, Tool::Pen, Tool::Text, Tool::Highlight, Tool::Blur, Tool::Eraser, Tool::Steps, Tool::Crop, Tool::CopyText];
    const MEASURE: [Tool; 2] = [Tool::Picker, Tool::Ruler];
    const LOOK: [Tool; 1] = [Tool::Frame];

    fn icon(self) -> Ico {
        match self {
            Tool::Select => Ico::Select,
            Tool::Arrow => Ico::Arrow,
            Tool::Rect => Ico::Rect,
            Tool::Ellipse => Ico::Ellipse,
            Tool::Pen => Ico::Pen,
            Tool::Text => Ico::Text,
            Tool::Highlight => Ico::Highlight,
            Tool::Blur => Ico::Blur,
            Tool::Eraser => Ico::Eraser,
            Tool::Steps => Ico::Steps,
            Tool::Crop => Ico::Crop,
            Tool::CopyText => Ico::ScanText,
            Tool::Picker => Ico::Eyedropper,
            Tool::Ruler => Ico::Ruler,
            Tool::Frame => Ico::Frame,
        }
    }

    /// Tooltip with the one-key shortcut.
    fn tip(self) -> &'static str {
        match self {
            Tool::Select => "Select and move (V)",
            Tool::Arrow => "Arrow (A)",
            Tool::Rect => "Box (R)",
            Tool::Ellipse => "Circle (L)",
            Tool::Pen => "Pen (P)",
            Tool::Text => "Label (T)",
            Tool::Highlight => "Highlighter (H)",
            Tool::Blur => "Blur (B)",
            Tool::Eraser => "Eraser: removes text (E)",
            Tool::Steps => "Numbered steps (N)",
            Tool::Crop => "Crop (C)",
            Tool::CopyText => "Copy text: drag a box (O)",
            Tool::Picker => "Pick a color (I)",
            Tool::Ruler => "Ruler (M)",
            Tool::Frame => "Frame (F)",
        }
    }

    fn key(self) -> Key {
        match self {
            Tool::Select => Key::V,
            Tool::Arrow => Key::A,
            Tool::Rect => Key::R,
            Tool::Ellipse => Key::L,
            Tool::Pen => Key::P,
            Tool::Text => Key::T,
            Tool::Highlight => Key::H,
            Tool::Blur => Key::B,
            Tool::Eraser => Key::E,
            Tool::Steps => Key::N,
            Tool::Crop => Key::C,
            Tool::CopyText => Key::O,
            Tool::Picker => Key::I,
            Tool::Ruler => Key::M,
            Tool::Frame => Key::F,
        }
    }

    /// Tools that drag out a box or a line.
    fn drags(self) -> bool {
        matches!(self, Tool::Arrow | Tool::Rect | Tool::Ellipse | Tool::Highlight | Tool::Blur | Tool::Eraser | Tool::Crop | Tool::CopyText)
    }
}

// ---------------------------------------------------------------- toolbar layout
//
// Two rows. The main bar has the tools on the left and undo/redo + Pin, Copy,
// Save on the right; the sub bar under it holds only the current tool's
// options. The main bar is laid out from both ends with known widths, so the
// sides never meet: when space runs short the right buttons drop their
// labels, then the tool buttons tighten. EDITOR_MIN_WIDTH fits the tightest form.

const BAR_PAD: f32 = 14.0;
/// Least space kept between the left and right groups.
const BAR_GAP: f32 = 16.0;
/// Undo + redo.
const UNDO_W: f32 = 30.0 + 2.0 + 30.0;
/// Between undo/redo and the Pin/Copy/Save buttons.
const UNDO_GAP: f32 = 12.0;
/// Pin, Copy and Save as icons, with the gaps between them.
const RIGHT_ICONS_W: f32 = 3.0 * 34.0 + 2.0 * 8.0;
const SUB_BAR_H: f32 = 36.0;

/// Width of the three tool plates (drawing, measuring, frame).
const fn tools_w(button: f32, gap: f32) -> f32 {
    (Tool::DRAW.len() as f32 * button + 4.0) + gap + (Tool::MEASURE.len() as f32 * button + 4.0) + gap + (Tool::LOOK.len() as f32 * button + 4.0)
}

/// The Frame tool's sub bar: name, backdrops, padding, Round, Shadow, ratio.
const FRAME_SWATCH: f32 = 26.0;
const FRAME_PAD_W: f32 = 96.0;
const FRAME_CHIP_W: f32 = 70.0;
const FRAME_RATIO_W: f32 = 176.0;
const FRAME_BAR_W: f32 = BAR_PAD + 56.0 + (render::BACKDROPS.len() + 1) as f32 * FRAME_SWATCH + 14.0 + 56.0 + FRAME_PAD_W + 16.0 + 2.0 * FRAME_CHIP_W + 8.0 + 16.0 + FRAME_RATIO_W + BAR_PAD;

const fn max_f(a: f32, b: f32) -> f32 {
    if a > b {
        a
    } else {
        b
    }
}

/// Narrowest editor window: tight tool buttons and icon-only right buttons,
/// and room for the Frame options.
pub const EDITOR_MIN_WIDTH: f32 = max_f(BAR_PAD + tools_w(28.0, 6.0) + BAR_GAP + UNDO_W + UNDO_GAP + RIGHT_ICONS_W + BAR_PAD, FRAME_BAR_W);

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BarLayout {
    /// Pin, Copy and Save without their labels.
    pub icons_only: bool,
    /// Tool button width and the gap between the two tool plates.
    pub button: f32,
    pub group_gap: f32,
}

impl BarLayout {
    /// Pick the roomiest form that fits `width`; `labeled_w` is Pin + Copy +
    /// Save with labels and gaps, as measured with the current font.
    pub fn fit(width: f32, labeled_w: f32) -> BarLayout {
        let right = |buttons: f32| UNDO_W + UNDO_GAP + buttons + BAR_PAD;
        let roomy = BAR_PAD + tools_w(32.0, 10.0) + BAR_GAP;
        if roomy + right(labeled_w) <= width {
            BarLayout { icons_only: false, button: 32.0, group_gap: 10.0 }
        } else if roomy + right(RIGHT_ICONS_W) <= width {
            BarLayout { icons_only: true, button: 32.0, group_gap: 10.0 }
        } else {
            BarLayout { icons_only: true, button: 28.0, group_gap: 6.0 }
        }
    }

    /// Where the tools end and where the right group starts, for `width`.
    #[cfg(test)]
    pub fn spans(&self, width: f32, labeled_w: f32) -> (f32, f32) {
        let buttons = if self.icons_only { RIGHT_ICONS_W } else { labeled_w };
        (BAR_PAD + tools_w(self.button, self.group_gap), width - BAR_PAD - buttons - UNDO_GAP - UNDO_W)
    }
}

pub const COLORS: [(&str, Rgba8); 6] = [
    ("Red", [255, 69, 58, 255]),
    ("Amber", [255, 179, 64, 255]),
    ("Green", [48, 209, 88, 255]),
    ("Blue", [77, 163, 255, 255]),
    ("White", [255, 255, 255, 255]),
    ("Black", [28, 28, 31, 255]),
];
/// Stroke widths (pt) for thin / medium / thick.
const STROKES: [f32; 3] = [2.0, 4.0, 7.0];
/// Label text size (pt) per stroke size.
const TEXT_PT: [f32; 3] = [13.0, 17.0, 24.0];
const STEP_PT: [f32; 3] = [20.0, 26.0, 34.0];

/// What the editor asks the tool to do (it owns the clipboard, files and pins).
#[derive(Clone, Debug, PartialEq)]
pub enum Request {
    Copy,
    Save,
    Pin,
    Close,
    CopyText(String),
}

enum Ocr {
    /// Copy text from this box (image px).
    Copy(Rect),
    /// Blur or erase only the text in this box.
    Mark(Mark),
}

pub struct Editor {
    /// History id of the capture, if it came from history.
    pub shot: Option<u64>,
    pub title: String,
    base: RgbaImage,
    pub doc: Doc,
    undo: Vec<Doc>,
    redo: Vec<Doc>,
    pub tool: Tool,
    color: usize,
    stroke: usize,
    pub blur: BlurStyle,
    pub cover: Cover,
    pub keep_breaks: bool,
    /// Image pixels per point (2 on Retina captures).
    px_per_pt: f32,
    drag: Option<(Pos2, Pos2)>,
    moving: Option<(usize, Pos2, Mark)>,
    /// A handle being dragged: mark, handle, the mark before.
    resizing: Option<(usize, usize, Mark)>,
    /// The freehand line being drawn.
    pen: Option<Vec<Pos2>>,
    /// The frame Frame turns on with; the last one picked.
    pub frame_style: Frame,
    selected: Option<usize>,
    text: Option<(Pos2, String)>,
    tex: Option<egui::TextureHandle>,
    tex_for: Option<Vec<Mark>>,
    effects: Option<RgbaImage>,
    ocr_tx: Sender<(Ocr, Vec<TextLine>)>,
    ocr_rx: Receiver<(Ocr, Vec<TextLine>)>,
    ocr_busy: bool,
    /// A crop area picked with the Crop tool, applied on Enter.
    crop_pending: Option<Rect>,
    status: Option<(String, Instant)>,
    pub requests: Vec<Request>,
    /// The doc changed since it was opened (closing keeps the edit in history).
    pub edited: bool,
}

impl Editor {
    #[allow(clippy::too_many_arguments)]
    pub fn new(shot: Option<u64>, title: String, base: RgbaImage, px_per_pt: f32, blur: BlurStyle, cover: Cover, keep_breaks: bool, frame_style: Frame) -> Editor {
        let (ocr_tx, ocr_rx) = channel();
        Editor {
            shot,
            title,
            base,
            doc: Doc { ppp: px_per_pt.max(1.0), ..Doc::default() },
            undo: Vec::new(),
            redo: Vec::new(),
            tool: Tool::Crop,
            color: 0,
            stroke: 1,
            blur,
            cover,
            keep_breaks,
            px_per_pt: px_per_pt.max(1.0),
            drag: None,
            moving: None,
            resizing: None,
            pen: None,
            frame_style,
            selected: None,
            text: None,
            tex: None,
            tex_for: None,
            effects: None,
            ocr_tx,
            ocr_rx,
            ocr_busy: false,
            crop_pending: None,
            status: None,
            requests: Vec::new(),
            edited: false,
        }
    }

    pub fn image_size(&self) -> (u32, u32) {
        self.base.dimensions()
    }

    /// The finished picture with every mark burned in and the crop applied.
    pub fn rendered(&self) -> RgbaImage {
        render::render(&self.base, &self.doc)
    }

    pub fn flash(&mut self, msg: impl Into<String>) {
        self.status = Some((msg.into(), Instant::now()));
    }

    /// A sample set of marks, for the screenshot view.
    pub fn demo_marks(&mut self) {
        let s = self.base.width() as f32 / 794.0;
        let p = |x: f32, y: f32| Pos2::new(x * s, y * s);
        let red = COLORS[0].1;
        self.doc.marks = vec![
            Mark::Highlight { r: Rect::from_min_max(p(206.0, 94.0), p(524.0, 112.0)), color: COLORS[1].1 },
            Mark::Blur { r: Rect::from_min_max(p(216.0, 172.0), p(386.0, 190.0)), style: BlurStyle::Pixelate, boxes: vec![] },
            Mark::Rect { r: Rect::from_min_max(p(204.0, 212.0), p(536.0, 272.0)), color: red, width: 4.0 * s },
            Mark::Step { at: p(186.0, 168.0), n: 1, color: red, size: 26.0 * s },
            Mark::Step { at: p(186.0, 242.0), n: 2, color: red, size: 26.0 * s },
            Mark::Arrow { a: p(620.0, 380.0), b: p(342.0, 322.0), color: red, width: 4.0 * s },
            Mark::Text { at: p(560.0, 386.0), text: "then press Save".into(), color: red, size: 17.0 * s },
        ];
        self.tool = Tool::Blur;
    }

    fn push_undo(&mut self) {
        self.undo.push(self.doc.clone());
        if self.undo.len() > 100 {
            self.undo.remove(0);
        }
        self.redo.clear();
        self.edited = true;
    }

    fn add(&mut self, m: Mark) {
        self.push_undo();
        self.doc.marks.push(m);
    }

    fn undo(&mut self) {
        if let Some(prev) = self.undo.pop() {
            self.redo.push(std::mem::replace(&mut self.doc, prev));
            self.selected = None;
            self.drop_gestures();
        }
    }

    fn redo(&mut self) {
        if let Some(next) = self.redo.pop() {
            self.undo.push(std::mem::replace(&mut self.doc, next));
            self.selected = None;
            self.drop_gestures();
        }
    }

    /// A drag in progress points into the old document: drop it.
    fn drop_gestures(&mut self) {
        self.resizing = None;
        self.pen = None;
        self.drag = None;
    }

    fn color(&self) -> Rgba8 {
        COLORS[self.color].1
    }

    fn width_px(&self) -> f32 {
        STROKES[self.stroke] * self.px_per_pt
    }

    /// The part of the image on screen: the crop, unless cropping right now.
    fn view(&self) -> Rect {
        let full = Rect::from_min_size(Pos2::ZERO, Vec2::new(self.base.width() as f32, self.base.height() as f32));
        match self.doc.crop {
            Some(c) if self.tool != Tool::Crop => c.intersect(full),
            _ => full,
        }
    }

    // ------------------------------------------------------------ frame

    /// Draw the editor into `full`. `lights`: paint traffic lights (screenshots).
    pub fn ui(&mut self, ui: &mut Ui, full: Rect, lights: bool) {
        self.poll_ocr(ui.ctx());
        let p = ui.painter().clone();
        p.rect_filled(full, 12.0, BG());
        // title bar
        let title = Rect::from_min_size(full.min, Vec2::new(full.width(), 42.0));
        p.rect_filled(title, egui::CornerRadius { nw: 12, ne: 12, sw: 0, se: 0 }, FOOTER());
        hairline(&p, title.left_bottom(), title.right_bottom());
        if lights {
            for (k, c) in [Color32::from_rgb(255, 95, 87), Color32::from_rgb(254, 188, 46), Color32::from_rgb(40, 200, 64)].iter().enumerate() {
                p.circle_filled(title.left_center() + Vec2::new(20.0 + 20.0 * k as f32, 0.0), 6.0, *c);
            }
        }
        let (w, h) = self.base.dimensions();
        let head = p.text(title.center() - Vec2::new(30.0, 0.0), Align2::CENTER_CENTER, &self.title, bold(13.0), TEXT());
        p.text(Pos2::new(head.right() + 6.0, title.center().y), Align2::LEFT_CENTER, format!("· {w}×{h}"), font(13.0), TEXT_FAINT());
        let drag = ui.interact(Rect::from_min_max(title.min + Vec2::new(90.0, 0.0), title.max), ui.scope_id().with("drag"), Sense::click_and_drag());
        if drag.drag_started() {
            ui.ctx().send_viewport_cmd(egui::ViewportCommand::StartDrag);
        }

        let bar = Rect::from_min_size(title.left_bottom(), Vec2::new(full.width(), 46.0));
        self.toolbar(ui, bar);
        hairline(&p, bar.left_bottom(), bar.right_bottom());
        // the sub bar is always there, so the canvas never jumps between tools
        let strip = Rect::from_min_size(bar.left_bottom(), Vec2::new(full.width(), SUB_BAR_H));
        self.options(ui, strip);
        let top = strip.bottom();
        hairline(&p, Pos2::new(full.left(), top), Pos2::new(full.right(), top));
        let foot = Rect::from_min_max(Pos2::new(full.left(), full.bottom() - 32.0), full.max);
        hairline(&p, foot.left_top(), foot.right_top());
        let marks = self.doc.marks.len();
        p.text(foot.left_center() + Vec2::new(14.0, 0.0), Align2::LEFT_CENTER, format!("{w}×{h} px · {marks} mark{}", if marks == 1 { "" } else { "s" }), font(12.0), TEXT_FAINT());
        p.text(foot.right_center() - Vec2::new(14.0, 0.0), Align2::RIGHT_CENTER, "⌘C copy · ⌘S save · ⌘Z undo · Esc close", font(12.0), TEXT_FAINT());

        let canvas = Rect::from_min_max(Pos2::new(full.left(), top), Pos2::new(full.right(), foot.top()));
        p.rect_filled(canvas, 0.0, pick(Color32::from_rgb(0x0B, 0x12, 0x0A), Color32::from_rgb(0xD5, 0xDE, 0xCC)));
        self.canvas(ui, canvas);
        self.keys(ui);
        if self.ocr_busy {
            // stays up while Vision works; the first run loads its model
            self.status = Some((mac::ocr_status().into(), Instant::now()));
        }
        if let Some((msg, at)) = &self.status {
            if at.elapsed().as_secs_f32() < 2.5 {
                let g = p.layout_no_wrap(msg.clone(), font(13.0), TEXT());
                let r = Rect::from_center_size(Pos2::new(canvas.center().x, canvas.bottom() - 30.0), g.size() + Vec2::new(28.0, 16.0));
                p.rect_filled(r, r.height() / 2.0, POPOVER());
                p.rect_stroke(r, r.height() / 2.0, Stroke::new(1.0, white(30)), StrokeKind::Inside);
                p.galley(r.center() - g.size() / 2.0, g, TEXT());
                ui.ctx().request_repaint();
            } else {
                self.status = None;
            }
        }
    }

    fn toolbar(&mut self, ui: &mut Ui, bar: Rect) {
        let p = ui.painter().clone();
        let y = bar.center().y;
        const RIGHT: [(&str, u8, Ico, &str); 3] = [
            ("Save  ⌘S", BTN_PRIMARY, Ico::Save, "Save to your captures folder (⌘S)"),
            ("Copy  ⌘C", BTN_SECONDARY, Ico::Copy, "Copy the picture with your marks (⌘C)"),
            ("Pin", BTN_SECONDARY, Ico::Pin, "Pin: keep it floating on top"),
        ];
        let labeled_w = |p: &egui::Painter, (label, kind, ic, _): (&str, u8, Ico, &str)| button_width(p, label, kind) - 6.0 + if ic == Ico::Pin { 18.0 } else { 0.0 };
        let labeled_total: f32 = RIGHT.iter().map(|b| labeled_w(&p, *b)).sum::<f32>() + 2.0 * 8.0;
        let lay = BarLayout::fit(bar.width(), labeled_total);

        // left: the tools
        let mut x = bar.left() + BAR_PAD;
        let bw = lay.button;
        let group = |ui: &mut Ui, x: &mut f32, tools: &[Tool], cur: &mut Tool| {
            let plate = Rect::from_min_size(Pos2::new(*x, y - 16.0), Vec2::new(tools.len() as f32 * bw + 4.0, 32.0));
            p.rect_filled(plate, R_BUTTON, white(13));
            for (k, t) in tools.iter().enumerate() {
                let r = Rect::from_min_size(Pos2::new(plate.left() + 2.0 + bw * k as f32, plate.top() + 2.0), Vec2::new(bw, 28.0));
                if icon_toggle(ui, r, t.icon(), t.tip(), *cur == *t).clicked() {
                    *cur = *t;
                }
            }
            *x = plate.right() + lay.group_gap;
        };
        let mut tool = self.tool;
        group(ui, &mut x, &Tool::DRAW, &mut tool);
        group(ui, &mut x, &Tool::MEASURE, &mut tool);
        group(ui, &mut x, &Tool::LOOK, &mut tool);
        if tool != self.tool {
            self.set_tool(tool);
        }

        // right: Save, Copy, Pin (from the right edge), then undo/redo
        let mut rx = bar.right() - BAR_PAD;
        for (k, b) in RIGHT.iter().enumerate() {
            let (label, kind, ic, tip) = *b;
            let w = if lay.icons_only { 34.0 } else { labeled_w(&p, *b) };
            let r = Rect::from_min_max(Pos2::new(rx - w, y - 14.0), Pos2::new(rx, y + 14.0));
            rx = r.left() - 8.0;
            let resp = if lay.icons_only {
                let resp = pill_button(ui, r, "", kind, true);
                icons::draw(ui.painter(), ic, Rect::from_center_size(r.center(), Vec2::splat(14.0)), if kind == BTN_PRIMARY { on_color(BLUE()) } else { TEXT() });
                resp
            } else if ic == Ico::Pin {
                let resp = pill_button(ui, r, "", kind, true);
                icons::draw(ui.painter(), ic, Rect::from_center_size(Pos2::new(r.left() + 16.0, y), Vec2::splat(14.0)), if kind == BTN_PRIMARY { on_color(BLUE()) } else { TEXT() });
                ui.painter().text(Pos2::new(r.left() + 28.0, y), Align2::LEFT_CENTER, label, font(12.0), if kind == BTN_PRIMARY { on_color(BLUE()) } else { TEXT() });
                resp
            } else {
                pill_button(ui, r, label, kind, true)
            };
            if resp.on_hover_text(tip).clicked() {
                self.requests.push([Request::Save, Request::Copy, Request::Pin][k].clone());
            }
        }
        let rr = Rect::from_min_size(Pos2::new(rx + 8.0 - UNDO_GAP - 30.0, y - 14.0), Vec2::new(30.0, 28.0));
        let ur = rr.translate(Vec2::new(-32.0, 0.0));
        if plain_icon(ui, ur, Ico::Undo, "Undo (⌘Z)", !self.undo.is_empty()).clicked() {
            self.undo();
        }
        if plain_icon(ui, rr, Ico::Redo, "Redo (⇧⌘Z)", !self.redo.is_empty()).clicked() {
            self.redo();
        }
    }

    /// The sub bar: only the current tool's options, or a short hint.
    fn options(&mut self, ui: &mut Ui, strip: Rect) {
        let p = ui.painter().clone();
        p.rect_filled(strip, 0.0, FOOTER());
        let y = strip.center().y;
        let mut x = strip.left() + BAR_PAD;
        let name = match self.tool {
            Tool::Select => "Select",
            Tool::Arrow => "Arrow",
            Tool::Rect => "Box",
            Tool::Ellipse => "Circle",
            Tool::Pen => "Pen",
            Tool::Text => "Label",
            Tool::Highlight => "Highlighter",
            Tool::Blur => "Blur",
            Tool::Eraser => "Eraser",
            Tool::Steps => "Steps",
            Tool::Crop => "Crop",
            Tool::CopyText => "Copy text",
            Tool::Picker => "Pick a color",
            Tool::Ruler => "Ruler",
            Tool::Frame => "Frame",
        };
        x = p.text(Pos2::new(x, y), Align2::LEFT_CENTER, name, bold(12.0), TEXT()).right() + 16.0;
        let hint: String = match self.tool {
            Tool::Frame => {
                self.frame_options(ui, x, y);
                String::new()
            }
            Tool::Arrow | Tool::Rect | Tool::Ellipse | Tool::Pen | Tool::Highlight | Tool::Steps | Tool::Text => {
                for (k, (cname, c)) in COLORS.iter().enumerate() {
                    let r = Rect::from_center_size(Pos2::new(x + 9.0, y), Vec2::splat(18.0));
                    let resp = ui.interact(r.expand(3.0), ui.scope_id().with(("color", k)), Sense::click()).on_hover_text(*cname);
                    if self.color == k {
                        crate::kit::glass::stroke_circle(&p, r.center(), 11.5, Stroke::new(2.0, BLUE()));
                    }
                    p.circle_filled(r.center(), 9.0, Color32::from_rgba_unmultiplied(c[0], c[1], c[2], c[3]));
                    crate::kit::glass::stroke_circle(&p, r.center(), 9.0, Stroke::new(1.0, if self.color == k { Color32::WHITE } else { white(64) }));
                    if resp.clicked() {
                        self.color = k;
                        self.recolor_selected();
                    }
                    x += 26.0;
                }
                p.line_segment([Pos2::new(x + 4.0, y - 9.0), Pos2::new(x + 4.0, y + 9.0)], Stroke::new(1.0, white(26)));
                x += 14.0;
                let sized = matches!(self.tool, Tool::Text | Tool::Steps);
                x = p.text(Pos2::new(x, y), Align2::LEFT_CENTER, if sized { "Size" } else { "Width" }, font(12.0), TEXT_DIM()).right() + 8.0;
                let plate = Rect::from_min_size(Pos2::new(x, y - 13.0), Vec2::new(3.0 * 30.0 + 4.0, 26.0));
                p.rect_filled(plate, R_BUTTON, white(13));
                for k in 0..3 {
                    let r = Rect::from_min_size(Pos2::new(plate.left() + 2.0 + 30.0 * k as f32, plate.top() + 2.0), Vec2::new(30.0, 22.0));
                    let tip = if sized { ["Small", "Medium", "Large"][k] } else { ["Thin", "Medium", "Thick"][k] };
                    let resp = ui.interact(r, ui.scope_id().with(("stroke", k)), Sense::click()).on_hover_text(tip);
                    if self.stroke == k {
                        p.rect_filled(r, 6.0, pick(Color32::from_white_alpha(40), Color32::WHITE));
                    }
                    let t = [2.0, 4.0, 6.0][k];
                    p.rect_filled(Rect::from_center_size(r.center(), Vec2::new(14.0, t)), t / 2.0, if self.stroke == k { TEXT() } else { TEXT_SOFT() });
                    if resp.clicked() {
                        self.stroke = k;
                    }
                }
                x = plate.right() + 16.0;
                String::new()
            }
            Tool::Blur | Tool::Eraser => {
                if self.tool == Tool::Blur {
                    x = p.text(Pos2::new(x, y), Align2::LEFT_CENTER, "Style", font(12.0), TEXT_DIM()).right() + 8.0;
                    let r = Rect::from_min_size(Pos2::new(x, y - 13.0), Vec2::new(200.0, 26.0));
                    let cur = match self.blur {
                        BlurStyle::Pixelate => 0,
                        BlurStyle::Blur => 1,
                        BlurStyle::Solid => 2,
                    };
                    if let Some(i) = segmented(ui, r, ui.scope_id().with("style"), &["Pixelate", "Blur", "Solid"], cur) {
                        self.blur = [BlurStyle::Pixelate, BlurStyle::Blur, BlurStyle::Solid][i];
                    }
                    x = r.right() + 16.0;
                }
                x = p.text(Pos2::new(x, y), Align2::LEFT_CENTER, "Cover", font(12.0), TEXT_DIM()).right() + 8.0;
                let r = Rect::from_min_size(Pos2::new(x, y - 13.0), Vec2::new(170.0, 26.0));
                if let Some(i) = segmented(ui, r, ui.scope_id().with("cover"), &["Whole box", "Only text"], (self.cover == Cover::Text) as usize) {
                    self.cover = if i == 1 { Cover::Text } else { Cover::Box };
                }
                x = r.right() + 16.0;
                if self.tool == Tool::Eraser { "Fills with the color around the text.".into() } else { String::new() }
            }
            Tool::Crop => match self.crop_pending {
                Some(c) => format!("{:.0} × {:.0} · Enter to crop · Esc to cancel", c.width().round(), c.height().round()),
                None => "Drag an area · Enter to crop".into(),
            },
            Tool::CopyText => "Drag over text, it's copied".into(),
            Tool::Picker => "Click to copy a color's hex code".into(),
            Tool::Ruler => "Hover to measure, drag for a box, click to copy".into(),
            Tool::Select => "Click a mark to select, drag to move, ⌫ deletes".into(),
        };
        if !hint.is_empty() {
            text_fit(&p, Pos2::new(x, y), Align2::LEFT_CENTER, &hint, font(12.0), TEXT_FAINT(), (strip.right() - BAR_PAD - x).max(40.0));
        }
    }

    /// Backdrops (the first swatch turns the frame off), padding, Round,
    /// Shadow and ratio. Any pick turns the frame on.
    fn frame_options(&mut self, ui: &mut Ui, mut x: f32, y: f32) {
        let p = ui.painter().clone();
        let cur = self.doc.frame;
        let mut next = cur;
        let mut style = cur.unwrap_or(self.frame_style);
        for k in 0..=render::BACKDROPS.len() {
            let r = Rect::from_center_size(Pos2::new(x + FRAME_SWATCH / 2.0 - 4.0, y), Vec2::splat(18.0));
            let name = if k == 0 { "None" } else { render::BACKDROPS[k - 1].0 };
            let resp = ui.interact(r.expand(3.0), ui.scope_id().with(("bg", k)), Sense::click()).on_hover_text(name);
            let on = match cur {
                None => k == 0,
                Some(f) => f.bg + 1 == k,
            };
            if on {
                crate::kit::glass::stroke_circle(&p, r.center(), 11.5, Stroke::new(2.0, BLUE()));
            }
            if k == 0 {
                crate::kit::glass::stroke_circle(&p, r.center(), 8.5, Stroke::new(1.5, TEXT_SOFT()));
                let d = Vec2::splat(8.5 * std::f32::consts::FRAC_1_SQRT_2);
                p.line_segment([r.center() - d, r.center() + d], Stroke::new(1.5, TEXT_SOFT()));
            } else {
                let (_, c0, c1) = render::BACKDROPS[k - 1];
                gradient_disc(&p, r.center(), 9.0, c32(c0), c32(c1));
                crate::kit::glass::stroke_circle(&p, r.center(), 9.0, Stroke::new(1.0, white(64)));
            }
            if resp.clicked() {
                if k == 0 {
                    next = None;
                } else {
                    style.bg = k - 1;
                    next = Some(style);
                }
            }
            x += FRAME_SWATCH;
        }
        p.line_segment([Pos2::new(x + 2.0, y - 9.0), Pos2::new(x + 2.0, y + 9.0)], Stroke::new(1.0, white(26)));
        x += 14.0;
        x = p.text(Pos2::new(x, y), Align2::LEFT_CENTER, "Padding", font(12.0), TEXT_DIM()).right() + 8.0;
        let r = Rect::from_min_size(Pos2::new(x, y - 13.0), Vec2::new(FRAME_PAD_W, 26.0));
        if let Some(i) = segmented(ui, r, ui.scope_id().with("pad"), &["S", "M", "L"], style.pad) {
            style.pad = i;
            next = Some(style);
        }
        x = r.right() + 16.0;
        for (k, (label, on)) in [("Round", style.round), ("Shadow", style.shadow)].into_iter().enumerate() {
            let r = Rect::from_min_size(Pos2::new(x, y - 13.0), Vec2::new(FRAME_CHIP_W, 26.0));
            if chip(ui, r, label, on && cur.is_some()).clicked() {
                if k == 0 {
                    style.round = !on || cur.is_none();
                } else {
                    style.shadow = !on || cur.is_none();
                }
                next = Some(style);
            }
            x = r.right() + 8.0;
        }
        x += 8.0;
        let r = Rect::from_min_size(Pos2::new(x, y - 13.0), Vec2::new(FRAME_RATIO_W, 26.0));
        let labels: Vec<&str> = Ratio::ALL.iter().map(|r| r.label()).collect();
        let at = Ratio::ALL.iter().position(|r| *r == style.ratio).unwrap_or(0);
        if let Some(i) = segmented(ui, r, ui.scope_id().with("ratio"), &labels, at) {
            style.ratio = Ratio::ALL[i];
            next = Some(style);
        }
        if next != cur {
            self.set_frame(next);
        }
    }

    fn set_frame(&mut self, f: Option<Frame>) {
        if f == self.doc.frame {
            return;
        }
        self.push_undo();
        self.doc.frame = f;
        if let Some(f) = f {
            self.frame_style = f;
        }
    }

    fn set_tool(&mut self, t: Tool) {
        self.commit_text();
        self.tool = t;
        self.selected = None;
        self.drag = None;
        self.pen = None;
        self.crop_pending = None;
        // picking Frame shows it at once, with the last style
        if t == Tool::Frame && self.doc.frame.is_none() {
            self.set_frame(Some(self.frame_style));
        }
    }

    /// Crop to the picked area (Enter).
    fn apply_crop(&mut self) {
        let Some(r) = self.crop_pending.take() else { return };
        self.push_undo();
        self.doc.crop = Some(r);
        self.tool = Tool::Select;
        self.flash("Cropped. Undo to bring it back.");
    }

    fn recolor_selected(&mut self) {
        let Some(i) = self.selected else { return };
        let c = self.color();
        let mut m = self.doc.marks[i].clone();
        match &mut m {
            Mark::Arrow { color, .. } | Mark::Rect { color, .. } | Mark::Ellipse { color, .. } | Mark::Pen { color, .. } | Mark::Text { color, .. } | Mark::Highlight { color, .. } | Mark::Step { color, .. } => *color = c,
            _ => return,
        }
        self.push_undo();
        self.doc.marks[i] = m;
    }

    // ------------------------------------------------------------ canvas

    fn canvas(&mut self, ui: &mut Ui, area: Rect) {
        let ctx = ui.ctx().clone();
        // the image with its pixel effects, re-rendered when they change
        let effects: Vec<Mark> = self.doc.marks.iter().filter(|m| m.is_effect()).cloned().collect();
        if self.tex.is_none() || self.tex_for.as_ref() != Some(&effects) {
            let img = render::apply_effects(&self.base, &effects);
            let ci = egui::ColorImage::from_rgba_unmultiplied([img.width() as usize, img.height() as usize], img.as_raw());
            match &mut self.tex {
                Some(t) => t.set(ci, egui::TextureOptions::LINEAR),
                None => self.tex = Some(ctx.load_texture("capture-editor", ci, egui::TextureOptions::LINEAR)),
            }
            self.effects = Some(img);
            self.tex_for = Some(effects);
        }
        let view = self.view();
        let pad = 24.0;
        let avail = area.shrink(pad);
        // the frame shows around the picture, except while cropping
        let frame = self.doc.frame.filter(|_| self.tool != Tool::Crop);
        let geom = frame.map(|f| render::frame_geom(view.width(), view.height(), &f, self.px_per_pt));
        let outer = geom.map(|g| g.size).unwrap_or(view.size());
        // fit, but never larger than true size (px_per_pt image pixels per point)
        let scale = (avail.width() / outer.x).min(avail.height() / outer.y).min(1.0 / self.px_per_pt).max(0.01);
        let outer_r = Rect::from_center_size(avail.center(), outer * scale);
        let shown = match geom {
            Some(g) => Rect::from_min_size(outer_r.min + g.inner.min.to_vec2() * scale, view.size() * scale),
            None => outer_r,
        };
        let to_screen = |q: Pos2| shown.min + (q - view.min) * scale;
        let to_img = |s: Pos2| view.min + (s - shown.min) / scale;
        let p = ui.painter_at(area);
        let radius = geom.map(|g| g.radius * scale).unwrap_or(0.0);
        match (frame, geom) {
            (Some(f), Some(g)) => {
                let (_, c0, c1) = render::BACKDROPS[f.bg.min(render::BACKDROPS.len() - 1)];
                gradient_rect(&p, outer_r, c32(c0), c32(c1));
                if let Some((blur, dy)) = g.shadow {
                    let sh = egui::epaint::Shadow { offset: [0, (dy * scale).round().min(127.0) as i8], blur: (blur * scale).round().min(255.0) as u8, spread: 0, color: Color32::from_black_alpha(110) };
                    p.add(sh.as_shape(shown, radius));
                }
            }
            _ => {
                p.rect_filled(shown.expand(1.0), 4.0, Color32::from_black_alpha(90));
            }
        }
        if let Some(t) = &self.tex {
            let (w, h) = (self.base.width() as f32, self.base.height() as f32);
            let uv = Rect::from_min_max(Pos2::new(view.min.x / w, view.min.y / h), Pos2::new(view.max.x / w, view.max.y / h));
            p.add(egui::epaint::RectShape::filled(shown, radius, Color32::WHITE).with_texture(t.id(), uv));
        }
        // shapes; with a frame they stop at the picture's edge, as in the export
        let mp = if geom.is_some() { p.with_clip_rect(shown.intersect(area)) } else { p.clone() };
        for (i, m) in self.doc.marks.iter().enumerate() {
            draw_mark(&mp, m, &to_screen, scale);
            if self.selected == Some(i) {
                let b = m.bounds();
                p.rect_stroke(Rect::from_min_max(to_screen(b.min), to_screen(b.max)).expand(3.0), 3.0, Stroke::new(1.0, with_alpha(ACCENT(), 0.6)), StrokeKind::Outside);
                for h in m.handles() {
                    let hr = Rect::from_center_size(to_screen(h), Vec2::splat(8.0));
                    p.rect_filled(hr, 2.0, Color32::WHITE);
                    p.rect_stroke(hr, 2.0, Stroke::new(1.5, ACCENT()), StrokeKind::Middle);
                }
            }
        }
        if let Some(pts) = &self.pen {
            let m = Mark::Pen { pts: pts.clone(), color: self.color(), width: self.width_px() };
            draw_mark(&mp, &m, &to_screen, scale);
        }
        if self.tool == Tool::Crop && self.drag.is_none() {
            if let Some(c) = self.crop_pending {
                let cr = Rect::from_min_max(to_screen(c.min), to_screen(c.max));
                shade_outside(&p, shown, cr);
                let corners = [cr.left_top(), cr.right_top(), cr.right_bottom(), cr.left_bottom(), cr.left_top()];
                p.extend(egui::Shape::dashed_line(&corners, Stroke::new(1.5, Color32::WHITE), 6.0, 4.0));
                tag(&p, cr.right_bottom() + Vec2::new(6.0, 6.0), &format!("{:.0} × {:.0}", c.width().round(), c.height().round()));
                let hint = "Enter to crop · Esc to cancel";
                let g = p.layout_no_wrap(hint.to_string(), font(12.0), TEXT());
                let mut at = Pos2::new(cr.center().x - g.size().x / 2.0 - 12.0, cr.top() - g.size().y - 22.0);
                if at.y < area.top() + 4.0 {
                    at.y = cr.bottom() + 10.0;
                }
                at.x = at.x.clamp(area.left() + 4.0, area.right() - g.size().x - 28.0);
                let hr = Rect::from_min_size(at, g.size() + Vec2::new(24.0, 12.0));
                p.rect_filled(hr, hr.height() / 2.0, POPOVER());
                p.rect_stroke(hr, hr.height() / 2.0, Stroke::new(1.0, white(30)), StrokeKind::Inside);
                p.galley(hr.center() - g.size() / 2.0, g, TEXT());
            } else if let Some(c) = self.doc.crop {
                let cr = Rect::from_min_max(to_screen(c.min), to_screen(c.max));
                shade_outside(&p, shown, cr);
                p.rect_stroke(cr, 0.0, Stroke::new(1.5, Color32::WHITE), StrokeKind::Middle);
            }
        }

        let resp = ui.interact(area, ui.scope_id().with("canvas"), Sense::click_and_drag());
        let hover = resp.hover_pos().filter(|h| shown.contains(*h));
        let at_img = |s: Pos2| {
            let q = to_img(s);
            Pos2::new(q.x.clamp(view.min.x, view.max.x), q.y.clamp(view.min.y, view.max.y))
        };

        // dragging out shapes
        if self.tool.drags() {
            if resp.drag_started() {
                // only on the picture, not on the frame around it
                if let Some(s) = resp.interact_pointer_pos().filter(|s| shown.contains(*s)) {
                    let q = at_img(s);
                    self.drag = Some((q, q));
                }
            }
            if let (Some((a, _)), Some(s)) = (self.drag, resp.interact_pointer_pos()) {
                if resp.dragged() || resp.drag_stopped() {
                    self.drag = Some((a, at_img(s)));
                }
            }
            if let Some((a, b)) = self.drag {
                self.draw_drag_preview(&p, a, b, &to_screen, scale, shown);
            }
            if resp.drag_stopped() {
                if let Some((a, b)) = self.drag.take() {
                    self.finish_drag(a, b, ui.ctx());
                }
            }
            if hover.is_some() {
                ctx.set_cursor_icon(egui::CursorIcon::Crosshair);
            }
        }
        match self.tool {
            Tool::Select => self.select_tool(&resp, &at_img, ui.ctx(), scale),
            Tool::Pen => {
                if resp.drag_started() {
                    if let Some(s) = resp.interact_pointer_pos().filter(|s| shown.contains(*s)) {
                        self.pen = Some(vec![at_img(s)]);
                    }
                }
                if let (Some(pts), Some(s)) = (self.pen.as_mut(), resp.interact_pointer_pos()) {
                    let q = at_img(s);
                    // a point every 2 screen points keeps the line smooth and light
                    let far = pts.last().is_none_or(|l| (q - *l).length() * scale >= 2.0);
                    // the release point too, so the stroke ends where the hand did
                    if (resp.dragged() && far) || (resp.drag_stopped() && pts.last() != Some(&q)) {
                        pts.push(q);
                    }
                }
                if resp.drag_stopped() {
                    if let Some(pts) = self.pen.take() {
                        self.add(Mark::Pen { pts, color: self.color(), width: self.width_px() });
                    }
                }
                if hover.is_some() {
                    ctx.set_cursor_icon(egui::CursorIcon::Crosshair);
                }
            }
            Tool::Text => {
                if resp.clicked() {
                    if let Some(s) = resp.interact_pointer_pos() {
                        self.commit_text();
                        self.text = Some((at_img(s), String::new()));
                    }
                }
                if hover.is_some() {
                    ctx.set_cursor_icon(egui::CursorIcon::Text);
                }
            }
            Tool::Steps => {
                if resp.clicked() {
                    if let Some(s) = resp.interact_pointer_pos() {
                        let n = self.doc.next_step();
                        let m = Mark::Step { at: at_img(s), n, color: self.color(), size: STEP_PT[self.stroke] * self.px_per_pt };
                        self.add(m);
                    }
                }
            }
            Tool::Picker => {
                if let Some(h) = hover {
                    let q = to_img(h);
                    if let Some(c) = self.effects.as_ref().and_then(|img| render::pixel(img, q.x as i64, q.y as i64)) {
                        loupe(&p, self.effects.as_ref().unwrap(), q, h, c);
                        ctx.set_cursor_icon(egui::CursorIcon::Crosshair);
                        if resp.clicked() {
                            let hex = render::hex(c);
                            self.requests.push(Request::CopyText(hex.clone()));
                            self.flash(format!("Copied {hex}"));
                        }
                    }
                }
            }
            Tool::Ruler => {
                if let (Some(h), Some(img)) = (hover, self.effects.as_ref()) {
                    let q = to_img(h);
                    if let Some((x0, x1, y0, y1)) = render::edge_span(img, q.x as i64, q.y as i64, 10) {
                        let (wp, hp) = ((x1 - x0 + 1) as f32 / self.px_per_pt, (y1 - y0 + 1) as f32 / self.px_per_pt);
                        let yl = Pos2::new(0.0, q.y);
                        let a = to_screen(Pos2::new(x0 as f32, yl.y));
                        let b = to_screen(Pos2::new(x1 as f32 + 1.0, yl.y));
                        let c = to_screen(Pos2::new(q.x, y0 as f32));
                        let d = to_screen(Pos2::new(q.x, y1 as f32 + 1.0));
                        let st = Stroke::new(1.5, Color32::from_rgb(255, 214, 10));
                        p.line_segment([a, b], st);
                        p.line_segment([c, d], st);
                        let label = format!("{:.0} × {:.0} pt", wp, hp);
                        tag(&p, h + Vec2::new(14.0, 14.0), &label);
                        if resp.clicked() {
                            self.requests.push(Request::CopyText(format!("{:.0} × {:.0}", wp, hp)));
                            self.flash(format!("Copied {label}"));
                        }
                    }
                    ctx.set_cursor_icon(egui::CursorIcon::Crosshair);
                }
            }
            _ => {}
        }
        // the label being typed
        if let Some((at, mut s)) = self.text.take() {
            let pos = to_screen(at);
            let size = TEXT_PT[self.stroke] * self.px_per_pt * scale;
            let c = self.color();
            let col = Color32::from_rgba_unmultiplied(c[0], c[1], c[2], 255);
            let ink = render::ink_on(c);
            let mut done = false;
            let mut child = ui.new_child(egui::UiBuilder::new().max_rect(Rect::from_min_size(pos, Vec2::new(area.right() - pos.x, size * 2.0))));
            let tw = (s.chars().count() as f32 * size * 0.62).max(size * 6.0);
            let te = egui::TextEdit::singleline(&mut s)
                .font(egui::FontId::proportional(size.max(8.0)))
                .text_color(Color32::from_rgb(ink[0], ink[1], ink[2]))
                .background_color(col)
                .hint_text("Type, then Enter")
                .desired_width(tw);
            let r = child.add(te);
            if !r.has_focus() && !r.lost_focus() {
                r.request_focus();
            }
            if r.lost_focus() {
                done = true;
            }
            if done {
                self.text = Some((at, s));
                self.commit_text();
            } else {
                self.text = Some((at, s));
            }
        }
        if self.ocr_busy {
            spinner(&p, Pos2::new(shown.right() - 16.0, shown.top() + 16.0), 7.0, ctx.input(|i| i.time), Color32::WHITE);
            crate::kit::glass::animate(&ctx);
        }
    }

    fn draw_drag_preview(&self, p: &egui::Painter, a: Pos2, b: Pos2, to_screen: &dyn Fn(Pos2) -> Pos2, scale: f32, shown: Rect) {
        let r = Rect::from_two_pos(a, b);
        let sr = Rect::from_min_max(to_screen(r.min), to_screen(r.max));
        match self.tool {
            Tool::Crop => {
                shade_outside(p, shown, sr);
                p.rect_stroke(sr, 0.0, Stroke::new(1.5, Color32::WHITE), StrokeKind::Middle);
                let (w, h) = (r.width().round(), r.height().round());
                tag(p, sr.right_bottom() + Vec2::new(6.0, 6.0), &format!("{w:.0} × {h:.0}"));
            }
            Tool::Blur | Tool::Eraser | Tool::CopyText => {
                p.rect_filled(sr, 3.0, with_alpha(BLUE(), 0.16));
                p.extend(egui::Shape::dashed_line(&[sr.left_top(), sr.right_top(), sr.right_bottom(), sr.left_bottom(), sr.left_top()], Stroke::new(1.5, ACCENT()), 5.0, 3.0));
            }
            _ => {
                if let Some(m) = self.mark_from_drag(a, b) {
                    draw_mark(p, &m, to_screen, scale);
                }
            }
        }
    }

    fn mark_from_drag(&self, a: Pos2, b: Pos2) -> Option<Mark> {
        let r = Rect::from_two_pos(a, b);
        let c = self.color();
        Some(match self.tool {
            Tool::Arrow => Mark::Arrow { a, b, color: c, width: self.width_px() },
            Tool::Rect => Mark::Rect { r, color: c, width: self.width_px() },
            Tool::Ellipse => Mark::Ellipse { r, color: c, width: self.width_px() },
            Tool::Highlight => Mark::Highlight { r, color: c },
            _ => return None,
        })
    }

    fn finish_drag(&mut self, a: Pos2, b: Pos2, ctx: &egui::Context) {
        let r = Rect::from_two_pos(a, b);
        let tiny = r.width() < 3.0 && r.height() < 3.0;
        if tiny {
            return;
        }
        match self.tool {
            Tool::Arrow | Tool::Rect | Tool::Ellipse | Tool::Highlight => {
                if let Some(m) = self.mark_from_drag(a, b) {
                    self.add(m);
                }
            }
            // picked, not applied: Enter crops, Esc or another drag replaces it
            Tool::Crop => self.crop_pending = Some(r),
            Tool::Blur | Tool::Eraser => {
                let m = if self.tool == Tool::Blur { Mark::Blur { r, style: self.blur, boxes: vec![] } } else { Mark::Erase { r, boxes: vec![] } };
                if self.cover == Cover::Text {
                    self.run_ocr(Ocr::Mark(m), r, ctx);
                } else {
                    self.add(m);
                }
            }
            Tool::CopyText => self.run_ocr(Ocr::Copy(r), r, ctx),
            _ => {}
        }
    }

    fn run_ocr(&mut self, job: Ocr, r: Rect, ctx: &egui::Context) {
        let Some((x, y, w, h)) = render::clamp_rect(r, self.base.width(), self.base.height()) else { return };
        let src = self.effects.clone().unwrap_or_else(|| self.base.clone());
        let crop = image::imageops::crop_imm(&src, x, y, w, h).to_image();
        let tx = self.ocr_tx.clone();
        let ctx = ctx.clone();
        self.ocr_busy = true;
        std::thread::spawn(move || {
            // tests check the editor, not Vision, which can take many seconds to load
            let mut lines = if cfg!(test) { Vec::new() } else { mac::recognize_image(&crop) };
            for l in lines.iter_mut() {
                l.rect = l.rect.translate(Vec2::new(x as f32, y as f32));
            }
            let _ = tx.send((job, lines));
            ctx.request_repaint();
        });
    }

    fn poll_ocr(&mut self, _ctx: &egui::Context) {
        while let Ok((job, lines)) = self.ocr_rx.try_recv() {
            self.ocr_busy = false;
            match job {
                Ocr::Copy(r) => {
                    if lines.is_empty() {
                        self.flash("No text found in that box");
                        continue;
                    }
                    let _ = r;
                    let text = mac::join_lines(&lines, self.keep_breaks);
                    self.requests.push(Request::CopyText(text));
                    let n = lines.len();
                    self.flash(format!("Copied {n} line{}", if n == 1 { "" } else { "s" }));
                }
                Ocr::Mark(mut m) => {
                    let boxes: Vec<Rect> = lines.iter().map(|l| l.rect).collect();
                    if boxes.is_empty() {
                        self.flash("No text found, so the whole box is covered");
                    }
                    match &mut m {
                        Mark::Blur { boxes: b, .. } | Mark::Erase { boxes: b, .. } => *b = boxes,
                        _ => {}
                    }
                    self.add(m);
                }
            }
        }
    }

    /// The selected mark's handle under `q` (image px), if any.
    fn handle_at(&self, q: Pos2, scale: f32) -> Option<(usize, usize)> {
        let i = self.selected?;
        let k = self.doc.marks.get(i)?.handles().iter().position(|h| (*h - q).length() * scale <= 7.0)?;
        Some((i, k))
    }

    fn select_tool(&mut self, resp: &egui::Response, at_img: &dyn Fn(Pos2) -> Pos2, ctx: &egui::Context, scale: f32) {
        let slack = 6.0 / scale;
        // handles first: they sit on or outside the mark's edge
        if let Some(h) = resp.hover_pos() {
            if let Some((i, k)) = self.resizing.as_ref().map(|r| (r.0, r.1)).or_else(|| self.handle_at(at_img(h), scale)) {
                let corner = self.doc.marks.get(i).is_some_and(|m| matches!(m, Mark::Arrow { .. }));
                ctx.set_cursor_icon(match (corner, k) {
                    (true, _) => egui::CursorIcon::Crosshair,
                    (_, 0 | 4) => egui::CursorIcon::ResizeNwSe,
                    (_, 2 | 6) => egui::CursorIcon::ResizeNeSw,
                    (_, 1 | 5) => egui::CursorIcon::ResizeVertical,
                    _ => egui::CursorIcon::ResizeHorizontal,
                });
            }
        }
        if resp.drag_started() {
            if let Some((i, k)) = resp.interact_pointer_pos().and_then(|s| self.handle_at(at_img(s), scale)) {
                self.resizing = Some((i, k, self.doc.marks[i].clone()));
            }
        }
        if let Some((i, k, orig)) = self.resizing.clone() {
            if i >= self.doc.marks.len() {
                self.resizing = None;
                return;
            }
            if let (true, Some(s)) = (resp.dragged() || resp.drag_stopped(), resp.interact_pointer_pos()) {
                let mut m = orig.clone();
                m.drag_handle(k, at_img(s));
                self.doc.marks[i] = m;
            }
            if resp.drag_stopped() {
                self.resizing = None;
                if self.doc.marks[i] != orig {
                    let resized = std::mem::replace(&mut self.doc.marks[i], orig);
                    self.push_undo();
                    self.doc.marks[i] = resized;
                }
            }
            return;
        }
        if resp.drag_started() || resp.clicked() {
            if let Some(s) = resp.interact_pointer_pos() {
                let q = at_img(s);
                self.selected = self.doc.marks.iter().rposition(|m| m.hit(q, slack));
                if resp.drag_started() {
                    if let Some(i) = self.selected {
                        self.moving = Some((i, q, self.doc.marks[i].clone()));
                    }
                }
            }
        }
        if let (Some((i, start, orig)), Some(s)) = (self.moving.clone(), resp.interact_pointer_pos()) {
            if resp.dragged() {
                let mut m = orig;
                m.translate(at_img(s) - start);
                self.doc.marks[i] = m;
            }
        }
        if resp.drag_stopped() {
            if let Some((i, _, orig)) = self.moving.take() {
                if self.doc.marks[i] != orig {
                    let moved = self.doc.marks[i].clone();
                    self.doc.marks[i] = orig;
                    self.push_undo();
                    self.doc.marks[i] = moved;
                }
            }
        }
    }

    fn commit_text(&mut self) {
        if let Some((at, s)) = self.text.take() {
            let s = s.trim().to_string();
            if !s.is_empty() {
                let m = Mark::Text { at, text: s, color: self.color(), size: TEXT_PT[self.stroke] * self.px_per_pt };
                self.add(m);
            }
        }
    }

    fn keys(&mut self, ui: &mut Ui) {
        if self.text.is_some() || ui.ctx().egui_wants_keyboard_input() {
            if ui.input(|i| i.key_pressed(Key::Escape)) {
                self.text = None;
            }
            return;
        }
        let (cmd, shift) = ui.input(|i| (i.modifiers.command, i.modifiers.shift));
        let pressed = |k: Key| ui.input(|i| i.key_pressed(k));
        if cmd && pressed(Key::Z) {
            if shift {
                self.redo();
            } else {
                self.undo();
            }
        } else if cmd && pressed(Key::C) || ui.input(|i| i.events.iter().any(|e| matches!(e, egui::Event::Copy))) {
            self.requests.push(Request::Copy);
        } else if cmd && pressed(Key::S) {
            self.requests.push(Request::Save);
        } else if pressed(Key::Enter) && self.crop_pending.is_some() {
            self.apply_crop();
        } else if pressed(Key::Escape) {
            if self.crop_pending.is_some() {
                self.crop_pending = None;
            } else if self.drag.is_some() || self.selected.is_some() {
                self.drag = None;
                self.selected = None;
            } else {
                self.requests.push(Request::Close);
            }
        } else if (pressed(Key::Delete) || pressed(Key::Backspace)) && self.selected.is_some() {
            let i = self.selected.take().unwrap();
            self.push_undo();
            self.doc.marks.remove(i);
        } else if !cmd {
            for t in Tool::DRAW.iter().chain(Tool::MEASURE.iter()).chain(Tool::LOOK.iter()) {
                if pressed(t.key()) {
                    self.set_tool(*t);
                    break;
                }
            }
        }
    }
}

// ---------------------------------------------------------------- drawing helpers

fn c32(c: Rgba8) -> Color32 {
    Color32::from_rgba_unmultiplied(c[0], c[1], c[2], c[3])
}

/// The editor's live view of a shape (the export uses `render`).
fn draw_mark(p: &egui::Painter, m: &Mark, to_screen: &dyn Fn(Pos2) -> Pos2, scale: f32) {
    match m {
        Mark::Arrow { a, b, color, width } => {
            let w = width * scale;
            let end = render::shaft_end(*a, *b, *width);
            p.line_segment([to_screen(*a), to_screen(end)], Stroke::new(w, c32(*color)));
            p.circle_filled(to_screen(*a), w / 2.0, c32(*color));
            let head: Vec<Pos2> = render::arrow_head(*a, *b, *width).iter().map(|q| to_screen(*q)).collect();
            p.add(egui::Shape::convex_polygon(head, c32(*color), Stroke::new(w * 0.6, c32(*color))));
        }
        Mark::Rect { r, color, width } => {
            let sr = Rect::from_min_max(to_screen(r.min), to_screen(r.max));
            p.rect_stroke(sr, width * 1.5 * scale, Stroke::new(width * scale, c32(*color)), StrokeKind::Middle);
        }
        Mark::Ellipse { r, color, width } => {
            let sr = Rect::from_min_max(to_screen(r.min), to_screen(r.max));
            p.add(egui::Shape::ellipse_stroke(sr.center(), sr.size() / 2.0, Stroke::new(width * scale, c32(*color))));
        }
        Mark::Pen { pts, color, width } => {
            let w = width * scale;
            let sp: Vec<Pos2> = pts.iter().map(|q| to_screen(*q)).collect();
            if let (Some(a), Some(b)) = (sp.first(), sp.last()) {
                // round ends, as in the export
                p.circle_filled(*a, w / 2.0, c32(*color));
                p.circle_filled(*b, w / 2.0, c32(*color));
            }
            if sp.len() > 1 {
                p.add(egui::Shape::line(sp, Stroke::new(w, c32(*color))));
            }
        }
        Mark::Highlight { r, color } => {
            let sr = Rect::from_min_max(to_screen(r.min), to_screen(r.max));
            p.rect_filled(sr, 2.0 * scale, Color32::from_rgba_unmultiplied(color[0], color[1], color[2], 110));
        }
        Mark::Text { at, text, color, size } => {
            let s = render::label_size(text, *size);
            let r = Rect::from_min_size(to_screen(*at), s * scale);
            p.rect_filled(r, size * 0.3 * scale, c32(*color));
            let ink = render::ink_on(*color);
            p.text(r.center(), Align2::CENTER_CENTER, text, egui::FontId::proportional((size * scale).max(4.0)), c32(ink));
        }
        Mark::Step { at, n, color, size } => {
            let c = to_screen(*at);
            p.circle_filled(c, size * scale / 2.0, c32(*color));
            p.text(c, Align2::CENTER_CENTER, n.to_string(), bold((size * scale * 0.55).max(4.0)), c32(render::ink_on(*color)));
        }
        Mark::Blur { .. } | Mark::Erase { .. } => {}
    }
}

/// A rect filled with a top-left → bottom-right gradient (as the export draws it).
fn gradient_rect(p: &egui::Painter, r: Rect, c0: Color32, c1: Color32) {
    let d = r.max - r.min;
    let t = |q: Pos2| ((q - r.min).dot(d) / d.length_sq().max(1.0)).clamp(0.0, 1.0);
    let mut mesh = egui::Mesh::default();
    for q in [r.left_top(), r.right_top(), r.right_bottom(), r.left_bottom()] {
        mesh.colored_vertex(q, lerp_color(c0, c1, t(q)));
    }
    mesh.add_triangle(0, 1, 2);
    mesh.add_triangle(0, 2, 3);
    p.add(egui::Shape::mesh(mesh));
}

/// A round swatch of a backdrop's gradient.
fn gradient_disc(p: &egui::Painter, c: Pos2, radius: f32, c0: Color32, c1: Color32) {
    let mut mesh = egui::Mesh::default();
    let t = |v: Vec2| ((v.x + v.y) / (2.0 * radius) * std::f32::consts::FRAC_1_SQRT_2 + 0.5).clamp(0.0, 1.0);
    mesh.colored_vertex(c, lerp_color(c0, c1, 0.5));
    let n = 32;
    for k in 0..=n {
        let a = k as f32 / n as f32 * std::f32::consts::TAU;
        let v = Vec2::new(a.cos(), a.sin()) * radius;
        mesh.colored_vertex(c + v, lerp_color(c0, c1, t(v)));
        if k > 0 {
            mesh.add_triangle(0, k, k + 1);
        }
    }
    p.add(egui::Shape::mesh(mesh));
}

/// A small on/off chip with its label inside.
fn chip(ui: &mut Ui, r: Rect, label: &str, on: bool) -> egui::Response {
    let resp = ui.interact(r, ui.scope_id().with(("chip", label)), Sense::click());
    let p = ui.painter();
    let fill = if on { BLUE() } else if resp.hovered() { white(26) } else { white(15) };
    p.rect_filled(r, R_SMALL, fill);
    p.text(r.center(), Align2::CENTER_CENTER, label, font(12.0), if on { ON_ACTION } else { TEXT_DIM() });
    resp
}

fn shade_outside(p: &egui::Painter, shown: Rect, keep: Rect) {
    let keep = keep.intersect(shown);
    let shade = Color32::from_black_alpha(140);
    p.rect_filled(Rect::from_min_max(shown.min, Pos2::new(shown.right(), keep.top())), 0.0, shade);
    p.rect_filled(Rect::from_min_max(Pos2::new(shown.left(), keep.bottom()), shown.max), 0.0, shade);
    p.rect_filled(Rect::from_min_max(Pos2::new(shown.left(), keep.top()), Pos2::new(keep.left(), keep.bottom())), 0.0, shade);
    p.rect_filled(Rect::from_min_max(Pos2::new(keep.right(), keep.top()), Pos2::new(shown.right(), keep.bottom())), 0.0, shade);
}

/// A small yellow measurement tag.
pub fn tag(p: &egui::Painter, pos: Pos2, label: &str) {
    let g = p.layout_no_wrap(label.to_owned(), bold(11.0), BG());
    let r = Rect::from_min_size(pos, g.size() + Vec2::new(12.0, 6.0));
    p.rect_filled(r, 5.0, Color32::from_rgb(255, 214, 10));
    p.galley(r.min + Vec2::new(6.0, 3.0), g, BG());
}

/// A magnified 9×9 pixel grid around `q` (image px) drawn near `at`, with the hex.
pub fn loupe(p: &egui::Painter, img: &RgbaImage, q: Pos2, at: Pos2, c: Rgba8) {
    let cell = 10.0;
    let n = 9i64;
    let size = cell * n as f32;
    let origin = at + Vec2::new(18.0, 18.0);
    let r = Rect::from_min_size(origin, Vec2::splat(size));
    p.rect_filled(r.expand(3.0), 8.0, Color32::WHITE);
    for dy in 0..n {
        for dx in 0..n {
            let px = render::pixel(img, q.x as i64 + dx - n / 2, q.y as i64 + dy - n / 2).unwrap_or([0, 0, 0, 255]);
            let cr = Rect::from_min_size(origin + Vec2::new(dx as f32 * cell, dy as f32 * cell), Vec2::splat(cell));
            p.rect_filled(cr, 0.0, c32([px[0], px[1], px[2], 255]));
        }
    }
    let mid = Rect::from_min_size(origin + Vec2::splat((n / 2) as f32 * cell), Vec2::splat(cell));
    p.rect_stroke(mid, 0.0, Stroke::new(2.0, Color32::WHITE), StrokeKind::Outside);
    p.rect_stroke(mid, 0.0, Stroke::new(1.0, Color32::BLACK), StrokeKind::Inside);
    let label = render::hex(c);
    let g = p.layout_no_wrap(label, mono(12.0), TEXT());
    let lr = Rect::from_min_size(Pos2::new(r.left(), r.bottom() + 8.0), Vec2::new(g.size().x + 30.0, 24.0));
    p.rect_filled(lr, 7.0, POPOVER());
    p.rect_filled(Rect::from_center_size(Pos2::new(lr.left() + 12.0, lr.center().y), Vec2::splat(12.0)), 3.0, c32([c[0], c[1], c[2], 255]));
    p.galley(Pos2::new(lr.left() + 22.0, lr.center().y - g.size().y / 2.0), g, TEXT());
}

/// A tool button that stays lit while its tool is on.
fn icon_toggle(ui: &mut Ui, r: Rect, ic: Ico, tip: &str, on: bool) -> egui::Response {
    let resp = ui.interact(r, ui.scope_id().with(("tool", tip)), Sense::click());
    let p = ui.painter();
    if on {
        p.rect_filled(r, 6.0, BLUE());
    } else if resp.hovered() {
        p.rect_filled(r, 6.0, white(20));
    }
    icons::draw(p, ic, Rect::from_center_size(r.center(), Vec2::splat(16.0)), if on { ON_ACTION } else { TEXT_SOFT() });
    resp.on_hover_text(tip)
}

fn plain_icon(ui: &mut Ui, r: Rect, ic: Ico, tip: &str, enabled: bool) -> egui::Response {
    let resp = ui.interact(r, ui.scope_id().with(("plain", tip)), if enabled { Sense::click() } else { Sense::hover() });
    if enabled && resp.hovered() {
        ui.painter().rect_filled(r, 6.0, white(20));
    }
    icons::draw(ui.painter(), ic, Rect::from_center_size(r.center(), Vec2::splat(16.0)), if enabled { TEXT_SOFT() } else { TEXT_MUTED() });
    resp.on_hover_text(tip)
}

#[cfg(test)]
mod tests {
    #[test]
    fn toolbar_sides_never_meet() {
        // generous guess at the labeled Pin/Copy/Save width
        let labeled = 240.0;
        for w in [EDITOR_MIN_WIDTH, 640.0, 700.0, 760.0, 800.0, 1000.0, 1600.0] {
            if w < EDITOR_MIN_WIDTH {
                continue;
            }
            let lay = BarLayout::fit(w, labeled);
            let (left_end, right_start) = lay.spans(w, labeled);
            assert!(left_end + BAR_GAP <= right_start + 0.01, "overlap at {w}: {lay:?} {left_end} {right_start}");
        }
        assert!(BarLayout::fit(EDITOR_MIN_WIDTH, labeled).icons_only);
        assert!(!BarLayout::fit(1200.0, labeled).icons_only);
    }

    use super::*;

    fn ed() -> Editor {
        Editor::new(None, "t".into(), RgbaImage::from_pixel(200, 100, image::Rgba([255, 255, 255, 255])), 2.0, BlurStyle::Pixelate, Cover::Box, true, Frame::default())
    }

    #[test]
    fn undo_and_redo_walk_the_history() {
        let mut e = ed();
        e.add(Mark::Highlight { r: Rect::from_min_max(Pos2::ZERO, Pos2::new(10.0, 10.0)), color: [1, 2, 3, 255] });
        e.add(Mark::Step { at: Pos2::new(5.0, 5.0), n: 1, color: [0; 4], size: 10.0 });
        assert_eq!(e.doc.marks.len(), 2);
        e.undo();
        assert_eq!(e.doc.marks.len(), 1);
        e.redo();
        assert_eq!(e.doc.marks.len(), 2);
        e.undo();
        e.add(Mark::Step { at: Pos2::new(1.0, 1.0), n: 1, color: [0; 4], size: 10.0 });
        e.redo();
        assert_eq!(e.doc.marks.len(), 2, "a new mark clears redo");
        assert!(e.edited);
    }

    #[test]
    fn drags_make_the_right_marks() {
        let mut e = ed();
        let ctx = egui::Context::default();
        e.tool = Tool::Arrow;
        e.finish_drag(Pos2::new(10.0, 10.0), Pos2::new(100.0, 50.0), &ctx);
        assert!(matches!(e.doc.marks[0], Mark::Arrow { width, .. } if width == STROKES[1] * 2.0));
        e.tool = Tool::Blur;
        e.finish_drag(Pos2::new(10.0, 10.0), Pos2::new(1.0, 1.0), &ctx);
        assert!(matches!(&e.doc.marks[1], Mark::Blur { boxes, .. } if boxes.is_empty()));
        e.tool = Tool::Crop;
        e.finish_drag(Pos2::new(150.0, 80.0), Pos2::new(50.0, 20.0), &ctx);
        assert_eq!(e.doc.crop, None, "a crop drag only picks the area");
        e.apply_crop();
        assert_eq!(e.doc.crop, Some(Rect::from_min_max(Pos2::new(50.0, 20.0), Pos2::new(150.0, 80.0))));
        assert_eq!(e.rendered().dimensions(), (100, 60));
        // a click without a drag adds nothing
        let n = e.doc.marks.len();
        e.tool = Tool::Rect;
        e.finish_drag(Pos2::new(5.0, 5.0), Pos2::new(6.0, 6.0), &ctx);
        assert_eq!(e.doc.marks.len(), n);
    }

    #[test]
    fn frame_tool_turns_the_frame_on_and_undo_takes_it_off() {
        let mut e = ed();
        let plain = e.rendered().dimensions();
        e.set_tool(Tool::Frame);
        assert_eq!(e.doc.frame, Some(Frame::default()));
        let framed = e.rendered().dimensions();
        assert!(framed.0 > plain.0 && framed.1 > plain.1);
        e.set_frame(Some(Frame { bg: 3, ..Frame::default() }));
        assert_eq!(e.frame_style.bg, 3, "the last pick is remembered");
        e.undo();
        e.undo();
        assert_eq!(e.doc.frame, None);
        assert_eq!(e.rendered().dimensions(), plain);
    }

    #[test]
    fn ellipse_drag_makes_an_ellipse() {
        let mut e = ed();
        let ctx = egui::Context::default();
        e.tool = Tool::Ellipse;
        e.finish_drag(Pos2::new(10.0, 10.0), Pos2::new(100.0, 50.0), &ctx);
        assert!(matches!(e.doc.marks[0], Mark::Ellipse { .. }));
    }

    #[test]
    fn only_text_waits_for_recognition_then_covers_the_box_if_none() {
        let mut e = ed();
        let ctx = egui::Context::default();
        e.tool = Tool::Eraser;
        e.cover = Cover::Text;
        e.finish_drag(Pos2::new(10.0, 10.0), Pos2::new(90.0, 40.0), &ctx);
        assert!(e.doc.marks.is_empty(), "added once text is found");
        let t0 = Instant::now();
        while e.doc.marks.is_empty() && t0.elapsed().as_secs() < 10 {
            e.poll_ocr(&ctx);
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        assert!(!e.doc.marks.is_empty(), "recognition never answered");
        assert!(matches!(&e.doc.marks[0], Mark::Erase { boxes, .. } if boxes.is_empty()), "no OCR here: whole box");
    }

    #[test]
    fn undo_in_the_middle_of_a_resize_drops_it() {
        let mut e = ed();
        e.add(Mark::Highlight { r: Rect::from_min_max(Pos2::ZERO, Pos2::new(10.0, 10.0)), color: [1, 2, 3, 255] });
        e.resizing = Some((0, 4, e.doc.marks[0].clone()));
        e.undo();
        assert!(e.resizing.is_none() && e.doc.marks.is_empty());
    }
}
