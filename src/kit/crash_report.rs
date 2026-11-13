//! Crash and error reports to Sentry, only in official release builds.
//!
//! The DSN is baked in at compile time from `CMDMEOW_SENTRY_DSN`, which only
//! `.github/workflows/release.yml` sets (from a repository secret). Every other
//! build (forks, `cargo run`, `scripts/dev-install.sh`, PR CI) has no DSN, so
//! the SDK is never started and nothing leaves the Mac. Dev bundles stay off
//! even with a DSN.

/// The DSN this binary was built with, if any.
fn dsn() -> Option<&'static str> {
    option_env!("CMDMEOW_SENTRY_DSN").map(str::trim).filter(|d| !d.is_empty())
}

/// Starts Sentry when this is an official build. Keep the returned guard
/// alive for the whole run: dropping it flushes and stops reporting.
pub fn start() -> Option<sentry::ClientInitGuard> {
    let dsn = dsn()?;
    if super::fs::is_dev() {
        return None;
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
    guard.is_enabled().then_some(guard)
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
    fn plain_builds_have_no_dsn() {
        // CI and local builds never set CMDMEOW_SENTRY_DSN, so Sentry stays off.
        if option_env!("CMDMEOW_SENTRY_DSN").is_none() {
            assert!(super::dsn().is_none());
            assert!(super::start().is_none());
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
