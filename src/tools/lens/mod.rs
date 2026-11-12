//! Disk Map tool: measures a folder tree and shows it as a drill-down
//! treemap; selected items go to the Trash after review. Lives in the full
//! window.

mod history;
mod page;
pub mod scan;

use crate::core::tool::{Badge, Cx, MenuItem, Tool, ToolEnv, ToolId, View};
use crate::kit::glass::*;
use egui::{Rect, Ui};
use std::path::PathBuf;

pub const ID: ToolId = "lens";

pub struct Lens {
    pub(crate) lens: scan::Shared,
    pub(crate) lens_root: PathBuf,
    pub(crate) lens_path: PathBuf,
    /// Zoom animation: (clicked block when zooming in, folder left when
    /// zooming out, start time).
    pub(crate) lens_morph: Option<(Option<egui::Rect>, PathBuf, f64)>,
    pub(crate) lens_selected: Vec<(PathBuf, u64)>,
    /// The folder whose details show beside the rings.
    pub(crate) lens_focus: Option<PathBuf>,
    /// Inside "Other": how many of the folder's biggest items are hidden
    /// (0 = the folder itself).
    pub(crate) lens_skip: usize,
    pub(crate) lens_review: bool,
    pub(crate) lens_msg: Option<String>,
    /// The folder list while mapping, refreshed a few times a second:
    /// (when, root, rows).
    pub(crate) lens_live: Option<(f64, PathBuf, Vec<scan::LiveRow>)>,
    /// Row order of the live list; re-sorted by size only now and then so
    /// rows don't keep swapping places.
    pub(crate) lens_order: Vec<PathBuf>,
    pub(crate) lens_order_at: f64,
    /// Where each live row and its bar were drawn last frame: (row, bar fill).
    pub(crate) lens_rows: std::collections::HashMap<PathBuf, (egui::Rect, f32)>,
    /// Top-level folders in the order they got their colour, so a folder
    /// keeps its colour while the map grows.
    pub(crate) lens_hues: Vec<PathBuf>,
    /// Show rings around the folder instead of bubbles.
    pub(crate) lens_rings: bool,
    /// Folders visited before / after the shown one (Back, Forward).
    pub(crate) lens_back: Vec<PathBuf>,
    pub(crate) lens_fwd: Vec<PathBuf>,
    /// What was under the pointer on the map last frame.
    pub(crate) lens_hover: Option<PathBuf>,
    /// What the right-click menu on the map is for: (path, bytes).
    pub(crate) lens_menu: Option<(PathBuf, u64)>,
    /// Browsing a finished folder while the rest is still being measured.
    pub(crate) lens_peek: bool,
    /// A folder chosen in the folder dialog, waiting to be mapped.
    pub(crate) lens_pick: std::sync::Arc<std::sync::Mutex<Option<PathBuf>>>,
    /// Items moved to the Trash while mapping, to take off the finished map.
    pub(crate) lens_trashed: Vec<PathBuf>,
}

/// Disk Map choices kept between runs.
#[derive(Default, serde::Serialize, serde::Deserialize)]
struct Prefs {
    #[serde(default)]
    rings: bool,
}

fn prefs_path() -> PathBuf {
    crate::kit::fs::data_path("diskmap-prefs.json")
}

pub(crate) fn save_prefs(app: &Lens) {
    crate::kit::fs::save_json_later(&prefs_path(), &Prefs { rings: app.lens_rings }, false);
}

impl Lens {
    pub fn new(_env: &ToolEnv) -> Self {
        let root = PathBuf::from("/");
        Lens {
            lens: Default::default(),
            lens_root: root.clone(),
            lens_path: root,
            lens_morph: None,
            lens_selected: vec![],
            lens_focus: None,
            lens_skip: 0,
            lens_review: false,
            lens_msg: None,
            lens_live: None,
            lens_order: vec![],
            lens_order_at: 0.0,
            lens_rows: Default::default(),
            lens_hues: vec![],
            lens_rings: crate::kit::fs::load_json::<Prefs>(&prefs_path()).rings,
            lens_back: vec![],
            lens_fwd: vec![],
            lens_hover: None,
            lens_menu: None,
            lens_peek: false,
            lens_pick: Default::default(),
            lens_trashed: vec![],
        }
    }
}

impl Tool for Lens {
    fn id(&self) -> ToolId {
        ID
    }
    fn name(&self) -> &'static str {
        "Disk Map"
    }
    fn about(&self) -> &'static str {
        "Find what takes space"
    }
    fn badge(&self) -> Badge {
        Badge { icon: Icon::Map, color: tint::DISK_MAP() }
    }
    fn needs(&self) -> Vec<crate::kit::perms::Need> {
        use crate::kit::perms::{Need, Perm};
        vec![Need::must(Perm::FullDisk)]
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
        if view == "lens" || view == "sizes" || view == "lens-rings" {
            page::demo_tree(self);
            self.lens_rings = view == "lens-rings";
            if view == "sizes" {
                return None;
            }
            Some(View::Page)
        } else if view == "lens-none" {
            page::demo_tree(self);
            self.lens_selected.clear();
            self.lens_focus = None;
            Some(View::Page)
        } else if view == "lens-live" {
            page::demo_live(self);
            Some(View::Page)
        } else if view == "lens-peek" {
            page::demo_live(self);
            self.lens_peek = true;
            self.lens_path = self.lens_root.join("Projects");
            Some(View::Page)
        } else {
            None
        }
    }
}
