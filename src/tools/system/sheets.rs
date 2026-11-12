//! Mac Health detail sheets (design A): one template for storage, memory,
//! battery, CPU, network and devices. Header with a tint chip, a big number
//! with a stacked bar (or a sparkline), a legend card, a facts card and
//! buttons at the bottom right. The sheet reports what the user asked for
//! instead of acting on the app, so any shell can host it.

use crate::kit::charge;
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
    MeasureFolders,
    ToggleAlerts,
    ToggleMenuBar,
    OpenBluetooth,
    /// Charge limit, a value from [`charge::LIMITS`].
    SetLimit(u8),
    ToggleSailing,
    ToggleHeat,
    InstallHelper,
}

fn level_color(l: Level) -> Color32 {
    match l {
        Level::Good => GREEN(),
        Level::Fair => AMBER(),
        Level::Critical => RED(),
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
    Legend { color, label: label.into(), value: value.into(), value_col: TEXT() }
}

fn leg_dim(label: &str, value: impl Into<String>) -> Legend {
    Legend { color: FREE, label: label.into(), value: value.into(), value_col: TEXT_DIM() }
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
    /// On/off rows under the cards: label, hint, state, what a click does.
    switches: Vec<(&'static str, String, bool, Action)>,
    /// The charge limit row above the switches: the selected index in
    /// [`charge::LIMITS`].
    limit: Option<usize>,
    buttons: Vec<(&'static str, u8, Action)>,
}

impl Spec {
    fn new(icon: Icon, chip: Color32, title: impl Into<String>, sub: impl Into<String>) -> Self {
        Spec { icon, chip, title: title.into(), sub: sub.into(), hero: None, segs: vec![], spark: vec![], legend: vec![], speed: false, facts: vec![], switches: vec![], limit: None, buttons: vec![] }
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
            let mut sp = Spec::new(Icon::Disk, tint::HEALTH(), name, format!("{} drive", human_bytes(s.disk_total)));
            let total = s.disk_total.max(1) as f32;
            let used = s.disk_total.saturating_sub(s.disk_free);
            let (n, u) = split_num(&human_bytes(s.disk_free));
            sp.hero = Some((n, u, format!("free of {}", human_bytes(s.disk_total))));
            let colors = [tint::HEALTH(), AMBER(), tint::KEEP_AWAKE(), GREEN()];
            let cat_sum: u64 = s.categories.iter().map(|c| c.1).sum();
            let other = used.saturating_sub(cat_sum);
            for (i, (label, b)) in s.categories.iter().enumerate() {
                let c = colors[i % colors.len()];
                sp.segs.push((*b as f32 / total, c));
                sp.legend.push(leg(c, label, human_bytes(*b)));
            }
            sp.segs.push((other as f32 / total, PURPLE()));
            sp.legend.push(leg(PURPLE(), "Other", human_bytes(other)));
            sp.legend.push(leg_dim("Free", human_bytes(s.disk_free)));
            let (hv, hc, hh) = if s.smart_status.is_empty() {
                ("—".to_string(), TEXT(), "macOS didn't report the drive's health.")
            } else if s.smart_status.eq_ignore_ascii_case("Verified") {
                ("Verified".into(), GREEN(), "No problems")
            } else {
                (s.smart_status.clone(), RED(), "Back up now and replace it")
            };
            sp.facts.push(fact("Drive health", hv, hc, hh));
            match s.trash_bytes {
                Some(b) => {
                    let hint = if s.trash_overfill() { format!("Over the {} nudge limit.", human_bytes(m::TRASH_SIZE_THRESHOLD)) } else { format!("Under the {} nudge limit.", human_bytes(m::TRASH_SIZE_THRESHOLD)) };
                    sp.facts.push(fact("Trash", human_bytes(b), if s.trash_overfill() { AMBER() } else { TEXT() }, &hint));
                }
                None => sp.facts.push(fact("Trash", "—", TEXT(), "Needs Full Disk Access")),
            }
            let measure = if s.categories.is_empty() { "Measure folders" } else { "Measure again" };
            sp.buttons = vec![(measure, BTN_SECONDARY, Action::MeasureFolders), ("Disk Map", BTN_SECONDARY, Action::Open("lens")), ("Free up space", BTN_PRIMARY, Action::Open("cleanup"))];
            sp
        }
        Sheet::Memory => {
            let mut sp = Spec::new(Icon::Memory, tint::HEALTH(), "Memory", format!("{} memory", human_bytes(s.mem_total)));
            let total = s.mem_total.max(1) as f32;
            let (n, u) = split_num(&human_bytes(s.mem_available));
            sp.hero = Some((n, u, "available".into()));
            sp.segs = vec![(s.mem_active as f32 / total, tint::HEALTH()), (s.mem_wired as f32 / total, AMBER()), (s.mem_compressed as f32 / total, PURPLE())];
            sp.legend = vec![
                leg(tint::HEALTH(), "Apps (active)", human_bytes(s.mem_active)),
                leg(AMBER(), "Wired", human_bytes(s.mem_wired)),
                leg(PURPLE(), "Compressed", human_bytes(s.mem_compressed)),
                leg_dim("Available", human_bytes(s.mem_available)),
            ];
            let l = s.mem_level();
            let word = match l {
                Level::Good => "normal",
                Level::Fair => "elevated",
                Level::Critical => "high",
            };
            let hint = if l == Level::Good { "" } else { "Memory is tight" };
            sp.facts.push(fact("Pressure", format!("{:.0}% · {word}", s.mem_pressure), level_color(l), hint));
            sp.facts.push(fact("Swap used", human_bytes(s.swap_used), TEXT(), ""));
            sp
        }
        Sheet::Battery => {
            let b = &s.battery;
            let c = &s.charge;
            // the helper stopped charging (limit or heat)
            let ours = b.plugged && !b.charging && c.policy.active() && c.helper_ok == Some(true);
            let state = if !b.present {
                "Power adapter".to_string()
            } else if ours && c.status.state.hot {
                "Paused · battery warm".to_string()
            } else if ours && c.status.state.held {
                format!("Held at {}% limit", c.policy.limit)
            } else if b.full || (b.plugged && b.percent >= 100) {
                "Fully charged".into()
            } else if b.charging {
                "Charging".into()
            } else if b.plugged {
                "Not charging · on adapter".into()
            } else {
                "On battery".into()
            };
            let mut sp = Spec::new(Icon::Battery(b.percent), tint::HEALTH(), "Battery", state);
            if !b.present {
                sp.facts.push(fact("Power", "Adapter", TEXT(), "This Mac has no battery."));
                return sp;
            }
            sp.hero = Some((b.percent.to_string(), "%".into(), "charge".into()));
            sp.segs = vec![(b.percent as f32 / 100.0, if b.percent <= 20 { RED() } else { GREEN() })];
            let hv = b.health_pct.map(|h| format!("{h:.0}%")).unwrap_or_else(|| "—".into());
            let hd = match m::battery_health_state(&b.health_text, b.permanent_failure) {
                m::BatteryHealthState::Good => {
                    if b.health_pct.map(|h| h >= 99.5).unwrap_or(true) {
                        "Normal."
                    } else {
                        "Normal"
                    }
                }
                m::BatteryHealthState::Fair => "Some capacity lost",
                m::BatteryHealthState::Poor | m::BatteryHealthState::Service => "Should be replaced soon.",
                m::BatteryHealthState::Unknown => "Not available",
            };
            sp.facts.push(fact("Health", hv, level_color(s.battery_level()), hd));
            sp.facts.push(fact("Charge cycles", format!("{} of {}", b.cycles, if b.design_cycles > 0 { b.design_cycles } else { 1000 }), TEXT(), ""));
            match b.temp_c {
                Some(t) => sp.facts.push(fact("Temperature", format!("{t:.0} °C"), if t < 35.0 { TEXT() } else { AMBER() }, if t < 35.0 { "Within the normal range." } else { "Warmer than ideal." })),
                None => sp.facts.push(fact("Temperature", "—", TEXT(), "Not reported")),
            }
            let time = if ours && c.status.state.hot {
                Some((format!("Over {:.0} °C", charge::HEAT_C), ""))
            } else if ours && c.status.state.held {
                Some((format!("{}% limit", c.policy.limit), ""))
            } else if b.charging {
                b.minutes_left.map(|m| (format!("{} until full", human_duration(m as u64 * 60)), ""))
            } else if !b.plugged {
                b.minutes_left.map(|m| (format!("About {} left", human_duration(m as u64 * 60)), ""))
            } else if !b.full && b.percent < 100 {
                Some(("Held".to_string(), "macOS pauses charging to protect the battery."))
            } else {
                None
            };
            if let Some((v, h)) = time {
                sp.facts.push(fact(if b.plugged && !b.charging { "Why not charging" } else { "Time" }, v, TEXT(), h));
            }
            sp.limit = Some(charge::LIMITS.iter().position(|l| *l == c.policy.limit).unwrap_or(0));
            if c.policy.active() {
                sp.switches.push(("Sailing", format!("Resume below {}%", c.policy.resume_below()), c.policy.sailing, Action::ToggleSailing));
                sp.switches.push(("Heat protection", format!("Pause at {:.0} °C", charge::HEAT_C), c.policy.heat, Action::ToggleHeat));
            }
            if c.needs_helper() || c.installing {
                sp.buttons = vec![("Install helper", BTN_PRIMARY, Action::InstallHelper)];
            }
            sp
        }
        Sheet::Cpu => {
            let sub = if s.chip.is_empty() { "Processor".to_string() } else { s.chip.clone() };
            let mut sp = Spec::new(Icon::Cpu, tint::HEALTH(), "CPU", sub);
            sp.hero = Some((format!("{:.0}", s.cpu_load), "%".into(), "load".into()));
            sp.spark = s.cpu_history.clone();
            sp.legend = vec![
                leg(tint::HEALTH(), "User", format!("{:.0}%", s.cpu_user)),
                leg(AMBER(), "System", format!("{:.0}%", s.cpu_system)),
                leg_dim("Idle", format!("{:.0}%", (100.0 - s.cpu_load).max(0.0))),
            ];
            let (th, tc) = match s.thermal {
                0 => ("No thermal slowdown.", GREEN()),
                1 => ("Warm", AMBER()),
                2 => ("Hot, slowing down", RED()),
                _ => ("Too hot", RED()),
            };
            sp.facts.push(fact("Temperature", s.thermal_word(), tc, th));
            let uh = if s.uptime_warning() { "Over a week" } else { "" };
            sp.facts.push(fact("Uptime", human_duration(s.uptime_secs), if s.uptime_warning() { AMBER() } else { TEXT() }, uh));
            sp
        }
        Sheet::Network => {
            let title = if s.wifi.name.is_empty() { "Network".to_string() } else { s.wifi.name.clone() };
            let mut sub = if s.wifi.wired { "Wired".to_string() } else { "Wi-Fi".to_string() };
            if !s.wifi.phy.is_empty() {
                sub.push_str(&format!(" · {}", s.wifi.phy));
            }
            let mut sp = Spec::new(Icon::Wifi, tint::HEALTH(), title, sub);
            let up = human_rate(s.net_up);
            let (n, u) = split_num(&up);
            sp.hero = Some((n, u, format!("up · {} down", human_rate(s.net_down))));
            sp.speed = true;
            let dash = |v: &str| if v.is_empty() { "—".to_string() } else { v.to_string() };
            sp.facts.push(fact("Security", dash(&s.wifi.security), TEXT(), ""));
            sp.facts.push(fact("Channel", dash(&s.wifi.channel), TEXT(), ""));
            sp.facts.push(fact("Signal / noise", dash(&s.wifi.signal), TEXT(), ""));
            let label = if s.speed.down_mbps.is_some() { "Test again" } else { "Test speed" };
            sp.buttons = vec![(label, BTN_PRIMARY, Action::SpeedTest)];
            sp
        }
        Sheet::Devices => {
            let devs = super::devices::lowest_first(&s.devices);
            let mut sp = Spec::new(Icon::Bluetooth, tint::NEUTRAL(), "Devices", super::devices_ui::note(&devs));
            for d in &devs {
                match d.battery() {
                    Some(b) => sp.legend.push(Legend { color: super::devices::color(b), label: format!("{} · {}", d.name, d.kind.label()), value: format!("{b}%"), value_col: super::devices::text_color(b) }),
                    None => sp.legend.push(Legend { color: TEXT_FAINT(), label: d.name.clone(), value: "no battery data".into(), value_col: TEXT_FAINT() }),
                }
                if d.split() {
                    for (label, v) in d.parts() {
                        if let Some(v) = v {
                            sp.legend.push(Legend { color: with_alpha(super::devices::color(v), 0.55), label: format!("    {label}"), value: format!("{v}%"), value_col: TEXT_DIM() });
                        }
                    }
                }
            }
            let (low, crit) = (m::BT_LOW_BATTERY_MAX_PCT + 1, m::BT_CRITICAL_BATTERY_MAX_PCT + 1);
            sp.switches.push(("Low battery alerts", format!("macOS notification below {low}% and {crit}%"), crate::kit::notify::shown(s.bt_prefs.alerts), Action::ToggleAlerts));
            sp.switches.push(("Show in menu bar", format!("Only while a device is below {low}%"), s.bt_prefs.menu_bar, Action::ToggleMenuBar));
            sp.buttons = vec![("Bluetooth Settings", BTN_SECONDARY, Action::OpenBluetooth)];
            sp
        }
    }
}

/// Draw one sheet into `rect` and report what was clicked.
/// A button whose work is already running.
fn busy(act: Action, s: &Snapshot) -> bool {
    (act == Action::SpeedTest && s.speed.running) || (act == Action::MeasureFolders && s.sizing) || (act == Action::InstallHelper && s.charge.installing)
}

fn busy_label(act: Action) -> &'static str {
    match act {
        Action::MeasureFolders => "Measuring…",
        Action::InstallHelper => "Installing…",
        _ => "Testing…",
    }
}

pub fn sheet(ui: &mut Ui, rect: Rect, kind: Sheet, s: &Snapshot) -> Action {
    let sp = spec(kind, s);
    let mut action = Action::None;
    if sheet_header(ui, rect, sp.icon, sp.chip, &sp.title, &sp.sub) {
        action = Action::Close;
    }
    let p = ui.painter().clone();
    let x0 = rect.left() + 16.0;
    let x1 = rect.right() - 16.0;
    let top = rect.top() + SHEET_HEADER_H + 16.0;
    let h = body(&p, ui.ctx(), &sp, kind, s, x0, x1 - x0, top);
    let sw = switches(ui, &sp, x0, x1 - x0, top + h + 14.0);
    if sw != Action::None {
        action = sw;
    }
    let a = buttons(ui, &sp, s, x1, rect.bottom() - 16.0);
    if a != Action::None {
        action = a;
    }
    action
}

/// One section of the Mac Health page: the sheet's content without the
/// sheet frame. Its header (chip, title, subtitle, buttons on the right)
/// sits on one line above the cards.
pub struct Section {
    sp: Spec,
    kind: Sheet,
}

/// Height of a section's header line on the page.
pub const SECTION_HEAD_H: f32 = 46.0;

impl Section {
    pub fn new(kind: Sheet, s: &Snapshot) -> Self {
        Section { sp: spec(kind, s), kind }
    }

    /// Total height at width `w` (header + content), measured off-screen.
    pub fn height(&self, p: &egui::Painter, ctx: &egui::Context, s: &Snapshot, w: f32) -> f32 {
        let hidden = p.with_clip_rect(Rect::NOTHING);
        SECTION_HEAD_H + body(&hidden, ctx, &self.sp, self.kind, s, 0.0, w, 0.0) + switches_h(&self.sp)
    }

    /// Draw the section at `r` (its width and top; the height comes from
    /// [`Section::height`]) and report what was clicked.
    pub fn show(&self, ui: &mut Ui, r: Rect, s: &Snapshot) -> Action {
        let p = ui.painter().clone();
        tool_chip(&p, Rect::from_min_size(r.min + Vec2::new(0.0, 2.0), Vec2::splat(30.0)), self.sp.icon, self.sp.chip);
        let right = r.right();
        // buttons first so the titles can stop short of them
        let mut bx = right;
        let mut action = Action::None;
        for (label, kind_, act) in self.sp.buttons.iter().rev() {
            let bw = button_width(&p, label, *kind_);
            let br = Rect::from_min_size(Pos2::new(bx - bw, r.top() + 2.0), Vec2::new(bw, 28.0));
            let enabled = !busy(*act, s);
            if pill_button(ui, br, if enabled { label } else { busy_label(*act) }, *kind_, enabled).clicked() {
                action = *act;
            }
            bx -= bw + 8.0;
        }
        let tw = (bx - r.left() - 40.0 - 8.0).max(60.0);
        text_fit(&p, r.min + Vec2::new(40.0, 0.0), Align2::LEFT_TOP, &self.sp.title, bold(15.0), TEXT(), tw);
        text_fit(&p, r.min + Vec2::new(40.0, 19.0), Align2::LEFT_TOP, &self.sp.sub, font(12.0), TEXT_DIM(), tw);
        let h = body(&p, ui.ctx(), &self.sp, self.kind, s, r.left(), r.width(), r.top() + SECTION_HEAD_H);
        let sw = switches(ui, &self.sp, r.left(), r.width(), r.top() + SECTION_HEAD_H + h + 14.0);
        if sw != Action::None {
            action = sw;
        }
        action
    }
}

/// Legends this wide get two columns.
const LEGEND_TWO_COLUMNS_MIN: f32 = 400.0;

/// The content under a header: hero number with its bar or sparkline, the
/// legend card, the speed test card and the facts card. Returns the height.
fn body(p: &egui::Painter, ctx: &egui::Context, sp: &Spec, kind: Sheet, s: &Snapshot, x0: f32, w: f32, top: f32) -> f32 {
    let mut y = top;
    // hero
    if let Some((big, unit, cap)) = &sp.hero {
        y += hero_number(p, Pos2::new(x0, y), big, unit, cap) + 10.0;
        if !sp.segs.is_empty() {
            stacked_bar(p, Rect::from_min_size(Pos2::new(x0, y), Vec2::new(w, 10.0)), &sp.segs);
            y += 10.0 + 14.0;
        } else if sp.spark.len() > 1 {
            let hr = Rect::from_min_size(Pos2::new(x0, y), Vec2::new(w, 48.0));
            let n = sp.spark.len();
            // scale to the recent peak (at least 25%) so a calm CPU still shows its shape
            let top_v = (sp.spark.iter().copied().fold(0.0, f32::max) * 1.25).clamp(25.0, 100.0);
            let pts: Vec<Pos2> = sp.spark.iter().enumerate().map(|(i, v)| Pos2::new(hr.left() + hr.width() * i as f32 / (n - 1) as f32, hr.bottom() - 2.0 - (hr.height() - 4.0) * (v / top_v).clamp(0.0, 1.0))).collect();
            // soft fill under the line, then the line
            let mut mesh = egui::epaint::Mesh::default();
            let fill = with_alpha(tint::HEALTH(), 0.14);
            for (i, q) in pts.iter().enumerate() {
                mesh.colored_vertex(*q, fill);
                mesh.colored_vertex(Pos2::new(q.x, hr.bottom()), with_alpha(tint::HEALTH(), 0.0));
                if i > 0 {
                    let k = (i as u32) * 2;
                    mesh.add_triangle(k - 2, k - 1, k);
                    mesh.add_triangle(k - 1, k + 1, k);
                }
            }
            p.add(mesh);
            p.add(egui::epaint::PathShape::line(pts, egui::epaint::PathStroke::new(1.6, tint::HEALTH())));
            hairline(p, hr.left_bottom(), hr.right_bottom());
            y += 48.0 + 14.0;
        } else {
            y += 4.0;
        }
    }

    // legend card
    if !sp.legend.is_empty() {
        // wide enough (the window page): two columns, split by a hairline
        let cols = if w >= LEGEND_TWO_COLUMNS_MIN && sp.legend.len() >= 4 { 2 } else { 1 };
        let rows = sp.legend.len().div_ceil(cols);
        let h = 8.0 + rows as f32 * 30.0;
        let lr = Rect::from_min_size(Pos2::new(x0, y), Vec2::new(w, h));
        glass_card(p, lr, R_CARD, 0.0, false);
        let cw = w / cols as f32;
        if cols == 2 {
            hairline(p, Pos2::new(x0 + cw, y + 10.0), Pos2::new(x0 + cw, y + h - 10.0));
        }
        for (i, l) in sp.legend.iter().enumerate() {
            let (c, r) = (i / rows, i % rows);
            let rr = Rect::from_min_size(Pos2::new(x0 + c as f32 * cw, y + 4.0 + r as f32 * 30.0), Vec2::new(cw, 30.0));
            legend_row(p, rr, l.color, &l.label, &l.value, l.value_col);
        }
        y += h + 14.0;
    } else if kind == Sheet::Devices {
        y += paragraph(p, Pos2::new(x0, y), "No devices", font(13.0), TEXT_DIM(), w) + 14.0;
    }

    // speed test card
    if sp.speed {
        let sp_ = &s.speed;
        let lr = Rect::from_min_size(Pos2::new(x0, y), Vec2::new(w, 120.0));
        glass_card(p, lr, R_CARD, 0.0, false);
        p.text(lr.min + Vec2::new(12.0, 12.0), Align2::LEFT_TOP, "Speed test", bold(13.0), TEXT());
        if let Some(l) = sp_.latency_ms {
            p.text(Pos2::new(lr.right() - 12.0, lr.top() + 13.0), Align2::RIGHT_TOP, format!("Latency {l:.0} ms"), font(12.0), TEXT_FAINT());
        }
        for (k, (label, v)) in [("Download", sp_.down_mbps), ("Upload", sp_.up_mbps)].iter().enumerate() {
            let x = lr.left() + 12.0 + k as f32 * (w / 2.0);
            p.text(Pos2::new(x, lr.top() + 38.0), Align2::LEFT_TOP, *label, font(12.0), TEXT_DIM());
            let txt = v.map(|v| format!("{v:.0}")).unwrap_or_else(|| "—".into());
            let g = p.layout_no_wrap(txt, bold(24.0), TEXT());
            let gw = g.size().x;
            p.galley(Pos2::new(x, lr.top() + 54.0), g, TEXT());
            p.text(Pos2::new(x + gw + 5.0, lr.top() + 76.0), Align2::LEFT_BOTTOM, "Mbps", font(13.0), TEXT_DIM());
        }
        let note_y = lr.top() + 92.0;
        if sp_.running {
            bar(p, Rect::from_min_size(Pos2::new(lr.left() + 12.0, note_y + 4.0), Vec2::new(w - 24.0, 4.0)), sp_.progress, tint::HEALTH());
            ctx.request_repaint();
        } else if let Some(e) = &sp_.error {
            text_fit(p, Pos2::new(lr.left() + 12.0, note_y), Align2::LEFT_TOP, &format!("{e}. Check the connection and try again."), font(12.0), AMBER(), w - 24.0);
        } else if let Some(d) = sp_.down_mbps {
            let good = monitor::good_for(d);
            let txt = if good.is_empty() { "Very slow".to_string() } else { format!("Good for {}.", good.join(", ").to_lowercase()) };
            text_fit(p, Pos2::new(lr.left() + 12.0, note_y), Align2::LEFT_TOP, &txt, font(12.0), TEXT_DIM(), w - 24.0);
        } else {
            text_fit(p, Pos2::new(lr.left() + 12.0, note_y), Align2::LEFT_TOP, "", font(12.0), TEXT_DIM(), w - 24.0);
        }
        y += 120.0 + 14.0;
    }

    // facts card (measure first, then draw)
    if !sp.facts.is_empty() {
        let measure = |f: &Fact| if f.hint.is_empty() { 32.0 } else { 27.0 + p.layout(f.hint.clone(), font(12.0), TEXT_FAINT(), w - 24.0).size().y + 8.0 };
        let h: f32 = 8.0 + sp.facts.iter().map(measure).sum::<f32>();
        let fr = Rect::from_min_size(Pos2::new(x0, y), Vec2::new(w, h));
        glass_card(p, fr, R_CARD, 0.0, false);
        let mut fy = y + 4.0;
        for f in &sp.facts {
            fy += fact_row(p, Rect::from_min_size(Pos2::new(x0, fy), Vec2::new(w, 40.0)), &f.label, &f.value, f.col, &f.hint);
        }
        y += h;
    }
    y - top
}

const SWITCH_ROW_H: f32 = 48.0;

/// Rows in the switches card (the charge limit row counts as one).
fn switch_rows(sp: &Spec) -> usize {
    sp.switches.len() + sp.limit.is_some() as usize
}

/// Height of the switches card plus the gap above it (0 without switches).
fn switches_h(sp: &Spec) -> f32 {
    let n = switch_rows(sp);
    if n == 0 { 0.0 } else { 14.0 + 8.0 + n as f32 * SWITCH_ROW_H }
}

/// The on/off rows of a sheet in one card starting at `y`, under the
/// charge limit row when there is one.
fn switches(ui: &mut Ui, sp: &Spec, x0: f32, w: f32, y: f32) -> Action {
    let n = switch_rows(sp);
    if n == 0 {
        return Action::None;
    }
    let p = ui.painter().clone();
    let card = Rect::from_min_size(Pos2::new(x0, y), Vec2::new(w, 8.0 + n as f32 * SWITCH_ROW_H));
    glass_card(&p, card, R_CARD, 0.0, false);
    let mut action = Action::None;
    let first = sp.limit.is_some() as usize;
    if let Some(sel) = sp.limit {
        let rr = Rect::from_min_size(Pos2::new(x0, y + 4.0), Vec2::new(w, SWITCH_ROW_H));
        let sw = (w * 0.55).clamp(160.0, 240.0);
        let sr = Rect::from_min_size(Pos2::new(rr.right() - 12.0 - sw, rr.center().y - 13.0), Vec2::new(sw, 26.0));
        let labels: Vec<String> = charge::LIMITS.iter().map(|l| if *l >= 100 { "Off".to_string() } else { format!("{l}%") }).collect();
        let labels: Vec<&str> = labels.iter().map(String::as_str).collect();
        if let Some(i) = segmented(ui, sr, ui.scope_id().with("charge-limit"), &labels, sel) {
            action = Action::SetLimit(charge::LIMITS[i]);
        }
        text_fit(&p, Pos2::new(rr.left() + 12.0, rr.center().y), Align2::LEFT_CENTER, "Charge limit", font(13.0), TEXT(), sr.left() - rr.left() - 24.0);
    }
    for (k, (label, hint, on, act)) in sp.switches.iter().enumerate() {
        let k = k + first;
        let rr = Rect::from_min_size(Pos2::new(x0, y + 4.0 + k as f32 * SWITCH_ROW_H), Vec2::new(w, SWITCH_ROW_H));
        if k > 0 {
            hairline(&p, Pos2::new(rr.left() + 12.0, rr.top()), Pos2::new(rr.right() - 12.0, rr.top()));
        }
        let sr = Rect::from_min_size(Pos2::new(rr.right() - 12.0 - 36.0, rr.center().y - 10.0), Vec2::new(36.0, 20.0));
        if switch(ui, sr, ui.scope_id().with(("sheet-switch", *label)), *on, true).clicked() {
            action = *act;
        }
        let tw = sr.left() - rr.left() - 24.0;
        text_fit(&p, Pos2::new(rr.left() + 12.0, rr.center().y - 1.0), Align2::LEFT_BOTTOM, label, font(13.0), TEXT(), tw);
        text_fit(&p, Pos2::new(rr.left() + 12.0, rr.center().y + 2.0), Align2::LEFT_TOP, hint, font(12.0), TEXT_FAINT(), tw);
    }
    action
}

/// A sheet's buttons in a row ending at `right`, sitting on `bottom`.
fn buttons(ui: &mut Ui, sp: &Spec, s: &Snapshot, right: f32, bottom: f32) -> Action {
    let p = ui.painter().clone();
    let mut action = Action::None;
    let mut bx = right;
    let by = bottom - 30.0;
    for (label, kind_, act) in sp.buttons.iter().rev() {
        let bw = button_width(&p, label, *kind_);
        let br = Rect::from_min_size(Pos2::new(bx - bw, by), Vec2::new(bw, 30.0));
        let enabled = !busy(*act, s);
        if pill_button(ui, br, if enabled { label } else { busy_label(*act) }, *kind_, enabled).clicked() {
            action = *act;
        }
        bx -= bw + 8.0;
    }
    action
}
