//! Claude info for the menu panel: plan and limits come from the Claude CLI
//! (`claude auth status`, `claude -p /usage`, which spends no tokens), local
//! activity comes from counting usage fields in ~/.claude/projects logs.

use crate::kit::activity::{self, Every, Wake};
use crate::kit::claude_code::{self as cc, claude_bin, claude_cmd, projects_dir, scan_projects, Project};
use crate::kit::mechanics as m;
use chrono::{DateTime, Local, Utc};
use serde_json::Value;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime};

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
    /// `claude auth status` said plainly that nobody is signed in.
    pub signed_out: bool,
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
    /// Claude Code is missing or signed out (known only after the first check).
    pub fn unavailable(&self) -> Option<cc::Availability> {
        if self.loading && self.fetched.is_none() {
            None
        } else if !self.installed {
            Some(cc::Availability::NotInstalled)
        } else if self.signed_out && self.limits.is_empty() {
            Some(cc::Availability::SignedOut)
        } else {
            None
        }
    }
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

/// One assistant reply: (unix time, counted tokens, output tokens).
type UsageEntry = (i64, u64, u64);
/// Parsed log files by path.
type UsageCache = HashMap<PathBuf, LogUsage>;

/// What one session log held so far. Logs only grow, so a longer log is
/// read on from where the last read stopped instead of from the start.
#[derive(Default)]
struct LogUsage {
    /// Modification time at the last read.
    mtime: Option<SystemTime>,
    /// Bytes read so far (whole lines only).
    read: u64,
    entries: Vec<UsageEntry>,
    /// Message ids seen, so a reply logged over several lines counts once.
    ids: std::collections::HashSet<Vec<u8>>,
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
            activity::low_priority();
            let mut cache: UsageCache = HashMap::new();
            // due right away the first time
            let mut limits_at = Every::default();
            let mut local_at = Every::default();
            let mut size_at = Every::default();
            loop {
                // turned off in Settings: no CLI calls, no log reading
                if !cc::tool_on() {
                    activity::nap(Duration::from_secs(3600), Some(cc::recheck()));
                    limits_at.reset();
                    continue;
                }
                // the limits also feed the menu-bar title, so they refresh hidden too
                let retry_empty = s.lock().unwrap().limits.is_empty() && limits_at.age().is_some_and(|a| a.as_secs() > 60);
                if retry_empty || limits_at.due(Duration::from_secs(m::CLAUDE_LIMITS_REFRESH_SECS)) {
                    limits_at.mark();
                    s.lock().unwrap().loading = true;
                    activity::repaint(&ctx);
                    fetch_cli(&s);
                }
                // the rest is only shown in the panel and the window: hidden,
                // it waits until they open (the nap below ends then)
                if (activity::visible() || local_at.age().is_none()) && local_at.due(Duration::from_secs(m::CLAUDE_LOCAL_REFRESH_SECS)) {
                    let projects = scan_projects();
                    let local = local_usage(&mut cache);
                    // ~/.claude walks every file: once per launch, and again on Refresh
                    let data = (size_at.age().is_none() && !activity::recording()).then(|| {
                        size_at.mark();
                        crate::kit::fs::dir_size(&dirs::home_dir().unwrap_or_default().join(".claude"))
                    });
                    let mut g = s.lock().unwrap();
                    g.projects = projects;
                    g.local = local;
                    if let Some(d) = data {
                        g.data_bytes = d;
                    }
                }
                s.lock().unwrap().loading = false;
                activity::repaint(&ctx);
                // wait, but wake early when someone asks for a refresh or the
                // panel opens
                if activity::nap(Duration::from_secs(m::CLAUDE_LOCAL_REFRESH_SECS), Some(cc::recheck())) == Wake::Kicked {
                    limits_at.reset();
                    local_at.reset();
                    size_at.reset();
                }
            }
        })
        .ok();
    shared
}

pub fn request_refresh() {
    cc::request_recheck();
}

fn fetch_cli(s: &Shared) {
    let Some(bin) = claude_bin() else {
        cc::set_signed_in(None);
        let mut g = s.lock().unwrap();
        g.installed = false;
        g.signed_out = false;
        g.limits.clear();
        g.error = Some(format!("Claude Code isn't installed on this Mac. {}", cc::Availability::NotInstalled.how()));
        return;
    };
    // plan + login
    let status = claude_cmd(&bin).args(["auth", "status", "--json"]).output();
    let (known, plan) = match status {
        Ok(o) => {
            let v: Value = serde_json::from_slice(&o.stdout).unwrap_or(Value::Null);
            (v["loggedIn"].as_bool(), v["subscriptionType"].as_str().unwrap_or("").to_string())
        }
        Err(_) => (None, String::new()),
    };
    let logged_in = known.unwrap_or(false);
    cc::set_signed_in(known);
    // limits (a local slash command: no model call, no tokens)
    let usage = claude_cmd(&bin)
        .args(["-p", "/usage", "--output-format", "json", "--no-session-persistence"])
        .output();
    let mut g = s.lock().unwrap();
    g.installed = true;
    g.logged_in = logged_in;
    g.signed_out = known == Some(false);
    if !plan.is_empty() {
        g.plan = plan;
    }
    match usage {
        Ok(o) => {
            let v: Value = serde_json::from_slice(&o.stdout).unwrap_or(Value::Null);
            let text = v["result"].as_str().unwrap_or("").to_string();
            let (limits, contrib, note) = parse_usage(&text);
            if limits.is_empty() {
                g.error = Some(if logged_in { "Claude didn't report limits right now.".into() } else { format!("Sign in to Claude Code to see your limits. {}", cc::Availability::SignedOut.how()) });
                if g.signed_out {
                    g.limits.clear();
                }
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

/// Token counts from the last 7 days of local sessions (counts only, no content kept).
fn local_usage(cache: &mut UsageCache) -> Local7 {
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
                let log = cache.entry(p).or_default();
                if log.mtime != Some(mt) || log.read != md.len() {
                    read_usage(&f.path(), md.len(), log);
                    log.mtime = Some(mt);
                }
            }
        }
    }
    cache.retain(|k, _| seen.contains(k));
    let today_start = Local::now().date_naive().and_hms_opt(0, 0, 0).and_then(|d| d.and_local_timezone(Local).single()).map(|d| d.timestamp()).unwrap_or(now - 86_400);
    let mut l = Local7 { daily: vec![0; 7], ..Default::default() };
    for log in cache.values() {
        for &(ts, tokens, out) in &log.entries {
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

/// Read on in a session log of `len` bytes from where `log` stopped,
/// adding (timestamp, input+output+cache-write tokens, output tokens) for
/// each assistant reply. A log that got shorter is read again from the start.
fn read_usage(p: &Path, len: u64, log: &mut LogUsage) {
    use std::io::{BufRead, BufReader, Seek, SeekFrom};
    if len < log.read {
        *log = LogUsage::default();
    }
    let Ok(mut f) = std::fs::File::open(p) else { return };
    if f.seek(SeekFrom::Start(log.read)).is_err() {
        return;
    }
    let mut r = BufReader::with_capacity(1 << 20, f);
    let mut line = Vec::new();
    loop {
        line.clear();
        match r.read_until(b'\n', &mut line) {
            Ok(0) | Err(_) => break,
            Ok(_) => {}
        }
        // a line still being written is read next time, whole
        if line.last() != Some(&b'\n') {
            break;
        }
        log.read += line.len() as u64;
        if let Some(e) = usage_entry(&line, &mut log.ids) {
            log.entries.push(e);
        }
    }
}

/// The usage in one log line, unless its message was counted already.
fn usage_entry(line: &[u8], ids: &mut std::collections::HashSet<Vec<u8>>) -> Option<UsageEntry> {
    let ui = find(line, b"\"usage\":{")?;
    let ti = find(line, b"\"timestamp\":\"")?;
    let ts_bytes = &line[ti + 13..];
    let te = ts_bytes.iter().position(|&b| b == b'"')?;
    let ts = std::str::from_utf8(&ts_bytes[..te]).ok().and_then(|s| DateTime::parse_from_rfc3339(s).ok()).map(|d| d.timestamp())?;
    // one reply can be logged in several lines; count each message id once
    if let Some(mi) = find(line, b"\"id\":\"msg_") {
        let rest = &line[mi + 6..];
        if let Some(e) = rest.iter().position(|&b| b == b'"') {
            if !ids.insert(rest[..e].to_vec()) {
                return None;
            }
        }
    }
    let obj = balanced(&line[ui + 8..]);
    let v: Value = serde_json::from_slice(obj).unwrap_or(Value::Null);
    let g = |k: &str| v[k].as_u64().unwrap_or(0);
    let tokens = g("input_tokens") + g("output_tokens") + g("cache_creation_input_tokens");
    Some((ts, tokens, g("output_tokens")))
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
        signed_out: false,
        plan: "max".into(),
        limits,
        contrib,
        note,
        error: None,
        fetched: Some(Local::now()),
        local: Local7 { tokens_5h: 1_250_000, tokens_today: 3_400_000, tokens_7d: 21_000_000, output_7d: 900_000, requests_today: 637, daily: vec![2_100_000, 3_900_000, 1_200_000, 4_400_000, 2_800_000, 3_200_000, 3_400_000] },
        projects: demo_projects(),
        data_bytes: 5_400_000_000,
    }
}

/// Sample Claude Code projects for screenshots.
fn demo_projects() -> Vec<Project> {
    let home = dirs::home_dir().unwrap_or_default();
    let ago = |h: u64| Some(std::time::SystemTime::now() - std::time::Duration::from_secs(h * 3600));
    [("Projects/cmdmeow", true, 0, 1_200_000_000), ("Projects/site", true, 3, 640_000_000), ("notes", true, 50, 88_000_000), ("Projects/old-experiment", false, 900, 412_000_000)]
        .into_iter()
        .map(|(rel, exists, hours, bytes)| Project { folder: home.join(".claude/projects").join(rel.replace('/', "-")), cwd: Some(home.join(rel)), cwd_exists: exists, last_used: ago(hours), bytes })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn usage_text() {
        let info = demo_info();
        assert_eq!(info.limits.len(), 3);
        let s = info.session().unwrap();
        assert_eq!(s.pct, 19.0);
        assert_eq!(s.resets, "9:10pm");
        assert_eq!(info.weekly().unwrap().label, "Current week (all models)");
        assert_eq!(info.contrib.len(), 1);
        assert_eq!(info.contrib[0].lines.len(), 2);
        assert!(info.note.starts_with("You are currently"));
        assert_eq!(info.plan_title(), "Claude Max");
        assert_eq!(ClaudeInfo::default().plan_title(), "Claude");
        let (l, c, n) = parse_usage("");
        assert!(l.is_empty() && c.is_empty() && n.is_empty());
    }

    #[test]
    fn limit_labels() {
        assert_eq!(short_label("Current session"), "Current session (5h)");
        assert_eq!(short_label("Current week (all models)"), "This week · all models");
        assert_eq!(short_label("Current week (Opus)"), "This week · Opus");
        assert_eq!(short_label("Something else"), "Something else");
    }

    #[test]
    fn token_formatting() {
        assert_eq!(human_tokens(999), "999");
        assert_eq!(human_tokens(12_345), "12K");
        assert_eq!(human_tokens(3_400_000), "3.4M");
        assert_eq!(human_tokens(2_000_000_000), "2.0B");
    }

    #[test]
    fn balanced_object() {
        assert_eq!(balanced(b"{\"a\":{\"b\":1}},\"x\":2}"), b"{\"a\":{\"b\":1}}");
        assert_eq!(balanced(b"{open"), b"{open");
    }

    #[test]
    fn usage_lines_counted_once_per_message() {
        let dir = std::env::temp_dir().join(format!("cmb-usage-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let f = dir.join("s.jsonl");
        let line = |id: &str| format!(
            "{{\"cwd\":\"/tmp/proj\",\"timestamp\":\"2026-11-08T10:00:00Z\",\"message\":{{\"id\":\"{id}\",\"usage\":{{\"input_tokens\":10,\"output_tokens\":5,\"cache_creation_input_tokens\":100,\"cache_read_input_tokens\":9999}}}}}}\n"
        );
        let body = line("msg_1") + &line("msg_1") + &line("msg_2") + "{\"type\":\"user\"}\n";
        // a half-written last line waits for the next read
        let partial = line("msg_3");
        std::fs::write(&f, body.clone() + &partial[..40]).unwrap();
        let mut log = LogUsage::default();
        let len = |f: &Path| std::fs::metadata(f).unwrap().len();
        read_usage(&f, len(&f), &mut log);
        let cwd = crate::kit::claude_code::first_cwd(&f);
        assert_eq!(log.entries.len(), 2);
        assert_eq!(log.read, body.len() as u64);
        // cache reads are not counted
        assert_eq!(log.entries[0].1, 115);
        assert_eq!(log.entries[0].2, 5);
        assert_eq!(cwd, Some(PathBuf::from("/tmp/proj")));
        // the log grows: only the new lines are read, repeats still count once
        std::fs::write(&f, body.clone() + &partial + &line("msg_2")).unwrap();
        read_usage(&f, len(&f), &mut log);
        assert_eq!(log.entries.len(), 3);
        // the log was replaced by a shorter one: read again from the start
        std::fs::write(&f, line("msg_9")).unwrap();
        read_usage(&f, len(&f), &mut log);
        std::fs::remove_dir_all(&dir).unwrap();
        assert_eq!(log.entries.len(), 1);
    }
}
