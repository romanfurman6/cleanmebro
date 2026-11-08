//! Capture: screenshots of an area, a window or the screen with macOS's own
//! `screencapture`, system-wide shortcuts, a floating preview, an editor,
//! pinned captures, text recognition, a color picker and a ruler, plus a
//! History and a Settings page.
//!
//! Page routes: "" (as left), "history", "settings".

pub mod editor;
mod grab;
mod hotkeys;
mod icons;
mod mac;
mod render;
mod settings;
mod store;
mod ui;
mod windows;

use crate::core::tool::{Action, Badge, Cx, Recommendation, Tool, ToolEnv, ToolId, View, WidgetSpec};
use crate::kit::glass::{tint, Icon};
use editor::Editor;
use egui::{Rect, Ui};
use grab::Grab;
use image::RgbaImage;
use settings::{Command, Settings};
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::mpsc::{channel, Receiver, Sender};
use std::time::Instant;
use store::{Kind, Store};

pub const ID: ToolId = "capture";
const TILE: &str = "tile";
const CARD: &str = "capture";

/// What a grab is for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Purpose {
    /// A capture for history (preview, editor…).
    Keep(Kind),
    /// Copy the text in the area.
    Text,
    /// Freeze the screen for the picker or the ruler.
    Measure(Measure),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Measure {
    Color,
    Ruler,
}

/// Results from background threads.
enum Done {
    Grab(Purpose, Result<Option<RgbaImage>, String>),
    Text(Vec<mac::TextLine>),
    Thumb(u64, Option<egui::ColorImage>),
    Folder(Option<PathBuf>),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tab {
    History,
    Settings,
}

/// Small textures of history captures, loaded on a thread when first shown.
#[derive(Default)]
struct Thumbs {
    tex: HashMap<u64, egui::TextureHandle>,
    pending: HashSet<u64>,
}

/// Longest side of a thumbnail, in pixels.
const THUMB_PX: u32 = 360;

fn thumb_image(img: &RgbaImage) -> egui::ColorImage {
    let (w, h) = img.dimensions();
    let k = (THUMB_PX as f32 / w.max(h).max(1) as f32).min(1.0);
    let t = image::imageops::thumbnail(img, ((w as f32 * k) as u32).max(1), ((h as f32 * k) as u32).max(1));
    egui::ColorImage::from_rgba_unmultiplied([t.width() as usize, t.height() as usize], t.as_raw())
}

pub struct Capture {
    settings: Settings,
    /// Where settings live; None for sample data (screenshots), which never saves.
    dir: Option<PathBuf>,
    store: Store,
    keys: Option<hotkeys::Keys>,
    key_errors: Vec<(Command, String)>,
    /// Commands to run on the next ui pass, with their delay in seconds.
    queue: Vec<(Command, u32)>,
    tx: Sender<Done>,
    rx: Receiver<Done>,
    /// A grab or text recognition is running.
    busy: bool,
    countdown: Option<windows::Countdown>,
    preview: Option<windows::Preview>,
    pins: Vec<windows::Pin>,
    editor: Option<windows::EditorWin>,
    overlay: Option<windows::Overlay>,
    hud: Option<(String, Instant)>,
    /// The Screen Recording card, with the capture that waits for it.
    perm_card: Option<(Grab, Purpose, u32)>,
    /// The user said "Not now" this session: capture without asking.
    perm_skip: bool,
    perm_needed: bool,
    thumbs: Thumbs,
    tab: Tab,
    /// Recording a new shortcut for this command.
    recording: Option<Command>,
    filter: usize,
    confirm_clear: Option<Instant>,
    /// Shell requests made outside a ui pass.
    actions: Vec<Action>,
    /// Points → pixels of the main screen.
    ppp: f32,
    demo_editor: Option<Editor>,
    ctx: egui::Context,
    next_pin: u64,
}

impl Capture {
    pub fn new(env: &ToolEnv) -> Self {
        let (tx, rx) = channel();
        let (settings, dir, store) = if env.demo {
            (Settings::default(), None, demo_store())
        } else {
            let dir = settings::data_dir();
            (settings::load(&dir), Some(dir.clone()), Store::open(dir.join("history")))
        };
        let mut c = Capture {
            settings,
            dir,
            store,
            keys: if env.demo { None } else { hotkeys::Keys::new(env.ctx.clone()) },
            key_errors: Vec::new(),
            queue: Vec::new(),
            tx,
            rx,
            busy: false,
            countdown: None,
            preview: None,
            pins: Vec::new(),
            editor: None,
            overlay: None,
            hud: None,
            perm_card: None,
            perm_skip: false,
            perm_needed: false,
            thumbs: Thumbs::default(),
            tab: Tab::History,
            recording: None,
            filter: 0,
            confirm_clear: None,
            actions: Vec::new(),
            ppp: if cfg!(target_os = "macos") { 2.0 } else { 1.0 },
            demo_editor: None,
            ctx: env.ctx.clone(),
            next_pin: 1,
        };
        c.apply_keys();
        c
    }

    fn save_settings(&self) {
        if let Some(dir) = &self.dir {
            settings::save(dir, &self.settings);
        }
    }

    /// Register the shortcuts from settings (not while one is being recorded).
    fn apply_keys(&mut self) {
        if let Some(k) = &mut self.keys {
            self.key_errors = k.apply(&self.settings.bound());
        }
    }

    fn hud(&mut self, msg: impl Into<String>) {
        self.hud = Some((msg.into(), Instant::now()));
        self.ctx.request_repaint();
    }

    fn copy_image(&self, img: &RgbaImage) -> bool {
        mac::copy_png(&store::png_bytes(img))
    }

    fn copy_text(&self, text: &str) {
        if !mac::copy_text(text) {
            self.ctx.copy_text(text.to_owned());
        }
    }

    /// Start a command (from a shortcut or a button). Needs the shell for
    /// hiding the panel, so it runs on a ui pass.
    fn run(&mut self, c: Command, delay: u32, cx: &mut Cx) {
        match c {
            Command::Area => self.start(Grab::Area, Purpose::Keep(Kind::Area), delay, cx),
            Command::Window => self.start(Grab::Window, Purpose::Keep(Kind::Window), delay, cx),
            Command::Screen => self.start(Grab::Screen, Purpose::Keep(Kind::Screen), delay, cx),
            Command::Delayed => {
                let d = if self.settings.delay > 0 { self.settings.delay } else { settings::DEFAULT_DELAY };
                self.start(Grab::Screen, Purpose::Keep(Kind::Screen), d, cx)
            }
            Command::Text => self.start(Grab::Area, Purpose::Text, 0, cx),
            Command::Color => self.start(Grab::Screen, Purpose::Measure(Measure::Color), 0, cx),
            Command::Ruler => self.start(Grab::Screen, Purpose::Measure(Measure::Ruler), 0, cx),
            Command::History => {
                self.tab = Tab::History;
                cx.open_page(ID, "history");
            }
        }
    }

    fn start(&mut self, g: Grab, p: Purpose, delay: u32, cx: &mut Cx) {
        if self.busy || self.countdown.is_some() || self.overlay.is_some() {
            return;
        }
        cx.push(Action::HidePanel);
        self.preview = None;
        if !self.perm_skip && !mac::can_record_screen() {
            self.perm_needed = true;
            self.perm_card = Some((g, p, delay));
            return;
        }
        if delay > 0 {
            self.countdown = Some(windows::Countdown { until: Instant::now() + std::time::Duration::from_secs(delay as u64), total: delay, grab: g, purpose: p });
        } else {
            // a moment for the panel to get out of the picture
            self.spawn_grab(g, p, if g == Grab::Screen { 250 } else { 120 });
        }
    }

    fn spawn_grab(&mut self, g: Grab, p: Purpose, settle_ms: u64) {
        self.busy = true;
        let tx = self.tx.clone();
        let ctx = self.ctx.clone();
        std::thread::spawn(move || {
            std::thread::sleep(std::time::Duration::from_millis(settle_ms));
            let r = grab::run(g);
            let _ = tx.send(Done::Grab(p, r));
            ctx.request_repaint();
        });
    }

    fn spawn_ocr(&mut self, img: RgbaImage) {
        self.busy = true;
        let tx = self.tx.clone();
        let ctx = self.ctx.clone();
        std::thread::spawn(move || {
            let _ = tx.send(Done::Text(mac::recognize_image(&img)));
            ctx.request_repaint();
        });
    }

    fn done(&mut self, d: Done) {
        match d {
            Done::Grab(p, r) => {
                self.busy = false;
                match (p, r) {
                    (_, Ok(None)) => {}
                    (_, Err(e)) => self.hud(format!("Couldn't capture: {e}")),
                    (Purpose::Keep(kind), Ok(Some(img))) => self.keep(img, kind),
                    (Purpose::Text, Ok(Some(img))) => self.spawn_ocr(img),
                    (Purpose::Measure(m), Ok(Some(img))) => self.overlay = Some(windows::Overlay::new(m, img)),
                }
            }
            Done::Text(lines) => {
                self.busy = false;
                if lines.is_empty() {
                    self.hud("No text found");
                } else {
                    self.copy_text(&mac::join_lines(&lines, self.settings.keep_breaks));
                    let n = lines.len();
                    self.hud(format!("Copied {n} line{}", if n == 1 { "" } else { "s" }));
                }
            }
            Done::Thumb(id, img) => {
                self.thumbs.pending.remove(&id);
                if let Some(ci) = img {
                    let t = self.ctx.load_texture(format!("capture-thumb-{id}"), ci, egui::TextureOptions::LINEAR);
                    self.thumbs.tex.insert(id, t);
                }
            }
            Done::Folder(Some(p)) => {
                self.settings.folder = p;
                self.save_settings();
            }
            Done::Folder(None) => {}
        }
    }

    /// A fresh capture: history, then copy / save / sound, then what the
    /// user picked to happen next.
    fn keep(&mut self, img: RgbaImage, kind: Kind) {
        let id = match self.store.add(&img, kind, self.settings.keep) {
            Ok(id) => {
                self.put_thumb(id, &img);
                Some(id)
            }
            Err(e) => {
                self.hud(format!("Couldn't keep it in history: {e}"));
                None
            }
        };
        self.forget_dropped_thumbs();
        if self.settings.sound {
            mac::play_shutter();
        }
        let copied = self.settings.copy && self.copy_image(&img);
        let mut saved = false;
        if self.settings.save {
            match store::save_copy(&img, &self.settings, chrono::Local::now()) {
                Ok(path) => {
                    saved = true;
                    if let Some(id) = id {
                        self.store.set_saved(id, path);
                    }
                }
                Err(e) => self.hud(format!("Couldn't save: {e}")),
            }
        }
        let note = match (copied, saved) {
            (true, true) => "Copied and saved",
            (true, false) => "Copied",
            (false, true) => "Saved",
            _ => "Captured",
        };
        match self.settings.after {
            settings::After::Preview => self.preview = Some(windows::Preview::new(id, img, note.into())),
            settings::After::Editor => self.open_editor(id, img),
            settings::After::Nothing => {
                if copied || saved {
                    self.hud(note);
                }
            }
        }
    }

    fn put_thumb(&mut self, id: u64, img: &RgbaImage) {
        let t = self.ctx.load_texture(format!("capture-thumb-{id}"), thumb_image(img), egui::TextureOptions::LINEAR);
        self.thumbs.tex.insert(id, t);
    }

    fn forget_dropped_thumbs(&mut self) {
        let ids: HashSet<u64> = self.store.shots.iter().map(|s| s.id).collect();
        self.thumbs.tex.retain(|id, _| ids.contains(id));
    }

    /// The thumbnail of a history capture; starts loading it when missing.
    fn thumb(&mut self, id: u64) -> Option<egui::TextureId> {
        if let Some(t) = self.thumbs.tex.get(&id) {
            return Some(t.id());
        }
        if self.thumbs.pending.len() < 3 && self.thumbs.pending.insert(id) {
            let path = self.store.path(id);
            let tx = self.tx.clone();
            let ctx = self.ctx.clone();
            std::thread::spawn(move || {
                let img = image::open(&path).ok().map(|i| thumb_image(&i.to_rgba8()));
                let _ = tx.send(Done::Thumb(id, img));
                ctx.request_repaint();
            });
        }
        None
    }

    fn title_for(&self, id: Option<u64>) -> String {
        match id.and_then(|id| self.store.get(id)) {
            Some(s) => format!("Capture from {}", s.time().format("%-I:%M %P")),
            None => "Capture".into(),
        }
    }

    /// Keep an editor's picture in history as its own entry when it has
    /// changes; returns the id the editor now stands for.
    fn commit_edit(&mut self, ed: &mut Editor) -> Option<u64> {
        if ed.edited {
            let img = ed.rendered();
            match self.store.add(&img, Kind::Edited, self.settings.keep) {
                Ok(id) => {
                    self.put_thumb(id, &img);
                    self.forget_dropped_thumbs();
                    ed.shot = Some(id);
                    ed.edited = false;
                }
                Err(e) => ed.flash(format!("Couldn't keep the edit: {e}")),
            }
        }
        ed.shot
    }

    fn open_editor(&mut self, id: Option<u64>, img: RgbaImage) {
        // one editor at a time; the one open keeps its changes in history
        if let Some(mut old) = self.editor.take() {
            self.commit_edit(&mut old.ed);
        }
        let title = self.title_for(id);
        let s = &self.settings;
        let ed = Editor::new(id, title, img, self.ppp, s.blur, s.cover, s.keep_breaks);
        self.editor = Some(windows::EditorWin::new(ed));
    }

    fn open_editor_for(&mut self, id: u64) {
        match self.store.load(id) {
            Some(img) => self.open_editor(Some(id), img),
            None => self.hud("That capture is gone"),
        }
    }

    fn pin(&mut self, id: Option<u64>, img: RgbaImage) {
        if let Some(id) = id {
            self.store.set_pinned(id, true);
        }
        let n = self.next_pin;
        self.next_pin += 1;
        let offset = (self.pins.len() as f32) * 24.0;
        self.pins.push(windows::Pin::new(n, id, img, self.ppp, offset));
    }

    fn pin_shot(&mut self, id: u64) {
        match self.store.load(id) {
            Some(img) => self.pin(Some(id), img),
            None => self.hud("That capture is gone"),
        }
    }

    /// Save a copy to the user's folder; remembers it on the history entry.
    fn save_image(&mut self, id: Option<u64>, img: &RgbaImage) -> Option<PathBuf> {
        match store::save_copy(img, &self.settings, chrono::Local::now()) {
            Ok(path) => {
                if let Some(id) = id {
                    self.store.set_saved(id, path.clone());
                }
                Some(path)
            }
            Err(e) => {
                self.hud(format!("Couldn't save: {e}"));
                None
            }
        }
    }

    fn shown_folder(&self) -> String {
        let home = dirs::home_dir().map(|h| h.display().to_string()).unwrap_or_default();
        let f = self.settings.folder.display().to_string();
        if !home.is_empty() && f.starts_with(&home) {
            f.replacen(&home, "~", 1)
        } else {
            f
        }
    }

    fn choose_folder(&mut self) {
        let tx = self.tx.clone();
        let ctx = self.ctx.clone();
        std::thread::spawn(move || {
            let out = std::process::Command::new("osascript").args(["-e", "POSIX path of (choose folder with prompt \"Save captures to\")"]).output();
            let path = out.ok().filter(|o| o.status.success()).map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string()).filter(|s| !s.is_empty()).map(PathBuf::from);
            let _ = tx.send(Done::Folder(path));
            ctx.request_repaint();
        });
    }
}

/// Sample history for screenshots, in a throwaway folder.
fn demo_store() -> Store {
    let dir = std::env::temp_dir().join(format!("cleanmebro-capture-demo-{}", std::process::id()));
    let _ = std::fs::create_dir_all(&dir);
    let mut st = Store::open(dir);
    let now = chrono::Local::now().timestamp_millis() as u64;
    let hour = 3_600_000u64;
    let items: [(u64, Kind, u32, u32); 8] = [
        (1, Kind::Area, 794, 446),
        (2, Kind::Window, 794, 500),
        (4, Kind::Area, 640, 410),
        (5, Kind::Screen, 900, 560),
        (7, Kind::Edited, 920, 300),
        (26, Kind::Window, 794, 520),
        (28, Kind::Area, 760, 520),
        (31, Kind::Screen, 900, 560),
    ];
    for (i, (h, kind, w, ht)) in items.iter().enumerate() {
        let id = now - h * hour - i as u64;
        let img = grab::sample(*w, *ht);
        if img.save(st.path(id)).is_ok() {
            st.shots.push(store::Shot { id, kind: *kind, w: *w, h: *ht, saved: None, pinned: i == 2 });
        }
    }
    st
}

fn badge() -> Badge {
    Badge { icon: Icon::Crop, color: tint::CAPTURE }
}

impl Tool for Capture {
    fn id(&self) -> ToolId {
        ID
    }
    fn name(&self) -> &'static str {
        "Capture"
    }
    fn badge(&self) -> Badge {
        badge()
    }
    fn section(&self) -> &'static str {
        "Create"
    }
    fn sidebar_meta(&self) -> String {
        self.settings.shortcut(Command::Area).map(|s| s.display()).unwrap_or_default()
    }
    fn widgets(&self) -> Vec<WidgetSpec> {
        vec![
            WidgetSpec { id: TILE, title: "Capture (quick action)", span: 1, default_on: true, group: None },
            WidgetSpec { id: CARD, title: "Capture", span: 4, default_on: true, group: None },
        ]
    }
    fn widget_height(&self, widget: &str, _width: f32) -> f32 {
        if widget == TILE { crate::kit::glass::QUICK_TILE_H } else { ui::CARD_H }
    }
    fn widget_ui(&mut self, widget: &str, ui: &mut Ui, rect: Rect, cx: &mut Cx) {
        if widget == TILE {
            ui::tile(self, ui, rect);
        } else {
            ui::card(self, ui, rect, cx);
        }
    }
    fn has_page(&self) -> bool {
        true
    }
    fn open_page(&mut self, route: &str) {
        match route {
            "history" => self.tab = Tab::History,
            "settings" => self.tab = Tab::Settings,
            _ => {}
        }
    }
    fn page_ui(&mut self, ui: &mut Ui, rect: Rect, cx: &mut Cx) {
        if let Some(ed) = &mut self.demo_editor {
            ed.ui(ui, rect, false);
            return;
        }
        ui::page(self, ui, rect, cx);
    }
    fn recommendations(&self) -> Vec<Recommendation> {
        if !self.perm_needed || mac::can_record_screen() {
            return Vec::new();
        }
        vec![Recommendation {
            priority: 40,
            badge: badge(),
            title: "Capture needs Screen Recording".into(),
            body: "Without it, captures show only the desktop. Turn it on in System Settings.".into(),
            action: "Settings".into(),
            go: Action::OpenPage { tool: ID.into(), route: "settings".into() },
        }]
    }
    fn tick(&mut self, ctx: &egui::Context) -> bool {
        if let Some(k) = &self.keys {
            if self.recording.is_none() {
                for c in k.poll() {
                    self.queue.push((c, 0));
                }
            }
        }
        while let Ok(d) = self.rx.try_recv() {
            self.done(d);
        }
        if let Some(c) = &self.countdown {
            if Instant::now() >= c.until {
                let (g, p) = (c.grab, c.purpose);
                self.countdown = None;
                self.spawn_grab(g, p, 220);
            }
        }
        let timed = self.countdown.is_some() || self.hud.is_some() || self.preview.is_some();
        if timed {
            ctx.request_repaint_after(std::time::Duration::from_millis(100));
        }
        if !self.queue.is_empty() || !self.actions.is_empty() {
            ctx.request_repaint();
        }
        let windows = !self.pins.is_empty() || self.editor.is_some() || self.overlay.is_some() || self.perm_card.is_some();
        timed || windows || !self.queue.is_empty() || !self.actions.is_empty()
    }
    fn windows(&mut self, ctx: &egui::Context, cx: &mut Cx) {
        if let Some(p) = ctx.input(|i| i.viewport().native_pixels_per_point) {
            self.ppp = p;
        }
        for a in self.actions.drain(..) {
            cx.push(a);
        }
        for (c, d) in std::mem::take(&mut self.queue) {
            self.run(c, d, cx);
        }
        windows::show(self, ctx, cx);
    }
    fn demo_view(&mut self, view: &str) -> Option<View> {
        match view {
            "capture" => {
                self.tab = Tab::History;
                Some(View::Page)
            }
            "capture-settings" => {
                self.tab = Tab::Settings;
                Some(View::Page)
            }
            "capture-editor" => {
                let mut ed = Editor::new(None, "Capture from 5:41 pm".into(), grab::sample(1588, 892), 2.0, self.settings.blur, self.settings.cover, true);
                ed.demo_marks();
                self.demo_editor = Some(ed);
                Some(View::Page)
            }
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn demo() -> Capture {
        Capture::new(&ToolEnv { ctx: egui::Context::default(), demo: true })
    }

    #[test]
    fn thumbnails_are_small() {
        let t = thumb_image(&grab::sample(1600, 900));
        assert_eq!(t.size, [THUMB_PX as usize, 202]);
    }

    #[test]
    fn closing_an_edited_capture_keeps_it_as_a_new_entry() {
        let mut c = demo();
        let before = c.store.shots.len();
        let first = c.store.shots[0].id;
        let mut ed = Editor::new(Some(first), "t".into(), grab::sample(200, 100), 2.0, c.settings.blur, c.settings.cover, true);
        assert_eq!(c.commit_edit(&mut ed), Some(first), "no changes, nothing new");
        ed.edited = true;
        let id = c.commit_edit(&mut ed).expect("kept");
        assert_ne!(id, first);
        assert_eq!(c.store.shots.len(), before + 1);
        assert_eq!(c.store.shots[0].kind, Kind::Edited);
        assert!(!ed.edited);
        assert!(c.store.get(first).is_some(), "the original stays");
    }

    #[test]
    fn panel_buttons_queue_commands_for_the_next_frame() {
        let mut c = demo();
        c.queue.push((Command::History, 0));
        assert!(c.tick(&c.ctx.clone()), "queued work asks for a frame even while hidden");
        let mut cx = Cx::new(None);
        c.windows(&c.ctx.clone(), &mut cx);
        assert!(c.queue.is_empty());
        assert_eq!(cx.take_actions(), vec![Action::OpenPage { tool: ID.into(), route: "history".into() }]);
    }
}
