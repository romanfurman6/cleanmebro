//! Claude tool: plan and limits from the Claude CLI, local token activity and
//! the Claude Code projects on this Mac. A panel card, a limits sheet, a page
//! in the full window and the session % next to the menu-bar icon.

pub mod backend;
mod page;
mod ui;

use crate::core::tool::{Action, Badge, Cx, MenuItem, Recommendation, Tool, ToolEnv, ToolId, View, WidgetSpec};
use crate::kit::glass::{tint, Icon};
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
    Badge { icon: Icon::Sparkle, color: tint::CLAUDE }
}

impl Tool for Claude {
    fn id(&self) -> ToolId {
        ID
    }
    fn name(&self) -> &'static str {
        "Claude"
    }
    fn badge(&self) -> Badge {
        badge()
    }
    fn section(&self) -> &'static str {
        "Monitor"
    }
    fn widgets(&self) -> Vec<WidgetSpec> {
        vec![WidgetSpec { id: LIMITS, title: "Claude limits", span: 4, default_on: true, group: None }]
    }
    fn widget_height(&self, _widget: &str, _width: f32) -> f32 {
        ui::CARD_H
    }
    fn widget_ui(&mut self, widget: &str, ui: &mut Ui, rect: Rect, cx: &mut Cx) {
        if widget != LIMITS {
            return;
        }
        let resp = crate::kit::glass::card(ui, rect, ui.scope_id().with("c-claude"), crate::kit::glass::R_CARD, cx.is_detail_open(ID, LIMITS));
        let cl = self.info.lock().unwrap().clone();
        ui::card(ui.painter(), rect, &cl, ui.ctx().input(|i| i.time));
        if resp.clicked() {
            cx.toggle_detail(ID, LIMITS);
        }
    }
    fn detail_ui(&mut self, _detail: &str, ui: &mut Ui, rect: Rect, cx: &mut Cx) {
        ui::detail(&self.info, ui, rect, cx);
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
            _ => None,
        }
    }
}
