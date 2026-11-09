//! Cleanup tool: finds junk by rule, lets Claude judge each path, and moves
//! what you approve to the Trash. Lives in the full window.
//!
//! Page routes: "" (as left), "claude" (open on the Claude group).

pub mod engine;
mod page;

use crate::core::tool::{Action, Badge, Cx, Recommendation, Tool, ToolEnv, ToolId, View, WidgetSpec};
use crate::kit::glass::{human_bytes, quick_tile, tint, Icon, QUICK_TILE_H};
use egui::{Rect, Ui};
use engine::Group;

pub const ID: ToolId = "cleanup";

pub struct Cleanup {
    pub(crate) clean: engine::Shared,
    pub(crate) demo: bool,
    pub(crate) use_ai: bool,
    pub(crate) delete_now: bool,
    /// Group shown in the results; `None` = "Needs your OK".
    pub(crate) clean_group: Option<Group>,
    pub(crate) clean_confirm: bool,
    pub(crate) clean_log: bool,
    /// Screenshots: show Cleanup as if Claude were in this state.
    pub(crate) demo_claude: Option<crate::kit::claude_code::Availability>,
}

impl Cleanup {
    pub fn new(env: &ToolEnv) -> Self {
        Cleanup {
            clean: Default::default(),
            demo: env.demo,
            use_ai: true,
            delete_now: false,
            clean_group: Some(Group::Claude),
            clean_confirm: false,
            clean_log: false,
            demo_claude: None,
        }
    }
}

fn badge() -> Badge {
    Badge { icon: Icon::Broom, color: tint::CLEANUP }
}

impl Tool for Cleanup {
    fn id(&self) -> ToolId {
        ID
    }
    fn name(&self) -> &'static str {
        "Cleanup"
    }
    fn about(&self) -> &'static str {
        "Safe junk removal, checked by Claude when it's on"
    }
    fn badge(&self) -> Badge {
        badge()
    }
    fn section(&self) -> &'static str {
        "Clean"
    }
    fn widgets(&self) -> Vec<WidgetSpec> {
        vec![WidgetSpec { id: "clean", title: "Clean (quick action)", span: 1, default_on: true, group: None }]
    }
    fn widget_height(&self, _widget: &str, _width: f32) -> f32 {
        QUICK_TILE_H
    }
    fn widget_ui(&mut self, _widget: &str, ui: &mut Ui, rect: Rect, cx: &mut Cx) {
        let sub = {
            let g = self.clean.lock().unwrap();
            match g.phase {
                engine::Phase::Results => human_bytes(g.total()),
                engine::Phase::Scanning | engine::Phase::Judging => "Scanning…".into(),
                engine::Phase::Cleaning => "Cleaning…".into(),
                _ => "Scan".into(),
            }
        };
        if quick_tile(ui, rect, ui.scope_id().with("q-clean"), Icon::Broom, tint::CLEANUP, "Clean", &sub, true).clicked() {
            cx.open_page(ID, "");
        }
    }
    fn has_page(&self) -> bool {
        true
    }
    fn open_page(&mut self, route: &str) {
        if route == "claude" {
            self.clean_group = Some(Group::Claude);
        }
    }
    fn sidebar_meta(&self) -> String {
        let g = self.clean.lock().unwrap();
        if g.phase == engine::Phase::Results { human_bytes(g.total()) } else { String::new() }
    }
    fn page_ui(&mut self, ui: &mut Ui, rect: Rect, cx: &mut Cx) {
        page::page(self, ui, rect, cx);
    }
    fn recommendations(&self) -> Vec<Recommendation> {
        let blue = tint::CLEANUP;
        let go = Action::OpenPage { tool: ID.into(), route: String::new() };
        // always on offer, at the lowest priority
        let mut v = vec![Recommendation {
            priority: 0,
            badge: Badge { icon: Icon::Broom, color: blue },
            title: "Let's give your Mac a quick clean".into(),
            body: if self.demo || crate::kit::claude_code::availability().ready() {
                "Claude checks each cache and log path and tells you what's safe to remove.".into()
            } else {
                "Finds caches, logs and leftovers and tells you what's safe to remove.".into()
            },
            action: "Start Cleanup".into(),
            go: go.clone(),
        }];
        let claude_junk = self.clean.lock().unwrap().claude_safe_bytes();
        if claude_junk > 50_000_000 {
            v.push(Recommendation {
                priority: 4,
                badge: Badge { icon: Icon::Broom, color: blue },
                title: "Claude caches can be trimmed".into(),
                body: format!("{} of old Claude caches, logs and versions are safe to remove.", human_bytes(claude_junk)),
                action: "Review Cleanup".into(),
                go: Action::OpenPage { tool: ID.into(), route: "claude".into() },
            });
        }
        v
    }
    fn demo_view(&mut self, view: &str) -> Option<View> {
        match view {
            "cleanup" => Some(View::Page),
            "cleanup-no-claude" => {
                self.demo_claude = Some(crate::kit::claude_code::Availability::NotInstalled);
                Some(View::Page)
            }
            "results" => {
                *self.clean.lock().unwrap() = page::demo_results();
                Some(View::Page)
            }
            _ => None,
        }
    }
}

/// Read-only check of the Cleanup rules on this Mac: scans, optionally asks
/// Claude, prints what it would offer. Nothing is removed.
pub fn scan_report(ai: bool) {
    let state: engine::Shared = Default::default();
    engine::start_scan(state.clone(), egui::Context::default(), ai);
    loop {
        std::thread::sleep(std::time::Duration::from_millis(300));
        if state.lock().unwrap().phase == engine::Phase::Results {
            break;
        }
    }
    let g = state.lock().unwrap();
    let home = dirs::home_dir().unwrap_or_default().display().to_string();
    for it in &g.items {
        println!(
            "{:<12} {:>10} {:<14} {:<5} {} | {}",
            format!("{:?}", it.group),
            human_bytes(it.bytes),
            format!("{:?}", it.verdict),
            format!("{:?}", it.by),
            it.path.display().to_string().replacen(&home, "~", 1),
            it.reason
        );
    }
    println!("total offered {} · ai used {} · ai error {:?}", human_bytes(g.total()), g.ai_used, g.ai_error);
}
