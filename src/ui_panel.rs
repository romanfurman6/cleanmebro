//! The drop-down health panel (layout follows the original's menu: header,
//! a wide card, a 2-column grid of tiles, today's recommendation and a footer)
//! and the detail sheet that opens beside it when a tile is clicked.

use crate::app::{App, Detail, Page};
use crate::claude;
use crate::glass::*;
use crate::mechanics as m;
use crate::monitor::{self, Level, Snapshot};
use egui::{Align2, Color32, Pos2, Rect, Sense, Stroke, Ui, Vec2};

const PAD: f32 = 18.0;
const GAP: f32 = 12.0;

fn level_color(l: Level) -> Color32 {
    match l {
        Level::Good => CYAN,
        Level::Fair => AMBER,
        Level::Critical => RED,
    }
}

pub fn panel(app: &mut App, ui: &mut Ui, rect: Rect) {
    let ctx = ui.ctx().clone();
    let s = app.mon.lock().unwrap().clone();
    let cl = app.claude.lock().unwrap().clone();
    let p = ui.painter().clone();
    let inner = rect.shrink(PAD);

    // ---- header: "Mac Health: Good"
    let health = s.health();
    let g1 = p.layout_no_wrap("Mac Health: ".into(), bold(25.0), TEXT);
    let w1 = g1.size().x;
    p.galley(Pos2::new(inner.left() + 6.0, inner.top() + 10.0), g1, TEXT);
    p.text(Pos2::new(inner.left() + 6.0 + w1, inner.top() + 10.0), Align2::LEFT_TOP, if s.ready { health.word() } else { "…" }, bold(25.0), level_color(health));
    p.text(Pos2::new(inner.left() + 6.0, inner.top() + 46.0), Align2::LEFT_TOP, if s.model.is_empty() { "Mac" } else { &s.model }, font(15.0), TEXT_DIM);
    let lap = Rect::from_center_size(Pos2::new(inner.right() - 44.0, inner.top() + 34.0), Vec2::new(78.0, 60.0));
    glow(&p, lap.center(), 46.0, Color32::from_rgba_unmultiplied(110, 230, 255, 40));
    icon(&p, Icon::Laptop, lap, Color32::WHITE);

    let col_w = (inner.width() - GAP) / 2.0;
    let mut y = inner.top() + 82.0;

    // ---- Claude card (in place of the original's protection card)
    let r = Rect::from_min_size(Pos2::new(inner.left(), y), Vec2::new(inner.width(), 104.0));
    let resp = card(ui, r, ui.id().with("c-claude"), m::CARD_RADIUS, app.detail == Some(Detail::Claude));
    claude_card(&p, r, &cl, ctx.input(|i| i.time));
    if resp.clicked() {
        app.set_detail(&ctx, Some(Detail::Claude));
    }
    y += 104.0 + GAP;

    // ---- row 1: disk + memory
    let left = Rect::from_min_size(Pos2::new(inner.left(), y), Vec2::new(col_w, 104.0));
    let right = Rect::from_min_size(Pos2::new(inner.left() + col_w + GAP, y), Vec2::new(col_w, 104.0));
    let resp = card(ui, left, ui.id().with("c-disk"), m::CARD_RADIUS, app.detail == Some(Detail::Storage));
    tile_head(&p, left, Icon::Disk, &s.volume_name_or(), &format!("Available: {}", human_bytes(s.disk_free)));
    let free_up = link(ui, left.right_bottom() + Vec2::new(-16.0, -14.0), Align2::RIGHT_BOTTOM, "Free Up", 16.0);
    if free_up.clicked() {
        app.open_window(&ctx, Page::Cleanup);
    } else if resp.clicked() {
        app.set_detail(&ctx, Some(Detail::Storage));
    }
    let resp = card(ui, right, ui.id().with("c-mem"), m::CARD_RADIUS, app.detail == Some(Detail::Memory));
    tile_head(&p, right, Icon::Memory, "Memory", &format!("Pressure: {:.0}%", s.mem_pressure));
    let more = link(ui, right.right_bottom() + Vec2::new(-16.0, -14.0), Align2::RIGHT_BOTTOM, "Details", 16.0);
    if resp.clicked() || more.clicked() {
        app.set_detail(&ctx, Some(Detail::Memory));
    }
    y += 104.0 + GAP;

    // ---- row 2: battery + cpu
    let left = Rect::from_min_size(Pos2::new(inner.left(), y), Vec2::new(col_w, 78.0));
    let right = Rect::from_min_size(Pos2::new(inner.left() + col_w + GAP, y), Vec2::new(col_w, 78.0));
    let resp = card(ui, left, ui.id().with("c-bat"), m::CARD_RADIUS, app.detail == Some(Detail::Battery));
    let bstatus = battery_status(&s);
    tile_head(&p, left, Icon::Battery(s.battery.percent), "Battery", &bstatus);
    if s.battery.present {
        p.text(left.right_top() + Vec2::new(-16.0, 16.0), Align2::RIGHT_TOP, format!("{}%", s.battery.percent), bold(15.0), TEXT);
    }
    if resp.clicked() {
        app.set_detail(&ctx, Some(Detail::Battery));
    }
    let resp = card(ui, right, ui.id().with("c-cpu"), m::CARD_RADIUS, app.detail == Some(Detail::Cpu));
    tile_head(&p, right, Icon::Cpu, "CPU", &format!("Load: {:.0}%", s.cpu_load));
    let tc = match s.thermal {
        0 => CYAN,
        1 => AMBER,
        _ => RED,
    };
    p.text(right.right_top() + Vec2::new(-16.0, 16.0), Align2::RIGHT_TOP, s.thermal_word(), bold(15.0), tc);
    if resp.clicked() {
        app.set_detail(&ctx, Some(Detail::Cpu));
    }
    y += 78.0 + GAP;

    // ---- row 3: network + devices
    let left = Rect::from_min_size(Pos2::new(inner.left(), y), Vec2::new(col_w, 130.0));
    let right = Rect::from_min_size(Pos2::new(inner.left() + col_w + GAP, y), Vec2::new(col_w, 112.0));
    let resp = card(ui, left, ui.id().with("c-net"), m::CARD_RADIUS, app.detail == Some(Detail::Network));
    icon(&p, Icon::Wifi, Rect::from_min_size(left.min + Vec2::new(16.0, 14.0), Vec2::splat(24.0)), TEXT);
    text_fit(&p, left.min + Vec2::new(50.0, 15.0), Align2::LEFT_TOP, &s.wifi.name, bold(16.0), TEXT, left.width() - 64.0);
    icon(&p, Icon::Up, Rect::from_min_size(left.min + Vec2::new(18.0, 50.0), Vec2::splat(16.0)), TEXT);
    p.text(left.min + Vec2::new(42.0, 58.0), Align2::LEFT_CENTER, human_rate(s.net_up), font(15.0), TEXT);
    icon(&p, Icon::Down, Rect::from_min_size(left.min + Vec2::new(18.0, 74.0), Vec2::splat(16.0)), TEXT);
    p.text(left.min + Vec2::new(42.0, 82.0), Align2::LEFT_CENTER, human_rate(s.net_down), font(15.0), TEXT);
    let label = if s.speed.running { "Testing…" } else { "Test Speed" };
    let ts = link(ui, left.right_bottom() + Vec2::new(-16.0, -12.0), Align2::RIGHT_BOTTOM, label, 16.0);
    if ts.clicked() {
        monitor::start_speed_test(app.mon.clone(), ctx.clone());
        app.detail = None;
        app.set_detail(&ctx, Some(Detail::Network));
    } else if resp.clicked() {
        app.set_detail(&ctx, Some(Detail::Network));
    }
    let resp = card(ui, right, ui.id().with("c-dev"), m::CARD_RADIUS, app.detail == Some(Detail::Devices));
    p.text(right.min + Vec2::new(16.0, 16.0), Align2::LEFT_TOP, "Connected Devices", bold(16.0), TEXT);
    if s.devices.is_empty() {
        p.text(right.min + Vec2::new(16.0, 52.0), Align2::LEFT_TOP, "No devices connected", font(14.0), TEXT_DIM);
    }
    for (k, d) in s.devices.iter().take(2).enumerate() {
        let ry = right.top() + 56.0 + k as f32 * 30.0;
        let ic = if d.kind.contains("Head") { Icon::Headphones } else { Icon::Bluetooth };
        icon(&p, ic, Rect::from_center_size(Pos2::new(right.left() + 27.0, ry), Vec2::splat(20.0)), TEXT_DIM);
        let maxw = right.width() - if d.battery.is_some() { 100.0 } else { 60.0 };
        text_fit(&p, Pos2::new(right.left() + 46.0, ry), Align2::LEFT_CENTER, &d.name, font(15.0), TEXT, maxw);
        if let Some(b) = d.battery {
            p.text(Pos2::new(right.right() - 16.0, ry), Align2::RIGHT_CENTER, format!("{b}%"), bold(15.0), if b <= m::BT_LOW_BATTERY_MAX_PCT { RED } else { AMBER });
        }
    }
    if resp.clicked() {
        app.set_detail(&ctx, Some(Detail::Devices));
    }
    y += 130.0 + 16.0;

    // ---- today's recommendation
    p.text(Pos2::new(inner.left() + 4.0, y), Align2::LEFT_TOP, "Today's Recommendation", bold(17.0), TEXT_DIM);
    y += 30.0;
    let r = Rect::from_min_max(Pos2::new(inner.left(), y), Pos2::new(inner.right(), rect.bottom() - 64.0));
    glass_card(&p, r, m::CARD_RADIUS, 0.0, false);
    let rec = recommendation(app, &s, &cl);
    tile_icon(&p, Rect::from_min_size(r.min + Vec2::new(16.0, 16.0), Vec2::splat(38.0)), rec.icon, rec.top, rec.bottom);
    p.text(r.min + Vec2::new(68.0, 14.0), Align2::LEFT_TOP, rec.title, bold(15.5), TEXT);
    paragraph(&p, r.min + Vec2::new(68.0, 36.0), &rec.body, font(13.5), TEXT_DIM, r.width() - 86.0);
    let bw = p.layout_no_wrap(rec.action.to_string(), bold(14.0), TEXT).size().x + 36.0;
    let br = Rect::from_min_size(Pos2::new(r.right() - 16.0 - bw, r.bottom() - 44.0), Vec2::new(bw, 32.0));
    if pill_button(ui, br, rec.action, 0, true).clicked() {
        match rec.go {
            Go::Cleanup => app.open_window(&ctx, Page::Cleanup),
            Go::Lens => app.open_window(&ctx, Page::SpaceLens),
            Go::Claude => app.set_detail(&ctx, Some(Detail::Claude)),
            Go::Memory => app.set_detail(&ctx, Some(Detail::Memory)),
            Go::Cpu => app.set_detail(&ctx, Some(Detail::Cpu)),
            Go::Disk => app.set_detail(&ctx, Some(Detail::Storage)),
            Go::Battery => app.set_detail(&ctx, Some(Detail::Battery)),
        }
    }

    // ---- footer
    let fy = rect.bottom() - 52.0;
    p.line_segment([Pos2::new(rect.left() + 14.0, fy), Pos2::new(rect.right() - 14.0, fy)], Stroke::new(1.0, white(26)));
    let b1 = Rect::from_min_size(Pos2::new(rect.left() + 14.0, fy + 8.0), Vec2::splat(36.0));
    if square_button(ui, b1, Icon::Monitor, "Open the app").clicked() {
        app.open_window(&ctx, Page::Cleanup);
    }
    let center_r = Rect::from_center_size(Pos2::new(rect.center().x, fy + 26.0), Vec2::new(200.0, 30.0));
    let cr = ui.interact(center_r, ui.id().with("open-app"), Sense::click());
    p.text(center_r.center(), Align2::CENTER_CENTER, "Open CleanMeBro", font(16.0), if cr.hovered() { TEXT } else { TEXT_DIM });
    if cr.clicked() {
        app.open_window(&ctx, Page::Cleanup);
    }
    let b3 = Rect::from_min_size(Pos2::new(rect.right() - 50.0, fy + 8.0), Vec2::splat(36.0));
    let b2 = b3.translate(Vec2::new(-44.0, 0.0));
    if square_button(ui, b2, Icon::Broom, "Cleanup").clicked() {
        app.open_window(&ctx, Page::Cleanup);
    }
    if square_button(ui, b3, Icon::Gear, "Settings").clicked() {
        app.gear_open = !app.gear_open;
    }
    if app.gear_open {
        let menu = Rect::from_min_size(Pos2::new(rect.right() - 200.0, fy - 118.0), Vec2::new(186.0, 112.0));
        glass_card(&p, menu, 12.0, 0.4, false);
        let p2 = ui.painter();
        p2.rect_filled(menu, 12.0, Color32::from_rgba_unmultiplied(28, 28, 32, 235));
        let rows = ["Refresh Claude limits", "Space Lens", "Quit CleanMeBro"];
        for (k, label) in rows.iter().enumerate() {
            let rr = Rect::from_min_size(menu.min + Vec2::new(6.0, 6.0 + k as f32 * 33.0), Vec2::new(menu.width() - 12.0, 32.0));
            let resp = ui.interact(rr, ui.id().with(("gear", k)), Sense::click());
            if resp.hovered() {
                ui.painter().rect_filled(rr, 7.0, with_alpha(BLUE, 0.85));
            }
            ui.painter().text(rr.left_center() + Vec2::new(10.0, 0.0), Align2::LEFT_CENTER, *label, font(14.0), TEXT);
            if resp.clicked() {
                app.gear_open = false;
                match k {
                    0 => {
                        claude::request_refresh();
                        app.toast = Some(("Asking Claude for your limits…".into(), std::time::Instant::now()));
                    }
                    1 => app.open_window(&ctx, Page::SpaceLens),
                    _ => app.quit(&ctx),
                }
            }
        }
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

fn square_button(ui: &mut Ui, r: Rect, ic: Icon, tip: &str) -> egui::Response {
    let id = ui.id().with(("sq", tip));
    let resp = ui.interact(r, id, Sense::click());
    let h = ui.ctx().animate_bool(id, resp.hovered());
    let p = ui.painter();
    gradient_rect(p, r, 10.0, white((26.0 + 20.0 * h) as u8), white((10.0 + 14.0 * h) as u8));
    rim(p, r, 10.0, 0.8 + 0.2 * h);
    icon(p, ic, r.shrink(9.0), TEXT);
    resp.on_hover_text(tip)
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

fn claude_card(p: &egui::Painter, r: Rect, cl: &claude::ClaudeInfo, t: f64) {
    tile_icon(p, Rect::from_min_size(r.min + Vec2::new(16.0, 15.0), Vec2::splat(30.0)), Icon::Sparkle, Color32::from_rgb(255, 180, 140), Color32::from_rgb(210, 96, 66));
    p.text(r.min + Vec2::new(58.0, 15.0), Align2::LEFT_TOP, cl.plan_title(), bold(17.0), TEXT);
    let status = if cl.loading && cl.limits.is_empty() {
        "Checking…".to_string()
    } else if !cl.installed && !cl.loading {
        "Not installed".to_string()
    } else if cl.limits.is_empty() {
        cl.error.clone().unwrap_or_else(|| "No data yet".into())
    } else {
        format!("{} tokens today", claude::human_tokens(cl.local.tokens_today))
    };
    let st_col = if cl.limits.is_empty() { TEXT_FAINT } else { TEXT_DIM };
    if !cl.limits.is_empty() {
        p.circle_filled(r.right_top() + Vec2::new(-16.0 - p.layout_no_wrap(status.clone(), font(13.0), st_col).size().x - 10.0, 25.0), 3.5, GREEN);
    }
    text_fit(p, r.right_top() + Vec2::new(-16.0, 18.0), Align2::RIGHT_TOP, &status, font(13.0), st_col, 180.0);
    if cl.limits.is_empty() && cl.loading {
        spinner(p, r.right_top() + Vec2::new(-24.0, 60.0), 9.0, t, TEXT_DIM);
    }
    let rows = [cl.session().cloned(), cl.weekly().cloned()];
    for (k, l) in rows.iter().enumerate() {
        let yy = r.top() + 58.0 + k as f32 * 24.0;
        let label = if k == 0 { "Session" } else { "Week" };
        p.text(Pos2::new(r.left() + 18.0, yy), Align2::LEFT_CENTER, label, font(13.5), TEXT_DIM);
        let bar_r = Rect::from_min_size(Pos2::new(r.left() + 82.0, yy - 3.5), Vec2::new(r.width() - 250.0, 7.0));
        match l {
            Some(l) => {
                let c = if l.pct >= 90.0 { RED } else if l.pct >= 70.0 { AMBER } else { BLUE };
                bar(p, bar_r, l.pct / 100.0, c);
                p.text(Pos2::new(bar_r.right() + 10.0, yy), Align2::LEFT_CENTER, format!("{:.0}%", l.pct), bold(13.5), TEXT);
                text_fit(p, Pos2::new(r.right() - 16.0, yy), Align2::RIGHT_CENTER, &short_reset(&l.resets), font(12.5), TEXT_FAINT, 110.0);
            }
            None => bar(p, bar_r, 0.0, TEXT_DIM),
        }
    }
}

fn short_reset(s: &str) -> String {
    if s.is_empty() {
        return String::new();
    }
    let s = s.split(" (").next().unwrap_or(s);
    format!("resets {}", s)
}

enum Go {
    Disk,
    Battery,
    Cleanup,
    Lens,
    Claude,
    Memory,
    Cpu,
}

struct Rec {
    icon: Icon,
    top: Color32,
    bottom: Color32,
    title: &'static str,
    body: String,
    action: &'static str,
    go: Go,
}

/// Today's Recommendation: the original ranks its health recommendations by
/// priority (AssistantDomain: drive health 100, battery health 80, out of disk
/// space 20, Trash overfill 5). Claude limits and restart are slotted in by us.
fn recommendation(app: &App, s: &Snapshot, cl: &claude::ClaudeInfo) -> Rec {
    let blue = (Color32::from_rgb(100, 170, 255), Color32::from_rgb(10, 110, 235));
    let mut recs: Vec<(u32, Rec)> = Vec::new();
    if !s.smart_status.is_empty() && !s.smart_status.eq_ignore_ascii_case("Verified") {
        recs.push((m::PRIORITY_DRIVE_HEALTH, Rec { icon: Icon::Disk, top: blue.0, bottom: blue.1, title: "Your drive needs attention", body: "Back up immediately. This drive is in bad condition and should be replaced.".into(), action: "Show Disk", go: Go::Disk }));
    }
    if s.battery_level() == Level::Critical {
        recs.push((m::PRIORITY_BATTERY_HEALTH, Rec { icon: Icon::Battery(s.battery.percent), top: blue.0, bottom: blue.1, title: "Battery health is critical", body: "Your battery is in bad condition and should be replaced soon.".into(), action: "Show Battery", go: Go::Battery }));
    }
    if let Some(l) = cl.session() {
        if l.pct >= 80.0 {
            recs.push((60, Rec { icon: Icon::Sparkle, top: Color32::from_rgb(255, 180, 140), bottom: Color32::from_rgb(210, 96, 66), title: "Your Claude session is almost used up", body: format!("{:.0}% of the current 5-hour session is used. It {}.", l.pct, short_reset(&l.resets)), action: "See Limits", go: Go::Claude }));
        }
    }
    if s.disk_level() != Level::Good {
        recs.push((m::PRIORITY_OUT_OF_DISK, Rec { icon: Icon::Lens, top: blue.0, bottom: blue.1, title: "Low disk space", body: format!("Less than {} is free. See what takes the most space.", human_bytes(m::disk_alert_threshold(s.disk_total))), action: "Open Space Lens", go: Go::Lens }));
    }
    if s.mem_level() != Level::Good {
        recs.push((15, Rec { icon: Icon::Memory, top: blue.0, bottom: blue.1, title: "Memory is under pressure", body: "Quitting a few unused apps will give your Mac room to breathe.".into(), action: "Show Memory", go: Go::Memory }));
    }
    if s.uptime_warning() {
        recs.push((10, Rec { icon: Icon::Power, top: blue.0, bottom: blue.1, title: "Time for a restart", body: format!("Your Mac has been running for {} days. A restart clears temporary files and memory.", s.uptime_days()), action: "Show CPU", go: Go::Cpu }));
    }
    if s.trash_overfill() {
        recs.push((m::PRIORITY_TRASH_OVERFILL, Rec { icon: Icon::Trash, top: blue.0, bottom: blue.1, title: "Empty Trash Bins", body: format!("Your Trash holds {}. Empty it to get the space back.", human_bytes(s.trash_bytes.unwrap_or(0))), action: "Review Cleanup", go: Go::Cleanup }));
    }
    let claude_junk = app.clean.lock().unwrap().claude_safe_bytes();
    if claude_junk > 50_000_000 {
        recs.push((4, Rec { icon: Icon::Broom, top: blue.0, bottom: blue.1, title: "Claude caches can be trimmed", body: format!("{} of old Claude caches, logs and versions are safe to remove.", human_bytes(claude_junk)), action: "Review Cleanup", go: Go::Cleanup }));
    }
    recs.sort_by(|a, b| b.0.cmp(&a.0));
    recs.into_iter().next().map(|r| r.1).unwrap_or(Rec { icon: Icon::Broom, top: blue.0, bottom: blue.1, title: "Let's give your Mac a quick clean", body: "Claude checks each cache and log path and tells you what's safe to remove.".into(), action: "Start Cleanup", go: Go::Cleanup })
}

// ======================================================================= details

pub fn detail(app: &mut App, ui: &mut Ui, rect: Rect, d: Detail) {
    let s = app.mon.lock().unwrap().clone();
    let inner = rect.shrink(PAD);
    let p = ui.painter().clone();
    let ctx = ui.ctx().clone();
    let title = match d {
        Detail::Claude => app.claude.lock().unwrap().plan_title(),
        Detail::Storage => s.volume_name_or(),
        Detail::Memory => "Memory".into(),
        Detail::Battery => "Battery".into(),
        Detail::Cpu => "CPU".into(),
        Detail::Network => if s.wifi.name.is_empty() { "Network".into() } else { s.wifi.name.clone() },
        Detail::Devices => "Connected Devices".into(),
    };
    text_fit(&p, inner.min + Vec2::new(4.0, 6.0), Align2::LEFT_TOP, &title, bold(25.0), TEXT, inner.width() - 50.0);
    // close
    let cr = Rect::from_center_size(Pos2::new(inner.right() - 10.0, inner.top() + 20.0), Vec2::splat(26.0));
    let resp = ui.interact(cr, ui.id().with("detail-close"), Sense::click());
    p.circle_filled(cr.center(), 12.0, white(if resp.hovered() { 50 } else { 24 }));
    p.line_segment([cr.center() + Vec2::new(-4.0, -4.0), cr.center() + Vec2::new(4.0, 4.0)], Stroke::new(1.5, TEXT));
    p.line_segment([cr.center() + Vec2::new(-4.0, 4.0), cr.center() + Vec2::new(4.0, -4.0)], Stroke::new(1.5, TEXT));
    if resp.clicked() {
        app.set_detail(&ctx, None);
        return;
    }
    let body = Rect::from_min_max(inner.min + Vec2::new(0.0, 56.0), inner.max);
    match d {
        Detail::Storage => storage(app, ui, body, &s),
        Detail::Memory => memory(ui, body, &s),
        Detail::Battery => battery(ui, body, &s),
        Detail::Cpu => cpu(ui, body, &s),
        Detail::Network => network(app, ui, body, &s),
        Detail::Devices => devices(ui, body, &s),
        Detail::Claude => claude_detail(app, ui, body),
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

fn storage(app: &mut App, ui: &mut Ui, r: Rect, s: &Snapshot) {
    let ctx = ui.ctx().clone();
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
        app.open_window(&ctx, Page::Cleanup);
    }
    let lr = Rect::from_min_size(Pos2::new(r.left() + w + GAP, y + 34.0), Vec2::new(w, 150.0));
    glass_card(&p, lr, m::CARD_RADIUS, 0.0, false);
    icon(&p, Icon::Lens, Rect::from_min_size(lr.min + Vec2::new(16.0, 16.0), Vec2::splat(24.0)), TEXT);
    p.text(lr.min + Vec2::new(52.0, 28.0), Align2::LEFT_CENTER, "Space Lens", bold(16.0), TEXT);
    paragraph(&p, lr.min + Vec2::new(16.0, 54.0), "See which folders take the most space.", font(13.5), TEXT_DIM, lr.width() - 32.0);
    if link(ui, lr.right_bottom() + Vec2::new(-16.0, -12.0), Align2::RIGHT_BOTTOM, "Visualize", 15.0).clicked() {
        app.open_window(&ctx, Page::SpaceLens);
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

fn network(app: &mut App, ui: &mut Ui, r: Rect, s: &Snapshot) {
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
        monitor::start_speed_test(app.mon.clone(), ctx.clone());
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

pub fn claude_detail(app: &mut App, ui: &mut Ui, r: Rect) {
    let ctx = ui.ctx().clone();
    let cl = app.claude.lock().unwrap().clone();
    let p = ui.painter().clone();
    let t = ctx.input(|i| i.time);
    let mut y = r.top();
    // limits
    let lim_h = 52.0 + cl.limits.len().max(2) as f32 * 62.0;
    let lr = Rect::from_min_size(Pos2::new(r.left(), y), Vec2::new(r.width(), lim_h));
    glass_card(&p, lr, m::CARD_RADIUS, 0.0, false);
    p.text(lr.min + Vec2::new(16.0, 14.0), Align2::LEFT_TOP, "Plan limits", bold(16.0), TEXT);
    let upd = match (&cl.fetched, cl.loading) {
        (_, true) => "Updating…".to_string(),
        (Some(f), _) => format!("Updated {}", f.format("%H:%M")),
        _ => String::new(),
    };
    p.text(lr.right_top() + Vec2::new(-16.0, 16.0), Align2::RIGHT_TOP, upd, font(12.5), TEXT_FAINT);
    if cl.loading {
        spinner(&p, lr.right_top() + Vec2::new(-110.0, 23.0), 6.0, t, TEXT_DIM);
        ctx.request_repaint();
    }
    if cl.limits.is_empty() {
        let msg = cl.error.clone().unwrap_or_else(|| "Asking Claude Code for your limits…".into());
        paragraph(&p, lr.min + Vec2::new(16.0, 48.0), &msg, font(14.0), TEXT_DIM, lr.width() - 32.0);
    }
    for (i, l) in cl.limits.iter().enumerate() {
        let yy = lr.top() + 48.0 + i as f32 * 62.0;
        p.text(Pos2::new(lr.left() + 16.0, yy), Align2::LEFT_TOP, claude::short_label(&l.label), font(14.0), TEXT);
        p.text(Pos2::new(lr.right() - 16.0, yy), Align2::RIGHT_TOP, format!("{:.0}%", l.pct), bold(14.0), TEXT);
        let c = if l.pct >= 90.0 { RED } else if l.pct >= 70.0 { AMBER } else { BLUE };
        bar(&p, Rect::from_min_size(Pos2::new(lr.left() + 16.0, yy + 22.0), Vec2::new(lr.width() - 32.0, 7.0)), l.pct / 100.0, c);
        if !l.resets.is_empty() {
            text_fit(&p, Pos2::new(lr.left() + 16.0, yy + 36.0), Align2::LEFT_TOP, &format!("Resets {}", l.resets), font(12.0), TEXT_FAINT, lr.width() - 32.0);
        }
    }
    y = lr.bottom() + GAP;
    // local activity
    let ar = Rect::from_min_size(Pos2::new(r.left(), y), Vec2::new(r.width(), 150.0));
    glass_card(&p, ar, m::CARD_RADIUS, 0.0, false);
    p.text(ar.min + Vec2::new(16.0, 14.0), Align2::LEFT_TOP, "On this Mac", bold(16.0), TEXT);
    let stats = [("Last 5 hours", cl.local.tokens_5h), ("Today", cl.local.tokens_today), ("7 days", cl.local.tokens_7d)];
    for (i, (k, v)) in stats.iter().enumerate() {
        let x = ar.left() + 16.0 + i as f32 * ((ar.width() - 32.0) / 3.0);
        p.text(Pos2::new(x, ar.top() + 44.0), Align2::LEFT_TOP, claude::human_tokens(*v), bold(20.0), TEXT);
        p.text(Pos2::new(x, ar.top() + 70.0), Align2::LEFT_TOP, format!("tokens · {k}"), font(12.0), TEXT_FAINT);
    }
    let maxd = cl.local.daily.iter().copied().max().unwrap_or(1).max(1) as f32;
    let bw = (ar.width() - 32.0) / 7.0;
    for (i, v) in cl.local.daily.iter().enumerate() {
        let h = 34.0 * (*v as f32 / maxd);
        let br = Rect::from_min_max(Pos2::new(ar.left() + 16.0 + i as f32 * bw + 3.0, ar.bottom() - 12.0 - h.max(2.0)), Pos2::new(ar.left() + 16.0 + (i + 1) as f32 * bw - 3.0, ar.bottom() - 12.0));
        gradient_rect(&p, br, 3.0, Color32::from_rgb(255, 180, 140), Color32::from_rgb(210, 96, 66));
    }
    y = ar.bottom() + GAP;
    // what's driving usage
    let rest = Rect::from_min_max(Pos2::new(r.left(), y), r.max);
    glass_card(&p, rest, m::CARD_RADIUS, 0.0, false);
    p.text(rest.min + Vec2::new(16.0, 14.0), Align2::LEFT_TOP, "What uses your limits", bold(16.0), TEXT);
    let mut yy = rest.top() + 42.0;
    if let Some(c) = cl.contrib.first() {
        p.text(Pos2::new(rest.left() + 16.0, yy), Align2::LEFT_TOP, &c.title, font(13.0), TEXT_FAINT);
        yy += 22.0;
        for line in &c.lines {
            if yy > rest.bottom() - 60.0 {
                break;
            }
            p.circle_filled(Pos2::new(rest.left() + 20.0, yy + 8.0), 2.5, Color32::from_rgb(255, 170, 130));
            yy += paragraph(&p, Pos2::new(rest.left() + 30.0, yy), line, font(13.0), TEXT_DIM, rest.width() - 46.0) + 6.0;
        }
    } else {
        paragraph(&p, Pos2::new(rest.left() + 16.0, yy), "Claude Code reports this once you've used it on this Mac.", font(13.0), TEXT_DIM, rest.width() - 32.0);
    }
    let junk = app.clean.lock().unwrap().claude_safe_bytes();
    let label = if junk > 0 { format!("Clean {} of Claude junk", human_bytes(junk)) } else { "Clean Claude caches".into() };
    let bw = p.layout_no_wrap(label.clone(), bold(14.0), TEXT).size().x + 36.0;
    let br = Rect::from_min_size(Pos2::new(rest.right() - 16.0 - bw, rest.bottom() - 46.0), Vec2::new(bw, 32.0));
    if pill_button(ui, br, &label, 1, true).clicked() {
        app.clean_group = Some(crate::cleanup::Group::Claude);
        app.open_window(&ctx, Page::Cleanup);
    }
    let br2 = Rect::from_min_size(Pos2::new(rest.left() + 16.0, rest.bottom() - 46.0), Vec2::new(110.0, 32.0));
    if pill_button(ui, br2, "Details", 0, true).clicked() {
        app.open_window(&ctx, Page::Claude);
    }
}
