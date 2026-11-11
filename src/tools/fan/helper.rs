//! The fan helper: a small root daemon that writes fan speeds to the SMC,
//! and the app's side of talking to it.
//!
//! Installed once with the admin password (`osascript … with administrator
//! privileges`): this binary is copied to `/Library/PrivilegedHelperTools`
//! and started by launchd as `--fan-helper <uid>`. It listens on a Unix
//! socket only that user (and root) can open, one JSON line in, one out.
//!
//! Safety lives in the helper, so it holds even if the app dies:
//! - a `set` is a lease: without a new one within [`LEASE_SECS`] (app quit,
//!   crashed or the Mac slept) the fans go back to Auto;
//! - any sensor at 95 °C or more gives the fans back to Auto;
//! - on start and on SIGTERM the fans go back to Auto;
//! - every second the speeds are written again, so a reset by macOS (after
//!   sleep, on M3 and later) doesn't stick.

use serde::{Deserialize, Serialize};
use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::UnixStream;
use std::path::Path;
use std::time::Duration;

// The helper keeps the ids from the app's first name (CleanMeBro), so a
// helper that is already installed keeps working after the rename.
pub const LABEL: &str = "com.romanfurman6.cleanmebro.fanhelper";
pub const SOCKET: &str = "/var/run/com.romanfurman6.cleanmebro.fan.sock";
const BIN: &str = "/Library/PrivilegedHelperTools/com.romanfurman6.cleanmebro.fanhelper";
const PLIST: &str = "/Library/LaunchDaemons/com.romanfurman6.cleanmebro.fanhelper.plist";
/// Bump when the helper's behaviour or protocol changes: the app then
/// offers to update it.
pub const VERSION: u32 = 2;
/// How long a `set` holds without a fresh one.
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
pub const LEASE_SECS: u64 = 8;

/// How long the idle helper waits for a call before it checks for a stop
/// signal again (a signal also wakes it early).
#[cfg(target_os = "macos")]
const IDLE_WAIT_MS: i32 = 10_000;

#[derive(Serialize, Deserialize, Debug, PartialEq)]
#[serde(tag = "op", rename_all = "lowercase")]
pub enum Request {
    Hello,
    /// Hold each fan at this rpm (`None` = Auto for that fan).
    Set { rpm: Vec<Option<f32>> },
    Auto,
}

#[derive(Serialize, Deserialize, Debug, Default, PartialEq)]
pub struct Reply {
    pub version: u32,
    /// Some fan is under the helper's control now.
    pub manual: bool,
    /// The helper refused or reverted: "hot" (a sensor at 95 °C).
    #[serde(default)]
    pub note: String,
}

// ---------------------------------------------------------------- client

/// Send one request to the helper. `None` if it isn't running.
pub fn ask(req: &Request) -> Option<Reply> {
    let mut s = UnixStream::connect(SOCKET).ok()?;
    s.set_read_timeout(Some(Duration::from_millis(1500))).ok()?;
    s.set_write_timeout(Some(Duration::from_millis(500))).ok()?;
    let mut line = serde_json::to_string(req).ok()?;
    line.push('\n');
    s.write_all(line.as_bytes()).ok()?;
    let mut out = String::new();
    BufReader::new(s).read_line(&mut out).ok()?;
    serde_json::from_str(out.trim()).ok()
}

/// Install (or update) the helper. Blocks while macOS asks for the admin
/// password. `Err` carries what to show, "" when the user cancelled.
pub fn install() -> Result<(), String> {
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let uid = unsafe { libc::getuid() };
    let plist = format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>Label</key><string>{LABEL}</string>
  <key>ProgramArguments</key>
  <array><string>{BIN}</string><string>--fan-helper</string><string>{uid}</string></array>
  <key>RunAtLoad</key><true/>
  <key>KeepAlive</key><true/>
  <key>ProcessType</key><string>Interactive</string>
</dict>
</plist>
"#
    );
    let dir = std::env::temp_dir().join(format!("cmdmeow-fan-{}", std::process::id()));
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let plist_tmp = dir.join("helper.plist");
    std::fs::write(&plist_tmp, plist).map_err(|e| e.to_string())?;
    let script = install_script(&sh_quote(&exe), &sh_quote(&plist_tmp));
    let r = run_admin(&dir, &script, "Cmd+Meow wants to add its fan helper so it can change fan speed.");
    let _ = std::fs::remove_dir_all(&dir);
    r?;
    // give launchd a moment to start it
    for _ in 0..40 {
        if ask(&Request::Hello).is_some() {
            return Ok(());
        }
        std::thread::sleep(Duration::from_millis(250));
    }
    Err("The helper was installed but didn't start.".into())
}

/// The root script that (re)installs the helper. An update has to:
/// - wait until launchd has really let go of the old helper: `bootout`
///   returns before it's gone, and `bootstrap` right after fails with
///   "Bootstrap failed: 5: Input/output error";
/// - put the new binary at a new inode (copy, then rename): copying over
///   the old file in place keeps the kernel's cached code signature of the
///   old one, so the new helper is killed at launch and never answers;
/// - re-enable the service, in case it was turned off.
fn install_script(exe: &str, plist: &str) -> String {
    format!(
        "set -e\n\
         launchctl bootout system/{LABEL} 2>/dev/null || true\n\
         i=0; while launchctl print system/{LABEL} >/dev/null 2>&1 && [ $i -lt 50 ]; do sleep 0.1; i=$((i+1)); done\n\
         mkdir -p /Library/PrivilegedHelperTools\n\
         rm -f {BIN}.new\n\
         cp {exe} {BIN}.new\n\
         chown root:wheel {BIN}.new\n\
         chmod 755 {BIN}.new\n\
         mv -f {BIN}.new {BIN}\n\
         cp -f {plist} {PLIST}\n\
         chown root:wheel {PLIST}\n\
         chmod 644 {PLIST}\n\
         launchctl enable system/{LABEL} 2>/dev/null || true\n\
         launchctl bootstrap system {PLIST} 2>/dev/null || {{ sleep 1; launchctl bootstrap system {PLIST}; }}\n"
    )
}

/// Stop and remove the helper (fans go back to Auto as it stops).
pub fn uninstall() -> Result<(), String> {
    let dir = std::env::temp_dir().join(format!("cmdmeow-fan-{}", std::process::id()));
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let script = format!("launchctl bootout system/{LABEL} 2>/dev/null || true\nrm -f {PLIST} {BIN} {SOCKET}\n");
    let r = run_admin(&dir, &script, "Cmd+Meow wants to remove its fan helper.");
    let _ = std::fs::remove_dir_all(&dir);
    r
}

fn sh_quote(p: &Path) -> String {
    format!("'{}'", p.to_string_lossy().replace('\'', r"'\''"))
}

/// Run `script` as root after the macOS admin prompt.
fn run_admin(dir: &Path, script: &str, prompt: &str) -> Result<(), String> {
    let file = dir.join("run.sh");
    std::fs::write(&file, script).map_err(|e| e.to_string())?;
    let cmd = format!("/bin/sh {}", sh_quote(&file));
    let apple = format!("do shell script \"{}\" with prompt \"{}\" with administrator privileges", cmd.replace('\\', "\\\\").replace('"', "\\\""), prompt);
    let out = std::process::Command::new("/usr/bin/osascript").arg("-e").arg(apple).output().map_err(|e| e.to_string())?;
    if out.status.success() {
        return Ok(());
    }
    let err = String::from_utf8_lossy(&out.stderr).trim().to_string();
    // -128: the user pressed Cancel
    if err.contains("-128") {
        return Err(String::new());
    }
    Err(if err.is_empty() { "The helper couldn't be installed.".into() } else { err })
}

// ---------------------------------------------------------------- daemon

/// The helper's main loop (`--fan-helper <uid>`), run as root by launchd.
#[cfg(target_os = "macos")]
pub fn serve(uid: u32) -> ! {
    use super::data::{merge, Snapshot};
    use super::probe::{mac::Control, Probe};
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
                _ => {}
            }
            if changed {
                apply(&mut ctl, &targets, hot, &mut manual);
            }
            let note = if hot { "hot" } else { "" }.to_string();
            let reply = Reply { version: VERSION, manual, note };
            if let Ok(mut s) = serde_json::to_string(&reply) {
                s.push('\n');
                let _ = (&stream).write_all(s.as_bytes());
            }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn protocol_round_trips() {
        let r = Request::Set { rpm: vec![Some(2400.0), None] };
        let s = serde_json::to_string(&r).unwrap();
        assert_eq!(s, r#"{"op":"set","rpm":[2400.0,null]}"#);
        assert_eq!(serde_json::from_str::<Request>(&s).unwrap(), r);
        assert_eq!(serde_json::from_str::<Request>(r#"{"op":"auto"}"#).unwrap(), Request::Auto);
        assert_eq!(serde_json::from_str::<Reply>(r#"{"version":1,"manual":true}"#).unwrap(), Reply { version: 1, manual: true, note: String::new() });
    }

    #[test]
    fn quotes_paths_for_sh() {
        assert_eq!(sh_quote(Path::new("/Applications/CmdMeow Dev.app/x")), "'/Applications/CmdMeow Dev.app/x'");
        assert_eq!(sh_quote(Path::new("/a'b")), r"'/a'\''b'");
    }

    #[test]
    fn install_replaces_the_binary_by_rename() {
        let s = install_script("'/x/cmdmeow'", "'/t/helper.plist'");
        // never copy over the running binary in place
        assert!(!s.contains(&format!("cp -f '/x/cmdmeow' {BIN}\n")));
        assert!(s.contains(&format!("mv -f {BIN}.new {BIN}")));
        // waits for the old helper before bootstrapping the new one
        let wait = s.find("while launchctl print").unwrap();
        assert!(wait < s.find("launchctl bootstrap").unwrap());
        assert!(s.find("launchctl bootout").unwrap() < wait);
    }
}
