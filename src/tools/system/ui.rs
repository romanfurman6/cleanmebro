//! Mac health rows for the panel (disk, memory, CPU, battery, network,
//! devices), the panel's title line, and the detail sheet each row opens.

use super::devices;
use super::devices_ui;
use super::monitor::{self, Level, Snapshot};
use super::ID;
use crate::core::tool::{Cx, PanelHeader, Size};
use crate::kit::glass::*;
use crate::kit::tile;
use egui::{Align2, Color32, Pos2, Rect, Ui};

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

/// "1.44 TB" -> ("1.44", "TB")
fn split_num(s: &str) -> (String, String) {
    match s.split_once(' ') {
        Some((a, b)) => (a.to_string(), b.to_string()),
        None => (s.to_string(), String::new()),
    }
}

/// One reading, ready for any widget size.
struct Metric {
    glyph: Icon,
    label: String,
    big: String,
    unit: String,
    caption: String,
    bar: Option<(f32, Color32)>,
}

impl Metric {
    fn value(&self) -> String {
        if self.unit.is_empty() || self.unit == "%" { format!("{}{}", self.big, self.unit) } else { format!("{} {}", self.big, self.unit) }
    }
}

fn metric(s: &Snapshot, id: &str) -> Option<Metric> {
    let pct = |f: f32| (f / 100.0).clamp(0.01, 1.0);
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
                (Icon::Battery(s.battery.percent), "Battery".into(), (s.battery.percent.to_string(), "%".into()), battery_status(s).to_lowercase(), Some((frac, c)))
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
            let low = devices::lowest_first(&s.devices).into_iter().next();
            let caption = if n == 0 { "None connected".to_string() } else { devices_ui::note(&s.devices) };
            match low.as_ref().and_then(|d| d.battery().map(|b| (d, b))) {
                Some((d, b)) => (d.kind.icon(), "Devices".into(), (b.to_string(), "%".into()), caption, Some(((b as f32 / 100.0).clamp(0.02, 1.0), devices::color(b)))),
                None => (Icon::Bluetooth, "Devices".into(), (n.to_string(), "connected".into()), caption, None),
            }
        }
        _ => return None,
    };
    Some(Metric { glyph, label, big, unit, caption, bar: bar_ })
}

/// The six readings, in panel order.
const METRICS: [&str; 6] = ["storage", "memory", "cpu", "battery", "network", "devices"];

/// A Mac Health widget at `size`; a click opens its sheet (the overview
/// opens the Mac Health page).
pub fn widget(mon: &monitor::Shared, id: &str, size: Size, ui: &mut Ui, r: Rect, cx: &mut Cx) {
    let s = mon.lock().unwrap().clone();
    let wid = ui.scope_id().with(("metric", id));
    if id == "health" {
        if tile::card(ui, r, wid, false).clicked() {
            cx.open_page(ID, "");
        }
        overview(&s, size, ui.painter(), r);
        return;
    }
    let sel = cx.is_detail_open(ID, id);
    if id == "devices" {
        if devices_ui::widget(&s, size, ui, r, wid, sel).clicked() {
            cx.toggle_detail(ID, id);
        }
        return;
    }
    let Some(mt) = metric(&s, id) else { return };
    let col = tint::HEALTH;
    let resp = match size {
        Size::S11 => match mt.bar {
            Some((f, c)) => tile::small_ring(ui, r, wid, mt.glyph, f, c, &mt.value(), sel),
            None => tile::small(ui, r, wid, mt.glyph, col, &mt.big, &mt.label, sel),
        },
        Size::S12 => tile::wide(ui, r, wid, mt.glyph, col, &mt.label, &mt.value(), mt.bar.ok_or(mt.caption.as_str()), sel),
        Size::S14 => tile::row(ui, r, wid, mt.glyph, col, &mt.label, &mt.value(), mt.bar.ok_or(mt.caption.as_str()), sel),
        _ => {
            let resp = tile::card(ui, r, wid, sel);
            let p = ui.painter();
            let body = tile::head(p, r, mt.glyph, col, &mt.label, &mt.caption);
            medium_body(&s, id, &mt, p, body);
            resp
        }
    };
    if resp.clicked() {
        cx.toggle_detail(ID, id);
    }
}

/// A list line: label, optional bar, value.
type Line = (String, Option<(f32, Color32)>, String);

/// The body of a Medium metric: a big value and the reading's breakdown.
fn medium_body(s: &Snapshot, id: &str, mt: &Metric, p: &egui::Painter, body: Rect) {
    let big = p.text(body.left_top(), Align2::LEFT_TOP, &mt.big, bold(26.0), TEXT);
    p.text(Pos2::new(big.right() + 4.0, big.bottom() - 5.0), Align2::LEFT_BOTTOM, &mt.unit, font(13.0), TEXT_DIM);
    let list = Rect::from_min_max(Pos2::new(body.left(), big.bottom() + 8.0), body.max);
    let lines: Vec<Line> = match id {
        "storage" => {
            let tot = s.disk_total.max(1) as f32;
            s.categories.iter().take(3).map(|(n, b)| (n.clone(), Some(((*b as f32 / tot).clamp(0.01, 1.0), ACCENT)), human_bytes(*b))).collect()
        }
        "memory" => {
            let tot = s.mem_total.max(1) as f32;
            [("App memory", s.mem_active), ("Wired", s.mem_wired), ("Compressed", s.mem_compressed)].into_iter().map(|(n, b)| (n.to_string(), Some(((b as f32 / tot).clamp(0.01, 1.0), PURPLE)), human_bytes(b))).collect()
        }
        "cpu" => {
            tile::spark(p, list.shrink2(egui::Vec2::new(0.0, 2.0)), &s.cpu_history, mt.bar.map(|b| b.1).unwrap_or(GREEN), Some(with_alpha(GREEN, 0.12)));
            Vec::new()
        }
        "network" => vec![("Download".into(), None, human_rate(s.net_down)), ("Upload".into(), None, human_rate(s.net_up)), ("Received today".into(), None, human_bytes(s.net_recv_total))],
        _ => vec![(mt.caption.clone(), mt.bar, String::new())],
    };
    let lh = (list.height() / 3.0).min(22.0);
    for (k, (label, frac, value)) in lines.iter().enumerate() {
        let lr = Rect::from_min_size(Pos2::new(list.left(), list.top() + k as f32 * lh), egui::Vec2::new(list.width(), lh));
        tile::bar_line(p, lr, label, *frac, value);
    }
}

/// The Mac Health overview: a ring per reading (Medium) or a line per
/// reading with the CPU history (Large).
fn overview(s: &Snapshot, size: Size, p: &egui::Painter, r: Rect) {
    let status = header(s);
    let body = tile::head(p, r, Icon::Pulse, tint::HEALTH, "Mac Health", &status.status);
    let ms: Vec<Metric> = METRICS.iter().filter_map(|id| metric(s, id)).collect();
    if size == Size::S44 {
        let lh = 30.0;
        for (k, mt) in ms.iter().enumerate() {
            let lr = Rect::from_min_size(Pos2::new(body.left(), body.top() + k as f32 * lh), egui::Vec2::new(body.width(), lh));
            icon(p, mt.glyph, Rect::from_center_size(Pos2::new(lr.left() + 9.0, lr.center().y), egui::Vec2::splat(16.0)), TEXT_SOFT);
            tile::bar_line(p, Rect::from_min_max(Pos2::new(lr.left() + 26.0, lr.top()), lr.max), &mt.label, mt.bar, &mt.value());
            if k > 0 {
                hairline(p, Pos2::new(lr.left() + 26.0, lr.top()), Pos2::new(lr.right(), lr.top()));
            }
        }
        let sr = Rect::from_min_max(Pos2::new(body.left(), body.top() + ms.len() as f32 * lh + 14.0), body.max);
        p.text(sr.left_top(), Align2::LEFT_TOP, "CPU · last minute", font(11.0), TEXT_FAINT);
        tile::spark(p, Rect::from_min_max(sr.left_top() + egui::Vec2::new(0.0, 18.0), sr.max), &s.cpu_history, GREEN, Some(with_alpha(GREEN, 0.12)));
        return;
    }
    // Medium: three across, two down
    let cw = body.width() / 3.0;
    let ch = body.height() / 2.0;
    for (k, mt) in ms.iter().enumerate() {
        let c = Pos2::new(body.left() + cw * ((k % 3) as f32 + 0.5), body.top() + ch * ((k / 3) as f32) + 18.0);
        let (f, col) = mt.bar.unwrap_or((0.0, TEXT_FAINT));
        tile::ring(p, Pos2::new(body.left() + cw * (k % 3) as f32 + 18.0, c.y), 14.0, 3.5, f, col);
        icon(p, mt.glyph, Rect::from_center_size(Pos2::new(body.left() + cw * (k % 3) as f32 + 18.0, c.y), egui::Vec2::splat(13.0)), TEXT_SOFT);
        let x = body.left() + cw * (k % 3) as f32 + 38.0;
        text_fit(p, Pos2::new(x, c.y - 7.0), Align2::LEFT_CENTER, &mt.value(), bold(12.0), TEXT, cw - 40.0);
        text_fit(p, Pos2::new(x, c.y + 8.0), Align2::LEFT_CENTER, &mt.label, font(10.0), TEXT_FAINT, cw - 40.0);
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
        Action::ToggleAlerts => monitor::change_bt_prefs(mon, |p| p.alerts = !p.alerts),
        Action::ToggleMenuBar => monitor::change_bt_prefs(mon, |p| p.menu_bar = !p.menu_bar),
        Action::OpenBluetooth => devices::open_bluetooth_settings(),
        Action::None => {}
    }
}
