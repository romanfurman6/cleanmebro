//! Drawing for Dictation: the pill under the caret, the panel widget and
//! the window page (history and settings).

use super::settings::{Engine, SILENCE_CHOICES};
use super::{mac, Dictation, Note, Phase, Tab, ID, TINT};
use crate::core::tool::{Cx, Size};
use crate::kit::glass::*;
use crate::kit::hotkeys::Shortcut;
use crate::kit::perms::{Perm, Status};
use crate::kit::tile;
use egui::{Align2, Color32, Pos2, Rect, ScrollArea, Sense, Stroke, StrokeKind, Ui, UiBuilder, Vec2, ViewportBuilder, ViewportId};

const PILL_TITLE: &str = "Cmd+Meow Dictation";
const PILL_H: f32 = 34.0;
const PILL_W: f32 = 132.0;
const ROW_H: f32 = sp::SETTINGS_ROW_H;
/// History cards: inner padding, the meta line, the gap under it, the gap
/// between cards.
const PAD: f32 = 12.0;
const META_H: f32 = 14.0;
const GAP: f32 = 6.0;
const ROW_GAP: f32 = 8.0;
/// The red of a live mic.
const LIVE: Color32 = Color32::from_rgb(0xFF, 0x45, 0x3A);

fn clock(secs: f32) -> String {
    let s = secs.max(0.0) as u64;
    format!("{}:{:02}", s / 60, s % 60)
}

fn ago(at: i64) -> String {
    let s = (chrono::Local::now().timestamp() - at).max(0);
    if s < 60 {
        "now".into()
    } else if s < 3600 {
        format!("{} min", s / 60)
    } else if s < 86_400 {
        format!("{} h", s / 3600)
    } else {
        format!("{} d", s / 86_400)
    }
}

fn words(n: usize) -> String {
    format!("{n} word{}", if n == 1 { "" } else { "s" })
}

// ---------------------------------------------------------------- the pill

fn note_text(n: &Note) -> Option<String> {
    match n {
        Note::Pasted => None,
        Note::Copied => Some("Copied · ⌘V to paste".into()),
        Note::Problem(m) => Some(m.clone()),
    }
}

/// The pill under the caret while dictating, and its short notes after.
pub fn pill(d: &mut Dictation, ctx: &egui::Context) {
    if matches!(d.phase, Phase::Idle) {
        return;
    }
    let text = match &d.phase {
        Phase::Showing { note, .. } => note_text(note),
        _ => None,
    };
    let w = match &text {
        Some(t) => {
            // about 6.6 pt a character at 12 pt
            (t.chars().count() as f32 * 6.6 + 56.0).clamp(PILL_W, 360.0)
        }
        None => PILL_W,
    };
    let size = Vec2::new(w, PILL_H);
    let (caret, mouse, screen) = d.place;
    let pos = mac::pill_pos(caret, mouse, screen, size);
    let builder = ViewportBuilder::default()
        .with_title(PILL_TITLE)
        .with_decorations(false)
        .with_transparent(true)
        .with_resizable(false)
        .with_always_on_top()
        .with_taskbar(false)
        .with_has_shadow(false)
        .with_active(false)
        .with_mouse_passthrough(true)
        .with_position(pos)
        .with_inner_size(size);
    let t = ctx.input(|i| i.time);
    let levels = d.levels.clone();
    let phase_secs = match &d.phase {
        Phase::Listening { since, .. } => since.elapsed().as_secs_f32(),
        _ => 0.0,
    };
    crate::kit::viewport::show(ctx, ViewportId::from_hash_of(PILL_TITLE), builder, |ui, _| {
        mac::float(PILL_TITLE);
        let r = ui.max_rect().shrink(1.0);
        let p = ui.painter().clone();
        p.rect_filled(r, r.height() / 2.0, Color32::from_rgba_unmultiplied(24, 24, 26, 242));
        p.rect_stroke(r, r.height() / 2.0, Stroke::new(1.0, white(30)), StrokeKind::Inside);
        let cy = r.center().y;
        match &d.phase {
            Phase::Listening { .. } => {
                let pulse = 0.55 + 0.45 * (0.5 + 0.5 * (t * 5.0).sin()) as f32;
                p.circle_filled(Pos2::new(r.left() + 16.0, cy), 4.5, with_alpha(LIVE, pulse));
                let tr = p.text(Pos2::new(r.right() - 13.0, cy), Align2::RIGHT_CENTER, clock(phase_secs), mono(12.0), TEXT_SOFT());
                bars(&p, Rect::from_min_max(Pos2::new(r.left() + 28.0, r.top() + 8.0), Pos2::new(tr.left() - 10.0, r.bottom() - 8.0)), &levels, white(225));
            }
            Phase::Working { .. } => {
                spinner(&p, Pos2::new(r.left() + 17.0, cy), 6.5, t, TEXT_SOFT());
                bars(&p, Rect::from_min_max(Pos2::new(r.left() + 32.0, r.top() + 8.0), Pos2::new(r.right() - 14.0, r.bottom() - 8.0)), &levels, white(70));
            }
            Phase::Showing { note: Note::Pasted, .. } => {
                icon(&p, Icon::Check, Rect::from_center_size(r.center(), Vec2::splat(16.0)), GREEN());
            }
            Phase::Showing { note, .. } => {
                let (glyph, col) = match note {
                    Note::Copied => (Icon::Clipboard, TEXT_SOFT()),
                    _ => (Icon::Mic, AMBER()),
                };
                icon(&p, glyph, Rect::from_center_size(Pos2::new(r.left() + 18.0, cy), Vec2::splat(14.0)), col);
                if let Some(t) = &text {
                    text_fit(&p, Pos2::new(r.left() + 32.0, cy), Align2::LEFT_CENTER, t, font(12.0), TEXT(), r.width() - 44.0);
                }
            }
            Phase::Idle => {}
        }
    });
}

/// The voice as bars, newest on the right, mirrored around the middle.
fn bars(p: &egui::Painter, r: Rect, levels: &[f32], color: Color32) {
    let (bw, gap) = (2.5, 2.0);
    let n = ((r.width() + gap) / (bw + gap)) as usize;
    let start = levels.len().saturating_sub(n);
    let shown = &levels[start..];
    let x0 = r.right() - shown.len() as f32 * (bw + gap) + gap;
    for (k, v) in shown.iter().enumerate() {
        let h = (r.height() * v.clamp(0.0, 1.0)).max(2.0);
        let x = x0 + k as f32 * (bw + gap);
        p.rect_filled(Rect::from_center_size(Pos2::new(x + bw / 2.0, r.center().y), Vec2::new(bw, h)), 1.0, color);
    }
    // the quiet part not heard yet: dots
    let mut x = x0 - gap - bw;
    while x >= r.left() {
        p.circle_filled(Pos2::new(x + bw / 2.0, r.center().y), 1.0, with_alpha(color, 0.35));
        x -= bw + gap;
    }
}

// ---------------------------------------------------------------- widget

pub fn widget(d: &mut Dictation, size: Size, ui: &mut Ui, rect: Rect, cx: &mut Cx) {
    let id = ui.scope_id().with("w-dictation");
    let listening = d.listening();
    let key = d.shortcut_label();
    let today = words(d.words_today());
    let resp = match size {
        Size::S11 => {
            if listening {
                let resp = tile::card(ui, rect, id, false);
                let p = ui.painter();
                p.rect_stroke(rect, tile::R_TILE, Stroke::new(1.5, LIVE), StrokeKind::Inside);
                icon(p, Icon::Mic, Rect::from_min_size(rect.min + Vec2::splat(tile::PAD), Vec2::splat(18.0)), LIVE);
                tile::small_text(p, rect, "Listening", &key, TEXT_DIM());
                resp
            } else {
                tile::small(ui, rect, id, Icon::Mic, TINT(), "Dictation", &key, false)
            }
        }
        Size::S14 => {
            let (resp, right) = tile::row_split(ui, rect, id, Icon::Mic, if listening { LIVE } else { TINT() }, "Dictation", &today, &key, false);
            latest(d, ui, right, 2);
            resp
        }
        _ => {
            let resp = tile::card(ui, rect, id, false);
            let p = ui.painter().clone();
            let body = tile::head(&p, rect, Icon::Mic, TINT(), "Dictation", &format!("{key} · {today}"));
            latest(d, ui, body, 4);
            resp
        }
    };
    if resp.on_hover_text("Open Dictation").clicked() {
        cx.open_page(ID, "history");
    }
}

/// The newest dictations in `r`, one per line.
fn latest(d: &Dictation, ui: &mut Ui, r: Rect, n: usize) {
    let p = ui.painter();
    let items: Vec<_> = d.history.items.iter().take(n).collect();
    if items.is_empty() {
        text_fit(p, r.center(), Align2::CENTER_CENTER, &format!("Hold {} and speak", d.shortcut_label()), font(11.0), TEXT_FAINT(), r.width());
        return;
    }
    let lh = (r.height() / n as f32).min(26.0);
    let top = r.center().y - lh * items.len() as f32 / 2.0;
    for (k, s) in items.iter().enumerate() {
        let y = top + lh * (k as f32 + 0.5);
        let ag = p.text(Pos2::new(r.right(), y), Align2::RIGHT_CENTER, ago(s.at), font(11.0), TEXT_FAINT());
        text_fit(p, Pos2::new(r.left(), y), Align2::LEFT_CENTER, &s.text, font(12.0), TEXT_SOFT(), ag.left() - r.left() - 8.0);
    }
}

// ---------------------------------------------------------------- page

pub fn page(d: &mut Dictation, ui: &mut Ui, r: Rect, cx: &mut Cx) {
    let p = ui.painter().clone();
    let (sub, col) = if d.listening() {
        ("Listening".to_string(), LIVE)
    } else if !d.settings.engine.ready() {
        ("Speech model not downloaded".to_string(), AMBER())
    } else {
        (format!("{} · {} · {}", d.shortcut_label(), d.settings.engine.name(), words(d.words_today())), TEXT_DIM())
    };
    let btns = page_header(&p, r, "Dictation", &sub, col);
    let tr = Rect::from_min_max(Pos2::new(btns.right() - 180.0, btns.top()), btns.max);
    let cur = (d.tab == Tab::Settings) as usize;
    if let Some(i) = segmented(ui, tr, ui.scope_id().with("dict-tab"), &["History", "Settings"], cur) {
        d.tab = if i == 1 { Tab::Settings } else { Tab::History };
        d.recording_key = false;
    }
    let body = Rect::from_min_max(Pos2::new(r.left(), r.top() + PAGE_HEADER_H + 1.0), r.max);
    match d.tab {
        Tab::History => history(d, ui, body, cx),
        Tab::Settings => settings_page(d, ui, body, cx),
    }
}

fn history(d: &mut Dictation, ui: &mut Ui, body: Rect, cx: &mut Cx) {
    let x0 = body.left() + 24.0;
    let w = body.width() - 48.0;
    if d.history.items.is_empty() {
        let p = ui.painter();
        tile::empty_note(p, Rect::from_min_size(Pos2::new(x0, body.top() + 20.0), Vec2::new(w, 120.0)), &format!("Hold {} anywhere and speak", d.shortcut_label()));
        return;
    }
    let mut child = ui.new_child(UiBuilder::new().max_rect(body));
    let mut copy: Option<String> = None;
    let mut remove: Option<usize> = None;
    ScrollArea::vertical().id_salt("dict-history").auto_shrink([false, false]).show(&mut child, |ui| {
        let p = ui.painter().clone();
        let tw = w - 24.0 - 120.0;
        // each entry is as tall as its text
        // 12 pad · meta 14 · 6 gap · text · 12 pad, then the 8 gap between cards
        let heights: Vec<f32> = d.history.items.iter().map(|s| PAD + META_H + GAP + p.layout(s.text.clone(), font(13.0), TEXT(), tw).size().y.max(18.0) + PAD + ROW_GAP).collect();
        let total: f32 = heights.iter().sum::<f32>() + 40.0;
        let (area, _) = ui.allocate_exact_size(Vec2::new(body.width(), total), Sense::hover());
        let mut y = area.top() + 16.0;
        let id = ui.scope_id().with("dict-rows");
        for (k, s) in d.history.items.iter().enumerate() {
            let rr = Rect::from_min_size(Pos2::new(x0, y), Vec2::new(w, heights[k] - ROW_GAP));
            let resp = ui.interact(rr, id.with(k), Sense::click());
            let h = ui.ctx().animate_bool(id.with(("h", k)), resp.hovered());
            p.rect_filled(rr, R_CARD, lerp_color(CARD(), RAISED(), h * 0.5));
            let meta = format!("{} · {} · {:.0} s", ago(s.at), if s.app.is_empty() { "—" } else { &s.app }, s.secs.max(1.0));
            p.text(Pos2::new(rr.left() + PAD, rr.top() + PAD), Align2::LEFT_TOP, meta, font(11.0), TEXT_FAINT());
            paragraph(&p, Pos2::new(rr.left() + PAD, rr.top() + PAD + META_H + GAP), &s.text, font(13.0), TEXT(), tw);
            let cr = Rect::from_min_size(Pos2::new(rr.right() - PAD - 64.0, rr.center().y - 13.0), Vec2::new(64.0, 26.0));
            let xr = Rect::from_center_size(Pos2::new(cr.left() - 20.0, cr.center().y), Vec2::splat(22.0));
            if h > 0.01 && icon_button(ui, xr, Icon::Trash, "Remove").clicked() {
                remove = Some(k);
            }
            if pill_button(ui, cr, "Copy", BTN_SECONDARY, true).clicked() || resp.double_clicked() {
                copy = Some(s.text.clone());
            }
            y += heights[k];
        }
    });
    if let Some(t) = copy {
        ui.ctx().copy_text(t);
        cx.toast("Copied");
    }
    if let Some(k) = remove {
        d.history.items.remove(k);
        d.save_history();
    }
}

fn card_rows(p: &egui::Painter, x: f32, y: f32, w: f32, rows: usize) -> Rect {
    let r = Rect::from_min_size(Pos2::new(x, y), Vec2::new(w, rows as f32 * ROW_H + 8.0));
    glass_card(p, r, R_CARD, 0.0, false);
    for k in 1..rows {
        let ly = r.top() + 4.0 + k as f32 * ROW_H;
        p.line_segment([Pos2::new(r.left() + 12.0, ly), Pos2::new(r.right() - 12.0, ly)], Stroke::new(1.0, white(13)));
    }
    r
}

fn row_y(card: Rect, k: usize) -> f32 {
    card.top() + 4.0 + k as f32 * ROW_H + ROW_H / 2.0
}

fn label(p: &egui::Painter, card: Rect, k: usize, text: &str) {
    p.text(Pos2::new(card.left() + 12.0, row_y(card, k)), Align2::LEFT_CENTER, text, font(13.0), TEXT());
}

fn settings_page(d: &mut Dictation, ui: &mut Ui, body: Rect, cx: &mut Cx) {
    record_key(d, ui, cx);
    let mut child = ui.new_child(UiBuilder::new().max_rect(body));
    ScrollArea::vertical().id_salt("dict-settings").auto_shrink([false, false]).show(&mut child, |ui| {
        let gap = 20.0;
        let colw = (body.width() - 48.0 - gap) / 2.0;
        // the Speaking card, then Words, which grows with the list
        let left_h = 16.0 + 28.0 + 4.0 * ROW_H + 8.0 + 28.0 + 28.0 + words_card_h(ui.painter(), &d.settings.words, colw) + 24.0;
        let (area, _) = ui.allocate_exact_size(Vec2::new(body.width(), left_h.max(520.0)), Sense::hover());
        let p = ui.painter().clone();
        let x0 = area.left() + 24.0;
        let mut changed = left_col(d, ui, &p, Rect::from_min_size(Pos2::new(x0, area.top() + 16.0), Vec2::new(colw, area.height())));
        changed |= right_col(d, ui, &p, Rect::from_min_size(Pos2::new(x0 + colw + gap, area.top() + 16.0), Vec2::new(colw, area.height())));
        if changed {
            d.save_settings();
        }
    });
}

/// While recording: the next key press with a modifier becomes the shortcut.
fn record_key(d: &mut Dictation, ui: &mut Ui, cx: &mut Cx) {
    if !d.recording_key {
        return;
    }
    d.recording_seen = std::time::Instant::now();
    let mut result: Option<Option<Shortcut>> = None;
    let mut cancel = false;
    ui.input(|i| {
        for e in &i.events {
            if let egui::Event::Key { key, pressed: true, modifiers, .. } = e {
                let plain = !modifiers.command && !modifiers.ctrl && !modifiers.alt && !modifiers.mac_cmd;
                if *key == egui::Key::Escape && plain {
                    cancel = true;
                } else if (*key == egui::Key::Backspace || *key == egui::Key::Delete) && plain {
                    result = Some(None);
                } else if let Some(s) = Shortcut::from_press(*key, *modifiers) {
                    result = Some(Some(s));
                }
            }
        }
    });
    if cancel {
        d.recording_key = false;
        return;
    }
    if let Some(s) = result {
        if let Some(w) = s.as_ref().and_then(|s| s.conflict()) {
            cx.toast(format!("{}: {w}", s.as_ref().map(|s| s.display()).unwrap_or_default()));
        }
        d.settings.shortcut = s;
        d.save_settings();
        d.recording_key = false;
    }
    ui.ctx().request_repaint();
}

/// Shortcut, model, silence. Returns whether a setting changed.
fn left_col(d: &mut Dictation, ui: &mut Ui, p: &egui::Painter, col: Rect) -> bool {
    let id = ui.scope_id().with("dict-left");
    let mut changed = false;
    let x0 = col.left();
    p.text(Pos2::new(x0, col.top()), Align2::LEFT_TOP, "Speaking", bold(15.0), TEXT());
    let card = card_rows(p, x0, col.top() + 28.0, col.width(), 4);

    // shortcut
    label(p, card, 0, "Shortcut");
    let y = row_y(card, 0);
    let rec = d.recording_key;
    let cur = d.settings.shortcut.clone();
    let text = if rec { "Type a shortcut…".to_string() } else { cur.as_ref().map(|s| s.display()).unwrap_or_else(|| "Record".into()) };
    let g = p.layout_no_wrap(text.clone(), font(12.0), TEXT());
    let bw = (g.size().x + 20.0).max(76.0);
    let br = Rect::from_min_max(Pos2::new(card.right() - 12.0 - bw, y - 12.0), Pos2::new(card.right() - 12.0, y + 12.0));
    let resp = ui.interact(br, id.with("key"), Sense::click());
    if rec {
        p.rect_filled(br, 6.0, with_alpha(BLUE(), 0.22));
        p.rect_stroke(br, 6.0, Stroke::new(1.0, ACCENT()), StrokeKind::Inside);
    } else if cur.is_some() {
        p.rect_filled(br, 6.0, white(if resp.hovered() { 30 } else { 20 }));
        p.rect_stroke(br, 6.0, Stroke::new(1.0, white(26)), StrokeKind::Inside);
    } else {
        dashed_rect(p, br, 6.0, white(60));
    }
    p.text(br.center(), Align2::CENTER_CENTER, text, font(12.0), if cur.is_some() || rec { TEXT() } else { TEXT_FAINT() });
    if resp.on_hover_text("Click, then press the keys. Esc cancels, ⌫ removes it.").clicked() {
        d.recording_key = !rec;
        // counts as drawn now, so the next tick doesn't take it for abandoned
        d.recording_seen = std::time::Instant::now();
    }
    if let Some(e) = &d.key_error {
        text_fit(p, Pos2::new(br.left() - 10.0, y), Align2::RIGHT_CENTER, e, font(11.0), AMBER(), br.left() - card.left() - 110.0);
    }

    // model
    label(p, card, 1, "Model");
    let y = row_y(card, 1);
    let mr = Rect::from_min_max(Pos2::new(card.right() - 12.0 - 230.0, y - 13.0), Pos2::new(card.right() - 12.0, y + 13.0));
    let ci = Engine::ALL.iter().position(|e| *e == d.settings.engine).unwrap_or(0);
    let names: Vec<&str> = Engine::ALL.iter().map(|e| e.name()).collect();
    if let Some(i) = segmented(ui, mr, id.with("engine"), &names, ci) {
        d.settings.engine = Engine::ALL[i];
        changed = true;
    }

    // download
    label(p, card, 2, "Speech model");
    let y = row_y(card, 2);
    let engine = d.settings.engine;
    let right = card.right() - 12.0;
    match &d.download {
        Some(dl) if dl.engine == engine => {
            let unpacking = dl.unpacking.load(std::sync::atomic::Ordering::Relaxed);
            let frac = dl.progress.fraction();
            let sr = Rect::from_min_max(Pos2::new(right - 60.0, y - 12.0), Pos2::new(right, y + 12.0));
            if pill_button(ui, sr, "Stop", BTN_SECONDARY, true).clicked() {
                d.stop_download();
            }
            let what = if unpacking { "Unpacking…".to_string() } else { format!("{}%", (frac * 100.0) as u32) };
            let tr = p.text(Pos2::new(sr.left() - 10.0, y), Align2::RIGHT_CENTER, what, font(12.0), TEXT_DIM());
            bar(p, Rect::from_min_max(Pos2::new(tr.left() - 120.0, y - 2.0), Pos2::new(tr.left() - 10.0, y + 2.0)), if unpacking { 1.0 } else { frac }, BLUE());
        }
        _ if engine.ready() => {
            let rr = link(ui, Pos2::new(right, y), Align2::RIGHT_CENTER, "Remove", 12.0);
            p.text(Pos2::new(rr.rect.left() - 12.0, y), Align2::RIGHT_CENTER, "Ready", font(12.0), GREEN());
            let tip = match engine {
                Engine::Parakeet => format!("Delete the model ({})", human_bytes(engine.bytes())),
                Engine::Whisper => format!("Delete the model ({}); Capture's transcripts use it too", human_bytes(engine.bytes())),
            };
            if rr.on_hover_text(tip).clicked() {
                match engine {
                    Engine::Parakeet => crate::kit::parakeet::remove(),
                    Engine::Whisper => crate::kit::whisper::Model::Accurate.remove(),
                }
            }
        }
        _ => {
            let label = format!("Download {}", human_bytes(engine.bytes()));
            let bw = button_width(p, &label, BTN_PRIMARY);
            let dr = Rect::from_min_max(Pos2::new(right - bw, y - 12.0), Pos2::new(right, y + 12.0));
            if pill_button(ui, dr, &label, BTN_PRIMARY, d.download.is_none()).clicked() {
                d.start_download(engine);
            }
            if let Some(e) = &d.download_error {
                text_fit(p, Pos2::new(dr.left() - 10.0, y), Align2::RIGHT_CENTER, e, font(11.0), AMBER(), dr.left() - card.left() - 130.0);
            }
        }
    }

    // hands-free stop
    label(p, card, 3, "Stop after silence");
    let y = row_y(card, 3);
    let sr = Rect::from_min_max(Pos2::new(card.right() - 12.0 - 230.0, y - 13.0), Pos2::new(card.right() - 12.0, y + 13.0));
    let si = SILENCE_CHOICES.iter().position(|s| *s == d.settings.silence_secs).unwrap_or(2);
    let names: Vec<String> = SILENCE_CHOICES.iter().map(|s| if *s == 0 { "Never".to_string() } else { format!("{s} s") }).collect();
    let names: Vec<&str> = names.iter().map(String::as_str).collect();
    if let Some(i) = segmented(ui, sr, id.with("silence"), &names, si) {
        d.settings.silence_secs = SILENCE_CHOICES[i];
        changed = true;
    }

    changed |= words_card(d, ui, p, x0, card.bottom() + 28.0, col.width());
    changed
}

const CHIP_H: f32 = 24.0;
const CHIP_GAP: f32 = 6.0;
/// Room in a chip for its ×.
const CHIP_X: f32 = 20.0;

/// Where each word's chip goes in a `w`-wide area: wrapped into rows.
fn chip_rects(p: &egui::Painter, words: &[String], w: f32) -> Vec<Rect> {
    let (mut x, mut y) = (0.0f32, 0.0f32);
    words
        .iter()
        .map(|t| {
            let cw = (p.layout_no_wrap(t.clone(), font(12.0), TEXT()).size().x + 10.0 + CHIP_X).min(w);
            if x > 0.0 && x + cw > w {
                x = 0.0;
                y += CHIP_H + CHIP_GAP;
            }
            let r = Rect::from_min_size(Pos2::new(x, y), Vec2::new(cw, CHIP_H));
            x += cw + CHIP_GAP;
            r
        })
        .collect()
}

fn words_card_h(p: &egui::Painter, words: &[String], colw: f32) -> f32 {
    let chips = chip_rects(p, words, colw - 24.0);
    ROW_H + 8.0 + chips.last().map(|r| r.bottom() + 12.0).unwrap_or(0.0)
}

/// The person's own words: a field to add one, the list as chips with ×.
/// Returns whether the list changed.
fn words_card(d: &mut Dictation, ui: &mut Ui, p: &egui::Painter, x0: f32, top: f32, w: f32) -> bool {
    let id = ui.scope_id().with("dict-words");
    let mut changed = false;
    let tr = p.text(Pos2::new(x0, top), Align2::LEFT_TOP, "Words", bold(15.0), TEXT());
    // what it's for, beside the title
    text_fit(p, Pos2::new(tr.right() + 10.0, tr.center().y + 1.0), Align2::LEFT_CENTER, "Names and terms to spell your way: \"клод\" or \"Clode\" becomes \"Claude\"", font(12.0), TEXT_FAINT(), x0 + w - tr.right() - 10.0);
    let card = Rect::from_min_size(Pos2::new(x0, top + 28.0), Vec2::new(w, words_card_h(p, &d.settings.words, w)));
    glass_card(p, card, R_CARD, 0.0, false);

    let y = card.top() + 4.0 + ROW_H / 2.0;
    let ar = Rect::from_min_max(Pos2::new(card.right() - 12.0 - 56.0, y - 12.0), Pos2::new(card.right() - 12.0, y + 12.0));
    let fr = Rect::from_min_max(Pos2::new(card.left() + 12.0, y - 13.0), Pos2::new(ar.left() - 8.0, y + 13.0));
    p.rect_filled(fr, 6.0, white(15));
    let tr = fr.shrink2(Vec2::new(8.0, 0.0));
    let mut child = ui.new_child(UiBuilder::new().max_rect(tr).layout(egui::Layout::left_to_right(egui::Align::Center)));
    let te = egui::TextEdit::singleline(&mut d.word_draft).hint_text(egui::RichText::new("Name, brand or term").color(TEXT_FAINT())).frame(egui::Frame::NONE).font(font(12.0)).text_color(TEXT()).desired_width(tr.width()).id(id.with("field"));
    let resp = child.add(te);
    let entered = resp.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
    let can_add = !d.word_draft.trim().is_empty();
    if (pill_button(ui, ar, "Add", BTN_SECONDARY, can_add).clicked() || entered) && d.settings.add_word(&d.word_draft) {
        d.word_draft.clear();
        changed = true;
        if entered {
            resp.request_focus();
        }
    }

    let origin = Pos2::new(card.left() + 12.0, card.top() + 4.0 + ROW_H + 4.0);
    let mut remove = None;
    for (k, (word, r)) in d.settings.words.iter().zip(chip_rects(p, &d.settings.words, w - 24.0)).enumerate() {
        let r = r.translate(origin.to_vec2());
        let xr = Rect::from_center_size(Pos2::new(r.right() - CHIP_X / 2.0 - 2.0, r.center().y), Vec2::splat(16.0));
        let x = ui.interact(xr, id.with(("x", k)), Sense::click());
        p.rect_filled(r, CHIP_H / 2.0, white(if x.hovered() { 26 } else { 18 }));
        text_fit(p, Pos2::new(r.left() + 10.0, r.center().y), Align2::LEFT_CENTER, word, font(12.0), TEXT(), r.width() - 10.0 - CHIP_X);
        p.text(xr.center(), Align2::CENTER_CENTER, "×", font(13.0), if x.hovered() { TEXT() } else { TEXT_FAINT() });
        if x.on_hover_text("Remove").clicked() {
            remove = Some(k);
        }
    }
    if let Some(k) = remove {
        d.settings.words.remove(k);
        changed = true;
    }
    changed
}

/// Clipboard, history, permissions. Returns whether a setting changed.
fn right_col(d: &mut Dictation, ui: &mut Ui, p: &egui::Painter, col: Rect) -> bool {
    let id = ui.scope_id().with("dict-right");
    let mut changed = false;
    let x0 = col.left();
    p.text(Pos2::new(x0, col.top()), Align2::LEFT_TOP, "After", bold(15.0), TEXT());
    let card = card_rows(p, x0, col.top() + 28.0, col.width(), 2);
    let rows: [(&str, &mut bool); 2] = [("Put the clipboard back", &mut d.settings.restore_clipboard), ("Keep history", &mut d.settings.keep_history)];
    for (k, (l, on)) in rows.into_iter().enumerate() {
        label(p, card, k, l);
        let y = row_y(card, k);
        let sr = Rect::from_min_size(Pos2::new(card.right() - 12.0 - 40.0, y - 12.0), Vec2::new(40.0, 24.0));
        if switch(ui, sr, id.with(("sw", k)), *on, true).clicked() {
            *on = !*on;
            changed = true;
        }
    }
    if !d.history.items.is_empty() {
        let y = row_y(card, 1);
        let lr = link(ui, Pos2::new(card.right() - 12.0 - 40.0 - 14.0, y), Align2::RIGHT_CENTER, "Clear", 12.0);
        if lr.clicked() {
            d.history.items.clear();
            d.save_history();
        }
    }

    let y0 = card.bottom() + 28.0;
    p.text(Pos2::new(x0, y0), Align2::LEFT_TOP, "Permissions", bold(15.0), TEXT());
    let card = card_rows(p, x0, y0 + 28.0, col.width(), 2);
    for (k, perm) in [Perm::Microphone, Perm::Accessibility].into_iter().enumerate() {
        let y = row_y(card, k);
        icon(p, perm.icon(), Rect::from_center_size(Pos2::new(card.left() + 21.0, y), Vec2::splat(15.0)), TEXT_DIM());
        p.text(Pos2::new(card.left() + 38.0, y), Align2::LEFT_CENTER, perm.name(), font(13.0), TEXT());
        let right = card.right() - 12.0;
        if perm.status() == Status::Allowed {
            p.text(Pos2::new(right, y), Align2::RIGHT_CENTER, "Allowed", font(12.0), GREEN());
        } else {
            let label = perm.ask_label();
            let bw = button_width(p, label, BTN_PRIMARY);
            let br = Rect::from_min_max(Pos2::new(right - bw, y - 12.0), Pos2::new(right, y + 12.0));
            if pill_button(ui, br, label, BTN_PRIMARY, true).clicked() {
                perm.ask();
            }
            ui.ctx().request_repaint_after(std::time::Duration::from_secs(1));
        }
    }
    changed
}
