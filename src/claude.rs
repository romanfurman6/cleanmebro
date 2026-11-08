//! Claude info for the menu panel: plan and limits come from the Claude CLI
//! (`claude auth status`, `claude -p /usage`, which spends no tokens), local
//! activity comes from counting usage fields in ~/.claude/projects logs.

use crate::mechanics as m;
use chrono::{DateTime, Local, Utc};
use serde_json::Value;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime};

#[derive(Clone, Debug, Default)]
pub struct Limit {
    pub label: String,
    pub pct: f32,
    pub resets: String,
}

#[derive(Clone, Debug, Default)]
pub struct Contrib {
    pub title: String,
    pub lines: Vec<String>,
}

#[derive(Clone, Debug, Default)]
pub struct Project {
    pub folder: PathBuf, // the ~/.claude/projects/<x> folder
    pub cwd: Option<PathBuf>,
    pub cwd_exists: bool,
    pub last_used: Option<SystemTime>,
    pub bytes: u64,
}

impl Project {
    pub fn display_name(&self) -> String {
        match &self.cwd {
            Some(c) => c.file_name().map(|s| s.to_string_lossy().into_owned()).unwrap_or_else(|| c.display().to_string()),
            None => self.folder.file_name().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default(),
        }
    }
    pub fn days_idle(&self) -> f64 {
        self.last_used
            .and_then(|t| SystemTime::now().duration_since(t).ok())
            .map(|d| d.as_secs_f64() / 86_400.0)
            .unwrap_or(9999.0)
    }
    /// A project the person still works on: its folder exists and it was used recently.
    pub fn in_use(&self) -> bool {
        self.cwd_exists && self.days_idle() <= m::CLAUDE_PROJECT_ACTIVE_DAYS as f64
    }
}

#[derive(Clone, Debug, Default)]
pub struct Local7 {
    pub tokens_5h: u64,
    pub tokens_today: u64,
    pub tokens_7d: u64,
    pub output_7d: u64,
    pub requests_today: u64,
    pub daily: Vec<u64>, // 7 buckets, oldest first
}

#[derive(Clone, Debug, Default)]
pub struct ClaudeInfo {
    pub loading: bool,
    pub installed: bool,
    pub logged_in: bool,
    pub plan: String,
    pub limits: Vec<Limit>,
    pub contrib: Vec<Contrib>,
    pub note: String,
    pub error: Option<String>,
    pub fetched: Option<DateTime<Local>>,
    pub local: Local7,
    pub projects: Vec<Project>,
    pub data_bytes: u64,
}

impl ClaudeInfo {
    pub fn session(&self) -> Option<&Limit> {
        self.limits.iter().find(|l| l.label.to_lowercase().contains("session"))
    }
    pub fn weekly(&self) -> Option<&Limit> {
        self.limits
            .iter()
            .find(|l| l.label.to_lowercase().contains("all models"))
            .or_else(|| self.limits.iter().find(|l| l.label.to_lowercase().contains("week")))
    }
    pub fn plan_title(&self) -> String {
        if self.plan.is_empty() {
            return "Claude".into();
        }
        let mut p = self.plan.clone();
        if let Some(f) = p.get_mut(0..1) {
            f.make_ascii_uppercase();
        }
        format!("Claude {p}")
    }
}

pub type Shared = Arc<Mutex<ClaudeInfo>>;

/// Find the claude executable even when the app is started from Finder (no shell PATH).
pub fn claude_bin() -> Option<PathBuf> {
    let home = dirs::home_dir().unwrap_or_default();
    let mut cands = vec![
        home.join(".local/bin/claude"),
        PathBuf::from("/opt/homebrew/bin/claude"),
        PathBuf::from("/usr/local/bin/claude"),
        home.join(".claude/local/claude"),
        home.join(".npm-global/bin/claude"),
    ];
    if let Ok(path) = std::env::var("PATH") {
        for d in path.split(':') {
            cands.push(Path::new(d).join("claude"));
        }
    }
    cands.into_iter().find(|p| p.is_file())
}

fn claude_cmd(bin: &Path) -> Command {
    let mut c = Command::new(bin);
    // a neutral working folder so no project settings or CLAUDE.md get involved
    c.current_dir(std::env::temp_dir());
    let path = std::env::var("PATH").unwrap_or_default();
    c.env("PATH", format!("/opt/homebrew/bin:/usr/local/bin:/usr/bin:/bin:{path}"));
    c
}

pub fn start(ctx: egui::Context, demo: bool) -> Shared {
    let shared: Shared = Arc::new(Mutex::new(ClaudeInfo { loading: true, ..Default::default() }));
    if demo {
        *shared.lock().unwrap() = demo_info();
        return shared;
    }
    let s = shared.clone();
    std::thread::Builder::new()
        .name("claude".into())
        .spawn(move || {
            let mut cache: HashMap<PathBuf, (SystemTime, u64, Vec<(i64, u64, u64)>)> = HashMap::new();
            let mut last_limits = Instant::now() - Duration::from_secs(3600);
            loop {
                let want_limits = last_limits.elapsed().as_secs() >= m::CLAUDE_LIMITS_REFRESH_SECS
                    || s.lock().unwrap().limits.is_empty() && last_limits.elapsed().as_secs() > 60;
                if want_limits {
                    s.lock().unwrap().loading = true;
                    ctx.request_repaint();
                    fetch_cli(&s);
                    last_limits = Instant::now();
                }
                let projects = scan_projects();
                let local = local_usage(&mut cache);
                let data = crate::monitor::dir_size(&dirs::home_dir().unwrap_or_default().join(".claude"), 8);
                {
                    let mut g = s.lock().unwrap();
                    g.projects = projects;
                    g.local = local;
                    g.data_bytes = data;
                    g.loading = false;
                }
                ctx.request_repaint();
                // wait, but wake early when someone asks for a refresh
                for _ in 0..(m::CLAUDE_LOCAL_REFRESH_SECS * 2) {
                    std::thread::sleep(Duration::from_millis(500));
                    if REFRESH.swap(false, std::sync::atomic::Ordering::SeqCst) {
                        last_limits = Instant::now() - Duration::from_secs(3600 * 24);
                        break;
                    }
                }
            }
        })
        .ok();
    shared
}

static REFRESH: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

pub fn request_refresh() {
    REFRESH.store(true, std::sync::atomic::Ordering::SeqCst);
}

fn fetch_cli(s: &Shared) {
    let Some(bin) = claude_bin() else {
        let mut g = s.lock().unwrap();
        g.installed = false;
        g.error = Some("Claude Code isn't installed on this Mac.".into());
        return;
    };
    // plan + login
    let status = claude_cmd(&bin).args(["auth", "status", "--json"]).output();
    let (logged_in, plan) = match status {
        Ok(o) => {
            let v: Value = serde_json::from_slice(&o.stdout).unwrap_or(Value::Null);
            (v["loggedIn"].as_bool().unwrap_or(false), v["subscriptionType"].as_str().unwrap_or("").to_string())
        }
        Err(_) => (false, String::new()),
    };
    // limits (a local slash command: no model call, no tokens)
    let usage = claude_cmd(&bin)
        .args(["-p", "/usage", "--output-format", "json", "--no-session-persistence"])
        .output();
    let mut g = s.lock().unwrap();
    g.installed = true;
    g.logged_in = logged_in;
    if !plan.is_empty() {
        g.plan = plan;
    }
    match usage {
        Ok(o) => {
            let v: Value = serde_json::from_slice(&o.stdout).unwrap_or(Value::Null);
            let text = v["result"].as_str().unwrap_or("").to_string();
            let (limits, contrib, note) = parse_usage(&text);
            if limits.is_empty() {
                g.error = Some(if logged_in { "Claude didn't report limits right now.".into() } else { "Sign in to Claude Code to see your limits.".into() });
            } else {
                g.error = None;
                g.limits = limits;
                g.contrib = contrib;
                g.note = note;
                g.fetched = Some(Local::now());
            }
        }
        Err(e) => g.error = Some(format!("Couldn't run Claude: {e}")),
    }
}

/// Parse the text of `/usage`.
pub fn parse_usage(text: &str) -> (Vec<Limit>, Vec<Contrib>, String) {
    let mut limits = Vec::new();
    let mut contrib: Vec<Contrib> = Vec::new();
    let mut note = String::new();
    for raw in text.lines() {
        let line = raw.trim_end();
        let t = line.trim();
        if let Some(idx) = t.find("% used") {
            if let Some((label, rest)) = t[..idx].rsplit_once(':') {
                let pct = rest.trim().parse::<f32>().unwrap_or(0.0);
                let resets = t[idx..].split("resets").nth(1).map(|s| s.trim().to_string()).unwrap_or_default();
                limits.push(Limit { label: label.trim().to_string(), pct, resets });
                continue;
            }
        }
        if t.starts_with("Last ") && t.contains('·') {
            contrib.push(Contrib { title: t.to_string(), lines: vec![] });
            continue;
        }
        if line.starts_with("  ") && !t.is_empty() {
            if let Some(c) = contrib.last_mut() {
                c.lines.push(t.to_string());
                continue;
            }
        }
        if t.starts_with("You are currently") {
            note = t.to_string();
        }
    }
    (limits, contrib, note)
}

/// Pretty label for a limit row.
pub fn short_label(l: &str) -> String {
    let low = l.to_lowercase();
    if low.contains("session") {
        "Current session (5h)".into()
    } else if low.contains("all models") {
        "This week · all models".into()
    } else if let (Some(a), Some(b)) = (l.find('('), l.rfind(')')) {
        format!("This week · {}", &l[a + 1..b])
    } else {
        l.to_string()
    }
}

// ---------------------------------------------------------------- local data

pub fn projects_dir() -> PathBuf {
    dirs::home_dir().unwrap_or_default().join(".claude/projects")
}

pub fn scan_projects() -> Vec<Project> {
    let mut out = Vec::new();
    let Ok(rd) = std::fs::read_dir(projects_dir()) else { return out };
    for e in rd.flatten() {
        let folder = e.path();
        if !folder.is_dir() {
            continue;
        }
        let mut last: Option<SystemTime> = None;
        let mut cwd: Option<PathBuf> = None;
        if let Ok(files) = std::fs::read_dir(&folder) {
            let mut jsonls: Vec<(SystemTime, PathBuf)> = files
                .flatten()
                .filter(|f| f.path().extension().map(|x| x == "jsonl").unwrap_or(false))
                .filter_map(|f| f.metadata().ok().and_then(|m| m.modified().ok()).map(|t| (t, f.path())))
                .collect();
            jsonls.sort_by(|a, b| b.0.cmp(&a.0));
            last = jsonls.first().map(|x| x.0);
            for (_, p) in jsonls.iter().take(3) {
                if let Some(c) = first_cwd(p) {
                    cwd = Some(c);
                    break;
                }
            }
        }
        if last.is_none() {
            last = folder.metadata().ok().and_then(|m| m.modified().ok());
        }
        // the folder name is the project path with every non-alphanumeric
        // character turned into '-'; find the real folder it stands for
        let enc = folder.file_name().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
        if let Some(real) = resolve_encoded(&enc) {
            cwd = Some(real);
        } else if cwd.as_ref().map(|c| encode(&c.display().to_string()) != enc).unwrap_or(false) {
            cwd = Some(PathBuf::from(enc.replace('-', "/")));
        }
        let cwd_exists = cwd.as_ref().map(|c| c.exists()).unwrap_or(false);
        let bytes = crate::monitor::dir_size(&folder, 4);
        out.push(Project { folder, cwd, cwd_exists, last_used: last, bytes });
    }
    out.sort_by(|a, b| b.last_used.cmp(&a.last_used));
    out
}

/// How Claude Code names a project folder: non-alphanumerics become '-'.
pub fn encode(path: &str) -> String {
    path.chars().map(|c| if c.is_ascii_alphanumeric() { c } else { '-' }).collect()
}

/// Walk the disk to find the folder whose encoded path equals `enc`.
fn resolve_encoded(enc: &str) -> Option<PathBuf> {
    fn go(dir: &Path, rest: &str, depth: usize) -> Option<PathBuf> {
        if depth > 14 {
            return None;
        }
        for e in std::fs::read_dir(dir).ok()?.flatten() {
            let name = e.file_name().to_string_lossy().into_owned();
            let en = encode(&name);
            if en.is_empty() || !rest.starts_with(&en) {
                continue;
            }
            let p = e.path();
            if rest.len() == en.len() {
                if p.is_dir() {
                    return Some(p);
                }
            } else if rest.as_bytes()[en.len()] == b'-' && p.is_dir() {
                if let Some(found) = go(&p, &rest[en.len() + 1..], depth + 1) {
                    return Some(found);
                }
            }
        }
        None
    }
    let rest = enc.strip_prefix('-')?;
    go(Path::new("/"), rest, 0)
}

fn first_cwd(p: &Path) -> Option<PathBuf> {
    use std::io::{BufRead, BufReader};
    let f = std::fs::File::open(p).ok()?;
    for line in BufReader::new(f).lines().take(60).map_while(Result::ok) {
        if let Some(i) = line.find("\"cwd\":\"") {
            let rest = &line[i + 7..];
            if let Some(end) = rest.find('"') {
                return Some(PathBuf::from(rest[..end].replace("\\/", "/")));
            }
        }
    }
    None
}

/// Token counts from the last 7 days of local sessions (counts only, no content kept).
fn local_usage(cache: &mut HashMap<PathBuf, (SystemTime, u64, Vec<(i64, u64, u64)>)>) -> Local7 {
    let now = Utc::now().timestamp();
    let week = now - 7 * 86_400;
    let mut seen = std::collections::HashSet::new();
    if let Ok(rd) = std::fs::read_dir(projects_dir()) {
        for proj in rd.flatten() {
            let Ok(files) = std::fs::read_dir(proj.path()) else { continue };
            for f in files.flatten() {
                let p = f.path();
                if p.extension().map(|x| x != "jsonl").unwrap_or(true) {
                    continue;
                }
                let Ok(md) = f.metadata() else { continue };
                let Ok(mt) = md.modified() else { continue };
                let mts = mt.duration_since(SystemTime::UNIX_EPOCH).map(|d| d.as_secs() as i64).unwrap_or(0);
                if mts < week {
                    continue;
                }
                seen.insert(p.clone());
                let fresh = cache.get(&p).map(|(t, len, _)| *t == mt && *len == md.len()).unwrap_or(false);
                if !fresh {
                    let entries = read_usage(&p);
                    cache.insert(p, (mt, md.len(), entries));
                }
            }
        }
    }
    cache.retain(|k, _| seen.contains(k));
    let today_start = Local::now().date_naive().and_hms_opt(0, 0, 0).and_then(|d| d.and_local_timezone(Local).single()).map(|d| d.timestamp()).unwrap_or(now - 86_400);
    let mut l = Local7 { daily: vec![0; 7], ..Default::default() };
    for (_, _, entries) in cache.values() {
        for &(ts, tokens, out) in entries {
            if ts < week {
                continue;
            }
            l.tokens_7d += tokens;
            l.output_7d += out;
            if ts >= now - 5 * 3600 {
                l.tokens_5h += tokens;
            }
            if ts >= today_start {
                l.tokens_today += tokens;
                l.requests_today += 1;
            }
            let day = ((today_start - ts) as f64 / 86_400.0).ceil() as i64; // 0 = today
            let idx = 6 - day.clamp(0, 6);
            l.daily[idx as usize] += tokens;
        }
    }
    l
}

/// (timestamp, input+output+cache-write tokens, output tokens) for each assistant reply.
fn read_usage(p: &Path) -> Vec<(i64, u64, u64)> {
    use std::io::{BufRead, BufReader};
    let mut out = Vec::new();
    let Ok(f) = std::fs::File::open(p) else { return out };
    let mut seen_ids = std::collections::HashSet::new();
    for line in BufReader::with_capacity(1 << 20, f).split(b'\n').map_while(Result::ok) {
        let Some(ui) = find(&line, b"\"usage\":{") else { continue };
        let Some(ti) = find(&line, b"\"timestamp\":\"") else { continue };
        let ts_bytes = &line[ti + 13..];
        let Some(te) = ts_bytes.iter().position(|&b| b == b'"') else { continue };
        let ts = std::str::from_utf8(&ts_bytes[..te]).ok().and_then(|s| DateTime::parse_from_rfc3339(s).ok()).map(|d| d.timestamp());
        let Some(ts) = ts else { continue };
        // one reply can be logged in several lines; count each message id once
        if let Some(mi) = find(&line, b"\"id\":\"msg_") {
            let rest = &line[mi + 6..];
            if let Some(e) = rest.iter().position(|&b| b == b'"') {
                if !seen_ids.insert(rest[..e].to_vec()) {
                    continue;
                }
            }
        }
        let obj = balanced(&line[ui + 8..]);
        let v: Value = serde_json::from_slice(obj).unwrap_or(Value::Null);
        let g = |k: &str| v[k].as_u64().unwrap_or(0);
        let tokens = g("input_tokens") + g("output_tokens") + g("cache_creation_input_tokens");
        out.push((ts, tokens, g("output_tokens")));
    }
    out
}

fn find(hay: &[u8], needle: &[u8]) -> Option<usize> {
    hay.windows(needle.len()).position(|w| w == needle)
}

fn balanced(s: &[u8]) -> &[u8] {
    let mut depth = 0i32;
    for (i, &b) in s.iter().enumerate() {
        match b {
            b'{' => depth += 1,
            b'}' => {
                depth -= 1;
                if depth == 0 {
                    return &s[..=i];
                }
            }
            _ => {}
        }
    }
    s
}

pub fn human_tokens(n: u64) -> String {
    if n >= 1_000_000_000 {
        format!("{:.1}B", n as f64 / 1e9)
    } else if n >= 1_000_000 {
        format!("{:.1}M", n as f64 / 1e6)
    } else if n >= 1_000 {
        format!("{:.0}K", n as f64 / 1e3)
    } else {
        n.to_string()
    }
}

fn demo_info() -> ClaudeInfo {
    let (limits, contrib, note) = parse_usage(
        "You are currently using your subscription to power your Claude Code usage\n\n\
         Current session: 19% used · resets 9:10pm\n\
         Current week (all models): 5% used · resets Nov 8 at 5am\n\
         Current week (Opus): 0% used · resets Nov 8 at 5am\n\n\
         Last 24h · 637 requests · 8 sessions\n  95% of your usage was at >150k context\n  51% of your usage came from sessions active for 8+ hours\n",
    );
    ClaudeInfo {
        loading: false,
        installed: true,
        logged_in: true,
        plan: "max".into(),
        limits,
        contrib,
        note,
        error: None,
        fetched: Some(Local::now()),
        local: Local7 { tokens_5h: 1_250_000, tokens_today: 3_400_000, tokens_7d: 21_000_000, output_7d: 900_000, requests_today: 637, daily: vec![2_100_000, 3_900_000, 1_200_000, 4_400_000, 2_800_000, 3_200_000, 3_400_000] },
        projects: vec![],
        data_bytes: 5_400_000_000,
    }
}
