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
    /// Owner (uid, gid) before an admin move, given back on Undo.
    pub owner: Option<(u32, u32)>,
}

#[derive(Clone, Debug, Default)]
pub struct Report {
    pub moved: Vec<Moved>,
    /// (what, why)
    pub failed: Vec<(String, String)>,
    /// Names of the apps handled.
    pub names: Vec<String>,
    pub reset: bool,
    /// Apps whose bundle was meant to go.
    pub apps: Vec<String>,
    /// Of those, the ones whose bundle stayed.
    pub kept: Vec<String>,
    /// What didn't move, ready for another try.
    pub retry: Vec<Target>,
}

impl Report {
    pub fn bytes(&self) -> u64 {
        self.moved.iter().map(|m| m.bytes).sum()
    }

    /// The result line and, when something stayed, a second line saying what.
    pub fn summary(&self) -> (String, Option<String>) {
        let size = human_bytes(self.bytes());
        let who = match self.names.len() {
            0 => "Nothing".to_string(),
            1 => self.names[0].clone(),
            n => format!("{n} apps"),
        };
        let items = |n: usize| format!("{n} item{} couldn't be moved", if n == 1 { "" } else { "s" });
        let failed = (!self.failed.is_empty()).then(|| items(self.failed.len()));
        if self.moved.is_empty() {
            return (format!("{who} couldn't be removed"), failed);
        }
        if self.reset {
            return (format!("{who} is reset · {size} to the Trash"), failed);
        }
        if self.apps.is_empty() {
            return (format!("{size} of {who}'s files moved to the Trash"), failed);
        }
        let gone = self.apps.len() - self.kept.len();
        let stayed = (!self.kept.is_empty()).then(|| format!("{} stayed", list_names(&self.kept)));
        let title = match (gone, self.apps.len()) {
            (0, 1) => format!("{} stayed · {size} of its files moved", self.apps[0]),
            (0, _) => format!("No app removed · {size} of their files moved"),
            (g, n) if g == n => format!("{who} moved to the Trash · {size}"),
            (g, n) => format!("{g} of {n} apps moved to the Trash · {size}"),
        };
        let detail = match (gone, stayed) {
            (0, _) => self.failed.first().map(|(_, why)| why.clone()),
            (_, Some(s)) => Some(s),
            (_, None) => failed,
        };
        (title, detail)
    }
}

/// "A", "A and B", "A, B and 2 more".
fn list_names(names: &[String]) -> String {
    match names {
        [] => String::new(),
        [a] => a.clone(),
        [a, b] => format!("{a} and {b}"),
        [a, b, rest @ ..] => format!("{a}, {b} and {} more", rest.len()),
    }
}

fn human_bytes(b: u64) -> String {
    crate::kit::glass::human_bytes(b)
}

/// Whether moving `p` needs root: its folder isn't writable, or it is a
/// folder that isn't (an App Store or installer-package app is root's).
pub fn needs_root(p: &Path) -> bool {
    let Some(parent) = p.parent() else { return false };
    if !writable(parent) {
        return true;
    }
    p.symlink_metadata().is_ok_and(|m| m.is_dir()) && !writable(p)
}

fn writable(p: &Path) -> bool {
    let c = std::ffi::CString::new(p.to_string_lossy().as_bytes()).unwrap_or_default();
    unsafe { libc::access(c.as_ptr(), libc::W_OK) == 0 }
}

#[cfg(unix)]
fn owner_of(p: &Path) -> Option<(u32, u32)> {
    use std::os::unix::fs::MetadataExt;
    p.symlink_metadata().ok().map(|m| (m.uid(), m.gid()))
}
#[cfg(not(unix))]
fn owner_of(_p: &Path) -> Option<(u32, u32)> {
    None
}

/// Where removal may ever reach: app folders, ~/Library and the system's
/// launchd and helper folders. A last guard under the matching rules.
pub fn movable(p: &Path, roots: &Roots, app_roots: &[PathBuf]) -> bool {
    if crate::kit::fs::is_protected(p).is_some() {
        return false;
    }
    let depth_under = |root: &Path| p.strip_prefix(root).ok().map(|r| r.components().count());
    let is_app = p.extension().is_some_and(|e| e == "app");
    if is_app && app_roots.iter().any(|r| matches!(depth_under(r), Some(1 | 2))) {
        return true;
    }
    // at least one folder below ~/Library (never ~/Library/Caches itself)
    if matches!(depth_under(&roots.user_library), Some(n) if n >= 2) {
        return true;
    }
    ["Library/LaunchAgents", "Library/LaunchDaemons", "Library/PrivilegedHelperTools"].iter().any(|d| matches!(depth_under(&roots.system.join(d)), Some(1)))
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
pub fn run(targets: &[Target], roots: &Roots, app_roots: &[PathBuf], mut progress: impl FnMut(String)) -> Report {
    let mut rep = Report { reset: targets.iter().all(|t| t.reset), ..Default::default() };
    let trash = crate::kit::fs::home().join(".Trash");
    // what didn't move, per target, for another try
    let mut retry: Vec<Vec<Item>> = vec![Vec::new(); targets.len()];
    // items that need the admin password, done together at the end: (target, item)
    let mut admin: Vec<(usize, Item)> = Vec::new();
    for (k, t) in targets.iter().enumerate() {
        rep.names.push(t.name.clone());
        let app_item = t.items.iter().find(|i| i.kind == Kind::App && i.selected && !t.reset);
        if app_item.is_some() {
            rep.apps.push(t.name.clone());
        }
        if t.app.is_some() && mac::is_running(&t.id) {
            progress(format!("Asking {} to quit…", t.name));
            mac::ask_to_quit(&t.id);
            let start = Instant::now();
            while mac::is_running(&t.id) && start.elapsed() < QUIT_WAIT {
                std::thread::sleep(Duration::from_millis(250));
            }
            if mac::is_running(&t.id) {
                rep.failed.push((t.name.clone(), "It is still open. Quit it, then try again.".into()));
                retry[k] = t.items.iter().filter(|i| i.selected).cloned().collect();
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
                rep.failed.push((it.path.display().to_string(), "Not a place apps keep their files; left alone".into()));
                continue;
            }
            if it.admin || needs_root(&it.path) {
                admin.push((k, it.clone()));
                continue;
            }
            if it.kind == Kind::Launch {
                // stop a running agent before its plist goes
                let _ = std::process::Command::new("/bin/launchctl").arg("bootout").arg(format!("gui/{}", mac::uid())).arg(&it.path).output();
            }
            match crate::kit::fs::move_to_trash_at(&it.path) {
                Ok(to) => {
                    prefs |= it.kind == Kind::Preferences;
                    rep.moved.push(Moved { from: it.path.clone(), to, admin: false, bytes: it.bytes, owner: None });
                }
                // root's, or locked down some other way: one more go with the password
                Err(e) if e.to_lowercase().contains("permission") => admin.push((k, it.clone())),
                Err(e) => {
                    rep.failed.push((it.path.display().to_string(), e));
                    retry[k].push(it.clone());
                }
            }
        }
        if t.reset && prefs && !t.id.is_empty() {
            // drop what the preferences daemon still holds in memory
            let _ = std::process::Command::new("/usr/bin/defaults").arg("delete").arg(&t.id).output();
        }
    }
    if !admin.is_empty() {
        progress("Waiting for your password…".into());
        let mut cmd = String::new();
        let mut planned = Vec::new();
        let mut taken: Vec<PathBuf> = Vec::new();
        for (k, it) in &admin {
            let p = &it.path;
            let name = p.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
            let mut dest = trash_dest(&trash, &name);
            let mut n = 1;
            while taken.contains(&dest) {
                dest = trash_dest(&trash, &format!("{n} {name}"));
                n += 1;
            }
            taken.push(dest.clone());
            if it.kind == Kind::Launch && p.extension().is_some_and(|e| e == "plist") {
                let domain = if p.starts_with(roots.system.join("Library/LaunchDaemons")) { "system".to_string() } else { format!("gui/{}", mac::uid()) };
                cmd.push_str(&format!("/bin/launchctl bootout {domain} {} 2>/dev/null; ", mac::sh_quote(&p.to_string_lossy())));
            }
            cmd.push_str(&format!(
                "/bin/mv -n {src} {dst} && /usr/sbin/chown -R {uid} {dst}; ",
                src = mac::sh_quote(&p.to_string_lossy()),
                dst = mac::sh_quote(&dest.to_string_lossy()),
                uid = mac::uid()
            ));
            planned.push((*k, it.clone(), Moved { from: p.clone(), to: dest, admin: true, bytes: it.bytes, owner: owner_of(p) }));
        }
        cmd.push_str("true");
        let res = mac::run_admin(&cmd, "Cmd+Meow wants to move apps and their files to your Trash.");
        for (k, it, m) in planned {
            let why = match &res {
                Ok(_) if m.to.symlink_metadata().is_ok() && m.from.symlink_metadata().is_err() => {
                    rep.moved.push(m);
                    continue;
                }
                Ok(_) => "macOS didn't let it move".to_string(),
                Err(e) if e.contains("-128") || e.to_lowercase().contains("cancel") => "Password not given".to_string(),
                Err(e) => e.clone(),
            };
            rep.failed.push((m.from.display().to_string(), why));
            retry[k].push(it);
        }
    }
    for (t, items) in targets.iter().zip(retry) {
        let app_kept = !t.reset && items.iter().any(|i| i.kind == Kind::App);
        if app_kept {
            rep.kept.push(t.name.clone());
        }
        if !items.is_empty() {
            let mut again = t.clone();
            again.items = items.into_iter().map(|mut i| {
                i.selected = true;
                i
            }).collect();
            rep.retry.push(again);
        }
    }
    rep
}

/// Put everything from `rep` back where it was. Returns (restored, failures).
pub fn undo(rep: &Report) -> (usize, Vec<(String, String)>) {
    let mut ok = 0;
    let mut failed = Vec::new();
    let mut admin_cmd = String::new();
    let mut admin_items = Vec::new();
    for m in rep.moved.iter().rev() {
        if m.from.symlink_metadata().is_ok() {
            failed.push((m.from.display().to_string(), "Something new is already there".into()));
            continue;
        }
        if m.to.symlink_metadata().is_err() {
            failed.push((m.from.display().to_string(), "No longer in the Trash".into()));
            continue;
        }
        if m.admin {
            let owner = m.owner.map(|(u, g)| format!("{u}:{g}")).unwrap_or_else(|| "root:wheel".into());
            admin_cmd.push_str(&format!("/bin/mv -n {} {} && /usr/sbin/chown -R {owner} {}; ", mac::sh_quote(&m.to.to_string_lossy()), mac::sh_quote(&m.from.to_string_lossy()), mac::sh_quote(&m.from.to_string_lossy())));
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
        match mac::run_admin(&admin_cmd, "Cmd+Meow wants to put system app files back.") {
            Ok(_) => {
                for m in admin_items {
                    if m.from.symlink_metadata().is_ok() {
                        ok += 1;
                    } else {
                        failed.push((m.from.display().to_string(), "Couldn't be put back".into()));
                    }
                }
            }
            Err(e) => failed.extend(admin_items.iter().map(|m| (m.from.display().to_string(), e.clone()))),
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
        let roots = Roots { user_library: PathBuf::from("/Users/a/Library"), system: PathBuf::from("/") };
        let apps = vec![PathBuf::from("/Applications"), PathBuf::from("/Users/a/Applications")];
        assert!(movable(Path::new("/Applications/Docker.app"), &roots, &apps));
        assert!(movable(Path::new("/Applications/Setapp/X.app"), &roots, &apps));
        assert!(!movable(Path::new("/Applications"), &roots, &apps));
        assert!(!movable(Path::new("/Applications/Docs"), &roots, &apps));
        assert!(movable(Path::new("/Users/a/Library/Caches/com.x.y"), &roots, &apps));
        assert!(!movable(Path::new("/Users/a/Library/Caches"), &roots, &apps));
        assert!(!movable(Path::new("/Users/a/Library"), &roots, &apps));
        assert!(movable(Path::new("/Library/LaunchDaemons/com.x.plist"), &roots, &apps));
        assert!(!movable(Path::new("/Library/LaunchDaemons"), &roots, &apps));
        assert!(!movable(Path::new("/Library/Application Support/x"), &roots, &apps));
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
        let app = f.file("apps/Thing.app/Contents/Info.plist", "x").parent().unwrap().parent().unwrap().to_path_buf();
        let cache = f.lib("Caches/com.thing.app/blob");
        let cache_dir = cache.parent().unwrap().to_path_buf();
        let prefs = f.lib("Preferences/com.thing.app.plist");
        let keep = f.lib("Logs/com.thing.app/log");
        let item = |p: &Path, kind, selected| Item { path: p.to_path_buf(), kind, bytes: 10, admin: false, selected };
        let t = Target {
            name: "Thing".into(),
            id: "com.thing.app".into(),
            app: Some(app.clone()),
            items: vec![item(&app, Kind::App, true), item(&cache_dir, Kind::Caches, true), item(&prefs, Kind::Preferences, true), item(keep.parent().unwrap(), Kind::Logs, false)],
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
        let app = f.file("apps/Thing.app/x", "x").parent().unwrap().to_path_buf();
        let support = f.lib("Application Support/com.reset.app/db");
        let outside = f.file("elsewhere/file", "x");
        let item = |p: &Path, kind| Item { path: p.to_path_buf(), kind, bytes: 1, admin: false, selected: true };
        let t = Target {
            name: "Thing".into(),
            id: "com.reset.app".into(),
            app: Some(app.clone()),
            items: vec![item(&app, Kind::App), item(support.parent().unwrap(), Kind::Support), item(&outside, Kind::Support)],
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

    #[test]
    fn summary_counts_only_what_moved() {
        let mv = |p: &str, b| Moved { from: PathBuf::from(p), to: PathBuf::from("/t"), admin: false, bytes: b, owner: None };
        let names = |v: &[&str]| v.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        // all gone
        let all = Report { moved: vec![mv("/Applications/A.app", 1000)], names: names(&["A"]), apps: names(&["A"]), ..Default::default() };
        assert_eq!(all.summary(), (format!("A moved to the Trash · {}", human_bytes(1000)), None));
        // two of five stayed
        let part = Report {
            moved: vec![mv("/Applications/A.app", 1), mv("/Applications/B.app", 1), mv("/Applications/C.app", 1)],
            failed: vec![("/Applications/D.app".into(), "x".into()), ("/Applications/E.app".into(), "x".into())],
            names: names(&["A", "B", "C", "D", "E"]),
            apps: names(&["A", "B", "C", "D", "E"]),
            kept: names(&["D", "E"]),
            ..Default::default()
        };
        let (t, d) = part.summary();
        assert!(t.starts_with("3 of 5 apps moved"), "{t}");
        assert_eq!(d.as_deref(), Some("D and E stayed"));
        // the app stayed, only its files went
        let one = Report { moved: vec![mv("/Users/a/Library/Caches/x", 5)], failed: vec![("/Applications/A.app".into(), "Password not given".into())], names: names(&["A"]), apps: names(&["A"]), kept: names(&["A"]), ..Default::default() };
        let (t, d) = one.summary();
        assert!(t.starts_with("A stayed · "), "{t}");
        assert_eq!(d.as_deref(), Some("Password not given"));
    }

    #[test]
    fn what_fails_comes_back_for_another_try() {
        let f = Fake::new("retry");
        let app = f.file("apps/Thing.app/x", "x").parent().unwrap().to_path_buf();
        let outside = f.file("elsewhere/file", "x");
        let item = |p: &Path, kind| Item { path: p.to_path_buf(), kind, bytes: 1, admin: false, selected: true };
        let t = Target { name: "Thing".into(), id: "com.retry.app".into(), app: Some(app.clone()), items: vec![item(&app, Kind::App), item(&outside, Kind::Support)], reset: false };
        let rep = run(&[t], &f.roots, &[f.root.join("apps")], |_| {});
        assert_eq!(rep.apps, vec!["Thing".to_string()]);
        assert!(rep.kept.is_empty(), "the app moved");
        // a path outside app places is refused for good, not offered again
        assert!(rep.retry.is_empty());
        undo(&rep);
    }
}
