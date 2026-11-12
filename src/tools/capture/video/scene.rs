//! What goes on top of a recording when it's rendered: the cursor (recorded
//! on its own, drawn smooth), the camera (its own track), a frame around the
//! picture and burned-in captions. Pure: the model, the timing maths and a
//! CPU compositor over BGRA frames, so it's all tested on any platform. The
//! macOS side ([`super::compose`]) only reads and writes the frames.
//!
//! Frames are BGRA, premultiplied (opaque screens are the same either way).
//! tiny-skia thinks in RGBA, so every color we paint goes in with red and
//! blue swapped ([`bgra`]); pictures from the video stay as they are.

use super::speech::Line;
use crate::tools::capture::render::{self, Frame, Rgba8};
use serde::{Deserialize, Serialize};
use tiny_skia::{FillRule, FilterQuality, Paint, PathBuilder, Pattern, Pixmap, PixmapMut, PixmapPaint, SpreadMode, Transform};

// ---------------------------------------------------------------- recorded data

/// The cursor's shape at a moment.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum Shape {
    #[default]
    Arrow,
    Text,
    Hand,
}

/// The cursor as it was recorded. Times are video seconds; positions are a
/// share of the recorded picture (0..1, outside when the cursor left it).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Track {
    pub moves: Vec<(f64, f32, f32)>,
    /// When a button went down.
    pub clicks: Vec<f64>,
    /// When the shape changed, and to what.
    pub shapes: Vec<(f64, Shape)>,
}

/// Where the camera bubble was while recording: time, center x, center y
/// and diameter, all as a share of the picture's width (y of its height).
pub type CameraSpot = (f64, f32, f32, f32);

/// What a recording keeps beside its raw video for rendering.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Recorded {
    pub cursor: Option<Track>,
    /// The camera is the raw file's second video track.
    pub camera: bool,
    pub camera_spots: Vec<CameraSpot>,
    pub fps: u32,
}

// ---------------------------------------------------------------- style

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct CursorStyle {
    pub show: bool,
    /// 1, 1.5 or 2 times the system size.
    pub size: f32,
    pub smooth: bool,
    /// A ring where you click.
    pub clicks: bool,
    /// Fade out when it hasn't moved for a while.
    pub hide_idle: bool,
}

impl Default for CursorStyle {
    fn default() -> CursorStyle {
        CursorStyle { show: true, size: 1.5, smooth: true, clicks: true, hide_idle: false }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Place {
    /// Where the bubble was during the recording.
    Recorded,
    TopLeft,
    TopRight,
    BottomLeft,
    BottomRight,
}

impl Place {
    pub const ALL: [Place; 5] = [Place::Recorded, Place::TopLeft, Place::TopRight, Place::BottomLeft, Place::BottomRight];
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct CameraStyle {
    pub show: bool,
    pub place: Place,
    /// Diameter as a share of the video's shorter side; None keeps the
    /// recorded size.
    pub size: Option<f32>,
    /// A rounded square instead of a circle.
    pub square: bool,
    pub mirror: bool,
}

impl Default for CameraStyle {
    fn default() -> CameraStyle {
        CameraStyle { show: true, place: Place::Recorded, size: None, square: false, mirror: true }
    }
}

/// Camera sizes the viewer offers (share of the shorter side).
pub const CAMERA_SIZES: [f32; 3] = [0.2, 0.28, 0.38];

/// How a recording is rendered. Kept with the recording; the last one
/// used is the default for the next.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct Style {
    pub frame: Option<Frame>,
    pub cursor: CursorStyle,
    pub camera: CameraStyle,
    /// Burn the transcript in as captions.
    pub captions: bool,
}

// ---------------------------------------------------------------- cursor motion

/// Samples per second of the smoothed path.
const SMOOTH_HZ: f64 = 120.0;
/// The spring's speed (rad/s): settles in about 4/ω seconds.
const SPRING_W: f64 = 28.0;
/// A jump this far in one sample is a teleport (a new screen): no glide.
const TELEPORT: f32 = 0.35;

/// The raw position at `t`, between samples.
fn raw_at(moves: &[(f64, f32, f32)], t: f64) -> Option<(f32, f32)> {
    let first = moves.first()?;
    if t <= first.0 {
        return Some((first.1, first.2));
    }
    let i = moves.partition_point(|m| m.0 <= t);
    if i >= moves.len() {
        let l = moves.last()?;
        return Some((l.1, l.2));
    }
    let (a, b) = (moves[i - 1], moves[i]);
    if (b.1 - a.1).abs().max((b.2 - a.2).abs()) > TELEPORT {
        // a jump to another screen: no slide across
        return Some((a.1, a.2));
    }
    let k = if b.0 > a.0 { ((t - a.0) / (b.0 - a.0)) as f32 } else { 1.0 };
    Some((a.1 + (b.1 - a.1) * k, a.2 + (b.2 - a.2) * k))
}

/// The path the cursor is drawn along. Smooth: a critically damped spring
/// chasing the recorded position (no overshoot), sampled at 120 Hz.
#[derive(Clone, Debug, Default)]
pub struct Path {
    t0: f64,
    pts: Vec<(f32, f32)>,
    raw: Vec<(f64, f32, f32)>,
    smooth: bool,
    /// When it last moved, per smoothed sample (for hiding when idle).
    still_since: Vec<f64>,
}

impl Path {
    pub fn new(track: &Track, smooth: bool) -> Path {
        let raw = track.moves.clone();
        let (Some(first), Some(last)) = (raw.first(), raw.last()) else { return Path::default() };
        let (t0, t1) = (first.0, last.0);
        let n = ((t1 - t0) * SMOOTH_HZ).ceil().max(0.0) as usize + 1;
        let dt = 1.0 / SMOOTH_HZ;
        let mut pts = Vec::with_capacity(n);
        let mut still_since = Vec::with_capacity(n);
        let (mut x, mut y) = (first.1 as f64, first.2 as f64);
        let (mut vx, mut vy) = (0.0f64, 0.0f64);
        let mut last_target = (first.1, first.2);
        let mut still = t0;
        for k in 0..n {
            let t = t0 + k as f64 * dt;
            let target = raw_at(&raw, t).unwrap_or(last_target);
            let jump = (target.0 - last_target.0).abs().max((target.1 - last_target.1).abs());
            if jump > 0.0005 {
                still = t;
            }
            if !smooth || jump > TELEPORT {
                (x, y, vx, vy) = (target.0 as f64, target.1 as f64, 0.0, 0.0);
            } else {
                // x'' = w²(target - x) - 2w x', a few small steps per sample
                for _ in 0..4 {
                    let h = dt / 4.0;
                    let ax = SPRING_W * SPRING_W * (target.0 as f64 - x) - 2.0 * SPRING_W * vx;
                    let ay = SPRING_W * SPRING_W * (target.1 as f64 - y) - 2.0 * SPRING_W * vy;
                    vx += ax * h;
                    vy += ay * h;
                    x += vx * h;
                    y += vy * h;
                }
            }
            last_target = target;
            pts.push((x as f32, y as f32));
            still_since.push(still);
        }
        Path { t0, pts, raw, smooth, still_since }
    }

    /// Where to draw the cursor at video time `t`.
    pub fn at(&self, t: f64) -> Option<(f32, f32)> {
        if !self.smooth {
            return raw_at(&self.raw, t);
        }
        let n = self.pts.len();
        if n == 0 {
            return None;
        }
        let f = ((t - self.t0) * SMOOTH_HZ).max(0.0);
        let i = (f.floor() as usize).min(n - 1);
        let j = (i + 1).min(n - 1);
        let k = (f - i as f64).clamp(0.0, 1.0) as f32;
        let (a, b) = (self.pts[i], self.pts[j]);
        Some((a.0 + (b.0 - a.0) * k, a.1 + (b.1 - a.1) * k))
    }

    /// Seconds the cursor has been still at `t`.
    pub fn still_for(&self, t: f64) -> f64 {
        let n = self.still_since.len();
        if n == 0 {
            return 0.0;
        }
        let i = (((t - self.t0) * SMOOTH_HZ).max(0.0) as usize).min(n - 1);
        (t - self.still_since[i]).max(0.0)
    }
}

/// How visible the cursor is, 0..1: fades out after 2 s still, back in at once.
pub fn idle_alpha(still_for: f64) -> f32 {
    (1.0 - ((still_for - 2.0) / 0.4).clamp(0.0, 1.0)) as f32
}

/// How long a click's ring lasts.
pub const CLICK_SECS: f64 = 0.45;

/// The click under way at `t`: how far along it is (0..1).
pub fn click_at(clicks: &[f64], t: f64) -> Option<f32> {
    let i = clicks.partition_point(|c| *c <= t);
    let c = *clicks.get(i.checked_sub(1)?)?;
    let p = (t - c) / CLICK_SECS;
    (p < 1.0).then_some(p as f32)
}

/// The cursor shrinks a little while pressed, then springs back.
pub fn press_scale(p: Option<f32>) -> f32 {
    match p {
        Some(p) if p < 0.4 => 1.0 - 0.18 * (p / 0.4 * std::f32::consts::PI).sin(),
        _ => 1.0,
    }
}

pub fn shape_at(shapes: &[(f64, Shape)], t: f64) -> Shape {
    let i = shapes.partition_point(|s| s.0 <= t);
    i.checked_sub(1).map(|i| shapes[i].1).unwrap_or_default()
}

/// The camera bubble at `t` as recorded: center and diameter (share of width).
pub fn camera_spot_at(spots: &[CameraSpot], t: f64) -> Option<(f32, f32, f32)> {
    let i = spots.partition_point(|s| s.0 <= t);
    let s = spots.get(i.saturating_sub(1))?;
    Some((s.1, s.2, s.3))
}

// ---------------------------------------------------------------- layout

/// Where the picture goes in the rendered video, in output pixels.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Layout {
    pub out: (u32, u32),
    /// The recorded picture inside the output.
    pub screen: egui::Rect,
    pub radius: f32,
    pub shadow: Option<(f32, f32)>,
}

/// Even sizes: H.264 wants them.
fn even(v: f32) -> u32 {
    ((v / 2.0).round() as u32 * 2).max(2)
}

/// The output keeps the recording's size: with a frame, the picture
/// shrinks inside it instead of the video growing.
pub fn layout(src: (u32, u32), frame: Option<&Frame>) -> Layout {
    let (w, h) = (src.0 as f32, src.1 as f32);
    let Some(f) = frame else {
        return Layout { out: (even(w), even(h)), screen: egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(w, h)), radius: 0.0, shadow: None };
    };
    let ppp = (w.max(h) / 1440.0).max(1.0);
    let g = render::frame_geom(w, h, f, ppp);
    let k = w.max(h) / g.size.x.max(g.size.y);
    let out = (even(g.size.x * k), even(g.size.y * k));
    let sx = out.0 as f32 / g.size.x;
    let sy = out.1 as f32 / g.size.y;
    let screen = egui::Rect::from_min_max(egui::pos2(g.inner.min.x * sx, g.inner.min.y * sy), egui::pos2(g.inner.max.x * sx, g.inner.max.y * sy));
    Layout { out, screen, radius: g.radius * k, shadow: g.shadow.map(|(b, d)| (b * k, d * k)) }
}

/// The camera bubble in output pixels: center and diameter.
pub fn camera_rect(style: &CameraStyle, lay: &Layout, recorded: Option<(f32, f32, f32)>) -> (egui::Pos2, f32) {
    let s = lay.screen;
    let short = (lay.out.0.min(lay.out.1)) as f32;
    let rec_d = recorded.map(|r| r.2 * s.width());
    let d = style.size.map(|k| k * short).or(rec_d).unwrap_or(CAMERA_SIZES[0] * short).max(16.0);
    let m = short * 0.04 + d / 2.0;
    let c = match (style.place, recorded) {
        (Place::Recorded, Some((x, y, _))) => egui::pos2(s.left() + x * s.width(), s.top() + y * s.height()),
        (Place::TopLeft, _) => egui::pos2(s.left() + m, s.top() + m),
        (Place::TopRight, _) => egui::pos2(s.right() - m, s.top() + m),
        (Place::BottomRight, _) => egui::pos2(s.right() - m, s.bottom() - m),
        _ => egui::pos2(s.left() + m, s.bottom() - m),
    };
    // keep it on the video
    let r = d / 2.0;
    let c = egui::pos2(c.x.clamp(r, (lay.out.0 as f32 - r).max(r)), c.y.clamp(r, (lay.out.1 as f32 - r).max(r)));
    (c, d)
}

// ---------------------------------------------------------------- captions

/// Captions: at most this many lines on screen at once.
const CAPTION_LINES: usize = 2;

/// The caption to show at `t` (video seconds), already split into the
/// lines that fit `max_w` px at `size` px. Long lines show in pages, each
/// for its share of the line's time.
pub fn caption_at(lines: &[Line], t: f64, size: f32, max_w: f32) -> Option<Vec<String>> {
    let l = lines.iter().find(|l| t >= l.start && t < l.end)?;
    let wrapped = wrap(&l.text, size, max_w);
    if wrapped.is_empty() {
        return None;
    }
    let pages = wrapped.len().div_ceil(CAPTION_LINES);
    let k = (((t - l.start) / (l.end - l.start).max(0.01)) * pages as f64).floor() as usize;
    let k = k.min(pages - 1);
    Some(wrapped.into_iter().skip(k * CAPTION_LINES).take(CAPTION_LINES).collect())
}

fn wrap(text: &str, size: f32, max_w: f32) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let mut cur = String::new();
    for w in text.split_whitespace() {
        let next = if cur.is_empty() { w.to_string() } else { format!("{cur} {w}") };
        if !cur.is_empty() && render::measure(&next, size).0 > max_w {
            out.push(std::mem::take(&mut cur));
            cur = w.to_string();
        } else {
            cur = next;
        }
    }
    if !cur.is_empty() {
        out.push(cur);
    }
    out
}

// ---------------------------------------------------------------- compositor

/// A color for a BGRA pixmap (tiny-skia reads it as RGBA).
pub fn bgra(c: Rgba8) -> tiny_skia::Color {
    tiny_skia::Color::from_rgba8(c[2], c[1], c[0], c[3])
}

/// A cursor picture: BGRA premultiplied, and its hot spot (px).
pub struct Sprite {
    pub img: Pixmap,
    pub hot: (f32, f32),
    /// The picture's pixels per point (2 for Retina cursor images).
    pub ppp: f32,
}

impl Sprite {
    /// From straight RGBA (as decoded from a PNG).
    pub fn from_rgba(rgba: &image::RgbaImage, hot: (f32, f32), ppp: f32) -> Option<Sprite> {
        let mut img = Pixmap::new(rgba.width(), rgba.height())?;
        for (d, s) in img.data_mut().chunks_exact_mut(4).zip(rgba.as_raw().chunks_exact(4)) {
            let a = s[3] as u32;
            d[0] = (s[2] as u32 * a / 255) as u8;
            d[1] = (s[1] as u32 * a / 255) as u8;
            d[2] = (s[0] as u32 * a / 255) as u8;
            d[3] = s[3];
        }
        Some(Sprite { img, hot, ppp })
    }

    /// A plain arrow, for when the system's picture isn't there.
    pub fn arrow() -> Sprite {
        let (w, h) = (34u32, 50u32);
        let mut img = Pixmap::new(w, h).expect("sprite");
        let pts = [(3.0, 3.0), (3.0, 39.0), (12.0, 31.0), (18.0, 45.0), (24.0, 42.0), (18.0, 29.0), (30.0, 29.0)];
        let mut pb = PathBuilder::new();
        pb.move_to(pts[0].0, pts[0].1);
        for p in &pts[1..] {
            pb.line_to(p.0, p.1);
        }
        pb.close();
        if let Some(path) = pb.finish() {
            let mut paint = Paint { anti_alias: true, ..Default::default() };
            paint.set_color(bgra([0, 0, 0, 255]));
            img.fill_path(&path, &paint, FillRule::Winding, Transform::identity(), None);
            paint.set_color(bgra([255, 255, 255, 255]));
            let stroke = tiny_skia::Stroke { width: 3.0, line_join: tiny_skia::LineJoin::Round, ..Default::default() };
            img.stroke_path(&path, &paint, &stroke, Transform::identity(), None);
        }
        Sprite { img, hot: (3.0, 3.0), ppp: 2.0 }
    }
}

/// The cursor pictures by shape.
pub struct Sprites {
    pub arrow: Sprite,
    pub text: Option<Sprite>,
    pub hand: Option<Sprite>,
}

impl Sprites {
    fn get(&self, s: Shape) -> &Sprite {
        match s {
            Shape::Arrow => &self.arrow,
            Shape::Text => self.text.as_ref().unwrap_or(&self.arrow),
            Shape::Hand => self.hand.as_ref().unwrap_or(&self.arrow),
        }
    }
}

/// One frame's inputs.
pub struct FrameIn<'a> {
    /// The recorded picture, BGRA, tight rows.
    pub screen: &'a [u8],
    pub screen_size: (u32, u32),
    /// The camera's picture, BGRA, tight rows, if any.
    pub camera: Option<(&'a [u8], (u32, u32))>,
    /// Video time (of the recording, after cuts: what the cursor and the
    /// captions are timed in).
    pub t: f64,
    /// Time in the recording before cuts, for the cursor and camera logs.
    pub source_t: f64,
}

/// Renders frames of one recording with one style.
pub struct Compositor {
    pub style: Style,
    pub lay: Layout,
    recorded: Recorded,
    path: Option<Path>,
    sprites: Sprites,
    /// The frame's backdrop and shadow, drawn once.
    base: Option<Pixmap>,
    lines: Vec<Line>,
    /// Points per output pixel… the other way: output pixels per point of
    /// the recorded screen (cursor and ring sizes follow it).
    px_per_pt: f32,
}

impl Compositor {
    /// `src`: the recording's size in pixels; `src_ppp`: its pixels per
    /// point (2 on Retina). `lines`: captions, in output time.
    pub fn new(style: Style, src: (u32, u32), src_ppp: f32, recorded: Recorded, sprites: Sprites, lines: Vec<Line>) -> Compositor {
        let lay = layout(src, style.frame.as_ref());
        let path = recorded.cursor.as_ref().map(|t| Path::new(t, style.cursor.smooth));
        let base = style.frame.as_ref().and_then(|f| backdrop(f, &lay));
        let px_per_pt = src_ppp.max(0.5) * lay.screen.width() / src.0.max(1) as f32;
        Compositor { style, lay, recorded, path, sprites, base, lines, px_per_pt }
    }

    pub fn out_size(&self) -> (u32, u32) {
        self.lay.out
    }

    /// Draw one output frame into `out` (BGRA, tight, out_size).
    pub fn draw(&self, f: &FrameIn, out: &mut [u8]) {
        let (ow, oh) = self.lay.out;
        debug_assert_eq!(out.len(), (ow * oh * 4) as usize);
        let Some(mut pm) = PixmapMut::from_bytes(out, ow, oh) else { return };
        let (sw, sh) = f.screen_size;
        let s = self.lay.screen;
        // the picture
        match &self.base {
            Some(base) => {
                pm.data_mut().copy_from_slice(base.data());
                if let Some(src) = tiny_skia::PixmapRef::from_bytes(f.screen, sw, sh) {
                    let tf = Transform::from_row(s.width() / sw as f32, 0.0, 0.0, s.height() / sh as f32, s.left(), s.top());
                    let paint = Paint { shader: Pattern::new(src, SpreadMode::Pad, FilterQuality::Bilinear, 1.0, tf), anti_alias: true, ..Default::default() };
                    if let Some(path) = rounded(s, self.lay.radius) {
                        pm.fill_path(&path, &paint, FillRule::Winding, Transform::identity(), None);
                    }
                }
            }
            None if (sw, sh) == (ow, oh) => pm.data_mut().copy_from_slice(&f.screen[..(ow * oh * 4) as usize]),
            None => {
                if let Some(src) = tiny_skia::PixmapRef::from_bytes(f.screen, sw, sh) {
                    let tf = Transform::from_scale(ow as f32 / sw as f32, oh as f32 / sh as f32);
                    pm.draw_pixmap(0, 0, src, &PixmapPaint { quality: FilterQuality::Bilinear, ..Default::default() }, tf, None);
                }
            }
        }
        self.draw_camera(&mut pm, f);
        self.draw_cursor(&mut pm, f.source_t);
        if self.style.captions {
            self.draw_caption(&mut pm, f.t);
        }
    }

    fn draw_camera(&self, pm: &mut PixmapMut, f: &FrameIn) {
        let cs = &self.style.camera;
        let Some((cam, (cw, ch))) = f.camera else { return };
        if !cs.show || !self.recorded.camera {
            return;
        }
        let Some(src) = tiny_skia::PixmapRef::from_bytes(cam, cw, ch) else { return };
        let (c, d) = camera_rect(cs, &self.lay, camera_spot_at(&self.recorded.camera_spots, f.source_t));
        let r = egui::Rect::from_center_size(c, egui::vec2(d, d));
        // fill the bubble: scale the shorter side to the diameter, center
        let k = d / cw.min(ch) as f32;
        let (dw, dh) = (cw as f32 * k, ch as f32 * k);
        let x0 = c.x - dw / 2.0;
        let y0 = c.y - dh / 2.0;
        let tf = if cs.mirror { Transform::from_row(-k, 0.0, 0.0, k, x0 + dw, y0) } else { Transform::from_row(k, 0.0, 0.0, k, x0, y0) };
        let radius = if cs.square { d * 0.22 } else { d / 2.0 };
        let Some(path) = rounded(r, radius) else { return };
        // soft shadow, then the picture, then a hairline
        let mut shade = Paint { anti_alias: true, ..Default::default() };
        shade.set_color(bgra([0, 0, 0, 60]));
        if let Some(sp) = rounded(r.translate(egui::vec2(0.0, d * 0.02)).expand(d * 0.012), radius + d * 0.012) {
            pm.fill_path(&sp, &shade, FillRule::Winding, Transform::identity(), None);
        }
        let paint = Paint { shader: Pattern::new(src, SpreadMode::Pad, FilterQuality::Bilinear, 1.0, tf), anti_alias: true, ..Default::default() };
        pm.fill_path(&path, &paint, FillRule::Winding, Transform::identity(), None);
        let mut line = Paint { anti_alias: true, ..Default::default() };
        line.set_color(bgra([255, 255, 255, 90]));
        pm.stroke_path(&path, &line, &tiny_skia::Stroke { width: (d * 0.008).max(1.0), ..Default::default() }, Transform::identity(), None);
    }

    fn draw_cursor(&self, pm: &mut PixmapMut, t: f64) {
        let cs = &self.style.cursor;
        let (Some(track), Some(path)) = (&self.recorded.cursor, &self.path) else { return };
        if !cs.show {
            return;
        }
        let Some((x, y)) = path.at(t) else { return };
        let s = self.lay.screen;
        let at = egui::pos2(s.left() + x * s.width(), s.top() + y * s.height());
        // a cursor that left the picture isn't drawn (a little slack for its body)
        if !s.expand(4.0).contains(at) {
            return;
        }
        let alpha = if cs.hide_idle { idle_alpha(path.still_for(t)) } else { 1.0 };
        if alpha <= 0.01 {
            return;
        }
        let click = if cs.clicks { click_at(&track.clicks, t) } else { None };
        if let Some(p) = click {
            let r = (10.0 + 22.0 * p) * self.px_per_pt;
            let mut pb = PathBuilder::new();
            pb.push_circle(at.x, at.y, r);
            if let Some(ring) = pb.finish() {
                let mut paint = Paint { anti_alias: true, ..Default::default() };
                paint.set_color(bgra([255, 255, 255, (150.0 * (1.0 - p) * alpha) as u8]));
                pm.stroke_path(&ring, &paint, &tiny_skia::Stroke { width: 2.5 * self.px_per_pt, ..Default::default() }, Transform::identity(), None);
                paint.set_color(bgra([0, 0, 0, (40.0 * (1.0 - p) * alpha) as u8]));
                pm.fill_path(&ring, &paint, FillRule::Winding, Transform::identity(), None);
            }
        }
        let sp = self.sprites.get(shape_at(&track.shapes, t));
        let k = cs.size * press_scale(click) * self.px_per_pt / sp.ppp;
        let tf = Transform::from_row(k, 0.0, 0.0, k, at.x - sp.hot.0 * k, at.y - sp.hot.1 * k);
        pm.draw_pixmap(0, 0, sp.img.as_ref(), &PixmapPaint { quality: FilterQuality::Bilinear, opacity: alpha, ..Default::default() }, tf, None);
    }

    fn draw_caption(&self, pm: &mut PixmapMut, t: f64) {
        let (ow, oh) = self.lay.out;
        let size = (oh.min(ow) as f32 * 0.042).max(12.0);
        let max_w = ow as f32 * 0.8;
        let Some(lines) = caption_at(&self.lines, t, size, max_w) else { return };
        let lh = render::measure("Ag", size).1;
        let widest = lines.iter().map(|l| render::measure(l, size).0).fold(0.0f32, f32::max);
        let pad = size * 0.5;
        let h = lh * lines.len() as f32 + pad;
        let bottom = self.lay.screen.bottom().min(oh as f32) - size * 1.2;
        let r = egui::Rect::from_min_max(egui::pos2(ow as f32 / 2.0 - widest / 2.0 - pad, bottom - h), egui::pos2(ow as f32 / 2.0 + widest / 2.0 + pad, bottom));
        if let Some(path) = rounded(r, size * 0.35) {
            let mut paint = Paint { anti_alias: true, ..Default::default() };
            paint.set_color(bgra([0, 0, 0, 165]));
            pm.fill_path(&path, &paint, FillRule::Winding, Transform::identity(), None);
        }
        for (k, l) in lines.iter().enumerate() {
            let w = render::measure(l, size).0;
            let at = egui::pos2(ow as f32 / 2.0 - w / 2.0, r.top() + pad / 2.0 + lh * k as f32);
            render::glyphs_into(pm.data_mut(), ow, oh, l, at, size, [255, 255, 255, 255]);
        }
    }
}

/// The frame's gradient and the picture's shadow, once per render.
fn backdrop(f: &Frame, lay: &Layout) -> Option<Pixmap> {
    let (ow, oh) = lay.out;
    // render::framed draws the same backdrop around a picture; here the
    // picture is a placeholder the frames are drawn over
    let (_, c0, c1) = render::BACKDROPS[f.bg.min(render::BACKDROPS.len() - 1)];
    let mut pm = Pixmap::new(ow, oh)?;
    let grad = tiny_skia::LinearGradient::new(
        tiny_skia::Point::from_xy(0.0, 0.0),
        tiny_skia::Point::from_xy(ow as f32, oh as f32),
        vec![tiny_skia::GradientStop::new(0.0, bgra(c0)), tiny_skia::GradientStop::new(1.0, bgra(c1))],
        SpreadMode::Pad,
        Transform::identity(),
    )?;
    pm.fill_rect(tiny_skia::Rect::from_xywh(0.0, 0.0, ow as f32, oh as f32)?, &Paint { shader: grad, ..Default::default() }, Transform::identity(), None);
    if let Some((blur, dy)) = lay.shadow {
        render::soft_shadow(&mut pm, lay.screen.translate(egui::vec2(0.0, dy)), lay.radius, blur);
    }
    Some(pm)
}

fn rounded(r: egui::Rect, radius: f32) -> Option<tiny_skia::Path> {
    let k = radius.min(r.width() / 2.0).min(r.height() / 2.0).max(0.0);
    if k <= 0.0 {
        return tiny_skia::Rect::from_xywh(r.left(), r.top(), r.width(), r.height()).map(PathBuilder::from_rect);
    }
    if (k - r.width() / 2.0).abs() < 0.01 && (r.width() - r.height()).abs() < 0.01 {
        let mut pb = PathBuilder::new();
        pb.push_circle(r.center().x, r.center().y, k);
        return pb.finish();
    }
    let (x0, y0, x1, y1) = (r.left(), r.top(), r.right(), r.bottom());
    let c = k * 0.4477; // circle-ish corners with cubics
    let mut pb = PathBuilder::new();
    pb.move_to(x0 + k, y0);
    pb.line_to(x1 - k, y0);
    pb.cubic_to(x1 - c, y0, x1, y0 + c, x1, y0 + k);
    pb.line_to(x1, y1 - k);
    pb.cubic_to(x1, y1 - c, x1 - c, y1, x1 - k, y1);
    pb.line_to(x0 + k, y1);
    pb.cubic_to(x0 + c, y1, x0, y1 - c, x0, y1 - k);
    pb.line_to(x0, y0 + k);
    pb.cubic_to(x0, y0 + c, x0 + c, y0, x0 + k, y0);
    pb.close();
    pb.finish()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn track() -> Track {
        Track { moves: vec![(0.0, 0.1, 0.1), (1.0, 0.1, 0.1), (1.01, 0.4, 0.4), (3.0, 0.4, 0.4)], clicks: vec![2.0], shapes: vec![(0.0, Shape::Arrow), (2.5, Shape::Text)] }
    }

    #[test]
    fn the_smooth_cursor_glides_without_overshoot_and_settles() {
        let p = Path::new(&track(), true);
        let (x, _) = p.at(1.02).unwrap();
        assert!(x > 0.1 && x < 0.38, "on its way, not there yet: {x}");
        let mut prev = 0.0;
        for k in 0..100 {
            let (x, _) = p.at(1.0 + k as f64 * 0.01).unwrap();
            assert!(x <= 0.4001, "no overshoot");
            assert!(x + 1e-4 >= prev, "always toward the target");
            prev = x;
        }
        let (x, y) = p.at(2.0).unwrap();
        assert!((x - 0.4).abs() < 0.002 && (y - 0.4).abs() < 0.002, "settled");
        let raw = Path::new(&track(), false);
        assert_eq!(raw.at(1.02), Some((0.4, 0.4)));
    }

    #[test]
    fn a_teleport_jumps() {
        let t = Track { moves: vec![(0.0, 0.0, 0.0), (0.5, 0.0, 0.0), (0.51, 0.9, 0.9)], ..Default::default() };
        let p = Path::new(&t, true);
        assert_eq!(p.at(0.52).map(|(x, _)| (x * 10.0).round()), Some(9.0));
    }

    #[test]
    fn idle_clicks_and_shapes() {
        let p = Path::new(&track(), true);
        assert!(p.still_for(0.9) > 0.8);
        assert!(p.still_for(1.05) < 0.1);
        assert_eq!(idle_alpha(1.0), 1.0);
        assert_eq!(idle_alpha(3.0), 0.0);
        assert_eq!(click_at(&[2.0], 1.9), None);
        assert!(click_at(&[2.0], 2.2).is_some_and(|p| (p - 0.444).abs() < 0.01));
        assert_eq!(click_at(&[2.0], 2.5), None);
        assert!(press_scale(Some(0.2)) < 0.9);
        assert_eq!(press_scale(None), 1.0);
        assert_eq!(shape_at(&track().shapes, 2.0), Shape::Arrow);
        assert_eq!(shape_at(&track().shapes, 2.6), Shape::Text);
    }

    #[test]
    fn layouts_keep_the_size_and_fit_the_frame() {
        let plain = layout((1920, 1080), None);
        assert_eq!(plain.out, (1920, 1080));
        let f = Frame { ratio: render::Ratio::Square, ..Frame::default() };
        let l = layout((1920, 1080), Some(&f));
        assert_eq!(l.out, (1920, 1920));
        assert!(l.screen.left() > 0.0 && l.screen.right() < 1920.0);
        assert!((l.screen.width() / l.screen.height() - 16.0 / 9.0).abs() < 0.01, "the picture keeps its shape");
        let odd = layout((1001, 667), None);
        assert_eq!((odd.out.0 % 2, odd.out.1 % 2), (0, 0));
    }

    #[test]
    fn the_camera_goes_where_asked_and_stays_on_the_video() {
        let lay = layout((1000, 600), None);
        let mut cs = CameraStyle::default();
        let (c, d) = camera_rect(&cs, &lay, Some((0.2, 0.7, 0.18)));
        assert_eq!((c.x.round(), c.y.round(), d.round()), (200.0, 420.0, 180.0));
        cs.place = Place::TopRight;
        cs.size = Some(0.2);
        let (c, d) = camera_rect(&cs, &lay, None);
        assert_eq!(d, 120.0);
        assert!(c.x > 800.0 && c.y < 200.0);
        let (c, _) = camera_rect(&CameraStyle::default(), &lay, Some((1.2, -0.3, 0.18)));
        assert!(c.x <= 1000.0 - 90.0 && c.y >= 90.0, "pulled back on");
    }

    #[test]
    fn long_captions_page_through() {
        let text = "one two three four five six seven eight nine ten eleven twelve";
        let lines = vec![Line { start: 0.0, end: 4.0, text: text.into(), words: vec![] }];
        let size = 20.0;
        let w = render::measure("one two three", size).0 + 1.0;
        let first = caption_at(&lines, 0.1, size, w).unwrap();
        assert_eq!(first.len(), 2);
        assert_eq!(first[0], "one two three");
        let last = caption_at(&lines, 3.99, size, w).unwrap();
        assert!(last.last().unwrap().ends_with("twelve"));
        assert_eq!(caption_at(&lines, 4.0, size, w), None);
    }

    fn solid(w: u32, h: u32, c: [u8; 4]) -> Vec<u8> {
        c.iter().copied().cycle().take((w * h * 4) as usize).collect()
    }

    #[test]
    fn frames_get_the_cursor_camera_and_backdrop() {
        let rec = Recorded { cursor: Some(track()), camera: true, camera_spots: vec![(0.0, 0.8, 0.8, 0.2)], fps: 30 };
        let style = Style::default();
        let sprites = Sprites { arrow: Sprite::arrow(), text: None, hand: None };
        let c = Compositor::new(style, (200, 100), 1.0, rec.clone(), sprites, vec![]);
        let screen = solid(200, 100, [10, 20, 30, 255]);
        let cam = solid(40, 30, [0, 0, 255, 255]); // red in BGRA
        let mut out = vec![0u8; 200 * 100 * 4];
        c.draw(&FrameIn { screen: &screen, screen_size: (200, 100), camera: Some((&cam, (40, 30))), t: 0.0, source_t: 0.0 }, &mut out);
        let px = |x: u32, y: u32| out[((y * 200 + x) * 4) as usize..][..4].to_vec();
        assert_eq!(px(5, 50), vec![10, 20, 30, 255], "the screen");
        assert_eq!(px(160, 80)[2], 255, "the camera bubble around (160, 80)");
        // the cursor's hot spot at (20, 10): its body below-right of it
        let body = px(23, 16);
        assert_ne!(body, vec![10, 20, 30, 255], "the cursor is drawn");

        let framed = Style { frame: Some(Frame::default()), ..Style::default() };
        let sprites = Sprites { arrow: Sprite::arrow(), text: None, hand: None };
        let c = Compositor::new(framed, (200, 100), 1.0, rec, sprites, vec![]);
        let (ow, oh) = c.out_size();
        let mut out = vec![0u8; (ow * oh * 4) as usize];
        c.draw(&FrameIn { screen: &screen, screen_size: (200, 100), camera: None, t: 0.0, source_t: 0.0 }, &mut out);
        let corner = &out[..4];
        let (_, c0, _) = render::BACKDROPS[0];
        assert_eq!(corner, &[c0[2], c0[1], c0[0], 255], "backdrop in BGRA");
        let mid = c.lay.screen.center();
        let i = ((mid.y as u32 * ow + mid.x as u32) * 4) as usize;
        assert_eq!(&out[i..i + 4], &[10, 20, 30, 255], "the picture inside");
    }

    #[test]
    fn captions_burn_in() {
        let style = Style { captions: true, ..Style::default() };
        let lines = vec![Line { start: 0.0, end: 2.0, text: "Hello".into(), words: vec![] }];
        let c = Compositor::new(style, (400, 300), 1.0, Recorded::default(), Sprites { arrow: Sprite::arrow(), text: None, hand: None }, lines);
        let screen = solid(400, 300, [200, 200, 200, 255]);
        let mut out = vec![0u8; 400 * 300 * 4];
        c.draw(&FrameIn { screen: &screen, screen_size: (400, 300), camera: None, t: 1.0, source_t: 1.0 }, &mut out);
        // a dark box near the bottom middle
        let dark = (230..290).any(|y| {
            let i = ((y * 400 + 200) * 4) as usize;
            out[i] < 120
        });
        assert!(dark, "caption box drawn");
    }
}
