//! Crash and error reports to Sentry: only in official release builds, and
//! only after the person said yes (setup step or Settings › General).
//!
//! The DSN is baked in at compile time from `CMDMEOW_SENTRY_DSN`, which only
//! `.github/workflows/release.yml` sets (from a repository secret). Every other
//! build (forks, `cargo run`, `scripts/dev-install.sh`, PR CI) has no DSN, so
//! the SDK is never started, the question is never asked and nothing leaves
//! the Mac. Dev bundles stay off even with a DSN.

use super::sync::LockExt;
use std::sync::Mutex;

/// The running client; dropping it flushes and stops reporting.
static CLIENT: Mutex<Option<sentry::ClientInitGuard>> = Mutex::new(None);

/// The DSN this binary was built with, if any.
fn dsn() -> Option<&'static str> {
    option_env!("CMDMEOW_SENTRY_DSN").map(str::trim).filter(|d| !d.is_empty())
}

/// This is an official build that can send crash reports (so ask about it).
pub fn available() -> bool {
    dsn().is_some() && !super::fs::is_dev()
}

fn consent_file() -> std::path::PathBuf {
    super::fs::data_path("crash-reports")
}

/// What the person chose: Some(true) send, Some(false) don't, None not asked yet.
pub fn consent() -> Option<bool> {
    match std::fs::read_to_string(consent_file()).ok()?.trim() {
        "on" => Some(true),
        "off" => Some(false),
        _ => None,
    }
}

/// Saves the choice and starts or stops reporting right away.
pub fn set_consent(on: bool) {
    let f = consent_file();
    if let Some(d) = f.parent() {
        let _ = std::fs::create_dir_all(d);
    }
    let _ = std::fs::write(f, if on { "on" } else { "off" });
    if on {
        start();
    } else {
        // drop outside the lock: closing flushes, which can take a moment
        let old = CLIENT.lock_ok().take();
        drop(old);
    }
}

/// Starts Sentry when this is an official build and the person agreed.
/// Hold the returned value in `main`: when it drops (normal exit or a panic
/// unwinding out of main) queued reports are sent first.
pub fn start_if_allowed() -> Flush {
    if consent() == Some(true) {
        start();
    }
    Flush
}

/// Whether reports are being sent right now.
pub fn running() -> bool {
    CLIENT.lock_ok().is_some()
}

fn start() {
    let Some(dsn) = dsn() else { return };
    if super::fs::is_dev() {
        return;
    }
    let mut slot = CLIENT.lock_ok();
    if slot.is_some() {
        return;
    }
    let release = match option_env!("CMDMEOW_BUILD") {
        Some(b) if !b.is_empty() => format!("cmdmeow@{}+{b}", env!("CARGO_PKG_VERSION")),
        _ => format!("cmdmeow@{}", env!("CARGO_PKG_VERSION")),
    };
    // crashes and errors only (no tracing, sessions or logs: those features
    // are off in Cargo.toml), and no IP address or other personal data
    let options = sentry::ClientOptions::new()
        .dsn(dsn)
        .release(release)
        .environment("production")
        .send_default_pii(false)
        // not the Mac's hostname, which often holds the owner's name
        .server_name("mac")
        .before_send(|mut event| {
            scrub(&mut event);
            Some(event)
        });
    let guard = sentry::init(options);
    if guard.is_enabled() {
        *slot = Some(guard);
    }
}

/// Sends queued reports when dropped.
pub struct Flush;

impl Drop for Flush {
    fn drop(&mut self) {
        if let Some(c) = sentry::Hub::current().client() {
            c.flush(Some(std::time::Duration::from_secs(2)));
        }
    }
}

/// Swaps the home folder for `~` in error texts, so a path in a panic
/// message doesn't carry the user name.
fn scrub(event: &mut sentry::protocol::Event<'static>) {
    let Some(home) = dirs::home_dir().map(|h| h.to_string_lossy().into_owned()).filter(|h| h.len() > 1) else { return };
    let fix = |s: &mut String| {
        if s.contains(&home) {
            *s = s.replace(&home, "~");
        }
    };
    if let Some(m) = event.message.as_mut() {
        fix(m);
    }
    for e in event.exception.values.iter_mut() {
        if let Some(v) = e.value.as_mut() {
            fix(v);
        }
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn plain_builds_never_report() {
        // CI and local builds never set CMDMEOW_SENTRY_DSN: nothing to ask, nothing starts
        if option_env!("CMDMEOW_SENTRY_DSN").is_none() {
            assert!(!super::available());
            super::start();
            assert!(!super::running());
        }
    }

    #[test]
    fn home_folder_is_scrubbed() {
        let Some(home) = dirs::home_dir().map(|h| h.to_string_lossy().into_owned()).filter(|h| h.len() > 1) else { return };
        let mut ev = sentry::protocol::Event { message: Some(format!("no file {home}/Desktop/a.png")), ..Default::default() };
        ev.exception.values.push(sentry::protocol::Exception { value: Some(format!("open {home}/x")), ..Default::default() });
        super::scrub(&mut ev);
        assert_eq!(ev.message.as_deref(), Some("no file ~/Desktop/a.png"));
        assert_eq!(ev.exception.values[0].value.as_deref(), Some("open ~/x"));
    }
}
