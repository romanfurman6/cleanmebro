//! Mac health widgets for the panel (header, disk, memory, battery, CPU,
//! network, devices) and the detail sheet each one opens.

use super::monitor::{self, Level, Snapshot};
use super::ID;
use crate::core::tool::Cx;
use crate::kit::glass::*;
use crate::kit::mechanics as m;
use egui::{Align2, Color32, Pos2, Rect, Ui, Vec2};

/// Other tools this one links to, by id only.
const CLEANUP: &str = "cleanup";
const LENS: &str = "lens";

const GAP: f32 = 12.0;

fn level_color(l: Level) -> Color32 {
    match l {
        Level::Good => CYAN,
        Level::Fair => AMBER,
        Level::Critical => RED,
    }
}

pub fn widget_height(id: &str) -> f32 {
    match id {
        "health" => 70.0,
        "storage" | "memory" => 104.0,
        "battery" | "cpu" => 78.0,
        "network" => 130.0,
        "devices" => 112.0,
        _ => 0.0,
    }
}

pub fn widget(mon: &monitor::Shared, id: &str, ui: &mut Ui, r: Rect, cx: &mut Cx) {
    let ctx = ui.ctx().clone();
    let s = mon.lock().unwrap().clone();
    let p = ui.painter().clone();
    let open = cx.is_detail_open(ID, id);
    match id {
        // "Mac Health: Good"
        "health" => {
            let health = s.health();
            let g1 = p.layout_no_wrap("Mac Health: ".into(), bold(25.0), TEXT);
            let w1 = g1.size().x;
            p.galley(Pos2::new(r.left() + 6.0, r.top() + 10.0), g1, TEXT);
            p.text(Pos2::new(r.left() + 6.0 + w1, r.top() + 10.0), Align2::LEFT_TOP, if s.ready { health.word() } else { "…" }, bold(25.0), level_color(health));
            p.text(Pos2::new(r.left() + 6.0, r.top() + 46.0), Align2::LEFT_TOP, if s.model.is_empty() { "Mac" } else { &s.model }, font(15.0), TEXT_DIM);
            let lap = Rect::from_center_size(Pos2::new(r.right() - 44.0, r.top() + 34.0), Vec2::new(78.0, 60.0));
            glow(&p, lap.center(), 46.0, Color32::from_rgba_unmultiplied(110, 230, 255, 40));
            icon(&p, Icon::Laptop, lap, Color32::WHITE);
        }
        "storage" => {
            let resp = card(ui, r, ui.id().with("c-disk"), m::CARD_RADIUS, open);
            tile_head(&p, r, Icon::Disk, &s.volume_name_or(), &format!("Available: {}", human_bytes(s.disk_free)));
            let free_up = link(ui, r.right_bottom() + Vec2::new(-16.0, -14.0), Align2::RIGHT_BOTTOM, "Free Up", 16.0);
            if free_up.clicked() {
                cx.open_page(CLEANUP, "");
            } else if resp.clicked() {
                cx.toggle_detail(ID, id);
            }
        }
        "memory" => {
            let resp = card(ui, r, ui.id().with("c-mem"), m::CARD_RADIUS, open);
            tile_head(&p, r, Icon::Memory, "Memory", &format!("Pressure: {:.0}%", s.mem_pressure));
            let more = link(ui, r.right_bottom() + Vec2::new(-16.0, -14.0), Align2::RIGHT_BOTTOM, "Details", 16.0);
            if resp.clicked() || more.clicked() {
                cx.toggle_detail(ID, id);
            }
        }
        "battery" => {
            let resp = card(ui, r, ui.id().with("c-bat"), m::CARD_RADIUS, open);
            tile_head(&p, r, Icon::Battery(s.battery.percent), "Battery", &battery_status(&s));
            if s.battery.present {
                p.text(r.right_top() + Vec2::new(-16.0, 16.0), Align2::RIGHT_TOP, format!("{}%", s.battery.percent), bold(15.0), TEXT);
            }
            if resp.clicked() {
                cx.toggle_detail(ID, id);
            }
        }
        "cpu" => {
            let resp = card(ui, r, ui.id().with("c-cpu"), m::CARD_RADIUS, open);
            tile_head(&p, r, Icon::Cpu, "CPU", &format!("Load: {:.0}%", s.cpu_load));
            let tc = match s.thermal {
                0 => CYAN,
                1 => AMBER,
                _ => RED,
            };
            p.text(r.right_top() + Vec2::new(-16.0, 16.0), Align2::RIGHT_TOP, s.thermal_word(), bold(15.0), tc);
            if resp.clicked() {
                cx.toggle_detail(ID, id);
            }
        }
        "network" => {
            let resp = card(ui, r, ui.id().with("c-net"), m::CARD_RADIUS, open);
            icon(&p, Icon::Wifi, Rect::from_min_size(r.min + Vec2::new(16.0, 14.0), Vec2::splat(24.0)), TEXT);
            text_fit(&p, r.min + Vec2::new(50.0, 15.0), Align2::LEFT_TOP, &s.wifi.name, bold(16.0), TEXT, r.width() - 64.0);
            icon(&p, Icon::Up, Rect::from_min_size(r.min + Vec2::new(18.0, 50.0), Vec2::splat(16.0)), TEXT);
            p.text(r.min + Vec2::new(42.0, 58.0), Align2::LEFT_CENTER, human_rate(s.net_up), font(15.0), TEXT);
            icon(&p, Icon::Down, Rect::from_min_size(r.min + Vec2::new(18.0, 74.0), Vec2::splat(16.0)), TEXT);
            p.text(r.min + Vec2::new(42.0, 82.0), Align2::LEFT_CENTER, human_rate(s.net_down), font(15.0), TEXT);
            let label = if s.speed.running { "Testing…" } else { "Test Speed" };
            let ts = link(ui, r.right_bottom() + Vec2::new(-16.0, -12.0), Align2::RIGHT_BOTTOM, label, 16.0);
            if ts.clicked() {
                monitor::start_speed_test(mon.clone(), ctx.clone());
                cx.show_detail(ID, id);
            } else if resp.clicked() {
                cx.toggle_detail(ID, id);
            }
        }
        "devices" => {
            let resp = card(ui, r, ui.id().with("c-dev"), m::CARD_RADIUS, open);
            p.text(r.min + Vec2::new(16.0, 16.0), Align2::LEFT_TOP, "Connected Devices", bold(16.0), TEXT);
            if s.devices.is_empty() {
                p.text(r.min + Vec2::new(16.0, 52.0), Align2::LEFT_TOP, "No devices connected", font(14.0), TEXT_DIM);
            }
            for (k, d) in s.devices.iter().take(2).enumerate() {
                let ry = r.top() + 56.0 + k as f32 * 30.0;
                let ic = if d.kind.contains("Head") { Icon::Headphones } else { Icon::Bluetooth };
                icon(&p, ic, Rect::from_center_size(Pos2::new(r.left() + 27.0, ry), Vec2::splat(20.0)), TEXT_DIM);
                let maxw = r.width() - if d.battery.is_some() { 100.0 } else { 60.0 };
                text_fit(&p, Pos2::new(r.left() + 46.0, ry), Align2::LEFT_CENTER, &d.name, font(15.0), TEXT, maxw);
                if let Some(b) = d.battery {
                    p.text(Pos2::new(r.right() - 16.0, ry), Align2::RIGHT_CENTER, format!("{b}%"), bold(15.0), if b <= m::BT_LOW_BATTERY_MAX_PCT { RED } else { AMBER });
                }
            }
            if resp.clicked() {
                cx.toggle_detail(ID, id);
            }
        }
        _ => {}
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

fn tile_head(p: &egui::Painter, r: Rect, ic: Icon, title: &str, sub: &str) {
    icon(p, ic, Rect::from_min_size(r.min + Vec2::new(14.0, 14.0), Vec2::splat(28.0)), TEXT);
    text_fit(p, r.min + Vec2::new(54.0, 14.0), Align2::LEFT_TOP, title, bold(16.5), TEXT, r.width() - 66.0);
    text_fit(p, r.min + Vec2::new(54.0, 38.0), Align2::LEFT_TOP, sub, font(14.5), TEXT_DIM, r.width() - 64.0);
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
