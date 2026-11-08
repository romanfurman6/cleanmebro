//! Mac Health detail sheets (design A): one template for storage, memory,
//! battery, CPU, network and devices. Header with a tint chip, a big number
//! with a stacked bar (or a sparkline), a legend card, a facts card and
//! buttons at the bottom right. The sheet reports what the user asked for
//! instead of acting on the app, so any shell can host it.

use super::monitor::{self, Level, Snapshot};
use crate::kit::glass::*;
use crate::kit::mechanics as m;
use egui::{Align2, Color32, Pos2, Rect, Ui, Vec2};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Sheet {
    Storage,
    Memory,
    Battery,
    Cpu,
    Network,
    Devices,
}

/// What the user clicked in a sheet.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Action {
    None,
    Close,
    /// Open another tool's page by id ("cleanup", "lens").
    Open(&'static str),
    SpeedTest,
}

fn level_color(l: Level) -> Color32 {
    match l {
        Level::Good => GREEN,
        Level::Fair => AMBER,
        Level::Critical => RED,
    }
}

/// The free-space track colour in legends.
const FREE: Color32 = Color32::from_rgba_premultiplied(41, 41, 41, 41);

struct Fact {
    label: String,
    value: String,
    col: Color32,
    hint: String,
}

fn fact(label: &str, value: impl Into<String>, col: Color32, hint: &str) -> Fact {
    Fact { label: label.into(), value: value.into(), col, hint: hint.into() }
}

struct Legend {
    color: Color32,
    label: String,
    value: String,
    value_col: Color32,
}

fn leg(color: Color32, label: &str, value: impl Into<String>) -> Legend {
    Legend { color, label: label.into(), value: value.into(), value_col: TEXT }
}

fn leg_dim(label: &str, value: impl Into<String>) -> Legend {
    Legend { color: FREE, label: label.into(), value: value.into(), value_col: TEXT_DIM }
}

/// Everything one sheet shows, filled per kind and drawn by one template.
struct Spec {
    icon: Icon,
    chip: Color32,
    title: String,
    sub: String,
    hero: Option<(String, String, String)>,
    segs: Vec<(f32, Color32)>,
    spark: Vec<f32>,
    legend: Vec<Legend>,
    speed: bool,
    facts: Vec<Fact>,
    buttons: Vec<(&'static str, u8, Action)>,
}

impl Spec {
    fn new(icon: Icon, chip: Color32, title: impl Into<String>, sub: impl Into<String>) -> Self {
        Spec { icon, chip, title: title.into(), sub: sub.into(), hero: None, segs: vec![], spark: vec![], legend: vec![], speed: false, facts: vec![], buttons: vec![] }
    }
}

fn split_num(s: &str) -> (String, String) {
    match s.split_once(' ') {
        Some((a, b)) => (a.to_string(), b.to_string()),
        None => (s.to_string(), String::new()),
    }
}

fn spec(kind: Sheet, s: &Snapshot) -> Spec {
    match kind {
        Sheet::Storage => {
            let name = if s.volume_name.is_empty() { "Macintosh HD".to_string() } else { s.volume_name.clone() };
            let mut sp = Spec::new(Icon::Disk, BLUE, name, format!("{} drive", human_bytes(s.disk_total)));
            let total = s.disk_total.max(1) as f32;
            let used = s.disk_total.saturating_sub(s.disk_free);
            let (n, u) = split_num(&human_bytes(s.disk_free));
            sp.hero = Some((n, u, format!("free of {}", human_bytes(s.disk_total))));
            let colors = [ACCENT, AMBER, TEAL, GREEN];
            let cat_sum: u64 = s.categories.iter().map(|c| c.1).sum();
            let other = used.saturating_sub(cat_sum);
            for (i, (label, b)) in s.categories.iter().enumerate() {
                let c = colors[i % colors.len()];
                sp.segs.push((*b as f32 / total, c));
                sp.legend.push(leg(c, label, human_bytes(*b)));
            }
            sp.segs.push((other as f32 / total, PURPLE));
            sp.legend.push(leg(PURPLE, "Other", human_bytes(other)));
            sp.legend.push(leg_dim("Free", human_bytes(s.disk_free)));
            let (hv, hc, hh) = if s.smart_status.is_empty() {
                ("—".to_string(), TEXT, "macOS didn't report the drive's health.")
            } else if s.smart_status.eq_ignore_ascii_case("Verified") {
                ("Verified".into(), GREEN, "The drive reports no problems.")
            } else {
                (s.smart_status.clone(), RED, "Back up now. This drive should be replaced soon.")
            };
            sp.facts.push(fact("Drive health", hv, hc, hh));
            match s.trash_bytes {
                Some(b) => {
                    let hint = if s.trash_overfill() { format!("Over the {} nudge limit.", human_bytes(m::TRASH_SIZE_THRESHOLD)) } else { format!("Under the {} nudge limit.", human_bytes(m::TRASH_SIZE_THRESHOLD)) };
                    sp.facts.push(fact("Trash", human_bytes(b), if s.trash_overfill() { AMBER } else { TEXT }, &hint));
                }
                None => sp.facts.push(fact("Trash", "—", TEXT, "Allow Full Disk Access to measure it.")),
            }
            sp.buttons = vec![("Disk Map", BTN_SECONDARY, Action::Open("lens")), ("Free up space", BTN_PRIMARY, Action::Open("cleanup"))];
            sp
        }
        Sheet::Memory => {
            let mut sp = Spec::new(Icon::Memory, tint::HEALTH, "Memory", format!("{} memory", human_bytes(s.mem_total)));
            let total = s.mem_total.max(1) as f32;
            let (n, u) = split_num(&human_bytes(s.mem_available));
            sp.hero = Some((n, u, "available".into()));
            sp.segs = vec![(s.mem_active as f32 / total, ACCENT), (s.mem_wired as f32 / total, AMBER), (s.mem_compressed as f32 / total, PURPLE)];
            sp.legend = vec![
                leg(ACCENT, "Apps (active)", human_bytes(s.mem_active)),
                leg(AMBER, "Wired", human_bytes(s.mem_wired)),
                leg(PURPLE, "Compressed", human_bytes(s.mem_compressed)),
                leg_dim("Available", human_bytes(s.mem_available)),
            ];
            let l = s.mem_level();
            let word = match l {
                Level::Good => "normal",
                Level::Fair => "elevated",
                Level::Critical => "high",
            };
            let hint = if l == Level::Good { "Plenty of room for more apps." } else { "Quit a few unused apps to relieve memory." };
            sp.facts.push(fact("Pressure", format!("{:.0}% · {word}", s.mem_pressure), level_color(l), hint));
            sp.facts.push(fact("Swap used", human_bytes(s.swap_used), TEXT, "Swap grows when memory runs short."));
            sp
        }
        Sheet::Battery => {
            let b = &s.battery;
            let state = if !b.present {
                "Power adapter"
            } else if b.full || (b.plugged && b.percent >= 100) {
                "Fully charged"
            } else if b.charging {
                "Charging"
            } else if b.plugged {
                "Not charging · on adapter"
            } else {
                "On battery"
            };
            let mut sp = Spec::new(Icon::Battery(b.percent), tint::HEALTH, "Battery", state);
            if !b.present {
                sp.facts.push(fact("Power", "Adapter", TEXT, "This Mac has no battery."));
                return sp;
            }
            sp.hero = Some((b.percent.to_string(), "%".into(), "charge".into()));
            sp.segs = vec![(b.percent as f32 / 100.0, if b.percent <= 20 { RED } else { GREEN })];
            let hv = b.health_pct.map(|h| format!("{h:.0}%")).unwrap_or_else(|| "—".into());
            let hd = match m::battery_health_state(&b.health_text, b.permanent_failure) {
                m::BatteryHealthState::Good => {
                    if b.health_pct.map(|h| h >= 99.5).unwrap_or(true) {
                        "Normal."
                    } else {
                        "Normal. Some capacity lost with age."
                    }
                }
                m::BatteryHealthState::Fair => "Some capacity lost, still works fine.",
                m::BatteryHealthState::Poor | m::BatteryHealthState::Service => "Should be replaced soon.",
                m::BatteryHealthState::Unknown => "Couldn't be calculated right now.",
            };
            sp.facts.push(fact("Health", hv, level_color(s.battery_level()), hd));
            sp.facts.push(fact("Charge cycles", format!("{} of {}", b.cycles, if b.design_cycles > 0 { b.design_cycles } else { 1000 }), TEXT, ""));
            match b.temp_c {
                Some(t) => sp.facts.push(fact("Temperature", format!("{t:.0} °C"), if t < 35.0 { TEXT } else { AMBER }, if t < 35.0 { "Within the normal range." } else { "Warmer than ideal." })),
                None => sp.facts.push(fact("Temperature", "—", TEXT, "This Mac doesn't report it.")),
            }
            let time = if b.charging {
                b.minutes_left.map(|m| (format!("{} until full", human_duration(m as u64 * 60)), ""))
            } else if !b.plugged {
                b.minutes_left.map(|m| (format!("About {} left", human_duration(m as u64 * 60)), ""))
            } else if !b.full && b.percent < 100 {
                Some(("Held".to_string(), "macOS pauses charging to protect the battery."))
            } else {
                None
            };
            if let Some((v, h)) = time {
                sp.facts.push(fact(if b.plugged && !b.charging { "Why not charging" } else { "Time" }, v, TEXT, h));
            }
            sp
        }
        Sheet::Cpu => {
            let sub = if s.chip.is_empty() { "Processor".to_string() } else { s.chip.clone() };
            let mut sp = Spec::new(Icon::Cpu, tint::HEALTH, "CPU", sub);
            sp.hero = Some((format!("{:.0}", s.cpu_load), "%".into(), "load".into()));
            sp.spark = s.cpu_history.clone();
            sp.legend = vec![
                leg(ACCENT, "User", format!("{:.0}%", s.cpu_user)),
                leg(AMBER, "System", format!("{:.0}%", s.cpu_system)),
                leg_dim("Idle", format!("{:.0}%", (100.0 - s.cpu_load).max(0.0))),
            ];
            let (th, tc) = match s.thermal {
                0 => ("No thermal slowdown.", GREEN),
                1 => ("Warming up, still within normal.", AMBER),
                2 => ("Hot. macOS is slowing down to cool off.", RED),
                _ => ("Far too hot. Act to avoid damage.", RED),
            };
            sp.facts.push(fact("Temperature", s.thermal_word(), tc, th));
            let uh = if s.uptime_warning() { "Over a week. Restart to clear temp files and memory." } else { "Restart once a week to clear temp files." };
            sp.facts.push(fact("Uptime", human_duration(s.uptime_secs), if s.uptime_warning() { AMBER } else { TEXT }, uh));
            sp
        }
        Sheet::Network => {
            let title = if s.wifi.name.is_empty() { "Network".to_string() } else { s.wifi.name.clone() };
            let mut sub = if s.wifi.wired { "Wired".to_string() } else { "Wi-Fi".to_string() };
            if !s.wifi.phy.is_empty() {
                sub.push_str(&format!(" · {}", s.wifi.phy));
            }
            let mut sp = Spec::new(Icon::Wifi, BLUE, title, sub);
            let up = human_rate(s.net_up);
            let (n, u) = split_num(&up);
            sp.hero = Some((n, u, format!("up · {} down", human_rate(s.net_down))));
            sp.speed = true;
            let dash = |v: &str| if v.is_empty() { "—".to_string() } else { v.to_string() };
            sp.facts.push(fact("Security", dash(&s.wifi.security), TEXT, ""));
            sp.facts.push(fact("Channel", dash(&s.wifi.channel), TEXT, ""));
            sp.facts.push(fact("Signal / noise", dash(&s.wifi.signal), TEXT, ""));
            let label = if s.speed.down_mbps.is_some() { "Test again" } else { "Test speed" };
            sp.buttons = vec![(label, BTN_PRIMARY, Action::SpeedTest)];
            sp
        }
        Sheet::Devices => {
            let mut sp = Spec::new(Icon::Headphones, tint::NEUTRAL, "Devices", format!("{} connected", s.devices.len()));
            for d in &s.devices {
                match d.battery {
                    Some(b) => {
                        let low = b <= m::BT_LOW_BATTERY_MAX_PCT;
                        let c = if low { RED } else if b < 50 { AMBER } else { GREEN };
                        sp.legend.push(Legend { color: c, label: d.name.clone(), value: format!("{b}%"), value_col: if low { RED } else { TEXT } });
                    }
                    None => sp.legend.push(Legend { color: TEXT_FAINT, label: d.name.clone(), value: "no battery data".into(), value_col: TEXT_FAINT }),
                }
            }
            sp.facts.push(fact("Low battery alert", format!("Below {}%", m::BT_LOW_BATTERY_MAX_PCT + 1), TEXT, "Shown in the panel when a device runs low."));
            sp
        }
    }
}

/// Draw one sheet into `rect` and report what was clicked.
pub fn sheet(ui: &mut Ui, rect: Rect, kind: Sheet, s: &Snapshot) -> Action {
    let sp = spec(kind, s);
    let mut action = Action::None;
    if sheet_header(ui, rect, sp.icon, sp.chip, &sp.title, &sp.sub) {
        action = Action::Close;
    }
    let p = ui.painter().clone();
    let x0 = rect.left() + 16.0;
    let x1 = rect.right() - 16.0;
    let w = x1 - x0;
    let mut y = rect.top() + SHEET_HEADER_H + 16.0;

    // hero
    if let Some((big, unit, cap)) = &sp.hero {
        y += hero_number(&p, Pos2::new(x0, y), big, unit, cap) + 10.0;
        if !sp.segs.is_empty() {
            stacked_bar(&p, Rect::from_min_size(Pos2::new(x0, y), Vec2::new(w, 10.0)), &sp.segs);
            y += 10.0 + 14.0;
        } else if sp.spark.len() > 1 {
            let hr = Rect::from_min_size(Pos2::new(x0, y), Vec2::new(w, 48.0));
            let n = sp.spark.len();
            let pts: Vec<Pos2> = sp.spark.iter().enumerate().map(|(i, v)| Pos2::new(hr.left() + hr.width() * i as f32 / (n - 1) as f32, hr.bottom() - 2.0 - (hr.height() - 4.0) * (v / 100.0).clamp(0.0, 1.0))).collect();
            p.add(egui::epaint::PathShape::line(pts, egui::epaint::PathStroke::new(1.6, ACCENT)));
            hairline(&p, hr.left_bottom(), hr.right_bottom());
            y += 48.0 + 14.0;
        } else {
            y += 4.0;
        }
    }

    // legend card
    if !sp.legend.is_empty() {
        let h = 8.0 + sp.legend.len() as f32 * 30.0;
        let lr = Rect::from_min_size(Pos2::new(x0, y), Vec2::new(w, h));
        glass_card(&p, lr, R_CARD, 0.0, false);
        for (i, l) in sp.legend.iter().enumerate() {
            let rr = Rect::from_min_size(Pos2::new(x0, y + 4.0 + i as f32 * 30.0), Vec2::new(w, 30.0));
            legend_row(&p, rr, l.color, &l.label, &l.value, l.value_col);
        }
        y += h + 14.0;
    } else if kind == Sheet::Devices {
        paragraph(&p, Pos2::new(x0, y), "Connect wireless or wired devices to your Mac and they appear here.", font(13.0), TEXT_DIM, w);
        y += 40.0;
    }

    // speed test card
    if sp.speed {
        let sp_ = &s.speed;
        let lr = Rect::from_min_size(Pos2::new(x0, y), Vec2::new(w, 120.0));
        glass_card(&p, lr, R_CARD, 0.0, false);
        p.text(lr.min + Vec2::new(12.0, 12.0), Align2::LEFT_TOP, "Speed test", bold(13.0), TEXT);
        if let Some(l) = sp_.latency_ms {
            p.text(Pos2::new(lr.right() - 12.0, lr.top() + 13.0), Align2::RIGHT_TOP, format!("Latency {l:.0} ms"), font(12.0), TEXT_FAINT);
        }
        for (k, (label, v)) in [("Download", sp_.down_mbps), ("Upload", sp_.up_mbps)].iter().enumerate() {
            let x = lr.left() + 12.0 + k as f32 * (w / 2.0);
            p.text(Pos2::new(x, lr.top() + 38.0), Align2::LEFT_TOP, *label, font(12.0), TEXT_DIM);
            let txt = v.map(|v| format!("{v:.0}")).unwrap_or_else(|| "—".into());
            let g = p.layout_no_wrap(txt, bold(24.0), TEXT);
            let gw = g.size().x;
            p.galley(Pos2::new(x, lr.top() + 54.0), g, TEXT);
            p.text(Pos2::new(x + gw + 5.0, lr.top() + 76.0), Align2::LEFT_BOTTOM, "Mbps", font(13.0), TEXT_DIM);
        }
        let note_y = lr.top() + 92.0;
        if sp_.running {
            bar(&p, Rect::from_min_size(Pos2::new(lr.left() + 12.0, note_y + 4.0), Vec2::new(w - 24.0, 4.0)), sp_.progress, ACCENT);
            ui.ctx().request_repaint();
        } else if let Some(e) = &sp_.error {
            text_fit(&p, Pos2::new(lr.left() + 12.0, note_y), Align2::LEFT_TOP, &format!("{e}. Check the connection and try again."), font(12.0), AMBER, w - 24.0);
        } else if let Some(d) = sp_.down_mbps {
            let good = monitor::good_for(d);
            let txt = if good.is_empty() { "Very slow. Try reconnecting to Wi-Fi.".to_string() } else { format!("Good for {}.", good.join(", ").to_lowercase()) };
            text_fit(&p, Pos2::new(lr.left() + 12.0, note_y), Align2::LEFT_TOP, &txt, font(12.0), TEXT_DIM, w - 24.0);
        } else {
            text_fit(&p, Pos2::new(lr.left() + 12.0, note_y), Align2::LEFT_TOP, "Downloads a small test file to measure your connection.", font(12.0), TEXT_DIM, w - 24.0);
        }
        y += 120.0 + 14.0;
    }

    // facts card (measure first, then draw)
    if !sp.facts.is_empty() {
        let measure = |f: &Fact| if f.hint.is_empty() { 32.0 } else { 27.0 + p.layout(f.hint.clone(), font(12.0), TEXT_FAINT, w - 24.0).size().y + 8.0 };
        let h: f32 = 8.0 + sp.facts.iter().map(measure).sum::<f32>();
        let fr = Rect::from_min_size(Pos2::new(x0, y), Vec2::new(w, h));
        glass_card(&p, fr, R_CARD, 0.0, false);
        let mut fy = y + 4.0;
        for f in &sp.facts {
            fy += fact_row(&p, Rect::from_min_size(Pos2::new(x0, fy), Vec2::new(w, 40.0)), &f.label, &f.value, f.col, &f.hint);
        }
    }

    // buttons, bottom right
    let mut bx = x1;
    let by = rect.bottom() - 16.0 - 30.0;
    for (label, kind_, act) in sp.buttons.iter().rev() {
        let bw = button_width(&p, label, *kind_);
        let br = Rect::from_min_size(Pos2::new(bx - bw, by), Vec2::new(bw, 30.0));
        let enabled = !(*act == Action::SpeedTest && s.speed.running);
        if pill_button(ui, br, if enabled { label } else { "Testing…" }, *kind_, enabled).clicked() {
            action = *act;
        }
        bx -= bw + 8.0;
    }
    action
}
