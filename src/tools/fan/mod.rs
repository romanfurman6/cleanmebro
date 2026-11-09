//! Fan Control tool: fan speeds and chip temperatures, live. A quick tile and
//! a card for the panel, a detail sheet and a page in the full window.
//! Reading needs no admin rights; changing fan speed (a later step) will
//! need a privileged helper.

pub mod data;
mod page;
pub mod probe;
mod ui;

use crate::core::tool::{Badge, Cx, Tool, ToolEnv, ToolId, View, WidgetSpec};
use crate::kit::glass::{tint, Icon};
use data::Snapshot;
use egui::{Rect, Ui};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

pub const ID: ToolId = "fan";
const TILE: &str = "tile";
const CARD: &str = "fans";
const SHEET: &str = "fans";

pub type Shared = Arc<Mutex<Snapshot>>;

pub struct FanControl {
    snap: Shared,
    on: Arc<AtomicBool>,
}

impl FanControl {
    pub fn new(env: &ToolEnv) -> Self {
        let on = Arc::new(AtomicBool::new(true));
        let snap = if env.demo { Arc::new(Mutex::new(data::demo())) } else { start(env.ctx.clone(), on.clone()) };
        FanControl { snap, on }
    }
}

/// Read the sensors every [`data::SAMPLE_SECS`] on a background thread
/// while the tool is on.
fn start(ctx: egui::Context, on: Arc<AtomicBool>) -> Shared {
    let snap: Shared = Arc::new(Mutex::new(Snapshot::default()));
    let out = snap.clone();
    std::thread::Builder::new()
        .name("fan-probe".into())
        .spawn(move || {
            let mut probe = probe::Probe::open();
            let chip = chip_name();
            loop {
                if on.load(Ordering::Relaxed) {
                    let r = probe.read();
                    let thermal = crate::kit::native::thermal_state();
                    let mut s = out.lock().unwrap();
                    s.loaded = true;
                    s.has_smc = r.has_smc;
                    s.fans = r.fans;
                    s.sensors = data::merge(r.hid, r.smc_temps);
                    s.thermal = thermal;
                    s.chip = chip.clone();
                    s.push_history();
                    drop(s);
                    ctx.request_repaint();
                }
                std::thread::sleep(Duration::from_secs(data::SAMPLE_SECS));
            }
        })
        .ok();
    snap
}

/// "Apple M3 Max", from `machdep.cpu.brand_string`.
fn chip_name() -> String {
    #[cfg(target_os = "macos")]
    {
        let name = std::ffi::CString::new("machdep.cpu.brand_string").unwrap();
        let mut buf = [0u8; 128];
        let mut len = buf.len();
        let ok = unsafe { libc::sysctlbyname(name.as_ptr(), buf.as_mut_ptr() as *mut libc::c_void, &mut len, std::ptr::null_mut(), 0) } == 0;
        if ok {
            return String::from_utf8_lossy(&buf[..len.saturating_sub(1)]).trim().to_string();
        }
    }
    String::new()
}

fn badge() -> Badge {
    Badge { icon: Icon::Fan, color: tint::FAN }
}

impl Tool for FanControl {
    fn id(&self) -> ToolId {
        ID
    }
    fn name(&self) -> &'static str {
        "Fan Control"
    }
    fn about(&self) -> &'static str {
        "Fan speed and chip temperatures"
    }
    fn badge(&self) -> Badge {
        badge()
    }
    fn section(&self) -> &'static str {
        "Monitor"
    }
    fn sidebar_meta(&self) -> String {
        let s = self.snap.lock().unwrap();
        match (s.avg_rpm(), s.hottest()) {
            (Some(r), _) => data::rpm_text(r),
            (None, Some((_, c))) => format!("{c:.0}°"),
            _ => String::new(),
        }
    }
    fn widgets(&self) -> Vec<WidgetSpec> {
        vec![
            WidgetSpec { id: TILE, title: "Fans (quick tile)", span: 1, default_on: true, group: None },
            WidgetSpec { id: CARD, title: "Fans", span: 4, default_on: true, group: None },
        ]
    }
    fn widget_height(&self, widget: &str, _width: f32) -> f32 {
        if widget == TILE {
            crate::kit::glass::QUICK_TILE_H
        } else {
            ui::CARD_H
        }
    }
    fn widget_ui(&mut self, widget: &str, ui: &mut Ui, rect: Rect, cx: &mut Cx) {
        let s = self.snap.lock().unwrap().clone();
        if widget == TILE {
            ui::tile(&s, ui, rect, cx);
        } else {
            ui::card(&s, ui, rect, cx);
        }
    }
    fn detail_ui(&mut self, _detail: &str, ui: &mut Ui, rect: Rect, cx: &mut Cx) {
        let s = self.snap.lock().unwrap().clone();
        ui::sheet(&s, ui, rect, cx);
    }
    fn has_page(&self) -> bool {
        true
    }
    fn page_ui(&mut self, ui: &mut Ui, rect: Rect, cx: &mut Cx) {
        let s = self.snap.lock().unwrap().clone();
        page::page(&s, ui, rect, cx);
    }
    fn set_on(&mut self, on: bool) {
        self.on.store(on, Ordering::Relaxed);
    }
    fn demo_view(&mut self, view: &str) -> Option<View> {
        match view {
            "fan" => Some(View::Page),
            "fan-detail" => Some(View::Detail(SHEET)),
            "fan-air" => {
                *self.snap.lock().unwrap() = data::demo_fanless();
                Some(View::Page)
            }
            "fan-air-detail" => {
                *self.snap.lock().unwrap() = data::demo_fanless();
                Some(View::Detail(SHEET))
            }
            _ => None,
        }
    }
}
