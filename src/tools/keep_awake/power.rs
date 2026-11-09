//! Power assertions: the macOS way to say "don't sleep". Each one is held
//! until released; macOS drops them by itself if the app quits or crashes.
//! `pmset -g assertions` lists them under "CleanMeBro".

/// What to hold right now.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Want {
    /// Keep the Mac from idle sleep.
    pub system: bool,
    /// Keep the screen on too.
    pub display: bool,
}

/// The assertions this app holds.
#[derive(Default)]
pub struct Power {
    system: Option<u32>,
    display: Option<u32>,
    held: Want,
}

impl Power {
    pub fn held(&self) -> Want {
        self.held
    }

    /// Take or release assertions so that exactly `want` is held.
    pub fn apply(&mut self, want: Want) {
        if want.system != self.system.is_some() {
            self.system = toggle(
                self.system.take(),
                want.system,
                "PreventUserIdleSystemSleep",
                "CleanMeBro Keep Awake",
            );
        }
        let display = want.system && want.display;
        if display != self.display.is_some() {
            self.display = toggle(
                self.display.take(),
                display,
                "PreventUserIdleDisplaySleep",
                "CleanMeBro Keep Awake (screen)",
            );
        }
        self.held = Want {
            system: want.system,
            display,
        };
    }
}

impl Drop for Power {
    fn drop(&mut self) {
        self.apply(Want::default());
    }
}

fn toggle(old: Option<u32>, on: bool, kind: &str, name: &str) -> Option<u32> {
    if let Some(id) = old {
        mac::release(id);
    }
    if on {
        // off macOS (tests) a fake id keeps the bookkeeping honest
        Some(mac::create(kind, name).unwrap_or(0))
    } else {
        None
    }
}

#[cfg(target_os = "macos")]
mod mac {
    use std::ffi::{c_char, c_void, CString};

    type CFStringRef = *const c_void;
    const UTF8: u32 = 0x0800_0100;
    const LEVEL_ON: u32 = 255;

    #[link(name = "CoreFoundation", kind = "framework")]
    unsafe extern "C" {
        fn CFStringCreateWithCString(
            alloc: *const c_void,
            s: *const c_char,
            encoding: u32,
        ) -> CFStringRef;
        fn CFRelease(p: *const c_void);
    }

    #[link(name = "IOKit", kind = "framework")]
    unsafe extern "C" {
        fn IOPMAssertionCreateWithName(
            kind: CFStringRef,
            level: u32,
            name: CFStringRef,
            id: *mut u32,
        ) -> i32;
        fn IOPMAssertionRelease(id: u32) -> i32;
    }

    fn cf(s: &str) -> CFStringRef {
        let c = CString::new(s).unwrap_or_default();
        unsafe { CFStringCreateWithCString(std::ptr::null(), c.as_ptr(), UTF8) }
    }

    pub fn create(kind: &str, name: &str) -> Option<u32> {
        let (k, n) = (cf(kind), cf(name));
        if k.is_null() || n.is_null() {
            return None;
        }
        let mut id = 0u32;
        let rc = unsafe { IOPMAssertionCreateWithName(k, LEVEL_ON, n, &mut id) };
        unsafe {
            CFRelease(k);
            CFRelease(n);
        }
        (rc == 0).then_some(id)
    }

    pub fn release(id: u32) {
        if id != 0 {
            unsafe { IOPMAssertionRelease(id) };
        }
    }
}

#[cfg(not(target_os = "macos"))]
mod mac {
    pub fn create(_kind: &str, _name: &str) -> Option<u32> {
        None
    }
    pub fn release(_id: u32) {}
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn display_needs_system() {
        let mut p = Power::default();
        p.apply(Want {
            system: false,
            display: true,
        });
        assert_eq!(p.held(), Want::default());
        p.apply(Want {
            system: true,
            display: true,
        });
        assert_eq!(
            p.held(),
            Want {
                system: true,
                display: true
            }
        );
        p.apply(Want {
            system: true,
            display: false,
        });
        assert_eq!(
            p.held(),
            Want {
                system: true,
                display: false
            }
        );
        p.apply(Want::default());
        assert_eq!(p.held(), Want::default());
    }
}
