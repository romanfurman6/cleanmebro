//! What starts with the Mac: apps opened at login (System Events' list) and
//! launchd agents and daemons. Each can be turned off without removing
//! anything, and turned back on.

use super::{mac, plist};
use std::collections::HashSet;
use std::path::{Path, PathBuf};

/// Which launchd folder a job comes from.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Scope {
    /// ~/Library/LaunchAgents
    User,
    /// /Library/LaunchAgents: runs for every user, switchable per user
    AllUsers,
    /// /Library/LaunchDaemons: runs as root, needs the admin password
    System,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Job {
    pub label: String,
    pub plist: PathBuf,
    pub program: Option<PathBuf>,
    pub scope: Scope,
    pub on: bool,
    /// Its program is gone: data left by a removed app.
    pub orphan: bool,
}

impl Job {
    fn domain(&self) -> String {
        match self.scope {
            Scope::System => "system".into(),
            _ => format!("gui/{}", mac::uid()),
        }
    }
}

/// An app macOS opens at login.
#[derive(Clone, Debug, PartialEq)]
pub struct LoginApp {
    pub name: String,
    pub path: PathBuf,
    pub hidden: bool,
    pub on: bool,
}

/// launchd jobs of every scope, macOS's own left out.
pub fn jobs(user_library: &Path, system: &Path, disabled_user: &HashSet<String>, disabled_system: &HashSet<String>) -> Vec<Job> {
    let mut out = Vec::new();
    for (dir, scope) in [(user_library.join("LaunchAgents"), Scope::User), (system.join("Library/LaunchAgents"), Scope::AllUsers), (system.join("Library/LaunchDaemons"), Scope::System)] {
        let Ok(rd) = std::fs::read_dir(&dir) else { continue };
        let mut found: Vec<Job> = rd
            .flatten()
            .filter(|e| e.file_name().to_string_lossy().ends_with(".plist"))
            .filter_map(|e| {
                let path = e.path();
                let v = plist::read(&path)?;
                let label = v.get_str("Label")?.to_string();
                if label.starts_with("com.apple.") {
                    return None;
                }
                let program = v
                    .get_str("Program")
                    .map(PathBuf::from)
                    .or_else(|| match v.get("ProgramArguments") {
                        Some(plist::Value::Array(a)) => a.first().and_then(|x| x.str()).map(PathBuf::from),
                        _ => None,
                    })
                    .or_else(|| v.get_str("BundleProgram").map(PathBuf::from));
                let orphan = program.as_ref().is_some_and(|p| p.is_absolute() && p.symlink_metadata().is_err());
                let disabled = if scope == Scope::System { disabled_system } else { disabled_user };
                let on = !disabled.contains(&label) && v.get_bool("Disabled") != Some(true);
                Some(Job { label, plist: path, program, scope, on, orphan })
            })
            .collect();
        found.sort_by_key(|j| j.label.to_lowercase());
        out.extend(found);
    }
    out
}

/// Labels `launchctl print-disabled <domain>` reports as disabled.
pub fn parse_disabled(out: &str) -> HashSet<String> {
    out.lines()
        .filter_map(|l| {
            let (k, v) = l.split_once("=>")?;
            let v = v.trim();
            (v == "disabled" || v == "true").then(|| k.trim().trim_matches('"').to_string())
        })
        .collect()
}

pub fn disabled(domain: &str) -> HashSet<String> {
    let Ok(o) = std::process::Command::new("/bin/launchctl").args(["print-disabled", domain]).output() else { return HashSet::new() };
    parse_disabled(&String::from_utf8_lossy(&o.stdout))
}

pub fn read_jobs(user_library: &Path) -> Vec<Job> {
    let du = disabled(&format!("gui/{}", mac::uid()));
    let ds = disabled("system");
    jobs(user_library, Path::new("/"), &du, &ds)
}

/// Turn a job on or off: it stays switched across restarts.
pub fn set_job(job: &Job, on: bool) -> Result<(), String> {
    let target = format!("{}/{}", job.domain(), job.label);
    let plist = mac::sh_quote(&job.plist.to_string_lossy());
    let cmd = if on {
        format!("/bin/launchctl enable {t}; /bin/launchctl bootstrap {d} {plist} 2>/dev/null; true", t = mac::sh_quote(&target), d = job.domain())
    } else {
        format!("/bin/launchctl disable {t}; /bin/launchctl bootout {t} 2>/dev/null; true", t = mac::sh_quote(&target))
    };
    if job.scope == Scope::System {
        mac::run_admin(&cmd, "CleanMeBro wants to change what starts with your Mac.").map(|_| ())
    } else {
        let o = std::process::Command::new("/bin/sh").arg("-c").arg(&cmd).output().map_err(|e| e.to_string())?;
        if o.status.success() {
            Ok(())
        } else {
            Err(String::from_utf8_lossy(&o.stderr).trim().to_string())
        }
    }
}

/// Apps opened at login. macOS asks once to let us read them.
pub fn login_apps() -> Result<Vec<LoginApp>, String> {
    let script = r#"set out to ""
tell application "System Events"
repeat with li in every login item
set out to out & (name of li) & tab & (path of li) & tab & (hidden of li) & linefeed
end repeat
end tell
return out"#;
    Ok(parse_login_apps(&mac::osascript(script)?))
}

pub fn parse_login_apps(out: &str) -> Vec<LoginApp> {
    out.lines()
        .filter_map(|l| {
            let mut it = l.split('\t');
            let name = it.next()?.trim().to_string();
            let path = PathBuf::from(it.next()?.trim());
            let hidden = it.next().is_some_and(|h| h.trim() == "true");
            (!name.is_empty()).then_some(LoginApp { name, path, hidden, on: true })
        })
        .collect()
}

/// Add or remove an app from the login list.
pub fn set_login_app(app: &LoginApp, on: bool) -> Result<(), String> {
    let script = if on {
        format!(
            "tell application \"System Events\" to make login item at end with properties {{path:{}, hidden:{}}}",
            mac::as_quote(&app.path.to_string_lossy()),
            app.hidden
        )
    } else {
        format!("tell application \"System Events\" to delete login item {}", mac::as_quote(&app.name))
    };
    mac::osascript(&script).map(|_| ())
}

/// System Settings › General › Login Items.
pub fn open_settings() {
    mac::open("x-apple.systempreferences:com.apple.LoginItems-Settings.extension");
}

#[cfg(test)]
mod tests {
    use super::super::leftovers::tests::Fake;
    use super::*;

    #[test]
    fn reads_jobs_and_their_state() {
        let f = Fake::new("login");
        let job = |label: &str, prog: &str, extra: &str| format!("<plist><dict><key>Label</key><string>{label}</string><key>ProgramArguments</key><array><string>{prog}</string></array>{extra}</dict></plist>");
        f.file("home/Library/LaunchAgents/com.google.updater.plist", &job("com.google.updater", "/bin/sh", ""));
        f.file("home/Library/LaunchAgents/com.gone.agent.plist", &job("com.gone.agent", "/nope/agent", ""));
        f.file("home/Library/LaunchAgents/com.off.agent.plist", &job("com.off.agent", "/bin/sh", "<key>Disabled</key><true/>"));
        f.file("home/Library/LaunchAgents/com.apple.x.plist", &job("com.apple.x", "/bin/sh", ""));
        f.file("sys/Library/LaunchDaemons/com.docker.vmnetd.plist", &job("com.docker.vmnetd", "/bin/sh", ""));
        f.file("sys/Library/LaunchAgents/com.ms.update.plist", &job("com.ms.update", "/bin/sh", ""));
        let du: HashSet<String> = ["com.google.updater".to_string()].into();
        let js = jobs(&f.roots.user_library, &f.roots.system, &du, &HashSet::new());
        let find = |l: &str| js.iter().find(|j| j.label == l).unwrap();
        assert_eq!(js.len(), 5);
        assert!(!find("com.google.updater").on);
        assert!(find("com.gone.agent").orphan);
        assert!(!find("com.off.agent").on);
        assert_eq!(find("com.docker.vmnetd").scope, Scope::System);
        assert_eq!(find("com.ms.update").scope, Scope::AllUsers);
        assert!(find("com.ms.update").on);
    }

    #[test]
    fn parses_tool_output() {
        let d = parse_disabled("disabled services = {\n\t\"com.a.b\" => disabled\n\t\"com.c.d\" => enabled\n\t\"com.e.f\" => true\n}\n");
        assert!(d.contains("com.a.b") && d.contains("com.e.f") && !d.contains("com.c.d"));
        let apps = parse_login_apps("Slack\t/Applications/Slack.app\ttrue\nSpotify\t/Applications/Spotify.app\tfalse\n\n");
        assert_eq!(apps.len(), 2);
        assert!(apps[0].hidden && !apps[1].hidden);
        assert_eq!(apps[1].path, PathBuf::from("/Applications/Spotify.app"));
    }
}
