//! Reads the hardware: fan speeds from the SMC (AppleSMC user client) and
//! temperatures from the HID sensor hub, with SMC temperature keys filling
//! the parts the hub doesn't name. Reading needs no admin rights. Elsewhere
//! than macOS there is nothing to read.

use super::data::Fan;
#[cfg(target_os = "macos")]
use super::data::Sensor;

/// One reading of the hardware.
#[derive(Default)]
pub struct Reading {
    pub has_smc: bool,
    pub fans: Vec<Fan>,
    pub hid: Vec<super::data::Sensor>,
    pub smc_temps: Vec<super::data::Sensor>,
}

/// An open connection to the sensors, kept between readings.
pub struct Probe {
    #[cfg(target_os = "macos")]
    smc: Option<mac::Smc>,
    #[cfg(target_os = "macos")]
    hid: Option<mac::Hid>,
    /// SMC temperature keys found once at start.
    #[cfg(target_os = "macos")]
    temp_keys: Vec<(String, mac::KeyInfo)>,
}

impl Probe {
    #[cfg(target_os = "macos")]
    pub fn open() -> Self {
        let smc = mac::Smc::open();
        let temp_keys = smc.as_ref().map(|s| s.temperature_keys()).unwrap_or_default();
        Probe { smc, hid: mac::Hid::open(), temp_keys }
    }
    #[cfg(not(target_os = "macos"))]
    pub fn open() -> Self {
        Probe {}
    }

    #[cfg(target_os = "macos")]
    pub fn read(&mut self) -> Reading {
        let mut r = Reading::default();
        if let Some(smc) = &self.smc {
            r.has_smc = true;
            r.fans = smc.fans();
            for (k, info) in &self.temp_keys {
                if let (Some(g), Some(c)) = (super::data::group_of_smc(k), smc.read_with(k, *info)) {
                    r.smc_temps.push(Sensor { name: k.clone(), group: g, celsius: c });
                }
            }
        }
        if let Some(hid) = &self.hid {
            r.hid = hid.temperatures();
        }
        r
    }
    #[cfg(not(target_os = "macos"))]
    pub fn read(&mut self) -> Reading {
        Reading::default()
    }
}

/// Print every fan and sensor this Mac reports (`--fan-report`), to check
/// the readings on a new chip.
pub fn report() {
    let mut p = Probe::open();
    let r = p.read();
    println!("SMC: {}", if r.has_smc { "open" } else { "not available" });
    println!("Fans: {}", r.fans.len());
    for (i, f) in r.fans.iter().enumerate() {
        println!("  fan {i}: {:.0} rpm (min {:.0}, max {:.0})", f.rpm, f.min, f.max);
    }
    #[cfg(target_os = "macos")]
    if let Some(smc) = mac::Smc::open() {
        println!("Control keys:");
        let n = r.fans.len().max(1);
        let mut keys = vec!["Ftst".to_string(), "CHTE".into(), "CH0B".into(), "CH0C".into()];
        for i in 0..n {
            for k in ["Md", "md", "Tg", "Dc"] {
                keys.push(format!("F{i}{k}"));
            }
        }
        for k in keys {
            match smc.info_of(&k) {
                Some((size, kind)) => println!("  {k:<5} {kind} ({size} B) = {:?} {:02x?}", smc.read(&k), smc.read_bytes(&k).unwrap_or_default()),
                None => println!("  {k:<5} -"),
            }
        }
    }
    println!("HID temperature sensors: {}", r.hid.len());
    for s in &r.hid {
        println!("  {:<32} {:>6.1} °C  {:?}", s.name, s.celsius, s.group);
    }
    println!("SMC temperature keys: {}", r.smc_temps.len());
    for s in &r.smc_temps {
        println!("  {:<6} {:>6.1} °C  {:?}", s.name, s.celsius, s.group);
    }
}

/// The bytes that switch charging off (`inhibit`) or back on, per key.
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
fn charge_bytes(key: &str, inhibit: bool) -> Option<Vec<u8>> {
    match key {
        "CHTE" => Some(if inhibit { vec![1, 0, 0, 0] } else { vec![0; 4] }),
        "CH0B" | "CH0C" => Some(vec![if inhibit { 2 } else { 0 }]),
        _ => None,
    }
}

/// Which charging keys to use, given each key's size on this Mac (`None`
/// = missing). `CHTE` wins: where it exists the older keys do nothing.
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
fn charge_keys(size_of: impl Fn(&str) -> Option<usize>) -> Option<Vec<&'static str>> {
    let fits = |k: &str| size_of(k).is_some_and(|n| charge_bytes(k, false).is_some_and(|b| b.len() == n));
    if fits("CHTE") {
        Some(vec!["CHTE"])
    } else {
        let keys: Vec<&'static str> = ["CH0B", "CH0C"].into_iter().filter(|k| fits(k)).collect();
        (!keys.is_empty()).then_some(keys)
    }
}

#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
fn fourcc(s: &str) -> u32 {
    s.bytes().take(4).fold(0u32, |a, b| (a << 8) | b as u32)
}

#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
fn fourcc_str(v: u32) -> String {
    v.to_be_bytes().iter().map(|b| *b as char).collect()
}

/// Encode `v` as SMC bytes of a numeric type, `size` bytes long.
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
fn encode(kind: u32, v: f32, size: usize) -> Option<Vec<u8>> {
    let b = match fourcc_str(kind).as_str() {
        "flt " => v.to_le_bytes().to_vec(),
        "fpe2" => ((v.max(0.0) * 4.0) as u16).to_be_bytes().to_vec(),
        "ui8 " => vec![v.clamp(0.0, 255.0) as u8],
        "ui16" => (v.clamp(0.0, 65535.0) as u16).to_be_bytes().to_vec(),
        "ui32" => (v.max(0.0) as u32).to_be_bytes().to_vec(),
        _ => return None,
    };
    (b.len() == size).then_some(b)
}

/// Decode SMC bytes of a numeric type.
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
fn decode(kind: u32, b: &[u8]) -> Option<f32> {
    let t = fourcc_str(kind);
    Some(match t.as_str() {
        "flt " if b.len() >= 4 => f32::from_le_bytes([b[0], b[1], b[2], b[3]]),
        "fpe2" if b.len() >= 2 => u16::from_be_bytes([b[0], b[1]]) as f32 / 4.0,
        "sp78" if b.len() >= 2 => i16::from_be_bytes([b[0], b[1]]) as f32 / 256.0,
        "ui8 " if !b.is_empty() => b[0] as f32,
        "ui16" if b.len() >= 2 => u16::from_be_bytes([b[0], b[1]]) as f32,
        "ui32" if b.len() >= 4 => u32::from_be_bytes([b[0], b[1], b[2], b[3]]) as f32,
        _ => return None,
    })
}

#[cfg(target_os = "macos")]
pub(super) mod mac {
    use super::super::data::{group_of_hid, plausible, Fan, Sensor};
    use super::{decode, encode, fourcc, fourcc_str};
    use std::ffi::{c_char, c_void, CStr, CString};

    // ------------------------------------------------------------ IOKit / CF

    #[repr(C)]
    pub struct CFDictionaryCallBacks {
        _private: [u8; 0],
    }

    #[link(name = "IOKit", kind = "framework")]
    extern "C" {
        fn IOServiceMatching(name: *const c_char) -> *mut c_void;
        fn IOServiceGetMatchingService(main_port: u32, matching: *mut c_void) -> u32;
        fn IOServiceOpen(service: u32, owning_task: u32, kind: u32, connect: *mut u32) -> i32;
        fn IOServiceClose(connect: u32) -> i32;
        fn IOObjectRelease(obj: u32) -> i32;
        fn IOConnectCallStructMethod(conn: u32, selector: u32, input: *const c_void, in_size: usize, output: *mut c_void, out_size: *mut usize) -> i32;

        fn IOHIDEventSystemClientCreate(alloc: *const c_void) -> *mut c_void;
        fn IOHIDEventSystemClientSetMatching(client: *mut c_void, matching: *const c_void) -> i32;
        fn IOHIDEventSystemClientCopyServices(client: *mut c_void) -> *const c_void;
        fn IOHIDServiceClientCopyProperty(service: *const c_void, key: *const c_void) -> *const c_void;
        fn IOHIDServiceClientCopyEvent(service: *const c_void, kind: i64, options: i32, timestamp: i64) -> *const c_void;
        fn IOHIDEventGetFloatValue(event: *const c_void, field: i32) -> f64;
    }

    #[link(name = "CoreFoundation", kind = "framework")]
    extern "C" {
        static kCFTypeDictionaryKeyCallBacks: CFDictionaryCallBacks;
        static kCFTypeDictionaryValueCallBacks: CFDictionaryCallBacks;
        fn CFDictionaryCreate(alloc: *const c_void, keys: *const *const c_void, values: *const *const c_void, n: isize, kcb: *const CFDictionaryCallBacks, vcb: *const CFDictionaryCallBacks) -> *const c_void;
        fn CFNumberCreate(alloc: *const c_void, kind: isize, value: *const c_void) -> *const c_void;
        fn CFStringCreateWithCString(alloc: *const c_void, s: *const c_char, encoding: u32) -> *const c_void;
        fn CFStringGetCString(s: *const c_void, buf: *mut c_char, len: isize, encoding: u32) -> u8;
        fn CFGetTypeID(cf: *const c_void) -> usize;
        fn CFStringGetTypeID() -> usize;
        fn CFArrayGetCount(a: *const c_void) -> isize;
        fn CFArrayGetValueAtIndex(a: *const c_void, i: isize) -> *const c_void;
        fn CFRelease(cf: *const c_void);
    }

    const UTF8: u32 = 0x0800_0100;
    const CF_NUMBER_SINT32: isize = 3;

    fn cfstr(s: &str) -> *const c_void {
        let c = CString::new(s).unwrap();
        unsafe { CFStringCreateWithCString(std::ptr::null(), c.as_ptr(), UTF8) }
    }

    fn cfnum(v: i32) -> *const c_void {
        unsafe { CFNumberCreate(std::ptr::null(), CF_NUMBER_SINT32, &v as *const i32 as *const c_void) }
    }

    fn string_of(cf: *const c_void) -> Option<String> {
        if cf.is_null() || unsafe { CFGetTypeID(cf) != CFStringGetTypeID() } {
            return None;
        }
        let mut buf = [0 as c_char; 256];
        if unsafe { CFStringGetCString(cf, buf.as_mut_ptr(), buf.len() as isize, UTF8) } == 0 {
            return None;
        }
        Some(unsafe { CStr::from_ptr(buf.as_ptr()) }.to_string_lossy().into_owned())
    }

    // ------------------------------------------------------------ SMC

    #[repr(C)]
    #[derive(Clone, Copy, Default)]
    struct Vers {
        major: u8,
        minor: u8,
        build: u8,
        reserved: u8,
        release: u16,
    }

    #[repr(C)]
    #[derive(Clone, Copy, Default)]
    struct PLimit {
        version: u16,
        length: u16,
        cpu: u32,
        gpu: u32,
        mem: u32,
    }

    #[repr(C)]
    #[derive(Clone, Copy, Default, Debug)]
    pub struct KeyInfo {
        size: u32,
        kind: u32,
        attributes: u8,
    }

    /// `SMCKeyData_t` from Apple's SMC user client.
    #[repr(C)]
    #[derive(Clone, Copy, Default)]
    struct KeyData {
        key: u32,
        vers: Vers,
        plimit: PLimit,
        info: KeyInfo,
        result: u8,
        status: u8,
        data8: u8,
        data32: u32,
        bytes: [u8; 32],
    }

    const _: () = assert!(std::mem::size_of::<KeyData>() == 80);

    const KERNEL_INDEX_SMC: u32 = 2;
    const CMD_READ_BYTES: u8 = 5;
    const CMD_WRITE_BYTES: u8 = 6;
    const CMD_READ_INDEX: u8 = 8;
    const CMD_READ_KEYINFO: u8 = 9;

    pub struct Smc {
        conn: u32,
    }

    impl Smc {
        pub fn open() -> Option<Smc> {
            unsafe {
                let name = CString::new("AppleSMC").unwrap();
                let matching = IOServiceMatching(name.as_ptr());
                if matching.is_null() {
                    return None;
                }
                let service = IOServiceGetMatchingService(0, matching);
                if service == 0 {
                    return None;
                }
                let mut conn = 0u32;
                #[allow(deprecated)]
                let task = libc::mach_task_self();
                let kr = IOServiceOpen(service, task, 0, &mut conn);
                IOObjectRelease(service);
                (kr == 0 && conn != 0).then_some(Smc { conn })
            }
        }

        fn call(&self, input: &KeyData) -> Option<KeyData> {
            let mut out = KeyData::default();
            let mut size = std::mem::size_of::<KeyData>();
            let kr = unsafe { IOConnectCallStructMethod(self.conn, KERNEL_INDEX_SMC, input as *const KeyData as *const c_void, std::mem::size_of::<KeyData>(), &mut out as *mut KeyData as *mut c_void, &mut size) };
            (kr == 0 && out.result == 0).then_some(out)
        }

        fn info(&self, key: &str) -> Option<KeyInfo> {
            let input = KeyData { key: fourcc(key), data8: CMD_READ_KEYINFO, ..Default::default() };
            self.call(&input).map(|o| o.info).filter(|i| i.size > 0 && i.size <= 32)
        }

        /// (size, type) of a key, if this SMC has it.
        pub fn info_of(&self, key: &str) -> Option<(u32, String)> {
            self.info(key).map(|i| (i.size, fourcc_str(i.kind)))
        }

        /// Write a number to a key in the key's own type. Needs root.
        pub fn write(&self, key: &str, v: f32) -> bool {
            let Some(info) = self.info(key) else { return false };
            let Some(b) = encode(info.kind, v, info.size as usize) else { return false };
            self.write_raw(key, info, &b)
        }

        /// Write raw bytes to a key; they must be exactly the key's size.
        /// Needs root.
        pub fn write_bytes(&self, key: &str, b: &[u8]) -> bool {
            self.info(key).is_some_and(|info| self.write_raw(key, info, b))
        }

        fn write_raw(&self, key: &str, info: KeyInfo, b: &[u8]) -> bool {
            if b.len() != info.size as usize {
                return false;
            }
            let mut input = KeyData { key: fourcc(key), info, data8: CMD_WRITE_BYTES, ..Default::default() };
            input.bytes[..b.len()].copy_from_slice(b);
            self.call(&input).is_some()
        }

        /// A key's raw bytes.
        pub fn read_bytes(&self, key: &str) -> Option<Vec<u8>> {
            let info = self.info(key)?;
            let out = self.call(&KeyData { key: fourcc(key), info, data8: CMD_READ_BYTES, ..Default::default() })?;
            Some(out.bytes[..info.size as usize].to_vec())
        }

        pub fn read_with(&self, key: &str, info: KeyInfo) -> Option<f32> {
            let input = KeyData { key: fourcc(key), info, data8: CMD_READ_BYTES, ..Default::default() };
            let out = self.call(&input)?;
            decode(info.kind, &out.bytes[..info.size as usize])
        }

        pub fn read(&self, key: &str) -> Option<f32> {
            self.read_with(key, self.info(key)?)
        }

        pub fn fans(&self) -> Vec<Fan> {
            let n = self.read("FNum").unwrap_or(0.0) as usize;
            (0..n.min(4))
                .filter_map(|i| {
                    let rpm = self.read(&format!("F{i}Ac"))?;
                    let min = self.read(&format!("F{i}Mn")).unwrap_or(0.0);
                    let max = self.read(&format!("F{i}Mx")).unwrap_or(0.0);
                    Some(Fan { rpm: rpm.max(0.0), min, max: max.max(min) })
                })
                .collect()
        }

        /// Every `T…` key with a float or fixed-point type that reads as a
        /// believable temperature now.
        pub fn temperature_keys(&self) -> Vec<(String, KeyInfo)> {
            let count = self.read("#KEY").unwrap_or(0.0) as u32;
            let mut out = Vec::new();
            for i in 0..count.min(4000) {
                let input = KeyData { data8: CMD_READ_INDEX, data32: i, ..Default::default() };
                let Some(k) = self.call(&input) else { continue };
                let name = fourcc_str(k.key);
                if super::super::data::group_of_smc(&name).is_none() {
                    continue;
                }
                let Some(info) = self.info(&name) else { continue };
                if !matches!(fourcc_str(info.kind).as_str(), "flt " | "sp78") {
                    continue;
                }
                if self.read_with(&name, info).is_some_and(plausible) {
                    out.push((name, info));
                }
            }
            out
        }
    }

    /// Fan speed control through the SMC. Writing needs root, so only the
    /// helper uses this. On Apple silicon `Ftst` = 1 first takes the fans
    /// away from thermalmonitord; the mode key is `F{i}Md` or, on newer
    /// chips, `F{i}md`.
    pub struct Control {
        smc: Smc,
        test_key: bool,
        modes: Vec<String>,
        unlocked: bool,
        pub ranges: Vec<(f32, f32)>,
    }

    impl Control {
        pub fn open() -> Option<Control> {
            let smc = Smc::open()?;
            let fans = smc.fans();
            let modes = (0..fans.len()).map(|i| if smc.info(&format!("F{i}md")).is_some() { format!("F{i}md") } else { format!("F{i}Md") }).collect();
            let test_key = smc.info("Ftst").is_some();
            let ranges = fans.iter().map(|f| (f.min, f.max)).collect();
            Some(Control { smc, test_key, modes, unlocked: false, ranges })
        }

        /// Hold fan `i` at `rpm`. True once the SMC reports the fan as manual.
        pub fn manual(&mut self, i: usize, rpm: f32) -> bool {
            let Some(mode) = self.modes.get(i) else { return false };
            if self.test_key && !self.unlocked {
                self.unlocked = self.smc.write("Ftst", 1.0);
            }
            if self.smc.read(mode) != Some(1.0) {
                self.smc.write(mode, 1.0);
            }
            let (lo, hi) = self.ranges[i];
            self.smc.write(&format!("F{i}Tg"), rpm.clamp(lo, hi.max(lo)));
            self.smc.read(mode) == Some(1.0)
        }

        /// Give fan `i` back to macOS.
        pub fn release(&mut self, i: usize) {
            if let Some(mode) = self.modes.get(i) {
                self.smc.write(mode, 0.0);
            }
        }

        /// Give every fan back to macOS.
        pub fn auto(&mut self) {
            for i in 0..self.modes.len() {
                self.release(i);
            }
            if self.test_key {
                self.smc.write("Ftst", 0.0);
            }
            self.unlocked = false;
        }
    }

    /// Turns battery charging off and on through the SMC (root only). Newer
    /// firmware has one key, `CHTE`; older Apple silicon firmware two,
    /// `CH0B` and `CH0C`.
    pub struct Charger {
        smc: Smc,
        keys: Vec<&'static str>,
    }

    impl Charger {
        pub fn open() -> Option<Charger> {
            let smc = Smc::open()?;
            let keys = super::charge_keys(|k| smc.info(k).map(|i| i.size as usize))?;
            Some(Charger { smc, keys })
        }

        /// Switch charging off (`inhibit`) or back on. True if every key took it.
        pub fn set(&self, inhibit: bool) -> bool {
            let mut ok = true;
            for k in &self.keys {
                ok &= super::charge_bytes(k, inhibit).is_some_and(|b| self.smc.write_bytes(k, &b));
            }
            ok
        }

        /// Charging is switched off now.
        pub fn inhibited(&self) -> Option<bool> {
            let k = self.keys.first()?;
            let b = self.smc.read_bytes(k)?;
            Some(Some(b) != super::charge_bytes(k, false))
        }
    }

    impl Drop for Smc {
        fn drop(&mut self) {
            unsafe {
                IOServiceClose(self.conn);
            }
        }
    }

    // ------------------------------------------------------------ HID sensors

    const HID_TEMPERATURE: i64 = 15;

    pub struct Hid {
        client: *mut c_void,
        product: *const c_void,
    }

    impl Hid {
        pub fn open() -> Option<Hid> {
            unsafe {
                let client = IOHIDEventSystemClientCreate(std::ptr::null());
                if client.is_null() {
                    return None;
                }
                // Apple's vendor page 0xff00, usage 5 = temperature sensors.
                let keys = [cfstr("PrimaryUsagePage"), cfstr("PrimaryUsage")];
                let vals = [cfnum(0xff00), cfnum(5)];
                let dict = CFDictionaryCreate(std::ptr::null(), keys.as_ptr(), vals.as_ptr(), 2, &kCFTypeDictionaryKeyCallBacks, &kCFTypeDictionaryValueCallBacks);
                IOHIDEventSystemClientSetMatching(client, dict);
                for c in keys.iter().chain(vals.iter()) {
                    CFRelease(*c);
                }
                CFRelease(dict);
                Some(Hid { client, product: cfstr("Product") })
            }
        }

        pub fn temperatures(&self) -> Vec<Sensor> {
            let mut out = Vec::new();
            unsafe {
                let services = IOHIDEventSystemClientCopyServices(self.client);
                if services.is_null() {
                    return out;
                }
                for i in 0..CFArrayGetCount(services) {
                    let sc = CFArrayGetValueAtIndex(services, i);
                    let name_cf = IOHIDServiceClientCopyProperty(sc, self.product);
                    let name = string_of(name_cf);
                    if !name_cf.is_null() {
                        CFRelease(name_cf);
                    }
                    let Some(name) = name else { continue };
                    let Some(group) = group_of_hid(&name) else { continue };
                    let ev = IOHIDServiceClientCopyEvent(sc, HID_TEMPERATURE, 0, 0);
                    if ev.is_null() {
                        continue;
                    }
                    let c = IOHIDEventGetFloatValue(ev, (HID_TEMPERATURE << 16) as i32) as f32;
                    CFRelease(ev);
                    if plausible(c) {
                        out.push(Sensor { name, group, celsius: c });
                    }
                }
                CFRelease(services);
            }
            out.sort_by(|a, b| a.name.cmp(&b.name));
            out
        }
    }

    impl Drop for Hid {
        fn drop(&mut self) {
            unsafe {
                CFRelease(self.product);
                CFRelease(self.client);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{charge_bytes, charge_keys, decode, encode, fourcc};

    #[test]
    fn picks_charging_keys() {
        let mac = |have: &'static [(&'static str, usize)]| move |k: &str| have.iter().find(|h| h.0 == k).map(|h| h.1);
        assert_eq!(charge_keys(mac(&[("CHTE", 4), ("CH0B", 1), ("CH0C", 1)])), Some(vec!["CHTE"]));
        assert_eq!(charge_keys(mac(&[("CH0B", 1), ("CH0C", 1)])), Some(vec!["CH0B", "CH0C"]));
        assert_eq!(charge_keys(mac(&[("CH0B", 1)])), Some(vec!["CH0B"]));
        // a key of an unexpected size is never written
        assert_eq!(charge_keys(mac(&[("CHTE", 2), ("CH0B", 1)])), Some(vec!["CH0B"]));
        assert_eq!(charge_keys(mac(&[("CHTE", 2)])), None);
        assert_eq!(charge_keys(mac(&[])), None);
        assert_eq!(charge_bytes("CHTE", true), Some(vec![1, 0, 0, 0]));
        assert_eq!(charge_bytes("CH0C", false), Some(vec![0]));
        assert_eq!(charge_bytes("F0Md", true), None);
    }

    #[test]
    fn encodes_smc_types() {
        assert_eq!(encode(fourcc("flt "), 2140.0, 4), Some(2140.0f32.to_le_bytes().to_vec()));
        assert_eq!(encode(fourcc("fpe2"), 2140.0, 2), Some(vec![0x21, 0x70]));
        assert_eq!(encode(fourcc("ui8 "), 1.0, 1), Some(vec![1]));
        assert_eq!(encode(fourcc("ui8 "), 1.0, 2), None);
        assert_eq!(encode(fourcc("ch8*"), 1.0, 1), None);
        for (k, v, n) in [("flt ", 3196.5, 4), ("fpe2", 1350.0, 2), ("ui16", 7.0, 2)] {
            assert_eq!(decode(fourcc(k), &encode(fourcc(k), v, n).unwrap()), Some(v));
        }
    }

    #[test]
    fn decodes_smc_types() {
        assert_eq!(decode(fourcc("flt "), &2140.0f32.to_le_bytes()), Some(2140.0));
        assert_eq!(decode(fourcc("fpe2"), &[0x21, 0x70]), Some(2140.0));
        assert_eq!(decode(fourcc("sp78"), &[0x40, 0x80]), Some(64.5));
        assert_eq!(decode(fourcc("ui8 "), &[2]), Some(2.0));
        assert_eq!(decode(fourcc("ch8*"), &[1]), None);
    }
}
