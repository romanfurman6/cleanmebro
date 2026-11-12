//! The helper itself: run by launchd as root (`--fan-helper <uid>`). The
//! protocol, install and the client live in [`crate::kit::helper`].
//!
//! Safety lives in the helper, so it holds even if the app dies:
//! - a `set` is a lease: without a new one within [`LEASE_SECS`] (app quit,
//!   crashed or the Mac slept) the fans go back to Auto;
//! - any sensor at 95 °C or more gives the fans back to Auto;
//! - on start and on SIGTERM the fans go back to Auto;
//! - every second the speeds are written again, so a reset by macOS (after
//!   sleep, on M3 and later) doesn't stick.
//!
//! It also holds the battery's charge limit ([`crate::kit::charge`]): the
//! policy is kept in [`CHARGE_FILE`] so it holds while the app is closed and
//! after a restart, and charging is switched back on when the helper stops.

pub use crate::kit::helper::*;
#[cfg(target_os = "macos")]
use crate::kit::charge::{self, Policy, Status};
#[cfg(target_os = "macos")]
use std::io::{BufRead, BufReader, Write};
#[cfg(target_os = "macos")]
use std::time::Duration;

/// How often the helper looks at the battery while it holds a limit.
#[cfg(target_os = "macos")]
const CHARGE_EVERY: Duration = Duration::from_secs(20);

/// How long the idle helper waits for a call before it checks for a stop
/// signal again (a signal also wakes it early).
#[cfg(target_os = "macos")]
const IDLE_WAIT_MS: i32 = 10_000;

/// The helper's main loop (`--fan-helper <uid>`), run as root by launchd.
#[cfg(target_os = "macos")]
pub fn serve(uid: u32) -> ! {
    use super::data::{merge, Snapshot};
    use super::probe::{
        mac::{Charger, Control},
        Probe,
    };
    use std::os::unix::io::AsRawFd;
    use std::os::unix::net::UnixListener;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::time::{Instant, SystemTime};

    static STOP: AtomicBool = AtomicBool::new(false);
    extern "C" fn on_term(_: libc::c_int) {
        STOP.store(true, Ordering::SeqCst);
    }
    unsafe {
        libc::signal(libc::SIGTERM, on_term as *const () as libc::sighandler_t);
        libc::signal(libc::SIGINT, on_term as *const () as libc::sighandler_t);
    }

    let Some(mut ctl) = Control::open() else {
        eprintln!("fan helper: SMC not available");
        std::thread::sleep(Duration::from_secs(60));
        std::process::exit(1);
    };
    ctl.auto();
    let mut probe = Probe::open();
    let charger = Charger::open();
    let mut policy: Option<Policy> = std::fs::read_to_string(CHARGE_FILE).ok().and_then(|t| serde_json::from_str::<Policy>(&t).ok()).map(Policy::sanitize);
    let mut cstate = charge::State::default();
    // charging was switched off by this helper and must come back on
    let mut touched = false;
    let mut charge_at = Instant::now() - CHARGE_EVERY;

    let _ = std::fs::remove_file(SOCKET);
    let listener = match UnixListener::bind(SOCKET) {
        Ok(l) => l,
        Err(e) => {
            eprintln!("fan helper: can't listen on {SOCKET}: {e}");
            std::thread::sleep(Duration::from_secs(10));
            std::process::exit(1);
        }
    };
    if let Ok(c) = std::ffi::CString::new(SOCKET) {
        unsafe {
            libc::chown(c.as_ptr(), uid, 0);
            libc::chmod(c.as_ptr(), 0o600);
        }
    }
    let _ = listener.set_nonblocking(true);

    let mut targets: Vec<Option<f32>> = Vec::new();
    let mut lease = SystemTime::UNIX_EPOCH;
    let mut manual = false;
    let mut hot = false;
    let mut last_tick = Instant::now() - Duration::from_secs(5);

    loop {
        if STOP.load(Ordering::SeqCst) {
            ctl.auto();
            if let (true, Some(c)) = (touched, &charger) {
                c.set(false);
            }
            let _ = std::fs::remove_file(SOCKET);
            std::process::exit(0);
        }
        // Idle (no lease, fans on auto) the helper sleeps until the app
        // calls: no SMC reads, no wakeups. Under a lease it ticks once a
        // second to watch temperatures and the lease.
        let idle = !manual && targets.iter().all(Option::is_none);
        let wait = if idle { IDLE_WAIT_MS } else { 1000u64.saturating_sub(last_tick.elapsed().as_millis() as u64) as i32 };
        let mut pfd = libc::pollfd { fd: listener.as_raw_fd(), events: libc::POLLIN, revents: 0 };
        unsafe { libc::poll(&mut pfd, 1, wait) };
        let mut changed = false;
        while let Ok((stream, _)) = listener.accept() {
            let _ = stream.set_nonblocking(false);
            let _ = stream.set_read_timeout(Some(Duration::from_millis(500)));
            let mut line = String::new();
            let mut reader = BufReader::new(&stream);
            if reader.read_line(&mut line).is_err() {
                continue;
            }
            match serde_json::from_str::<Request>(line.trim()) {
                Ok(Request::Set { rpm }) => {
                    targets = rpm.into_iter().take(ctl.ranges.len()).collect();
                    lease = SystemTime::now() + Duration::from_secs(LEASE_SECS);
                    changed = true;
                    // read temperatures right away when waking from idle
                    if idle {
                        last_tick = Instant::now() - Duration::from_secs(5);
                    }
                }
                Ok(Request::Auto) => {
                    targets.clear();
                    changed = true;
                }
                Ok(Request::Charge { policy: p }) => {
                    let p = p.sanitize();
                    if policy != Some(p) {
                        policy = Some(p);
                        if let Ok(t) = serde_json::to_string(&p) {
                            let _ = std::fs::write(CHARGE_FILE, t);
                        }
                        // answer with the new state
                        tick_charge(charger.as_ref(), policy, &mut cstate, &mut touched);
                        charge_at = Instant::now();
                    }
                }
                _ => {}
            }
            if changed {
                apply(&mut ctl, &targets, hot, &mut manual);
            }
            let note = if hot { "hot" } else { "" }.to_string();
            let charge = Status { supported: charger.is_some(), policy, state: cstate };
            let reply = Reply { version: VERSION, manual, note, charge };
            if let Ok(mut s) = serde_json::to_string(&reply) {
                s.push('\n');
                let _ = (&stream).write_all(s.as_bytes());
            }
        }
        if (touched || policy.is_some_and(|p| p.active())) && charge_at.elapsed() >= CHARGE_EVERY {
            charge_at = Instant::now();
            tick_charge(charger.as_ref(), policy, &mut cstate, &mut touched);
        }
        if !(idle && !changed) && last_tick.elapsed() >= Duration::from_secs(1) {
            last_tick = Instant::now();
            let r = probe.read();
            let snap = Snapshot { sensors: merge(r.hid, r.smc_temps), ..Default::default() };
            hot = snap.hottest().is_some_and(|(_, c)| c >= super::control::TRIP_C);
            if SystemTime::now() > lease {
                targets.clear();
            }
            apply(&mut ctl, &targets, hot, &mut manual);
        }
    }

    /// Read the battery and switch charging to what the policy says. It is
    /// written again whenever the SMC disagrees (macOS may reset it).
    fn tick_charge(charger: Option<&Charger>, policy: Option<Policy>, st: &mut charge::State, touched: &mut bool) {
        let Some(c) = charger else { return };
        let p = policy.unwrap_or_default();
        let Some(b) = charge::read_battery() else { return };
        let off = charge::decide(&p, b, st);
        if c.inhibited() != Some(off) {
            c.set(off);
        }
        *touched = off;
    }

    fn apply(ctl: &mut Control, targets: &[Option<f32>], hot: bool, manual: &mut bool) {
        let want = !hot && targets.iter().any(Option::is_some);
        if !want {
            if *manual {
                ctl.auto();
                *manual = false;
            }
            return;
        }
        let mut any = false;
        for i in 0..ctl.ranges.len() {
            match targets.get(i).copied().flatten() {
                Some(rpm) => any |= ctl.manual(i, rpm),
                None => ctl.release(i),
            }
        }
        *manual = any || *manual;
    }
}

#[cfg(not(target_os = "macos"))]
pub fn serve(_uid: u32) -> ! {
    eprintln!("The fan helper runs on macOS only.");
    std::process::exit(1);
}

