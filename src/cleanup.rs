//! Cleanup: find junk by the original's rules (System Junk, Xcode Junk, app
//! caches, AI Clutter), then let Claude judge each PATH (never contents) as
//! safe / ask / keep. Anything not clearly safe waits for the person's OK.

use crate::claude;
use crate::mechanics as m;
use crate::monitor::dir_size;
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::{Arc, Mutex};
use std::time::SystemTime;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Group {
    UserCache,
    Logs,
    AppCaches,
    Xcode,
    Developer,
    Claude,
    AiTools,
    Trash,
}

impl Group {
    pub const ALL: [Group; 8] = [
        Group::Claude,
        Group::UserCache,
        Group::AppCaches,
        Group::Logs,
        Group::Xcode,
        Group::Developer,
        Group::AiTools,
        Group::Trash,
    ];
    pub fn title(self) -> &'static str {
        match self {
            Group::UserCache => "User Cache Files",
            Group::Logs => "Log Files",
            Group::AppCaches => "App Caches",
            Group::Xcode => "Xcode Junk",
            Group::Developer => "Developer Caches",
            Group::Claude => "Claude",
            Group::AiTools => "Other AI Tools",
            Group::Trash => "Trash Bins",
        }
    }
    pub fn blurb(self) -> &'static str {
        match self {
            Group::UserCache => "Apps keep cache files to start faster. They pile up over time and are rebuilt when needed.",
            Group::Logs => "Activity logs from apps and macOS. Useful only for debugging, especially when old.",
            Group::AppCaches => "Known caches of popular apps: Adobe media cache, Spotify, Electron and Chromium apps, sandboxed apps.",
            Group::Xcode => "Intermediate build data, device support files, simulator caches and archives created by Xcode.",
            Group::Developer => "Package manager downloads (npm, Gradle, pip, Go, Bun, CocoaPods) that are fetched again when needed.",
            Group::Claude => "Claude Code and Claude Desktop caches, logs, temp files and old versions. Projects you use are never touched.",
            Group::AiTools => "Caches and logs of other AI tools such as Codex, Gemini, Cursor and Chrome DevTools MCP.",
            Group::Trash => "Items you already deleted still take space until the Trash is emptied.",
        }
    }
    pub fn colors(self) -> (egui::Color32, egui::Color32) {
        use egui::Color32 as C;
        match self {
            Group::UserCache => (C::from_rgb(120, 200, 255), C::from_rgb(40, 110, 230)),
            Group::Logs => (C::from_rgb(190, 190, 200), C::from_rgb(110, 110, 120)),
            Group::AppCaches => (C::from_rgb(110, 240, 200), C::from_rgb(20, 160, 130)),
            Group::Xcode => (C::from_rgb(120, 190, 255), C::from_rgb(30, 90, 210)),
            Group::Developer => (C::from_rgb(255, 200, 120), C::from_rgb(230, 120, 40)),
            Group::Claude => (C::from_rgb(255, 170, 130), C::from_rgb(214, 100, 70)),
            Group::AiTools => (C::from_rgb(120, 235, 230), C::from_rgb(20, 160, 170)),
            Group::Trash => (C::from_rgb(170, 170, 178), C::from_rgb(90, 90, 98)),
        }
    }
    pub fn icon(self) -> crate::glass::Icon {
        use crate::glass::Icon;
        match self {
            Group::UserCache => Icon::Folder,
            Group::Logs => Icon::Chart,
            Group::AppCaches => Icon::Monitor,
            Group::Xcode => Icon::Cpu,
            Group::Developer => Icon::Gear,
            Group::Claude => Icon::Sparkle,
            Group::AiTools => Icon::Sparkle,
            Group::Trash => Icon::Trash,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Verdict {
    Safe,
    Ask,
    Keep,
}

impl Verdict {
    pub fn word(self) -> &'static str {
        match self {
            Verdict::Safe => "Safe",
            Verdict::Ask => "Needs your OK",
            Verdict::Keep => "Kept",
        }
    }
    fn rank(self) -> u8 {
        match self {
            Verdict::Safe => 0,
            Verdict::Ask => 1,
            Verdict::Keep => 2,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Who {
    Rule,
    Ai,
    You,
}

#[derive(Clone, Debug)]
pub struct Item {
    pub group: Group,
    pub title: String,
    pub path: PathBuf,
    pub bytes: u64,
    pub age_days: f64,
    pub kind: &'static str, // Caches, Logs, Temp Files, Stale Versions, Build Data...
    pub verdict: Verdict,
    pub reason: String,
    pub rule_reason: String,
    pub by: Who,
    pub selected: bool,
    pub hard_keep: bool,
    pub permanent: bool, // emptying the Trash can't be undone
    pub app_running: bool,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub enum Phase {
    #[default]
    Intro,
    Scanning,
    Judging,
    Results,
    Cleaning,
    Done,
}

#[derive(Clone, Debug, Default)]
pub struct State {
    pub phase: Phase,
    pub current: String,
    pub progress: f32,
    pub items: Vec<Item>,
    pub ai_status: String,
    pub ai_used: bool,
    pub ai_error: Option<String>,
    pub cleaned_bytes: u64,
    pub cleaned_count: usize,
    pub failed: Vec<(String, String)>,
    pub log: Vec<String>,
    pub stop: bool,
}

impl State {
    pub fn total(&self) -> u64 {
        self.items.iter().filter(|i| i.verdict != Verdict::Keep).map(|i| i.bytes).sum()
    }
    pub fn selected_bytes(&self) -> u64 {
        self.items.iter().filter(|i| i.selected).map(|i| i.bytes).sum()
    }
    pub fn pending_questions(&self) -> usize {
        self.items.iter().filter(|i| i.verdict == Verdict::Ask && i.by != Who::You).count()
    }
    pub fn group_bytes(&self, g: Group) -> u64 {
        self.items.iter().filter(|i| i.group == g && i.verdict != Verdict::Keep).map(|i| i.bytes).sum()
    }
    /// Claude junk safe to clean (for the panel's recommendation).
    pub fn claude_safe_bytes(&self) -> u64 {
        self.items.iter().filter(|i| i.group == Group::Claude && i.verdict == Verdict::Safe).map(|i| i.bytes).sum()
    }
}

pub type Shared = Arc<Mutex<State>>;

// ---------------------------------------------------------------- protection

fn home() -> PathBuf {
    dirs::home_dir().unwrap_or_else(|| PathBuf::from("/"))
}

/// Paths that are never offered for removal, whatever the rules or the AI say.
pub fn is_protected(p: &Path) -> Option<&'static str> {
    let h = home();
    let personal = ["Documents", "Desktop", "Pictures", "Movies", "Music", "Projects", "Developer", "Dropbox", "Library/Mobile Documents", "Library/Keychains", "Library/Messages", ".ssh", ".gnupg"];
    for d in personal {
        if p.starts_with(h.join(d)) {
            return Some("Personal files and settings are never removed");
        }
    }
    if p.starts_with(h.join("Library/Mail")) {
        return Some("Mail data is never removed");
    }
    let claude_keep = ["settings.json", "settings.local.json", "CLAUDE.md", "skills", "plugins/marketplaces", "plugins/repos", "agents", "commands", "history.jsonl", "scheduled-tasks", "sessions"];
    for d in claude_keep {
        if p.starts_with(h.join(".claude").join(d)) {
            return Some("Your Claude settings, skills and history are kept");
        }
    }
    if p.join(".git").exists() {
        return Some("Looks like a code project (has .git)");
    }
    None
}

// ---------------------------------------------------------------- scanning

type ScanStep = (&'static str, fn(&mut Ctx));

struct Ctx {
    items: Vec<Item>,
    running_ids: HashSet<String>,
    running_names: HashSet<String>,
    shared: Shared,
    egui: egui::Context,
}

impl Ctx {
    fn stopped(&self) -> bool {
        self.shared.lock().unwrap().stop
    }
    fn status(&self, s: &str, progress: f32) {
        let mut g = self.shared.lock().unwrap();
        g.current = s.to_string();
        g.progress = progress;
        self.egui.request_repaint();
    }
    #[allow(clippy::too_many_arguments)]
    fn add(&mut self, group: Group, title: &str, path: PathBuf, kind: &'static str, verdict: Verdict, reason: &str) {
        if !path.exists() {
            return;
        }
        self.status(&path.display().to_string(), -1.0);
        let bytes = dir_size(&path);
        if bytes < m::MIN_ITEM_BYTES {
            return;
        }
        let age_days = age_days(&path);
        let mut it = Item {
            group,
            title: title.to_string(),
            path: path.clone(),
            bytes,
            age_days,
            kind,
            verdict,
            reason: reason.to_string(),
            rule_reason: reason.to_string(),
            by: Who::Rule,
            selected: false,
            hard_keep: false,
            permanent: false,
            app_running: false,
        };
        if let Some(why) = is_protected(&path) {
            it.verdict = Verdict::Keep;
            it.reason = why.into();
            it.hard_keep = true;
        }
        let name = path.file_name().map(|s| s.to_string_lossy().to_lowercase()).unwrap_or_default();
        if verdict == Verdict::Safe && kind == "Caches" && (self.running_ids.contains(&name) || self.running_names.contains(&title.to_lowercase())) {
            it.verdict = Verdict::Ask;
            it.app_running = true;
            let app = if title.contains(' ') && title.len() > 18 { pretty(&name) } else { title.to_string() };
            it.reason = format!("{} is open right now — quit it first or allow anyway", if app.is_empty() { title.to_string() } else { app });
        }
        self.items.push(it);
    }
}

fn age_days(p: &Path) -> f64 {
    p.symlink_metadata()
        .ok()
        .and_then(|m| m.modified().ok())
        .and_then(|t| SystemTime::now().duration_since(t).ok())
        .map(|d| d.as_secs_f64() / 86_400.0)
        .unwrap_or(0.0)
}

fn children(p: &Path) -> Vec<PathBuf> {
    let mut v: Vec<PathBuf> = std::fs::read_dir(p).map(|rd| rd.flatten().map(|e| e.path()).collect()).unwrap_or_default();
    v.sort();
    v
}

fn fname(p: &Path) -> String {
    p.file_name().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default()
}

/// Friendly app name from a bundle id or folder name.
pub fn pretty(name: &str) -> String {
    let known = [
        ("com.spotify.client", "Spotify"),
        ("com.google.Chrome", "Google Chrome"),
        ("com.apple.Safari", "Safari"),
        ("com.microsoft.VSCode", "Visual Studio Code"),
        ("com.tinyspeck.slackmacgap", "Slack"),
        ("com.hnc.Discord", "Discord"),
        ("com.apple.dt.Xcode", "Xcode"),
        ("org.mozilla.firefox", "Firefox"),
        ("com.brave.Browser", "Brave"),
        ("us.zoom.xos", "Zoom"),
        ("com.anthropic.claudefordesktop", "Claude"),
    ];
    for (id, n) in known {
        if name.eq_ignore_ascii_case(id) {
            return n.into();
        }
    }
    if name.matches('.').count() >= 2 && !name.contains(' ') {
        let parts: Vec<&str> = name.split('.').collect();
        let generic = ["macos", "mac", "osx", "desktop", "app", "client", "helper", "agent", "main"];
        let mut last = parts[parts.len() - 1];
        if generic.contains(&last.to_lowercase().as_str()) && parts.len() >= 3 {
            last = parts[parts.len() - 2];
        }
        let s = last.replace(['-', '_'], " ");
        let mut chars = s.chars();
        return chars.next().map(|f| f.to_uppercase().chain(chars).collect()).unwrap_or_default();
    }
    name.to_string()
}

fn running_apps() -> (HashSet<String>, HashSet<String>) {
    let (ids, names) = crate::native::running_apps();
    let ids: HashSet<String> = ids.into_iter().map(|s| s.to_lowercase()).collect();
    let names: HashSet<String> = names.into_iter().map(|s| s.to_lowercase()).collect();
    (ids, names)
}

const CHROMIUM_CACHES: [&str; 9] = [
    "Cache",
    "Code Cache",
    "GPUCache",
    "DawnCache",
    "DawnWebGPUCache",
    "DawnGraphiteCache",
    "GraphiteDawnCache",
    "Service Worker/CacheStorage",
    "Shared Dictionary",
];

pub fn start_scan(shared: Shared, ctx: egui::Context, use_ai: bool) {
    {
        let mut g = shared.lock().unwrap();
        *g = State { phase: Phase::Scanning, ..Default::default() };
    }
    std::thread::spawn(move || {
        let (running_ids, running_names) = running_apps();
        let mut c = Ctx { items: Vec::new(), running_ids, running_names, shared: shared.clone(), egui: ctx.clone() };
        let steps: [ScanStep; 8] = [
            ("Claude", scan_claude),
            ("AI tools", scan_ai_tools),
            ("User caches", scan_user_caches),
            ("App caches", scan_app_caches),
            ("Logs", scan_logs),
            ("Xcode", scan_xcode),
            ("Developer caches", scan_developer),
            ("Trash", scan_trash),
        ];
        for (i, (_, f)) in steps.iter().enumerate() {
            if c.stopped() {
                break;
            }
            c.status("", i as f32 / steps.len() as f32);
            f(&mut c);
            c.shared.lock().unwrap().progress = (i + 1) as f32 / steps.len() as f32;
        }
        // de-duplicate (a path found by two rules keeps the first)
        let mut seen = HashSet::new();
        c.items.retain(|it| seen.insert(it.path.clone()));
        // drop items nested inside another item
        let paths: Vec<PathBuf> = c.items.iter().map(|i| i.path.clone()).collect();
        c.items.retain(|it| !paths.iter().any(|p| p != &it.path && it.path.starts_with(p)));
        c.items.sort_by(|a, b| a.group.cmp(&b.group).then(b.bytes.cmp(&a.bytes)));
        for it in c.items.iter_mut() {
            it.selected = it.verdict == Verdict::Safe && !it.permanent;
        }
        let stopped = c.stopped();
        {
            let mut g = shared.lock().unwrap();
            g.items = std::mem::take(&mut c.items);
            g.phase = if use_ai && !stopped { Phase::Judging } else { Phase::Results };
        }
        ctx.request_repaint();
        if use_ai && !stopped {
            judge(&shared, &ctx);
            let mut g = shared.lock().unwrap();
            for it in g.items.iter_mut() {
                it.selected = it.verdict == Verdict::Safe && !it.permanent;
            }
            g.phase = Phase::Results;
            ctx.request_repaint();
        }
    });
}

// ---- Claude (catalog read from the original's AIToolScanning, plus project rules)

fn scan_claude(c: &mut Ctx) {
    let h = home();
    let cl = h.join(".claude");
    let claude_running = c.running_names.iter().any(|n| n == "claude") || process_running("claude");
    // AIToolScanning sub_7118: Claude Code catalog
    c.add(Group::Claude, "Claude Code debug logs", cl.join("debug"), "Logs", Verdict::Safe, "Debug logs; Claude Code writes new ones as needed");
    c.add(Group::Claude, "Claude Code cache", cl.join("cache"), "Caches", Verdict::Safe, "Cache; rebuilt automatically");
    c.add(Group::Claude, "Claude Code telemetry", cl.join("telemetry"), "Caches", Verdict::Safe, "Usage telemetry waiting to be sent");
    c.add(Group::Claude, "Stats cache", cl.join("stats-cache.json"), "Caches", Verdict::Safe, "Cached statistics; recalculated");
    c.add(Group::Claude, "MCP sign-in cache", cl.join("mcp-needs-auth-cache.json"), "Caches", Verdict::Safe, "Cached MCP sign-in state");
    c.add(Group::Claude, "Plugin install counts", cl.join("plugins/install-counts-cache.json"), "Caches", Verdict::Safe, "Cached plugin popularity counts");
    // shell snapshots: only the old ones (a running session uses the newest)
    let snaps = cl.join("shell-snapshots");
    for f in children(&snaps) {
        let age = age_days(&f);
        if age >= m::SHELL_SNAPSHOT_MIN_AGE_DAYS {
            c.add(Group::Claude, "Old shell snapshot", f, "Temp Files", Verdict::Safe, "Temporary shell snapshot from a finished session");
        }
    }
    // stale versions: keep the one ~/.local/bin/claude points to (ActiveVersionResolver)
    let versions = h.join(".local/share/claude/versions");
    let active = std::fs::canonicalize(h.join(".local/bin/claude")).ok();
    for v in children(&versions) {
        let is_active = active.as_ref().map(|a| a.starts_with(&v) || a == &v).unwrap_or(false);
        if !is_active {
            c.add(Group::Claude, &format!("Claude Code {}", fname(&v)), v, "Stale Versions", Verdict::Safe, "Old Claude Code version; the one you run is kept");
        }
    }
    c.add(Group::Claude, "Claude CLI logs", h.join("Library/Caches/claude-cli-nodejs"), "Logs", Verdict::Safe, "Claude Code's own error/MCP logs");
    // AIToolScanning sub_7308: Claude Desktop
    c.add(Group::Claude, "Claude Desktop logs", h.join("Library/Logs/Claude"), "Logs", Verdict::Safe, "Claude Desktop logs");
    c.add(Group::Claude, "Claude Desktop cache", h.join("Library/Caches/com.anthropic.claudefordesktop"), "Caches", Verdict::Safe, "App cache; rebuilt automatically");
    c.add(Group::Claude, "Claude updater cache", h.join("Library/Caches/com.anthropic.claudefordesktop.ShipIt"), "Caches", Verdict::Safe, "Leftover update download");
    let desk = h.join("Library/Application Support/Claude");
    let desktop_running = c.running_ids.contains("com.anthropic.claudefordesktop");
    for sub in CHROMIUM_CACHES {
        let p = desk.join(sub);
        let (v, r) = if desktop_running { (Verdict::Ask, "Claude Desktop is open — its web cache is in use") } else { (Verdict::Safe, "Web view cache; rebuilt automatically") };
        c.add(Group::Claude, &format!("Claude Desktop {}", sub), p, "Caches", v, r);
    }
    c.add(Group::Claude, "Claude Desktop VM image", desk.join("vm_bundles"), "Downloads", Verdict::Ask, "Sandbox VM image for agent mode; downloaded again if removed");
    c.add(Group::Claude, "Claude Desktop Code VM", desk.join("claude-code-vm"), "Downloads", Verdict::Ask, "Runtime for Claude Code inside the desktop app; re-downloaded if removed");
    // projects: the ones you use are kept, old ones are offered for review
    for p in claude::scan_projects() {
        let idle = p.days_idle();
        let name = p.display_name();
        if p.in_use() {
            c.add(Group::Claude, &format!("Project · {name}"), p.folder.clone(), "Projects", Verdict::Keep, &format!("You used this project {} — kept", ago(idle)));
            if let Some(it) = c.items.last_mut() {
                if it.path == p.folder {
                    it.hard_keep = true;
                }
            }
        } else if !p.cwd_exists {
            c.add(Group::Claude, &format!("Project · {name}"), p.folder.clone(), "Old Sessions", Verdict::Ask, &format!("Chat history for a folder that no longer exists; last used {}", ago(idle)));
        } else {
            c.add(Group::Claude, &format!("Project · {name}"), p.folder.clone(), "Old Sessions", Verdict::Ask, &format!("Chat history not used for {} days (the project folder itself is never touched)", idle as u64));
        }
    }
    // old file-history snapshots and uploads
    for (sub, label, kind) in [("file-history", "Edit history", "Old Sessions"), ("uploads", "Uploaded files", "Temp Files")] {
        for f in children(&cl.join(sub)) {
            if age_days(&f) > m::CLAUDE_OLD_DATA_DAYS as f64 {
                c.add(Group::Claude, &format!("{label} · {}", short_id(&fname(&f))), f, kind, Verdict::Ask, &format!("{label} older than {} days", m::CLAUDE_OLD_DATA_DAYS));
            }
        }
    }
    if claude_running {
        // a live session may hold files in these folders
        for it in c.items.iter_mut().filter(|i| i.group == Group::Claude && i.kind == "Temp Files" && i.verdict == Verdict::Safe) {
            if it.age_days < 1.0 {
                it.verdict = Verdict::Ask;
                it.reason = "Claude is running — this may belong to an open session".into();
            }
        }
    }
}

fn short_id(s: &str) -> String {
    if s.chars().count() > 14 {
        format!("{}…", s.chars().take(12).collect::<String>())
    } else {
        s.to_string()
    }
}

fn ago(days: f64) -> String {
    if days < 1.0 {
        "today".into()
    } else if days < 2.0 {
        "yesterday".into()
    } else {
        format!("{} days ago", days as u64)
    }
}

fn process_running(name: &str) -> bool {
    Command::new("pgrep").args(["-x", name]).output().map(|o| o.status.success()).unwrap_or(false)
}

// ---- other AI tools (AIToolScanning catalog)

fn scan_ai_tools(c: &mut Ctx) {
    let h = home();
    for sub in ["codex_apps_server_info", "codex_apps_tools"] {
        c.add(Group::AiTools, "Codex cache", h.join(".codex/cache").join(sub), "Caches", Verdict::Safe, "Codex app cache");
    }
    let gem = h.join(".gemini/antigravity-browser-profile");
    for sub in ["Default/Cache", "Default/Code Cache", "Default/DawnGraphiteCache", "Default/DawnWebGPUCache", "Default/GPUCache", "Default/Service Worker/CacheStorage", "GraphiteDawnCache", "component_crx_cache", "extensions_crx_cache"] {
        c.add(Group::AiTools, "Gemini / Antigravity browser cache", gem.join(sub), "Caches", Verdict::Safe, "Browser cache of the agent profile");
    }
    let mcp = h.join(".cache/chrome-devtools-mcp/chrome-profile");
    for sub in ["Default/Cache", "Default/Code Cache", "Default/DawnCache", "Default/GPUCache", "Default/GrShaderCache", "Default/GraphiteDawnCache", "Default/Service Worker/CacheStorage", "GraphiteDawnCache", "component_crx_cache", "extensions_crx_cache"] {
        c.add(Group::AiTools, "Chrome DevTools MCP cache", mcp.join(sub), "Caches", Verdict::Safe, "Browser cache of the MCP profile");
    }
    for f in children(&h.join(".local/share/cursor-agent")) {
        if f.extension().map(|e| e == "log").unwrap_or(false) {
            c.add(Group::AiTools, "Cursor agent log", f, "Logs", Verdict::Safe, "Cursor agent log file");
        }
    }
    c.add(Group::AiTools, "OpenCode logs", h.join(".local/share/opencode/log"), "Logs", Verdict::Safe, "OpenCode logs");
    // local models are big and costly to get back: always ask
    c.add(Group::AiTools, "Ollama models", h.join(".ollama/models"), "Local Models", Verdict::Ask, "Downloaded models may still be used by your projects");
    c.add(Group::AiTools, "LM Studio models", h.join(".lmstudio/models"), "Local Models", Verdict::Ask, "Downloaded models may still be used by your projects");
    c.add(Group::AiTools, "Hugging Face cache", h.join(".cache/huggingface"), "Local Models", Verdict::Ask, "Downloaded models and datasets; large to download again");
}

// ---- System Junk: user caches (CMUserCachesScanTask)

fn scan_user_caches(c: &mut Ctx) {
    let h = home();
    let caches = h.join("Library/Caches");
    let skip = ["claude-cli-nodejs", "com.anthropic.claudefordesktop", "com.anthropic.claudefordesktop.ShipIt"];
    let keep = [
        ("com.apple.bird", "iCloud Drive sync state — removing it can disrupt syncing"),
        ("CloudKit", "iCloud sync data"),
        ("com.apple.cloudd", "iCloud sync data"),
        ("com.apple.nsurlsessiond", "Downloads in progress for apps"),
        ("com.apple.akd", "Apple ID sign-in"),
        ("FamilyCircle", "Family Sharing"),
    ];
    for p in children(&caches) {
        if c.stopped() {
            return;
        }
        let n = fname(&p);
        if skip.contains(&n.as_str()) || n.starts_with('.') {
            continue;
        }
        if let Some((_, why)) = keep.iter().find(|(k, _)| *k == n) {
            c.add(Group::UserCache, &pretty(&n), p, "Caches", Verdict::Keep, why);
            continue;
        }
        let (v, why) = if n.starts_with("com.apple.") {
            (Verdict::Ask, "macOS system cache — usually safe, rebuilt by macOS")
        } else if n == "Homebrew" {
            (Verdict::Safe, "Downloaded installers; Homebrew fetches them again")
        } else {
            (Verdict::Safe, "App cache; the app rebuilds it")
        };
        c.add(Group::UserCache, &pretty(&n), p, "Caches", v, why);
    }
}

// ---- App caches: hard-coded app rules from SystemJunkScanning + Electron + sandbox

fn scan_app_caches(c: &mut Ctx) {
    let h = home();
    let rules: [(&str, &str, Verdict, &str); 10] = [
        ("Library/Application Support/Adobe/Common/Media Cache Files", "Adobe Media Cache", Verdict::Safe, "Premiere/After Effects media cache; rebuilt on open"),
        ("Library/Application Support/Adobe/Common/Media Cache", "Adobe Media Cache Database", Verdict::Safe, "Media cache index; rebuilt on open"),
        ("Library/Application Support/Spotify/PersistentCache/Storage", "Spotify offline cache", Verdict::Safe, "Streamed music cache (not your downloads list)"),
        ("Library/Application Support/Telegram Desktop/tdata/user_data", "Telegram media cache", Verdict::Ask, "Media from chats; downloaded again when opened"),
        ("Library/Containers/org.telegram.desktop/Data/Library/Application Support/Telegram Desktop/tdata/user_data", "Telegram media cache", Verdict::Ask, "Media from chats; downloaded again when opened"),
        ("Library/Containers/com.tencent.xinWeChat/Data/Library/Application Support/com.tencent.xinWeChat/2.0b4.0.9", "WeChat data", Verdict::Ask, "WeChat media and chat files"),
        ("Library/Containers/com.apple.mail/Data/Library/Mail Downloads", "Mail Downloads", Verdict::Safe, "Copies of opened attachments; the originals stay in your mail"),
        ("Library/Application Support/Steam/appcache", "Steam app cache", Verdict::Safe, "Store and library cache"),
        ("Library/Application Support/Steam/depotcache", "Steam depot cache", Verdict::Safe, "Download manifests"),
        ("Library/Application Support/MobileSync/Backup", "iOS Device Backups", Verdict::Ask, "Backups of your iPhone/iPad — removing the latest one means you can't restore from it"),
    ];
    for (rel, title, v, why) in rules {
        c.add(Group::AppCaches, title, h.join(rel), "Caches", v, why);
    }
    // Electron / Chromium app caches (CMElectronAppsCacheScanTask)
    let support = h.join("Library/Application Support");
    for app in children(&support) {
        if c.stopped() {
            return;
        }
        let n = fname(&app);
        if n == "Claude" || !app.is_dir() {
            continue;
        }
        for base in [app.clone(), app.join("Default")] {
            for sub in CHROMIUM_CACHES {
                let p = base.join(sub);
                if p.is_dir() {
                    c.add(Group::AppCaches, &format!("{} · {}", pretty(&n), sub), p, "Caches", Verdict::Safe, "Web view cache; rebuilt automatically");
                }
            }
        }
    }
    // sandboxed apps (CMSandboxContainersCachesScanTask)
    for ct in children(&h.join("Library/Containers")) {
        if c.stopped() {
            return;
        }
        let n = fname(&ct);
        let p = ct.join("Data/Library/Caches");
        if p.is_dir() {
            let v = if n.starts_with("com.apple.") { Verdict::Ask } else { Verdict::Safe };
            c.add(Group::AppCaches, &format!("{} (sandboxed)", pretty(&n)), p, "Caches", v, "Sandboxed app cache; rebuilt automatically");
        }
    }
}

// ---- logs (CMUserLogsScanTask, DiagnosticLogs)

fn scan_logs(c: &mut Ctx) {
    let h = home();
    for p in children(&h.join("Library/Logs")) {
        let n = fname(&p);
        if n == "Claude" || n.starts_with('.') {
            continue;
        }
        let title = if n == "DiagnosticReports" { "Crash reports".to_string() } else { format!("{} logs", pretty(&n)) };
        c.add(Group::Logs, &title, p, "Logs", Verdict::Safe, "Log files; only useful for debugging");
    }
    for ct in children(&h.join("Library/Containers")) {
        let p = ct.join("Data/Library/Logs");
        if p.is_dir() {
            c.add(Group::Logs, &format!("{} logs (sandboxed)", pretty(&fname(&ct))), p, "Logs", Verdict::Safe, "Log files of a sandboxed app");
        }
    }
}

// ---- Xcode Junk (CMXcode* scan tasks)

fn scan_xcode(c: &mut Ctx) {
    let x = home().join("Library/Developer");
    for p in children(&x.join("Xcode/DerivedData")) {
        c.add(Group::Xcode, &format!("Derived Data · {}", fname(&p).split('-').next().unwrap_or("")), p, "Build Data", Verdict::Safe, "Build products and index; Xcode rebuilds them");
    }
    c.add(Group::Xcode, "Xcode Caches", x.join("Xcode/DocumentationCache"), "Caches", Verdict::Safe, "Documentation cache");
    c.add(Group::Xcode, "Swift module cache", x.join("Xcode/ModuleCache.noindex"), "Caches", Verdict::Safe, "Precompiled module cache");
    c.add(Group::Xcode, "SwiftUI Previews", x.join("Xcode/UserData/Previews"), "Caches", Verdict::Safe, "Preview simulator data");
    c.add(Group::Xcode, "iOS Device Logs", x.join("Xcode/iOS Device Logs"), "Logs", Verdict::Safe, "Device logs collected by Xcode");
    c.add(Group::Xcode, "Simulator caches", x.join("CoreSimulator/Caches"), "Caches", Verdict::Safe, "Simulator dyld caches; rebuilt on boot");
    for os in ["iOS DeviceSupport", "watchOS DeviceSupport", "macOS DeviceSupport", "tvOS DeviceSupport", "visionOS DeviceSupport"] {
        let mut vers = children(&x.join("Xcode").join(os));
        vers.sort_by_key(|v| version_key(&fname(v)));
        let newest = vers.last().cloned();
        for v in vers {
            let latest = Some(&v) == newest.as_ref();
            let (verdict, why) = if latest { (Verdict::Ask, "Newest device support — needed to debug on that device") } else { (Verdict::Safe, "Support files for an older OS version; recreated when you connect that device") };
            c.add(Group::Xcode, &format!("{} {}", os.replace(" DeviceSupport", ""), fname(&v)), v, "Device Support", verdict, why);
        }
    }
    for a in children(&x.join("Xcode/Archives")) {
        c.add(Group::Xcode, &format!("Archives {}", fname(&a)), a, "Archives", Verdict::Ask, "App archives hold debug symbols for builds you shipped");
    }
}

fn version_key(s: &str) -> Vec<u32> {
    s.split(|c: char| !c.is_ascii_digit()).filter_map(|x| x.parse().ok()).take(4).collect()
}

// ---- developer caches (Gradle from the original, the rest common package managers)

fn scan_developer(c: &mut Ctx) {
    let h = home();
    let rules: [(&str, &str, Verdict, &str); 9] = [
        (".npm/_cacache", "npm cache", Verdict::Safe, "npm downloads; fetched again when needed"),
        (".gradle/caches", "Gradle caches", Verdict::Safe, "Gradle dependency cache"),
        (".cache/pip", "pip cache", Verdict::Safe, "Python package downloads"),
        ("go/pkg/mod/cache/download", "Go module downloads", Verdict::Safe, "Go module download cache"),
        (".bun/install/cache", "Bun cache", Verdict::Safe, "Bun package downloads"),
        (".yarn/berry/cache", "Yarn cache", Verdict::Safe, "Yarn package downloads"),
        (".cargo/registry/cache", "Rust crate downloads", Verdict::Ask, "Crate archives; needed for offline builds"),
        (".cocoapods/repos", "CocoaPods specs", Verdict::Ask, "Spec repositories; cloned again on next install"),
        ("Library/pnpm/store", "pnpm store", Verdict::Ask, "Shared package store linked into your projects"),
    ];
    for (rel, title, v, why) in rules {
        c.add(Group::Developer, title, h.join(rel), "Caches", v, why);
    }
}

// ---- Trash bins

fn scan_trash(c: &mut Ctx) {
    let t = home().join(".Trash");
    if std::fs::read_dir(&t).is_ok() {
        c.add(Group::Trash, "Trash", t, "Trash", Verdict::Safe, "Already deleted items; emptying can't be undone");
        if let Some(it) = c.items.last_mut() {
            if it.group == Group::Trash {
                it.permanent = true;
            }
        }
    }
}

// ---------------------------------------------------------------- AI judge

fn judge(shared: &Shared, ctx: &egui::Context) {
    let Some(bin) = claude::claude_bin() else {
        let mut g = shared.lock().unwrap();
        g.ai_error = Some("Claude Code isn't installed, so only the built-in rules were used.".into());
        return;
    };
    let h = home().display().to_string();
    let candidates: Vec<usize> = {
        let g = shared.lock().unwrap();
        g.items.iter().enumerate().filter(|(_, it)| !it.hard_keep && it.bytes >= m::AI_MIN_BYTES).map(|(i, _)| i).collect()
    };
    let batches: Vec<Vec<usize>> = candidates.chunks(m::AI_BATCH).map(|c| c.to_vec()).collect();
    let n = batches.len().max(1);
    for (bi, batch) in batches.iter().enumerate() {
        if shared.lock().unwrap().stop {
            break;
        }
        let lines: String = {
            let g = shared.lock().unwrap();
            batch
                .iter()
                .enumerate()
                .map(|(k, &i)| {
                    let it = &g.items[i];
                    format!(
                        "{k} | {} | {} | modified {} days ago | group: {} | rule says {} ({}){}\n",
                        it.path.display().to_string().replacen(&h, "~", 1),
                        crate::glass::human_bytes(it.bytes),
                        it.age_days as u64,
                        it.group.title(),
                        it.verdict.word().to_lowercase(),
                        it.rule_reason,
                        if it.app_running { " | app is open now" } else { "" }
                    )
                })
                .collect()
        };
        {
            let mut g = shared.lock().unwrap();
            g.ai_status = format!("Claude is reviewing {} paths (batch {} of {})…", batch.len(), bi + 1, n);
            g.progress = bi as f32 / n as f32;
        }
        ctx.request_repaint();
        let out = Command::new(&bin)
            .current_dir(std::env::temp_dir())
            .args(["-p", &lines, "--system-prompt", SYSTEM, "--output-format", "json", "--no-session-persistence", "--model", m::AI_MODEL, "--tools", "", "--strict-mcp-config"])
            .output();
        let text = match out {
            Ok(o) if !o.status.success() => {
                let err = String::from_utf8_lossy(&o.stderr).trim().lines().last().unwrap_or("").to_string();
                let mut g = shared.lock().unwrap();
                g.ai_error = Some(if err.is_empty() { "Claude couldn't review the paths; the built-in rules were used.".into() } else { format!("Claude couldn't review the paths ({err}); the built-in rules were used.") });
                return;
            }
            Ok(o) => serde_json::from_slice::<serde_json::Value>(&o.stdout).ok().and_then(|v| v["result"].as_str().map(|s| s.to_string())).unwrap_or_default(),
            Err(e) => {
                shared.lock().unwrap().ai_error = Some(format!("Couldn't run Claude: {e}"));
                return;
            }
        };
        let verdicts = parse_verdicts(&text);
        let mut g = shared.lock().unwrap();
        if verdicts.is_empty() {
            g.ai_error = Some("Claude's answer couldn't be read; the built-in rules were used for some items.".into());
            continue;
        }
        g.ai_used = true;
        for (k, v, r) in verdicts {
            let Some(&i) = batch.get(k) else { continue };
            let it = &mut g.items[i];
            let new = merge_verdict(it.verdict, v, it.app_running);
            if new != it.verdict || !r.is_empty() {
                it.by = Who::Ai;
                if !r.is_empty() {
                    it.reason = r;
                }
            }
            it.verdict = new;
        }
        drop(g);
        ctx.request_repaint();
    }
}

/// Combine a rule's verdict with the AI's: the AI may always be more careful;
/// it may relax a rule's "ask" to "safe", except for items the person must
/// confirm (app open). It never relaxes "keep".
fn merge_verdict(rule: Verdict, ai: Verdict, app_running: bool) -> Verdict {
    match (rule, ai) {
        (_, ai) if ai.rank() > rule.rank() => ai,
        (Verdict::Ask, Verdict::Safe) if !app_running => Verdict::Safe,
        (cur, _) => cur,
    }
}

const SYSTEM: &str = "You are the safety judge inside a Mac cleanup app. You only see file and folder PATHS with size and age — never their contents. \
For each numbered line decide: \
\"safe\" = removing it cannot lose the person's work, settings or history; the app rebuilds or re-downloads it (caches, logs, temp files, build outputs, old versions). \
\"ask\" = probably removable but it could cost something: a big re-download, losing chat/history, an app that is open, or a purpose you are not sure about. \
\"keep\" = personal files, settings, credentials, databases, active project data, or anything that looks important. \
Claude Code chat histories of projects the person still uses must be keep; histories of deleted folders are ask. When in doubt choose ask. \
Reply ONLY with a JSON array like [{\"i\":0,\"v\":\"safe\",\"r\":\"reason in under 14 words\"}] covering every line.";

fn parse_verdicts(text: &str) -> Vec<(usize, Verdict, String)> {
    let (Some(a), Some(b)) = (text.find('['), text.rfind(']')) else { return vec![] };
    let Ok(v) = serde_json::from_str::<serde_json::Value>(&text[a..=b]) else { return vec![] };
    v.as_array()
        .map(|arr| {
            arr.iter()
                .filter_map(|e| {
                    let i = e["i"].as_u64()? as usize;
                    let verdict = match e["v"].as_str()? {
                        "safe" => Verdict::Safe,
                        "keep" => Verdict::Keep,
                        _ => Verdict::Ask,
                    };
                    Some((i, verdict, e["r"].as_str().unwrap_or("").to_string()))
                })
                .collect()
        })
        .unwrap_or_default()
}

// ---------------------------------------------------------------- removal

pub fn start_clean(shared: Shared, ctx: egui::Context, delete_now: bool) {
    let targets: Vec<Item> = {
        let mut g = shared.lock().unwrap();
        g.phase = Phase::Cleaning;
        g.cleaned_bytes = 0;
        g.cleaned_count = 0;
        g.failed.clear();
        g.log.clear();
        g.items.iter().filter(|i| i.selected && i.verdict != Verdict::Keep && !i.hard_keep).cloned().collect()
    };
    std::thread::spawn(move || {
        let n = targets.len().max(1);
        for (k, it) in targets.iter().enumerate() {
            {
                let mut g = shared.lock().unwrap();
                g.current = it.path.display().to_string();
                g.progress = k as f32 / n as f32;
            }
            ctx.request_repaint();
            // last safety net
            if let Some(why) = is_protected(&it.path).filter(|_| it.group != Group::Trash) {
                let mut g = shared.lock().unwrap();
                g.failed.push((it.title.clone(), why.to_string()));
                g.log.push(format!("Skipped {}: {why}", it.path.display()));
                continue;
            }
            let res = if it.group == Group::Trash {
                empty_folder(&it.path)
            } else if delete_now {
                remove(&it.path)
            } else {
                move_to_trash(&it.path)
            };
            let mut g = shared.lock().unwrap();
            match res {
                Ok(()) => {
                    g.cleaned_bytes += it.bytes;
                    g.cleaned_count += 1;
                    g.log.push(format!("Removed {} ({})", it.path.display(), crate::glass::human_bytes(it.bytes)));
                    g.items.retain(|x| x.path != it.path);
                }
                Err(e) => {
                    g.failed.push((it.title.clone(), e.clone()));
                    g.log.push(format!("Couldn't remove {}: {e}", it.path.display()));
                }
            }
        }
        let mut g = shared.lock().unwrap();
        g.progress = 1.0;
        g.phase = Phase::Done;
        ctx.request_repaint();
    });
}

fn remove(p: &Path) -> Result<(), String> {
    let md = p.symlink_metadata().map_err(|e| e.to_string())?;
    if md.is_dir() {
        std::fs::remove_dir_all(p).map_err(|e| e.to_string())
    } else {
        std::fs::remove_file(p).map_err(|e| e.to_string())
    }
}

fn empty_folder(p: &Path) -> Result<(), String> {
    let mut err = None;
    for c in children(p) {
        if let Err(e) = remove(&c) {
            err = Some(e);
        }
    }
    err.map_or(Ok(()), Err)
}

/// Move to the Trash (so it can be put back). Never falls back to a permanent
/// delete: an item that can't be trashed is reported as failed.
pub fn move_to_trash(p: &Path) -> Result<(), String> {
    crate::native::trash_item(p)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn short_id_is_utf8_safe() {
        assert_eq!(short_id("abc"), "abc");
        assert_eq!(short_id("0123456789abcdef"), "0123456789ab…");
        // multi-byte names used to panic on a byte slice
        assert_eq!(short_id("знімок-екрана-2026"), "знімок-екран…");
    }

    #[test]
    fn pretty_names() {
        assert_eq!(pretty("com.spotify.client"), "Spotify");
        assert_eq!(pretty("COM.GOOGLE.CHROME"), "Google Chrome");
        assert_eq!(pretty("com.example.my-tool"), "My tool");
        assert_eq!(pretty("com.example.Foo.helper"), "Foo");
        assert_eq!(pretty("Homebrew"), "Homebrew");
        assert_eq!(pretty("com.example.ёлка"), "Ёлка");
    }

    #[test]
    fn version_sorting() {
        let mut v = vec!["17.10 (21A)", "17.2 (21C)", "9.3"];
        v.sort_by_key(|s| version_key(s));
        assert_eq!(v, vec!["9.3", "17.2 (21C)", "17.10 (21A)"]);
    }

    #[test]
    fn verdicts_from_ai_text() {
        let text = "Sure:\n[{\"i\":0,\"v\":\"safe\",\"r\":\"cache\"},{\"i\":1,\"v\":\"keep\"},{\"i\":2,\"v\":\"maybe\",\"r\":\"?\"},{\"v\":\"safe\"}]\nDone.";
        let v = parse_verdicts(text);
        assert_eq!(v.len(), 3);
        assert_eq!(v[0], (0, Verdict::Safe, "cache".to_string()));
        assert_eq!(v[1], (1, Verdict::Keep, String::new()));
        // anything unknown is treated as "ask"
        assert_eq!(v[2].1, Verdict::Ask);
        assert!(parse_verdicts("no json here").is_empty());
        assert!(parse_verdicts("[not json]").is_empty());
    }

    #[test]
    fn ai_can_only_be_more_careful_except_ask_to_safe() {
        use Verdict::*;
        assert_eq!(merge_verdict(Safe, Ask, false), Ask);
        assert_eq!(merge_verdict(Safe, Keep, false), Keep);
        assert_eq!(merge_verdict(Ask, Safe, false), Safe);
        // an open app must still be confirmed
        assert_eq!(merge_verdict(Ask, Safe, true), Ask);
        // "keep" is never relaxed
        assert_eq!(merge_verdict(Keep, Safe, false), Keep);
        assert_eq!(merge_verdict(Keep, Ask, false), Keep);
    }

    #[test]
    fn personal_folders_are_protected() {
        let h = home();
        assert!(is_protected(&h.join("Documents/report.pdf")).is_some());
        assert!(is_protected(&h.join(".ssh")).is_some());
        assert!(is_protected(&h.join("Library/Mail/V10")).is_some());
        assert!(is_protected(&h.join(".claude/settings.json")).is_some());
        assert!(is_protected(&h.join(".claude/skills/x")).is_some());
        assert!(is_protected(&h.join(".claude/debug")).is_none());
        assert!(is_protected(&h.join("Library/Caches/com.example.app")).is_none());
        // "Documents" must match a whole folder name, not a prefix
        assert!(is_protected(&h.join("DocumentsOld")).is_none());
    }

    #[test]
    fn git_folders_are_protected() {
        let dir = std::env::temp_dir().join(format!("cmb-git-{}", std::process::id()));
        std::fs::create_dir_all(dir.join(".git")).unwrap();
        let r = is_protected(&dir);
        std::fs::remove_dir_all(&dir).unwrap();
        assert!(r.is_some());
    }

    #[test]
    fn state_totals() {
        let item = |group, bytes, verdict, selected| Item {
            group,
            title: String::new(),
            path: PathBuf::from(format!("/x/{bytes}")),
            bytes,
            age_days: 0.0,
            kind: "Caches",
            verdict,
            reason: String::new(),
            rule_reason: String::new(),
            by: Who::Rule,
            selected,
            hard_keep: false,
            permanent: false,
            app_running: false,
        };
        let s = State {
            items: vec![
                item(Group::Claude, 10, Verdict::Safe, true),
                item(Group::Claude, 20, Verdict::Ask, false),
                item(Group::Logs, 40, Verdict::Keep, false),
                item(Group::Logs, 80, Verdict::Safe, true),
            ],
            ..Default::default()
        };
        assert_eq!(s.total(), 110);
        assert_eq!(s.selected_bytes(), 90);
        assert_eq!(s.pending_questions(), 1);
        assert_eq!(s.group_bytes(Group::Logs), 80);
        assert_eq!(s.claude_safe_bytes(), 10);
    }
}
