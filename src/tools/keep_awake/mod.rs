//! Keep Awake: keeps the Mac from sleeping, by hand with one switch, or by
//! itself while Claude Code is working (and a few minutes after, so a pause
//! between prompts doesn't put the Mac to sleep). Uses macOS power
//! assertions, the same thing `caffeinate` uses, without a helper process.
//!
//! Page routes: "" only.

mod engine;
mod power;
mod sessions;
mod ui;

use crate::core::tool::{Badge, Cx, MenuItem, Size, Tool, ToolEnv, ToolId, View, WidgetSpec, UP_TO_MEDIUM};
use crate::kit::glass::{tint, Icon};
use egui::{Rect, Ui};
use engine::{now, Engine, State};
use std::sync::{Arc, Mutex};
use std::time::Duration;

pub const ID: ToolId = "keep_awake";
const CARD: &str = "card";
const MENU_TOGGLE: &str = "toggle";

/// How often Claude Code's logs are checked and the state is brought up to date.
const POLL: Duration = Duration::from_secs(5);

type Shared = Arc<Mutex<Engine>>;

pub struct KeepAwake {
    engine: Shared,
    /// Sample data for screenshots: the clock is frozen at this time.
    demo_now: Option<i64>,
}

impl KeepAwake {
    pub fn new(env: &ToolEnv) -> Self {
        if env.demo {
            let (engine, t) = demo_engine(State::Claude);
            return KeepAwake { engine: Arc::new(Mutex::new(engine)), demo_now: Some(t) };
        }
        let engine: Shared = Arc::new(Mutex::new(Engine::new(Some(engine::data_dir()))));
        let shared = engine.clone();
        let ctx = env.ctx.clone();
        std::thread::Builder::new()
            .name("keep-awake".into())
            .spawn(move || watch(shared, ctx))
            .ok();
        KeepAwake { engine, demo_now: None }
    }

    fn now(&self) -> i64 {
        self.demo_now.unwrap_or_else(now)
    }

    fn state(&self) -> State {
        self.engine.lock().unwrap().state(self.now())
    }

    pub fn toggle(&mut self) {
        let t = self.now();
        let mut e = self.engine.lock().unwrap();
        let on = e.manual_since.is_none();
        e.set_manual(on, t);
    }
}

/// The background loop: read Claude Code's logs and keep the assertions in
/// line, also while every window is hidden.
fn watch(engine: Shared, ctx: egui::Context) {
    crate::kit::activity::low_priority();
    let mut scanner = sessions::Scanner::default();
    let root = crate::kit::claude_code::projects_dir();
    loop {
        let t = now();
        let watch_claude = engine.lock().unwrap().settings.claude;
        let found = if watch_claude { scanner.scan(&root, t) } else { Vec::new() };
        {
            let mut e = engine.lock().unwrap();
            let before = (e.state(t), e.sessions.clone());
            e.sessions = found;
            e.step(t);
            if before != (e.state(t), e.sessions.clone()) {
                crate::kit::activity::repaint(&ctx);
            }
        }
        std::thread::sleep(POLL);
    }
}

fn badge() -> Badge {
    Badge { icon: Icon::Cup, color: tint::KEEP_AWAKE }
}

/// Sample data for screenshots, showing `state`.
fn demo_engine(state: State) -> (Engine, i64) {
    use sessions::Session;
    let t = engine::today_start(now()) + 13 * 3600 + 7 * 60;
    let mut e = Engine::new(None);
    let h = |x: f64| t - ((13.0 + 7.0 / 60.0 - x) * 3600.0) as i64;
    e.log = vec![
        engine::Span { start: h(9.2), end: h(9.9), claude: false },
        engine::Span { start: h(10.7), end: h(11.3), claude: true },
        engine::Span { start: h(11.3), end: h(11.9), claude: false },
    ];
    let idle = Session { project: "notes".into(), last_write: t - 3 * 3600 };
    match state {
        State::Off => {
            e.sessions = vec![idle];
            e.step(t);
        }
        State::Manual => {
            e.sessions = vec![idle];
            e.set_manual(true, t - 25 * 60);
        }
        State::Claude => {
            e.sessions = vec![
                Session { project: "cmdmeow".into(), last_write: h(11.9) },
                Session { project: "api-server".into(), last_write: h(11.9) },
                idle.clone(),
            ];
            e.step(h(11.9));
            e.sessions[0].last_write = t - 12;
            e.sessions[1].last_write = t - 48;
        }
        State::WindDown { .. } => {
            e.sessions = vec![Session { project: "cmdmeow".into(), last_write: t - 300 }, idle];
            e.step(t - 300);
        }
    }
    e.step(t);
    (e, t)
}

impl Tool for KeepAwake {
    fn id(&self) -> ToolId {
        ID
    }
    fn name(&self) -> &'static str {
        "Keep Awake"
    }
    fn badge(&self) -> Badge {
        badge()
    }
    fn section(&self) -> &'static str {
        "Utilities"
    }
    fn about(&self) -> &'static str {
        "Keep the Mac awake, or only while Claude Code works"
    }
    fn sidebar_meta(&self) -> String {
        ui::short_state(self.state()).into()
    }
    fn widgets(&self) -> Vec<WidgetSpec> {
        vec![WidgetSpec { id: CARD, title: "Keep Awake", about: "The switch, and what keeps your Mac up.", sizes: UP_TO_MEDIUM, size: Size::S11, default_on: true }]
    }
    fn widget_ui(&mut self, _widget: &str, size: Size, ui: &mut Ui, rect: Rect, cx: &mut Cx) {
        ui::widget(self, size, ui, rect, cx);
    }
    fn has_page(&self) -> bool {
        true
    }
    fn page_ui(&mut self, ui: &mut Ui, rect: Rect, _cx: &mut Cx) {
        ui::page(self, ui, rect);
    }
    fn menu_items(&self) -> Vec<MenuItem> {
        let on = self.engine.lock().unwrap().manual_since.is_some();
        vec![MenuItem { id: MENU_TOGGLE, label: if on { "Let the Mac sleep" } else { "Keep the Mac awake" }.into() }]
    }
    fn menu_action(&mut self, item: &str, _cx: &mut Cx) {
        if item == MENU_TOGGLE {
            self.toggle();
        }
    }
    fn set_on(&mut self, on: bool) {
        let t = self.now();
        self.engine.lock().unwrap().set_enabled(on, t);
    }
    fn tick(&mut self, ctx: &egui::Context) -> bool {
        // steam and countdowns move while awake and on screen; the watcher
        // thread does the rest (hidden, these frames would only burn CPU)
        if crate::kit::activity::visible() && self.state().awake() {
            crate::kit::glass::animate(ctx);
        }
        false
    }
    fn demo_view(&mut self, view: &str) -> Option<View> {
        let state = match view {
            "awake" | "awake-claude" => State::Claude,
            "awake-off" => State::Off,
            "awake-manual" => State::Manual,
            "awake-winddown" => State::WindDown { left: 0 },
            _ => return None,
        };
        let (e, t) = demo_engine(state);
        *self.engine.lock().unwrap() = e;
        self.demo_now = Some(t);
        Some(View::Page)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn demo_states_show_what_they_say() {
        for (want, matches) in [
            (State::Off, (|s: State| s == State::Off) as fn(State) -> bool),
            (State::Manual, |s| s == State::Manual),
            (State::Claude, |s| s == State::Claude),
            (State::WindDown { left: 0 }, |s| matches!(s, State::WindDown { .. })),
        ] {
            let (e, t) = demo_engine(want);
            assert!(matches(e.state(t)), "{want:?} → {:?}", e.state(t));
        }
    }
}
