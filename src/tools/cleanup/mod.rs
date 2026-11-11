//! Cleanup tool: finds junk by rule, lets Claude judge each path, and moves
//! what you approve to the Trash. Lives in the full window.
//!
//! Page routes: "" (as left), "claude" (open on the Claude group).

pub mod engine;
mod page;

use crate::core::tool::{Action, Badge, Cx, Recommendation, Size, Tool, ToolEnv, ToolId, View, WidgetSpec};
use crate::kit::glass::{human_bytes, tint, Icon, ACCENT};
use crate::kit::tile;
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
    fn needs(&self) -> Vec<crate::kit::perms::Need> {
        use crate::kit::perms::{Need, Perm};
        vec![Need::must(Perm::FullDisk)]
    }

    fn section(&self) -> &'static str {
        "Clean"
    }
    fn widgets(&self) -> Vec<WidgetSpec> {
        vec![WidgetSpec { id: "clean", title: "Clean", about: "What the last clean removed, and a scan.", sizes: &[Size::S12, Size::S14], size: Size::S12, default_on: true }]
    }
    fn widget_ui(&mut self, _widget: &str, size: Size, ui: &mut Ui, rect: Rect, cx: &mut Cx) {
        let g = self.clean.lock().unwrap().clone();
        let (value, sub, frac) = match g.phase {
            engine::Phase::Results => (human_bytes(g.total()), "found · ready to review".to_string(), None),
            engine::Phase::Scanning | engine::Phase::Judging => ("Scanning…".into(), g.current.clone(), Some(g.progress)),
            engine::Phase::Cleaning => ("Cleaning…".into(), g.current.clone(), Some(g.progress)),
            engine::Phase::Done => (human_bytes(g.cleaned_bytes), "cleaned".into(), None),
            engine::Phase::Intro => ("Scan".into(), String::new(), None),
        };
        // before a scan: what past cleans removed (the disk itself is Storage's)
        let hist = if g.phase == engine::Phase::Intro { engine::history() } else { Vec::new() };
        let last = hist.last().copied();
        let freed: u64 = hist.iter().map(|h| h.1).sum();
        let intro = g.phase == engine::Phase::Intro;
        let (value, sub) = match last {
            Some(_) if intro => (human_bytes(freed), "freed so far".to_string()),
            None if intro && size != Size::S12 => ("Not cleaned".to_string(), String::new()),
            _ => (value, sub),
        };
        let id = ui.scope_id().with("w-clean");
        let col = tint::CLEANUP;
        let bar = frac.map(|f| (f.clamp(0.01, 1.0), ACCENT)).ok_or(sub.as_str());
        let resp = match size {
            Size::S12 => tile::wide(ui, rect, id, Icon::Broom, col, "Clean", &value, bar, false),
            _ => {
                let (resp, right) = tile::row_split(ui, rect, id, Icon::Broom, col, "Clean", &value, &sub, false);
                let p = ui.painter();
                let lines: Vec<(String, Option<(f32, egui::Color32)>, String)> = match g.phase {
                    engine::Phase::Results => {
                        let tot = g.total().max(1) as f32;
                        let mut groups: Vec<(engine::Group, u64)> = engine::Group::ALL.iter().map(|k| (*k, g.group_bytes(*k))).filter(|(_, b)| *b > 0).collect();
                        groups.sort_by_key(|g| std::cmp::Reverse(g.1));
                        groups.iter().take(3).map(|(grp, b)| (grp.title().to_string(), Some(((*b as f32 / tot).clamp(0.01, 1.0), ACCENT)), human_bytes(*b))).collect()
                    }
                    engine::Phase::Scanning | engine::Phase::Judging | engine::Phase::Cleaning => vec![("Progress".into(), frac.map(|f| (f.clamp(0.01, 1.0), ACCENT)), format!("{:.0}%", frac.unwrap_or(0.0) * 100.0))],
                    engine::Phase::Intro if !hist.is_empty() => vec![
                        ("Last clean".into(), None, last.map(|l| engine::days_ago(l.0)).unwrap_or_default()),
                        ("It freed".into(), None, last.map(|l| human_bytes(l.1)).unwrap_or_default()),
                    ],
                    _ => Vec::new(),
                };
                if g.phase == engine::Phase::Intro && hist.is_empty() {
                    // never cleaned: the right half is the way in
                    let bw = crate::kit::glass::button_width(p, "Scan", crate::kit::glass::BTN_PRIMARY);
                    let br = Rect::from_center_size(egui::pos2(right.right() - bw / 2.0, right.center().y), egui::vec2(bw, 28.0));
                    if crate::kit::glass::pill_button(ui, br, "Scan", crate::kit::glass::BTN_PRIMARY, true).clicked() {
                        cx.open_page(ID, "");
                    }
                } else {
                    let empty = if g.phase == engine::Phase::Done { "All clean" } else { "" };
                    tile::lines(p, right, &lines, empty);
                }
                resp
            }
        };
        if resp.clicked() {
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
        let go = Action::OpenPage { tool: ID.into(), route: String::new() };
        // always on offer, at the lowest priority
        let mut v = vec![Recommendation {
            priority: 0,
            title: "Clean caches and logs".into(),
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
            "cleanup-done" => {
                *self.clean.lock().unwrap() = page::demo_done();
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
