//! Disk Map tool: measures a folder tree and shows it as a list and a
//! bubble map; selected items go to the Trash after review. Lives in the full
//! window.

mod page;
pub mod scan;

use crate::core::tool::{Badge, Cx, MenuItem, Tool, ToolEnv, ToolId, View};
use crate::kit::glass::{tint, Icon};
use egui::{Rect, Ui};
use std::path::PathBuf;

pub const ID: ToolId = "lens";

pub struct Lens {
    pub(crate) lens: scan::Shared,
    pub(crate) lens_root: PathBuf,
    pub(crate) lens_path: PathBuf,
    pub(crate) lens_back: Vec<PathBuf>,
    pub(crate) lens_fwd: Vec<PathBuf>,
    pub(crate) lens_selected: Vec<(PathBuf, u64)>,
    pub(crate) lens_review: bool,
    pub(crate) lens_msg: Option<String>,
    demo: bool,
}

impl Lens {
    pub fn new(env: &ToolEnv) -> Self {
        let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("/"));
        Lens {
            lens: Default::default(),
            lens_root: home.clone(),
            lens_path: home,
            lens_back: vec![],
            lens_fwd: vec![],
            lens_selected: vec![],
            lens_review: false,
            lens_msg: None,
            demo: env.demo,
        }
    }

    /// (volume name, total, available) of the startup disk.
    pub(crate) fn disk(&self) -> (String, u64, u64) {
        if self.demo {
            return ("Macintosh HD".into(), 2_000_000_000_000, 1_440_000_000_000);
        }
        let (total, free) = crate::kit::fs::volume_space("/");
        (crate::kit::fs::root_volume_name(), total, free)
    }
}

impl Tool for Lens {
    fn id(&self) -> ToolId {
        ID
    }
    fn name(&self) -> &'static str {
        "Disk Map"
    }
    fn badge(&self) -> Badge {
        Badge { icon: Icon::Map, color: tint::DISK_MAP }
    }
    fn section(&self) -> &'static str {
        "Clean"
    }
    fn has_page(&self) -> bool {
        true
    }
    fn page_ui(&mut self, ui: &mut Ui, rect: Rect, cx: &mut Cx) {
        page::page(self, ui, rect, cx);
    }
    fn menu_items(&self) -> Vec<MenuItem> {
        vec![MenuItem { id: "open", label: "Disk Map".into() }]
    }
    fn menu_action(&mut self, item: &str, cx: &mut Cx) {
        if item == "open" {
            cx.open_page(ID, "");
        }
    }
    fn demo_view(&mut self, view: &str) -> Option<View> {
        if view == "lens" {
            page::demo_tree(self);
            Some(View::Page)
        } else {
            None
        }
    }
}
