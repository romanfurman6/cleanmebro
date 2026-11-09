//! Removing: quit the app, move its bundle and files to the Trash (items
//! outside the home folder after one admin password prompt), and Undo, which
//! moves everything back. Nothing is ever deleted outright.

use super::leftovers::{Item, Kind, Roots};
use super::mac;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

/// How long an app gets to quit before we give up on it.
const QUIT_WAIT: Duration = Duration::from_secs(20);

/// One app (or the leftovers of one gone app) to remove or reset.
#[derive(Clone, Debug)]
pub struct Target {
    pub name: String,
    pub id: String,
    /// The bundle, when it is installed.
    pub app: Option<PathBuf>,
    /// Bundle and files; only `selected` ones are moved.
    pub items: Vec<Item>,
    /// Keep the app, remove its data.
    pub reset: bool,
}

/// Something that went to the Trash.
#[derive(Clone, Debug, PartialEq)]
pub struct Moved {
    pub from: PathBuf,
    pub to: PathBuf,
    pub admin: bool,
    pub bytes: u64,
}

#[derive(Clone, Debug, Default)]
pub struct Report {
    pub moved: Vec<Moved>,
    /// (what, why)
    pub failed: Vec<(String, String)>,
    /// Names of the apps handled.
    pub names: Vec<String>,
    pub reset: bool,
}

impl Report {
    pub fn bytes(&self) -> u64 {
        self.moved.iter().map(|m| m.bytes).sum()
    }
}

/// Where removal may ever reach: app folders, ~/Library and the system's
/// launchd and helper folders. A last guard under the matching rules.
pub fn movable(p: &Path, roots: &Roots, app_roots: &[PathBuf]) -> bool {
    if crate::kit::fs::is_protected(p).is_some() {
        return false;
    }
    let depth_under = |root: &Path| p.strip_prefix(root).ok().map(|r| r.components().count());
    let is_app = p.extension().is_some_and(|e| e == "app");
    if is_app
        && app_roots
            .iter()
            .any(|r| matches!(depth_under(r), Some(1 | 2)))
    {
        return true;
    }
    // at least one folder below ~/Library (never ~/Library/Caches itself)
    if matches!(depth_under(&roots.user_library), Some(n) if n >= 2) {
        return true;
    }
    [
        "Library/LaunchAgents",
        "Library/LaunchDaemons",
        "Library/PrivilegedHelperTools",
    ]
    .iter()
    .any(|d| matches!(depth_under(&roots.system.join(d)), Some(1)))
}

/// A free name for `name` in the Trash folder.
fn trash_dest(trash: &Path, name: &str) -> PathBuf {
    let mut dest = trash.join(name);
    let mut k = 1;
    while dest.symlink_metadata().is_ok() {
        let (stem, ext) = match name.rsplit_once('.') {
            Some((s, e)) if !s.is_empty() => (s.to_string(), format!(".{e}")),
            _ => (name.to_string(), String::new()),
        };
        dest = trash.join(format!("{stem} {k}{ext}"));
        k += 1;
    }
    dest
}

/// Remove `targets`. `progress` gets a line for the UI as it goes.
pub fn run(
    targets: &[Target],
    roots: &Roots,
    app_roots: &[PathBuf],
    mut progress: impl FnMut(String),
) -> Report {
    let mut rep = Report {
        reset: targets.iter().all(|t| t.reset),
        ..Default::default()
    };
    let trash = crate::kit::fs::home().join(".Trash");
    // items that need the admin password, done together at the end
    let mut admin: Vec<(PathBuf, u64, Kind)> = Vec::new();
    for t in targets {
        rep.names.push(t.name.clone());
        if t.app.is_some() && mac::is_running(&t.id) {
            progress(format!("Asking {} to quit…", t.name));
            mac::ask_to_quit(&t.id);
            let start = Instant::now();
            while mac::is_running(&t.id) && start.elapsed() < QUIT_WAIT {
                std::thread::sleep(Duration::from_millis(250));
            }
            if mac::is_running(&t.id) {
                rep.failed.push((
                    t.name.clone(),
                    "It is still open. Quit it, then try again.".into(),
                ));
                continue;
            }
        }
        progress(format!("Moving {} to the Trash…", t.name));
        let mut prefs = false;
        for it in t.items.iter().filter(|i| i.selected) {
            if t.reset && matches!(it.kind, Kind::App | Kind::Launch) {
                continue;
            }
            if !it.path.exists() && it.path.symlink_metadata().is_err() {
                continue;
            }
            if !movable(&it.path, roots, app_roots) {
                rep.failed.push((
                    it.path.display().to_string(),
                    "Not a place apps keep their files; left alone".into(),
                ));
                continue;
            }
            if it.admin {
                admin.push((it.path.clone(), it.bytes, it.kind));
                continue;
            }
            if it.kind == Kind::Launch {
                // stop a running agent before its plist goes
                let _ = std::process::Command::new("/bin/launchctl")
                    .arg("bootout")
                    .arg(format!("gui/{}", mac::uid()))
                    .arg(&it.path)
                    .output();
            }
            match crate::kit::fs::move_to_trash_at(&it.path) {
                Ok(to) => {
                    prefs |= it.kind == Kind::Preferences;
                    rep.moved.push(Moved {
                        from: it.path.clone(),
                        to,
                        admin: false,
                        bytes: it.bytes,
                    });
                }
                // a bundle installed by an installer package can be root's
                Err(_) if it.kind == Kind::App && !writable_parent(&it.path) => {
                    admin.push((it.path.clone(), it.bytes, it.kind))
                }
                Err(e) => rep.failed.push((it.path.display().to_string(), e)),
            }
        }
        if t.reset && prefs && !t.id.is_empty() {
            // drop what the preferences daemon still holds in memory
            let _ = std::process::Command::new("/usr/bin/defaults")
                .arg("delete")
                .arg(&t.id)
                .output();
        }
    }
    if !admin.is_empty() {
        progress("Waiting for your password…".into());
        let mut cmd = String::new();
        let mut planned = Vec::new();
        let mut taken: Vec<PathBuf> = Vec::new();
        for (p, bytes, kind) in &admin {
            let name = p
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default();
            let mut dest = trash_dest(&trash, &name);
            let mut k = 1;
            while taken.contains(&dest) {
                dest = trash_dest(&trash, &format!("{k} {name}"));
                k += 1;
            }
            taken.push(dest.clone());
            if *kind == Kind::Launch && p.extension().is_some_and(|e| e == "plist") {
                let domain = if p.starts_with(roots.system.join("Library/LaunchDaemons")) {
                    "system".to_string()
                } else {
                    format!("gui/{}", mac::uid())
                };
                cmd.push_str(&format!(
                    "/bin/launchctl bootout {domain} {} 2>/dev/null; ",
                    mac::sh_quote(&p.to_string_lossy())
                ));
            }
            cmd.push_str(&format!(
                "/bin/mv -n {src} {dst} && /usr/sbin/chown -R {uid} {dst}; ",
                src = mac::sh_quote(&p.to_string_lossy()),
                dst = mac::sh_quote(&dest.to_string_lossy()),
                uid = mac::uid()
            ));
            planned.push(Moved {
                from: p.clone(),
                to: dest,
                admin: true,
                bytes: *bytes,
            });
        }
        cmd.push_str("true");
        match mac::run_admin(
            &cmd,
            "CleanMeBro wants to move app files that belong to the system to your Trash.",
        ) {
            Ok(_) => {
                for m in planned {
                    if m.to.symlink_metadata().is_ok() && m.from.symlink_metadata().is_err() {
                        rep.moved.push(m);
                    } else {
                        rep.failed
                            .push((m.from.display().to_string(), "Couldn't be moved".into()));
                    }
                }
            }
            Err(e) => {
                for m in planned {
                    rep.failed.push((m.from.display().to_string(), e.clone()));
                }
            }
        }
    }
    rep
}

fn writable_parent(p: &Path) -> bool {
    let Some(parent) = p.parent() else {
        return false;
    };
    let c = std::ffi::CString::new(parent.to_string_lossy().as_bytes()).unwrap_or_default();
    unsafe { libc::access(c.as_ptr(), libc::W_OK) == 0 }
}

/// Put everything from `rep` back where it was. Returns (restored, failures).
pub fn undo(rep: &Report) -> (usize, Vec<(String, String)>) {
    let mut ok = 0;
    let mut failed = Vec::new();
    let mut admin_cmd = String::new();
    let mut admin_items = Vec::new();
    for m in rep.moved.iter().rev() {
        if m.from.symlink_metadata().is_ok() {
            failed.push((
                m.from.display().to_string(),
                "Something new is already there".into(),
            ));
            continue;
        }
        if m.to.symlink_metadata().is_err() {
            failed.push((
                m.from.display().to_string(),
                "No longer in the Trash".into(),
            ));
            continue;
        }
        if m.admin {
            admin_cmd.push_str(&format!(
                "/bin/mv -n {} {} && /usr/sbin/chown -R root:wheel {}; ",
                mac::sh_quote(&m.to.to_string_lossy()),
                mac::sh_quote(&m.from.to_string_lossy()),
                mac::sh_quote(&m.from.to_string_lossy())
            ));
            admin_items.push(m);
            continue;
        }
        if let Some(parent) = m.from.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        match std::fs::rename(&m.to, &m.from) {
            Ok(()) => ok += 1,
            Err(e) => failed.push((m.from.display().to_string(), e.to_string())),
        }
    }
    if !admin_items.is_empty() {
        admin_cmd.push_str("true");
        match mac::run_admin(&admin_cmd, "CleanMeBro wants to put system app files back.") {
            Ok(_) => {
                for m in admin_items {
                    if m.from.symlink_metadata().is_ok() {
                        ok += 1;
                    } else {
                        failed.push((m.from.display().to_string(), "Couldn't be put back".into()));
                    }
                }
            }
            Err(e) => failed.extend(
                admin_items
                    .iter()
                    .map(|m| (m.from.display().to_string(), e.clone())),
            ),
        }
    }
    (ok, failed)
}

#[cfg(test)]
mod tests {
    use super::super::leftovers::tests::Fake;
    use super::*;

    #[test]
    fn only_app_places_are_movable() {
        let roots = Roots {
            user_library: PathBuf::from("/Users/a/Library"),
            system: PathBuf::from("/"),
        };
        let apps = vec![
            PathBuf::from("/Applications"),
            PathBuf::from("/Users/a/Applications"),
        ];
        assert!(movable(
            Path::new("/Applications/Docker.app"),
            &roots,
            &apps
        ));
        assert!(movable(
            Path::new("/Applications/Setapp/X.app"),
            &roots,
            &apps
        ));
        assert!(!movable(Path::new("/Applications"), &roots, &apps));
        assert!(!movable(Path::new("/Applications/Docs"), &roots, &apps));
        assert!(movable(
            Path::new("/Users/a/Library/Caches/com.x.y"),
            &roots,
            &apps
        ));
        assert!(!movable(
            Path::new("/Users/a/Library/Caches"),
            &roots,
            &apps
        ));
        assert!(!movable(Path::new("/Users/a/Library"), &roots, &apps));
        assert!(movable(
            Path::new("/Library/LaunchDaemons/com.x.plist"),
            &roots,
            &apps
        ));
        assert!(!movable(Path::new("/Library/LaunchDaemons"), &roots, &apps));
        assert!(!movable(
            Path::new("/Library/Application Support/x"),
            &roots,
            &apps
        ));
        assert!(!movable(Path::new("/usr/bin/x.app"), &roots, &apps));
    }

    #[test]
    fn trash_names_dont_collide() {
        let dir = std::env::temp_dir().join(format!("cmb-trashname-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("a.plist"), "x").unwrap();
        assert_eq!(trash_dest(&dir, "a.plist"), dir.join("a 1.plist"));
        assert_eq!(trash_dest(&dir, "b.plist"), dir.join("b.plist"));
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn remove_then_undo_round_trips() {
        let f = Fake::new("remove");
        let app = f
            .file("apps/Thing.app/Contents/Info.plist", "x")
            .parent()
            .unwrap()
            .parent()
            .unwrap()
            .to_path_buf();
        let cache = f.lib("Caches/com.thing.app/blob");
        let cache_dir = cache.parent().unwrap().to_path_buf();
        let prefs = f.lib("Preferences/com.thing.app.plist");
        let keep = f.lib("Logs/com.thing.app/log");
        let item = |p: &Path, kind, selected| Item {
            path: p.to_path_buf(),
            kind,
            bytes: 10,
            admin: false,
            selected,
        };
        let t = Target {
            name: "Thing".into(),
            id: "com.thing.app".into(),
            app: Some(app.clone()),
            items: vec![
                item(&app, Kind::App, true),
                item(&cache_dir, Kind::Caches, true),
                item(&prefs, Kind::Preferences, true),
                item(keep.parent().unwrap(), Kind::Logs, false),
            ],
            reset: false,
        };
        let mut lines = Vec::new();
        let rep = run(&[t], &f.roots, &[f.root.join("apps")], |l| lines.push(l));
        assert!(rep.failed.is_empty(), "{:?}", rep.failed);
        assert_eq!(rep.moved.len(), 3);
        assert_eq!(rep.bytes(), 30);
        assert!(!app.exists() && !cache_dir.exists() && !prefs.exists());
        assert!(keep.exists(), "unselected items stay");
        assert!(lines.iter().any(|l| l.contains("Thing")));
        let (ok, failed) = undo(&rep);
        assert_eq!((ok, failed.len()), (3, 0));
        assert!(app.join("Contents/Info.plist").exists() && cache.exists() && prefs.exists());
    }

    #[test]
    fn reset_keeps_the_app_and_refuses_odd_paths() {
        let f = Fake::new("reset");
        let app = f
            .file("apps/Thing.app/x", "x")
            .parent()
            .unwrap()
            .to_path_buf();
        let support = f.lib("Application Support/com.thing.app/db");
        let outside = f.file("elsewhere/file", "x");
        let item = |p: &Path, kind| Item {
            path: p.to_path_buf(),
            kind,
            bytes: 1,
            admin: false,
            selected: true,
        };
        let t = Target {
            name: "Thing".into(),
            id: "com.thing.app".into(),
            app: Some(app.clone()),
            items: vec![
                item(&app, Kind::App),
                item(support.parent().unwrap(), Kind::Support),
                item(&outside, Kind::Support),
            ],
            reset: true,
        };
        let rep = run(&[t], &f.roots, &[f.root.join("apps")], |_| {});
        assert!(app.exists(), "reset keeps the app");
        assert!(!support.exists());
        assert!(outside.exists());
        assert_eq!(rep.failed.len(), 1);
        assert!(rep.reset);
        undo(&rep);
    }
}
