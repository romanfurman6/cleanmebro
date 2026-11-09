//! Finding an app's files outside its bundle: its folders in ~/Library
//! (support data, caches, preferences, containers, saved state, logs, web
//! data) and launchd jobs that run it. Also the data left behind by apps that
//! are gone ("orphans").
//!
//! Matching is deliberately strict. A folder belongs to an app when it is
//! named by the app's bundle id (or an id below it), or, in the three folders
//! apps commonly name after themselves, by the app's exact name. A launchd
//! job belongs to an app when its label is the app's id or its program lives
//! inside the app. Anything that matches two installed apps, or that the
//! shared protection rules keep, is never offered.

use crate::kit::fs::is_protected;
use crate::kit::plist;
use std::collections::{BTreeMap, HashSet};
use std::path::{Path, PathBuf};

/// Where an item lives; drives its label and whether a reset removes it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Kind {
    App,
    Support,
    Caches,
    Preferences,
    Containers,
    GroupContainers,
    SavedState,
    Logs,
    WebData,
    Launch,
}

impl Kind {
    pub fn label(self) -> &'static str {
        match self {
            Kind::App => "App",
            Kind::Support => "Application Support",
            Kind::Caches => "Caches",
            Kind::Preferences => "Preferences",
            Kind::Containers => "Containers",
            Kind::GroupContainers => "Group Containers",
            Kind::SavedState => "Saved State",
            Kind::Logs => "Logs",
            Kind::WebData => "Web data",
            Kind::Launch => "Launch agent",
        }
    }
}

/// One folder of ~/Library (or /Library) that apps write into.
struct Place {
    rel: &'static str,
    kind: Kind,
    /// Apps also name folders here after themselves ("Application Support/Slack").
    by_name: bool,
    /// Entries may be data of apps that are gone.
    orphans: bool,
}

const USER_PLACES: &[Place] = &[
    Place {
        rel: "Application Support",
        kind: Kind::Support,
        by_name: true,
        orphans: true,
    },
    Place {
        rel: "Caches",
        kind: Kind::Caches,
        by_name: true,
        orphans: true,
    },
    Place {
        rel: "Preferences",
        kind: Kind::Preferences,
        by_name: false,
        orphans: true,
    },
    Place {
        rel: "Preferences/ByHost",
        kind: Kind::Preferences,
        by_name: false,
        orphans: true,
    },
    Place {
        rel: "Containers",
        kind: Kind::Containers,
        by_name: false,
        orphans: true,
    },
    Place {
        rel: "Group Containers",
        kind: Kind::GroupContainers,
        by_name: false,
        orphans: false,
    },
    Place {
        rel: "Saved Application State",
        kind: Kind::SavedState,
        by_name: false,
        orphans: true,
    },
    Place {
        rel: "Logs",
        kind: Kind::Logs,
        by_name: true,
        orphans: true,
    },
    Place {
        rel: "HTTPStorages",
        kind: Kind::WebData,
        by_name: false,
        orphans: true,
    },
    Place {
        rel: "WebKit",
        kind: Kind::WebData,
        by_name: false,
        orphans: true,
    },
    Place {
        rel: "Cookies",
        kind: Kind::WebData,
        by_name: false,
        orphans: true,
    },
    Place {
        rel: "Application Scripts",
        kind: Kind::Containers,
        by_name: false,
        orphans: true,
    },
];

/// launchd folders: (path under the root, needs admin).
const LAUNCH_USER: &str = "LaunchAgents";
const LAUNCH_SYSTEM: &[(&str, bool)] = &[
    ("Library/LaunchAgents", false),
    ("Library/LaunchDaemons", true),
];
const HELPERS_SYSTEM: &str = "Library/PrivilegedHelperTools";

/// Folder names that are vendors or shared homes, never one app's.
const SHARED_NAMES: &[&str] = &[
    "apple",
    "google",
    "microsoft",
    "adobe",
    "jetbrains",
    "mozilla",
    "oracle",
    "java",
    "crashreporter",
    "com.apple",
    "cloudkit",
    "icloud",
    "caches",
    "logs",
    "temp",
    "tmp",
    "default",
    "app",
    "data",
    "electron",
    "node",
    "python",
    "homebrew",
    "setapp",
    "steam",
    "unity",
    "autodesk",
    "zoom",
    "code",
];

/// The places to look in. Tests point these at temp folders.
#[derive(Clone, Debug)]
pub struct Roots {
    /// ~/Library
    pub user_library: PathBuf,
    /// "/" (holds Library/LaunchDaemons and so on)
    pub system: PathBuf,
}

impl Roots {
    pub fn real() -> Self {
        Roots {
            user_library: crate::kit::fs::home().join("Library"),
            system: PathBuf::from("/"),
        }
    }
}

/// An app as far as matching cares.
#[derive(Clone, Debug, Default)]
pub struct Owner {
    pub id: String,
    pub name: String,
    pub path: PathBuf,
}

/// A file or folder that belongs to an app.
#[derive(Clone, Debug, PartialEq)]
pub struct Item {
    pub path: PathBuf,
    pub kind: Kind,
    pub bytes: u64,
    /// Lives outside the home folder; moving it needs the admin password.
    pub admin: bool,
    pub selected: bool,
}

#[derive(Clone, Debug)]
struct Entry {
    path: PathBuf,
    name: String,
    kind: Kind,
    by_name: bool,
    orphans: bool,
    admin: bool,
    /// For launchd jobs: label and every string of the plist.
    job: Option<(String, Vec<String>)>,
}

/// One listing of every place, reused for all apps of a scan.
#[derive(Clone, Debug, Default)]
pub struct Index {
    entries: Vec<Entry>,
}

impl Index {
    pub fn build(roots: &Roots) -> Index {
        let mut entries = Vec::new();
        for pl in USER_PLACES {
            let dir = roots.user_library.join(pl.rel);
            for (path, name) in list(&dir) {
                if pl.rel == "Preferences" && path.is_dir() {
                    continue;
                }
                entries.push(Entry {
                    path,
                    name,
                    kind: pl.kind,
                    by_name: pl.by_name,
                    orphans: pl.orphans,
                    admin: false,
                    job: None,
                });
            }
        }
        let mut jobs = |dir: PathBuf, admin: bool| {
            for (path, name) in list(&dir) {
                if !name.ends_with(".plist") {
                    continue;
                }
                let v = plist::read(&path);
                let label = v
                    .as_ref()
                    .and_then(|v| v.get_str("Label"))
                    .unwrap_or(name.trim_end_matches(".plist"))
                    .to_string();
                let strings = v
                    .as_ref()
                    .map(|v| v.strings().into_iter().map(String::from).collect())
                    .unwrap_or_default();
                entries.push(Entry {
                    path,
                    name,
                    kind: Kind::Launch,
                    by_name: false,
                    orphans: false,
                    admin,
                    job: Some((label, strings)),
                });
            }
        };
        jobs(roots.user_library.join(LAUNCH_USER), false);
        for (rel, admin) in LAUNCH_SYSTEM {
            jobs(roots.system.join(rel), *admin);
        }
        for (path, name) in list(&roots.system.join(HELPERS_SYSTEM)) {
            entries.push(Entry {
                path,
                name,
                kind: Kind::Launch,
                by_name: false,
                orphans: false,
                admin: true,
                job: None,
            });
        }
        Index { entries }
    }

    /// Everything that belongs to `app` and to no other app in `all`
    /// (sizes not filled in).
    pub fn for_app(&self, app: &Owner, all: &[Owner]) -> Vec<Item> {
        let mut out = Vec::new();
        for e in &self.entries {
            if !belongs(e, app, all) {
                continue;
            }
            if all
                .iter()
                .any(|o| o.id != app.id && o.path != app.path && belongs(e, o, all))
            {
                continue;
            }
            if is_protected(&e.path).is_some() {
                continue;
            }
            out.push(Item {
                path: e.path.clone(),
                kind: e.kind,
                bytes: 0,
                admin: e.admin,
                selected: true,
            });
        }
        out.sort_by(|a, b| a.kind.cmp(&b.kind).then(a.path.cmp(&b.path)));
        out
    }

    /// Data of apps that are no longer installed, grouped by the id it is
    /// named after. `known` = every bundle id still on this Mac (installed
    /// anywhere, or running); `min_age_secs`: entries touched more recently
    /// are left alone, something is still using them.
    pub fn orphans(
        &self,
        known: &HashSet<String>,
        now: i64,
        min_age_secs: i64,
    ) -> BTreeMap<String, Vec<Item>> {
        let known_lc: Vec<String> = known.iter().map(|k| k.to_lowercase()).collect();
        let mut out: BTreeMap<String, Vec<Item>> = BTreeMap::new();
        for e in &self.entries {
            let id = match &e.job {
                // a job whose program is gone, named like an app id
                Some((label, strings)) => {
                    let program = strings.iter().find(|s| s.starts_with('/'));
                    if e.admin || program.is_none_or(|p| Path::new(p).exists()) {
                        continue;
                    }
                    label.clone()
                }
                None if e.orphans => match id_of(&e.name) {
                    Some(id) => id,
                    None => continue,
                },
                None => continue,
            };
            if !looks_like_bundle_id(&id) || is_system_id(&id) {
                continue;
            }
            let lc = id.to_lowercase();
            if known_lc.iter().any(|k| related(&lc, k)) {
                continue;
            }
            if is_protected(&e.path).is_some()
                || newest_mtime(&e.path).is_some_and(|t| now - t < min_age_secs)
            {
                continue;
            }
            out.entry(id).or_default().push(Item {
                path: e.path.clone(),
                kind: e.kind,
                bytes: 0,
                admin: e.admin,
                selected: true,
            });
        }
        out
    }
}

/// Whether index entry `e` is named for, or run by, `app`.
fn belongs(e: &Entry, app: &Owner, all: &[Owner]) -> bool {
    if app.id.is_empty() {
        return false;
    }
    let id = app.id.to_lowercase();
    if let Some((label, strings)) = &e.job {
        let inside = format!("{}/", app.path.display());
        return under_id(&label.to_lowercase(), &id)
            || (!app.path.as_os_str().is_empty()
                && strings
                    .iter()
                    .any(|s| s.starts_with(&inside) || *s == app.path.display().to_string()));
    }
    let stem = id_of(&e.name)
        .unwrap_or_else(|| e.name.clone())
        .to_lowercase();
    if e.kind == Kind::Launch {
        // privileged helper tools are named by id
        return under_id(&stem, &id);
    }
    if under_id(&stem, &id) {
        return true;
    }
    if e.kind == Kind::GroupContainers {
        // "TEAMID.com.foo.app", "group.com.foo.app", or the vendor's "group.com.foo"
        if stem.ends_with(&format!(".{id}")) {
            return true;
        }
        if let Some(vendor) = vendor_of(&id) {
            let others = all.iter().any(|o| {
                o.id.to_lowercase() != id && o.id.to_lowercase().starts_with(&format!("{vendor}."))
            });
            if !others
                && (stem == format!("group.{vendor}")
                    || stem.ends_with(&format!(".group.{vendor}")))
            {
                return true;
            }
        }
        return false;
    }
    if e.by_name {
        let name = app.name.trim().to_lowercase();
        let same_name = all
            .iter()
            .filter(|o| o.name.trim().to_lowercase() == name)
            .count();
        return name.chars().count() >= 3
            && !SHARED_NAMES.contains(&name.as_str())
            && same_name <= 1
            && e.name.to_lowercase() == name;
    }
    false
}

/// `name` is `id` or something below it ("com.foo.app.helper").
fn under_id(name: &str, id: &str) -> bool {
    name == id || (name.starts_with(id) && name.as_bytes().get(id.len()) == Some(&b'.'))
}

/// Either id is the other or below it.
fn related(a: &str, b: &str) -> bool {
    under_id(a, b) || under_id(b, a)
}

/// "com.foo" of "com.foo.app", when that still says who the vendor is.
fn vendor_of(id: &str) -> Option<String> {
    let parts: Vec<&str> = id.split('.').collect();
    (parts.len() >= 3).then(|| parts[..parts.len() - 1].join("."))
}

/// The id an entry is named by: its name without ".plist", ".savedState",
/// ".binarycookies", and the machine id ByHost files carry.
pub fn id_of(name: &str) -> Option<String> {
    let mut s = name;
    for suf in [".plist", ".savedState", ".binarycookies"] {
        if let Some(t) = s.strip_suffix(suf) {
            s = t;
            break;
        }
    }
    // ByHost: "com.foo.app.0123ABCD-...-....plist"
    if let Some((head, tail)) = s.rsplit_once('.') {
        if tail.len() >= 12 && tail.chars().all(|c| c.is_ascii_hexdigit() || c == '-') {
            s = head;
        }
    }
    looks_like_bundle_id(s).then(|| s.to_string())
}

/// Reverse-DNS with at least three parts and a domain-like first part.
pub fn looks_like_bundle_id(s: &str) -> bool {
    let parts: Vec<&str> = s.split('.').collect();
    if parts.len() < 3 || parts.iter().any(|p| p.is_empty()) {
        return false;
    }
    let tld = parts[0];
    if !(2..=6).contains(&tld.len()) || !tld.chars().all(|c| c.is_ascii_lowercase()) {
        return false;
    }
    s.chars()
        .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_'))
}

/// macOS's own ids: never orphans.
pub fn is_system_id(id: &str) -> bool {
    let l = id.to_lowercase();
    l.starts_with("com.apple.")
        || l.starts_with("group.com.apple.")
        || l.starts_with("systemgroup.")
}

/// Newest modification time in a folder (its top two levels), or of a file.
fn newest_mtime(p: &Path) -> Option<i64> {
    fn mtime(p: &Path) -> Option<i64> {
        let m = p.symlink_metadata().ok()?.modified().ok()?;
        Some(m.duration_since(std::time::UNIX_EPOCH).ok()?.as_secs() as i64)
    }
    let mut best = mtime(p);
    if p.is_dir() {
        for e in std::fs::read_dir(p)
            .into_iter()
            .flatten()
            .flatten()
            .take(500)
        {
            best = best.max(mtime(&e.path()));
            if e.path().is_dir() {
                for f in std::fs::read_dir(e.path())
                    .into_iter()
                    .flatten()
                    .flatten()
                    .take(200)
                {
                    best = best.max(mtime(&f.path()));
                }
            }
        }
    }
    best
}

/// (path, file name) of each entry of `dir`, hidden ones skipped.
fn list(dir: &Path) -> Vec<(PathBuf, String)> {
    let Ok(rd) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut out: Vec<(PathBuf, String)> = rd
        .flatten()
        .map(|e| (e.path(), e.file_name().to_string_lossy().into_owned()))
        .filter(|(_, n)| !n.starts_with('.'))
        .collect();
    out.sort();
    out
}

#[cfg(test)]
pub mod tests {
    use super::*;

    pub struct Fake {
        pub root: PathBuf,
        pub roots: Roots,
    }

    impl Fake {
        pub fn new(tag: &str) -> Fake {
            let root =
                std::env::temp_dir().join(format!("cmb-uninst-{tag}-{}", std::process::id()));
            let _ = std::fs::remove_dir_all(&root);
            let roots = Roots {
                user_library: root.join("home/Library"),
                system: root.join("sys"),
            };
            std::fs::create_dir_all(&roots.user_library).unwrap();
            Fake { root, roots }
        }
        pub fn file(&self, rel: &str, body: &str) -> PathBuf {
            let p = self.root.join(rel);
            std::fs::create_dir_all(p.parent().unwrap()).unwrap();
            std::fs::write(&p, body).unwrap();
            p
        }
        pub fn lib(&self, rel: &str) -> PathBuf {
            self.file(&format!("home/Library/{rel}"), "x")
        }
    }

    impl Drop for Fake {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.root);
        }
    }

    fn owner(id: &str, name: &str, path: &str) -> Owner {
        Owner {
            id: id.into(),
            name: name.into(),
            path: PathBuf::from(path),
        }
    }

    fn rels(items: &[Item], f: &Fake) -> Vec<String> {
        items
            .iter()
            .map(|i| i.path.strip_prefix(&f.root).unwrap().display().to_string())
            .collect()
    }

    #[test]
    fn ids_from_names() {
        assert_eq!(
            id_of("com.docker.docker.plist").as_deref(),
            Some("com.docker.docker")
        );
        assert_eq!(
            id_of("com.docker.docker.savedState").as_deref(),
            Some("com.docker.docker")
        );
        assert_eq!(
            id_of("com.foo.bar.0A1B2C3D-1111-2222-3333-444455556666.plist").as_deref(),
            Some("com.foo.bar")
        );
        assert_eq!(id_of("Slack"), None);
        assert_eq!(id_of("com.foo"), None);
        assert!(!looks_like_bundle_id("Com.Foo.bar"));
        assert!(looks_like_bundle_id("us.zoom.xos"));
        assert!(!looks_like_bundle_id("com..x"));
        assert!(is_system_id("com.apple.Safari"));
    }

    #[test]
    fn finds_an_apps_files_and_nothing_else() {
        let f = Fake::new("match");
        for rel in [
            "Application Support/Slack/x",
            "Application Support/com.tinyspeck.slackmacgap/x",
            "Caches/com.tinyspeck.slackmacgap/x",
            "Caches/com.tinyspeck.slackmacgap.ShipIt/x",
            "Preferences/com.tinyspeck.slackmacgap.plist",
            "Preferences/ByHost/com.tinyspeck.slackmacgap.0A1B2C3D-1111-2222-3333-444455556666.plist",
            "Containers/com.tinyspeck.slackmacgap/x",
            "Group Containers/BQR82RBBHL.com.tinyspeck.slackmacgap/x",
            "Saved Application State/com.tinyspeck.slackmacgap.savedState/x",
            "HTTPStorages/com.tinyspeck.slackmacgap/x",
            "Logs/Slack/x",
            // not Slack's
            "Caches/com.tinyspeck.slackmacgapX/x",
            "Caches/com.other.app/x",
            "Preferences/Slack/x",
            "Application Support/Google/x",
        ] {
            f.lib(rel);
        }
        f.file("home/Library/LaunchAgents/com.tinyspeck.updater.plist", "<plist><dict><key>Label</key><string>com.tinyspeck.updater</string><key>ProgramArguments</key><array><string>/Applications/Slack.app/Contents/Helper</string></array></dict></plist>");
        f.file("sys/Library/LaunchDaemons/com.tinyspeck.slackmacgap.helper.plist", "<plist><dict><key>Label</key><string>com.tinyspeck.slackmacgap.helper</string></dict></plist>");
        f.file(
            "sys/Library/LaunchDaemons/com.other.plist",
            "<plist><dict><key>Label</key><string>com.other.daemon</string></dict></plist>",
        );
        let idx = Index::build(&f.roots);
        let slack = owner(
            "com.tinyspeck.slackmacgap",
            "Slack",
            "/Applications/Slack.app",
        );
        let all = vec![
            slack.clone(),
            owner("com.other.app", "Other", "/Applications/Other.app"),
        ];
        let got = idx.for_app(&slack, &all);
        let r = rels(&got, &f);
        let want = [
            "home/Library/Application Support/Slack",
            "home/Library/Application Support/com.tinyspeck.slackmacgap",
            "home/Library/Caches/com.tinyspeck.slackmacgap",
            "home/Library/Caches/com.tinyspeck.slackmacgap.ShipIt",
            "home/Library/Preferences/com.tinyspeck.slackmacgap.plist",
            "home/Library/Preferences/ByHost/com.tinyspeck.slackmacgap.0A1B2C3D-1111-2222-3333-444455556666.plist",
            "home/Library/Containers/com.tinyspeck.slackmacgap",
            "home/Library/Group Containers/BQR82RBBHL.com.tinyspeck.slackmacgap",
            "home/Library/Saved Application State/com.tinyspeck.slackmacgap.savedState",
            "home/Library/HTTPStorages/com.tinyspeck.slackmacgap",
            "home/Library/Logs/Slack",
            "home/Library/LaunchAgents/com.tinyspeck.updater.plist",
            "sys/Library/LaunchDaemons/com.tinyspeck.slackmacgap.helper.plist",
        ];
        for w in want {
            assert!(r.iter().any(|x| x == w), "missing {w} in {r:#?}");
        }
        assert_eq!(r.len(), want.len(), "{r:#?}");
        let admin: Vec<_> = got.iter().filter(|i| i.admin).collect();
        assert_eq!(admin.len(), 1);
        assert!(got.iter().all(|i| i.selected));
    }

    #[test]
    fn shared_names_and_vendor_groups_are_careful() {
        let f = Fake::new("shared");
        f.lib("Application Support/Notes/x");
        f.lib("Group Containers/group.com.docker/x");
        f.lib("Group Containers/group.com.jetbrains/x");
        let idx = Index::build(&f.roots);
        // two installed apps are both called "Notes": the folder is no one's
        let a = owner("com.a.notes", "Notes", "/Applications/Notes.app");
        let b = owner("com.b.notes", "Notes", "/Users/x/Applications/Notes.app");
        assert!(idx.for_app(&a, &[a.clone(), b.clone()]).is_empty());
        // the vendor group goes with the vendor's only app
        let docker = owner("com.docker.docker", "Docker", "/Applications/Docker.app");
        assert_eq!(idx.for_app(&docker, std::slice::from_ref(&docker)).len(), 1);
        // two apps of one vendor share it: keep
        let idea = owner(
            "com.jetbrains.intellij",
            "IntelliJ IDEA",
            "/Applications/IntelliJ IDEA.app",
        );
        let pyc = owner(
            "com.jetbrains.pycharm",
            "PyCharm",
            "/Applications/PyCharm.app",
        );
        assert!(idx.for_app(&idea, &[idea.clone(), pyc]).is_empty());
    }

    #[test]
    fn orphans_skip_known_recent_and_system() {
        let f = Fake::new("orphans");
        for rel in [
            "Caches/com.gone.app/x",
            "Preferences/com.gone.app.plist",
            "Application Support/com.gone.app/x",
            "Caches/com.installed.app/x",
            "Caches/com.installed.app.helper/x",
            "Caches/com.apple.Safari/x",
            "Caches/Homebrew/x",
            "Group Containers/ABC.com.gone.app/x",
        ] {
            f.lib(rel);
        }
        f.file("home/Library/LaunchAgents/com.gone.agent.plist", "<plist><dict><key>Label</key><string>com.gone.agent</string><key>Program</key><string>/nonexistent/gone/agent</string></dict></plist>");
        f.file("home/Library/LaunchAgents/com.alive.agent.plist", "<plist><dict><key>Label</key><string>com.alive.agent</string><key>Program</key><string>/bin/sh</string></dict></plist>");
        let idx = Index::build(&f.roots);
        let known: HashSet<String> = ["com.installed.app".to_string()].into();
        let now = chrono::Utc::now().timestamp();
        // everything was just written: too fresh
        assert!(idx.orphans(&known, now, 7 * 86400).is_empty());
        let later = now + 30 * 86400;
        let o = idx.orphans(&known, later, 7 * 86400);
        let keys: Vec<&String> = o.keys().collect();
        assert_eq!(keys, vec!["com.gone.agent", "com.gone.app"]);
        assert_eq!(o["com.gone.app"].len(), 3);
    }
}
