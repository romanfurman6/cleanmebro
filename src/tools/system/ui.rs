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

pub fn detail_title(s: &Snapshot, d: &str) -> String {
    match d {
        "storage" => s.volume_name_or(),
        "memory" => "Memory".into(),
        "battery" => "Battery".into(),
        "cpu" => "CPU".into(),
        "network" => if s.wifi.name.is_empty() { "Network".into() } else { s.wifi.name.clone() },
        "devices" => "Connected Devices".into(),
        _ => "Mac Health".into(),
    }
}

pub fn detail(mon: &monitor::Shared, d: &str, ui: &mut Ui, body: Rect, cx: &mut Cx) {
    let s = mon.lock().unwrap().clone();
    match d {
        "storage" => storage(ui, body, &s, cx),
        "memory" => memory(ui, body, &s),
        "battery" => battery(ui, body, &s),
        "cpu" => cpu(ui, body, &s),
        "network" => network(mon, ui, body, &s),
        "devices" => devices(ui, body, &s),
        _ => {}
    }
}

/// A small fact card: icon, title, value on the right, description, optional link.
fn fact(ui: &mut Ui, r: Rect, ic: Icon, title: &str, value: &str, value_col: Color32, desc: &str, action: Option<&str>) -> bool {
    let p = ui.painter().clone();
    glass_card(&p, r, m::CARD_RADIUS, 0.0, false);
    // header strip like the original (slightly lighter)
    let head = Rect::from_min_size(r.min, Vec2::new(r.width(), 48.0));
    gradient_rect(&p, head, m::CARD_RADIUS, white(16), white(8));
    icon(&p, ic, Rect::from_min_size(r.min + Vec2::new(14.0, 12.0), Vec2::splat(24.0)), TEXT);
    let vr = p.text(Pos2::new(r.right() - 16.0, r.top() + 24.0), Align2::RIGHT_CENTER, value, bold(16.0), value_col);
    text_fit(&p, r.min + Vec2::new(46.0, 24.0), Align2::LEFT_CENTER, title, bold(16.0), TEXT, (vr.left() - r.left() - 54.0).max(30.0));
    paragraph(&p, r.min + Vec2::new(16.0, 60.0), desc, font(13.5), TEXT_DIM, r.width() - 32.0);
    if let Some(a) = action {
        return link(ui, r.right_bottom() + Vec2::new(-16.0, -12.0), Align2::RIGHT_BOTTOM, a, 15.0).clicked();
    }
    false
}

fn legend(p: &egui::Painter, pos: Pos2, color: Color32, label: &str, value: &str) {
    p.circle_filled(pos + Vec2::new(6.0, 9.0), 6.0, color);
    p.text(pos + Vec2::new(20.0, 0.0), Align2::LEFT_TOP, label, font(14.0), TEXT_DIM);
    p.text(pos + Vec2::new(20.0, 19.0), Align2::LEFT_TOP, value, bold(14.5), TEXT);
}

fn big_ring_center(p: &egui::Painter, c: Pos2, big: &str, unit: &str, sub: &str) {
    let g = p.layout_no_wrap(big.into(), bold(34.0), TEXT);
    let gu = p.layout_no_wrap(unit.into(), bold(15.0), TEXT);
    let w = g.size().x + 4.0 + gu.size().x;
    let x0 = c.x - w / 2.0;
    p.galley(Pos2::new(x0, c.y - 30.0), g.clone(), TEXT);
    p.galley(Pos2::new(x0 + g.size().x + 4.0, c.y - 20.0), gu, TEXT);
    // caption centred under the number, broken before its last word
    let (a, b) = sub.rsplit_once(' ').unwrap_or((sub, ""));
    p.text(Pos2::new(c.x, c.y + 18.0), Align2::CENTER_CENTER, a, font(14.0), TEXT_DIM);
    p.text(Pos2::new(c.x, c.y + 36.0), Align2::CENTER_CENTER, b, font(14.0), TEXT_DIM);
}

fn storage(ui: &mut Ui, r: Rect, s: &Snapshot, cx: &mut Cx) {
    let p = ui.painter().clone();
    let used = s.disk_total.saturating_sub(s.disk_free);
    let total = s.disk_total.max(1) as f32;
    let colors = [BLUE, AMBER, TEAL, GREEN, Color32::from_rgb(235, 235, 245)];
    let cat_sum: u64 = s.categories.iter().map(|c| c.1).sum();
    let other = used.saturating_sub(cat_sum);
    let mut segs: Vec<(f32, Color32)> = s.categories.iter().enumerate().map(|(i, c)| (c.1 as f32 / total, colors[i % 4])).collect();
    segs.push((other as f32 / total, colors[4]));
    let c = Pos2::new(r.left() + 118.0, r.top() + 112.0);
    p.circle_filled(c, 98.0, white(12));
    ring(&p, c, 86.0, 18.0, &segs);
    let free = human_bytes(s.disk_free);
    let (num, unit) = free.split_once(' ').unwrap_or((&free, ""));
    big_ring_center(&p, c, num, unit, &format!("of {} available", human_bytes(s.disk_total)));
    let mut ly = r.top() + 2.0;
    let names: Vec<(String, u64)> = s.categories.iter().cloned().chain(std::iter::once(("Other".to_string(), other))).collect();
    if s.categories.is_empty() {
        p.text(Pos2::new(r.left() + 250.0, ly + 20.0), Align2::LEFT_TOP, "Measuring folders…", font(14.0), TEXT_DIM);
    } else {
        for (i, (n, b)) in names.iter().enumerate() {
            legend(&p, Pos2::new(r.left() + 248.0, ly), colors[i.min(4)], n, &human_bytes(*b));
            ly += 46.0;
        }
    }
    let y = r.top() + 240.0;
    let w = (r.width() - GAP) / 2.0;
    let health = if s.smart_status.is_empty() { "—".to_string() } else if s.smart_status.eq_ignore_ascii_case("Verified") { "Okay".into() } else { s.smart_status.clone() };
    let hdesc = if s.smart_status.eq_ignore_ascii_case("Verified") || s.smart_status.is_empty() { "This drive is in good condition." } else { "This drive is in bad condition and should be replaced soon." };
    fact(ui, Rect::from_min_size(Pos2::new(r.left(), y), Vec2::new(w, 150.0)), Icon::Heart, "Health", &health, TEXT, hdesc, None);
    let used_pct = used as f32 / total * 100.0;
    fact(ui, Rect::from_min_size(Pos2::new(r.left() + w + GAP, y), Vec2::new(w, 150.0)), Icon::Disk, "Used", &format!("{used_pct:.0}%"), level_color(s.disk_level()), &format!("{} used of {}.", human_bytes(used), human_bytes(s.disk_total)), None);
    // components: Trash
    let y = y + 176.0;
    p.text(Pos2::new(r.left() + 4.0, y), Align2::LEFT_TOP, "Components", bold(18.0), TEXT_DIM);
    let tr = Rect::from_min_size(Pos2::new(r.left(), y + 34.0), Vec2::new(w, 150.0));
    glass_card(&p, tr, m::CARD_RADIUS, 0.0, false);
    icon(&p, Icon::Trash, Rect::from_min_size(tr.min + Vec2::new(16.0, 16.0), Vec2::splat(24.0)), TEXT);
    p.text(tr.min + Vec2::new(52.0, 28.0), Align2::LEFT_CENTER, "Trash", bold(16.0), TEXT);
    let tb = s.trash_bytes.unwrap_or(0);
    bar(&p, Rect::from_min_size(tr.min + Vec2::new(16.0, 60.0), Vec2::new(tr.width() - 32.0, 5.0)), tb as f32 / m::TRASH_SIZE_THRESHOLD as f32, if s.trash_overfill() { AMBER } else { TEXT });
    let txt = match s.trash_bytes {
        Some(b) => format!("Used: {} of {}", human_bytes(b), human_bytes(m::TRASH_SIZE_THRESHOLD)),
        None => "Allow Full Disk Access to measure".into(),
    };
    paragraph(&p, tr.min + Vec2::new(16.0, 78.0), &txt, font(13.5), TEXT_DIM, tr.width() - 32.0);
    if link(ui, tr.right_bottom() + Vec2::new(-16.0, -12.0), Align2::RIGHT_BOTTOM, "Free Up", 15.0).clicked() {
        cx.open_page(CLEANUP, "");
    }
    let lr = Rect::from_min_size(Pos2::new(r.left() + w + GAP, y + 34.0), Vec2::new(w, 150.0));
    glass_card(&p, lr, m::CARD_RADIUS, 0.0, false);
    icon(&p, Icon::Lens, Rect::from_min_size(lr.min + Vec2::new(16.0, 16.0), Vec2::splat(24.0)), TEXT);
    p.text(lr.min + Vec2::new(52.0, 28.0), Align2::LEFT_CENTER, "Space Lens", bold(16.0), TEXT);
    paragraph(&p, lr.min + Vec2::new(16.0, 54.0), "See which folders take the most space.", font(13.5), TEXT_DIM, lr.width() - 32.0);
    if link(ui, lr.right_bottom() + Vec2::new(-16.0, -12.0), Align2::RIGHT_BOTTOM, "Visualize", 15.0).clicked() {
        cx.open_page(LENS, "");
    }
}

fn memory(ui: &mut Ui, r: Rect, s: &Snapshot) {
    let p = ui.painter().clone();
    let total = s.mem_total.max(1) as f32;
    let cols = [BLUE, AMBER, Color32::from_rgb(255, 214, 10)];
    let c = Pos2::new(r.left() + 118.0, r.top() + 112.0);
    p.circle_filled(c, 98.0, white(12));
    ring(&p, c, 86.0, 18.0, &[(s.mem_active as f32 / total, cols[0]), (s.mem_wired as f32 / total, cols[1]), (s.mem_compressed as f32 / total, cols[2])]);
    let avail = human_bytes(s.mem_available);
    let (n, u) = avail.split_once(' ').unwrap_or((&avail, ""));
    big_ring_center(&p, c, n, u, &format!("of {} available", human_bytes(s.mem_total)));
    let rows = [("Active", s.mem_active, cols[0]), ("Wired", s.mem_wired, cols[1]), ("Compressed", s.mem_compressed, cols[2]), ("Available", s.mem_available, Color32::from_rgb(235, 235, 245))];
    for (i, (n, b, col)) in rows.iter().enumerate() {
        legend(&p, Pos2::new(r.left() + 248.0, r.top() + 10.0 + i as f32 * 48.0), *col, n, &human_bytes(*b));
    }
    let y = r.top() + 240.0;
    let w = (r.width() - GAP) / 2.0;
    let pl = s.mem_level();
    let desc = if pl == Level::Good { "Your Mac is ready to take on more tasks." } else { "Consider quitting some unimportant applications to relieve stress on Mac's memory." };
    fact(ui, Rect::from_min_size(Pos2::new(r.left(), y), Vec2::new(w, 170.0)), Icon::Chart, "Pressure", &format!("{:.0}%", s.mem_pressure), level_color(pl), desc, None);
    fact(ui, Rect::from_min_size(Pos2::new(r.left() + w + GAP, y), Vec2::new(w, 170.0)), Icon::Disk, "Swap File", &human_bytes(s.swap_used), TEXT, "An area on your drive that helps optimize Mac's memory performance.", None);
}

fn battery(ui: &mut Ui, r: Rect, s: &Snapshot) {
    let p = ui.painter().clone();
    let b = &s.battery;
    if !b.present {
        p.text(r.min + Vec2::new(4.0, 10.0), Align2::LEFT_TOP, "This Mac runs on a power adapter.", font(15.0), TEXT_DIM);
        return;
    }
    let c = Pos2::new(r.left() + 118.0, r.top() + 112.0);
    p.circle_filled(c, 98.0, white(12));
    let col = if b.percent <= 20 { RED } else { GREEN };
    ring(&p, c, 86.0, 18.0, &[(b.percent as f32 / 100.0, col)]);
    big_ring_center(&p, c, &b.percent.to_string(), "%", "charge");
    let (title, desc) = if b.full || (b.plugged && b.percent >= 100) {
        ("Fully Charged", "You're running from a power adapter".to_string())
    } else if b.charging {
        ("Charging", b.minutes_left.map(|m| format!("{} until full", human_duration(m as u64 * 60))).unwrap_or_default())
    } else if b.plugged {
        ("Not Charging", "Plugged in; macOS is holding the charge to protect the battery.".into())
    } else {
        ("Discharging", b.minutes_left.map(|m| format!("About {} until empty", human_duration(m as u64 * 60))).unwrap_or("Calculating…".into()))
    };
    p.text(Pos2::new(r.left() + 248.0, r.top() + 40.0), Align2::LEFT_TOP, title, bold(18.0), TEXT);
    paragraph(&p, Pos2::new(r.left() + 248.0, r.top() + 66.0), &desc, font(14.0), TEXT_DIM, r.width() - 252.0);
    p.text(Pos2::new(r.left() + 248.0, r.top() + 140.0), Align2::LEFT_TOP, format!("{}", b.cycles), bold(22.0), TEXT);
    p.text(Pos2::new(r.left() + 248.0, r.top() + 170.0), Align2::LEFT_TOP, format!("of {} charge cycles", if b.design_cycles > 0 { b.design_cycles } else { 1000 }), font(13.5), TEXT_DIM);
    let y = r.top() + 240.0;
    let w = (r.width() - GAP) / 2.0;
    // wording per macOS's verdict, as the original's BatteryMonitor (sub_25d4) maps it
    let hv = b.health_pct.map(|h| format!("{h:.0}%")).unwrap_or_else(|| "—".into());
    let hd = match m::battery_health_state(&b.health_text, b.permanent_failure) {
        m::BatteryHealthState::Good => {
            if b.health_pct.map(|h| h >= 99.5).unwrap_or(true) {
                "Your battery is in good condition."
            } else {
                "Your battery is in good condition, but already lost some capacity."
            }
        }
        m::BatteryHealthState::Fair => "Your battery has dropped some capacity, but remains functional.",
        m::BatteryHealthState::Poor | m::BatteryHealthState::Service => "Your battery is in bad condition and should be replaced soon.",
        m::BatteryHealthState::Unknown => "Battery health could not be calculated at the moment.",
    };
    fact(ui, Rect::from_min_size(Pos2::new(r.left(), y), Vec2::new(w, 160.0)), Icon::Heart, "Health", &hv, level_color(s.battery_level()), hd, None);
    let (tv, td) = match b.temp_c {
        Some(t) if t < 35.0 => (format!("{t:.0}°C"), "Battery temperature is within its perfect working range."),
        Some(t) => (format!("{t:.0}°C"), "Your battery temperature is not optimal."),
        None => ("—".into(), "Temperature isn't reported by this Mac."),
    };
    fact(ui, Rect::from_min_size(Pos2::new(r.left() + w + GAP, y), Vec2::new(w, 160.0)), Icon::Thermo, "Temperature", &tv, TEXT, td, None);
}

fn cpu(ui: &mut Ui, r: Rect, s: &Snapshot) {
    let p = ui.painter().clone();
    let c = Pos2::new(r.left() + 118.0, r.top() + 112.0);
    p.circle_filled(c, 98.0, white(12));
    ring(&p, c, 86.0, 18.0, &[(s.cpu_user / 100.0, BLUE), (s.cpu_system / 100.0, AMBER)]);
    big_ring_center(&p, c, &format!("{:.0}", s.cpu_load), "%", "load");
    let rows = [("User", s.cpu_user, BLUE), ("System", s.cpu_system, AMBER), ("Available", (100.0 - s.cpu_load).max(0.0), Color32::from_rgb(235, 235, 245))];
    for (i, (n, v, col)) in rows.iter().enumerate() {
        legend(&p, Pos2::new(r.left() + 248.0, r.top() + 10.0 + i as f32 * 48.0), *col, n, &format!("{v:.0}%"));
    }
    // load history sparkline
    let hr = Rect::from_min_size(Pos2::new(r.left() + 248.0, r.top() + 160.0), Vec2::new(r.width() - 252.0, 44.0));
    if s.cpu_history.len() > 1 {
        let n = s.cpu_history.len();
        let pts: Vec<Pos2> = s.cpu_history.iter().enumerate().map(|(i, v)| Pos2::new(hr.left() + hr.width() * i as f32 / (n - 1) as f32, hr.bottom() - hr.height() * (v / 100.0).clamp(0.0, 1.0))).collect();
        p.add(egui::epaint::PathShape::line(pts, egui::epaint::PathStroke::new(1.5, ACCENT)));
    }
    let y = r.top() + 240.0;
    let w = (r.width() - GAP) / 2.0;
    let (tdesc, tcol) = match s.thermal {
        0 => ("Your Mac's temperature is within its perfect working range.", CYAN),
        1 => ("Your Mac is warming up, but the readings are within normal.", AMBER),
        2 => ("Your Mac is hot and is slowing down to cool off.", RED),
        _ => ("Your Mac's temperature is far above recommended. Act to avoid hardware failure.", RED),
    };
    fact(ui, Rect::from_min_size(Pos2::new(r.left(), y), Vec2::new(w, 170.0)), Icon::Thermo, "Temperature", s.thermal_word(), tcol, tdesc, None);
    let udesc = if s.uptime_warning() { "It's been over a week now. Restart your Mac once in a while." } else { "You've started your system quite recently." };
    fact(ui, Rect::from_min_size(Pos2::new(r.left() + w + GAP, y), Vec2::new(w, 170.0)), Icon::Clock, "Uptime", &human_duration(s.uptime_secs), if s.uptime_warning() { AMBER } else { TEXT }, udesc, None);
}

fn network(mon: &monitor::Shared, ui: &mut Ui, r: Rect, s: &Snapshot) {
    let ctx = ui.ctx().clone();
    let p = ui.painter().clone();
    let w = (r.width() - GAP) / 2.0;
    // speeds
    for (k, (label, ic, rate, total)) in [("Upload", Icon::Up, s.net_up, s.net_sent_total), ("Download", Icon::Down, s.net_down, s.net_recv_total)].iter().enumerate() {
        let cr = Rect::from_min_size(Pos2::new(r.left() + k as f32 * (w + GAP), r.top()), Vec2::new(w, 104.0));
        glass_card(&p, cr, m::CARD_RADIUS, 0.0, false);
        icon(&p, *ic, Rect::from_min_size(cr.min + Vec2::new(14.0, 14.0), Vec2::splat(20.0)), TEXT);
        p.text(cr.min + Vec2::new(42.0, 24.0), Align2::LEFT_CENTER, *label, bold(15.0), TEXT);
        p.text(cr.min + Vec2::new(16.0, 58.0), Align2::LEFT_CENTER, human_rate(*rate), bold(19.0), TEXT);
        p.text(cr.min + Vec2::new(16.0, 84.0), Align2::LEFT_CENTER, format!("{} this session", human_bytes(*total)), font(12.5), TEXT_FAINT);
    }
    // speed test
    let tr = Rect::from_min_size(Pos2::new(r.left(), r.top() + 118.0), Vec2::new(r.width(), 230.0));
    glass_card(&p, tr, m::CARD_RADIUS, 0.0, false);
    p.text(tr.min + Vec2::new(16.0, 16.0), Align2::LEFT_TOP, "Test Your Connection", bold(17.0), TEXT);
    let sp = &s.speed;
    let label = if sp.running { "Stop" } else if sp.down_mbps.is_some() { "Test Again" } else { "Test speed" };
    let br = Rect::from_min_size(Pos2::new(tr.right() - 130.0, tr.top() + 12.0), Vec2::new(114.0, 30.0));
    if pill_button(ui, br, label, 1, !sp.running).clicked() {
        monitor::start_speed_test(mon.clone(), ctx.clone());
    }
    let gy = tr.top() + 64.0;
    for (k, (lab, v)) in [("Download", sp.down_mbps), ("Upload", sp.up_mbps)].iter().enumerate() {
        let x = tr.left() + 16.0 + k as f32 * (tr.width() / 2.0);
        p.text(Pos2::new(x, gy), Align2::LEFT_TOP, *lab, font(13.5), TEXT_DIM);
        let txt = v.map(|v| format!("{v:.0}")).unwrap_or_else(|| "—".into());
        let g = p.layout_no_wrap(txt, bold(30.0), TEXT);
        let gw = g.size().x;
        p.galley(Pos2::new(x, gy + 20.0), g, TEXT);
        p.text(Pos2::new(x + gw + 6.0, gy + 38.0), Align2::LEFT_TOP, "Mbps", font(13.0), TEXT_DIM);
    }
    if sp.running {
        bar(&p, Rect::from_min_size(Pos2::new(tr.left() + 16.0, gy + 74.0), Vec2::new(tr.width() - 32.0, 5.0)), sp.progress, VIOLET);
        ctx.request_repaint();
    }
    let ly = gy + 92.0;
    if let Some(e) = &sp.error {
        paragraph(&p, Pos2::new(tr.left() + 16.0, ly), &format!("{e}. Check your internet connection and try again."), font(13.5), AMBER, tr.width() - 32.0);
    } else if let Some(d) = sp.down_mbps {
        let good = monitor::good_for(d);
        if good.is_empty() {
            paragraph(&p, Pos2::new(tr.left() + 16.0, ly), "Internet connection speed is very low. Try reconnecting to Wi-Fi or contact your service provider.", font(13.5), AMBER, tr.width() - 32.0);
        } else {
            p.text(Pos2::new(tr.left() + 16.0, ly), Align2::LEFT_TOP, "Good for:", bold(14.0), TEXT);
            paragraph(&p, Pos2::new(tr.left() + 16.0, ly + 22.0), &good.join(" · "), font(13.5), TEXT_DIM, tr.width() - 32.0);
        }
    } else {
        p.text(Pos2::new(tr.left() + 16.0, ly), Align2::LEFT_TOP, "How it works", bold(14.0), TEXT);
        paragraph(&p, Pos2::new(tr.left() + 16.0, ly + 22.0), "CleanMeBro downloads a small test file to analyze your network.", font(13.5), TEXT_DIM, tr.width() - 32.0);
    }
    if let Some(l) = sp.latency_ms {
        p.text(tr.right_bottom() + Vec2::new(-16.0, -14.0), Align2::RIGHT_BOTTOM, format!("Latency {l:.0} ms"), font(12.5), TEXT_FAINT);
    }
    // info
    let ir = Rect::from_min_size(Pos2::new(r.left(), tr.bottom() + GAP), Vec2::new(r.width(), 170.0));
    glass_card(&p, ir, m::CARD_RADIUS, 0.0, false);
    let rows = [
        ("Connection type", if s.wifi.wired { "Wired".to_string() } else { "Wi-Fi".into() }),
        ("Security", if s.wifi.security.is_empty() { "—".into() } else { s.wifi.security.clone() }),
        ("Standard", if s.wifi.phy.is_empty() { "—".into() } else { s.wifi.phy.clone() }),
        ("Channel", if s.wifi.channel.is_empty() { "—".into() } else { s.wifi.channel.clone() }),
        ("Signal / noise", if s.wifi.signal.is_empty() { "—".into() } else { s.wifi.signal.clone() }),
    ];
    for (i, (k, v)) in rows.iter().enumerate() {
        let yy = ir.top() + 22.0 + i as f32 * 30.0;
        p.text(Pos2::new(ir.left() + 16.0, yy), Align2::LEFT_CENTER, *k, font(14.0), TEXT_DIM);
        text_fit(&p, Pos2::new(ir.right() - 16.0, yy), Align2::RIGHT_CENTER, v, bold(14.0), TEXT, ir.width() - 160.0);
    }
}

fn devices(ui: &mut Ui, r: Rect, s: &Snapshot) {
    let p = ui.painter().clone();
    if s.devices.is_empty() {
        paragraph(&p, r.min + Vec2::new(4.0, 6.0), "Connect wireless or wired devices to your Mac, and they will appear here.", font(15.0), TEXT_DIM, r.width() - 8.0);
        return;
    }
    for (i, d) in s.devices.iter().enumerate() {
        let cr = Rect::from_min_size(Pos2::new(r.left(), r.top() + i as f32 * 84.0), Vec2::new(r.width(), 72.0));
        if cr.bottom() > r.bottom() {
            break;
        }
        glass_card(&p, cr, m::CARD_RADIUS, 0.0, false);
        let ic = if d.kind.contains("Head") { Icon::Headphones } else { Icon::Bluetooth };
        tile_icon(&p, Rect::from_min_size(cr.min + Vec2::new(14.0, 16.0), Vec2::splat(40.0)), ic, Color32::from_rgb(100, 170, 255), Color32::from_rgb(10, 80, 220));
        text_fit(&p, cr.min + Vec2::new(68.0, 18.0), Align2::LEFT_TOP, &d.name, bold(16.0), TEXT, cr.width() - 150.0);
        p.text(cr.min + Vec2::new(68.0, 42.0), Align2::LEFT_TOP, if d.kind.is_empty() { "Bluetooth".to_string() } else { format!("{} · Bluetooth", d.kind) }, font(13.0), TEXT_DIM);
        match d.battery {
            Some(b) => {
                icon(&p, Icon::Battery(b), Rect::from_center_size(Pos2::new(cr.right() - 74.0, cr.center().y), Vec2::new(28.0, 28.0)), TEXT);
                p.text(Pos2::new(cr.right() - 16.0, cr.center().y), Align2::RIGHT_CENTER, format!("{b}%"), bold(16.0), if b <= m::BT_LOW_BATTERY_MAX_PCT { RED } else { TEXT });
            }
            None => {
                p.text(Pos2::new(cr.right() - 16.0, cr.center().y), Align2::RIGHT_CENTER, "No battery data", font(12.5), TEXT_FAINT);
            }
        }
    }
}

