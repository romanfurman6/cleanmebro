//! macOS notifications. Inside the app bundle they go through the system's
//! notification center (macOS asks once for permission); a bare binary
//! without a bundle id falls back to AppleScript's `display notification`.

/// Post a notification with a title and one line of text.
pub fn send(title: &str, body: &str) {
    #[cfg(target_os = "macos")]
    mac::send(title, body);
    #[cfg(not(target_os = "macos"))]
    let _ = (title, body);
}

#[cfg(target_os = "macos")]
mod mac {
    use block2::RcBlock;
    use objc2::runtime::Bool;
    use objc2_foundation::{NSBundle, NSError, NSString};
    use objc2_user_notifications::{UNAuthorizationOptions, UNMutableNotificationContent, UNNotificationRequest, UNUserNotificationCenter};
    use std::sync::Once;
    use std::sync::atomic::{AtomicU64, Ordering};

    static ASKED: Once = Once::new();
    static SEQ: AtomicU64 = AtomicU64::new(0);

    pub fn send(title: &str, body: &str) {
        // the notification center throws without a bundle id (cargo run)
        let bundled = NSBundle::mainBundle().bundleIdentifier().is_some();
        if !bundled {
            osascript(title, body);
            return;
        }
        let center = UNUserNotificationCenter::currentNotificationCenter();
        ASKED.call_once(|| {
            let done = RcBlock::new(|_granted: Bool, _err: *mut NSError| {});
            center.requestAuthorizationWithOptions_completionHandler(UNAuthorizationOptions::Alert | UNAuthorizationOptions::Sound, &done);
        });
        let content = UNMutableNotificationContent::new();
        content.setTitle(&NSString::from_str(title));
        content.setBody(&NSString::from_str(body));
        let id = NSString::from_str(&format!("cleanmebro.{}.{}", std::process::id(), SEQ.fetch_add(1, Ordering::Relaxed)));
        let req = UNNotificationRequest::requestWithIdentifier_content_trigger(&id, &content, None);
        center.addNotificationRequest_withCompletionHandler(&req, None);
    }

    fn osascript(title: &str, body: &str) {
        let q = |s: &str| s.replace('\\', "\\\\").replace('"', "\\\"");
        let script = format!("display notification \"{}\" with title \"{}\"", q(body), q(title));
        let _ = std::process::Command::new("osascript").args(["-e", &script]).spawn();
    }
}
