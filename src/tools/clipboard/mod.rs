//! Clipboard: a history of everything copied (text, links, code, colors,
//! images, files), a quick paste window on a system-wide shortcut, pinned
//! entries, and privacy controls (password managers and concealed items are
//! skipped, apps can be excluded, recording can be paused).
//!
//! Page routes: "" (as left), "history", "settings".

mod mac;
mod quick;
mod settings;
mod store;
mod ui;
mod watch;

use crate::core::tool::{Action, Badge, Cx, Recommendation, Size, Tool, ToolEnv, ToolId, View, WidgetSpec, ALL_SIZES};
use crate::kit::glass::Icon;
use crate::kit::hotkeys::Keys;
use egui::{Color32, Rect, Ui};
use settings::{Layout, Settings};
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::atomic::Ordering;
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::Arc;
use std::time::{Duration, Instant};
use store::{Clip, Kind, Store};

pub const ID: ToolId = "clipboard";
const CARD: &str = "recent";
pub const TINT: Color32 = Color32::from_rgb(0xC7, 0x80, 0x1F);

/// What the global shortcut starts.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Cmd {
    Quick,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tab {
    History,
    Settings,
}

/// The history filter: everything, one kind, or pinned only.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Filter {
    All,
    Kind(Kind),
    Pinned,
}

impl Filter {
    pub fn passes(self, e: &store::Entry) -> bool {
        match self {
            Filter::All => true,
            Filter::Kind(k) => e.kind == k,
            Filter::Pinned => e.pinned,
        }
    }
}

/// Recording is paused until then (None: until the user resumes).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Pause(Option<Instant>);

/// Small textures of image entries, loaded on a thread when first shown.
#[derive(Default)]
struct Thumbs {
    tex: HashMap<u64, egui::TextureHandle>,
    pending: HashSet<u64>,
}

const THUMB_PX: u32 = 420;

fn thumb_image(png: &[u8]) -> Option<egui::ColorImage> {
    let img = image::load_from_memory(png).ok()?.to_rgba8();
    let (w, h) = img.dimensions();
    let k = (THUMB_PX as f32 / w.max(h).max(1) as f32).min(1.0);
    let t = image::imageops::thumbnail(&img, ((w as f32 * k).round() as u32).max(1), ((h as f32 * k).round() as u32).max(1));
    Some(egui::ColorImage::from_rgba_unmultiplied([t.width() as usize, t.height() as usize], t.as_raw()))
}

pub struct Clipboard {
    settings: Settings,
    /// Where settings live; None for sample data (screenshots), which never saves.
    dir: Option<PathBuf>,
    store: Store,
    keys: Option<Keys<Cmd>>,
    key_error: Option<String>,
    /// Recording a new shortcut.
    recording: bool,
    shared: Arc<watch::Shared>,
    clips_tx: Sender<Clip>,
    clips: Receiver<Clip>,
    watching: bool,
    pause: Option<Pause>,
    quick: Option<quick::Quick>,
    /// Open quick paste on the next ui pass.
    want_quick: bool,
    tab: Tab,
    filter: Filter,
    search: String,
    selected: Option<u64>,
    thumbs: Thumbs,
    thumb_tx: Sender<(u64, Option<egui::ColorImage>)>,
    thumb_rx: Receiver<(u64, Option<egui::ColorImage>)>,
    hud: Option<(String, Instant)>,
    /// Apps picked in "Add app…" (None: cancelled).
    app_tx: Sender<Option<settings::Ignored>>,
    app_rx: Receiver<Option<settings::Ignored>>,
    confirm_clear: Option<Instant>,
    /// The last [`Clipboard::visible`] answer, keyed by its question and the
    /// history's version: it's asked several times every frame.
    visible_memo: std::cell::RefCell<Option<(Filter, String, u64, Vec<u64>)>>,
    /// Shell requests made outside a ui pass.
    actions: Vec<Action>,
    /// Asked for paste but Accessibility is off: show the hint in the panel.
    needs_access: bool,
    /// Screenshots only: draw quick paste inside the page.
    demo_quick: Option<quick::Quick>,
    ctx: egui::Context,
    demo: bool,
}

impl Clipboard {
    pub fn new(env: &ToolEnv) -> Self {
        let (clips_tx, clips) = channel();
        let (thumb_tx, thumb_rx) = channel();
        let (app_tx, app_rx) = channel();
        let (settings, dir, store) = if env.demo {
            (Settings::default(), None, demo_store())
        } else {
            let dir = settings::data_dir();
            (settings::load(&dir), Some(dir.clone()), Store::open(dir.join("history")))
        };
        let mut c = Clipboard {
            settings,
            dir,
            store,
            keys: if env.demo { None } else { Keys::new(env.ctx.clone()) },
            key_error: None,
            recording: false,
            shared: Arc::new(watch::Shared::default()),
            clips_tx,
            clips,
            watching: false,
            pause: None,
            quick: None,
            want_quick: false,
            tab: Tab::History,
            filter: Filter::All,
            search: String::new(),
            selected: None,
            thumbs: Thumbs::default(),
            thumb_tx,
            thumb_rx,
            hud: None,
            app_tx,
            app_rx,
            confirm_clear: None,
            visible_memo: Default::default(),
            actions: Vec::new(),
            needs_access: false,
            demo_quick: None,
            ctx: env.ctx.clone(),
            demo: env.demo,
        };
        let now = now_ms();
        c.store.trim(c.settings.limits(), now);
        c.sync_shared();
        c.apply_keys();
        c.start_watching();
        c
    }

    fn save_settings(&mut self) {
        if let Some(dir) = &self.dir {
            settings::save(dir, &self.settings);
        }
        self.sync_shared();
        self.store.trim(self.settings.limits(), now_ms());
        self.forget_dropped_thumbs();
    }

    fn sync_shared(&self) {
        self.shared.concealed.store(self.settings.record_concealed, Ordering::Relaxed);
        self.shared.images.store(self.settings.keep_images, Ordering::Relaxed);
        self.shared.paused.store(self.pause.is_some(), Ordering::Relaxed);
    }

    fn start_watching(&mut self) {
        if self.watching || self.demo {
            return;
        }
        self.shared = Arc::new(watch::Shared::default());
        self.sync_shared();
        watch::spawn(self.shared.clone(), self.clips_tx.clone(), self.ctx.clone());
        self.watching = true;
    }

    fn stop_watching(&mut self) {
        self.shared.stop.store(true, Ordering::Relaxed);
        self.watching = false;
    }

    fn apply_keys(&mut self) {
        if let Some(k) = &mut self.keys {
            let wanted: Vec<(Cmd, crate::kit::hotkeys::Shortcut)> = self.settings.shortcut.iter().map(|s| (Cmd::Quick, s.clone())).collect();
            self.key_error = k.apply(&wanted).into_iter().next().map(|(_, e)| e);
        }
    }

    fn hud(&mut self, msg: impl Into<String>) {
        self.hud = Some((msg.into(), Instant::now()));
        self.ctx.request_repaint();
    }

    pub fn paused(&self) -> bool {
        self.pause.is_some()
    }

    /// Pause for `mins` minutes (0: until resumed), or resume when paused.
    fn toggle_pause(&mut self, mins: u64) {
        self.pause = if self.pause.is_some() { None } else { Some(Pause((mins > 0).then(|| Instant::now() + Duration::from_secs(mins * 60)))) };
        self.sync_shared();
    }

    /// "Paused · 14 min left" and the like.
    pub fn pause_label(&self) -> Option<String> {
        let p = self.pause?;
        Some(match p.0 {
            Some(t) => {
                let left = t.saturating_duration_since(Instant::now()).as_secs();
                format!("Paused · {} min left", left.div_ceil(60).max(1))
            }
            None => "Paused".into(),
        })
    }

    /// A clip from the watcher: privacy rules, then history.
    fn take(&mut self, clip: Clip) {
        if self.pause.is_some() || self.settings.ignores(&clip.bundle) || (clip.concealed && !self.settings.record_concealed) {
            return;
        }
        if clip.png.is_some() && !self.settings.keep_images {
            return;
        }
        if let Some(id) = self.store.add(&clip, now_ms(), self.settings.limits()) {
            if self.selected.is_none() {
                self.selected = Some(id);
            }
        }
        self.forget_dropped_thumbs();
    }

    fn forget_dropped_thumbs(&mut self) {
        let ids: HashSet<u64> = self.store.entries.iter().map(|e| e.id).collect();
        self.thumbs.tex.retain(|id, _| ids.contains(id));
    }

    /// The thumbnail of an image entry; starts loading it when missing.
    fn thumb(&mut self, id: u64) -> Option<egui::TextureId> {
        if let Some(t) = self.thumbs.tex.get(&id) {
            return Some(t.id());
        }
        if self.thumbs.pending.len() < 3 && self.thumbs.pending.insert(id) {
            let path = self.store.image_path(id);
            let tx = self.thumb_tx.clone();
            let ctx = self.ctx.clone();
            std::thread::spawn(move || {
                let img = std::fs::read(&path).ok().and_then(|b| thumb_image(&b));
                let _ = tx.send((id, img));
                ctx.request_repaint();
            });
        }
        None
    }

    /// Put an entry on the pasteboard. `plain` drops formatting.
    fn copy(&mut self, id: u64, plain: bool) -> bool {
        let Some(e) = self.store.get(id).cloned() else { return false };
        let png = if e.kind == Kind::Image { self.store.png(id) } else { None };
        if e.kind == Kind::Image && png.is_none() {
            self.hud("That image is gone");
            return false;
        }
        let rtf = if plain { None } else { self.store.rtf(id) };
        let html = if plain { None } else { self.store.html(id) };
        let files = e.files();
        let text = if e.kind == Kind::File && plain { Some(e.text.as_str()) } else if e.kind == Kind::Image { None } else { Some(e.text.as_str()) };
        let w = mac::Write { text, rtf: rtf.as_deref(), html: html.as_deref(), png: png.as_deref(), files: if plain { &[] } else { &files } };
        match mac::write(&w) {
            Some(n) => {
                self.shared.own.store(n, Ordering::Relaxed);
                if !e.pinned {
                    self.store.touch(id, now_ms());
                }
                true
            }
            None => {
                // off macOS (and if the pasteboard refuses): egui's copy for text
                if e.kind != Kind::Image {
                    self.ctx.copy_text(e.text.clone());
                    return true;
                }
                false
            }
        }
    }

    /// Copy, then paste into the app that was in front (when allowed).
    fn paste(&mut self, id: u64, plain: bool, into: Option<i32>) {
        if !self.copy(id, plain) {
            return;
        }
        let can = mac::can_paste();
        match into {
            Some(pid) if self.settings.paste_now && can => {
                mac::activate_pid(pid);
                std::thread::spawn(|| {
                    // let the app come to the front first
                    std::thread::sleep(Duration::from_millis(160));
                    mac::press_paste();
                });
            }
            _ if self.settings.paste_now && !can && !self.demo => {
                self.needs_access = true;
                if let Some(pid) = into {
                    mac::activate_pid(pid);
                }
                self.hud("Copied. Allow Accessibility in Settings to paste right away.");
            }
            _ => {
                if let Some(pid) = into {
                    mac::activate_pid(pid);
                }
                self.hud(if plain { "Copied as plain text" } else { "Copied" });
            }
        }
    }

    fn open_quick(&mut self) {
        if self.quick.is_some() {
            self.quick = None;
            return;
        }
        let front = mac::front_app().filter(|(_, _, pid)| *pid != mac::own_pid()).map(|(_, _, pid)| pid);
        self.quick = Some(quick::Quick::new(self.settings.layout, front));
        self.ctx.request_repaint();
    }

    /// Entries that pass the filter and search, pinned first.
    fn visible(&self, filter: Filter, query: &str) -> Vec<u64> {
        let rev = self.store.rev();
        let mut memo = self.visible_memo.borrow_mut();
        if let Some((f, q, r, ids)) = memo.as_ref() {
            if *f == filter && q == query && *r == rev {
                return ids.clone();
            }
        }
        let words: Vec<String> = query.to_lowercase().split_whitespace().map(str::to_string).collect();
        let ids: Vec<u64> = self.store.ordered().into_iter().filter(|e| filter.passes(e) && e.matches_words(&words)).map(|e| e.id).collect();
        *memo = Some((filter, query.to_string(), rev, ids.clone()));
        ids
    }
}

fn now_ms() -> i64 {
    chrono::Local::now().timestamp_millis()
}

/// Sample history for screenshots.
fn demo_store() -> Store {
    let dir = std::env::temp_dir().join(format!("cleanmebro-clipboard-demo-{}", std::process::id()));
    let mut st = Store::open(dir.clone());
    st.entries.clear();
    let now = now_ms();
    let min = 60_000i64;
    let lim = store::Limits { keep: 500, max_days: 0, dedupe: true };
    let mut png = Vec::new();
    let img = image::RgbaImage::from_fn(640, 420, |x, y| {
        // a small sunset: sky, sun, two hills
        let (fx, fy) = (x as f32 / 640.0, y as f32 / 420.0);
        let sun = ((fx - 0.72).powi(2) + (fy - 0.28).powi(2) * 2.4).sqrt() < 0.09;
        let hill1 = fy > 0.80 - 0.32 * (1.0 - ((fx - 0.25) * 3.2).abs()).max(0.0) - 0.22 * (1.0 - ((fx - 0.62) * 3.0).abs()).max(0.0);
        let hill2 = fy > 0.90 - 0.18 * (1.0 - ((fx - 0.4) * 2.6).abs()).max(0.0) - 0.16 * (1.0 - ((fx - 0.82) * 2.8).abs()).max(0.0);
        let c = if hill2 {
            [62, 46, 82]
        } else if hill1 {
            [122, 78, 126]
        } else if sun {
            [255, 227, 179]
        } else {
            let t = fy;
            [(242.0 - 40.0 * t) as u8, (184.0 - 50.0 * t) as u8, (128.0 + 10.0 * t) as u8]
        };
        image::Rgba([c[0], c[1], c[2], 255])
    });
    let _ = img.write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png);
    let clip = |t: &str, app: &str, bundle: &str| Clip { text: Some(t.into()), app: app.into(), bundle: bundle.into(), ..Default::default() };
    let items: Vec<(i64, Clip, bool)> = vec![
        (60 * 24 + 5, clip("Thanks! I will check the build tonight and send notes.", "Mail", "com.apple.mail"), false),
        (60 * 22, clip("https://developer.apple.com/documentation/appkit/nspasteboard", "Safari", "com.apple.Safari"), false),
        (60 * 26, clip("Roman Furman · Kyiv, Ukraine", "Notes", "com.apple.Notes"), true),
        (60, Clip { files: vec!["/Users/roman/Documents/Q4 report.pdf".into()], app: "Finder".into(), bundle: "com.apple.finder".into(), ..Default::default() }, false),
        (20, clip("#0B6BD8", "Figma", "com.figma.Desktop"), false),
        (12, Clip { png: Some(png), app: "Photos".into(), bundle: "com.apple.Photos".into(), ..Default::default() }, false),
        (6, clip("fn main() {\n    let app = App::new();\n    app.tool(\"clipboard\")\n       .run()?;\n}", "Zed", "dev.zed.Zed"), true),
        (2, clip("https://github.com/romanfurman6/cleanmebro/pull/9", "Safari", "com.apple.Safari"), false),
        (0, clip("Ship the clipboard widget before Friday, then we test it on the real Mac together.", "Slack", "com.tinyspeck.slackmacgap"), false),
    ];
    let _ = std::fs::create_dir_all(dir.join("blobs"));
    for (ago, c, pin) in items {
        if let Some(id) = st.add(&c, now - ago * min, lim) {
            if pin {
                st.set_pinned(id, true);
            }
        }
    }
    st
}

fn badge() -> Badge {
    Badge { icon: Icon::Clipboard, color: TINT }
}

impl Tool for Clipboard {
    fn id(&self) -> ToolId {
        ID
    }
    fn name(&self) -> &'static str {
        "Clipboard"
    }
    fn badge(&self) -> Badge {
        badge()
    }
    fn section(&self) -> &'static str {
        "Create"
    }
    fn about(&self) -> &'static str {
        "History of everything you copy, with quick paste"
    }
    fn set_on(&mut self, on: bool) {
        if on {
            self.apply_keys();
            self.start_watching();
        } else {
            if let Some(k) = &mut self.keys {
                k.clear();
            }
            self.stop_watching();
            self.quick = None;
            self.recording = false;
        }
    }
    fn sidebar_meta(&self) -> String {
        if self.paused() {
            return "Paused".into();
        }
        self.settings.shortcut.as_ref().map(|s| s.display()).unwrap_or_default()
    }
    fn widgets(&self) -> Vec<WidgetSpec> {
        vec![WidgetSpec { id: CARD, title: "Clipboard", about: "Your latest clips; click one to copy it again.", sizes: ALL_SIZES, size: Size::S11, default_on: true }]
    }
    fn widget_ui(&mut self, _widget: &str, size: Size, ui: &mut Ui, rect: Rect, cx: &mut Cx) {
        ui::widget(self, size, ui, rect, cx);
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
        if let Some(mut q) = self.demo_quick.take() {
            // the window's own size, as it shows over other apps
            let r = Rect::from_min_size(rect.min + egui::vec2(24.0, 24.0), q.size().min(rect.size() - egui::vec2(48.0, 48.0)));
            quick::draw(self, &mut q, ui, r);
            self.demo_quick = Some(q);
            return;
        }
        ui::page(self, ui, rect, cx);
    }
    fn recommendations(&self) -> Vec<Recommendation> {
        if !self.needs_access || mac::can_paste() {
            return Vec::new();
        }
        vec![Recommendation {
            priority: 35,
            badge: badge(),
            title: "Clipboard can't paste for you yet".into(),
            body: "Allow Accessibility so quick paste can type ⌘V in the app you were using.".into(),
            action: "Settings".into(),
            go: Action::OpenPage { tool: ID.into(), route: "settings".into() },
        }]
    }
    fn tick(&mut self, ctx: &egui::Context) -> bool {
        if let Some(k) = &self.keys {
            if !self.recording && k.poll().contains(&Cmd::Quick) {
                self.want_quick = true;
            }
        }
        while let Ok(c) = self.clips.try_recv() {
            self.take(c);
        }
        while let Ok((id, img)) = self.thumb_rx.try_recv() {
            self.thumbs.pending.remove(&id);
            if let Some(ci) = img {
                let t = ctx.load_texture(format!("clip-thumb-{id}"), ci, egui::TextureOptions::LINEAR);
                self.thumbs.tex.insert(id, t);
            }
        }
        while let Ok(app) = self.app_rx.try_recv() {
            if let Some(app) = app {
                if !self.settings.ignore.iter().any(|i| i.bundle == app.bundle) {
                    self.settings.ignore.push(app);
                    self.save_settings();
                }
            }
        }
        if let Some(Pause(Some(t))) = self.pause {
            if Instant::now() >= t {
                self.pause = None;
                self.sync_shared();
            } else {
                ctx.request_repaint_after(Duration::from_secs(30));
            }
        }
        if self.want_quick {
            self.want_quick = false;
            self.open_quick();
        }
        if self.hud.is_some() {
            ctx.request_repaint_after(Duration::from_millis(100));
        }
        self.quick.is_some() || self.hud.is_some() || !self.actions.is_empty()
    }
    fn windows(&mut self, ctx: &egui::Context, cx: &mut Cx) {
        for a in self.actions.drain(..) {
            cx.push(a);
        }
        quick::show(self, ctx);
        ui::hud(self, ctx);
    }
    fn demo_view(&mut self, view: &str) -> Option<View> {
        match view {
            "clipboard" => {
                self.tab = Tab::History;
                self.selected = self.store.entries.iter().find(|e| e.kind == Kind::Link).map(|e| e.id);
                Some(View::Page)
            }
            "clipboard-settings" => {
                self.tab = Tab::Settings;
                Some(View::Page)
            }
            "clipboard-quick" | "clipboard-list" => {
                let layout = if view == "clipboard-list" { Layout::List } else { Layout::Cards };
                self.demo_quick = Some(quick::Quick::new(layout, None));
                Some(View::Page)
            }
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn demo() -> Clipboard {
        Clipboard::new(&ToolEnv { ctx: egui::Context::default(), demo: true })
    }

    #[test]
    fn sample_history_has_every_kind() {
        let c = demo();
        for k in Kind::ALL {
            assert!(c.store.entries.iter().any(|e| e.kind == k), "no {k:?} sample");
        }
        let v = c.visible(Filter::All, "");
        assert!(c.store.get(v[0]).unwrap().pinned, "pinned first");
        assert_eq!(c.visible(Filter::Kind(Kind::Color), "").len(), 1);
        assert_eq!(c.visible(Filter::All, "github").len(), 1);
    }

    #[test]
    fn privacy_rules() {
        let mut c = demo();
        let n = c.store.entries.len();
        let pw = Clip { text: Some("hunter2".into()), concealed: true, ..Default::default() };
        c.take(pw.clone());
        assert_eq!(c.store.entries.len(), n, "concealed skipped by default");
        c.take(Clip { text: Some("secret".into()), bundle: "com.1password.1password".into(), ..Default::default() });
        assert_eq!(c.store.entries.len(), n, "ignored app");
        c.toggle_pause(15);
        assert!(c.paused());
        c.take(Clip { text: Some("while paused".into()), ..Default::default() });
        assert_eq!(c.store.entries.len(), n);
        assert!(c.pause_label().unwrap().starts_with("Paused · 15 min"));
        c.toggle_pause(15);
        c.settings.record_concealed = true;
        c.take(pw);
        assert_eq!(c.store.entries.len(), n + 1, "turned on in Settings");
    }

    #[test]
    fn thumbnails_are_small() {
        let mut png = Vec::new();
        image::RgbaImage::new(1600, 900).write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png).unwrap();
        let t = thumb_image(&png).unwrap();
        assert_eq!(t.size, [THUMB_PX as usize, 236]);
    }
}
