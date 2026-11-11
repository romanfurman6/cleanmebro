//! Dictation: press the shortcut in any app, speak, and the words are typed
//! where the caret is. Hold the keys to talk (let go to finish), or tap them
//! to start and tap again to finish; Esc throws it away. While listening a
//! small pill sits under the caret. Everything runs on this Mac: Parakeet v3
//! (default) or Whisper turbo.
//!
//! Page routes: "" (as left), "history", "settings".

mod audio;
mod mac;
pub mod probe;
mod settings;
mod ui;

use crate::core::tool::{Action, Badge, Cx, Size, Tool, ToolEnv, ToolId, View, WidgetSpec};
use crate::kit::glass::Icon;
use crate::kit::hotkeys::{Keys, Shortcut};
use crate::kit::perms::{Need, Perm, Status};
use crate::kit::whisper::Progress;
use audio::{Mic, MODEL_RATE};
use egui::{Color32, Pos2, Rect, Ui};
use settings::{Engine, History, Said, Settings};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::Arc;
use std::time::{Duration, Instant};

pub const ID: ToolId = "dictation";
const CARD: &str = "card";
/// The tool's own setup step: the speech model.
const MODEL_NEED: &str = "dictation-model";
#[allow(non_snake_case)]
pub fn TINT() -> Color32 { crate::kit::glass::tint::CAPTURE() }

/// Held shorter than this, the shortcut was a tap: keep listening until the
/// next one.
const HOLD: Duration = Duration::from_millis(350);
/// Longest single dictation.
const MAX_TALK: Duration = Duration::from_secs(300);
/// Shorter than this, nothing was said.
const MIN_VOICE_SECS: f32 = 0.3;
/// A loaded model is let go after this long unused (it holds ~1 GB).
const IDLE_UNLOAD: Duration = Duration::from_secs(10 * 60);
/// Wait after bringing the app forward before pressing ⌘V.
const PASTE_DELAY: Duration = Duration::from_millis(90);
/// Wait after ⌘V before putting the old clipboard back.
const RESTORE_DELAY: Duration = Duration::from_millis(600);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tab {
    History,
    Settings,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Cmd {
    Talk,
    Cancel,
    /// Return while listening: stop now and type it.
    Done,
}

/// What the pill says once the talking is over.
#[derive(Clone, Debug, PartialEq)]
pub enum Note {
    Pasted,
    /// No Accessibility: the text is on the clipboard for ⌘V.
    Copied,
    Problem(String),
}

/// Where dictation is.
pub enum Phase {
    Idle,
    /// `heard` is when the voice was last above a whisper.
    Listening { mic: Mic, since: Instant, tapped: bool, heard: Instant },
    Working { job: u64 },
    Showing { note: Note, until: Instant },
}

/// Work for the model thread.
enum Job {
    /// Load the model now, while the user is still talking.
    Warm(Engine),
    Run { job: u64, engine: Engine, lang: Option<&'static str>, pcm: Vec<f32> },
}

struct Heard {
    job: u64,
    text: Result<String, String>,
}

/// A model download running on a thread.
pub struct Download {
    pub engine: Engine,
    pub progress: Arc<Progress>,
    pub unpacking: Arc<AtomicBool>,
    rx: Receiver<Result<(), String>>,
}

pub struct Dictation {
    settings: Settings,
    /// None for sample data (screenshots), which never saves.
    dir: Option<PathBuf>,
    history: History,
    pub phase: Phase,
    tab: Tab,
    on: bool,
    target: mac::Target,
    /// Where the pill goes: the caret, the mouse, the screen they're on.
    place: (Option<Rect>, Option<Pos2>, Option<Rect>),
    /// Loudness, newest last, for the pill's bars.
    levels: Vec<f32>,
    keys: Option<Keys<Cmd>>,
    esc: Option<Keys<Cmd>>,
    esc_bound: bool,
    bound: Option<Shortcut>,
    key_error: Option<String>,
    /// Recording a new shortcut on the Settings tab.
    recording_key: bool,
    /// When the recorder was last drawn; it stops once the page is gone.
    recording_seen: Instant,
    jobs: Sender<Job>,
    heard: Receiver<Heard>,
    next_job: u64,
    voice_secs: f32,
    paste_at: Option<(Instant, String)>,
    restore_at: Option<(Instant, mac::Saved, isize)>,
    pub download: Option<Download>,
    download_error: Option<String>,
    /// Shell requests made outside a ui pass.
    actions: Vec<Action>,
    ctx: egui::Context,
    demo: bool,
}

impl Dictation {
    pub fn new(env: &ToolEnv) -> Self {
        let (jobs, job_rx) = channel();
        let (heard_tx, heard) = channel();
        let (settings, dir, history) = if env.demo {
            (Settings::default(), None, demo_history())
        } else {
            let dir = settings::data_dir();
            (settings::load(&dir), Some(dir.clone()), settings::load_history(&dir))
        };
        if !env.demo {
            let ctx = env.ctx.clone();
            std::thread::Builder::new().name("dictation".into()).spawn(move || worker(job_rx, heard_tx, ctx)).ok();
        }
        Dictation {
            settings,
            dir,
            history,
            phase: Phase::Idle,
            tab: Tab::History,
            on: false,
            target: mac::Target::default(),
            place: (None, None, None),
            levels: Vec::new(),
            keys: None,
            esc: None,
            esc_bound: false,
            bound: None,
            key_error: None,
            recording_key: false,
            recording_seen: Instant::now(),
            jobs,
            heard,
            next_job: 0,
            voice_secs: 0.0,
            paste_at: None,
            restore_at: None,
            download: None,
            download_error: None,
            actions: Vec::new(),
            ctx: env.ctx.clone(),
            demo: env.demo,
        }
    }

    fn save_settings(&self) {
        if let Some(d) = &self.dir {
            settings::save(d, &self.settings);
        }
    }

    fn save_history(&self) {
        if let Some(d) = &self.dir {
            settings::save_history(d, &self.history);
        }
    }

    /// Words said today.
    pub fn words_today(&self) -> usize {
        self.history.words_since(today_start())
    }

    pub fn shortcut_label(&self) -> String {
        self.settings.shortcut.as_ref().map(|s| s.display()).unwrap_or_default()
    }

    pub fn listening(&self) -> bool {
        matches!(self.phase, Phase::Listening { .. })
    }

    // ------------------------------------------------------------ shortcuts

    /// Bind the shortcut when it changed, the tool turned on, or recording
    /// a new one ended.
    fn bind_keys(&mut self) {
        let want = if self.on && !self.recording_key { self.settings.shortcut.clone() } else { None };
        if self.keys.is_some() && self.bound == want {
            return;
        }
        if self.keys.is_none() {
            self.keys = Keys::new(self.ctx.clone());
        }
        let Some(keys) = self.keys.as_mut() else {
            self.key_error = Some("Shortcuts are unavailable".into());
            return;
        };
        self.key_error = None;
        match &want {
            Some(s) => {
                if let Some((_, e)) = keys.apply(&[(Cmd::Talk, s.clone())]).into_iter().next() {
                    self.key_error = Some(e);
                }
            }
            None => keys.clear(),
        }
        self.bound = want;
    }

    /// Esc cancels and Return finishes, but only while dictating: they're
    /// every app's keys otherwise.
    fn bind_esc(&mut self) {
        let want = matches!(self.phase, Phase::Listening { .. } | Phase::Working { .. });
        if want == self.esc_bound {
            return;
        }
        if self.esc.is_none() {
            self.esc = Keys::new(self.ctx.clone());
        }
        let Some(esc) = self.esc.as_mut() else { return };
        if want {
            let k = |key: &str| Shortcut { cmd: false, shift: false, alt: false, ctrl: false, key: key.into() };
            esc.apply(&[(Cmd::Cancel, k("Escape")), (Cmd::Done, k("Return"))]);
        } else {
            esc.clear();
        }
        self.esc_bound = want;
    }

    fn release_keys(&mut self) {
        if let Some(k) = self.keys.as_mut() {
            k.clear();
        }
        if let Some(k) = self.esc.as_mut() {
            k.clear();
        }
        self.bound = None;
        self.esc_bound = false;
    }

    // ------------------------------------------------------------ the flow

    fn show(&mut self, note: Note, secs: f32) {
        self.phase = Phase::Showing { note, until: Instant::now() + Duration::from_secs_f32(secs) };
    }

    /// The shortcut went down while idle: check what's needed, find the
    /// caret, start the mic.
    pub fn start(&mut self) {
        self.target = mac::front_app().unwrap_or_default();
        let caret = mac::caret(&self.target);
        let mouse = mac::mouse();
        let at = caret.map(|c| c.center()).or(mouse).unwrap_or(Pos2::new(600.0, 400.0));
        self.place = (caret, mouse, mac::screen_at(at));
        self.levels.clear();
        match Perm::Microphone.status() {
            Status::Allowed => {}
            Status::NotAsked => {
                Perm::Microphone.ask();
                return self.show(Note::Problem("Allow the microphone".into()), 2.5);
            }
            Status::Denied => {
                crate::kit::perms::open_settings(Perm::Microphone);
                return self.show(Note::Problem("Microphone is off for Cmd+Meow".into()), 3.0);
            }
        }
        let engine = self.settings.engine;
        if !engine.ready() {
            self.tab = Tab::Settings;
            self.actions.push(Action::OpenPage { tool: ID.into(), route: "settings".into() });
            if self.download.is_none() {
                self.start_download(engine);
            }
            return self.show(Note::Problem("Getting the speech model".into()), 2.5);
        }
        match Mic::start() {
            Ok(mic) => {
                let _ = self.jobs.send(Job::Warm(engine));
                self.phase = Phase::Listening { mic, since: Instant::now(), tapped: false, heard: Instant::now() };
            }
            Err(e) => self.show(Note::Problem(e), 2.5),
        }
    }

    /// Stop listening and hand the voice to the model.
    fn finish(&mut self) {
        let Phase::Listening { mic, .. } = std::mem::replace(&mut self.phase, Phase::Idle) else { return };
        let pcm = mic.stop();
        let secs = pcm.len() as f32 / MODEL_RATE as f32;
        let peak = pcm.iter().fold(0.0f32, |m, v| m.max(v.abs()));
        eprintln!("[dictation] listened {secs:.1} s, peak {peak:.3}");
        // a real room is never exactly zero: macOS hands out digital silence
        // when it keeps the microphone from us
        if secs >= MIN_VOICE_SECS && peak == 0.0 {
            return self.show(Note::Problem("The microphone gives only silence".into()), 2.5);
        }
        if secs < MIN_VOICE_SECS || audio::silent(&pcm, MODEL_RATE) {
            return self.show(Note::Problem("Didn't hear anything".into()), 1.4);
        }
        self.next_job += 1;
        self.voice_secs = secs;
        let job = Job::Run { job: self.next_job, engine: self.settings.engine, lang: None, pcm };
        if self.jobs.send(job).is_err() {
            return self.show(Note::Problem("Speech to text isn't running".into()), 2.5);
        }
        self.phase = Phase::Working { job: self.next_job };
    }

    fn cancel(&mut self) {
        if let Phase::Listening { mic, .. } = std::mem::replace(&mut self.phase, Phase::Idle) {
            let _ = mic.stop();
        }
        // a Working job's answer is ignored now: phase is Idle
    }

    fn on_heard(&mut self, h: Heard) {
        let Phase::Working { job, .. } = self.phase else { return };
        if job != h.job {
            return;
        }
        match h.text {
            Err(e) => self.show(Note::Problem(e), 3.0),
            Ok(t) if t.is_empty() => self.show(Note::Problem("Didn't catch that".into()), 1.6),
            Ok(t) => {
                if self.settings.keep_history {
                    self.history.add(Said { at: chrono::Local::now().timestamp(), text: t.clone(), app: self.target.name.clone(), secs: self.voice_secs });
                    self.save_history();
                }
                if mac::accessibility() {
                    mac::activate(&self.target);
                    self.paste_at = Some((Instant::now() + PASTE_DELAY, t));
                    self.show(Note::Pasted, 0.8);
                } else {
                    let _ = mac::paste(&t, false);
                    self.show(Note::Copied, 2.5);
                }
            }
        }
    }

    pub fn start_download(&mut self, engine: Engine) {
        if self.download.is_some() || engine.ready() {
            return;
        }
        let progress = Arc::new(Progress::default());
        let unpacking = Arc::new(AtomicBool::new(false));
        let (tx, rx) = channel();
        let (p, u, ctx) = (progress.clone(), unpacking.clone(), self.ctx.clone());
        std::thread::Builder::new()
            .name("dictation-model".into())
            .spawn(move || {
                let r = match engine {
                    Engine::Parakeet => crate::kit::parakeet::download(&p, &u),
                    Engine::Whisper => crate::kit::whisper::download(crate::kit::whisper::Model::Accurate, &p),
                };
                let _ = tx.send(r);
                ctx.request_repaint();
            })
            .ok();
        self.download_error = None;
        self.download = Some(Download { engine, progress, unpacking, rx });
    }

    pub fn stop_download(&mut self) {
        if let Some(d) = &self.download {
            d.progress.cancel.store(true, Ordering::Relaxed);
        }
    }

    fn poll_download(&mut self, ctx: &egui::Context) {
        let Some(d) = &self.download else { return };
        match d.rx.try_recv() {
            Ok(r) => {
                self.download_error = r.err().filter(|e| e != "Stopped");
                self.download = None;
            }
            Err(_) => ctx.request_repaint_after(Duration::from_millis(250)),
        }
    }

    /// Everything that moves the flow on; called on every frame.
    fn step(&mut self, ctx: &egui::Context) {
        self.bind_keys();
        let (presses, releases) = match &self.keys {
            Some(k) => (k.poll(), k.poll_released()),
            None => (Vec::new(), Vec::new()),
        };
        let side = self.esc.as_ref().map(|k| k.poll()).unwrap_or_default();
        let cancels: Vec<_> = side.iter().filter(|c| **c == Cmd::Cancel).collect();
        if side.contains(&Cmd::Done) && matches!(self.phase, Phase::Listening { .. }) {
            self.finish();
        }
        for c in presses {
            if c != Cmd::Talk {
                continue;
            }
            match &self.phase {
                Phase::Idle | Phase::Showing { .. } => self.start(),
                Phase::Listening { tapped: true, .. } => self.finish(),
                _ => {}
            }
        }
        for c in releases {
            if c != Cmd::Talk {
                continue;
            }
            if let Phase::Listening { since, tapped, .. } = &mut self.phase {
                if !*tapped {
                    if since.elapsed() < HOLD {
                        *tapped = true;
                    } else {
                        self.finish();
                    }
                }
            }
        }
        if !cancels.is_empty() {
            self.cancel();
        }
        let stop_after = self.settings.stop_after_silence();
        if let Phase::Listening { mic, since, tapped, heard } = &mut self.phase {
            let failed = mic.failed();
            let level = mic.level();
            if level > audio::VOICE_LEVEL {
                *heard = Instant::now();
            }
            // hands-free (tapped) only: holding the key is its own stop
            let quiet = *tapped && stop_after.is_some_and(|d| heard.elapsed() > d);
            let too_long = since.elapsed() > MAX_TALK || quiet;
            if quiet {
                eprintln!("[dictation] stopped after silence");
            }
            self.levels.push(level);
            if self.levels.len() > 64 {
                self.levels.remove(0);
            }
            if too_long {
                self.finish();
            } else if let Some(e) = failed {
                self.cancel();
                self.show(Note::Problem(e), 2.5);
            }
        }
        while let Ok(h) = self.heard.try_recv() {
            self.on_heard(h);
        }
        if self.paste_at.as_ref().is_some_and(|(t, _)| Instant::now() >= *t) {
            let (_, text) = self.paste_at.take().unwrap();
            if let Some((saved, count)) = mac::paste(&text, true) {
                if self.settings.restore_clipboard {
                    // a restore still pending holds the clipboard from before
                    let saved = self.restore_at.take().map(|(_, old, _)| old).unwrap_or(saved);
                    self.restore_at = Some((Instant::now() + RESTORE_DELAY, saved, count));
                }
            }
        }
        if self.restore_at.as_ref().is_some_and(|(t, _, _)| Instant::now() >= *t) {
            let (_, saved, count) = self.restore_at.take().unwrap();
            mac::restore(saved, count);
        }
        if self.recording_key && self.recording_seen.elapsed() > Duration::from_secs(1) {
            self.recording_key = false;
        }
        if let Phase::Showing { until, .. } = self.phase {
            if Instant::now() >= until {
                self.phase = Phase::Idle;
            }
        }
        self.bind_esc();
        self.poll_download(ctx);
        if !matches!(self.phase, Phase::Idle) || self.paste_at.is_some() || self.restore_at.is_some() {
            ctx.request_repaint_after(Duration::from_millis(30));
        }
    }
}

/// The model thread: keeps one model loaded while it's used, lets it go
/// after a while.
fn worker(rx: Receiver<Job>, tx: Sender<Heard>, ctx: egui::Context) {
    enum Loaded {
        Parakeet(crate::kit::parakeet::Loaded),
        Whisper(crate::kit::whisper::Loaded),
    }
    let mut loaded: Option<(Engine, Loaded)> = None;
    let load = |e: Engine| -> Result<Loaded, String> {
        match e {
            Engine::Parakeet => crate::kit::parakeet::Loaded::load().map(Loaded::Parakeet),
            Engine::Whisper => crate::kit::whisper::Loaded::load(crate::kit::whisper::Model::Accurate).map(Loaded::Whisper),
        }
    };
    loop {
        let job = match rx.recv_timeout(IDLE_UNLOAD) {
            Ok(j) => j,
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {
                loaded = None;
                continue;
            }
            Err(_) => return,
        };
        let engine = match &job {
            Job::Warm(e) | Job::Run { engine: e, .. } => *e,
        };
        if loaded.as_ref().is_none_or(|(e, _)| *e != engine) {
            loaded = None;
            match load(engine) {
                Ok(l) => loaded = Some((engine, l)),
                Err(e) => {
                    eprintln!("[dictation] {e}");
                    if let Job::Run { job, .. } = job {
                        let _ = tx.send(Heard { job, text: Err(e) });
                        ctx.request_repaint();
                    }
                    continue;
                }
            }
        }
        let Job::Run { job, lang, pcm, .. } = job else { continue };
        let t0 = Instant::now();
        let text = match &loaded {
            Some((_, Loaded::Parakeet(m))) => m.run(&pcm).map(|t| audio::tidy_text(&[t])),
            Some((_, Loaded::Whisper(m))) => m.run(&pcm, lang, None).map(|segs| audio::tidy_text(&segs.into_iter().map(|s| s.2).collect::<Vec<_>>())),
            None => Err("The speech model didn't load".into()),
        };
        eprintln!("[dictation] {:.1} s of voice in {} ms", pcm.len() as f32 / MODEL_RATE as f32, t0.elapsed().as_millis());
        let _ = tx.send(Heard { job, text });
        ctx.request_repaint();
    }
}

fn today_start() -> i64 {
    use chrono::{Local, TimeZone};
    let now = Local::now();
    now.date_naive().and_hms_opt(0, 0, 0).and_then(|d| Local.from_local_datetime(&d).single()).map(|d| d.timestamp()).unwrap_or(now.timestamp())
}

fn demo_history() -> History {
    let now = chrono::Local::now().timestamp();
    let mut h = History::default();
    for (ago, text, app) in [
        (3 * 3600, "Ship the dictation widget before Friday, then we test it on the real Mac together.", "Slack"),
        (40 * 60, "Привіт! Скинь, будь ласка, посилання на дизайн, я гляну ввечері.", "Telegram"),
        (12 * 60, "Fix the caret position in Electron apps and fall back to the mouse.", "Linear"),
        (90, "Треба оновити README і додати скріншоти нового вікна.", "Notes"),
    ] {
        h.add(Said { at: now - ago, text: text.into(), app: app.into(), secs: text.len() as f32 / 14.0 });
    }
    h
}

fn badge() -> Badge {
    Badge { icon: Icon::Mic, color: TINT() }
}

impl Tool for Dictation {
    fn id(&self) -> ToolId {
        ID
    }
    fn name(&self) -> &'static str {
        "Dictation"
    }
    fn badge(&self) -> Badge {
        badge()
    }
    fn section(&self) -> &'static str {
        "Create"
    }
    fn about(&self) -> &'static str {
        "Speak and the words are typed where the cursor is"
    }
    fn sidebar_meta(&self) -> String {
        if self.listening() { "Listening".into() } else { self.shortcut_label() }
    }
    fn needs(&self) -> Vec<Need> {
        vec![Need::must(Perm::Microphone), Need::must(Perm::Accessibility), Need::must(Perm::Own { id: MODEL_NEED, name: "Speech model", icon: Icon::Down })]
    }
    fn own_status(&self, id: &str) -> Status {
        if id == MODEL_NEED && self.settings.engine.ready() { Status::Allowed } else { Status::NotAsked }
    }
    fn ask_own(&mut self, id: &str) {
        if id == MODEL_NEED {
            self.start_download(self.settings.engine);
        }
    }
    fn setup_shortcuts(&self) -> Vec<(&'static str, String)> {
        self.settings.shortcut.as_ref().map(|s| vec![("Dictate", s.display())]).unwrap_or_default()
    }
    fn widgets(&self) -> Vec<WidgetSpec> {
        vec![WidgetSpec { id: CARD, title: "Dictation", about: "Your shortcut, words today and the latest dictations.", sizes: &[Size::S11, Size::S14, Size::S24], size: Size::S11, default_on: true }]
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
        ui::page(self, ui, rect, cx);
    }
    fn set_on(&mut self, on: bool) {
        self.on = on;
        if !on {
            self.cancel();
            self.paste_at = None;
            if let Some((_, saved, count)) = self.restore_at.take() {
                mac::restore(saved, count);
            }
            self.recording_key = false;
            self.release_keys();
        }
    }
    fn tick(&mut self, ctx: &egui::Context) -> bool {
        if self.demo {
            return false;
        }
        // a tool left on is never told so; it is on once it ticks
        self.on = true;
        self.step(ctx);
        !matches!(self.phase, Phase::Idle) || !self.actions.is_empty()
    }
    fn windows(&mut self, ctx: &egui::Context, cx: &mut Cx) {
        for a in self.actions.drain(..) {
            cx.push(a);
        }
        ui::pill(self, ctx);
    }
    fn demo_view(&mut self, view: &str) -> Option<View> {
        match view {
            "dictation" => {
                self.tab = Tab::History;
                Some(View::Page)
            }
            "dictation-settings" => {
                self.tab = Tab::Settings;
                Some(View::Page)
            }
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn demo() -> Dictation {
        Dictation::new(&ToolEnv { ctx: egui::Context::default(), demo: true })
    }

    #[test]
    fn sample_data_counts_words_today() {
        let d = demo();
        assert_eq!(d.history.items.len(), 4);
        assert!(d.words_today() <= d.history.items.iter().map(|s| s.text.split_whitespace().count()).sum());
        assert_eq!(d.shortcut_label(), "⌥D");
    }

    #[test]
    fn needs_mic_accessibility_and_the_model() {
        let d = demo();
        let n = d.needs();
        assert!(n.iter().any(|x| x.perm == Perm::Microphone));
        assert!(n.iter().any(|x| x.perm == Perm::Accessibility));
        assert!(n.iter().any(|x| matches!(x.perm, Perm::Own { id: MODEL_NEED, .. })));
    }

    #[test]
    fn an_answer_for_an_old_job_is_ignored() {
        let mut d = demo();
        d.phase = Phase::Working { job: 2 };
        d.on_heard(Heard { job: 1, text: Ok("old".into()) });
        assert!(matches!(d.phase, Phase::Working { job: 2, .. }));
        d.on_heard(Heard { job: 2, text: Ok(String::new()) });
        assert!(matches!(&d.phase, Phase::Showing { note: Note::Problem(_), .. }));
    }
}
