//! The installed apps: where they are, what they are called, where they came
//! from, when they were last opened, and which ones must never be removed.

use crate::kit::plist;
use std::path::{Path, PathBuf};

/// Where an app came from (a filter on the Apps page).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Source {
    Direct,
    AppStore,
    Homebrew,
    Setapp,
    Apple,
}

impl Source {
    pub fn tag(self) -> Option<&'static str> {
        match self {
            Source::Direct => None,
            Source::AppStore => Some("App Store"),
            Source::Homebrew => Some("Homebrew"),
            Source::Setapp => Some("Setapp"),
            Source::Apple => Some("Apple"),
        }
    }
}

/// What a bundle's Info.plist says about it.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Info {
    pub id: String,
    pub name: String,
    pub version: String,
}

/// Read an app bundle's Info.plist. The display name falls back to the
/// bundle's file name, which is what Finder shows.
pub fn read_info(app: &Path) -> Option<Info> {
    let v = plist::read(&app.join("Contents/Info.plist"))?;
    let id = v.get_str("CFBundleIdentifier")?.trim().to_string();
    if id.is_empty() {
        return None;
    }
    let file_name = app
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default();
    let name = if file_name.is_empty() {
        v.get_str("CFBundleDisplayName")
            .or(v.get_str("CFBundleName"))
            .unwrap_or("")
            .to_string()
    } else {
        file_name
    };
    let version = v
        .get_str("CFBundleShortVersionString")
        .or(v.get_str("CFBundleVersion"))
        .unwrap_or("")
        .trim()
        .to_string();
    Some(Info { id, name, version })
}

/// The folders apps are installed in.
pub fn app_roots() -> Vec<PathBuf> {
    vec![
        PathBuf::from("/Applications"),
        crate::kit::fs::home().join("Applications"),
    ]
}

/// Every `.app` in `roots`, and one folder level down ("/Applications/Utilities",
/// "/Applications/Setapp", vendor folders). Sorted, no duplicates.
pub fn find_bundles(roots: &[PathBuf]) -> Vec<PathBuf> {
    let mut out = Vec::new();
    for root in roots {
        let Ok(rd) = std::fs::read_dir(root) else {
            continue;
        };
        for e in rd.flatten() {
            let p = e.path();
            let name = e.file_name().to_string_lossy().into_owned();
            if name.starts_with('.') {
                continue;
            }
            if name.ends_with(".app") {
                out.push(p);
            } else if p.is_dir() && !p.is_symlink() {
                for f in std::fs::read_dir(&p).into_iter().flatten().flatten() {
                    let n = f.file_name().to_string_lossy().into_owned();
                    if n.ends_with(".app") && !n.starts_with('.') {
                        out.push(f.path());
                    }
                }
            }
        }
    }
    out.sort();
    out.dedup();
    out
}

/// App names Homebrew installed as casks ("Zoom.app").
pub fn cask_apps() -> Vec<String> {
    let mut out = Vec::new();
    for root in ["/opt/homebrew/Caskroom", "/usr/local/Caskroom"] {
        for cask in std::fs::read_dir(root).into_iter().flatten().flatten() {
            for ver in std::fs::read_dir(cask.path())
                .into_iter()
                .flatten()
                .flatten()
            {
                for f in std::fs::read_dir(ver.path())
                    .into_iter()
                    .flatten()
                    .flatten()
                {
                    let n = f.file_name().to_string_lossy().into_owned();
                    if n.ends_with(".app") {
                        out.push(n);
                    }
                }
            }
        }
    }
    out
}

/// Where the app came from.
pub fn source(app: &Path, id: &str, casks: &[String]) -> Source {
    let file = app
        .file_name()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default();
    if app.join("Contents/_MASReceipt/receipt").exists() {
        Source::AppStore
    } else if app.components().any(|c| c.as_os_str() == "Setapp")
        || app.join("Contents/Frameworks/Setapp.framework").exists()
        || id.ends_with("-setapp")
    {
        Source::Setapp
    } else if casks.contains(&file) {
        Source::Homebrew
    } else if id.starts_with("com.apple.") {
        Source::Apple
    } else {
        Source::Direct
    }
}

/// Apple's apps that are fine to remove even without an App Store receipt.
const APPLE_REMOVABLE: &[&str] = &[
    "com.apple.dt.Xcode",
    "com.apple.dt.Instruments",
    "com.apple.iWork.Pages",
    "com.apple.iWork.Keynote",
    "com.apple.iWork.Numbers",
    "com.apple.iMovieApp",
    "com.apple.garageband10",
];

/// Our own bundle ids (release and dev build).
const OWN_ID: &str = "com.romanfurman6.cleanmebro";

/// Why `app` may never be removed, if so.
pub fn locked(app: &Path, id: &str, source: Source) -> Option<&'static str> {
    if id == OWN_ID
        || id.starts_with(&format!("{OWN_ID}."))
        || own_bundle().is_some_and(|b| b == app)
    {
        return Some("This is CleanMeBro");
    }
    if source == Source::Apple && !APPLE_REMOVABLE.contains(&id) {
        return Some("Part of macOS");
    }
    if app.starts_with("/System") {
        return Some("Part of macOS");
    }
    None
}

/// The .app we are running from, if any.
fn own_bundle() -> Option<PathBuf> {
    let exe = std::env::current_exe().ok()?;
    exe.ancestors()
        .find(|a| a.extension().is_some_and(|e| e == "app"))
        .map(Path::to_path_buf)
}

/// When each app was last opened, from Spotlight (one `mdls` for all).
/// Unix seconds; None where Spotlight doesn't know.
#[cfg(target_os = "macos")]
pub fn last_used(apps: &[PathBuf]) -> Vec<Option<i64>> {
    let mut out = Vec::with_capacity(apps.len());
    for chunk in apps.chunks(200) {
        let res = std::process::Command::new("/usr/bin/mdls")
            .args(["-raw", "-name", "kMDItemLastUsedDate"])
            .args(chunk)
            .output();
        let vals: Vec<Option<i64>> = match res {
            Ok(o) => String::from_utf8_lossy(&o.stdout)
                .split('\0')
                .map(parse_mdls_date)
                .collect(),
            Err(_) => Vec::new(),
        };
        for k in 0..chunk.len() {
            out.push(vals.get(k).copied().flatten());
        }
    }
    out
}

#[cfg(not(target_os = "macos"))]
pub fn last_used(apps: &[PathBuf]) -> Vec<Option<i64>> {
    vec![None; apps.len()]
}

/// "2026-11-02 09:41:07 +0000" → unix seconds; "(null)" → None.
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
pub fn parse_mdls_date(s: &str) -> Option<i64> {
    let s = s.trim();
    chrono::DateTime::parse_from_str(s, "%Y-%m-%d %H:%M:%S %z")
        .ok()
        .map(|d| d.timestamp())
}

/// Every bundle id Spotlight knows an app for, wherever it is installed (used
/// so data of an app that lives in ~/Downloads isn't called an orphan).
#[cfg(target_os = "macos")]
pub fn all_installed_ids() -> Vec<String> {
    let Ok(o) = std::process::Command::new("/usr/bin/mdfind")
        .arg("kMDItemContentType == 'com.apple.application-bundle'")
        .output()
    else {
        return Vec::new();
    };
    let paths: Vec<PathBuf> = String::from_utf8_lossy(&o.stdout)
        .lines()
        .map(PathBuf::from)
        .collect();
    let mut out = Vec::new();
    for chunk in paths.chunks(300) {
        let Ok(o) = std::process::Command::new("/usr/bin/mdls")
            .args(["-raw", "-name", "kMDItemCFBundleIdentifier"])
            .args(chunk)
            .output()
        else {
            continue;
        };
        out.extend(
            String::from_utf8_lossy(&o.stdout)
                .split('\0')
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty() && s != "(null)"),
        );
    }
    out
}

#[cfg(not(target_os = "macos"))]
pub fn all_installed_ids() -> Vec<String> {
    Vec::new()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bundle(root: &Path, rel: &str, id: &str, extra: &str) -> PathBuf {
        let app = root.join(rel);
        std::fs::create_dir_all(app.join("Contents")).unwrap();
        std::fs::write(
            app.join("Contents/Info.plist"),
            format!("<plist><dict><key>CFBundleIdentifier</key><string>{id}</string><key>CFBundleName</key><string>Inner</string><key>CFBundleShortVersionString</key><string>1.2</string>{extra}</dict></plist>"),
        )
        .unwrap();
        app
    }

    #[test]
    fn finds_bundles_one_level_down() {
        let root = std::env::temp_dir().join(format!("cmb-apps-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let a = bundle(&root, "Docker.app", "com.docker.docker", "");
        let b = bundle(&root, "Utilities/Tool.app", "com.x.tool", "");
        bundle(&root, "Deep/er/No.app", "com.x.no", "");
        std::fs::create_dir_all(root.join(".Hidden.app")).unwrap();
        let found = find_bundles(&[root.clone(), root.join("missing")]);
        assert_eq!(found, vec![a.clone(), b]);
        let info = read_info(&a).unwrap();
        assert_eq!(
            info,
            Info {
                id: "com.docker.docker".into(),
                name: "Docker".into(),
                version: "1.2".into()
            }
        );
        assert!(read_info(&root.join("Deep")).is_none());
        std::fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn sources_and_locks() {
        let root = std::env::temp_dir().join(format!("cmb-src-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let mas = bundle(&root, "Pages.app", "com.apple.iWork.Pages", "");
        std::fs::create_dir_all(mas.join("Contents/_MASReceipt")).unwrap();
        std::fs::write(mas.join("Contents/_MASReceipt/receipt"), "r").unwrap();
        let zoom = bundle(&root, "zoom.us.app", "us.zoom.xos", "");
        let setapp = bundle(
            &root,
            "Setapp/CleanShot X.app",
            "pl.maketheweb.cleanshotx-setapp",
            "",
        );
        let safari = bundle(&root, "Safari.app", "com.apple.Safari", "");
        let casks = vec!["zoom.us.app".to_string()];
        assert_eq!(
            source(&mas, "com.apple.iWork.Pages", &casks),
            Source::AppStore
        );
        assert_eq!(source(&zoom, "us.zoom.xos", &casks), Source::Homebrew);
        assert_eq!(
            source(&setapp, "pl.maketheweb.cleanshotx-setapp", &casks),
            Source::Setapp
        );
        assert_eq!(source(&safari, "com.apple.Safari", &casks), Source::Apple);
        assert_eq!(
            locked(&safari, "com.apple.Safari", Source::Apple),
            Some("Part of macOS")
        );
        assert_eq!(
            locked(&mas, "com.apple.iWork.Pages", Source::AppStore),
            None
        );
        assert_eq!(
            locked(
                Path::new("/Applications/Xcode.app"),
                "com.apple.dt.Xcode",
                Source::Apple
            ),
            None
        );
        assert_eq!(
            locked(
                Path::new("/Applications/CleanMeBro Dev.app"),
                "com.romanfurman6.cleanmebro.dev",
                Source::Direct
            ),
            Some("This is CleanMeBro")
        );
        assert_eq!(locked(&zoom, "us.zoom.xos", Source::Homebrew), None);
        std::fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn mdls_dates() {
        assert_eq!(
            parse_mdls_date("2026-11-02 09:41:07 +0000"),
            Some(1793612467)
        );
        assert_eq!(parse_mdls_date("(null)"), None);
    }
}
