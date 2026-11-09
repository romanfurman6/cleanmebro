//! The editor's marks and how they're burned into the final image. Pure
//! pixel work, no UI: pixel effects first (blur, eraser), then shapes and text
//! drawn with anti-aliasing, then the crop. Coordinates are image pixels.
//! Also the pixel readers behind the color picker and the ruler.

use super::settings::BlurStyle;
use egui::{Pos2, Rect, Vec2};
use image::{Rgba, RgbaImage};

pub type Rgba8 = [u8; 4];

#[derive(Clone, Debug, PartialEq)]
pub enum Mark {
    Arrow {
        a: Pos2,
        b: Pos2,
        color: Rgba8,
        width: f32,
    },
    Rect {
        r: Rect,
        color: Rgba8,
        width: f32,
    },
    /// A label: text on a filled rounded box of `color`.
    Text {
        at: Pos2,
        text: String,
        color: Rgba8,
        size: f32,
    },
    Highlight {
        r: Rect,
        color: Rgba8,
    },
    /// `boxes` empty = the whole `r`; otherwise only these (the text OCR found).
    Blur {
        r: Rect,
        style: BlurStyle,
        boxes: Vec<Rect>,
    },
    Erase {
        r: Rect,
        boxes: Vec<Rect>,
    },
    Step {
        at: Pos2,
        n: u32,
        color: Rgba8,
        size: f32,
    },
}

impl Mark {
    /// The area it covers, for hit tests and the selection outline.
    pub fn bounds(&self) -> Rect {
        match self {
            Mark::Arrow { a, b, width, .. } => Rect::from_two_pos(*a, *b).expand(*width * 2.0),
            Mark::Rect { r, width, .. } => r.expand(*width / 2.0),
            Mark::Highlight { r, .. } | Mark::Blur { r, .. } | Mark::Erase { r, .. } => *r,
            Mark::Text { at, text, size, .. } => {
                let s = label_size(text, *size);
                Rect::from_min_size(*at, s)
            }
            Mark::Step { at, size, .. } => Rect::from_center_size(*at, Vec2::splat(*size)),
        }
    }

    /// Distance-based hit test for selecting a mark.
    pub fn hit(&self, p: Pos2, slack: f32) -> bool {
        match self {
            Mark::Arrow { a, b, width, .. } => segment_distance(p, *a, *b) <= width + slack,
            Mark::Rect { r, width, .. } => {
                let outer = r.expand(width / 2.0 + slack);
                let inner = r.shrink(width / 2.0 + slack);
                outer.contains(p) && (!inner.is_positive() || !inner.contains(p))
            }
            _ => self.bounds().expand(slack).contains(p),
        }
    }

    pub fn translate(&mut self, d: Vec2) {
        match self {
            Mark::Arrow { a, b, .. } => {
                *a += d;
                *b += d;
            }
            Mark::Rect { r, .. } | Mark::Highlight { r, .. } => *r = r.translate(d),
            Mark::Blur { r, boxes, .. } | Mark::Erase { r, boxes } => {
                *r = r.translate(d);
                for b in boxes {
                    *b = b.translate(d);
                }
            }
            Mark::Text { at, .. } | Mark::Step { at, .. } => *at += d,
        }
    }

    /// Pixel effects are applied under every shape.
    pub fn is_effect(&self) -> bool {
        matches!(self, Mark::Blur { .. } | Mark::Erase { .. })
    }
}

/// Everything the editor knows about one capture.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Doc {
    pub marks: Vec<Mark>,
    pub crop: Option<Rect>,
}

impl Doc {
    /// The next number for the Steps tool.
    pub fn next_step(&self) -> u32 {
        self.marks
            .iter()
            .filter_map(|m| {
                if let Mark::Step { n, .. } = m {
                    Some(*n)
                } else {
                    None
                }
            })
            .max()
            .unwrap_or(0)
            + 1
    }
}

/// The finished image: effects, shapes, then the crop.
pub fn render(base: &RgbaImage, doc: &Doc) -> RgbaImage {
    let mut img = apply_effects(base, &doc.marks);
    draw_shapes(&mut img, &doc.marks);
    match doc
        .crop
        .and_then(|c| clamp_rect(c, img.width(), img.height()))
    {
        Some((x, y, w, h)) => image::imageops::crop_imm(&img, x, y, w, h).to_image(),
        None => img,
    }
}

/// Only the pixel effects (Blur, Eraser): what the editor shows under its
/// live shapes.
pub fn apply_effects(base: &RgbaImage, marks: &[Mark]) -> RgbaImage {
    let mut img = base.clone();
    for m in marks {
        match m {
            Mark::Blur { r, style, boxes } => {
                for b in targets(*r, boxes) {
                    match style {
                        BlurStyle::Pixelate => pixelate(&mut img, b),
                        BlurStyle::Blur => box_blur(&mut img, b),
                        BlurStyle::Solid => fill(&mut img, b, [28, 28, 31, 255]),
                    }
                }
            }
            Mark::Erase { r, boxes } => {
                for b in targets(*r, boxes) {
                    let c = surrounding_color(&img, b);
                    fill(&mut img, b, c);
                }
            }
            _ => {}
        }
    }
    img
}

fn targets(r: Rect, boxes: &[Rect]) -> Vec<Rect> {
    if boxes.is_empty() {
        vec![r]
    } else {
        // a little air around each word so no edge pixels survive
        boxes.iter().map(|b| b.expand(2.0)).collect()
    }
}

/// A rect clamped to the image, as integer x, y, w, h. None if empty.
pub fn clamp_rect(r: Rect, w: u32, h: u32) -> Option<(u32, u32, u32, u32)> {
    let x0 = r.min.x.min(r.max.x).floor().clamp(0.0, w as f32) as u32;
    let y0 = r.min.y.min(r.max.y).floor().clamp(0.0, h as f32) as u32;
    let x1 = r.min.x.max(r.max.x).ceil().clamp(0.0, w as f32) as u32;
    let y1 = r.min.y.max(r.max.y).ceil().clamp(0.0, h as f32) as u32;
    (x1 > x0 && y1 > y0).then_some((x0, y0, x1 - x0, y1 - y0))
}

fn fill(img: &mut RgbaImage, r: Rect, c: Rgba8) {
    let Some((x, y, w, h)) = clamp_rect(r, img.width(), img.height()) else {
        return;
    };
    for yy in y..y + h {
        for xx in x..x + w {
            img.put_pixel(xx, yy, Rgba(c));
        }
    }
}

/// Big square blocks, each the average of what it covers.
fn pixelate(img: &mut RgbaImage, r: Rect) {
    let Some((x, y, w, h)) = clamp_rect(r, img.width(), img.height()) else {
        return;
    };
    let block = (w.min(h) / 3).clamp(6, 24);
    let mut by = y;
    while by < y + h {
        let bh = block.min(y + h - by);
        let mut bx = x;
        while bx < x + w {
            let bw = block.min(x + w - bx);
            let mut sum = [0u64; 4];
            for yy in by..by + bh {
                for xx in bx..bx + bw {
                    let p = img.get_pixel(xx, yy).0;
                    for k in 0..4 {
                        sum[k] += p[k] as u64;
                    }
                }
            }
            let n = (bw * bh) as u64;
            let avg = Rgba([
                (sum[0] / n) as u8,
                (sum[1] / n) as u8,
                (sum[2] / n) as u8,
                (sum[3] / n) as u8,
            ]);
            for yy in by..by + bh {
                for xx in bx..bx + bw {
                    img.put_pixel(xx, yy, avg);
                }
            }
            bx += bw;
        }
        by += bh;
    }
}

/// Three box-blur passes inside the rect (close to a Gaussian).
fn box_blur(img: &mut RgbaImage, r: Rect) {
    let Some((x, y, w, h)) = clamp_rect(r, img.width(), img.height()) else {
        return;
    };
    let radius = (w.min(h) / 4).clamp(4, 16) as i64;
    let mut buf: Vec<[f32; 4]> = Vec::with_capacity((w * h) as usize);
    for yy in y..y + h {
        for xx in x..x + w {
            let p = img.get_pixel(xx, yy).0;
            buf.push([p[0] as f32, p[1] as f32, p[2] as f32, p[3] as f32]);
        }
    }
    let (w, h) = (w as i64, h as i64);
    for _ in 0..3 {
        // horizontal then vertical running averages, edges clamped
        for pass in 0..2 {
            let (outer, inner) = if pass == 0 { (h, w) } else { (w, h) };
            let at = |o: i64, i: i64| {
                if pass == 0 {
                    (o * w + i) as usize
                } else {
                    (i * w + o) as usize
                }
            };
            let mut line = vec![[0f32; 4]; inner as usize];
            for o in 0..outer {
                for i in 0..inner {
                    let mut s = [0f32; 4];
                    for d in -radius..=radius {
                        let p = buf[at(o, (i + d).clamp(0, inner - 1))];
                        for k in 0..4 {
                            s[k] += p[k];
                        }
                    }
                    let n = (2 * radius + 1) as f32;
                    line[i as usize] = [s[0] / n, s[1] / n, s[2] / n, s[3] / n];
                }
                for i in 0..inner {
                    buf[at(o, i)] = line[i as usize];
                }
            }
        }
    }
    for yy in 0..h {
        for xx in 0..w {
            let p = buf[(yy * w + xx) as usize];
            img.put_pixel(
                x + xx as u32,
                y + yy as u32,
                Rgba([p[0] as u8, p[1] as u8, p[2] as u8, p[3] as u8]),
            );
        }
    }
}

/// The background around a rect: the per-channel median of the ring of
/// pixels just outside it. That's what the Eraser fills with.
pub fn surrounding_color(img: &RgbaImage, r: Rect) -> Rgba8 {
    let (w, h) = (img.width() as i64, img.height() as i64);
    let x0 = r.min.x.floor() as i64 - 1;
    let y0 = r.min.y.floor() as i64 - 1;
    let x1 = r.max.x.ceil() as i64;
    let y1 = r.max.y.ceil() as i64;
    let mut ring: Vec<Rgba8> = Vec::new();
    let mut take = |x: i64, y: i64| {
        if x >= 0 && y >= 0 && x < w && y < h {
            ring.push(img.get_pixel(x as u32, y as u32).0);
        }
    };
    for x in x0..=x1 {
        take(x, y0);
        take(x, y1);
    }
    for y in y0 + 1..y1 {
        take(x0, y);
        take(x1, y);
    }
    if ring.is_empty() {
        return [255, 255, 255, 255];
    }
    let mut out = [0u8; 4];
    for (k, o) in out.iter_mut().enumerate() {
        let mut ch: Vec<u8> = ring.iter().map(|p| p[k]).collect();
        ch.sort_unstable();
        *o = ch[ch.len() / 2];
    }
    out
}

// ---------------------------------------------------------------- shapes

/// The arrow head's three corners for a shaft from `a` to `b`.
pub fn arrow_head(a: Pos2, b: Pos2, width: f32) -> [Pos2; 3] {
    let d = (b - a).normalized();
    let d = if d.is_finite() { d } else { Vec2::X };
    let n = Vec2::new(-d.y, d.x);
    let len = (width * 4.5).max(10.0);
    let base = b - d * len;
    [b, base + n * len * 0.55, base - n * len * 0.55]
}

/// Where the shaft stops so its round cap hides under the head.
pub fn shaft_end(a: Pos2, b: Pos2, width: f32) -> Pos2 {
    let d = (b - a).normalized();
    if !d.is_finite() {
        return b;
    }
    b - d * (width * 4.5).max(10.0) * 0.6
}

/// Padding of a text label's box, as a fraction of its font size.
const LABEL_PAD: f32 = 0.35;

/// Size of a text label box (text plus padding) at `size` px.
pub fn label_size(text: &str, size: f32) -> Vec2 {
    let (tw, th) = measure(text, size);
    Vec2::new(tw + 2.0 * LABEL_PAD * size, th + LABEL_PAD * size)
}

/// White on dark colors, near-black on light ones.
pub fn ink_on(c: Rgba8) -> Rgba8 {
    let l = 0.2126 * c[0] as f32 + 0.7152 * c[1] as f32 + 0.0722 * c[2] as f32;
    if l > 160.0 {
        [28, 28, 31, 255]
    } else {
        [255, 255, 255, 255]
    }
}

fn draw_shapes(img: &mut RgbaImage, marks: &[Mark]) {
    use tiny_skia::{FillRule, LineCap, LineJoin, Paint, PathBuilder, Pixmap, Stroke, Transform};
    let (w, h) = img.dimensions();
    if !marks.iter().any(|m| !m.is_effect()) || w == 0 || h == 0 {
        return;
    }
    let Some(mut pm) = Pixmap::new(w, h) else {
        return;
    };
    // straight → premultiplied
    for (dst, src) in pm
        .data_mut()
        .chunks_exact_mut(4)
        .zip(img.as_raw().chunks_exact(4))
    {
        let a = src[3] as u32;
        dst[0] = (src[0] as u32 * a / 255) as u8;
        dst[1] = (src[1] as u32 * a / 255) as u8;
        dst[2] = (src[2] as u32 * a / 255) as u8;
        dst[3] = src[3];
    }
    let paint = |c: Rgba8| {
        let mut p = Paint::default();
        p.set_color_rgba8(c[0], c[1], c[2], c[3]);
        p.anti_alias = true;
        p
    };
    let stroke = |width: f32| Stroke {
        width,
        line_cap: LineCap::Round,
        line_join: LineJoin::Round,
        ..Default::default()
    };
    let t = Transform::identity();
    for m in marks {
        match m {
            Mark::Arrow { a, b, color, width } => {
                let end = shaft_end(*a, *b, *width);
                let mut pb = PathBuilder::new();
                pb.move_to(a.x, a.y);
                pb.line_to(end.x, end.y);
                if let Some(path) = pb.finish() {
                    pm.stroke_path(&path, &paint(*color), &stroke(*width), t, None);
                }
                let [p0, p1, p2] = arrow_head(*a, *b, *width);
                let mut pb = PathBuilder::new();
                pb.move_to(p0.x, p0.y);
                pb.line_to(p1.x, p1.y);
                pb.line_to(p2.x, p2.y);
                pb.close();
                if let Some(path) = pb.finish() {
                    pm.fill_path(&path, &paint(*color), FillRule::Winding, t, None);
                    pm.stroke_path(&path, &paint(*color), &stroke(width * 0.6), t, None);
                }
            }
            Mark::Rect { r, color, width } => {
                if let Some(path) = rounded(*r, width * 1.5) {
                    pm.stroke_path(&path, &paint(*color), &stroke(*width), t, None);
                }
            }
            Mark::Highlight { r, color } => {
                if let Some(path) = rounded(*r, 2.0) {
                    pm.fill_path(
                        &path,
                        &paint([color[0], color[1], color[2], 110]),
                        FillRule::Winding,
                        t,
                        None,
                    );
                }
            }
            Mark::Step { at, n, color, size } => {
                let mut pb = PathBuilder::new();
                pb.push_circle(at.x, at.y, size / 2.0);
                if let Some(path) = pb.finish() {
                    pm.fill_path(&path, &paint(*color), FillRule::Winding, t, None);
                }
                let label = n.to_string();
                let (tw, th) = measure(&label, size * 0.55);
                glyphs(
                    &mut pm,
                    &label,
                    Pos2::new(at.x - tw / 2.0, at.y - th / 2.0),
                    size * 0.55,
                    ink_on(*color),
                );
            }
            Mark::Text {
                at,
                text,
                color,
                size,
            } => {
                let s = label_size(text, *size);
                if let Some(path) = rounded(Rect::from_min_size(*at, s), size * 0.3) {
                    pm.fill_path(&path, &paint(*color), FillRule::Winding, t, None);
                }
                glyphs(
                    &mut pm,
                    text,
                    *at + Vec2::new(LABEL_PAD * size, LABEL_PAD * size / 2.0),
                    *size,
                    ink_on(*color),
                );
            }
            Mark::Blur { .. } | Mark::Erase { .. } => {}
        }
    }
    // premultiplied → straight
    for (dst, src) in img.chunks_exact_mut(4).zip(pm.data().chunks_exact(4)) {
        let a = src[3] as u32;
        if a == 0 {
            dst.copy_from_slice(&[0, 0, 0, 0]);
        } else {
            dst[0] = (src[0] as u32 * 255 / a).min(255) as u8;
            dst[1] = (src[1] as u32 * 255 / a).min(255) as u8;
            dst[2] = (src[2] as u32 * 255 / a).min(255) as u8;
            dst[3] = src[3];
        }
    }
}

fn rounded(r: Rect, radius: f32) -> Option<tiny_skia::Path> {
    let r = Rect::from_two_pos(r.min, r.max);
    let k = radius.min(r.width() / 2.0).min(r.height() / 2.0).max(0.0);
    let mut pb = tiny_skia::PathBuilder::new();
    let (x0, y0, x1, y1) = (r.min.x, r.min.y, r.max.x, r.max.y);
    pb.move_to(x0 + k, y0);
    pb.line_to(x1 - k, y0);
    pb.quad_to(x1, y0, x1, y0 + k);
    pb.line_to(x1, y1 - k);
    pb.quad_to(x1, y1, x1 - k, y1);
    pb.line_to(x0 + k, y1);
    pb.quad_to(x0, y1, x0, y1 - k);
    pb.line_to(x0, y0 + k);
    pb.quad_to(x0, y0, x0 + k, y0);
    pb.close();
    pb.finish()
}

// ---------------------------------------------------------------- text

/// A bold system font for labels in exported images.
fn label_font() -> Option<&'static ab_glyph::FontVec> {
    static FONT: std::sync::OnceLock<Option<ab_glyph::FontVec>> = std::sync::OnceLock::new();
    FONT.get_or_init(|| {
        let candidates: [(&str, u32); 5] = [
            ("/System/Library/Fonts/Supplemental/Arial Bold.ttf", 0),
            ("/System/Library/Fonts/Helvetica.ttc", 1),
            ("/System/Library/Fonts/SFNS.ttf", 0),
            ("/usr/share/fonts/truetype/dejavu/DejaVuSans-Bold.ttf", 0),
            ("/usr/share/fonts/TTF/DejaVuSans-Bold.ttf", 0),
        ];
        candidates.iter().find_map(|(path, index)| {
            let bytes = std::fs::read(path).ok()?;
            ab_glyph::FontVec::try_from_vec_and_index(bytes, *index).ok()
        })
    })
    .as_ref()
}

/// Width and line height of `text` at `size` px. A rough estimate when no
/// font is available.
pub fn measure(text: &str, size: f32) -> (f32, f32) {
    use ab_glyph::{Font, ScaleFont};
    let Some(font) = label_font() else {
        return (text.chars().count() as f32 * size * 0.6, size * 1.2);
    };
    let sf = font.as_scaled(size);
    let mut w: f32 = 0.0;
    let mut prev = None;
    let mut widest: f32 = 0.0;
    let mut lines = 0;
    for line in text.split('\n') {
        lines += 1;
        w = 0.0;
        for ch in line.chars() {
            let id = sf.glyph_id(ch);
            if let Some(p) = prev {
                w += sf.kern(p, id);
            }
            w += sf.h_advance(id);
            prev = Some(id);
        }
        widest = widest.max(w);
        prev = None;
    }
    let _ = w;
    (widest, sf.height() * lines.max(1) as f32)
}

/// Blend `text` into a premultiplied pixmap, top-left at `pos`.
fn glyphs(pm: &mut tiny_skia::Pixmap, text: &str, pos: Pos2, size: f32, color: Rgba8) {
    use ab_glyph::{point, Font, ScaleFont};
    let Some(font) = label_font() else { return };
    let sf = font.as_scaled(size);
    let (w, h) = (pm.width() as i64, pm.height() as i64);
    let data = pm.data_mut();
    let mut y = pos.y + sf.ascent();
    for line in text.split('\n') {
        let mut x = pos.x;
        let mut prev = None;
        for ch in line.chars() {
            let id = sf.glyph_id(ch);
            if let Some(p) = prev {
                x += sf.kern(p, id);
            }
            let g = id.with_scale_and_position(size, point(x, y));
            x += sf.h_advance(id);
            prev = Some(id);
            let Some(outline) = font.outline_glyph(g) else {
                continue;
            };
            let bb = outline.px_bounds();
            outline.draw(|gx, gy, cov| {
                let px = bb.min.x as i64 + gx as i64;
                let py = bb.min.y as i64 + gy as i64;
                if px < 0 || py < 0 || px >= w || py >= h {
                    return;
                }
                let a = cov.clamp(0.0, 1.0) * color[3] as f32 / 255.0;
                let i = ((py * w + px) * 4) as usize;
                for k in 0..3 {
                    let s = color[k] as f32 * a;
                    data[i + k] = (s + data[i + k] as f32 * (1.0 - a)).round() as u8;
                }
                data[i + 3] = (255.0 * a + data[i + 3] as f32 * (1.0 - a)).round() as u8;
            });
        }
        y += sf.height();
    }
}

// ---------------------------------------------------------------- picker and ruler

pub fn hex(c: Rgba8) -> String {
    format!("#{:02X}{:02X}{:02X}", c[0], c[1], c[2])
}

pub fn pixel(img: &RgbaImage, x: i64, y: i64) -> Option<Rgba8> {
    (x >= 0 && y >= 0 && (x as u32) < img.width() && (y as u32) < img.height())
        .then(|| img.get_pixel(x as u32, y as u32).0)
}

fn close(a: Rgba8, b: Rgba8, tol: i32) -> bool {
    (0..3).all(|k| (a[k] as i32 - b[k] as i32).abs() <= tol)
}

/// The ruler: from (x, y), how far the same color runs left, right, up and
/// down before an edge. Returns the span (x0, x1, y0, y1), inclusive.
pub fn edge_span(img: &RgbaImage, x: i64, y: i64, tol: i32) -> Option<(i64, i64, i64, i64)> {
    let c = pixel(img, x, y)?;
    let run = |dx: i64, dy: i64| {
        let (mut cx, mut cy) = (x, y);
        while let Some(p) = pixel(img, cx + dx, cy + dy) {
            if !close(p, c, tol) {
                break;
            }
            cx += dx;
            cy += dy;
        }
        (cx, cy)
    };
    let (x0, _) = run(-1, 0);
    let (x1, _) = run(1, 0);
    let (_, y0) = run(0, -1);
    let (_, y1) = run(0, 1);
    Some((x0, x1, y0, y1))
}

fn segment_distance(p: Pos2, a: Pos2, b: Pos2) -> f32 {
    let ab = b - a;
    let t = if ab.length_sq() > 0.0 {
        ((p - a).dot(ab) / ab.length_sq()).clamp(0.0, 1.0)
    } else {
        0.0
    };
    (p - (a + ab * t)).length()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn canvas() -> RgbaImage {
        RgbaImage::from_pixel(120, 80, Rgba([240, 240, 240, 255]))
    }

    #[test]
    fn arrow_and_rect_paint_their_color() {
        let mut d = Doc::default();
        d.marks.push(Mark::Arrow {
            a: Pos2::new(10.0, 40.0),
            b: Pos2::new(110.0, 40.0),
            color: [255, 0, 0, 255],
            width: 4.0,
        });
        d.marks.push(Mark::Rect {
            r: Rect::from_min_max(Pos2::new(20.0, 5.0), Pos2::new(60.0, 25.0)),
            color: [0, 0, 255, 255],
            width: 4.0,
        });
        let out = render(&canvas(), &d);
        assert_eq!(out.get_pixel(50, 40).0, [255, 0, 0, 255], "shaft");
        assert_eq!(out.get_pixel(108, 40).0[0], 255, "head reaches the tip");
        assert_eq!(out.get_pixel(40, 5).0, [0, 0, 255, 255], "rect edge");
        assert_eq!(
            out.get_pixel(40, 15).0,
            [240, 240, 240, 255],
            "rect inside stays"
        );
    }

    #[test]
    fn pixelate_and_blur_hide_detail_only_inside() {
        let mut base = canvas();
        for x in 0..120 {
            for y in 0..80 {
                if (x + y) % 2 == 0 {
                    base.put_pixel(x, y, Rgba([0, 0, 0, 255]));
                }
            }
        }
        let r = Rect::from_min_max(Pos2::new(30.0, 20.0), Pos2::new(90.0, 60.0));
        for style in [BlurStyle::Pixelate, BlurStyle::Blur] {
            let out = apply_effects(
                &base,
                &[Mark::Blur {
                    r,
                    style,
                    boxes: vec![],
                }],
            );
            let a = out.get_pixel(50, 30).0[0] as i32;
            let b = out.get_pixel(51, 30).0[0] as i32;
            assert!(
                (a - b).abs() < 40,
                "{style:?}: checkerboard is smoothed inside ({a} vs {b})"
            );
            assert_eq!(
                out.get_pixel(5, 5),
                base.get_pixel(5, 5),
                "{style:?}: outside untouched"
            );
        }
        let solid = apply_effects(
            &base,
            &[Mark::Blur {
                r,
                style: BlurStyle::Solid,
                boxes: vec![],
            }],
        );
        assert_eq!(solid.get_pixel(60, 40).0, [28, 28, 31, 255]);
    }

    #[test]
    fn eraser_fills_with_the_background_and_respects_text_boxes() {
        let mut base = RgbaImage::from_pixel(100, 40, Rgba([10, 120, 200, 255]));
        // "text": dark pixels in two words
        for (x0, x1) in [(10u32, 30u32), (50, 70)] {
            for x in x0..x1 {
                for y in 15..25 {
                    base.put_pixel(x, y, Rgba([0, 0, 0, 255]));
                }
            }
        }
        let r = Rect::from_min_max(Pos2::new(5.0, 10.0), Pos2::new(40.0, 30.0));
        let out = apply_effects(&base, &[Mark::Erase { r, boxes: vec![] }]);
        assert_eq!(out.get_pixel(20, 20).0, [10, 120, 200, 255]);
        assert_eq!(
            out.get_pixel(60, 20).0,
            [0, 0, 0, 255],
            "outside the box stays"
        );
        // only-text: the second word's box, nothing else
        let word = Rect::from_min_max(Pos2::new(50.0, 15.0), Pos2::new(70.0, 25.0));
        let out = apply_effects(
            &base,
            &[Mark::Erase {
                r: Rect::from_min_max(Pos2::new(0.0, 0.0), Pos2::new(100.0, 40.0)),
                boxes: vec![word],
            }],
        );
        assert_eq!(out.get_pixel(60, 20).0, [10, 120, 200, 255]);
        assert_eq!(
            out.get_pixel(20, 20).0,
            [0, 0, 0, 255],
            "first word wasn't in the boxes"
        );
    }

    #[test]
    fn crop_happens_last_and_clamps() {
        let mut d = Doc::default();
        d.crop = Some(Rect::from_min_max(
            Pos2::new(100.0, 70.0),
            Pos2::new(500.0, 500.0),
        ));
        assert_eq!(render(&canvas(), &d).dimensions(), (20, 10));
        d.crop = Some(Rect::from_min_max(
            Pos2::new(200.0, 200.0),
            Pos2::new(300.0, 300.0),
        ));
        assert_eq!(
            render(&canvas(), &d).dimensions(),
            (120, 80),
            "empty crop is ignored"
        );
    }

    #[test]
    fn labels_and_steps_draw_a_filled_box() {
        let mut d = Doc::default();
        d.marks.push(Mark::Text {
            at: Pos2::new(5.0, 5.0),
            text: "Hi".into(),
            color: [255, 69, 58, 255],
            size: 20.0,
        });
        d.marks.push(Mark::Step {
            at: Pos2::new(90.0, 60.0),
            n: d.next_step(),
            color: [255, 69, 58, 255],
            size: 26.0,
        });
        assert_eq!(d.next_step(), 2);
        let out = render(&canvas(), &d);
        assert_eq!(
            out.get_pixel(6, 7).0[1],
            69,
            "label box corner area is the label color"
        );
        assert_eq!(out.get_pixel(90, 50).0[1], 69, "step circle");
    }

    #[test]
    fn hit_tests() {
        let a = Mark::Arrow {
            a: Pos2::new(0.0, 0.0),
            b: Pos2::new(100.0, 0.0),
            color: [0; 4],
            width: 4.0,
        };
        assert!(a.hit(Pos2::new(50.0, 3.0), 2.0));
        assert!(!a.hit(Pos2::new(50.0, 20.0), 2.0));
        let r = Mark::Rect {
            r: Rect::from_min_max(Pos2::new(0.0, 0.0), Pos2::new(100.0, 100.0)),
            color: [0; 4],
            width: 4.0,
        };
        assert!(r.hit(Pos2::new(0.0, 50.0), 2.0));
        assert!(!r.hit(Pos2::new(50.0, 50.0), 2.0), "hollow inside");
    }

    #[test]
    fn ruler_and_picker() {
        let mut img = canvas();
        for x in 20..60 {
            for y in 10..30 {
                img.put_pixel(x, y, Rgba([11, 107, 216, 255]));
            }
        }
        assert_eq!(hex(pixel(&img, 30, 20).unwrap()), "#0B6BD8");
        assert_eq!(edge_span(&img, 30, 20, 8), Some((20, 59, 10, 29)));
        assert_eq!(pixel(&img, -1, 0), None);
        assert_eq!(
            surrounding_color(
                &img,
                Rect::from_min_max(Pos2::new(21.0, 11.0), Pos2::new(58.0, 28.0))
            ),
            [11, 107, 216, 255]
        );
    }
}
