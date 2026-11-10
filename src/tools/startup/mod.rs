//! Startup: what starts with the Mac (apps opened at login, launchd agents
//! and daemons), when each runs and whether it runs now. Turn any of them
//! off without removing anything, or turn off every suggested one (updaters,
//! agents of removed apps, known extras) at once, with Undo. A note pops up
//! when an app adds itself to startup.
//!
//! Page routes: "" (All), "suggested", "off".

mod items;
mod ui;

use crate::core::tool::{Action, Badge, Cx, Recommendation, Size, Tool, ToolEnv, ToolId, View, WidgetSpec};
use crate::kit::glass::{tint, Icon};
use egui::{Rect, Ui};
use items::{Job, LoginApp, Scope, When};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeSet, HashSet};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

pub const ID: ToolId = "startup";
const SUMMARY: &str = "summary";
/// How long Undo stays offered after a batch.
pub const UNDO_FOR: Duration = Duration::from_secs(60);
/// How often the launchd folders are read again.
const POLL: Duration = Duration::from_secs(20);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Filter {
    All,
    Suggested,
    Off,
}

/// One thing switched by a batch, as it was before.
#[derive(Clone, Debug)]
pub enum Change {
    Job(Job),
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    /// Login apps turned off here (macOS forgets them, so we keep them to turn back on).
    pub login_off: Vec<(String, PathBuf, bool)>,
    /// Job plists seen before (a new one is announced).
    pub known: BTreeSet<PathBuf>,
    /// `known` has been filled once (the first look announces nothing).
    pub seeded: bool,
    /// Notify when an app adds itself to startup.
    pub notify_new: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Settings { login_off: Vec::new(), known: BTreeSet::new(), seeded: false, notify_new: true }
    }
}

fn data_dir() -> PathBuf {
    crate::kit::fs::data_path("startup")
}

impl Settings {
    fn load() -> Settings {
        if let Some(s) = std::fs::read_to_string(data_dir().join("settings.json")).ok().and_then(|s| serde_json::from_str(&s).ok()) {
            return s;
        }
        // first run: keep the login apps turned off in the Uninstaller's old Login items tab
        let mut s = Settings::default();
        let old = data_dir().with_file_name("uninstaller").join("settings.json");
        if let Some(v) = std::fs::read_to_string(old).ok().and_then(|t| serde_json::from_str::<serde_json::Value>(&t).ok()) {
            if let Some(off) = v.get("login_off").and_then(|o| serde_json::from_value(o.clone()).ok()) {
                s.login_off = off;
            }
        }
        s
    }
    pub fn save(&self) {
        let dir = data_dir();
        if std::fs::create_dir_all(&dir).is_ok() {
            if let Ok(s) = serde_json::to_string_pretty(self) {
                let _ = std::fs::write(dir.join("settings.json"), s);
            }
        }
    }
}

#[derive(Default)]
pub struct State {
    pub jobs: Vec<Job>,
    pub jobs_loaded: bool,
    pub apps: Vec<LoginApp>,
    pub apps_loaded: bool,
    pub apps_error: Option<String>,
    pub settings: Settings,
    /// Labels that appeared while the app runs.
    pub new_labels: HashSet<String>,
    /// The last batch, for Undo.
    pub last: Option<(Vec<Change>, Instant)>,
    /// Something is being switched: its line.
    pub busy: Option<String>,
}

impl State {
    pub fn suggested(&self) -> Vec<Job> {
        self.jobs.iter().filter(|j| j.suggested()).cloned().collect()
    }
    pub fn total(&self) -> usize {
        self.jobs.len() + self.apps.len()
    }
    pub fn running(&self) -> usize {
        self.jobs.iter().filter(|j| j.pid.is_some()).count()
    }
    /// The last batch while Undo is still offered.
    pub fn undoable(&self) -> Option<usize> {
        self.last.as_ref().filter(|(_, t)| t.elapsed() < UNDO_FOR).map(|(c, _)| c.len())
    }
}

type Shared = Arc<Mutex<State>>;

pub struct Startup {
    st: Shared,
    ctx: egui::Context,
    pub demo: bool,
    user_library: PathBuf,
    /// App icons by path (None: couldn't be read).
    pub icons: std::collections::HashMap<PathBuf, Option<egui::TextureHandle>>,
    pub filter: Filter,
    pub search: String,
    apps_requested: bool,
}

impl Startup {
    pub fn new(env: &ToolEnv) -> Self {
        let user_library = crate::kit::fs::home().join("Library");
        let st: Shared = Arc::new(Mutex::new(State::default()));
        let me = Startup { st: st.clone(), ctx: env.ctx.clone(), demo: env.demo, user_library: user_library.clone(), filter: Filter::All, search: String::new(), apps_requested: false, icons: Default::default() };
        if env.demo {
            demo(&mut st.lock().unwrap(), false);
            return me;
        }
        st.lock().unwrap().settings = Settings::load();
        let ctx = env.ctx.clone();
        std::thread::Builder::new()
            .name("startup-watch".into())
            .spawn(move || {
                crate::kit::activity::low_priority();
                watch(st, user_library, ctx)
            })
            .ok();
        me
    }

    pub fn state(&self) -> std::sync::MutexGuard<'_, State> {
        self.st.lock().unwrap()
    }

    /// Read the login apps (macOS asks once to let us), the first time the page shows.
    pub fn load_apps(&mut self) {
        if self.apps_requested || self.demo {
            return;
        }
        self.apps_requested = true;
        let (st, ctx) = (self.st.clone(), self.ctx.clone());
        std::thread::Builder::new()
            .name("startup-login".into())
            .spawn(move || {
                let apps = items::login_apps();
                let mut g = st.lock().unwrap();
                match apps {
                    Ok(mut a) => {
                        for (name, path, hidden) in &g.settings.login_off {
                            if !a.iter().any(|x| x.path == *path) {
                                a.push(LoginApp { name: name.clone(), path: path.clone(), hidden: *hidden, on: false });
                            }
                        }
                        g.apps = a;
                        g.apps_error = None;
                    }
                    Err(e) => g.apps_error = Some(e),
                }
                g.apps_loaded = true;
                ctx.request_repaint();
            })
            .ok();
    }

    /// Run `f` off the main thread (it may wait for a password), then read the jobs again.
    fn work(&self, line: String, f: impl FnOnce(&Shared) -> Result<(), String> + Send + 'static) {
        if self.state().busy.is_some() {
            return;
        }
        if self.demo {
            let _ = f(&self.st);
            return;
        }
        self.state().busy = Some(line);
        let (st, ctx, lib) = (self.st.clone(), self.ctx.clone(), self.user_library.clone());
        std::thread::Builder::new()
            .name("startup-switch".into())
            .spawn(move || {
                let res = f(&st);
                let jobs = items::read_jobs(&lib);
                let mut g = st.lock().unwrap();
                g.jobs = jobs;
                g.busy = None;
                if let Err(e) = res {
                    if e != "Cancelled" {
                        crate::kit::notify::send("Couldn't change startup items", &e);
                    }
                }
                ctx.request_repaint();
            })
            .ok();
    }

    pub fn toggle_job(&self, j: &Job) {
        let (j, want) = (j.clone(), !j.on);
        let demo = self.demo;
        self.work(
            format!("Turning {} {}…", if want { "on" } else { "off" }, j.name),
            move |st| {
                if !demo {
                    items::set_jobs(&[(j.clone(), want)])?;
                }
                if let Some(x) = st.lock().unwrap().jobs.iter_mut().find(|x| x.key() == j.key()) {
                    x.on = want;
                    if !want {
                        x.pid = None;
                    }
                }
                Ok(())
            },
        );
    }

    pub fn toggle_app(&self, a: &LoginApp) {
        let (a, want) = (a.clone(), !a.on);
        let demo = self.demo;
        self.work(
            format!("Turning {} {}…", if want { "on" } else { "off" }, a.name),
            move |st| {
                if !demo {
                    items::set_login_app(&a, want)?;
                }
                let mut g = st.lock().unwrap();
                if let Some(x) = g.apps.iter_mut().find(|x| x.path == a.path) {
                    x.on = want;
                }
                g.settings.login_off.retain(|(_, p, _)| *p != a.path);
                if !want {
                    g.settings.login_off.push((a.name.clone(), a.path.clone(), a.hidden));
                }
                if !demo {
                    g.settings.save();
                }
                Ok(())
            },
        );
    }

    /// Turn off every suggested job, with one password prompt for the admin ones.
    pub fn turn_off_suggested(&self) {
        let list = self.state().suggested();
        if list.is_empty() {
            return;
        }
        let demo = self.demo;
        self.work(
            format!("Turning off {}…", plural(list.len(), "item")),
            move |st| {
                if !demo {
                    let batch: Vec<(Job, bool)> = list.iter().map(|j| (j.clone(), false)).collect();
                    items::set_jobs(&batch)?;
                }
                let mut g = st.lock().unwrap();
                for j in &list {
                    if let Some(x) = g.jobs.iter_mut().find(|x| x.key() == j.key()) {
                        x.on = false;
                        x.pid = None;
                    }
                }
                g.last = Some((list.into_iter().map(Change::Job).collect(), Instant::now()));
                Ok(())
            },
        );
    }

    /// Put the last batch back as it was.
    pub fn undo(&self) {
        let Some((changes, _)) = self.state().last.clone() else { return };
        let demo = self.demo;
        self.work(
            "Turning them back on…".into(),
            move |st| {
                let jobs: Vec<(Job, bool)> = changes.iter().map(|Change::Job(j)| (j.clone(), j.on)).collect();
                if !demo {
                    items::set_jobs(&jobs)?;
                }
                let mut g = st.lock().unwrap();
                for (j, on) in &jobs {
                    if let Some(x) = g.jobs.iter_mut().find(|x| x.key() == j.key()) {
                        x.on = *on;
                        x.pid = j.pid;
                    }
                }
                g.last = None;
                Ok(())
            },
        );
    }

    /// Stop a job and move its file to the Trash.
    pub fn trash(&self, j: &Job) {
        let j = j.clone();
        let demo = self.demo;
        self.work(
            format!("Moving {} to the Trash…", j.name),
            move |st| {
                if !demo {
                    items::trash_job(&j)?;
                }
                st.lock().unwrap().jobs.retain(|x| x.key() != j.key());
                Ok(())
            },
        );
    }

    pub fn set_notify(&self, on: bool) {
        let mut g = self.state();
        g.settings.notify_new = on;
        if !self.demo {
            g.settings.save();
        }
    }
}

pub fn plural(n: usize, word: &str) -> String {
    if n == 1 { format!("1 {word}") } else { format!("{n} {word}s") }
}

/// Read the launchd folders now and then; announce plists that weren't there before.
fn watch(st: Shared, lib: PathBuf, ctx: egui::Context) {
    loop {
        let jobs = items::read_jobs(&lib);
        let paths: BTreeSet<PathBuf> = items::plist_paths(&lib, std::path::Path::new("/")).into_iter().collect();
        let mut announce = Vec::new();
        {
            let mut g = st.lock().unwrap();
            let fresh: Vec<PathBuf> = paths.difference(&g.settings.known).cloned().collect();
            let changed = !fresh.is_empty() || g.settings.known.len() != paths.len();
            if g.settings.seeded {
                for p in &fresh {
                    if let Some(j) = jobs.iter().find(|j| j.plist == *p) {
                        g.new_labels.insert(j.label.clone());
                        if g.settings.notify_new && j.on {
                            announce.push(j.clone());
                        }
                    }
                }
            }
            g.settings.known = paths;
            g.settings.seeded = true;
            if changed {
                g.settings.save();
            }
            if g.busy.is_none() {
                g.jobs = jobs;
            }
            g.jobs_loaded = true;
        }
        for j in announce {
            let app = j.owner_name().unwrap_or_else(|| j.name.clone());
            crate::kit::notify::send(&format!("{app} added itself to startup"), &format!("{} · runs {}. Turn it off in Cmd+Meow › Startup.", j.label, j.when.label(j.scope)));
        }
        ctx.request_repaint();
        std::thread::sleep(POLL);
    }
}

/// Sample data for screenshots. `done`: right after "Turn off 4".
pub fn demo(g: &mut State, done: bool) {
    let job = |label: &str, name: &str, prog: &str, scope: Scope, when: When, on: bool, pid: Option<u32>, orphan: bool| {
        let program = Some(PathBuf::from(prog));
        let owner = items::owner_app(std::path::Path::new(prog));
        let dir = match scope {
            Scope::User => "/Users/you/Library/LaunchAgents",
            Scope::AllUsers => "/Library/LaunchAgents",
            Scope::System => "/Library/LaunchDaemons",
        };
        Job { label: label.into(), plist: PathBuf::from(format!("{dir}/{label}.plist")), program, scope, on, when, pid, orphan, owner, name: name.into() }
    };
    g.jobs = vec![
        job("com.docker.helper", "Docker", "/Applications/Docker.app/Contents/Library/LaunchServices/com.docker.helper", Scope::User, When::KeepsRunning, true, Some(812), false),
        job("com.google.GoogleUpdater.wake", "Google Updater", "/Users/you/Library/Application Support/Google/GoogleUpdater/Current/GoogleUpdater.app/Contents/MacOS/GoogleUpdater", Scope::User, When::Every(3600), true, None, false),
        job("com.valvesoftware.steamclean", "Steam Cleaner", "/Users/you/Library/Application Support/Steam/Steam.AppBundle/Steam/Contents/MacOS/steamclean", Scope::User, When::AtLogin, true, None, false),
        job("com.teamviewer.teamviewer_desktop", "TeamViewer", "/Applications/TeamViewer.app/Contents/Helpers/TeamViewer_Desktop", Scope::User, When::KeepsRunning, true, None, true),
        job("com.1password.SafariShim", "1Password", "/Applications/1Password.app/Contents/Library/LaunchAgents/SafariShim", Scope::AllUsers, When::OnDemand, true, Some(904), false),
        job("com.microsoft.autoupdate.helper", "Microsoft AutoUpdate", "/Library/Application Support/Microsoft/MAU2.0/Microsoft AutoUpdate.app/Contents/MacOS/Microsoft AutoUpdate", Scope::System, When::OnDemand, true, None, false),
        job("com.docker.vmnetd", "Docker", "/Library/PrivilegedHelperTools/com.docker.vmnetd", Scope::System, When::KeepsRunning, true, Some(233), false),
    ];
    g.apps = vec![
        LoginApp { name: "Slack".into(), path: "/Applications/Slack.app".into(), hidden: true, on: true },
        LoginApp { name: "Spotify".into(), path: "/Applications/Spotify.app".into(), hidden: false, on: false },
    ];
    g.jobs_loaded = true;
    g.apps_loaded = true;
    g.new_labels = ["com.1password.SafariShim".to_string()].into();
    if done {
        let list = g.suggested();
        for j in &list {
            if let Some(x) = g.jobs.iter_mut().find(|x| x.key() == j.key()) {
                x.on = false;
                x.pid = None;
            }
        }
        g.last = Some((list.into_iter().map(Change::Job).collect(), Instant::now()));
    }
}

fn badge() -> Badge {
    Badge { icon: Icon::Power, color: tint::STARTUP }
}

impl Tool for Startup {
    fn id(&self) -> ToolId {
        ID
    }
    fn name(&self) -> &'static str {
        "Startup"
    }
    fn badge(&self) -> Badge {
        badge()
    }
    fn needs(&self) -> Vec<crate::kit::perms::Need> {
        use crate::kit::perms::{Need, Perm};
        vec![Need::must(Perm::SystemEvents), Need::may(Perm::Notifications)]
    }

    fn section(&self) -> &'static str {
        "Clean"
    }
    fn about(&self) -> &'static str {
        "See what starts with your Mac and turn off what you don't need"
    }
    fn sidebar_meta(&self) -> String {
        let n = self.state().suggested().len();
        if n > 0 { n.to_string() } else { String::new() }
    }
    fn widgets(&self) -> Vec<WidgetSpec> {
        vec![WidgetSpec { id: SUMMARY, title: "Startup", about: "What starts with your Mac, and one button for what can go.", sizes: &[Size::S11, Size::S12, Size::S24], size: Size::S24, default_on: true }]
    }
    fn widget_ui(&mut self, _widget: &str, size: Size, ui: &mut Ui, rect: Rect, cx: &mut Cx) {
        ui::widget(self, size, ui, rect, cx);
    }
    fn has_page(&self) -> bool {
        true
    }
    fn open_page(&mut self, route: &str) {
        self.filter = match route {
            "suggested" => Filter::Suggested,
            "off" => Filter::Off,
            _ => Filter::All,
        };
    }
    fn page_ui(&mut self, ui: &mut Ui, rect: Rect, cx: &mut Cx) {
        ui::page(self, ui, rect, cx);
    }
    fn recommendations(&self) -> Vec<Recommendation> {
        let list = self.state().suggested();
        if list.is_empty() {
            return Vec::new();
        }
        let names: Vec<String> = list.iter().take(2).map(|j| j.name.clone()).collect();
        let more = if list.len() > 2 { format!(" and {} more", list.len() - 2) } else { String::new() };
        vec![Recommendation {
            priority: 8,
            badge: badge(),
            title: format!("{} start with your Mac for nothing", plural(list.len(), "item")),
            body: format!("{}{more}. Turning them off removes nothing.", names.join(", ")),
            action: "Review".into(),
            go: Action::OpenPage { tool: ID.into(), route: "suggested".into() },
        }]
    }
    fn tick(&mut self, ctx: &egui::Context) -> bool {
        let g = self.state();
        if g.busy.is_some() {
            ctx.request_repaint_after(Duration::from_millis(120));
        } else if g.undoable().is_some() {
            ctx.request_repaint_after(Duration::from_secs(1));
        }
        false
    }
    fn demo_view(&mut self, view: &str) -> Option<View> {
        match view {
            "startup" | "startup-done" | "startup-suggested" => {
                let mut g = self.state();
                *g = State::default();
                demo(&mut g, view == "startup-done");
                drop(g);
                self.filter = if view == "startup-suggested" { Filter::Suggested } else { Filter::All };
                Some(View::Page)
            }
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use items::Why;

    #[test]
    fn demo_suggests_the_right_ones() {
        let mut g = State::default();
        demo(&mut g, false);
        let s: Vec<String> = g.suggested().iter().map(|j| j.name.clone()).collect();
        assert_eq!(s, vec!["Google Updater", "Steam Cleaner", "TeamViewer", "Microsoft AutoUpdate"]);
        assert_eq!(g.jobs.iter().find(|j| j.name == "TeamViewer").unwrap().why(), Some(Why::Removed));
        let mut d = State::default();
        demo(&mut d, true);
        assert!(d.suggested().is_empty());
        assert_eq!(d.undoable(), Some(4));
        assert_eq!(plural(1, "item"), "1 item");
    }
}
