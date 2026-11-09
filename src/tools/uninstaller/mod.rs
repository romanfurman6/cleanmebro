//! Uninstaller: removes apps together with the files they leave in
//! ~/Library, cleans up after apps that are already gone, resets an app to a
//! fresh start, and turns off what starts with the Mac. Everything goes to the
//! Trash and Undo puts it back.
//!
//! Drop an app on its page or its widget, or just start dragging an app out of
//! /Applications: a small drop zone appears at the top of the screen.
//!
//! Page routes: "" (Apps), "leftovers", "login", "app:<path>" (select an app).

mod apps;
mod leftovers;
mod login;
mod mac;
mod plist;
mod remove;
mod ui;

use crate::core::tool::{Badge, Cx, Size, Tool, ToolEnv, ToolId, View, WidgetSpec};
use crate::kit::fs::dir_size;
use crate::kit::glass::Icon;
use apps::Source;
use egui::{Color32, Rect, Ui};
use leftovers::{Index, Item, Kind, Owner, Roots};
use remove::{Report, Target};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

pub const ID: ToolId = "uninstaller";
const DROP: &str = "drop";
pub const TINT: Color32 = Color32::from_rgb(0x3A, 0x8A, 0x3F);

/// Apps unused this long count as unused.
pub const UNUSED_DAYS: i64 = 90;
/// "Large" filter.
pub const LARGE: u64 = 1_000_000_000;
/// Leftovers touched more recently than this are still in use.
const ORPHAN_MIN_AGE: i64 = 7 * 86400;
/// How often the app folders are checked for apps that came or went.
const WATCH_EVERY: Duration = Duration::from_secs(8);
/// How long Undo stays offered.
pub const UNDO_FOR: Duration = Duration::from_secs(60);

/// An installed app and everything found for it.
#[derive(Clone, Debug)]
pub struct AppRow {
    pub path: PathBuf,
    pub name: String,
    pub id: String,
    pub version: String,
    pub source: Source,
    pub locked: Option<&'static str>,
    pub last_used: Option<i64>,
    /// The bundle first, then its files.
    pub items: Vec<Item>,
    /// Sizes are known.
    pub sized: bool,
}

impl AppRow {
    pub fn total(&self) -> u64 {
        self.items.iter().map(|i| i.bytes).sum()
    }
    pub fn app_bytes(&self) -> u64 {
        self.items.iter().filter(|i| i.kind == Kind::App).map(|i| i.bytes).sum()
    }
    pub fn owner(&self) -> Owner {
        Owner { id: self.id.clone(), name: self.name.clone(), path: self.path.clone() }
    }
    pub fn target(&self, reset: bool) -> Target {
        Target { name: self.name.clone(), id: self.id.clone(), app: Some(self.path.clone()), items: self.items.clone(), reset }
    }
}

/// Data of an app that's gone.
#[derive(Clone, Debug)]
pub struct Orphan {
    pub id: String,
    pub name: String,
    pub items: Vec<Item>,
    /// Newest change among its items.
    pub touched: Option<i64>,
    pub selected: bool,
}

impl Orphan {
    pub fn bytes(&self) -> u64 {
        self.items.iter().map(|i| i.bytes).sum()
    }
    pub fn target(&self) -> Target {
        Target { name: self.name.clone(), id: self.id.clone(), app: None, items: self.items.clone(), reset: false }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Default)]
pub enum Phase {
    #[default]
    Idle,
    /// Reading the app folders.
    Listing,
    /// Apps are listed; sizes and files come in.
    Sizing { done: usize, total: usize },
    Ready,
}

/// An app that left /Applications while CleanMeBro watched, with what it
/// left behind.
#[derive(Clone, Debug)]
pub struct Notice {
    pub target: Target,
    pub at: Instant,
}

/// What the drop zone and the widget show.
#[derive(Clone, Debug, Default)]
pub enum Drop {
    #[default]
    Idle,
    /// Finding the files of a dropped app.
    Reading(PathBuf),
    Confirm(Target),
    /// It can't be removed (with why).
    Refused(String, String),
}

/// What's dragged on screen right now, seen through the drag pasteboard.
#[derive(Clone, Debug, Default)]
pub struct Drag {
    /// Apps being dragged while the mouse is down.
    pub apps: Vec<PathBuf>,
    /// Keep showing the drop zone until then (drop can land just after release).
    pub until: Option<Instant>,
}

#[derive(Default)]
pub struct LoginState {
    pub loaded: bool,
    pub apps: Vec<login::LoginApp>,
    pub apps_error: Option<String>,
    pub jobs: Vec<login::Job>,
}

#[derive(Default)]
pub struct State {
    pub phase: Phase,
    pub apps: Vec<AppRow>,
    pub orphans: Vec<Orphan>,
    /// A removal is running: its progress line.
    pub working: Option<String>,
    /// The last removal, for Undo and the summary.
    pub last: Option<(Report, Instant)>,
    pub notice: Option<Notice>,
    pub drop: Drop,
    pub drag: Drag,
    pub login: LoginState,
    pub settings: Settings,
    /// Rescan when the current scan ends.
    pub again: bool,
    /// Bundle ids we removed ourselves (the watcher stays quiet about those).
    pub removed_by_us: HashSet<String>,
    /// The tool is off in Settings: the background watchers idle.
    pub off: bool,
    /// The page was shown. Until then scans only list the apps (enough to
    /// notice one being thrown away) and leave measuring them for later.
    pub page_seen: bool,
}


#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    /// Offer to remove what an app left when it is thrown away elsewhere.
    pub watch_removed: bool,
    /// Show the drop zone while an app is dragged.
    pub drop_zone: bool,
    /// Names of apps seen installed, by bundle id (names for leftovers).
    pub known: BTreeMap<String, String>,
    /// Login apps turned off here (so they can be turned back on).
    pub login_off: Vec<(String, PathBuf, bool)>,
}

impl Default for Settings {
    fn default() -> Self {
        Settings { watch_removed: true, drop_zone: true, known: BTreeMap::new(), login_off: Vec::new() }
    }
}

fn settings_path() -> PathBuf {
    crate::kit::fs::data_path("uninstaller").join("settings.json")
}

impl Settings {
    fn load() -> Settings {
        crate::kit::fs::load_json(&settings_path())
    }
    pub fn save(&self) {
        crate::kit::fs::save_json(&settings_path(), self, true);
    }
}

pub type Shared = Arc<Mutex<State>>;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tab {
    Apps,
    Leftovers,
    Login,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Filter {
    All,
    Unused,
    Large,
    Source(Source),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Sort {
    Size,
    Name,
    LastUsed,
}

/// The confirm sheet before anything moves.
pub struct Review {
    pub targets: Vec<Target>,
    pub open: HashSet<usize>,
}

pub struct Uninstaller {
    pub st: Shared,
    pub roots: Roots,
    pub app_roots: Vec<PathBuf>,
    pub demo: bool,
    ctx: egui::Context,
    pub tab: Tab,
    pub search: String,
    pub filter: Filter,
    pub sort: Sort,
    pub selected: Option<PathBuf>,
    /// Apps ticked for a batch.
    pub checked: Vec<PathBuf>,
    pub review: Option<Review>,
    pub icons: HashMap<PathBuf, Option<egui::TextureHandle>>,
    /// Demo clock (screenshots).
    pub demo_now: Option<i64>,
    /// Where the current drop/confirm came from, so only that place shows it.
    pub origin: Origin,
    /// Bundle ids of open apps, refreshed at most every 2 s.
    pub running: Option<(Vec<String>, Instant)>,
    pub login_requested: bool,
    /// The widget asked for the window page.
    pub want_page: bool,
    /// The drop zone keeps showing the result until then.
    pub done_until: Option<Instant>,
    on: bool,
}

/// Which surface started a removal.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Origin {
    Zone,
    Widget,
    #[default]
    Page,
}

pub fn now() -> i64 {
    chrono::Utc::now().timestamp()
}

impl Uninstaller {
    pub fn new(env: &ToolEnv) -> Self {
        let mut u = Uninstaller {
            st: Arc::new(Mutex::new(State::default())),
            roots: Roots::real(),
            app_roots: apps::app_roots(),
            demo: env.demo,
            ctx: env.ctx.clone(),
            tab: Tab::Apps,
            search: String::new(),
            filter: Filter::All,
            sort: Sort::Size,
            selected: None,
            checked: Vec::new(),
            review: None,
            icons: HashMap::new(),
            demo_now: None,
            origin: Origin::Page,
            running: None,
            login_requested: false,
            want_page: false,
            done_until: None,
            on: true,
        };
        if env.demo {
            ui::demo::fill(&mut u, "uninstaller");
            return u;
        }
        u.st.lock().unwrap().settings = Settings::load();
        watch(u.st.clone(), u.app_roots.clone(), u.roots.clone(), env.ctx.clone());
        watch_drag(u.st.clone(), u.app_roots.clone(), env.ctx.clone());
        u
    }

    pub fn now(&self) -> i64 {
        self.demo_now.unwrap_or_else(now)
    }

    /// Start a full scan (unless one runs; then once more after it).
    pub fn rescan(&self) {
        if self.demo {
            return;
        }
        start_scan(self.st.clone(), self.app_roots.clone(), self.roots.clone(), self.ctx.clone());
    }

    /// Move `targets` to the Trash in the background.
    pub fn run(&self, targets: Vec<Target>) {
        if targets.is_empty() {
            return;
        }
        let mut g = self.st.lock().unwrap();
        if g.working.is_some() {
            return;
        }
        if self.demo {
            let moved = targets.iter().flat_map(|t| t.items.iter().filter(|i| i.selected)).map(|i| remove::Moved { from: i.path.clone(), to: i.path.clone(), admin: false, bytes: i.bytes }).collect();
            g.last = Some((Report { moved, names: targets.iter().map(|t| t.name.clone()).collect(), reset: targets.iter().all(|t| t.reset), failed: vec![] }, Instant::now()));
            return;
        }
        g.working = Some("Starting…".into());
        for t in targets.iter().filter(|t| !t.reset) {
            g.removed_by_us.insert(t.id.clone());
        }
        drop(g);
        let (st, roots, app_roots, ctx) = (self.st.clone(), self.roots.clone(), self.app_roots.clone(), self.ctx.clone());
        std::thread::Builder::new()
            .name("uninstaller-remove".into())
            .spawn(move || {
                let st2 = st.clone();
                let c2 = ctx.clone();
                let rep = remove::run(&targets, &roots, &app_roots, move |line| {
                    st2.lock().unwrap().working = Some(line);
                    c2.request_repaint();
                });
                {
                    let mut g = st.lock().unwrap();
                    g.working = None;
                    g.last = Some((rep, Instant::now()));
                    g.drop = Drop::Idle;
                }
                ctx.request_repaint();
                start_scan(st, app_roots, roots, ctx);
            })
            .ok();
    }

    /// Put the last removal back.
    pub fn undo(&self) {
        let Some((rep, _)) = self.st.lock().unwrap().last.take() else { return };
        if self.demo {
            return;
        }
        let (st, roots, app_roots, ctx) = (self.st.clone(), self.roots.clone(), self.app_roots.clone(), self.ctx.clone());
        st.lock().unwrap().working = Some("Putting things back…".into());
        std::thread::Builder::new()
            .name("uninstaller-undo".into())
            .spawn(move || {
                let (n, failed) = remove::undo(&rep);
                {
                    let mut g = st.lock().unwrap();
                    g.working = None;
                    if !failed.is_empty() {
                        let mut r = Report { names: vec![format!("{n} put back")], ..Default::default() };
                        r.failed = failed;
                        g.last = Some((r, Instant::now()));
                    }
                }
                ctx.request_repaint();
                start_scan(st, app_roots, roots, ctx);
            })
            .ok();
    }

    /// An app was dropped (page, widget or drop zone): find its files, then confirm.
    pub fn dropped(&self, path: PathBuf) {
        let path = path.ancestors().find(|a| a.extension().is_some_and(|e| e == "app")).map(Path::to_path_buf).unwrap_or(path);
        let mut g = self.st.lock().unwrap();
        if !path.extension().is_some_and(|e| e == "app") {
            g.drop = Drop::Refused(path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default(), "Not an app. Drop an app from Applications.".into());
            return;
        }
        if let Some(row) = g.apps.iter().find(|a| a.path == path && a.sized) {
            g.drop = match row.locked {
                Some(why) => Drop::Refused(row.name.clone(), format!("{why}, so it can't be removed.")),
                None => Drop::Confirm(row.target(false)),
            };
            return;
        }
        g.drop = Drop::Reading(path.clone());
        drop(g);
        let (st, roots, app_roots, ctx) = (self.st.clone(), self.roots.clone(), self.app_roots.clone(), self.ctx.clone());
        std::thread::Builder::new()
            .name("uninstaller-drop".into())
            .spawn(move || {
                let casks = apps::cask_apps();
                let others: Vec<Owner> = st.lock().unwrap().apps.iter().map(|a| a.owner()).collect();
                let res = read_app(&path, &casks, &roots, &others, &app_roots);
                let mut g = st.lock().unwrap();
                if matches!(&g.drop, Drop::Reading(p) if *p == path) {
                    g.drop = match res {
                        Some(row) => match row.locked {
                            Some(why) => Drop::Refused(row.name, format!("{why}, so it can't be removed.")),
                            None => Drop::Confirm(row.target(false)),
                        },
                        None => Drop::Refused(path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default(), "Couldn't read this app.".into()),
                    };
                }
                ctx.request_repaint();
            })
            .ok();
    }

    pub fn load_login(&self) {
        if self.demo {
            return;
        }
        let (st, lib, ctx) = (self.st.clone(), self.roots.user_library.clone(), self.ctx.clone());
        std::thread::Builder::new()
            .name("uninstaller-login".into())
            .spawn(move || {
                let jobs = login::read_jobs(&lib);
                let apps = login::login_apps();
                let mut g = st.lock().unwrap();
                g.login.jobs = jobs;
                match apps {
                    Ok(mut a) => {
                        for (name, path, hidden) in &g.settings.login_off {
                            if !a.iter().any(|x| x.path == *path) {
                                a.push(login::LoginApp { name: name.clone(), path: path.clone(), hidden: *hidden, on: false });
                            }
                        }
                        g.login.apps = a;
                        g.login.apps_error = None;
                    }
                    Err(e) => g.login.apps_error = Some(e),
                }
                g.login.loaded = true;
                ctx.request_repaint();
            })
            .ok();
    }

    /// Visible apps for the current search, filter and sort.
    pub fn visible(&self) -> Vec<usize> {
        let g = self.st.lock().unwrap();
        let q = self.search.trim().to_lowercase();
        let now = self.now();
        let mut idx: Vec<usize> = (0..g.apps.len())
            .filter(|&i| {
                let a = &g.apps[i];
                (q.is_empty() || a.name.to_lowercase().contains(&q) || a.id.to_lowercase().contains(&q))
                    && match self.filter {
                        Filter::All => true,
                        Filter::Unused => a.locked.is_none() && a.last_used.is_some_and(|t| now - t > UNUSED_DAYS * 86400),
                        Filter::Large => a.total() >= LARGE,
                        Filter::Source(s) => a.source == s,
                    }
            })
            .collect();
        let apps = &g.apps;
        match self.sort {
            Sort::Size => idx.sort_by(|&a, &b| apps[b].total().cmp(&apps[a].total()).then(apps[a].name.to_lowercase().cmp(&apps[b].name.to_lowercase()))),
            Sort::Name => idx.sort_by_key(|&a| apps[a].name.to_lowercase()),
            Sort::LastUsed => idx.sort_by(|&a, &b| apps[a].last_used.unwrap_or(i64::MAX).cmp(&apps[b].last_used.unwrap_or(i64::MAX))),
        }
        // apps that can't be removed go last
        idx.sort_by_key(|&i| apps[i].locked.is_some());
        idx
    }
}

/// Read one bundle: info, files, sizes.
fn read_app(path: &Path, casks: &[String], roots: &Roots, others: &[Owner], _app_roots: &[PathBuf]) -> Option<AppRow> {
    let info = apps::read_info(path)?;
    let source = apps::source(path, &info.id, casks);
    let mut row = AppRow {
        path: path.to_path_buf(),
        name: info.name,
        id: info.id.clone(),
        version: info.version,
        source,
        locked: apps::locked(path, &info.id, source),
        last_used: apps::last_used(&[path.to_path_buf()]).into_iter().next().flatten(),
        items: Vec::new(),
        sized: false,
    };
    let index = Index::build(roots);
    let mut all = others.to_vec();
    if !all.iter().any(|o| o.path == row.path) {
        all.push(row.owner());
    }
    fill_items(&mut row, &index, &all);
    Some(row)
}

fn fill_items(row: &mut AppRow, index: &Index, all: &[Owner]) {
    let mut items = vec![Item { path: row.path.clone(), kind: Kind::App, bytes: dir_size(&row.path), admin: false, selected: true }];
    for mut it in index.for_app(&row.owner(), all) {
        it.bytes = dir_size(&it.path);
        items.push(it);
    }
    row.items = items;
    row.sized = true;
}

fn start_scan(st: Shared, app_roots: Vec<PathBuf>, roots: Roots, ctx: egui::Context) {
    {
        let mut g = st.lock().unwrap();
        if matches!(g.phase, Phase::Listing | Phase::Sizing { .. }) {
            g.again = true;
            return;
        }
        g.phase = Phase::Listing;
    }
    std::thread::Builder::new()
        .name("uninstaller-scan".into())
        .spawn(move || loop {
            let full = scan(&st, &app_roots, &roots, &ctx);
            let mut g = st.lock().unwrap();
            if g.again {
                g.again = false;
                g.phase = Phase::Listing;
                continue;
            }
            // listed only: the page measures everything when it opens
            g.phase = if full { Phase::Ready } else { Phase::Idle };
            ctx.request_repaint();
            break;
        })
        .ok();
}

/// One scan: list apps, then (once the page was seen) fill in files and
/// sizes app by app (the list shows right away and fills in), then find
/// orphans. Returns whether it went that far.
fn scan(st: &Shared, app_roots: &[PathBuf], roots: &Roots, ctx: &egui::Context) -> bool {
    let full = st.lock().unwrap().page_seen;
    let bundles = apps::find_bundles(app_roots);
    let casks = apps::cask_apps();
    let used = apps::last_used(&bundles);
    let mut rows: Vec<AppRow> = Vec::new();
    for (k, b) in bundles.iter().enumerate() {
        let Some(info) = apps::read_info(b) else { continue };
        let source = apps::source(b, &info.id, &casks);
        rows.push(AppRow { path: b.clone(), name: info.name, id: info.id.clone(), version: info.version, source, locked: apps::locked(b, &info.id, source), last_used: used[k], items: Vec::new(), sized: false });
    }
    {
        let mut g = st.lock().unwrap();
        // keep what an earlier scan knew until it is measured again
        for r in rows.iter_mut() {
            if let Some(old) = g.apps.iter().find(|a| a.path == r.path && a.id == r.id) {
                r.items = old.items.clone();
            }
        }
        for r in &rows {
            g.settings.known.insert(r.id.clone(), r.name.clone());
        }
        g.settings.save();
        g.apps = rows.clone();
        if !full {
            return false;
        }
        g.phase = Phase::Sizing { done: 0, total: rows.len() };
    }
    ctx.request_repaint();
    let index = Index::build(roots);
    let owners: Vec<Owner> = rows.iter().map(|r| r.owner()).collect();
    // small apps first so most of the list is complete quickly
    let mut order: Vec<usize> = (0..rows.len()).collect();
    order.sort_by_key(|&i| rows[i].items.first().map(|x| x.bytes).unwrap_or(0));
    for (n, i) in order.into_iter().enumerate() {
        let mut row = rows[i].clone();
        fill_items(&mut row, &index, &owners);
        let mut g = st.lock().unwrap();
        if let Some(slot) = g.apps.iter_mut().find(|a| a.path == row.path) {
            // keep the user's ticks on files
            for it in row.items.iter_mut() {
                if let Some(old) = slot.items.iter().find(|o| o.path == it.path) {
                    it.selected = old.selected;
                }
            }
            *slot = row;
        }
        g.phase = Phase::Sizing { done: n + 1, total: rows.len() };
        drop(g);
        ctx.request_repaint();
    }
    // orphans
    let mut known: HashSet<String> = rows.iter().map(|r| r.id.clone()).collect();
    known.extend(apps::all_installed_ids());
    #[cfg(target_os = "macos")]
    known.extend(crate::kit::native::running_apps().0);
    let names = st.lock().unwrap().settings.known.clone();
    let mut orphans: Vec<Orphan> = index
        .orphans(&known, now(), ORPHAN_MIN_AGE)
        .into_iter()
        .map(|(id, mut items)| {
            for it in items.iter_mut() {
                it.bytes = dir_size(&it.path);
            }
            let touched = items.iter().filter_map(|i| mtime(&i.path)).max();
            Orphan { name: names.get(&id).cloned().unwrap_or_else(|| pretty_name(&id)), id, items, touched, selected: true }
        })
        .collect();
    orphans.sort_by_key(|o| std::cmp::Reverse(o.bytes()));
    let mut g = st.lock().unwrap();
    for o in orphans.iter_mut() {
        if let Some(old) = g.orphans.iter().find(|x| x.id == o.id) {
            o.selected = old.selected;
        }
    }
    g.orphans = orphans;
    true
}

fn mtime(p: &Path) -> Option<i64> {
    let m = p.symlink_metadata().ok()?.modified().ok()?;
    Some(m.duration_since(std::time::UNIX_EPOCH).ok()?.as_secs() as i64)
}

/// "com.hnc.Discord" → "Discord", "us.zoom.xos" → "Zoom xos".
pub fn pretty_name(id: &str) -> String {
    let parts: Vec<&str> = id.split('.').collect();
    let pick = if parts.len() >= 3 { &parts[parts.len() - 1] } else { parts.last().unwrap_or(&id) };
    let mut c = pick.chars();
    match c.next() {
        Some(f) => f.to_uppercase().chain(c).collect(),
        None => id.to_string(),
    }
}

/// Watch the app folders: when an app disappears that we didn't remove,
/// offer its leftovers; when anything changes, scan again.
fn watch(st: Shared, app_roots: Vec<PathBuf>, roots: Roots, ctx: egui::Context) {
    start_scan(st.clone(), app_roots.clone(), roots.clone(), ctx.clone());
    std::thread::Builder::new()
        .name("uninstaller-watch".into())
        .spawn(move || {
            let mut last = apps::find_bundles(&app_roots);
            loop {
                std::thread::sleep(WATCH_EVERY);
                let now_b = apps::find_bundles(&app_roots);
                if now_b == last {
                    continue;
                }
                let gone: Vec<PathBuf> = last.iter().filter(|p| !now_b.contains(p)).cloned().collect();
                last = now_b;
                let (watching, rows, ours) = {
                    let g = st.lock().unwrap();
                    (g.settings.watch_removed, g.apps.clone(), g.removed_by_us.clone())
                };
                if watching {
                    for p in gone {
                        let Some(row) = rows.iter().find(|r| r.path == p) else { continue };
                        // moved elsewhere (renamed, other folder) or removed by us
                        if ours.contains(&row.id) || rows.iter().any(|r| r.id == row.id && r.path != p && r.path.exists()) {
                            continue;
                        }
                        let index = Index::build(&roots);
                        let others: Vec<Owner> = rows.iter().filter(|r| r.path != p).map(|r| r.owner()).collect();
                        let mut all = others.clone();
                        all.push(row.owner());
                        let mut items: Vec<Item> = index.for_app(&row.owner(), &all);
                        for it in items.iter_mut() {
                            it.bytes = dir_size(&it.path);
                        }
                        if items.iter().map(|i| i.bytes).sum::<u64>() < 100_000 {
                            continue;
                        }
                        st.lock().unwrap().notice = Some(Notice { target: Target { name: row.name.clone(), id: row.id.clone(), app: None, items, reset: false }, at: Instant::now() });
                        ctx.request_repaint();
                    }
                }
                start_scan(st.clone(), app_roots.clone(), roots.clone(), ctx.clone());
            }
        })
        .ok();
}

/// Notice an app being dragged out of an app folder (any drag, anywhere) by
/// watching the drag pasteboard, so the drop zone can appear.
fn watch_drag(st: Shared, app_roots: Vec<PathBuf>, ctx: egui::Context) {
    if !cfg!(target_os = "macos") {
        return;
    }
    std::thread::Builder::new()
        .name("uninstaller-drag".into())
        .spawn(move || {
            let mut last = mac::drag_change_count();
            loop {
                let on = {
                    let g = st.lock().unwrap();
                    g.settings.drop_zone && !g.off
                };
                // a drag lasts long enough for a relaxed look when the zone is off
                std::thread::sleep(Duration::from_millis(if on { 150 } else { 1000 }));
                let n = mac::drag_change_count();
                let down = mac::mouse_down();
                let mut g = st.lock().unwrap();
                if n != last && on {
                    last = n;
                    if down {
                        let apps: Vec<PathBuf> = mac::drag_files().into_iter().filter(|p| p.extension().is_some_and(|e| e == "app") && app_roots.iter().any(|r| p.starts_with(r))).collect();
                        if !apps.is_empty() {
                            g.drag = Drag { apps, until: None };
                            ctx.request_repaint();
                        }
                    }
                } else if n != last {
                    last = n;
                }
                if !g.drag.apps.is_empty() && !down && g.drag.until.is_none() {
                    // the drop arrives just after the button goes up
                    g.drag.until = Some(Instant::now() + Duration::from_millis(900));
                    ctx.request_repaint();
                }
                if g.drag.until.is_some_and(|u| Instant::now() > u) {
                    g.drag = Drag::default();
                    ctx.request_repaint();
                }
            }
        })
        .ok();
}

impl Tool for Uninstaller {
    fn id(&self) -> ToolId {
        ID
    }
    fn name(&self) -> &'static str {
        "Uninstaller"
    }
    fn badge(&self) -> Badge {
        Badge { icon: Icon::Trash, color: TINT }
    }
    fn section(&self) -> &'static str {
        "Clean"
    }
    fn about(&self) -> &'static str {
        "Remove apps with everything they leave behind"
    }
    fn sidebar_meta(&self) -> String {
        let g = self.st.lock().unwrap();
        if g.apps.is_empty() {
            String::new()
        } else {
            g.apps.len().to_string()
        }
    }
    fn widgets(&self) -> Vec<WidgetSpec> {
        vec![WidgetSpec { id: DROP, title: "Uninstall", about: "Drop an app on it to remove it with its files.", sizes: &[Size::S11, Size::S12, Size::S14], size: Size::S11, default_on: false }]
    }
    fn widget_ui(&mut self, _widget: &str, size: Size, ui: &mut Ui, rect: Rect, cx: &mut Cx) {
        ui::widget(self, size, ui, rect, cx);
    }
    fn has_page(&self) -> bool {
        true
    }
    fn open_page(&mut self, route: &str) {
        match route {
            "leftovers" => self.tab = Tab::Leftovers,
            "login" => self.tab = Tab::Login,
            r if r.starts_with("app:") => {
                self.tab = Tab::Apps;
                self.filter = Filter::All;
                self.search.clear();
                self.selected = Some(PathBuf::from(&r[4..]));
            }
            _ => self.tab = Tab::Apps,
        }
    }
    fn page_ui(&mut self, ui: &mut Ui, rect: Rect, cx: &mut Cx) {
        ui::page(self, ui, rect, cx);
    }
    fn set_on(&mut self, on: bool) {
        self.on = on;
        self.st.lock().unwrap().off = !on;
        if !on {
            let mut g = self.st.lock().unwrap();
            g.drag = Drag::default();
            g.notice = None;
        }
    }
    fn tick(&mut self, ctx: &egui::Context) -> bool {
        let g = self.st.lock().unwrap();
        let busy = g.working.is_some() || matches!(g.phase, Phase::Listing | Phase::Sizing { .. });
        if busy && crate::kit::activity::visible() {
            ctx.request_repaint_after(Duration::from_millis(250));
        }
        // our own floating windows need frames while the shell is hidden
        self.on && (!g.drag.apps.is_empty() || g.notice.is_some() || !matches!(g.drop, Drop::Idle) || g.last.as_ref().is_some_and(|(_, t)| t.elapsed() < Duration::from_secs(8)))
    }
    fn windows(&mut self, ctx: &egui::Context, cx: &mut Cx) {
        ui::windows(self, ctx, cx);
    }
    fn demo_view(&mut self, view: &str) -> Option<View> {
        ui::demo::fill(self, view)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pretty_names() {
        assert_eq!(pretty_name("com.hnc.Discord"), "Discord");
        assert_eq!(pretty_name("com.parallels.desktop.console"), "Console");
        assert_eq!(pretty_name("x"), "X");
    }

    #[test]
    fn filters_and_sorting() {
        let env = ToolEnv { ctx: egui::Context::default(), demo: true };
        let mut u = Uninstaller::new(&env);
        let all = u.visible();
        assert!(all.len() > 5);
        {
            let g = u.st.lock().unwrap();
            // biggest first, locked ones last
            let sizes: Vec<u64> = all.iter().filter(|&&i| g.apps[i].locked.is_none()).map(|&i| g.apps[i].total()).collect();
            assert!(sizes.windows(2).all(|w| w[0] >= w[1]));
            assert!(g.apps[*all.last().unwrap()].locked.is_some());
        }
        u.filter = Filter::Unused;
        let unused = u.visible();
        {
            let g = u.st.lock().unwrap();
            assert!(!unused.is_empty());
            assert!(unused.iter().all(|&i| g.apps[i].last_used.is_some_and(|t| u.now() - t > UNUSED_DAYS * 86400)));
        }
        u.filter = Filter::All;
        u.search = "dock".into();
        let found = u.visible();
        assert_eq!(found.len(), 1);
    }
}
