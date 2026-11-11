//! AI usage tool (id "claude" for saved prefs): plan limits and local token
//! use of the main AI coding assistant, Claude Code or Codex, picked on the
//! page. A panel card, a limits sheet, a page in the full window and the
//! session % next to the menu-bar icon. Claude also lists its projects.

use crate::kit::sync::LockExt;
pub mod backend;
mod page;
mod ui;

use crate::core::tool::{Action, Badge, Cx, MenuItem, Recommendation, Size, Tool, ToolEnv, ToolId, View, WidgetSpec, UP_TO_MEDIUM};
use crate::kit::glass::{tint, Icon};
use crate::kit::ai::{self, UsageInfo};
use egui::{Rect, Ui};

pub const ID: ToolId = "claude";
const LIMITS: &str = "limits";

pub struct Claude {
    info: backend::Shared,
    demo: bool,
}

impl Claude {
    pub fn new(env: &ToolEnv) -> Self {
        Claude { info: backend::start(env.ctx.clone(), env.demo), demo: env.demo }
    }
}

fn badge() -> Badge {
    Badge { icon: Icon::Gauge, color: tint::CLAUDE() }
}

impl Tool for Claude {
    fn id(&self) -> ToolId {
        ID
    }
    fn name(&self) -> &'static str {
        "AI Tool"
    }
    fn about(&self) -> &'static str {
        "Plan limits and token use of Claude Code or Codex"
    }
    fn badge(&self) -> Badge {
        badge()
    }
    fn section(&self) -> &'static str {
        "Monitor"
    }
    fn widgets(&self) -> Vec<WidgetSpec> {
        vec![WidgetSpec { id: LIMITS, title: "AI limits", about: "Session and weekly limits of Claude or Codex, tokens today.", sizes: UP_TO_MEDIUM, size: Size::S14, default_on: true }]
    }
    fn widget_ui(&mut self, widget: &str, size: Size, ui: &mut Ui, rect: Rect, cx: &mut Cx) {
        if widget != LIMITS {
            return;
        }
        let cl = self.info.lock_ok().clone();
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
        self.info.lock_ok().session().map(|l| format!("{:.0}%", l.pct)).unwrap_or_default()
    }
    fn page_ui(&mut self, ui: &mut Ui, rect: Rect, cx: &mut Cx) {
        page::page(&self.info, self.demo, ui, rect, cx);
    }
    fn recommendations(&self) -> Vec<Recommendation> {
        let info = self.info.lock_ok();
        let Some(l) = info.session().filter(|l| l.pct >= 80.0) else { return Vec::new() };
        vec![Recommendation {
            priority: 60,
            title: format!("Your {} session is almost used up", info.provider),
            body: format!("{:.0}% of the current 5-hour session is used. It {}.", l.pct, ui::short_reset(&l.resets)),
            action: "See Limits".into(),
            go: Action::ShowDetail { tool: ID.into(), detail: LIMITS.into(), toggle: true },
        }]
    }
    fn tray_title(&self) -> Option<String> {
        self.info.lock_ok().session().map(|l| format!("{:.0}%", l.pct))
    }
    fn menu_items(&self) -> Vec<MenuItem> {
        vec![MenuItem { id: "refresh", label: format!("Refresh {} limits", ai::selected_meta().name) }]
    }
    fn menu_action(&mut self, item: &str, cx: &mut Cx) {
        if item == "refresh" {
            backend::request_refresh();
            cx.toast(format!("Asking {} for your limits…", ai::selected_meta().name));
        }
    }
    fn demo_view(&mut self, view: &str) -> Option<View> {
        match view {
            "claude-detail" => Some(View::Detail(LIMITS)),
            "claude" => Some(View::Page),
            "codex" | "codex-detail" => {
                backend::select(&self.info, true, ai::index_of("codex"));
                Some(if view == "codex" { View::Page } else { View::Detail(LIMITS) })
            }
            "claude-missing" => {
                let error = Some(format!("Claude Code isn't installed on this Mac. {}", crate::kit::claude_code::Availability::NotInstalled.how()));
                *self.info.lock_ok() = UsageInfo { provider: ai::claude::NAME, loading: false, installed: false, fetched: Some(chrono::Local::now()), error, ..Default::default() };
                Some(View::Page)
            }
            _ => None,
        }
    }
}
