//! Capture shows as two tools in the Create section: Screenshot and Video.
//! Both are views of one [`Capture`]: one engine, one settings file, one
//! set of windows. Whichever is on drives the shared ticks and windows
//! (Screenshot when it's on), so nothing runs twice.

use super::{settings::Command, ui, Capture, VTab, CARD, ID, VIDEO_CARD, VIDEO_ID};
use crate::core::tool::{Badge, Cx, Recommendation, Size, Tool, ToolEnv, ToolId, View, WidgetSpec, UP_TO_MEDIUM};
use crate::kit::glass::{tint, Icon};
use egui::{Rect, Ui};
use std::cell::RefCell;
use std::rc::Rc;

pub struct Screenshot(Rc<RefCell<Capture>>);
pub struct Video(Rc<RefCell<Capture>>);

/// The two tools over one shared Capture.
pub fn tools(env: &ToolEnv) -> (Screenshot, Video) {
    let c = Rc::new(RefCell::new(Capture::new(env)));
    (Screenshot(c.clone()), Video(c))
}

fn key(c: &Capture, cmd: Command) -> String {
    c.settings.shortcut(cmd).map(|s| s.display()).unwrap_or_default()
}

impl Tool for Screenshot {
    fn id(&self) -> ToolId {
        ID
    }
    fn name(&self) -> &'static str {
        "Screenshot"
    }
    fn badge(&self) -> Badge {
        Badge { icon: Icon::Crop, color: tint::CAPTURE() }
    }
    fn section(&self) -> &'static str {
        "Create"
    }
    fn about(&self) -> &'static str {
        "Screenshots, markup and text from the screen"
    }
    fn needs(&self) -> Vec<crate::kit::perms::Need> {
        use crate::kit::perms::{Need, Perm};
        vec![Need::must(Perm::Screen)]
    }
    fn setup_shortcuts(&self) -> Vec<(&'static str, String)> {
        let c = self.0.borrow();
        c.settings.shortcut(Command::Area).map(|s| vec![("Capture area", s.display())]).unwrap_or_default()
    }
    fn sidebar_meta(&self) -> String {
        key(&self.0.borrow(), Command::Area)
    }
    fn set_on(&mut self, on: bool) {
        self.0.borrow_mut().set_parts(Some(on), None);
    }
    fn widgets(&self) -> Vec<WidgetSpec> {
        vec![WidgetSpec { id: CARD, title: "Screenshot", about: "Take a screenshot; recent captures at the bigger sizes.", sizes: UP_TO_MEDIUM, size: Size::S11, default_on: true }]
    }
    fn widget_ui(&mut self, _widget: &str, size: Size, ui: &mut Ui, rect: Rect, cx: &mut Cx) {
        ui::widget(&mut self.0.borrow_mut(), size, ui, rect, cx);
    }
    fn has_page(&self) -> bool {
        true
    }
    fn open_page(&mut self, route: &str) {
        Tool::open_page(&mut *self.0.borrow_mut(), route);
    }
    fn page_ui(&mut self, ui: &mut Ui, rect: Rect, cx: &mut Cx) {
        Tool::page_ui(&mut *self.0.borrow_mut(), ui, rect, cx);
    }
    fn recommendations(&self) -> Vec<Recommendation> {
        Tool::recommendations(&*self.0.borrow())
    }
    fn tick(&mut self, ctx: &egui::Context) -> bool {
        let mut c = self.0.borrow_mut();
        c.shot_on && Tool::tick(&mut *c, ctx)
    }
    fn windows(&mut self, ctx: &egui::Context, cx: &mut Cx) {
        let mut c = self.0.borrow_mut();
        if c.shot_on {
            Tool::windows(&mut *c, ctx, cx);
        }
    }
    fn demo_view(&mut self, view: &str) -> Option<View> {
        Tool::demo_view(&mut *self.0.borrow_mut(), view)
    }
}

impl Tool for Video {
    fn id(&self) -> ToolId {
        VIDEO_ID
    }
    fn name(&self) -> &'static str {
        "Video"
    }
    fn badge(&self) -> Badge {
        Badge { icon: Icon::Camera, color: tint::CAPTURE() }
    }
    fn section(&self) -> &'static str {
        "Create"
    }
    fn about(&self) -> &'static str {
        "Screen recordings with camera, microphone and a smooth cursor"
    }
    fn needs(&self) -> Vec<crate::kit::perms::Need> {
        use crate::kit::perms::{Need, Perm};
        vec![Need::must(Perm::Screen), Need::may(Perm::Microphone), Need::may(Perm::Camera)]
    }
    fn setup_shortcuts(&self) -> Vec<(&'static str, String)> {
        let c = self.0.borrow();
        c.settings.shortcut(Command::Record).map(|s| vec![("Record screen", s.display())]).unwrap_or_default()
    }
    fn sidebar_meta(&self) -> String {
        let c = self.0.borrow();
        c.rec.tray().map(|t| t.trim_start_matches(['●', '❚', ' ']).to_string()).unwrap_or_else(|| key(&c, Command::Record))
    }
    fn set_on(&mut self, on: bool) {
        self.0.borrow_mut().set_parts(None, Some(on));
    }
    fn widgets(&self) -> Vec<WidgetSpec> {
        vec![WidgetSpec { id: VIDEO_CARD, title: "Video", about: "Record the screen; recent recordings at the bigger sizes.", sizes: UP_TO_MEDIUM, size: Size::S11, default_on: true }]
    }
    fn widget_ui(&mut self, _widget: &str, size: Size, ui: &mut Ui, rect: Rect, cx: &mut Cx) {
        ui::video_widget(&mut self.0.borrow_mut(), size, ui, rect, cx);
    }
    fn has_page(&self) -> bool {
        true
    }
    fn open_page(&mut self, route: &str) {
        let mut c = self.0.borrow_mut();
        c.vtab = if route == "settings" { VTab::Settings } else { VTab::Recordings };
    }
    fn page_ui(&mut self, ui: &mut Ui, rect: Rect, cx: &mut Cx) {
        ui::video_page(&mut self.0.borrow_mut(), ui, rect, cx);
    }
    fn tray_title(&self) -> Option<String> {
        self.0.borrow().rec.tray()
    }
    fn tick(&mut self, ctx: &egui::Context) -> bool {
        let mut c = self.0.borrow_mut();
        !c.shot_on && c.video_on && Tool::tick(&mut *c, ctx)
    }
    fn windows(&mut self, ctx: &egui::Context, cx: &mut Cx) {
        let mut c = self.0.borrow_mut();
        if !c.shot_on && c.video_on {
            Tool::windows(&mut *c, ctx, cx);
        }
    }
    fn demo_view(&mut self, view: &str) -> Option<View> {
        let mut c = self.0.borrow_mut();
        match view {
            "video" => {
                c.vtab = VTab::Recordings;
                Some(View::Page)
            }
            "video-settings" => {
                c.vtab = VTab::Settings;
                Some(View::Page)
            }
            _ => None,
        }
    }
}
