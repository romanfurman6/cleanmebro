//! Opening Cmd+Meow at login, through macOS's own login items
//! (SMAppService, macOS 13+). On by default the first time the app runs
//! from its bundle; the switch is in Settings › General.

/// Whether the app opens at login.
pub fn enabled() -> bool {
    imp::status() == Some(1)
}

/// Turn opening at login on or off.
pub fn set(on: bool) -> Result<(), String> {
    imp::set(on)
}

/// The first run from the .app: turn it on once. Later runs leave the
/// person's choice alone.
pub fn default_on() {
    let marker = crate::kit::fs::data_path("login-default-done");
    if marker.exists() || !in_bundle() {
        return;
    }
    if let Some(dir) = marker.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    let _ = std::fs::write(&marker, b"");
    if let Err(e) = set(true) {
        eprintln!("[login] couldn't add Cmd+Meow to login items: {e}");
    }
}

/// Running from CmdMeow.app (not `cargo run`).
fn in_bundle() -> bool {
    std::env::current_exe().ok().is_some_and(|p| p.to_string_lossy().contains(".app/Contents/MacOS/"))
}

#[cfg(target_os = "macos")]
mod imp {
    use objc2::rc::Retained;
    use objc2::runtime::{AnyClass, AnyObject};
    use objc2::msg_send;
    use objc2_foundation::NSError;

    #[link(name = "ServiceManagement", kind = "framework")]
    extern "C" {}

    fn service() -> Option<Retained<AnyObject>> {
        let cls = AnyClass::get(c"SMAppService")?;
        unsafe { msg_send![cls, mainAppService] }
    }

    /// 0 not registered, 1 enabled, 2 needs approval in System Settings,
    /// 3 not found.
    pub fn status() -> Option<isize> {
        let s = service()?;
        Some(unsafe { msg_send![&*s, status] })
    }

    pub fn set(on: bool) -> Result<(), String> {
        let s = service().ok_or("Needs macOS 13 or later")?;
        let r: Result<(), Retained<NSError>> = unsafe {
            if on {
                msg_send![&*s, registerAndReturnError: _]
            } else {
                msg_send![&*s, unregisterAndReturnError: _]
            }
        };
        r.map_err(|e| e.localizedDescription().to_string())
    }
}

#[cfg(not(target_os = "macos"))]
mod imp {
    pub fn status() -> Option<isize> {
        None
    }
    pub fn set(_on: bool) -> Result<(), String> {
        Err("Only on macOS".into())
    }
}
