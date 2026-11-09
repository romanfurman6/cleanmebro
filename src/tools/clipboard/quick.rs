//! Quick paste: the window the shortcut opens over any app. Two layouts:
//! a shelf of big preview cards (default) or a compact list with a preview.
//! Type to search, arrows to move, ↵ pastes, ⇧↵ pastes plain text, ⌘1–9
//! pick directly, ⌘P pins, ⌘⌫ deletes, esc closes. Clicking outside closes.

use super::settings::Layout;
use super::store::{parse_color, Entry, Kind};
use super::ui::{app_color, chips, cover_image, entry_thumb, kind_glyph, pin_glyph, preview};
use super::{Clipboard, Filter, TINT};
use crate::kit::glass::*;
use egui::{
    Align2, Color32, Key, Pos2, Rect, Sense, Stroke, StrokeKind, Ui, Vec2, ViewportBuilder,
    ViewportCommand, ViewportId,
};

pub struct Quick {
    pub layout: Layout,
    /// The app to paste into (it was in front when quick paste opened).
    pub front: Option<i32>,
    query: String,
    filter: Filter,
    sel: usize,
    frames: u32,
    /// The window had focus once: losing it now means the user clicked away.
    focused: bool,
}

/// What the user chose.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Act {
    Paste(u64, bool),
    Pin(u64),
    Delete(u64),
    Close,
}

const CARDS_SIZE: Vec2 = Vec2::new(1180.0, 420.0);
const LIST_SIZE: Vec2 = Vec2::new(760.0, 520.0);
const CARD_W: f32 = 172.0;
const CARD_H: f32 = 284.0;
const CARD_GAP: f32 = 14.0;

impl Quick {
    pub fn new(layout: Layout, front: Option<i32>) -> Quick {
        Quick {
            layout,
            front,
            query: String::new(),
            filter: Filter::All,
            sel: 0,
            frames: 0,
            focused: false,
        }
    }

    pub fn size(&self) -> Vec2 {
        match self.layout {
            Layout::Cards => CARDS_SIZE,
            Layout::List => LIST_SIZE,
        }
    }
}

/// Keys pressed this frame, taken before the search field sees them.
fn keys(ui: &mut Ui, cards: bool) -> (Option<Act>, i32, Option<usize>, bool, bool) {
    let mut act = None;
    let mut step = 0i32;
    let mut pick = None;
    let mut pin = false;
    let mut del = false;
    ui.ctx().input_mut(|i| {
        if i.consume_key(egui::Modifiers::NONE, Key::Escape) {
            act = Some(Act::Close);
        }
        let (back, fwd) = if cards {
            (Key::ArrowLeft, Key::ArrowRight)
        } else {
            (Key::ArrowUp, Key::ArrowDown)
        };
        for (k, d) in [
            (back, -1),
            (fwd, 1),
            (Key::ArrowUp, -1),
            (Key::ArrowDown, 1),
        ] {
            while i.consume_key(egui::Modifiers::NONE, k) {
                step += d;
            }
        }
        if i.consume_key(egui::Modifiers::COMMAND, Key::P) {
            pin = true;
        }
        if i.consume_key(egui::Modifiers::COMMAND, Key::Backspace) {
            del = true;
        }
        let nums = [
            Key::Num1,
            Key::Num2,
            Key::Num3,
            Key::Num4,
            Key::Num5,
            Key::Num6,
            Key::Num7,
            Key::Num8,
            Key::Num9,
        ];
        for (n, k) in nums.iter().enumerate() {
            if i.consume_key(egui::Modifiers::COMMAND, *k) {
                pick = Some(n);
            }
        }
        if i.consume_key(egui::Modifiers::SHIFT, Key::Enter) {
            act = Some(Act::Paste(0, true));
        } else if i.consume_key(egui::Modifiers::NONE, Key::Enter) {
            act = Some(Act::Paste(0, false));
        }
    });
    (act, step, pick, pin, del)
}

/// The window's content in `r`. Returns what the user chose.
pub fn draw(c: &mut Clipboard, q: &mut Quick, ui: &mut Ui, r: Rect) -> Option<Act> {
    let cards = q.layout == Layout::Cards;
    let (mut act, step, pick, pin, del) = keys(ui, cards);
    let ids = c.visible(q.filter, &q.query);
    if step != 0 && !ids.is_empty() {
        q.sel = (q.sel as i32 + step).clamp(0, ids.len() as i32 - 1) as usize;
    }
    q.sel = q.sel.min(ids.len().saturating_sub(1));
    if let Some(Act::Paste(_, plain)) = act {
        act = ids.get(q.sel).map(|id| Act::Paste(*id, plain));
    }
    if let Some(n) = pick {
        if let Some(id) = ids.get(n) {
            act = Some(Act::Paste(*id, false));
        }
    }
    if pin {
        act = ids.get(q.sel).map(|id| Act::Pin(*id));
    }
    if del {
        act = ids.get(q.sel).map(|id| Act::Delete(*id));
    }

    let p = ui.painter().clone();
    let radius = if cards { 18.0 } else { 14.0 };
    p.rect_filled(r, radius, BG);
    p.rect_stroke(r, radius, Stroke::new(1.0, white(31)), StrokeKind::Inside);

    // top bar
    let top = r.top() + 14.0;
    let mut x = r.left() + 18.0;
    if cards {
        tool_chip(
            &p,
            Rect::from_min_size(Pos2::new(x, top + 1.0), Vec2::splat(26.0)),
            Icon::Clipboard,
            TINT,
        );
        x += 38.0;
    }
    let sw = if cards { 300.0 } else { 260.0 };
    let sr = Rect::from_min_size(Pos2::new(x, top), Vec2::new(sw, 30.0));
    let n = c.store.entries.len();
    let hint = if c.paused() {
        "Recording is paused".to_string()
    } else {
        format!("Type to search {n} items")
    };
    let before = q.query.clone();
    search_field(ui, sr, &mut q.query, &hint, true, 13.0);
    if q.query != before {
        q.sel = 0;
    }
    let old = q.filter;
    let chips_end = chips(
        ui,
        &p,
        sr.right() + 12.0,
        top + 2.0,
        &mut q.filter,
        "quick-filter",
    );
    if q.filter != old {
        q.sel = 0;
    }
    let ids = c.visible(q.filter, &q.query);
    if cards {
        let help = "← → move · ↵ paste · ⇧↵ plain text · ⌘P pin";
        text_fit(
            &p,
            Pos2::new(r.right() - 18.0, top + 15.0),
            Align2::RIGHT_CENTER,
            help,
            font(12.0),
            TEXT_FAINT,
            r.right() - 30.0 - chips_end,
        );
    }

    let body = Rect::from_min_max(Pos2::new(r.left(), top + 42.0), r.max);
    if ids.is_empty() {
        let msg = if c.store.entries.is_empty() {
            "Nothing copied yet"
        } else {
            "Nothing matches"
        };
        p.text(
            body.center() - Vec2::new(0.0, 10.0),
            Align2::CENTER_CENTER,
            msg,
            bold(15.0),
            TEXT_SOFT,
        );
        return act;
    }
    let chosen = if cards {
        shelf(c, q, ui, body, &ids)
    } else {
        list(c, q, ui, body, &ids)
    };
    act.or(chosen)
}

/// The card shelf: the selection stays in view.
fn shelf(c: &mut Clipboard, q: &mut Quick, ui: &mut Ui, body: Rect, ids: &[u64]) -> Option<Act> {
    let p = ui.painter().clone();
    let x0 = body.left() + 18.0;
    let fit = (((body.width() - 36.0) + CARD_GAP) / (CARD_W + CARD_GAP))
        .floor()
        .max(1.0) as usize;
    let first = q
        .sel
        .saturating_sub(fit.saturating_sub(2))
        .min(ids.len().saturating_sub(fit));
    let mut out = None;
    let y = body.top() + 16.0;
    for (slot, k) in (first..ids.len().min(first + fit)).enumerate() {
        let Some(e) = c.store.get(ids[k]).cloned() else {
            continue;
        };
        let sel = k == q.sel;
        let lift = ui
            .ctx()
            .animate_bool(ui.scope_id().with(("quick-lift", e.id)), sel)
            * 8.0;
        let r = Rect::from_min_size(
            Pos2::new(x0 + slot as f32 * (CARD_W + CARD_GAP), y - lift),
            Vec2::new(CARD_W, CARD_H + lift * 2.0),
        );
        let resp = ui.interact(r, ui.scope_id().with(("quick-card", e.id)), Sense::click());
        if resp.hovered() {
            ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
        }
        let key = if k < 9 {
            format!("⌘{}", k + 1)
        } else {
            String::new()
        };
        card(c, &p, r, &e, sel || resp.hovered(), sel, &key);
        if resp.clicked() {
            out = Some(Act::Paste(e.id, false));
        }
    }
    if ids.len() > first + fit {
        let more = ids.len() - first - fit;
        p.text(
            Pos2::new(body.right() - 18.0, body.bottom() - 16.0),
            Align2::RIGHT_CENTER,
            format!("{more} more →"),
            font(11.0),
            TEXT_FAINT,
        );
    }
    if first > 0 {
        p.text(
            Pos2::new(body.left() + 18.0, body.bottom() - 16.0),
            Align2::LEFT_CENTER,
            format!("← {first} more"),
            font(11.0),
            TEXT_FAINT,
        );
    }
    out
}

/// One big card: a color strip with the kind and app, the content, a footer.
fn card(
    c: &mut Clipboard,
    p: &egui::Painter,
    r: Rect,
    e: &Entry,
    hover: bool,
    sel: bool,
    key: &str,
) {
    let rad = 14.0;
    p.rect_filled(r, rad, if hover { RAISED } else { CARD });
    let tint = e.kind.tint();
    let head = Rect::from_min_size(r.min, Vec2::new(r.width(), 34.0));
    p.rect_filled(
        head,
        egui::CornerRadius {
            nw: rad as u8,
            ne: rad as u8,
            sw: 0,
            se: 0,
        },
        tint,
    );
    let chip = Rect::from_center_size(
        Pos2::new(head.left() + 20.0, head.center().y),
        Vec2::splat(16.0),
    );
    p.rect_filled(chip, 4.0, app_color(&e.app));
    p.rect_stroke(
        chip,
        4.0,
        Stroke::new(1.5, Color32::from_white_alpha(217)),
        StrokeKind::Inside,
    );
    p.text(
        Pos2::new(chip.right() + 7.0, head.center().y),
        Align2::LEFT_CENTER,
        e.kind.label(),
        bold(12.0),
        Color32::WHITE,
    );
    let tr = p.text(
        Pos2::new(head.right() - 12.0, head.center().y),
        Align2::RIGHT_CENTER,
        super::ui::ago(e),
        font(12.0),
        Color32::from_white_alpha(210),
    );
    if e.pinned {
        pin_glyph(
            p,
            Pos2::new(tr.left() - 12.0, head.center().y),
            Color32::WHITE,
        );
    }

    let foot_h = 30.0;
    let content = Rect::from_min_max(
        Pos2::new(r.left(), head.bottom()),
        Pos2::new(r.right(), r.bottom() - foot_h),
    );
    let clip = p.with_clip_rect(content);
    let inner = content.shrink2(Vec2::new(14.0, 12.0));
    let mut foot = if e.app.is_empty() {
        "Unknown app".to_string()
    } else {
        e.app.clone()
    };
    match e.kind {
        Kind::Text => {
            paragraph(
                &clip,
                inner.min,
                &e.text,
                font(13.0),
                Color32::from_rgb(0xE5, 0xE5, 0xEA),
                inner.width(),
            );
            foot = format!("{foot} · {} chars", e.text.chars().count());
        }
        Kind::Code => {
            let g = clip.layout(
                e.text.clone(),
                mono(11.5),
                Color32::from_rgb(0xD8, 0xF5, 0xE0),
                inner.width(),
            );
            clip.galley(inner.min, g, TEXT);
            foot = format!("{foot} · {} lines", e.text.lines().count().max(1));
        }
        Kind::Link => {
            let t = e.text.trim();
            let host = t
                .trim_start_matches("https://")
                .trim_start_matches("http://")
                .split('/')
                .next()
                .unwrap_or(t)
                .to_string();
            let band = Rect::from_min_size(content.min, Vec2::new(content.width(), 92.0));
            clip.rect_filled(band, 0.0, Color32::from_rgb(0x1D, 0x2A, 0x3D));
            let letter = host
                .trim_start_matches("www.")
                .chars()
                .next()
                .map(|c| c.to_ascii_uppercase().to_string())
                .unwrap_or_default();
            let lc = Rect::from_center_size(band.center(), Vec2::splat(46.0));
            clip.rect_filled(lc, 12.0, app_color(&host));
            clip.text(
                lc.center(),
                Align2::CENTER_CENTER,
                letter,
                bold(24.0),
                Color32::WHITE,
            );
            let path = t
                .trim_start_matches("https://")
                .trim_start_matches("http://")
                .trim_start_matches(&host)
                .trim_matches('/');
            text_fit(
                &clip,
                Pos2::new(inner.left(), band.bottom() + 10.0),
                Align2::LEFT_TOP,
                &host,
                bold(13.0),
                TEXT,
                inner.width(),
            );
            paragraph(
                &clip,
                Pos2::new(inner.left(), band.bottom() + 30.0),
                if path.is_empty() { "/" } else { path },
                font(12.0),
                ACCENT,
                inner.width(),
            );
        }
        Kind::Image => {
            let ir = Rect::from_min_size(content.min, Vec2::new(content.width(), 150.0));
            match c.thumb(e.id) {
                Some(t) => cover_image(&clip, ir, t, (e.w, e.h), 0.0),
                None => {
                    clip.rect_filled(ir, 0.0, with_alpha(tint, 0.3));
                    kind_glyph(
                        &clip,
                        Kind::Image,
                        Rect::from_center_size(ir.center(), Vec2::splat(40.0)),
                        TEXT_SOFT,
                        None,
                    );
                }
            }
            text_fit(
                &clip,
                Pos2::new(inner.left(), ir.bottom() + 10.0),
                Align2::LEFT_TOP,
                &format!("{}×{}", e.w, e.h),
                bold(13.0),
                TEXT,
                inner.width(),
            );
            foot = format!("{foot} · {}", human_bytes(e.bytes));
        }
        Kind::Color => {
            let col = parse_color(&e.text).unwrap_or(GRAY);
            let sw = Rect::from_min_size(content.min, Vec2::new(content.width(), 128.0));
            clip.rect_filled(sw, 0.0, col);
            let txt = if col.r() as u32 + col.g() as u32 + col.b() as u32 > 420 {
                Color32::from_rgb(20, 20, 22)
            } else {
                Color32::WHITE
            };
            text_fit(
                &clip,
                Pos2::new(sw.left() + 14.0, sw.bottom() - 12.0),
                Align2::LEFT_BOTTOM,
                e.text.trim(),
                mono(19.0),
                txt,
                sw.width() - 20.0,
            );
            let (h, s, l) = super::ui::hsl(col);
            clip.text(
                Pos2::new(inner.left(), sw.bottom() + 10.0),
                Align2::LEFT_TOP,
                format!("rgb({}, {}, {})", col.r(), col.g(), col.b()),
                mono(11.0),
                TEXT_DIM,
            );
            clip.text(
                Pos2::new(inner.left(), sw.bottom() + 27.0),
                Align2::LEFT_TOP,
                format!("hsl({h:.0}°, {s:.0}%, {l:.0}%)"),
                mono(11.0),
                TEXT_DIM,
            );
        }
        Kind::File => {
            let files = e.files();
            let name = files
                .first()
                .and_then(|f| f.file_name())
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default();
            let ext = files
                .first()
                .and_then(|f| f.extension())
                .map(|x| x.to_string_lossy().to_uppercase())
                .unwrap_or_else(|| "FILE".into());
            // a page with a folded corner and a band with the extension
            let pg = Rect::from_center_size(
                Pos2::new(content.center().x, content.top() + 70.0),
                Vec2::new(62.0, 76.0),
            );
            let fold = 18.0;
            let pts = vec![
                pg.left_top(),
                Pos2::new(pg.right() - fold, pg.top()),
                Pos2::new(pg.right(), pg.top() + fold),
                pg.right_bottom(),
                pg.left_bottom(),
            ];
            clip.add(egui::Shape::convex_polygon(pts, TEXT, Stroke::NONE));
            clip.add(egui::Shape::convex_polygon(
                vec![
                    Pos2::new(pg.right() - fold, pg.top()),
                    Pos2::new(pg.right(), pg.top() + fold),
                    Pos2::new(pg.right() - fold, pg.top() + fold),
                ],
                Color32::from_rgb(0xC7, 0xC7, 0xCC),
                Stroke::NONE,
            ));
            let band = Rect::from_min_size(
                Pos2::new(pg.left(), pg.top() + 42.0),
                Vec2::new(pg.width(), 18.0),
            );
            clip.rect_filled(
                band,
                0.0,
                if ext == "PDF" {
                    Color32::from_rgb(0xD7, 0x3A, 0x2F)
                } else {
                    TINT
                },
            );
            text_fit(
                &clip,
                band.center(),
                Align2::CENTER_CENTER,
                &ext,
                bold(10.0),
                Color32::WHITE,
                band.width() - 4.0,
            );
            text_fit(
                &clip,
                Pos2::new(content.center().x, pg.bottom() + 16.0),
                Align2::CENTER_TOP,
                &name,
                bold(13.0),
                TEXT,
                inner.width(),
            );
            if files.len() > 1 {
                clip.text(
                    Pos2::new(content.center().x, pg.bottom() + 36.0),
                    Align2::CENTER_TOP,
                    format!("and {} more", files.len() - 1),
                    font(12.0),
                    TEXT_FAINT,
                );
            }
        }
    }
    // footer
    let fy = r.bottom() - foot_h / 2.0;
    let kr = if key.is_empty() {
        Rect::from_min_size(Pos2::new(r.right() - 12.0, fy), Vec2::ZERO)
    } else {
        let g = p.layout_no_wrap(key.to_string(), font(11.0), TEXT);
        let kr = Rect::from_min_max(
            Pos2::new(r.right() - 12.0 - g.size().x - 12.0, fy - 9.0),
            Pos2::new(r.right() - 12.0, fy + 9.0),
        );
        p.rect_filled(kr, 5.0, if sel { BLUE } else { white(20) });
        p.text(
            kr.center(),
            Align2::CENTER_CENTER,
            key,
            font(11.0),
            if sel { Color32::WHITE } else { TEXT_DIM },
        );
        kr
    };
    text_fit(
        p,
        Pos2::new(r.left() + 14.0, fy),
        Align2::LEFT_CENTER,
        &foot,
        font(11.0),
        TEXT_FAINT,
        kr.left() - r.left() - 22.0,
    );
    if sel {
        p.rect_stroke(r, rad, Stroke::new(2.0, ACCENT), StrokeKind::Inside);
        p.rect_stroke(
            r.expand(3.0),
            rad + 3.0,
            Stroke::new(4.0, with_alpha(BLUE, 0.25)),
            StrokeKind::Outside,
        );
    } else {
        p.rect_stroke(r, rad, Stroke::new(1.0, white(20)), StrokeKind::Inside);
    }
}

/// The list layout: rows on the left, the selection's preview on the right.
fn list(c: &mut Clipboard, q: &mut Quick, ui: &mut Ui, body: Rect, ids: &[u64]) -> Option<Act> {
    let p = ui.painter().clone();
    let foot = Rect::from_min_max(Pos2::new(body.left(), body.bottom() - 38.0), body.max);
    hairline(
        &p,
        Pos2::new(body.left(), body.top()),
        Pos2::new(body.right(), body.top()),
    );
    let left = Rect::from_min_max(
        Pos2::new(body.left() + 8.0, body.top() + 8.0),
        Pos2::new(body.left() + 340.0, foot.top() - 4.0),
    );
    hairline(
        &p,
        Pos2::new(left.right() + 8.0, body.top()),
        Pos2::new(left.right() + 8.0, foot.top()),
    );
    let rows = ((left.height()) / 34.0).floor().max(1.0) as usize;
    let first = q
        .sel
        .saturating_sub(rows.saturating_sub(2))
        .min(ids.len().saturating_sub(rows));
    let mut out = None;
    for (slot, k) in (first..ids.len().min(first + rows)).enumerate() {
        let Some(e) = c.store.get(ids[k]).cloned() else {
            continue;
        };
        let r = Rect::from_min_size(
            Pos2::new(left.left(), left.top() + slot as f32 * 34.0),
            Vec2::new(left.width(), 33.0),
        );
        let resp = ui.interact(r, ui.scope_id().with(("quick-row", e.id)), Sense::click());
        let sel = k == q.sel;
        if sel {
            p.rect_filled(r, 7.0, BLUE);
        } else if resp.hovered() {
            p.rect_filled(r, 7.0, white(12));
        }
        let th = Rect::from_min_size(
            Pos2::new(r.left() + 8.0, r.center().y - 11.0),
            Vec2::splat(22.0),
        );
        entry_thumb(c, &p, th, &e, 5.0);
        let key = if k < 9 {
            format!("⌘{}", k + 1)
        } else {
            String::new()
        };
        let kr = p.text(
            Pos2::new(r.right() - 8.0, r.center().y),
            Align2::RIGHT_CENTER,
            key,
            font(11.0),
            if sel {
                Color32::from_white_alpha(217)
            } else {
                TEXT_FAINT
            },
        );
        let mut right = kr.left() - 8.0;
        if e.pinned {
            pin_glyph(
                &p,
                Pos2::new(right - 6.0, r.center().y),
                if sel { Color32::WHITE } else { TINT },
            );
            right -= 18.0;
        }
        let f = if e.kind == Kind::Code {
            mono(12.0)
        } else {
            font(12.0)
        };
        text_fit(
            &p,
            Pos2::new(th.right() + 9.0, r.center().y),
            Align2::LEFT_CENTER,
            &e.title(),
            f,
            TEXT,
            right - th.right() - 9.0,
        );
        if resp.clicked() {
            out = Some(Act::Paste(e.id, false));
        }
        if resp.hovered() {
            ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
        }
    }
    // preview
    if let Some(e) = ids.get(q.sel).and_then(|id| c.store.get(*id).cloned()) {
        let pr = Rect::from_min_max(
            Pos2::new(left.right() + 24.0, body.top() + 14.0),
            Pos2::new(body.right() - 16.0, foot.top() - 90.0),
        );
        preview(c, ui, &p, pr, &e, 10.0);
        let mut y = pr.bottom() + 12.0;
        let facts = [
            (
                "From",
                if e.app.is_empty() {
                    "Unknown app".to_string()
                } else {
                    e.app.clone()
                },
            ),
            (
                "Copied",
                format!(
                    "{}{}",
                    e.time().format("%b %-d, %-I:%M %P"),
                    if e.count > 1 {
                        format!(" · {} times", e.count)
                    } else {
                        String::new()
                    }
                ),
            ),
            ("Content", e.summary()),
        ];
        for (k, v) in facts {
            p.text(
                Pos2::new(pr.left(), y + 8.0),
                Align2::LEFT_CENTER,
                k,
                font(12.0),
                TEXT_FAINT,
            );
            text_fit(
                &p,
                Pos2::new(pr.left() + 70.0, y + 8.0),
                Align2::LEFT_CENTER,
                &v,
                font(12.0),
                TEXT,
                pr.width() - 70.0,
            );
            y += 21.0;
        }
    }
    // key hints
    p.rect_filled(
        foot,
        egui::CornerRadius {
            nw: 0,
            ne: 0,
            sw: 14,
            se: 14,
        },
        FOOTER,
    );
    hairline(&p, foot.left_top(), foot.right_top());
    let mut x = foot.left() + 14.0;
    for (k, l) in [
        ("↵", "Paste"),
        ("⇧↵", "Paste as plain text"),
        ("⌘P", "Pin"),
        ("⌘⌫", "Delete"),
        ("esc", "Close"),
    ] {
        let g = p.layout_no_wrap(k.to_string(), font(11.0), TEXT);
        let kr = Rect::from_min_size(
            Pos2::new(x, foot.center().y - 10.0),
            Vec2::new((g.size().x + 10.0).max(20.0), 20.0),
        );
        p.rect_filled(kr, 5.0, white(20));
        p.rect_stroke(kr, 5.0, Stroke::new(1.0, white(26)), StrokeKind::Inside);
        p.text(kr.center(), Align2::CENTER_CENTER, k, font(11.0), TEXT);
        let lr = p.text(
            Pos2::new(kr.right() + 6.0, foot.center().y),
            Align2::LEFT_CENTER,
            l,
            font(12.0),
            TEXT_DIM,
        );
        x = lr.right() + 16.0;
    }
    out
}

fn screen_size(ctx: &egui::Context) -> Vec2 {
    ctx.input(|i| i.viewport().monitor_size)
        .filter(|s| s.x > 100.0)
        .unwrap_or(Vec2::new(1512.0, 982.0))
}

/// Show the window while quick paste is open, and act on the choice.
pub fn show(c: &mut Clipboard, ctx: &egui::Context) {
    let Some(mut q) = c.quick.take() else { return };
    let screen = screen_size(ctx);
    let size = q.size().min(screen - Vec2::splat(40.0));
    let pos = match q.layout {
        Layout::Cards => Pos2::new(
            (screen.x - size.x) / 2.0,
            (screen.y - size.y - 90.0).max(40.0),
        ),
        Layout::List => Pos2::new(
            (screen.x - size.x) / 2.0,
            ((screen.y - size.y) * 0.35).max(40.0),
        ),
    };
    let title = format!("{}clipboard-quick", crate::kit::native::PLAIN_WINDOW);
    let builder = ViewportBuilder::default()
        .with_title(title)
        .with_decorations(false)
        .with_transparent(true)
        .with_resizable(false)
        .with_always_on_top()
        .with_taskbar(false)
        .with_has_shadow(true)
        .with_position(pos)
        .with_inner_size(size)
        .with_active(true);
    let mut act = None;
    let first = q.frames == 0;
    let shown = crate::kit::viewport::show(
        ctx,
        ViewportId::from_hash_of("clipboard-quick"),
        builder,
        |ui, _| {
            let vctx = ui.ctx().clone();
            if first {
                vctx.send_viewport_cmd(ViewportCommand::Focus);
                crate::kit::native::activate();
            }
            match vctx.input(|i| i.viewport().focused) {
                Some(true) => q.focused = true,
                Some(false) if q.focused => act = Some(Act::Close),
                _ => {}
            }
            if vctx.input(|i| i.viewport().close_requested()) {
                act = Some(Act::Close);
            }
            let r = ui.max_rect();
            if let Some(a) = draw(c, &mut q, ui, r) {
                act = Some(a);
            }
        },
    );
    if shown.is_some() {
        q.frames += 1;
    }
    match act {
        Some(Act::Close) => {
            if let Some(pid) = q.front {
                super::mac::activate_pid(pid);
            }
        }
        Some(Act::Paste(id, plain)) => c.paste(id, plain, q.front),
        Some(Act::Pin(id)) => {
            let on = c.store.get(id).is_some_and(|e| !e.pinned);
            c.store.set_pinned(id, on);
            c.quick = Some(q);
        }
        Some(Act::Delete(id)) => {
            c.store.remove(id);
            c.forget_dropped_thumbs();
            c.quick = Some(q);
        }
        None => c.quick = Some(q),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::tool::ToolEnv;

    fn run(c: &mut Clipboard, q: &mut Quick, events: Vec<egui::Event>) -> Option<Act> {
        let ctx = egui::Context::default();
        let mut out = None;
        let input = egui::RawInput {
            events,
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, CARDS_SIZE)),
            ..Default::default()
        };
        ctx.run_ui(input, |ui| {
            out = draw(c, q, ui, ui.max_rect());
        })
        .textures_delta
        .clear();
        out
    }

    fn key(k: Key, m: egui::Modifiers) -> egui::Event {
        egui::Event::Key {
            key: k,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: m,
        }
    }

    #[test]
    fn keyboard_picks_and_pastes() {
        let mut c = Clipboard::new(&ToolEnv {
            ctx: egui::Context::default(),
            demo: true,
        });
        let ids = c.visible(Filter::All, "");
        let mut q = Quick::new(Layout::Cards, None);
        assert_eq!(
            run(
                &mut c,
                &mut q,
                vec![key(Key::ArrowRight, egui::Modifiers::NONE)]
            ),
            None
        );
        assert_eq!(
            run(
                &mut c,
                &mut q,
                vec![key(Key::ArrowRight, egui::Modifiers::NONE)]
            ),
            None
        );
        assert_eq!(q.sel, 2);
        assert_eq!(
            run(&mut c, &mut q, vec![key(Key::Enter, egui::Modifiers::NONE)]),
            Some(Act::Paste(ids[2], false))
        );
        assert_eq!(
            run(
                &mut c,
                &mut q,
                vec![key(Key::Enter, egui::Modifiers::SHIFT)]
            ),
            Some(Act::Paste(ids[2], true))
        );
        assert_eq!(
            run(
                &mut c,
                &mut q,
                vec![key(Key::Num4, egui::Modifiers::COMMAND)]
            ),
            Some(Act::Paste(ids[3], false))
        );
        assert_eq!(
            run(&mut c, &mut q, vec![key(Key::P, egui::Modifiers::COMMAND)]),
            Some(Act::Pin(ids[2]))
        );
        assert_eq!(
            run(
                &mut c,
                &mut q,
                vec![key(Key::Escape, egui::Modifiers::NONE)]
            ),
            Some(Act::Close)
        );
        // the list layout moves with up and down
        let mut q = Quick::new(Layout::List, None);
        run(
            &mut c,
            &mut q,
            vec![key(Key::ArrowDown, egui::Modifiers::NONE)],
        );
        assert_eq!(q.sel, 1);
        run(
            &mut c,
            &mut q,
            vec![key(Key::ArrowUp, egui::Modifiers::NONE)],
        );
        run(
            &mut c,
            &mut q,
            vec![key(Key::ArrowUp, egui::Modifiers::NONE)],
        );
        assert_eq!(q.sel, 0, "stops at the top");
    }
}
