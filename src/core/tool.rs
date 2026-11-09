//! The contract between the shell and a tool. A tool is a self-contained
//! module (Mac health, Claude limits, Cleanup, Disk Map, …) that offers any
//! of: widgets for the menu-bar panel, detail sheets beside the panel, a page
//! in the full window, recommendations, menu entries and a tray title.
//!
//! Tools never import each other. To reach another tool they ask the shell
//! through [`Cx`] by the other tool's id (`cx.open_page("cleanup", "")`); if
//! that tool isn't installed the request is ignored.

use crate::kit::glass::Icon;
use egui::{Color32, Rect, Ui};

/// Stable id of a tool, used in settings keys and cross-tool requests.
pub type ToolId = &'static str;

/// Number of columns in the panel's widget grid.
pub const GRID_COLUMNS: u8 = 4;

/// What a tool needs to start: the egui context (for repaints from its
/// background threads) and whether it should show sample data.
pub struct ToolEnv {
    pub ctx: egui::Context,
    pub demo: bool,
}

/// The tool's icon and its one tint (sidebar chip, widget settings,
/// recommendation banner).
#[derive(Clone, Copy)]
pub struct Badge {
    pub icon: Icon,
    pub color: Color32,
}

/// A widget a tool offers for the panel.
#[derive(Clone, Copy, Debug)]
pub struct WidgetSpec {
    pub id: &'static str,
    /// Name shown in the widget gallery (Settings › Widgets).
    pub title: &'static str,
    /// Width in grid columns out of [`GRID_COLUMNS`]: 1 quarter, 2 half, 4 full.
    pub span: u8,
    /// Shown until the user changes the panel.
    pub default_on: bool,
    /// Widgets of the same group that sit next to each other share one card
    /// (full-width rows such as the Mac metrics).
    pub group: Option<&'static str>,
}

/// The panel's title line: the first tool that returns one wins.
pub struct PanelHeader {
    pub title: String,
    pub status: String,
    pub dot: Color32,
}

/// A suggestion for the panel's "Today's Recommendation". The highest
/// priority across all tools wins.
pub struct Recommendation {
    pub priority: u32,
    pub badge: Badge,
    pub title: String,
    pub body: String,
    /// Button label.
    pub action: String,
    /// What the button does.
    pub go: Action,
}

/// An entry a tool adds to the panel's gear menu.
pub struct MenuItem {
    pub id: &'static str,
    pub label: String,
}

/// Requests from a tool to the shell.
#[derive(Clone, Debug, PartialEq)]
pub enum Action {
    /// Open the full window on a tool's page. `route` is defined by that tool
    /// ("" = its default view).
    OpenPage { tool: String, route: String },
    /// Show a tool's detail sheet beside the panel. With `toggle`, a second
    /// request for the open sheet closes it.
    ShowDetail { tool: String, detail: String, toggle: bool },
    CloseDetail,
    /// Hide the menu-bar panel (e.g. before a screenshot of the screen).
    HidePanel,
    Toast(String),
    Quit,
}

/// What a `--view` name asks the shell to show (screenshots).
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum View {
    Detail(&'static str),
    Page,
}

/// Per-frame context handed to tools: what's open plus an outbox of [`Action`]s.
pub struct Cx {
    open_detail: Option<(String, String)>,
    actions: Vec<Action>,
}

impl Cx {
    pub fn new(open_detail: Option<(String, String)>) -> Self {
        Cx { open_detail, actions: Vec::new() }
    }
    pub fn open_page(&mut self, tool: &str, route: &str) {
        self.actions.push(Action::OpenPage { tool: tool.into(), route: route.into() });
    }
    /// Open or close (if already open) a detail sheet.
    pub fn toggle_detail(&mut self, tool: &str, detail: &str) {
        self.actions.push(Action::ShowDetail { tool: tool.into(), detail: detail.into(), toggle: true });
    }
    pub fn close_detail(&mut self) {
        self.actions.push(Action::CloseDetail);
    }
    pub fn toast(&mut self, msg: impl Into<String>) {
        self.actions.push(Action::Toast(msg.into()));
    }
    pub fn quit(&mut self) {
        self.actions.push(Action::Quit);
    }
    pub fn push(&mut self, a: Action) {
        self.actions.push(a);
    }
    /// Whether this tool's detail sheet `detail` is the one showing.
    pub fn is_detail_open(&self, tool: &str, detail: &str) -> bool {
        self.open_detail.as_ref().is_some_and(|(t, d)| t == tool && d == detail)
    }
    pub fn take_actions(&mut self) -> Vec<Action> {
        std::mem::take(&mut self.actions)
    }
}

/// A tool. Every method except identity has a do-nothing default, so a tool
/// implements only what it offers.
pub trait Tool {
    fn id(&self) -> ToolId;
    fn name(&self) -> &'static str;
    fn badge(&self) -> Badge;
    /// Sidebar group, e.g. "Clean" or "Monitor".
    fn section(&self) -> &'static str {
        "Tools"
    }
    /// One line on what the tool does, for Settings › Tools.
    fn about(&self) -> &'static str {
        ""
    }
    /// Short status beside the tool in the window sidebar ("18.3 GB", "19%").
    fn sidebar_meta(&self) -> String {
        String::new()
    }

    // ---- panel widgets
    fn panel_header(&self) -> Option<PanelHeader> {
        None
    }
    fn widgets(&self) -> Vec<WidgetSpec> {
        Vec::new()
    }
    /// Height of a widget at the given width.
    fn widget_height(&self, _widget: &str, _width: f32) -> f32 {
        0.0
    }
    fn widget_ui(&mut self, _widget: &str, _ui: &mut Ui, _rect: Rect, _cx: &mut Cx) {}

    // ---- detail sheets beside the panel
    /// Draws the whole sheet, header included (see `glass::sheet_header`).
    fn detail_ui(&mut self, _detail: &str, _ui: &mut Ui, _rect: Rect, _cx: &mut Cx) {}

    // ---- full-window page
    fn has_page(&self) -> bool {
        false
    }
    /// Called before the page is shown through a request; `route` is the
    /// tool-defined deep link.
    fn open_page(&mut self, _route: &str) {}
    fn page_ui(&mut self, _ui: &mut Ui, _rect: Rect, _cx: &mut Cx) {}

    // ---- shell hooks
    fn recommendations(&self) -> Vec<Recommendation> {
        Vec::new()
    }
    /// Text next to the menu-bar icon. Titles of all tools are shown, in tool order.
    fn tray_title(&self) -> Option<String> {
        None
    }
    fn menu_items(&self) -> Vec<MenuItem> {
        Vec::new()
    }
    fn menu_action(&mut self, _item: &str, _cx: &mut Cx) {}

    // ---- background
    /// Called on every frame, and also while every shell window is hidden
    /// (then without a ui pass): poll background work, global shortcuts and
    /// so on. Return true while the tool needs frames with a ui pass even
    /// though nothing of the shell is showing, e.g. for its own windows.
    fn tick(&mut self, _ctx: &egui::Context) -> bool {
        false
    }
    /// The user turned the tool on or off in Settings (also called at launch
    /// for tools left off). Off, `tick` and `windows` aren't called; release
    /// anything system-wide, such as global shortcuts.
    fn set_on(&mut self, _on: bool) {}
    /// Called on every frame with a ui pass: show the tool's own extra
    /// windows (`ctx.show_viewport_immediate`), whatever the shell shows.
    fn windows(&mut self, _ctx: &egui::Context, _cx: &mut Cx) {}
    /// Handle a `--view` name for screenshots: load sample data and say what to show.
    fn demo_view(&mut self, _view: &str) -> Option<View> {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cx_collects_actions_in_order() {
        let mut cx = Cx::new(Some(("system".into(), "memory".into())));
        assert!(cx.is_detail_open("system", "memory"));
        assert!(!cx.is_detail_open("system", "cpu"));
        assert!(!cx.is_detail_open("claude", "memory"));
        cx.open_page("cleanup", "claude");
        cx.toast("hi");
        cx.toggle_detail("system", "cpu");
        let a = cx.take_actions();
        assert_eq!(a.len(), 3);
        assert_eq!(a[0], Action::OpenPage { tool: "cleanup".into(), route: "claude".into() });
        assert_eq!(a[1], Action::Toast("hi".into()));
        assert_eq!(a[2], Action::ShowDetail { tool: "system".into(), detail: "cpu".into(), toggle: true });
        assert!(cx.take_actions().is_empty());
    }
}
