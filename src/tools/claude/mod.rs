//! Claude tool: plan and limits from the Claude CLI, local token activity and
//! the Claude Code projects on this Mac. A panel card, a limits sheet, a page
//! in the full window and the session % next to the menu-bar icon.

pub mod backend;
mod page;
mod ui;

use crate::core::tool::{Action, Badge, Cx, MenuItem, Recommendation, Size, Tool, ToolEnv, ToolId, View, WidgetSpec, UP_TO_MEDIUM};
use crate::kit::glass::{tint, Icon};
use crate::kit::ai::claude::{self, ClaudeCode};
use crate::kit::ai::UsageInfo;
use egui::{Rect, Ui};

pub const ID: ToolId = "claude";
const LIMITS: &str = "limits";

pub struct Claude {
    info: backend::Shared,
    demo: bool,
}

impl Claude {
    pub fn new(env: &ToolEnv) -> Self {
        Claude { info: backend::start(env.ctx.clone(), env.demo, Box::new(ClaudeCode::default())), demo: env.demo }
    }
}

fn badge() -> Badge {
    Badge { icon: Icon::Gauge, color: tint::CLAUDE }
}

impl Tool for Claude {
    fn id(&self) -> ToolId {
        ID
    }
    fn name(&self) -> &'static str {
        "Claude"
    }
    fn about(&self) -> &'static str {
        "Plan limits and local token use"
    }
    fn badge(&self) -> Badge {
        badge()
    }
    fn section(&self) -> &'static str {
        "Monitor"
    }
    fn widgets(&self) -> Vec<WidgetSpec> {
        vec![WidgetSpec { id: LIMITS, title: "Claude limits", about: "Session and weekly limits, tokens today.", sizes: UP_TO_MEDIUM, size: Size::S14, default_on: true }]
    }
    fn widget_ui(&mut self, widget: &str, size: Size, ui: &mut Ui, rect: Rect, cx: &mut Cx) {
        if widget != LIMITS {
            return;
        }
        let cl = self.info.lock().unwrap().clone();
        if ui::widget(ui, rect, size, &cl, cx.is_detail_open(ID, LIMITS)).clicked() {
            cx.toggle_detail(ID, LIMITS);
        }
    }
    fn detail_ui(&mut self, _detail: &str, ui: &mut Ui, rect: Rect, cx: &mut Cx) {
        ui::detail(&self.info, ui, rect, cx);
    }
    fn set_on(&mut self, on: bool) {
        crate::kit::claude_code::set_tool_on(on);
        if on {
            backend::request_refresh();
        }
    }
    fn has_page(&self) -> bool {
        true
    }
    fn sidebar_meta(&self) -> String {
        self.info.lock().unwrap().session().map(|l| format!("{:.0}%", l.pct)).unwrap_or_default()
    }
    fn page_ui(&mut self, ui: &mut Ui, rect: Rect, cx: &mut Cx) {
        page::page(&self.info, self.demo, ui, rect, cx);
    }
    fn recommendations(&self) -> Vec<Recommendation> {
        let info = self.info.lock().unwrap();
        let Some(l) = info.session().filter(|l| l.pct >= 80.0) else { return Vec::new() };
        vec![Recommendation {
            priority: 60,
            badge: badge(),
            title: "Your Claude session is almost used up".into(),
            body: format!("{:.0}% of the current 5-hour session is used. It {}.", l.pct, ui::short_reset(&l.resets)),
            action: "See Limits".into(),
            go: Action::ShowDetail { tool: ID.into(), detail: LIMITS.into(), toggle: true },
        }]
    }
    fn tray_title(&self) -> Option<String> {
        self.info.lock().unwrap().session().map(|l| format!("{:.0}%", l.pct))
    }
    fn menu_items(&self) -> Vec<MenuItem> {
        vec![MenuItem { id: "refresh", label: "Refresh Claude limits".into() }]
    }
    fn menu_action(&mut self, item: &str, cx: &mut Cx) {
        if item == "refresh" {
            backend::request_refresh();
            cx.toast("Asking Claude for your limits…");
        }
    }
    fn demo_view(&mut self, view: &str) -> Option<View> {
        match view {
            "claude-detail" => Some(View::Detail(LIMITS)),
            "claude" => Some(View::Page),
            "claude-missing" => {
                let error = Some(format!("Claude Code isn't installed on this Mac. {}", crate::kit::claude_code::Availability::NotInstalled.how()));
                *self.info.lock().unwrap() = UsageInfo { provider: claude::NAME, loading: false, installed: false, fetched: Some(chrono::Local::now()), error, ..Default::default() };
                Some(View::Page)
            }
            _ => None,
        }
    }
}
