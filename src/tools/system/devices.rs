//! Bluetooth device batteries: reading them (the Bluetooth report plus the
//! faster input-device registry), AirPods split into left, right and case,
//! low-battery alerts and the optional menu bar badge.

use crate::kit::glass::{Icon, AMBER, GREEN, RED, TEXT};
use crate::kit::mechanics as m;
use egui::Color32;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use std::path::PathBuf;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Kind {
    Earbuds,
    Headphones,
    Mouse,
    Keyboard,
    Trackpad,
    #[default]
    Other,
}

impl Kind {
    pub fn icon(self) -> Icon {
        match self {
            Kind::Earbuds => Icon::Earbuds,
            Kind::Headphones => Icon::Headphones,
            Kind::Mouse => Icon::Mouse,
            Kind::Keyboard => Icon::Keyboard,
            Kind::Trackpad => Icon::Trackpad,
            Kind::Other => Icon::Bluetooth,
        }
    }
    pub fn label(self) -> &'static str {
        match self {
            Kind::Earbuds | Kind::Headphones => "Headphones",
            Kind::Mouse => "Mouse",
            Kind::Keyboard => "Keyboard",
            Kind::Trackpad => "Trackpad",
            Kind::Other => "Device",
        }
    }
    /// Guess the kind from macOS's minor type and the device's name.
    pub fn guess(minor: &str, name: &str) -> Kind {
        let n = name.to_lowercase();
        let t = minor.to_lowercase();
        if n.contains("airpods max") {
            Kind::Headphones
        } else if n.contains("airpods") || n.contains("buds") || n.contains("beats fit") || n.contains("powerbeats") || n.contains("studio buds") {
            Kind::Earbuds
        } else if t.contains("mouse") || n.contains("mouse") || n.contains("mx master") || n.contains("mx anywhere") {
            Kind::Mouse
        } else if t.contains("trackpad") || n.contains("trackpad") {
            Kind::Trackpad
        } else if t.contains("keyboard") || n.contains("keyboard") || n.contains("keys") {
            Kind::Keyboard
        } else if t.contains("head") || t.contains("audio") || t.contains("speaker") || n.contains("headphones") {
            Kind::Headphones
        } else {
            Kind::Other
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Device {
    pub name: String,
    pub kind: Kind,
    /// Bluetooth address, lower case with dashes ("aa-bb-cc-dd-ee-ff"), to
    /// match the two sources; empty when unknown.
    pub address: String,
    /// One battery (mouse, keyboard, over-ear headphones).
    pub main: Option<u8>,
    /// Earbuds report each side and the case separately.
    pub left: Option<u8>,
    pub right: Option<u8>,
    pub case: Option<u8>,
}

impl Device {
    /// SF Symbols for this exact model, best first: macOS knows AirPods,
    /// AirPods Max, Beats and Magic accessories by name. Several spellings
    /// are listed because symbol names changed between macOS releases; the
    /// first one this Mac has wins, and the kind's own icon is the fallback.
    pub fn symbols(&self) -> &'static [&'static str] {
        let n = self.name.to_lowercase();
        match self.kind {
            _ if n.contains("airpods max") => &["airpods.max", "airpodsmax"],
            _ if n.contains("airpods pro") => &["airpods.pro", "airpodspro"],
            _ if n.contains("airpods 4") || n.contains("airpods (3") || n.contains("airpods 3") => &["airpods.gen3"],
            _ if n.contains("airpods") => &["airpods"],
            _ if n.contains("beats") && self.kind == Kind::Headphones => &["beats.headphones"],
            _ if n.contains("beats") || n.contains("powerbeats") => &["beats.earphones", "earbuds"],
            Kind::Earbuds => &["earbuds"],
            Kind::Headphones => &["headphones"],
            Kind::Mouse if n.contains("magic mouse") => &["magicmouse"],
            Kind::Mouse => &["computermouse"],
            Kind::Keyboard => &["keyboard"],
            Kind::Trackpad => &["rectangle.and.hand.point.up.left"],
            Kind::Other => &[],
        }
    }

    /// The charge that matters: the single battery, or the emptier earbud.
    /// The case doesn't count (it's not in your ears).
    pub fn battery(&self) -> Option<u8> {
        self.main.or(match (self.left, self.right) {
            (Some(a), Some(b)) => Some(a.min(b)),
            (a, b) => a.or(b),
        })
    }
    pub fn split(&self) -> bool {
        self.main.is_none() && (self.left.is_some() || self.right.is_some() || self.case.is_some())
    }
    /// Left, right and case for earbuds (None where a part isn't reported).
    pub fn parts(&self) -> [(&'static str, Option<u8>); 3] {
        [("Left", self.left), ("Right", self.right), ("Case", self.case)]
    }
    /// "L 42 · R 38 · Case 71" for earbuds, the kind otherwise.
    pub fn detail(&self) -> String {
        if !self.split() {
            return self.kind.label().to_string();
        }
        let v = |o: Option<u8>| o.map(|x| x.to_string()).unwrap_or_else(|| "–".into());
        let mut s = format!("L {} · R {}", v(self.left), v(self.right));
        if let Some(c) = self.case {
            s.push_str(&format!(" · Case {c}"));
        }
        s
    }
    /// A short word for the menu bar: "AirPods", "Mouse", "Keyboard".
    pub fn short(&self) -> String {
        match self.kind {
            Kind::Earbuds | Kind::Headphones if self.name.contains("AirPods") => "AirPods".into(),
            Kind::Other => self.name.split_whitespace().last().unwrap_or("Device").to_string(),
            k => k.label().to_string(),
        }
    }
}

/// Green above the low line, amber when low, red when almost empty.
pub fn color(pct: u8) -> Color32 {
    if pct <= m::BT_CRITICAL_BATTERY_MAX_PCT {
        RED
    } else if pct <= m::BT_LOW_BATTERY_MAX_PCT {
        AMBER
    } else {
        GREEN
    }
}

/// Text colour of a percentage: plain unless low.
pub fn text_color(pct: u8) -> Color32 {
    if pct <= m::BT_LOW_BATTERY_MAX_PCT {
        color(pct)
    } else {
        TEXT
    }
}

pub fn is_low(d: &Device) -> bool {
    d.battery().is_some_and(|b| b <= m::BT_LOW_BATTERY_MAX_PCT)
}

/// Devices with a battery first, emptiest first; the rest keep their order.
pub fn lowest_first(devs: &[Device]) -> Vec<Device> {
    let mut v = devs.to_vec();
    v.sort_by_key(|d| d.battery().unwrap_or(u8::MAX));
    v
}

fn norm_addr(a: &str) -> String {
    a.trim().to_lowercase().replace(':', "-")
}

fn pct(v: &Value) -> Option<u8> {
    match v {
        Value::String(s) => s.trim().trim_end_matches('%').trim().parse::<u8>().ok(),
        Value::Number(n) => n.as_u64().map(|x| x.min(100) as u8),
        _ => None,
    }
}

/// iPhone and iPad (macOS calls them "Phone", "Smartphone", "iPad"); not
/// "Headphones".
fn is_mobile(minor: &str, name: &str) -> bool {
    let t = minor.to_lowercase();
    t == "phone" || t.contains("smartphone") || t.contains("ipad") || t.contains("iphone") || name.contains("iPhone") || name.contains("iPad")
}

/// Devices from `system_profiler SPBluetoothDataType -json`. iPhone and
/// iPad are skipped (they aren't accessories).
pub fn parse_profiler(json: &str) -> Vec<Device> {
    let v: Value = serde_json::from_str(json).unwrap_or(Value::Null);
    let mut list = Vec::new();
    let Some(arr) = v["SPBluetoothDataType"][0]["device_connected"].as_array() else { return list };
    for entry in arr {
        let Some(obj) = entry.as_object() else { continue };
        for (name, info) in obj {
            let minor = info["device_minorType"].as_str().unwrap_or("");
            if is_mobile(minor, name) {
                continue;
            }
            let mut d = Device {
                name: name.clone(),
                kind: Kind::guess(minor, name),
                address: norm_addr(info["device_address"].as_str().unwrap_or("")),
                main: pct(&info["device_batteryLevelMain"]).or_else(|| pct(&info["device_batteryLevel"])),
                left: pct(&info["device_batteryLevelLeft"]),
                right: pct(&info["device_batteryLevelRight"]),
                case: pct(&info["device_batteryLevelCase"]),
            };
            // over-ear headphones that only report "Main" stay single
            if d.kind == Kind::Earbuds && d.main.is_some() && (d.left.is_some() || d.right.is_some()) {
                d.main = None;
            }
            list.push(d);
        }
    }
    list
}

/// One input device's battery from the HID registry.
#[derive(Clone, Debug, PartialEq)]
pub struct Hid {
    pub product: String,
    pub address: String,
    pub pct: u8,
}

/// Batteries from `ioreg -r -l -k BatteryPercent` (AppleDeviceManagementHIDEventService):
/// Apple keyboards, mice and trackpads, read without waiting for the
/// slower Bluetooth report.
pub fn parse_ioreg(out: &str) -> Vec<Hid> {
    let mut list = Vec::new();
    let mut cur: (Option<String>, Option<String>, Option<u8>) = (None, None, None);
    let flush = |c: &mut (Option<String>, Option<String>, Option<u8>), list: &mut Vec<Hid>| {
        if let (Some(p), Some(b)) = (c.0.take(), c.2.take()) {
            list.push(Hid { product: p, address: c.1.take().map(|a| norm_addr(&a)).unwrap_or_default(), pct: b.min(100) });
        }
        *c = (None, None, None);
    };
    for line in out.lines() {
        let t = line.trim_start_matches([' ', '|']).trim();
        if t.starts_with("+-o") {
            flush(&mut cur, &mut list);
            continue;
        }
        let Some((k, v)) = t.split_once(" = ") else { continue };
        let k = k.trim().trim_matches('"');
        let v = v.trim();
        match k {
            "Product" => cur.0 = Some(v.trim_matches('"').to_string()),
            "DeviceAddress" | "SerialNumber" if cur.1.is_none() && v.contains(['-', ':']) => cur.1 = Some(v.trim_matches('"').to_string()),
            "BatteryPercent" => cur.2 = v.parse::<u8>().ok(),
            _ => {}
        }
    }
    flush(&mut cur, &mut list);
    list
}

/// Put the fresher HID readings into the list: matched by address, then
/// by product name; unmatched ones are added.
pub fn merge(list: &mut Vec<Device>, hid: &[Hid]) {
    for h in hid {
        let at = list
            .iter()
            .position(|d| !h.address.is_empty() && d.address == h.address)
            .or_else(|| list.iter().position(|d| d.name.eq_ignore_ascii_case(&h.product) || d.name.ends_with(&h.product)));
        match at {
            Some(i) => {
                if !list[i].split() {
                    list[i].main = Some(h.pct);
                }
            }
            None => list.push(Device { name: h.product.clone(), kind: Kind::guess("", &h.product), address: h.address.clone(), main: Some(h.pct), ..Default::default() }),
        }
    }
}

// ------------------------------------------------------------------ alerts

/// Remembers which alerts went out so each device alerts once per step:
/// once when low, once more when almost empty. It forgets a device once it
/// is charged past the reset line, so hovering around 20% doesn't nag.
#[derive(Default)]
pub struct Alerts {
    fired: HashMap<String, u8>, // device name -> 1 low, 2 critical
}

impl Alerts {
    /// The notifications to send now, as (title, body).
    pub fn check(&mut self, devs: &[Device]) -> Vec<(String, String)> {
        let mut out = Vec::new();
        for d in devs {
            let Some(b) = d.battery() else { continue };
            let step = if b <= m::BT_CRITICAL_BATTERY_MAX_PCT { 2 } else if b <= m::BT_LOW_BATTERY_MAX_PCT { 1 } else { 0 };
            let had = self.fired.get(&d.name).copied().unwrap_or(0);
            if b >= m::BT_ALERT_RESET_PCT {
                self.fired.remove(&d.name);
                continue;
            }
            if step > had {
                self.fired.insert(d.name.clone(), step);
                let verb = if d.kind == Kind::Earbuds || d.name.ends_with('s') { "are" } else { "is" };
                let title = format!("{} {verb} at {b}%", d.name);
                let body = if d.split() {
                    let side = match (d.left, d.right) {
                        (Some(l), Some(r)) if l < r => "Left AirPod is almost empty. ",
                        (Some(l), Some(r)) if r < l => "Right AirPod is almost empty. ",
                        _ => "",
                    };
                    if side.is_empty() { "Put them in the case.".to_string() } else { format!("{side}Put them in the case.") }
                } else if step == 2 {
                    "Almost empty. Charge it now.".to_string()
                } else {
                    "Charge it soon.".to_string()
                };
                out.push((title, body));
            }
        }
        out
    }
}

// ------------------------------------------------------------------- prefs

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Prefs {
    /// macOS notification when a device runs low.
    pub alerts: bool,
    /// Show the emptiest device in the menu bar while it is low.
    pub menu_bar: bool,
}

impl Default for Prefs {
    fn default() -> Self {
        Prefs { alerts: true, menu_bar: true }
    }
}

fn prefs_path() -> PathBuf {
    crate::kit::fs::data_path("devices.json")
}

impl Prefs {
    pub fn load() -> Prefs {
        crate::kit::fs::load_json(&prefs_path())
    }
    pub fn save(&self) {
        crate::kit::fs::save_json(&prefs_path(), self, true);
    }
}

/// The menu bar text: the emptiest device while it is low ("Mouse 14%"),
/// nothing otherwise.
pub fn tray_title(devs: &[Device], prefs: Prefs) -> Option<String> {
    if !prefs.menu_bar {
        return None;
    }
    let d = lowest_first(devs).into_iter().next()?;
    let b = d.battery()?;
    (b <= m::BT_LOW_BATTERY_MAX_PCT).then(|| format!("{} {b}%", d.short()))
}

/// Opens System Settings at Bluetooth.
pub fn open_bluetooth_settings() {
    #[cfg(target_os = "macos")]
    {
        let _ = std::process::Command::new("open").arg("x-apple.systempreferences:com.apple.BluetoothSettings").spawn();
    }
}

/// The devices the demo screenshots show.
pub fn demo() -> Vec<Device> {
    vec![
        Device { name: "AirPods Pro".into(), kind: Kind::Earbuds, left: Some(42), right: Some(38), case: Some(71), ..Default::default() },
        Device { name: "Magic Keyboard".into(), kind: Kind::Keyboard, main: Some(82), ..Default::default() },
        Device { name: "MX Master 3S".into(), kind: Kind::Mouse, main: Some(14), ..Default::default() },
        Device { name: "Magic Trackpad".into(), kind: Kind::Trackpad, main: Some(67), ..Default::default() },
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn model_symbols() {
        let d = |n: &str| Device { name: n.into(), kind: Kind::guess("", n), ..Default::default() };
        assert_eq!(d("Roman's AirPods Max").symbols()[0], "airpods.max");
        assert_eq!(d("AirPods Pro").symbols()[0], "airpods.pro");
        assert_eq!(d("AirPods").symbols(), &["airpods"]);
        assert_eq!(d("Magic Mouse").symbols(), &["magicmouse"]);
        assert_eq!(d("MX Master 3S").symbols(), &["computermouse"]);
        assert_eq!(d("Magic Keyboard").symbols(), &["keyboard"]);
    }

    const PROFILER: &str = r#"{"SPBluetoothDataType":[{"controller_properties":{},"device_connected":[
        {"Alex's AirPods Pro":{"device_address":"AA:BB:CC:DD:EE:01","device_batteryLevelCase":"71%","device_batteryLevelLeft":"42%","device_batteryLevelRight":"38%","device_minorType":"Headphones"}},
        {"Magic Keyboard":{"device_address":"AA:BB:CC:DD:EE:02","device_batteryLevelMain":"82%","device_minorType":"Keyboard"}},
        {"MX Master 3S":{"device_address":"AA:BB:CC:DD:EE:03","device_minorType":"Mouse"}},
        {"Alex's iPhone":{"device_address":"AA:BB:CC:DD:EE:04","device_minorType":"Smartphone"}},
        {"AirPods Max":{"device_batteryLevelMain":"37%","device_minorType":"Headphones"}}
    ]}]}"#;

    #[test]
    fn profiler_splits_airpods_and_skips_phones() {
        let v = parse_profiler(PROFILER);
        assert_eq!(v.len(), 4);
        let pods = &v[0];
        assert_eq!(pods.kind, Kind::Earbuds);
        assert_eq!((pods.left, pods.right, pods.case), (Some(42), Some(38), Some(71)));
        assert_eq!(pods.battery(), Some(38));
        assert_eq!(pods.detail(), "L 42 · R 38 · Case 71");
        assert_eq!(pods.address, "aa-bb-cc-dd-ee-01");
        assert_eq!(v[1].kind, Kind::Keyboard);
        assert_eq!(v[1].battery(), Some(82));
        assert_eq!(v[2].kind, Kind::Mouse);
        assert_eq!(v[2].battery(), None);
        assert_eq!(v[3].kind, Kind::Headphones);
        assert_eq!(v[3].battery(), Some(37));
        assert!(!v[3].split());
    }

    #[test]
    fn one_airpod_in() {
        let d = Device { name: "AirPods".into(), kind: Kind::Earbuds, left: Some(51), ..Default::default() };
        assert_eq!(d.battery(), Some(51));
        assert_eq!(d.detail(), "L 51 · R –");
    }

    const IOREG: &str = r#"+-o AppleDeviceManagementHIDEventService  <class AppleDeviceManagementHIDEventService, id 0x100000a1b>
    {
      "Product" = "Magic Keyboard"
      "DeviceAddress" = "aa-bb-cc-dd-ee-02"
      "BatteryPercent" = 79
    }
+-o AppleDeviceManagementHIDEventService  <class AppleDeviceManagementHIDEventService, id 0x100000a1c>
    {
      "Product" = "Magic Trackpad"
      "BatteryPercent" = 55
    }
"#;

    #[test]
    fn ioreg_is_merged_by_address_and_adds_new_devices() {
        let hid = parse_ioreg(IOREG);
        assert_eq!(hid.len(), 2);
        assert_eq!(hid[0], Hid { product: "Magic Keyboard".into(), address: "aa-bb-cc-dd-ee-02".into(), pct: 79 });
        let mut v = parse_profiler(PROFILER);
        merge(&mut v, &hid);
        assert_eq!(v[1].battery(), Some(79));
        let pad = v.last().unwrap();
        assert_eq!((pad.kind, pad.battery()), (Kind::Trackpad, Some(55)));
    }

    #[test]
    fn alerts_fire_once_per_step_and_reset_after_charging() {
        let mut a = Alerts::default();
        let mouse = |b| vec![Device { name: "MX Master 3S".into(), kind: Kind::Mouse, main: Some(b), ..Default::default() }];
        assert!(a.check(&mouse(40)).is_empty());
        let n = a.check(&mouse(19));
        assert_eq!(n.len(), 1);
        assert_eq!(n[0].0, "MX Master 3S is at 19%");
        assert!(a.check(&mouse(18)).is_empty());
        assert!(a.check(&mouse(22)).is_empty(), "no reset below the reset line");
        assert!(a.check(&mouse(19)).is_empty());
        assert_eq!(a.check(&mouse(9)).len(), 1);
        assert!(a.check(&mouse(5)).is_empty());
        assert!(a.check(&mouse(80)).is_empty());
        assert_eq!(a.check(&mouse(15)).len(), 1, "alerts again after a charge");
    }

    #[test]
    fn tray_shows_only_the_low_device() {
        let p = Prefs::default();
        let mut v = demo();
        assert_eq!(tray_title(&v, p).as_deref(), Some("Mouse 14%"));
        assert_eq!(tray_title(&v, Prefs { menu_bar: false, ..p }), None);
        v[2].main = Some(60);
        assert_eq!(tray_title(&v, p), None);
        v[0].right = Some(6);
        assert_eq!(tray_title(&v, p).as_deref(), Some("AirPods 6%"));
    }

    #[test]
    fn lowest_first_puts_unknown_last() {
        let mut v = demo();
        v.push(Device { name: "Speaker".into(), ..Default::default() });
        let names: Vec<_> = lowest_first(&v).into_iter().map(|d| d.name).collect();
        assert_eq!(names, ["MX Master 3S", "AirPods Pro", "Magic Trackpad", "Magic Keyboard", "Speaker"]);
    }
}
