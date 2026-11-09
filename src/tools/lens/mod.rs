//! Disk Map tool: measures a folder tree and shows it as a drill-down
//! treemap; selected items go to the Trash after review. Lives in the full
//! window.

mod page;
pub mod scan;

use crate::core::tool::{Badge, Cx, MenuItem, Size, Tool, ToolEnv, ToolId, View, WidgetSpec};
use crate::kit::glass::*;
use crate::kit::tile;
use egui::{Align2, Pos2, Rect, Ui, Vec2};
use std::path::PathBuf;
use std::sync::atomic::Ordering;

pub const ID: ToolId = "lens";

pub struct Lens {
    demo: bool,
    pub(crate) lens: scan::Shared,
    pub(crate) lens_root: PathBuf,
    pub(crate) lens_path: PathBuf,
    /// Zoom animation: (clicked block when zooming in, folder left when
    /// zooming out, start time).
    pub(crate) lens_morph: Option<(Option<egui::Rect>, PathBuf, f64)>,
    pub(crate) lens_selected: Vec<(PathBuf, u64)>,
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
    /// Where each block was drawn last frame, so blocks glide to new sizes.
    pub(crate) lens_anim: std::collections::HashMap<PathBuf, egui::Rect>,
    pub(crate) lens_anim_area: egui::Rect,
}

impl Lens {
    pub fn new(env: &ToolEnv) -> Self {
        let root = PathBuf::from("/");
        Lens {
            demo: env.demo,
            lens: Default::default(),
            lens_root: root.clone(),
            lens_path: root,
            lens_morph: None,
            lens_selected: vec![],
            lens_skip: 0,
            lens_review: false,
            lens_msg: None,
            lens_live: None,
            lens_order: vec![],
            lens_order_at: 0.0,
            lens_rows: Default::default(),
            lens_hues: vec![],
            lens_anim: Default::default(),
            lens_anim_area: egui::Rect::NOTHING,
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
        "Find what takes space, remove by hand"
    }
    fn badge(&self) -> Badge {
        Badge {
            icon: Icon::Map,
            color: tint::DISK_MAP,
        }
    }
    fn section(&self) -> &'static str {
        "Clean"
    }
    fn widgets(&self) -> Vec<WidgetSpec> {
        vec![WidgetSpec {
            id: "map",
            title: "Disk Map",
            about: "What takes the space, from your last map.",
            sizes: &[Size::S11, Size::S12, Size::S24, Size::S44],
            size: Size::S24,
            default_on: false,
        }]
    }
    fn widget_ui(&mut self, _widget: &str, size: Size, ui: &mut Ui, rect: Rect, cx: &mut Cx) {
        if !self.demo {
            scan::load(&self.lens, ui.ctx());
        }
        // the biggest top-level folders of the last map
        let (root, total, top): (String, u64, Vec<(String, u64)>) =
            match self.lens.result.lock().unwrap().as_ref() {
                Some(n) => {
                    let mut kids: Vec<(String, u64)> = n
                        .children
                        .iter()
                        .map(|c| (c.name.clone(), c.bytes))
                        .collect();
                    kids.sort_by_key(|k| std::cmp::Reverse(k.1));
                    kids.truncate(if size == Size::S44 { 10 } else { 6 });
                    (scan::display_name(&n.path), n.bytes, kids)
                }
                None => (String::new(), 0, Vec::new()),
            };
        let running = self.lens.running.load(Ordering::Relaxed);
        let status = if running {
            "Mapping…".to_string()
        } else if top.is_empty() {
            "Not mapped yet".to_string()
        } else {
            format!("{} in {root}", human_bytes(total))
        };
        let biggest = top
            .first()
            .map(|(n, b)| format!("{n} · {}", human_bytes(*b)))
            .unwrap_or_else(|| "Open to map your disk".into());
        let id = ui.scope_id().with("w-lens");
        let col = tint::DISK_MAP;
        let resp = match size {
            Size::S11 => tile::small(
                ui,
                rect,
                id,
                Icon::Map,
                col,
                "Disk Map",
                if top.is_empty() { &status } else { &biggest },
                false,
            ),
            Size::S12 => {
                let v = if top.is_empty() {
                    "–".to_string()
                } else {
                    human_bytes(total)
                };
                tile::wide(
                    ui,
                    rect,
                    id,
                    Icon::Map,
                    col,
                    "Disk Map",
                    &v,
                    Err(&biggest),
                    false,
                )
            }
            _ => {
                let resp = tile::card(ui, rect, id, false);
                let p = ui.painter();
                let body = tile::head(p, rect, Icon::Map, col, "Disk Map", &status);
                if top.is_empty() {
                    tile::empty_note(
                        p,
                        body,
                        if running {
                            "Mapping your disk…"
                        } else {
                            "Open Disk Map to measure your disk"
                        },
                    );
                } else {
                    let vals: Vec<f64> = top.iter().map(|t| t.1 as f64).collect();
                    for (k, (r, (name, b))) in scan::squarify(&vals, body)
                        .into_iter()
                        .zip(&top)
                        .enumerate()
                    {
                        let r = r.shrink(1.5);
                        p.rect_filled(r, 6.0, page::HUES[k.min(page::HUES.len() - 1)]);
                        if r.width() > 44.0 && r.height() > 30.0 {
                            text_fit(
                                p,
                                r.left_top() + Vec2::new(7.0, 6.0),
                                Align2::LEFT_TOP,
                                name,
                                bold(11.0),
                                TEXT,
                                r.width() - 12.0,
                            );
                            text_fit(
                                p,
                                r.left_top() + Vec2::new(7.0, 21.0),
                                Align2::LEFT_TOP,
                                &human_bytes(*b),
                                font(10.0),
                                TEXT_SOFT,
                                r.width() - 12.0,
                            );
                        } else if r.width() > 30.0 && r.height() > 16.0 {
                            text_fit(
                                p,
                                Pos2::new(r.left() + 5.0, r.center().y),
                                Align2::LEFT_CENTER,
                                name,
                                font(10.0),
                                TEXT_SOFT,
                                r.width() - 8.0,
                            );
                        }
                    }
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
    fn page_ui(&mut self, ui: &mut Ui, rect: Rect, cx: &mut Cx) {
        page::page(self, ui, rect, cx);
    }
    fn menu_items(&self) -> Vec<MenuItem> {
        vec![MenuItem {
            id: "open",
            label: "Disk Map".into(),
        }]
    }
    fn menu_action(&mut self, item: &str, cx: &mut Cx) {
        if item == "open" {
            cx.open_page(ID, "");
        }
    }
    fn demo_view(&mut self, view: &str) -> Option<View> {
        if view == "lens" || view == "sizes" {
            page::demo_tree(self);
            if view == "sizes" {
                return None;
            }
            Some(View::Page)
        } else if view == "lens-live" {
            page::demo_live(self);
            Some(View::Page)
        } else {
            None
        }
    }
}
