//! macOS notifications. Inside the app bundle they go through the system's
//! notification center; a bare binary without a bundle id falls back to
//! AppleScript's `display notification`. Sending never asks macOS for
//! permission: setup asks, or the switch that turns an alert on ([`flip`]).

use crate::kit::perms::{Perm, Status};

/// Notifications are allowed (an alert switch shows on only then).
pub fn allowed() -> bool {
    Perm::Notifications.status() == Status::Allowed
}

/// What an alert switch shows: the person's choice, if macOS lets it through.
pub fn shown(pref: bool) -> bool {
    pref && allowed()
}

/// Click on an alert switch: off when it shows on; otherwise on, asking
/// macOS (or opening its settings) when notifications aren't allowed yet.
pub fn flip(pref: &mut bool) {
    if shown(*pref) {
        *pref = false;
    } else {
        *pref = true;
        if !allowed() {
            Perm::Notifications.ask();
        }
    }
}

/// Post a notification with a title and one line of text.
pub fn send(title: &str, body: &str) {
    #[cfg(target_os = "macos")]
    mac::send(title, body);
    #[cfg(not(target_os = "macos"))]
    let _ = (title, body);
}

#[cfg(target_os = "macos")]
mod mac {
    use objc2_foundation::{NSBundle, NSString};
    use objc2_user_notifications::{UNMutableNotificationContent, UNNotificationRequest, UNUserNotificationCenter};
    use std::sync::atomic::{AtomicU64, Ordering};

    static SEQ: AtomicU64 = AtomicU64::new(0);

    pub fn send(title: &str, body: &str) {
        // the notification center throws without a bundle id (cargo run)
        let bundled = NSBundle::mainBundle().bundleIdentifier().is_some();
        if !bundled {
            osascript(title, body);
            return;
        }
        let center = UNUserNotificationCenter::currentNotificationCenter();
        let content = UNMutableNotificationContent::new();
        content.setTitle(&NSString::from_str(title));
        content.setBody(&NSString::from_str(body));
        let id = NSString::from_str(&format!("cmdmeow.{}.{}", std::process::id(), SEQ.fetch_add(1, Ordering::Relaxed)));
        let req = UNNotificationRequest::requestWithIdentifier_content_trigger(&id, &content, None);
        center.addNotificationRequest_withCompletionHandler(&req, None);
    }

    fn osascript(title: &str, body: &str) {
        let q = |s: &str| s.replace('\\', "\\\\").replace('"', "\\\"");
        let script = format!("display notification \"{}\" with title \"{}\"", q(body), q(title));
        let _ = std::process::Command::new("osascript").args(["-e", &script]).spawn();
    }
}
