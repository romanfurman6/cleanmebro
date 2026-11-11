//! Live Mac health readings for the menu panel. A background thread refreshes a
//! shared snapshot; slow sources (system_profiler, folder sizes) run less often.

use crate::kit::sync::LockExt;
use crate::kit::activity::{self, Every};
use crate::kit::fs::dir_size;
use crate::kit::mechanics as m;
use serde_json::Value;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Level {
    #[default]
    Good,
    Fair,
    Critical,
}

impl Level {
    pub fn worst(self, other: Level) -> Level {
        use Level::*;
        match (self, other) {
            (Critical, _) | (_, Critical) => Critical,
            (Fair, _) | (_, Fair) => Fair,
            _ => Good,
        }
    }
}

#[derive(Clone, Debug, Default)]
pub struct Battery {
    pub present: bool,
    pub percent: u8,
    pub charging: bool,
    pub plugged: bool,
    pub full: bool,
    pub minutes_left: Option<u32>,
    pub cycles: u32,
    pub design_cycles: u32,
    pub health_pct: Option<f32>,
    pub temp_c: Option<f32>,
    /// macOS's own verdict ("Good", "Fair", "Poor", "Check Battery"...)
    pub health_text: String,
    pub permanent_failure: bool,
}

pub use super::devices::Device;
use super::devices;

#[derive(Clone, Debug, Default)]
pub struct Wifi {
    pub name: String,
    pub security: String,
    pub phy: String,
    pub channel: String,
    pub signal: String,
    pub wired: bool,
}

/// Where a speed test is.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum SpeedPhase {
    #[default]
    Idle,
    Ping,
    Download,
    Upload,
    Done,
}

#[derive(Clone, Debug, Default)]
pub struct SpeedTest {
    pub running: bool,
    pub phase: SpeedPhase,
    /// The rate of the transfer under way, in Mbps (for the gauge).
    pub live_mbps: f64,
    pub progress: f32,
    pub down_mbps: Option<f64>,
    pub up_mbps: Option<f64>,
    pub latency_ms: Option<f64>,
    pub error: Option<String>,
    /// Set by [`SpeedTest::stop`]; the test thread checks it between reads.
    pub cancel: Arc<AtomicBool>,
}

impl SpeedTest {
    pub fn stop(&self) {
        self.cancel.store(true, Ordering::Relaxed);
    }
}

#[derive(Clone, Debug, Default)]
pub struct Snapshot {
    pub ready: bool,
    pub model: String,
    pub chip: String,
    pub volume_name: String,
    pub disk_total: u64,
    pub disk_free: u64,
    pub smart_status: String,
    pub categories: Vec<(String, u64)>, // Applications, Documents, Downloads, Desktop
    pub trash_bytes: Option<u64>,
    /// The folders are being measured ([`measure_folders`]).
    pub sizing: bool,
    pub mem_total: u64,
    pub mem_pressure: f32,
    pub mem_pressure_level: u32,
    pub mem_active: u64,
    pub mem_wired: u64,
    pub mem_compressed: u64,
    pub mem_available: u64,
    pub swap_used: u64,
    pub swap_total: u64,
    pub cpu_load: f32,
    pub cpu_user: f32,
    pub cpu_system: f32,
    pub cpu_history: Vec<f32>,
    /// Memory pressure, one sample per refresh (last 60).
    pub mem_history: Vec<f32>,
    /// Download + upload in bytes/s, one sample per refresh (last 60).
    pub net_history: Vec<f32>,
    pub thermal: u8, // 0 nominal, 1 fair, 2 serious, 3 critical
    pub uptime_secs: u64,
    pub battery: Battery,
    pub net_up: f64,
    pub net_down: f64,
    pub net_sent_total: u64,
    pub net_recv_total: u64,
    pub online: bool,
    pub wifi: Wifi,
    pub devices: Vec<Device>,
    /// Device alert and menu bar settings (changed from the Devices sheet).
    pub bt_prefs: devices::Prefs,
    pub speed: SpeedTest,
}

impl Snapshot {
    pub fn disk_level(&self) -> Level {
        if self.disk_total == 0 {
            return Level::Good;
        }
        // the original raises "out of disk space" below this threshold;
        // half of it counts as critical here (estimated)
        let t = m::disk_alert_threshold(self.disk_total);
        if self.disk_free < t / 2 {
            Level::Critical
        } else if self.disk_free < t {
            Level::Fair
        } else {
            Level::Good
        }
    }
    pub fn mem_level(&self) -> Level {
        match self.mem_pressure_level {
            0 | m::KERNEL_PRESSURE_NORMAL => Level::Good,
            m::KERNEL_PRESSURE_WARN => Level::Fair,
            _ => Level::Critical,
        }
    }
    pub fn thermal_level(&self) -> Level {
        match self.thermal {
            0 | 1 => Level::Good,
            2 => Level::Fair,
            _ => Level::Critical,
        }
    }
    /// BatteryMonitor sub_25d4: "Check Battery" / "Permanent Battery Failure" or the
    /// failure flag → service; otherwise macOS's Good / Fair / Poor.
    pub fn battery_level(&self) -> Level {
        let b = &self.battery;
        match m::battery_health_state(&b.health_text, b.permanent_failure) {
            m::BatteryHealthState::Good | m::BatteryHealthState::Unknown => Level::Good,
            m::BatteryHealthState::Fair => Level::Fair,
            m::BatteryHealthState::Poor | m::BatteryHealthState::Service => Level::Critical,
        }
    }
    pub fn uptime_warning(&self) -> bool {
        self.uptime_secs > m::UPTIME_RESTART_INTERVAL_SECS
    }
    pub fn health(&self) -> Level {
        self.disk_level()
            .worst(self.mem_level())
            .worst(self.thermal_level())
            .worst(self.battery_level())
    }
    pub fn uptime_days(&self) -> u64 {
        self.uptime_secs / m::SECONDS_PER_DAY
    }
    pub fn trash_overfill(&self) -> bool {
        self.trash_bytes.map(|b| b > m::TRASH_SIZE_THRESHOLD).unwrap_or(false)
    }
    pub fn thermal_word(&self) -> &'static str {
        ["Cool", "Warm", "Hot", "Critical"][self.thermal.min(3) as usize]
    }
}

pub type Shared = Arc<Mutex<Snapshot>>;

/// Change the device alert / menu bar settings and save them.
pub fn change_bt_prefs(mon: &Shared, f: impl FnOnce(&mut devices::Prefs)) {
    let prefs = {
        let mut s = mon.lock_ok();
        f(&mut s.bt_prefs);
        s.bt_prefs
    };
    prefs.save();
}

pub fn start(ctx: egui::Context, demo: bool) -> Shared {
    let shared: Shared = Arc::new(Mutex::new(Snapshot { bt_prefs: devices::Prefs::load(), ..Snapshot::default() }));
    let s = shared.clone();
    std::thread::Builder::new()
        .name("monitor".into())
        .spawn(move || run(s, ctx, demo))
        .ok();
    shared
}

/// How often each slow source refreshes: (while on screen, while hidden).
/// Hidden, only what the menu-bar title and Bluetooth alerts need keeps going.
struct Cadence {
    shown: Duration,
    hidden: Option<Duration>,
}

impl Cadence {
    const fn secs(shown: u64, hidden: Option<u64>) -> Self {
        Cadence { shown: Duration::from_secs(shown), hidden: match hidden { Some(h) => Some(Duration::from_secs(h)), None => None } }
    }
    /// Due now? `None` while hidden means "not until it's shown again",
    /// except for the very first reading.
    fn due(&self, last: &mut Every) -> bool {
        match (activity::visible(), self.hidden) {
            (true, _) => last.due(self.shown),
            (false, Some(h)) => last.due(h),
            (false, None) => last.age().is_none() && last.due(self.shown),
        }
    }
}

const BATTERY_EVERY: Cadence = Cadence::secs(10, Some(60));
/// macOS's battery-health verdict changes over months.
const BATTERY_HEALTH_EVERY: Duration = Duration::from_secs(30 * 60);
/// Input devices (`ioreg`): batteries for the alerts and the menu-bar title.
const HID_EVERY: Cadence = Cadence::secs(10, Some(60));
/// The full Bluetooth report (`system_profiler`, slow).
const BT_PROFILE_EVERY: Cadence = Cadence::secs(60, Some(10 * 60));
/// The network's name (`ipconfig`, fast).
const WIFI_EVERY: Cadence = Cadence::secs(60, None);
/// Channel, signal and so on (`system_profiler`, ~10 s as it may scan).
const WIFI_DETAILS_EVERY: Cadence = Cadence::secs(10 * 60, None);
/// The Trash's size, for the "Empty Trash" nudge (one folder only).
const TRASH_EVERY: Cadence = Cadence::secs(600, None);
/// A folder walk is running (on its own thread).
static SIZING: AtomicBool = AtomicBool::new(false);

fn home_dir() -> PathBuf {
    dirs::home_dir().unwrap_or_else(|| PathBuf::from("/"))
}

/// Run `f` on its own thread at background priority, one walk at a time.
fn in_background(name: &str, f: impl FnOnce() + Send + 'static) {
    if SIZING.swap(true, Ordering::Relaxed) {
        return;
    }
    let spawned = std::thread::Builder::new().name(name.into()).spawn(move || {
        activity::low_priority();
        f();
        SIZING.store(false, Ordering::Relaxed);
    });
    if spawned.is_err() {
        SIZING.store(false, Ordering::Relaxed);
    }
}

/// Measure the storage chart's folders (Applications, Documents, Downloads,
/// Desktop, Trash). Only on the user's click: it walks every file.
/// Documents, Downloads and Desktop are behind macOS's folder questions, so
/// without Full Disk Access the click asks for that instead.
pub fn measure_folders(shared: Shared, ctx: egui::Context) {
    use crate::kit::perms::{Perm, Status};
    if SIZING.load(Ordering::Relaxed) {
        return;
    }
    if Perm::FullDisk.status() != Status::Allowed {
        Perm::FullDisk.ask();
        return;
    }
    shared.lock_ok().sizing = true;
    ctx.request_repaint();
    let s = shared.clone();
    in_background("folder-sizes", move || {
        let home = home_dir();
        let places = [("Applications", PathBuf::from("/Applications")), ("Documents", home.join("Documents")), ("Downloads", home.join("Downloads")), ("Desktop", home.join("Desktop"))];
        // each folder on its own thread: the slowest one sets the wait
        let (cats, trash) = std::thread::scope(|sc| {
            let jobs: Vec<_> = places.iter().map(|(n, p)| sc.spawn(move || (n.to_string(), dir_size(p)))).collect();
            let t = home.join(".Trash");
            let trash = std::fs::read_dir(&t).ok().map(|_| dir_size(&t));
            (jobs.into_iter().map(|j| j.join().unwrap_or_default()).collect::<Vec<_>>(), trash)
        });
        let mut g = s.lock_ok();
        g.categories = cats;
        g.trash_bytes = trash;
        g.sizing = false;
        drop(g);
        ctx.request_repaint();
    });
}

fn run(shared: Shared, ctx: egui::Context, _demo: bool) {
    let mut sys = sysinfo::System::new();
    let mut nets = sysinfo::Networks::new_with_refreshed_list();
    let mut last_ticks = cpu_ticks();
    let mut last_net = Instant::now();
    // the slow Bluetooth report, refreshed from the input registry in between
    let mut bt_profile: Vec<Device> = Vec::new();
    let mut bt_alerts = devices::Alerts::default();
    let (mut battery_at, mut health_at, mut hid_at, mut profile_at) = (Every::default(), Every::default(), Every::default(), Every::default());
    let (mut wifi_at, mut wifi_details_at, mut sizes_at) = (Every::default(), Every::default(), Every::default());
    let mut wifi_info = Wifi::default();
    let mut health_text = String::new();
    // one-off facts
    {
        let (model, chip) = hardware();
        let (vol, smart) = disk_info();
        let mut s = shared.lock_ok();
        s.model = model;
        s.chip = chip;
        s.volume_name = vol;
        s.smart_status = smart;
    }
    loop {
        // fast readings every refresh: all plain system calls, no processes
        sys.refresh_memory();
        nets.refresh(true);
        let now_ticks = cpu_ticks();
        let (load, user, system) = cpu_split(last_ticks, now_ticks);
        last_ticks = now_ticks;
        let dt = last_net.elapsed().as_secs_f64().max(0.2);
        last_net = Instant::now();
        let (mut rx, mut tx, mut rx_total, mut tx_total) = (0u64, 0u64, 0u64, 0u64);
        for (name, data) in nets.iter() {
            if name.starts_with("en") {
                rx += data.received();
                tx += data.transmitted();
                rx_total += data.total_received();
                tx_total += data.total_transmitted();
            }
        }
        let (total, free) = crate::kit::fs::volume_space("/");
        let vm = vm_counts();
        let (swap_used, swap_total) = swap();
        // kernel pressure level (SystemResourcesMonitor sub_9504): 1 normal, 2 warning, else critical
        let pressure_level = sysctl_u32("kern.memorystatus_vm_pressure_level").unwrap_or(1);
        let thermal = crate::kit::native::thermal_state();
        let uptime = sysinfo::System::uptime();

        let battery = BATTERY_EVERY.due(&mut battery_at).then(|| {
            let mut b = parse_battery(&run_cmd("ioreg", &["-rn", "AppleSmartBattery"]));
            if b.present {
                if health_text.is_empty() || health_at.due(BATTERY_HEALTH_EVERY) {
                    health_text = power_health_text();
                }
                b.health_text = health_text.clone();
            }
            b
        });
        if WIFI_DETAILS_EVERY.due(&mut wifi_details_at) {
            wifi_info = wifi_details();
        }
        let wifi = WIFI_EVERY.due(&mut wifi_at).then(|| wifi_name(&wifi_info));
        let slow_profile = BT_PROFILE_EVERY.due(&mut profile_at);
        if slow_profile {
            bt_profile = devices::parse_profiler(&run_cmd("system_profiler", &["SPBluetoothDataType", "-json"]));
        }
        let devices = (HID_EVERY.due(&mut hid_at) || slow_profile).then(|| {
            let mut list = bt_profile.clone();
            devices::merge(&mut list, &devices::parse_ioreg(&run_cmd("ioreg", &["-r", "-l", "-k", "BatteryPercent"])));
            list
        });

        {
            let mut s = shared.lock_ok();
            s.ready = true;
            s.disk_total = total;
            s.disk_free = free;
            s.mem_total = sys.total_memory();
            s.mem_pressure_level = pressure_level;
            if let Some(vm) = vm {
                s.mem_active = vm.0;
                s.mem_wired = vm.1;
                // "Pressure: N%" = (wired + compressed) / physical memory (sub_90b8)
                s.mem_pressure = m::memory_pressure_pct(vm.1, vm.2, s.mem_total);
                s.mem_compressed = vm.2;
                s.mem_available = s.mem_total.saturating_sub(vm.0 + vm.1 + vm.2);
            }
            s.swap_used = swap_used;
            s.swap_total = swap_total;
            s.cpu_load = load;
            s.cpu_user = user;
            s.cpu_system = system;
            s.cpu_history.push(load);
            if s.cpu_history.len() > 60 {
                s.cpu_history.remove(0);
            }
            let pressure = s.mem_pressure;
            s.mem_history.push(pressure);
            if s.mem_history.len() > 60 {
                s.mem_history.remove(0);
            }
            s.thermal = thermal;
            s.uptime_secs = uptime;
            s.net_down = rx as f64 / dt;
            s.net_up = tx as f64 / dt;
            let rate = (s.net_down + s.net_up) as f32;
            s.net_history.push(rate);
            if s.net_history.len() > 60 {
                s.net_history.remove(0);
            }
            s.net_recv_total = rx_total;
            s.net_sent_total = tx_total;
            s.online = rx_total + tx_total > 0;
            if let Some(b) = battery {
                s.battery = b;
            }
            if let Some(d) = devices {
                if s.bt_prefs.alerts {
                    for (title, body) in bt_alerts.check(&d) {
                        crate::kit::notify::send(&title, &body);
                    }
                } else {
                    // stay quiet, but remember so turning alerts on doesn't replay old ones
                    bt_alerts.check(&d);
                }
                s.devices = d;
            }
            if let Some(w) = wifi {
                s.wifi = w;
            }
        }
        activity::repaint(&ctx);

        // only the Trash is measured on its own (for the "Empty Trash" nudge);
        // the big folders wait for a click ([`measure_folders`])
        if !activity::recording() && !SIZING.load(Ordering::Relaxed) && TRASH_EVERY.due(&mut sizes_at) {
            let (shared, ctx) = (shared.clone(), ctx.clone());
            in_background("trash-size", move || {
                let trash = home_dir().join(".Trash");
                let bytes = std::fs::read_dir(&trash).ok().map(|_| dir_size(&trash));
                shared.lock_ok().trash_bytes = bytes;
                activity::repaint(&ctx);
            });
        }
        // the panel opening ends the nap: it shows fresh numbers right away
        activity::nap(Duration::from_millis(m::MONITOR_REFRESH_MS), None);
    }
}

// ---------------------------------------------------------------- sources

fn run_cmd(cmd: &str, args: &[&str]) -> String {
    Command::new(cmd)
        .args(args)
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).into_owned())
        .unwrap_or_default()
}

fn hardware() -> (String, String) {
    let out = run_cmd("system_profiler", &["SPHardwareDataType", "-json"]);
    let v: Value = serde_json::from_str(&out).unwrap_or(Value::Null);
    let item = &v["SPHardwareDataType"][0];
    let model = item["machine_name"].as_str().unwrap_or("Mac").to_string();
    let chip = item["chip_type"].as_str().or(item["cpu_type"].as_str()).unwrap_or("").to_string();
    (model, chip)
}

fn disk_info() -> (String, String) {
    let out = run_cmd("diskutil", &["info", "/"]);
    let mut name = "Macintosh HD".to_string();
    let mut smart = String::new();
    for line in out.lines() {
        let l = line.trim();
        if let Some(v) = l.strip_prefix("Volume Name:") {
            name = v.trim().to_string();
        }
        if let Some(v) = l.strip_prefix("SMART Status:") {
            smart = v.trim().to_string();
        }
    }
    (name, smart)
}

#[cfg(target_os = "macos")]
pub fn sysctl_u32(name: &str) -> Option<u32> {
    let c = std::ffi::CString::new(name).ok()?;
    let mut v: u32 = 0;
    let mut len = std::mem::size_of::<u32>();
    let r = unsafe { libc::sysctlbyname(c.as_ptr(), &mut v as *mut u32 as *mut _, &mut len, std::ptr::null_mut(), 0) };
    (r == 0).then_some(v)
}

#[cfg(target_os = "macos")]
fn swap() -> (u64, u64) {
    #[repr(C)]
    struct XswUsage {
        total: u64,
        avail: u64,
        used: u64,
        pagesize: u32,
        encrypted: u32,
    }
    let c = std::ffi::CString::new("vm.swapusage").unwrap();
    let mut v: XswUsage = unsafe { std::mem::zeroed() };
    let mut len = std::mem::size_of::<XswUsage>();
    let r = unsafe { libc::sysctlbyname(c.as_ptr(), &mut v as *mut XswUsage as *mut _, &mut len, std::ptr::null_mut(), 0) };
    if r == 0 {
        (v.used, v.total)
    } else {
        (0, 0)
    }
}

/// (app memory, wired, compressed) bytes, as `vm_stat` and the original's
/// RAMService compute them (SystemResourcesMonitor sub_90b8):
/// app memory = anonymous (internal) − purgeable pages. Read straight from
/// the kernel instead of starting `vm_stat` every refresh.
#[cfg(target_os = "macos")]
fn vm_counts() -> Option<(u64, u64, u64)> {
    unsafe {
        let mut info: libc::vm_statistics64 = std::mem::zeroed();
        let mut count = libc::HOST_VM_INFO64_COUNT;
        #[allow(deprecated)]
        let host = libc::mach_host_self();
        let r = libc::host_statistics64(host, libc::HOST_VM_INFO64, &mut info as *mut _ as *mut i32, &mut count);
        if r != 0 {
            return None;
        }
        #[allow(deprecated)]
        let page = libc::vm_page_size as u64;
        Some(mem_split(info.internal_page_count as u64, info.purgeable_count as u64, info.wire_count as u64, info.compressor_page_count as u64, page))
    }
}

/// (app memory, wired, compressed) bytes from page counts.
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
fn mem_split(internal: u64, purgeable: u64, wired: u64, compressed: u64, page: u64) -> (u64, u64, u64) {
    (internal.saturating_sub(purgeable) * page, wired * page, compressed * page)
}

#[cfg(target_os = "macos")]
fn cpu_ticks() -> [u64; 4] {
    unsafe {
        let mut info: libc::host_cpu_load_info = std::mem::zeroed();
        let mut count = libc::HOST_CPU_LOAD_INFO_COUNT;
        #[allow(deprecated)]
        let host = libc::mach_host_self();
        let r = libc::host_statistics(host, libc::HOST_CPU_LOAD_INFO, &mut info as *mut _ as *mut i32, &mut count);
        if r != 0 {
            return [0; 4];
        }
        let t = info.cpu_ticks;
        [t[0] as u64, t[1] as u64, t[2] as u64, t[3] as u64] // user, system, idle, nice
    }
}

// Other platforms build (so the tests run anywhere) but read nothing.
#[cfg(not(target_os = "macos"))]
pub fn sysctl_u32(_name: &str) -> Option<u32> {
    None
}
#[cfg(not(target_os = "macos"))]
fn vm_counts() -> Option<(u64, u64, u64)> {
    None
}
#[cfg(not(target_os = "macos"))]
fn swap() -> (u64, u64) {
    (0, 0)
}
#[cfg(not(target_os = "macos"))]
fn cpu_ticks() -> [u64; 4] {
    [0; 4]
}

fn cpu_split(a: [u64; 4], b: [u64; 4]) -> (f32, f32, f32) {
    let d: Vec<f64> = (0..4).map(|i| b[i].saturating_sub(a[i]) as f64).collect();
    let total = d.iter().sum::<f64>();
    if total <= 0.0 {
        return (0.0, 0.0, 0.0);
    }
    let user = ((d[0] + d[3]) / total * 100.0) as f32;
    let system = (d[1] / total * 100.0) as f32;
    (user + system, user, system)
}

/// Battery readings from `ioreg -rn AppleSmartBattery`.
fn parse_battery(out: &str) -> Battery {
    let mut b = Battery::default();
    if out.trim().is_empty() {
        return b;
    }
    b.present = true;
    let mut design = 0f32;
    let mut nominal = 0f32;
    let mut raw_max = 0f32;
    let (mut to_empty, mut to_full) = (None, None);
    for line in out.lines() {
        let l = line.trim_start_matches([' ', '|']).trim();
        let Some((k, v)) = l.split_once(" = ") else { continue };
        let k = k.trim_matches('"');
        let v = v.trim();
        let n = || v.parse::<f64>().ok();
        match k {
            "CurrentCapacity" => b.percent = n().unwrap_or(0.0).clamp(0.0, 100.0) as u8,
            "IsCharging" => b.charging = v == "Yes",
            "ExternalConnected" => b.plugged = v == "Yes",
            "FullyCharged" => b.full = v == "Yes",
            "AvgTimeToEmpty" => to_empty = n(),
            "AvgTimeToFull" => to_full = n(),
            "PermanentFailureStatus" => b.permanent_failure = n().unwrap_or(0.0) != 0.0,
            "CycleCount" => b.cycles = n().unwrap_or(0.0) as u32,
            "DesignCycleCount9C" => b.design_cycles = n().unwrap_or(0.0) as u32,
            "DesignCapacity" => design = n().unwrap_or(0.0) as f32,
            "NominalChargeCapacity" => nominal = n().unwrap_or(0.0) as f32,
            "AppleRawMaxCapacity" => raw_max = n().unwrap_or(0.0) as f32,
            "Temperature" => b.temp_c = n().map(|t| t as f32 / m::BATTERY_TEMP_DIVISOR),
            _ => {}
        }
    }
    // AvgTimeToEmpty / AvgTimeToFull, 65535 = unknown. Picked after the loop:
    // ioreg lists keys alphabetically, so "IsCharging" comes after both.
    let t = if b.charging { to_full } else { to_empty };
    b.minutes_left = t.filter(|&t| t > 0.0 && t < m::BATTERY_TIME_UNKNOWN).map(|t| t as u32);
    // Apple Silicon reports health from AppleRawMaxCapacity
    let max = if raw_max > 0.0 { raw_max } else { nominal };
    if design > 0.0 && max > 0.0 {
        b.health_pct = Some((max / design * 100.0).min(100.0));
    }
    b
}

/// macOS's battery verdict, the same text the original reads from the power
/// source ("BatteryHealth" / "BatteryHealthCondition").
fn power_health_text() -> String {
    let out = run_cmd("system_profiler", &["SPPowerDataType", "-json"]);
    let v: Value = serde_json::from_str(&out).unwrap_or(Value::Null);
    if let Some(arr) = v["SPPowerDataType"].as_array() {
        for e in arr {
            if let Some(h) = e["sppower_battery_health_info"]["sppower_battery_health"].as_str() {
                return h.to_string();
            }
        }
    }
    String::new()
}

/// Wi-Fi details from `system_profiler`. Slow: it can take ~10 s, as it
/// may scan for networks, so it runs rarely; [`wifi_name`] fills in between.
fn wifi_details() -> Wifi {
    let mut w = Wifi::default();
    let out = run_cmd("system_profiler", &["SPAirPortDataType", "-json"]);
    let v: Value = serde_json::from_str(&out).unwrap_or(Value::Null);
    if let Some(ifaces) = v["SPAirPortDataType"][0]["spairport_airport_interfaces"].as_array() {
        for i in ifaces {
            let cur = &i["spairport_current_network_information"];
            if cur.is_object() {
                w.name = cur["_name"].as_str().unwrap_or("").to_string();
                w.security = cur["spairport_security_mode"]
                    .as_str()
                    .unwrap_or("")
                    .replace("spairport_security_mode_", "")
                    .replace('_', " ")
                    .to_uppercase();
                w.phy = cur["spairport_network_phymode"].as_str().unwrap_or("").to_string();
                w.channel = cur["spairport_network_channel"].as_str().unwrap_or("").to_string();
                w.signal = cur["spairport_signal_noise"].as_str().unwrap_or("").to_string();
            }
        }
    }
    w
}

/// The network's name and security from `ipconfig` (fast), over what
/// [`wifi_details`] last found.
fn wifi_name(details: &Wifi) -> Wifi {
    let mut w = details.clone();
    if w.name.contains("redacted") {
        w.name.clear();
    }
    // macOS hides the network name without Location access
    apply_ipconfig(&mut w, &run_cmd("ipconfig", &["getsummary", "en0"]));
    if w.name.is_empty() {
        w.name = if w.security.is_empty() { "Network".into() } else { "Wi-Fi".into() };
    }
    w
}

fn apply_ipconfig(w: &mut Wifi, out: &str) {
    for line in out.lines() {
        let l = line.trim();
        if let Some(v) = l.strip_prefix("SSID : ") {
            if !v.contains("redacted") {
                w.name = v.to_string();
            }
        }
        if let Some(v) = l.strip_prefix("Security : ") {
            if w.security.is_empty() {
                w.security = v.replace('_', " ");
            }
        }
    }
}

// ---------------------------------------------------------------- speed test

pub fn start_speed_test(shared: Shared, ctx: egui::Context) {
    {
        let mut s = shared.lock_ok();
        if s.speed.running {
            return;
        }
        s.speed = SpeedTest { running: true, phase: SpeedPhase::Ping, ..Default::default() };
    }
    let cancel = shared.lock_ok().speed.cancel.clone();
    let stopped = move || cancel.load(Ordering::Relaxed);
    std::thread::spawn(move || {
        let set = |f: &dyn Fn(&mut SpeedTest)| {
            let mut s = shared.lock_ok();
            f(&mut s.speed);
            ctx.request_repaint();
        };
        // a stop ends the test quietly with what it has so far
        let fail = |msg: String| {
            let quiet = stopped();
            set(&|s| {
                s.running = false;
                s.phase = SpeedPhase::Done;
                s.live_mbps = 0.0;
                s.error = if quiet { None } else { Some(msg.clone()) };
            })
        };
        let agent = ureq::Agent::new_with_config(
            ureq::Agent::config_builder().timeout_global(Some(Duration::from_secs(30))).build(),
        );
        // latency: the first call opens the connection, the median of the
        // next few is the round trip
        let ping_url = format!("{}?bytes=0", m::SPEED_TEST_DOWN_URL);
        if let Err(e) = agent.get(&ping_url).call() {
            fail(format!("No internet connection ({e})"));
            return;
        }
        let mut rtts = Vec::new();
        for _ in 0..5 {
            if stopped() {
                fail(String::new());
                return;
            }
            let t0 = Instant::now();
            if agent.get(&ping_url).call().is_ok() {
                rtts.push(t0.elapsed().as_secs_f64() * 1000.0);
            }
        }
        rtts.sort_by(|a, b| a.total_cmp(b));
        let latency = rtts.get(rtts.len() / 2).copied().unwrap_or(0.0);
        set(&|s| {
            s.latency_ms = Some(latency);
            s.progress = 0.1;
            s.phase = SpeedPhase::Download;
        });
        // download, reporting the rate as it goes
        let t0 = Instant::now();
        let url = format!("{}?bytes={}", m::SPEED_TEST_DOWN_URL, m::SPEED_TEST_BYTES);
        let down = agent.get(&url).call().and_then(|r| {
            let mut reader = r.into_body().into_reader();
            let mut buf = vec![0u8; 64 * 1024];
            let mut got = 0u64;
            let mut last = Instant::now();
            loop {
                use std::io::Read;
                if stopped() {
                    return Err(ureq::Error::Io(std::io::Error::other("stopped")));
                }
                let n = reader.read(&mut buf).map_err(ureq::Error::Io)?;
                if n == 0 {
                    break;
                }
                got += n as u64;
                if last.elapsed() >= Duration::from_millis(80) {
                    last = Instant::now();
                    let mbps = got as f64 * 8.0 / t0.elapsed().as_secs_f64() / 1e6;
                    let frac = got as f32 / m::SPEED_TEST_BYTES as f32;
                    set(&|s| {
                        s.live_mbps = mbps;
                        s.progress = 0.1 + 0.5 * frac.min(1.0);
                    });
                }
            }
            Ok(got)
        });
        match down {
            Ok(bytes) => {
                let mbps = bytes as f64 * 8.0 / t0.elapsed().as_secs_f64() / 1e6;
                set(&|s| {
                    s.down_mbps = Some(mbps);
                    s.live_mbps = 0.0;
                    s.progress = 0.6;
                    s.phase = SpeedPhase::Upload;
                });
            }
            Err(e) => {
                fail(format!("Network error: {e}"));
                return;
            }
        }
        // upload, streamed from a counting reader so the gauge can follow
        let total = m::SPEED_TEST_UPLOAD_BYTES;
        let t0 = Instant::now();
        let report = |sent: u64| {
            let secs = t0.elapsed().as_secs_f64().max(1e-3);
            let mbps = sent as f64 * 8.0 / secs / 1e6;
            let frac = sent as f32 / total as f32;
            set(&|s| {
                s.live_mbps = mbps;
                s.progress = 0.6 + 0.4 * frac.min(1.0);
            });
        };
        let mut reader = Zeros { left: total, sent: 0, last: Instant::now(), report: &report, stopped: &stopped };
        let up = agent
            .post(m::SPEED_TEST_UP_URL)
            .header("Content-Type", "application/octet-stream")
            .header("Content-Length", &total.to_string())
            .send(ureq::SendBody::from_reader(&mut reader));
        let secs = t0.elapsed().as_secs_f64();
        let quiet = stopped();
        set(&|s| {
            match &up {
                Ok(_) => s.up_mbps = Some(total as f64 * 8.0 / secs / 1e6),
                Err(_) if quiet => {}
                Err(e) => s.error = Some(format!("Upload test failed: {e}")),
            }
            s.progress = 1.0;
            s.live_mbps = 0.0;
            s.phase = SpeedPhase::Done;
            s.running = false;
        });
    });
}

/// `left` zero bytes for an upload body; calls `report` with the bytes
/// handed out so far every ~80 ms.
struct Zeros<'a> {
    left: u64,
    sent: u64,
    last: Instant,
    report: &'a dyn Fn(u64),
    stopped: &'a dyn Fn() -> bool,
}

impl std::io::Read for Zeros<'_> {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        if (self.stopped)() {
            return Err(std::io::Error::other("stopped"));
        }
        let n = (buf.len() as u64).min(self.left).min(64 * 1024) as usize;
        buf[..n].fill(0);
        self.left -= n as u64;
        self.sent += n as u64;
        if self.last.elapsed() >= Duration::from_millis(80) {
            self.last = Instant::now();
            (self.report)(self.sent);
        }
        Ok(n)
    }
}

/// "Good for" activities, by download speed in Mbps.
pub fn good_for(mbps: f64) -> Vec<&'static str> {
    m::GOOD_FOR
        .iter()
        .filter(|(_, need)| mbps >= *need)
        .map(|(name, _)| *name)
        .collect()
}

// ---------------------------------------------------------------- demo data

/// Fixed readings for screenshots, so the picture never shows personal data.
pub fn demo_snapshot() -> Snapshot {
    Snapshot {
        ready: true,
        model: "MacBook Pro".into(),
        chip: "Apple M3 Max".into(),
        volume_name: "Macintosh HD".into(),
        disk_total: 2_000_000_000_000,
        disk_free: 1_440_000_000_000,
        smart_status: "Verified".into(),
        categories: vec![
            ("Applications".into(), 65_600_000_000),
            ("Documents".into(), 940_700_000),
            ("Downloads".into(), 596_900_000),
            ("Desktop".into(), 188_000),
        ],
        trash_bytes: Some(46_100_000),
        sizing: false,
        mem_total: 68_719_476_736,
        mem_pressure: 17.0,
        mem_pressure_level: 1,
        mem_active: 24_000_000_000,
        mem_wired: 7_600_000_000,
        mem_compressed: 7_000_000_000,
        mem_available: 30_000_000_000,
        swap_used: 4_190_000,
        swap_total: 1_073_741_824,
        cpu_load: 16.0,
        cpu_user: 11.0,
        cpu_system: 5.0,
        cpu_history: demo_series(60, 7, 14.0, 9.0, &[(22, 38.0), (41, 31.0)]),
        mem_history: demo_series(60, 3, 17.0, 3.0, &[]),
        net_history: demo_series(60, 11, 40_000.0, 60_000.0, &[(18, 3_200_000.0), (47, 900_000.0)]),
        thermal: 0,
        uptime_secs: 3 * 86_400 + 5 * 3600,
        battery: Battery {
            present: true,
            percent: 80,
            charging: false,
            plugged: true,
            full: false,
            minutes_left: None,
            cycles: 13,
            design_cycles: 1000,
            health_pct: Some(100.0),
            temp_c: Some(30.3),
            health_text: "Good".into(),
            permanent_failure: false,
        },
        net_up: 3_200_000.0,
        net_down: 20_000.0,
        net_sent_total: 1_200_000_000,
        net_recv_total: 5_400_000_000,
        online: true,
        wifi: Wifi { name: "Home Wi-Fi".into(), security: "WPA3 PERSONAL".into(), phy: "802.11ax".into(), channel: "149 (5GHz, 80MHz)".into(), signal: "-52 dBm / -92 dBm".into(), wired: false },
        devices: devices::demo(),
        bt_prefs: devices::Prefs::default(),
        speed: SpeedTest::default(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const IOREG: &str = r#"+-o AppleSmartBattery  <class AppleSmartBattery>
    {
      "AppleRawMaxCapacity" = 4500
      "AvgTimeToEmpty" = 65535
      "AvgTimeToFull" = 95
      "CurrentCapacity" = 62
      "CycleCount" = 13
      "DesignCapacity" = 5000
      "DesignCycleCount9C" = 1000
      "ExternalConnected" = Yes
      "FullyCharged" = No
      "IsCharging" = Yes
      "NominalChargeCapacity" = 4400
      "PermanentFailureStatus" = 0
      "Temperature" = 3030
    }
"#;

    #[test]
    fn battery_time_left_follows_charging_state() {
        // IsCharging comes after the time keys; charging must still pick AvgTimeToFull
        let b = parse_battery(IOREG);
        assert!(b.present && b.charging && b.plugged && !b.full);
        assert_eq!(b.percent, 62);
        assert_eq!(b.minutes_left, Some(95));
        assert_eq!(b.cycles, 13);
        assert_eq!(b.design_cycles, 1000);
        assert_eq!(b.health_pct, Some(90.0));
        assert_eq!(b.temp_c, Some(30.3));
        assert!(!b.permanent_failure);

        let on_battery = IOREG.replace("\"IsCharging\" = Yes", "\"IsCharging\" = No").replace("\"AvgTimeToEmpty\" = 65535", "\"AvgTimeToEmpty\" = 240");
        let b = parse_battery(&on_battery);
        assert!(!b.charging);
        assert_eq!(b.minutes_left, Some(240));

        // 65535 means unknown
        let unknown = IOREG.replace("\"IsCharging\" = Yes", "\"IsCharging\" = No");
        assert_eq!(parse_battery(&unknown).minutes_left, None);
    }

    #[test]
    fn no_battery() {
        let b = parse_battery("  \n");
        assert!(!b.present);
    }

    #[test]
    fn memory_split_from_pages() {
        // 16 KB pages: app = anonymous − purgeable
        assert_eq!(mem_split(1000, 200, 300, 50, 16384), (800 * 16384, 300 * 16384, 50 * 16384));
        assert_eq!(mem_split(10, 20, 0, 0, 4096), (0, 0, 0));
    }

    #[test]
    fn wifi_name_from_ipconfig() {
        let mut w = Wifi::default();
        apply_ipconfig(&mut w, "  SSID : <redacted>\n  Security : WPA2_Personal\n");
        assert_eq!((w.name.as_str(), w.security.as_str()), ("", "WPA2 Personal"));
        apply_ipconfig(&mut w, "  SSID : Home\n");
        assert_eq!(w.name, "Home");
    }

    #[test]
    fn cpu_split_shares() {
        let (load, user, sys) = cpu_split([0, 0, 0, 0], [30, 10, 50, 10]);
        assert!((user - 40.0).abs() < 1e-4); // user + nice
        assert!((sys - 10.0).abs() < 1e-4);
        assert!((load - 50.0).abs() < 1e-4);
        assert_eq!(cpu_split([5; 4], [5; 4]), (0.0, 0.0, 0.0));
        // counters going backwards never underflow
        assert_eq!(cpu_split([9; 4], [1; 4]), (0.0, 0.0, 0.0));
    }

    #[test]
    fn levels() {
        assert_eq!(Level::Good.worst(Level::Fair), Level::Fair);
        assert_eq!(Level::Critical.worst(Level::Good), Level::Critical);
        let mut s = demo_snapshot();
        assert_eq!(s.health(), Level::Good);
        // 2 TB disk → threshold 100 GB
        s.disk_free = 80_000_000_000;
        assert_eq!(s.disk_level(), Level::Fair);
        s.disk_free = 40_000_000_000;
        assert_eq!(s.disk_level(), Level::Critical);
        assert_eq!(s.health(), Level::Critical);
        s.mem_pressure_level = 2;
        assert_eq!(s.mem_level(), Level::Fair);
        s.mem_pressure_level = 4;
        assert_eq!(s.mem_level(), Level::Critical);
        s.thermal = 9;
        assert_eq!(s.thermal_word(), "Critical");
        s.uptime_secs = 8 * 86_400;
        assert!(s.uptime_warning());
        assert_eq!(s.uptime_days(), 8);
    }

    #[test]
    fn good_for_by_speed() {
        assert!(good_for(0.1).is_empty());
        assert_eq!(good_for(4.0), vec!["Messaging", "Audio calls", "Music streaming", "Video calls"]);
        assert_eq!(good_for(1000.0).len(), m::GOOD_FOR.len());
    }


}

/// Sample data that looks like a real reading: noise around `base` plus a
/// few spikes, not a smooth curve.
fn demo_series(n: usize, seed: u32, base: f32, spread: f32, spikes: &[(usize, f32)]) -> Vec<f32> {
    let mut x = seed.wrapping_mul(2_654_435_761).max(1);
    (0..n)
        .map(|i| {
            x ^= x << 13;
            x ^= x >> 17;
            x ^= x << 5;
            let noise = (x % 1000) as f32 / 1000.0;
            spikes.iter().find(|(k, _)| *k == i).map(|(_, v)| *v).unwrap_or(base + noise * spread)
        })
        .collect()
}
