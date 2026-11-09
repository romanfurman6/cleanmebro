//! Capture in the shell: the quick-action tile and card in the panel, and the
//! History and Settings page in the full window.

use super::icons::{self, Ico};
use super::settings::{After, Command, Format, Shortcut, DELAYS, KEEP_MAX, KEEP_MIN};
use super::folder::{self, FileShot};
use super::store::{Kind, Shot};
use super::{mac, record, Capture, Tab, ID};
use crate::core::tool::{Cx, Size};
use crate::kit::tile;
use crate::kit::glass::*;
use egui::{Align2, Color32, Pos2, Rect, ScrollArea, Sense, Stroke, StrokeKind, Ui, UiBuilder, Vec2};
use std::path::PathBuf;
use std::time::Instant;

const ROW_H: f32 = 36.0;
const HOVER_BTN: Color32 = Color32::from_rgb(0x2C, 0x2C, 0x31);

pub fn command_icon(c: Command) -> Ico {
    match c {
        Command::Area => Ico::Area,
        Command::Window => Ico::Window,
        Command::Screen => Ico::Screen,
        Command::Delayed => Ico::Timer,
        Command::Text => Ico::ScanText,
        Command::Color => Ico::Eyedropper,
        Command::Ruler => Ico::Ruler,
        Command::History => Ico::History,
        Command::Record => Ico::Record,
    }
}

fn key_label(c: &Capture, cmd: Command) -> String {
    c.settings.shortcut(cmd).map(|s| s.display()).unwrap_or_default()
}

// ---------------------------------------------------------------- panel

const MODES: [(Command, &str); 5] = [(Command::Area, "Area"), (Command::Window, "Window"), (Command::Screen, "Screen"), (Command::Text, "Text"), (Command::Record, "Record")];

/// The Capture widget at `size`.
pub fn widget(c: &mut Capture, size: Size, ui: &mut Ui, rect: Rect, cx: &mut Cx) {
    let id = ui.scope_id().with("w-capture");
    match size {
        Size::S11 => {
            let sub = if c.busy { "Capturing…".to_string() } else { key_label(c, Command::Area) };
            let sub = if sub.is_empty() { "Area".to_string() } else { sub };
            if tile::small(ui, rect, id, Icon::Crop, tint::CAPTURE, "Capture", &sub, false).on_hover_text("Capture an area").clicked() {
                c.queue.push((Command::Area, c.settings.delay));
            }
        }
        Size::S12 | Size::S14 => {
            let _ = tile::card(ui, rect, id, false);
            let p = ui.painter().clone();
            let mut x0 = rect.left() + 8.0;
            if size == Size::S12 {
                tool_chip(&p, Rect::from_min_size(Pos2::new(x0 + 2.0, rect.top() + 9.0), Vec2::splat(18.0)), Icon::Crop, tint::CAPTURE);
                p.text(Pos2::new(x0 + 26.0, rect.top() + 18.0), Align2::LEFT_CENTER, "Capture", font(11.0), TEXT_DIM);
                let r = Rect::from_min_max(Pos2::new(x0, rect.top() + 32.0), Pos2::new(rect.right() - 8.0, rect.bottom() - 8.0));
                modes(c, ui, &p, id, r, &MODES[..2], false);
            } else {
                let chip = Rect::from_min_size(Pos2::new(rect.left() + 12.0, rect.center().y - 17.0), Vec2::splat(34.0));
                tool_chip(&p, chip, Icon::Crop, tint::CAPTURE);
                x0 = chip.right() + 10.0;
                modes(c, ui, &p, id, Rect::from_min_max(Pos2::new(x0, rect.top() + 9.0), Pos2::new(rect.right() - 9.0, rect.bottom() - 9.0)), &MODES, true);
            }
        }
        _ => card(c, ui, rect, cx),
    }
}

/// Capture buttons side by side in `r`: icon, name and (`keys`) shortcut.
fn modes(c: &mut Capture, ui: &mut Ui, p: &egui::Painter, id: egui::Id, r: Rect, list: &[(Command, &str)], keys: bool) {
    let gap = 6.0;
    let n = list.len() as f32;
    let bw = (r.width() - (n - 1.0) * gap) / n;
    let tall = r.height() >= 48.0;
    let live = c.rec.tray();
    for (k, (cmd, label)) in list.iter().enumerate() {
        // while recording, Record is the stop button with the time
        let (label, fg) = match (&live, cmd) {
            (Some(t), Command::Record) => (t.trim_start_matches(['●', '❚', ' ']).to_string(), RED),
            _ => (label.to_string(), TEXT),
        };
        let label = label.as_str();
        let b = Rect::from_min_size(Pos2::new(r.left() + k as f32 * (bw + gap), r.top()), Vec2::new(bw, r.height()));
        let resp = ui.interact(b, id.with(("mode", k)), Sense::click());
        let h = ui.ctx().animate_bool(id.with(("mode-h", k)), resp.hovered());
        p.rect_filled(b, R_BUTTON, lerp_color(HOVER_BTN, Color32::from_rgb(0x36, 0x36, 0x3C), h));
        p.rect_stroke(b, R_BUTTON, Stroke::new(1.0, white(20)), StrokeKind::Inside);
        if tall {
            let top = b.top() + if keys { 15.0 } else { 18.0 };
            icons::draw(p, command_icon(*cmd), Rect::from_center_size(Pos2::new(b.center().x, top), Vec2::splat(17.0)), fg);
            p.text(Pos2::new(b.center().x, top + 17.0), Align2::CENTER_CENTER, label, font(11.0), fg);
            if keys {
                p.text(Pos2::new(b.center().x, top + 29.0), Align2::CENTER_CENTER, key_label(c, *cmd), font(10.0), TEXT_FAINT);
            }
        } else {
            let g = p.layout_no_wrap(label.to_string(), font(11.0), fg);
            let w = 15.0 + 5.0 + g.size().x;
            let x = b.center().x - w / 2.0;
            icons::draw(p, command_icon(*cmd), Rect::from_center_size(Pos2::new(x + 7.5, b.center().y), Vec2::splat(14.0)), fg);
            p.galley(Pos2::new(x + 20.0, b.center().y - g.size().y / 2.0), g, fg);
        }
        if resp.hovered() {
            ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
        }
        let tip = key_label(c, *cmd);
        let hover = if live.is_some() && *cmd == Command::Record { "Stop and save the recording".to_string() } else if tip.is_empty() { label.to_string() } else { format!("{label} · {tip}") };
        if resp.on_hover_text(hover).clicked() {
            let delay = if matches!(*cmd, Command::Text | Command::Record) { 0 } else { c.settings.delay };
            c.queue.push((*cmd, delay));
        }
    }
}

/// The Medium widget: the delay, four capture buttons and the latest captures.
pub fn card(c: &mut Capture, ui: &mut Ui, rect: Rect, cx: &mut Cx) {
    let p = ui.painter().clone();
    let id = ui.scope_id().with("capture-card");
    tile::paint_card(&p, rect, 0.0, false);
    let x0 = rect.left() + 12.0;
    let x1 = rect.right() - 12.0;
    let mut y = rect.top() + 10.0;
    tool_chip(&p, Rect::from_min_size(Pos2::new(x0, y), Vec2::splat(20.0)), Icon::Crop, tint::CAPTURE);
    p.text(Pos2::new(x0 + 28.0, y + 10.0), Align2::LEFT_CENTER, "Capture", bold(13.0), TEXT);
    let hl = link(ui, Pos2::new(x1, y + 10.0), Align2::RIGHT_CENTER, "History", 12.0);
    let h_left = hl.rect.left();
    if hl.clicked() {
        c.tab = Tab::History;
        cx.open_page(ID, "history");
    }
    // the delay, in the header
    let sr = Rect::from_min_size(Pos2::new(h_left - 12.0 - 150.0, y - 1.0), Vec2::new(150.0, 22.0));
    let cur = DELAYS.iter().position(|d| *d == c.settings.delay).unwrap_or(0);
    if let Some(i) = segmented(ui, sr, id.with("delay"), &["Off", "3 s", "5 s", "10 s"], cur) {
        c.settings.delay = DELAYS[i];
        c.save_settings();
    }
    y += 30.0;

    let gap = 6.0;
    let bw = (x1 - x0 - 3.0 * gap) / 4.0;
    modes(c, ui, &p, id, Rect::from_min_max(Pos2::new(x0, y), Pos2::new(x1, y + 52.0)), &MODES, true);
    y += 52.0 + 10.0;

    // the newest images in the screenshot folder; history when it has none
    c.refresh_files();
    let items: Vec<Item> = if c.files.is_empty() { c.store.shots.iter().take(4).map(Item::from_shot).collect() } else { c.files.iter().take(4).map(|f| Item::from_file(c, f)).collect() };
    let label = if c.files.is_empty() { "RECENT".to_string() } else { format!("RECENT IN {}", c.folder_name().to_uppercase()) };
    let ch = link(ui, Pos2::new(x1, y + 6.0), Align2::RIGHT_CENTER, "Change folder…", 11.0);
    let ch_left = ch.rect.left();
    if ch.on_hover_text(format!("Screenshots go to {}", c.shown_folder())).clicked() {
        c.choose_folder();
    }
    let lr = ui.interact(Rect::from_min_size(Pos2::new(x0, y), Vec2::new(ch_left - x0 - 8.0, 13.0)), id.with("recent-label"), Sense::click());
    text_fit(&p, Pos2::new(x0, y), Align2::LEFT_TOP, &label, bold(11.0), if lr.hovered() { TEXT_SOFT } else { TEXT_FAINT }, lr.rect.width());
    if lr.on_hover_text("Open the folder view").clicked() {
        c.tab = if c.files.is_empty() { Tab::History } else { Tab::Folder };
        cx.open_page(ID, if c.files.is_empty() { "history" } else { "folder" });
    }
    y += 18.0;
    if items.is_empty() {
        let r = Rect::from_min_max(Pos2::new(x0, y), Pos2::new(x1, y + 48.0));
        dashed_rect(&p, r, 6.0, white(40));
        let hint = match c.settings.shortcut(Command::Area) {
            Some(s) => format!("No captures yet. Press {} to capture an area.", s.display()),
            None => "No captures yet.".into(),
        };
        text_fit(&p, r.center(), Align2::CENTER_CENTER, &hint, font(12.0), TEXT_FAINT, r.width() - 16.0);
        return;
    }
    for (k, it) in items.iter().enumerate() {
        let r = Rect::from_min_size(Pos2::new(x0 + k as f32 * (bw + gap), y), Vec2::new(bw, 48.0));
        let resp = ui.interact(r, id.with(("recent", it.key)), Sense::click());
        let tex = c.thumb_at(it.key, it.path.clone());
        thumb(&p, r, tex, (it.w, it.h), 6.0);
        p.rect_stroke(r, 6.0, Stroke::new(if resp.hovered() { 1.5 } else { 1.0 }, if resp.hovered() { ACCENT } else { white(26) }), StrokeKind::Inside);
        if resp.hovered() {
            ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
        }
        if resp.on_hover_text(format!("{} · click to edit", it.title)).clicked() {
            run_act(c, Act::Edit, it);
        }
    }
}

/// A capture's thumbnail filling `r` (cropped to fit, top aligned), or a quiet
/// plate while it loads.
pub fn thumb(p: &egui::Painter, r: Rect, tex: Option<egui::TextureId>, size: (u32, u32), radius: f32) {
    let Some(t) = tex else {
        p.rect_filled(r, radius, white(12));
        return;
    };
    let (w, h) = (size.0.max(1) as f32, size.1.max(1) as f32);
    let img_aspect = w / h;
    let box_aspect = r.width() / r.height().max(1.0);
    let uv = if img_aspect > box_aspect {
        let f = box_aspect / img_aspect;
        Rect::from_min_max(Pos2::new((1.0 - f) / 2.0, 0.0), Pos2::new((1.0 + f) / 2.0, 1.0))
    } else {
        let f = img_aspect / box_aspect;
        Rect::from_min_max(Pos2::new(0.0, 0.0), Pos2::new(1.0, f))
    };
    p.add(egui::epaint::RectShape::filled(r, radius, Color32::WHITE).with_texture(t, uv));
}

// ---------------------------------------------------------------- page

pub fn page(c: &mut Capture, ui: &mut Ui, r: Rect, cx: &mut Cx) {
    let p = ui.painter().clone();
    let sub = match c.tab {
        Tab::History => {
            let n = c.store.shots.len();
            format!("{n} capture{} · {} · saving to {}", if n == 1 { "" } else { "s" }, human_bytes(c.store.bytes_on_disk()), c.shown_folder())
        }
        Tab::Folder => {
            c.refresh_files();
            let n = c.files.len();
            let more = if n >= super::folder::LIMIT { " (newest)" } else { "" };
            format!("{n} image{}{more} in {}", if n == 1 { "" } else { "s" }, c.shown_folder())
        }
        Tab::Settings => "Shortcuts work everywhere, even when the panel is closed.".into(),
    };
    let btns = page_header(&p, r, "Capture", &sub, TEXT_DIM);
    let mut right = btns.right();
    if c.tab != Tab::Settings {
        let label = "Capture area";
        let w = button_width(&p, label, BTN_PRIMARY) + 18.0;
        let br = Rect::from_min_max(Pos2::new(right - w, btns.top() - 1.0), Pos2::new(right, btns.bottom() + 1.0));
        if pill_button(ui, br, "", BTN_PRIMARY, !c.busy).clicked() {
            c.queue.push((Command::Area, 0));
        }
        icons::draw(&p, Ico::Area, Rect::from_center_size(Pos2::new(br.left() + 18.0, br.center().y), Vec2::splat(15.0)), Color32::WHITE);
        p.text(Pos2::new(br.left() + 31.0, br.center().y), Align2::LEFT_CENTER, label, bold(13.0), Color32::WHITE);
        right = br.left() - 12.0;
    }
    let tabs = [Tab::History, Tab::Folder, Tab::Settings];
    let tr = Rect::from_min_max(Pos2::new(right - 246.0, btns.top()), Pos2::new(right, btns.bottom()));
    let cur = tabs.iter().position(|t| *t == c.tab).unwrap_or(0);
    if let Some(i) = segmented(ui, tr, ui.scope_id().with("capture-tab"), &["History", "Folder", "Settings"], cur) {
        c.tab = tabs[i];
        stop_recording(c);
    }
    let body = Rect::from_min_max(Pos2::new(r.left(), r.top() + PAGE_HEADER_H + 1.0), r.max);
    match c.tab {
        Tab::History => history(c, ui, body),
        Tab::Folder => folder_tab(c, ui, body),
        Tab::Settings => settings_page(c, ui, body, cx),
    }
}

const FILTERS: [&str; 6] = ["All", "Area", "Window", "Screen", "Pinned", "Videos"];

fn passes(filter: usize, s: &Shot) -> bool {
    match filter {
        1 => s.kind == Kind::Area || s.kind == Kind::Edited,
        2 => s.kind == Kind::Window,
        3 => s.kind == Kind::Screen,
        4 => s.pinned,
        5 => s.kind == Kind::Recording,
        _ => true,
    }
}

fn day_label(d: chrono::NaiveDate) -> String {
    let today = chrono::Local::now().date_naive();
    if d == today {
        "TODAY".into()
    } else if Some(d) == today.pred_opt() {
        "YESTERDAY".into()
    } else {
        d.format("%A, %b %-d").to_string().to_uppercase()
    }
}

/// One picture in a grid: a history capture or an image in the folder.
#[derive(Clone)]
pub struct Item {
    key: u64,
    path: PathBuf,
    time: chrono::DateTime<chrono::Local>,
    w: u32,
    h: u32,
    title: String,
    meta: String,
    pinned: bool,
    src: Src,
    /// A recording: the video file (the thumbnail comes from `path` or history).
    video: Option<PathBuf>,
}

#[derive(Clone, Debug, PartialEq)]
enum Src {
    Shot(u64),
    File,
}

impl Item {
    fn from_shot(s: &Shot) -> Item {
        let meta = match s.kind {
            Kind::Window | Kind::Screen | Kind::Edited => s.kind.label().to_string(),
            Kind::Area => format!("{}×{}", s.w, s.h),
            Kind::Recording => format!("Video · {}", record::timeline::clock_label(s.secs.unwrap_or(0.0) as f64)),
        };
        let what = if s.kind == Kind::Recording { "Recording" } else { "Capture" };
        let video = (s.kind == Kind::Recording).then(|| s.saved.clone()).flatten();
        // history files live in the app's data folder, next to index.json
        Item { key: s.id, path: PathBuf::new(), time: s.time(), w: s.w, h: s.h, title: format!("{what} from {}", s.time().format("%-I:%M %P")), meta, pinned: s.pinned, src: Src::Shot(s.id), video }
    }
    /// An image in the folder, or a video (its picture comes from the
    /// history entry it was saved from, when there is one).
    fn from_file(c: &Capture, f: &FileShot) -> Item {
        if folder::is_video(&f.path) {
            let shot = c.store.video_entry(&f.path);
            let (path, w, h) = match shot {
                Some(s) => (c.store.path(s.id), s.w, s.h),
                None => (PathBuf::new(), 16, 9),
            };
            let meta = shot.and_then(|s| s.secs).map(|t| format!("Video · {}", record::timeline::clock_label(t as f64))).unwrap_or_else(|| "Video".into());
            return Item { key: f.key, path, time: f.time(), w, h, title: f.name(), meta, pinned: false, src: Src::File, video: Some(f.path.clone()) };
        }
        Item { key: f.key, path: f.path.clone(), time: f.time(), w: f.w, h: f.h, title: f.name(), meta: if f.w > 0 { format!("{}×{}", f.w, f.h) } else { String::new() }, pinned: false, src: Src::File, video: None }
    }
}

/// What a tile's buttons (or a click) ask for.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Act {
    Copy,
    Edit,
    Pin,
    Reveal,
    Delete,
}

fn load(c: &Capture, it: &Item) -> Option<image::RgbaImage> {
    match it.src {
        Src::Shot(id) => c.store.load(id),
        Src::File => image::open(&it.path).ok().map(|i| i.to_rgba8()),
    }
}

fn run_act(c: &mut Capture, act: Act, it: &Item) {
    if let Some(v) = &it.video {
        match act {
            Act::Edit => record::sys::open_file(v),
            Act::Copy => c.hud(if record::sys::copy_file(v) { "Copied the video" } else { "Couldn't copy" }),
            Act::Reveal => mac::reveal(v),
            Act::Pin => c.hud("Videos can't be pinned"),
            Act::Delete => {}
        }
        if act != Act::Delete {
            return;
        }
    }
    match (act, &it.src) {
        // a history capture is a PNG already: onto the clipboard as it is
        (Act::Copy, Src::Shot(id)) => match c.store.png(*id) {
            Some(png) => {
                c.copy_png(png);
                c.hud("Copied");
            }
            None => c.hud("Couldn't copy"),
        },
        (Act::Copy, Src::File) => match load(c, it) {
            Some(img) => {
                c.copy_image(img);
                c.hud("Copied");
            }
            None => c.hud("Couldn't copy"),
        },
        (Act::Edit, Src::Shot(id)) => c.open_editor_for(*id),
        (Act::Edit, Src::File) => {
            if let Some(f) = c.files.iter().find(|f| f.key == it.key).cloned() {
                c.open_file(&f);
            }
        }
        (Act::Pin, Src::Shot(id)) => c.pin_shot(*id),
        (Act::Pin, Src::File) => match load(c, it) {
            Some(img) => c.pin(None, img),
            None => c.hud("Couldn't open it"),
        },
        (Act::Reveal, Src::Shot(id)) => {
            let id = *id;
            let saved = c.store.get(id).and_then(|s| s.saved.clone()).filter(|p| p.exists());
            let path = match saved {
                Some(p) => Some(p),
                None => c.store.load(id).and_then(|img| c.save_image(Some(id), &img)),
            };
            if let Some(p) = path {
                mac::reveal(&p);
            }
        }
        (Act::Reveal, Src::File) => mac::reveal(&it.path),
        (Act::Delete, Src::Shot(id)) => {
            c.store.remove(*id);
            c.forget_dropped_thumbs();
        }
        (Act::Delete, Src::File) => match crate::kit::native::trash_item(&it.path) {
            Ok(()) => {
                c.files.retain(|f| f.key != it.key);
                c.forget_dropped_thumbs();
                c.hud("Moved to Trash");
            }
            Err(e) => c.hud(format!("Couldn't move it to the Trash: {e}")),
        },
    }
}

/// Pictures grouped by day, five to a row, with buttons on hover.
fn grid(c: &mut Capture, ui: &mut Ui, list: Rect, items: &[Item], salt: &str) -> Vec<(Act, Item)> {
    let mut days: Vec<(chrono::NaiveDate, Vec<&Item>)> = Vec::new();
    for it in items {
        let d = it.time.date_naive();
        match days.last_mut() {
            Some((day, v)) if *day == d => v.push(it),
            _ => days.push((d, vec![it])),
        }
    }
    let x0 = list.left() + 24.0;
    let cols = 5usize;
    let gap = 12.0;
    let cw = (list.width() - 48.0 - gap * (cols as f32 - 1.0)) / cols as f32;
    let th = 112.0;
    let cell_h = th + 26.0;
    let mut acts = Vec::new();
    let mut child = ui.new_child(UiBuilder::new().max_rect(list));
    ScrollArea::vertical().id_salt(salt).auto_shrink([false, false]).show(&mut child, |ui| {
        let total: f32 = days.iter().map(|(_, v)| 24.0 + v.len().div_ceil(cols) as f32 * (cell_h + gap)).sum::<f32>() + 8.0;
        let (area, _) = ui.allocate_exact_size(Vec2::new(list.width(), total), Sense::hover());
        let p = ui.painter().clone();
        let mut y = area.top();
        for (d, v) in &days {
            section_header(&p, Pos2::new(x0, y), Align2::LEFT_TOP, &day_label(*d));
            y += 24.0;
            for (i, it) in v.iter().enumerate() {
                let (row, col) = (i / cols, i % cols);
                let r = Rect::from_min_size(Pos2::new(x0 + col as f32 * (cw + gap), y + row as f32 * (cell_h + gap)), Vec2::new(cw, th));
                if !ui.is_rect_visible(r.expand(8.0)) {
                    continue;
                }
                let tex = match it.src {
                    Src::Shot(id) => c.thumb(id),
                    Src::File if it.path.as_os_str().is_empty() => None,
                    Src::File => c.thumb_at(it.key, it.path.clone()),
                };
                if let Some(a) = tile_ui(ui, &p, r, it, tex) {
                    acts.push((a, (*it).clone()));
                }
            }
            y += v.len().div_ceil(cols) as f32 * (cell_h + gap);
        }
    });
    acts
}

fn tile_ui(ui: &mut Ui, p: &egui::Painter, r: Rect, it: &Item, tex: Option<egui::TextureId>) -> Option<Act> {
    let id = ui.scope_id().with(("pic", it.key));
    let resp = ui.interact(r, id, Sense::click()).on_hover_text(&it.title);
    let hovered = ui.rect_contains_pointer(r);
    thumb(p, r, tex, (it.w, it.h), 8.0);
    p.rect_stroke(r, 8.0, if hovered { Stroke::new(2.0, ACCENT) } else { Stroke::new(1.0, white(26)) }, StrokeKind::Inside);
    if it.video.is_some() {
        p.circle_filled(r.center(), 18.0, Color32::from_black_alpha(170));
        icons::draw(p, Ico::Play, Rect::from_center_size(r.center() + Vec2::new(1.5, 0.0), Vec2::splat(16.0)), Color32::WHITE);
    }
    if it.pinned {
        let b = Rect::from_min_size(r.min + Vec2::new(6.0, 6.0), Vec2::splat(20.0));
        p.circle_filled(b.center(), 10.0, Color32::from_black_alpha(160));
        icons::draw(p, Ico::Pin, b.shrink(4.0), Color32::WHITE);
    }
    let ly = r.bottom() + 12.0;
    let mr = p.text(Pos2::new(r.right(), ly), Align2::RIGHT_CENTER, &it.meta, font(12.0), TEXT_FAINT);
    text_fit(p, Pos2::new(r.left(), ly), Align2::LEFT_CENTER, &it.time.format("%-I:%M %P").to_string(), font(12.0), TEXT, mr.left() - r.left() - 6.0);
    let mut out = None;
    if hovered {
        let bar = Rect::from_min_max(Pos2::new(r.left() + 1.0, r.bottom() - 38.0), Pos2::new(r.right() - 1.0, r.bottom() - 1.0));
        p.rect_filled(bar, egui::CornerRadius { nw: 0, ne: 0, sw: 7, se: 7 }, Color32::from_rgba_unmultiplied(20, 20, 22, 225));
        let delete_tip = if it.src == Src::File { "Move to Trash" } else { "Delete" };
        let acts: [(Ico, &str, Act); 5] = [
            (Ico::Copy, "Copy", Act::Copy),
            (if it.video.is_some() { Ico::Play } else { Ico::Edit }, if it.video.is_some() { "Play" } else { "Edit" }, Act::Edit),
            (Ico::Pin, "Pin on screen", Act::Pin),
            (Ico::Folder, "Show in Finder", Act::Reveal),
            (Ico::Trash, delete_tip, Act::Delete),
        ];
        let bw = 26.0;
        let total = acts.len() as f32 * bw + (acts.len() - 1) as f32 * 4.0;
        let mut x = bar.center().x - total / 2.0;
        for (ico, tip, a) in acts {
            let br = Rect::from_min_size(Pos2::new(x, bar.center().y - 13.0), Vec2::splat(bw));
            let b = ui.interact(br, id.with(tip), Sense::click());
            p.rect_filled(br, 6.0, white(if b.hovered() { 46 } else { 26 }));
            icons::draw(p, ico, br.shrink(6.0), if a == Act::Delete && b.hovered() { RED } else { TEXT });
            if b.on_hover_text(tip).clicked() {
                out = Some(a);
            }
            x += bw + 4.0;
        }
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    if out.is_none() && resp.clicked() {
        out = Some(Act::Edit);
    }
    out
}

fn history(c: &mut Capture, ui: &mut Ui, body: Rect) {
    let p = ui.painter().clone();
    let x0 = body.left() + 24.0;
    let x1 = body.right() - 24.0;
    let fy = body.top() + 12.0;
    let mut x = x0;
    for (i, f) in FILTERS.iter().enumerate() {
        let g = p.layout_no_wrap(f.to_string(), font(12.0), TEXT);
        let r = Rect::from_min_size(Pos2::new(x, fy), Vec2::new(g.size().x + 24.0, 26.0));
        let resp = ui.interact(r, ui.scope_id().with(("filter", i)), Sense::click());
        let on = c.filter == i;
        p.rect_filled(r, 13.0, if on { with_alpha(BLUE, 0.22) } else if resp.hovered() { white(10) } else { Color32::TRANSPARENT });
        p.rect_stroke(r, 13.0, Stroke::new(1.0, if on { with_alpha(ACCENT, 0.45) } else { white(26) }), StrokeKind::Inside);
        p.text(r.center(), Align2::CENTER_CENTER, *f, font(12.0), if on { TEXT } else { TEXT_DIM });
        if resp.clicked() {
            c.filter = i;
        }
        x = r.right() + 6.0;
    }
    let lr = link(ui, Pos2::new(x1, fy + 13.0), Align2::RIGHT_CENTER, "change", 12.0);
    if lr.clicked() {
        c.tab = Tab::Settings;
    }
    p.text(Pos2::new(lr.rect.left(), fy + 13.0), Align2::RIGHT_CENTER, format!("Keeps the last {} · ", c.settings.keep), font(12.0), TEXT_FAINT);

    let list = Rect::from_min_max(Pos2::new(body.left(), fy + 38.0), body.max);
    let items: Vec<Item> = c.store.shots.iter().filter(|s| passes(c.filter, s)).map(Item::from_shot).collect();
    if items.is_empty() {
        let msg = if c.store.shots.is_empty() { "No captures yet" } else { "Nothing here with this filter" };
        p.text(list.center() - Vec2::new(0.0, 30.0), Align2::CENTER_CENTER, msg, bold(15.0), TEXT_SOFT);
        if c.store.shots.is_empty() {
            let hint = match c.settings.shortcut(Command::Area) {
                Some(s) => format!("Press {} anywhere to capture an area. Captures land here.", s.display()),
                None => "Captures land here.".into(),
            };
            p.text(list.center() - Vec2::new(0.0, 6.0), Align2::CENTER_CENTER, hint, font(13.0), TEXT_DIM);
        }
        return;
    }
    for (a, it) in grid(c, ui, list, &items, "capture-history") {
        run_act(c, a, &it);
    }
}

/// The screenshot folder: every PNG and JPG in it, newest first.
fn folder_tab(c: &mut Capture, ui: &mut Ui, body: Rect) {
    let p = ui.painter().clone();
    let x0 = body.left() + 24.0;
    let x1 = body.right() - 24.0;
    let fy = body.top() + 12.0;
    let cy = fy + 13.0;
    icons::draw(&p, Ico::Folder, Rect::from_center_size(Pos2::new(x0 + 8.0, cy), Vec2::splat(16.0)), TEXT_DIM);
    let ow = button_width(&p, "Open in Finder", BTN_SECONDARY) - 6.0;
    let or = Rect::from_min_max(Pos2::new(x1 - ow, cy - 13.0), Pos2::new(x1, cy + 13.0));
    if pill_button(ui, or, "Open in Finder", BTN_SECONDARY, true).clicked() {
        let _ = std::process::Command::new("open").arg(&c.settings.folder).spawn();
    }
    let cw = button_width(&p, "Change folder…", BTN_SECONDARY) - 6.0;
    let cr = Rect::from_min_max(Pos2::new(or.left() - 8.0 - cw, cy - 13.0), Pos2::new(or.left() - 8.0, cy + 13.0));
    if pill_button(ui, cr, "Change folder…", BTN_SECONDARY, true).clicked() {
        c.choose_folder();
    }
    text_fit(&p, Pos2::new(x0 + 24.0, cy), Align2::LEFT_CENTER, &c.shown_folder(), mono(12.0), TEXT_SOFT, cr.left() - x0 - 36.0);

    let list = Rect::from_min_max(Pos2::new(body.left(), fy + 38.0), body.max);
    let items: Vec<Item> = c.files.iter().map(|f| Item::from_file(c, f)).collect();
    if items.is_empty() {
        let (msg, hint) = if c.files_busy && c.files_at.is_none() {
            ("Looking in the folder…", String::new())
        } else if !c.settings.save {
            ("No images in this folder", "Turn on \"Save to folder\" in Settings to put every capture here.".to_string())
        } else {
            ("No images in this folder", "Captures you take are saved here as PNG or JPG.".to_string())
        };
        p.text(list.center() - Vec2::new(0.0, 30.0), Align2::CENTER_CENTER, msg, bold(15.0), TEXT_SOFT);
        p.text(list.center() - Vec2::new(0.0, 6.0), Align2::CENTER_CENTER, hint, font(13.0), TEXT_DIM);
        return;
    }
    for (a, it) in grid(c, ui, list, &items, "capture-folder") {
        run_act(c, a, &it);
    }
    ui.ctx().request_repaint_after(std::time::Duration::from_secs(3));
}

// ---------------------------------------------------------------- settings

fn stop_recording(c: &mut Capture) {
    if c.recording.take().is_some() {
        c.apply_keys();
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

/// Screen recording: frame rate, quality, cursor, clicks and the countdown.
fn recording_options(c: &mut Capture, ui: &mut Ui, p: &egui::Painter, col: Rect) {
    use super::settings::{RecQuality, RECORD_FPS};
    let x0 = col.left();
    p.text(Pos2::new(x0, col.top()), Align2::LEFT_TOP, "Screen recording", bold(15.0), TEXT);
    let card = card_rows(p, x0, col.top() + 28.0, col.width(), 5);
    let id = ui.scope_id().with("rec-options");
    let mut s = c.settings.rec.clone();
    let seg = |ui: &mut Ui, k: usize, w: f32, labels: &[&str], cur: usize| {
        let y = row_y(card, k);
        segmented(ui, Rect::from_min_max(Pos2::new(card.right() - 12.0 - w, y - 12.0), Pos2::new(card.right() - 12.0, y + 12.0)), id.with(k), labels, cur)
    };
    let labels = ["Frame rate", "Quality", "Countdown", "Show the cursor", "Highlight clicks"];
    for (k, l) in labels.iter().enumerate() {
        p.text(Pos2::new(card.left() + 14.0, row_y(card, k)), Align2::LEFT_CENTER, *l, font(13.0), TEXT);
    }
    if let Some(i) = seg(ui, 0, 110.0, &["30 fps", "60 fps"], if s.fps == 30 { 0 } else { 1 }) {
        s.fps = RECORD_FPS[i];
    }
    if let Some(i) = seg(ui, 1, 180.0, &["Up to 1080p", "Up to 4K"], if s.quality == RecQuality::Smaller { 0 } else { 1 }) {
        s.quality = if i == 0 { RecQuality::Smaller } else { RecQuality::High };
    }
    let cd = match s.countdown {
        0 => 0,
        5 => 2,
        _ => 1,
    };
    if let Some(i) = seg(ui, 2, 150.0, &["Off", "3 s", "5 s"], cd) {
        s.countdown = [0, 3, 5][i];
    }
    for (k, on) in [(3usize, s.cursor), (4, s.clicks)] {
        let y = row_y(card, k);
        if switch(ui, Rect::from_min_size(Pos2::new(card.right() - 12.0 - 30.0, y - 9.0), Vec2::new(30.0, 18.0)), id.with(("sw", k)), on, true).clicked() {
            if k == 3 {
                s.cursor = !s.cursor;
            } else {
                s.clicks = !s.clicks;
            }
        }
    }
    p.text(Pos2::new(x0, card.bottom() + 10.0), Align2::LEFT_TOP, "Recordings are saved as mp4 to the same folder as captures.", font(12.0), TEXT_FAINT);
    if s != c.settings.rec {
        c.settings.rec = s;
        c.save_settings();
    }
}

fn row_y(card: Rect, k: usize) -> f32 {
    card.top() + 4.0 + k as f32 * ROW_H + ROW_H / 2.0
}

fn settings_page(c: &mut Capture, ui: &mut Ui, body: Rect, cx: &mut Cx) {
    record_keys(c, ui, cx);
    let mut child = ui.new_child(UiBuilder::new().max_rect(body));
    ScrollArea::vertical().id_salt("capture-settings").auto_shrink([false, false]).show(&mut child, |ui| {
        let (area, _) = ui.allocate_exact_size(Vec2::new(body.width(), 680.0), Sense::hover());
        let p = ui.painter().clone();
        let gap = 20.0;
        let x0 = area.left() + 24.0;
        let colw = (area.width() - 48.0 - gap) / 2.0;
        shortcuts(c, ui, &p, Rect::from_min_size(Pos2::new(x0, area.top() + 16.0), Vec2::new(colw, area.height())));
        let ry = area.top() + 16.0 + 28.0 + Command::ALL.len() as f32 * ROW_H + 8.0 + 24.0;
        recording_options(c, ui, &p, Rect::from_min_size(Pos2::new(x0, ry), Vec2::new(colw, area.bottom() - ry)));
        options(c, ui, &p, Rect::from_min_size(Pos2::new(x0 + colw + gap, area.top() + 16.0), Vec2::new(colw, area.height())));
    });
}

/// While recording: the next key press with a modifier becomes the shortcut.
fn record_keys(c: &mut Capture, ui: &mut Ui, cx: &mut Cx) {
    let Some(cmd) = c.recording else { return };
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
        stop_recording(c);
        return;
    }
    if let Some(s) = result {
        if let Some(w) = s.as_ref().and_then(|s| s.conflict()) {
            cx.toast(format!("{}: {w}", s.as_ref().map(|s| s.display()).unwrap_or_default()));
        }
        c.settings.set_shortcut(cmd, s);
        c.save_settings();
        stop_recording(c);
    }
    ui.ctx().request_repaint();
}

fn shortcuts(c: &mut Capture, ui: &mut Ui, p: &egui::Painter, col: Rect) {
    let x0 = col.left();
    p.text(Pos2::new(x0, col.top()), Align2::LEFT_TOP, "Keyboard shortcuts", bold(15.0), TEXT);
    let card = card_rows(p, x0, col.top() + 28.0, col.width(), Command::ALL.len());
    for (k, cmd) in Command::ALL.iter().enumerate() {
        let y = row_y(card, k);
        icons::draw(p, command_icon(*cmd), Rect::from_center_size(Pos2::new(card.left() + 21.0, y), Vec2::splat(16.0)), TEXT_DIM);
        let lr = p.text(Pos2::new(card.left() + 38.0, y), Align2::LEFT_CENTER, cmd.label(), font(13.0), TEXT);
        let rec = c.recording == Some(*cmd);
        let cur = c.settings.shortcut(*cmd).cloned();
        let label = if rec { "Type a shortcut…".to_string() } else { cur.as_ref().map(|s| s.display()).unwrap_or_else(|| "Record".into()) };
        let g = p.layout_no_wrap(label.clone(), font(12.0), TEXT);
        let bw = (g.size().x + 20.0).max(76.0);
        let clear = Rect::from_center_size(Pos2::new(card.right() - 22.0, y), Vec2::splat(20.0));
        let br = Rect::from_min_max(Pos2::new(clear.left() - 6.0 - bw, y - 12.0), Pos2::new(clear.left() - 6.0, y + 12.0));
        let resp = ui.interact(br, ui.scope_id().with(("key", k)), Sense::click());
        if rec {
            p.rect_filled(br, 6.0, with_alpha(BLUE, 0.22));
            p.rect_stroke(br, 6.0, Stroke::new(1.0, ACCENT), StrokeKind::Inside);
        } else if cur.is_some() {
            p.rect_filled(br, 6.0, white(if resp.hovered() { 30 } else { 20 }));
            p.rect_stroke(br, 6.0, Stroke::new(1.0, white(26)), StrokeKind::Inside);
        } else {
            dashed_rect(p, br, 6.0, white(60));
        }
        p.text(br.center(), Align2::CENTER_CENTER, label, font(12.0), if cur.is_some() || rec { TEXT } else { TEXT_FAINT });
        if resp.on_hover_text("Click, then press the keys. Esc cancels, ⌫ removes it.").clicked() {
            if rec {
                stop_recording(c);
            } else {
                c.recording = Some(*cmd);
                if let Some(keys) = &mut c.keys {
                    keys.clear();
                }
            }
        }
        if cur.is_some() && !rec {
            let cr = ui.interact(clear, ui.scope_id().with(("clear", k)), Sense::click());
            if cr.hovered() {
                p.circle_filled(clear.center(), 10.0, white(26));
            }
            icons::draw(p, Ico::Close, clear.shrink(5.0), TEXT_FAINT);
            if cr.on_hover_text("Remove shortcut").clicked() {
                c.settings.set_shortcut(*cmd, None);
                c.save_settings();
                c.apply_keys();
            }
        }
        // why it may not work
        let warn = c
            .key_errors
            .iter()
            .find(|(e, _)| e == cmd)
            .map(|(_, m)| m.clone())
            .or_else(|| cur.as_ref().and_then(|s| s.conflict()).map(|m| m.to_string()));
        if let Some(w) = warn {
            let wx = lr.right() + 10.0;
            if br.left() - 10.0 - wx > 40.0 {
                text_fit(p, Pos2::new(wx, y), Align2::LEFT_CENTER, &w, font(11.0), AMBER, br.left() - 10.0 - wx);
            }
        }
    }
    let ny = card.bottom() + 10.0;
    icons::draw(p, Ico::Keyboard, Rect::from_min_size(Pos2::new(x0 + 2.0, ny), Vec2::splat(14.0)), TEXT_FAINT);
    paragraph(
        p,
        Pos2::new(x0 + 22.0, ny),
        "⌘⇧3, ⌘⇧4 and ⌘⇧5 belong to macOS. Pick one of them and you get a warning; turn the macOS one off in System Settings › Keyboard first.",
        font(12.0),
        TEXT_DIM,
        col.width() - 24.0,
    );
}

fn options(c: &mut Capture, ui: &mut Ui, p: &egui::Painter, col: Rect) {
    let x0 = col.left();
    let w = col.width();
    let id = ui.scope_id().with("capture-options");
    let mut changed = false;
    let mut y = col.top();

    // after a capture
    p.text(Pos2::new(x0, y), Align2::LEFT_TOP, "After a capture", bold(15.0), TEXT);
    let card = card_rows(p, x0, y + 28.0, w, 5);
    let ry = row_y(card, 0);
    p.text(Pos2::new(card.left() + 12.0, ry), Align2::LEFT_CENTER, "Then", font(13.0), TEXT);
    let sr = Rect::from_min_max(Pos2::new(card.right() - 12.0 - 270.0, ry - 13.0), Pos2::new(card.right() - 12.0, ry + 13.0));
    let cur = match c.settings.after {
        After::Preview => 0,
        After::Editor => 1,
        After::Nothing => 2,
    };
    if let Some(i) = segmented(ui, sr, id.with("after"), &["Show preview", "Open editor", "Nothing"], cur) {
        c.settings.after = [After::Preview, After::Editor, After::Nothing][i];
        changed = true;
    }
    let hide_hint = c.settings.preview_secs.map(|s| format!("{s} s")).unwrap_or_default();
    let switches: [(&str, &str, bool); 4] = [
        ("Copy to clipboard", "", c.settings.copy),
        ("Save to folder", "", c.settings.save),
        ("Hide preview after", &hide_hint, c.settings.preview_secs.is_some()),
        ("Play sound", "", c.settings.sound),
    ];
    for (k, (label, hint, on)) in switches.iter().enumerate() {
        let ry = row_y(card, k + 1);
        p.text(Pos2::new(card.left() + 12.0, ry), Align2::LEFT_CENTER, *label, font(13.0), TEXT);
        let sw = Rect::from_center_size(Pos2::new(card.right() - 27.0, ry), Vec2::new(30.0, 18.0));
        if !hint.is_empty() {
            p.text(Pos2::new(sw.left() - 10.0, ry), Align2::RIGHT_CENTER, *hint, font(12.0), TEXT_FAINT);
        }
        if switch(ui, sw, id.with(("sw", k)), *on, true).clicked() {
            match k {
                0 => c.settings.copy = !c.settings.copy,
                1 => c.settings.save = !c.settings.save,
                2 => c.settings.preview_secs = if c.settings.preview_secs.is_some() { None } else { Some(6) },
                _ => c.settings.sound = !c.settings.sound,
            }
            changed = true;
        }
    }
    y = card.bottom() + 16.0;

    // saving
    p.text(Pos2::new(x0, y), Align2::LEFT_TOP, "Saving", bold(15.0), TEXT);
    let card = card_rows(p, x0, y + 28.0, w, 4);
    let ry = row_y(card, 0);
    p.text(Pos2::new(card.left() + 12.0, ry), Align2::LEFT_CENTER, "Folder", font(13.0), TEXT);
    let bw = button_width(p, "Change…", BTN_SECONDARY) - 6.0;
    let br = Rect::from_min_max(Pos2::new(card.right() - 12.0 - bw, ry - 12.0), Pos2::new(card.right() - 12.0, ry + 12.0));
    if pill_button(ui, br, "Change…", BTN_SECONDARY, true).clicked() {
        c.choose_folder();
    }
    text_fit(p, Pos2::new(br.left() - 10.0, ry), Align2::RIGHT_CENTER, &c.shown_folder(), mono(12.0), TEXT_DIM, br.left() - card.left() - 90.0);
    let ry = row_y(card, 1);
    p.text(Pos2::new(card.left() + 12.0, ry), Align2::LEFT_CENTER, "Format", font(13.0), TEXT);
    let fr = Rect::from_min_max(Pos2::new(card.right() - 12.0 - 110.0, ry - 13.0), Pos2::new(card.right() - 12.0, ry + 13.0));
    if let Some(i) = segmented(ui, fr, id.with("format"), &["PNG", "JPG"], (c.settings.format == Format::Jpg) as usize) {
        c.settings.format = if i == 1 { Format::Jpg } else { Format::Png };
        changed = true;
    }
    let ry = row_y(card, 2);
    p.text(Pos2::new(card.left() + 12.0, ry), Align2::LEFT_CENTER, "File name", font(13.0), TEXT);
    let example = c.settings.file_name(chrono::Local::now());
    text_fit(p, Pos2::new(card.right() - 12.0, ry), Align2::RIGHT_CENTER, &example, mono(12.0), TEXT_DIM, card.width() - 110.0);
    let ry = row_y(card, 3);
    p.text(Pos2::new(card.left() + 12.0, ry), Align2::LEFT_CENTER, "History keeps", font(13.0), TEXT);
    // − N +   then Clear history
    let confirming = c.confirm_clear.is_some_and(|t| t.elapsed().as_secs_f32() < 3.0);
    let clear_label = if confirming { "Click again to clear" } else { "Clear history" };
    let cw = button_width(p, clear_label, BTN_DANGER) - 6.0;
    let cr = Rect::from_min_max(Pos2::new(card.right() - 12.0 - cw, ry - 12.0), Pos2::new(card.right() - 12.0, ry + 12.0));
    if pill_button(ui, cr, clear_label, if confirming { BTN_DANGER } else { BTN_SECONDARY }, !c.store.shots.is_empty()).clicked() {
        if confirming {
            c.store.clear();
            c.forget_dropped_thumbs();
            c.confirm_clear = None;
            c.hud("History cleared");
        } else {
            c.confirm_clear = Some(Instant::now());
        }
    }
    if confirming {
        ui.ctx().request_repaint_after(std::time::Duration::from_millis(250));
    }
    let plus = Rect::from_center_size(Pos2::new(cr.left() - 24.0, ry), Vec2::new(24.0, 24.0));
    let num = Rect::from_center_size(Pos2::new(plus.left() - 20.0, ry), Vec2::new(40.0, 24.0));
    let minus = Rect::from_center_size(Pos2::new(num.left() - 12.0, ry), Vec2::new(24.0, 24.0));
    for (r, s, delta) in [(minus, "−", -10i64), (plus, "+", 10)] {
        let resp = ui.interact(r, id.with(("step", s)), Sense::click());
        p.rect_filled(r, 6.0, white(if resp.hovered() { 34 } else { 20 }));
        p.text(r.center(), Align2::CENTER_CENTER, s, bold(14.0), TEXT);
        if resp.clicked() {
            c.settings.keep = (c.settings.keep as i64 + delta).clamp(KEEP_MIN as i64, KEEP_MAX as i64) as usize;
            c.store.trim(c.settings.keep);
            changed = true;
        }
    }
    p.text(num.center(), Align2::CENTER_CENTER, c.settings.keep.to_string(), bold(13.0), TEXT);
    y = card.bottom() + 16.0;

    // permission
    p.text(Pos2::new(x0, y), Align2::LEFT_TOP, "Permission", bold(15.0), TEXT);
    let card = Rect::from_min_size(Pos2::new(x0, y + 28.0), Vec2::new(w, 56.0));
    glass_card(p, card, R_CARD, 0.0, false);
    let ok = mac::can_record_screen();
    p.circle_filled(Pos2::new(card.left() + 18.0, card.top() + 19.0), 4.0, if ok { GREEN } else { AMBER });
    p.text(Pos2::new(card.left() + 30.0, card.top() + 19.0), Align2::LEFT_CENTER, "Screen Recording", font(13.0), TEXT);
    let status = if ok { "Allowed. Needed to see other apps' windows." } else { "Not allowed yet: captures show only the desktop." };
    let bw = button_width(p, "Open System Settings", BTN_SECONDARY) - 6.0;
    let br = Rect::from_min_max(Pos2::new(card.right() - 12.0 - bw, card.center().y - 13.0), Pos2::new(card.right() - 12.0, card.center().y + 13.0));
    text_fit(p, Pos2::new(card.left() + 30.0, card.top() + 38.0), Align2::LEFT_CENTER, status, font(12.0), TEXT_DIM, br.left() - card.left() - 40.0);
    if pill_button(ui, br, "Open System Settings", if ok { BTN_SECONDARY } else { BTN_PRIMARY }, true).clicked() {
        if !ok {
            mac::ask_to_record_screen();
        }
        mac::open_privacy_settings();
    }
    if changed {
        c.save_settings();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn filters() {
        let s = |kind, pinned| Shot { id: 1, kind, w: 1, h: 1, saved: None, pinned, secs: None };
        assert!(passes(0, &s(Kind::Screen, false)));
        assert!(passes(1, &s(Kind::Edited, false)));
        assert!(!passes(2, &s(Kind::Area, false)));
        assert!(passes(4, &s(Kind::Window, true)));
        assert!(!passes(4, &s(Kind::Window, false)));
    }
}
