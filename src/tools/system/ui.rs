//! Mac health rows for the panel (disk, memory, CPU, battery, network,
//! devices), the panel's title line, and the detail sheet each row opens.

use super::monitor::{self, Level, Snapshot};
use super::ID;
use crate::core::tool::{Cx, PanelHeader};
use crate::kit::glass::*;
use crate::kit::mechanics as m;
use egui::{Color32, Rect, Ui};

fn level_color(l: Level) -> Color32 {
    match l {
        Level::Good => GREEN,
        Level::Fair => AMBER,
        Level::Critical => RED,
    }
}

/// The panel's title line: the Mac's name and its overall state.
pub fn header(s: &Snapshot) -> PanelHeader {
    let title = if s.model.is_empty() { "Mac".to_string() } else { s.model.clone() };
    if !s.ready {
        return PanelHeader { title, status: "Checking…".into(), dot: TEXT_FAINT };
    }
    let health = s.health();
    let status = match health {
        Level::Good => "All good",
        Level::Fair => "Needs a look",
        Level::Critical => "Needs attention",
    };
    PanelHeader { title, status: status.into(), dot: level_color(health) }
}

pub fn widget_height(id: &str) -> f32 {
    match id {
        "storage" | "memory" | "cpu" | "battery" | "network" | "devices" => METRIC_TILE_H,
        _ => 0.0,
    }
}

/// "1.44 TB" -> ("1.44", "TB")
fn split_num(s: &str) -> (String, String) {
    match s.split_once(' ') {
        Some((a, b)) => (a.to_string(), b.to_string()),
        None => (s.to_string(), String::new()),
    }
}

/// One half-width metric tile; a click opens its sheet.
pub fn widget(mon: &monitor::Shared, id: &str, ui: &mut Ui, r: Rect, cx: &mut Cx) {
    let s = mon.lock().unwrap().clone();
    let pct = |f: f32| (f / 100.0).clamp(0.01, 1.0);
    let mut cap_col = TEXT_FAINT;
    let (glyph, label, (big, unit), caption, bar_) = match id {
        "storage" => {
            let used = if s.disk_total > 0 { 1.0 - s.disk_free as f32 / s.disk_total as f32 } else { 0.0 };
            let c = if s.disk_level() == Level::Good { ACCENT } else { level_color(s.disk_level()) };
            (Icon::Disk, s.volume_name_or(), split_num(&human_bytes(s.disk_free)), "free".to_string(), Some((used.clamp(0.01, 1.0), c)))
        }
        "memory" => {
            let word = match s.mem_level() {
                Level::Good => "normal",
                Level::Fair => "elevated",
                Level::Critical => "high",
            };
            (Icon::Memory, "Memory".into(), (format!("{:.0}", s.mem_pressure), "%".into()), word.to_string(), Some((pct(s.mem_pressure), level_color(s.mem_level()))))
        }
        "cpu" => {
            let c = match s.thermal {
                0 => GREEN,
                1 => AMBER,
                _ => RED,
            };
            (Icon::Cpu, "CPU".into(), (format!("{:.0}", s.cpu_load), "%".into()), s.thermal_word().to_lowercase(), Some((pct(s.cpu_load), c)))
        }
        "battery" => {
            if s.battery.present {
                let c = if s.battery.percent <= 20 { RED } else { GREEN };
                let frac = (s.battery.percent as f32 / 100.0).clamp(0.01, 1.0);
                (Icon::Battery(s.battery.percent), "Battery".into(), (s.battery.percent.to_string(), "%".into()), battery_status(&s).to_lowercase(), Some((frac, c)))
            } else {
                (Icon::Battery(100), "Power".into(), ("AC".to_string(), String::new()), "Power adapter".to_string(), None)
            }
        }
        "network" => {
            let name = if s.wifi.name.is_empty() { "Network".to_string() } else { s.wifi.name.clone() };
            (Icon::Wifi, name, split_num(&human_rate(s.net_down)), format!("down · ↑ {} up", human_rate(s.net_up)), None)
        }
        "devices" => {
            let n = s.devices.len();
            let caption = if n == 0 {
                "None connected".to_string()
            } else {
                s.devices.iter().take(2).map(|d| match d.battery { Some(b) => format!("{} {b}%", d.name), None => d.name.clone() }).collect::<Vec<_>>().join(" · ")
            };
            if s.devices.iter().any(|d| d.battery.is_some_and(|b| b <= m::BT_LOW_BATTERY_MAX_PCT)) {
                cap_col = AMBER;
            }
            let glyph = if s.devices.iter().any(|d| d.kind.contains("Head")) { Icon::Headphones } else { Icon::Bluetooth };
            (glyph, "Devices".into(), (n.to_string(), "connected".into()), caption, None)
        }
        _ => return,
    };
    if metric_tile(ui, r, ui.scope_id().with(("metric", id)), glyph, &label, &big, &unit, &caption, cap_col, bar_, cx.is_detail_open(ID, id)).clicked() {
        cx.toggle_detail(ID, id);
    }
}

trait VolName {
    fn volume_name_or(&self) -> String;
}
impl VolName for Snapshot {
    fn volume_name_or(&self) -> String {
        if self.volume_name.is_empty() {
            "Macintosh HD".into()
        } else {
            self.volume_name.clone()
        }
    }
}

fn battery_status(s: &Snapshot) -> String {
    let b = &s.battery;
    if !b.present {
        return "Power adapter".into();
    }
    if b.full || (b.plugged && b.percent >= 100) {
        "Fully charged".into()
    } else if b.charging {
        "Charging".into()
    } else if b.plugged {
        "Not charging".into()
    } else if let Some(min) = b.minutes_left {
        format!("{} remaining", human_duration(min as u64 * 60))
    } else {
        "Calculating…".into()
    }
}

// ======================================================================= details

/// The detail sheet for a widget id; the sheet draws its own header.
pub fn detail(mon: &monitor::Shared, d: &str, ui: &mut Ui, rect: Rect, cx: &mut Cx) {
    use super::sheets::{self, Action, Sheet};
    let kind = match d {
        "storage" => Sheet::Storage,
        "memory" => Sheet::Memory,
        "battery" => Sheet::Battery,
        "cpu" => Sheet::Cpu,
        "network" => Sheet::Network,
        "devices" => Sheet::Devices,
        _ => return,
    };
    let s = mon.lock().unwrap().clone();
    match sheets::sheet(ui, rect, kind, &s) {
        Action::Close => cx.close_detail(),
        Action::Open(tool) => cx.open_page(tool, ""),
        Action::SpeedTest => monitor::start_speed_test(mon.clone(), ui.ctx().clone()),
        Action::None => {}
    }
}
