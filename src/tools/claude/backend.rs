//! Claude info for the menu panel: plan and limits come from the Claude CLI
//! (`claude auth status`, `claude -p /usage`, which spends no tokens), local
//! activity comes from counting usage fields in ~/.claude/projects logs.

use crate::kit::claude_code::{claude_bin, claude_cmd, projects_dir, scan_projects, Project};
use crate::kit::mechanics as m;
use chrono::{DateTime, Local, Utc};
use serde_json::Value;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
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

/// One assistant reply: (unix time, counted tokens, output tokens).
type UsageEntry = (i64, u64, u64);
/// Parsed log files by path, with the modification time and length they were read at.
type UsageCache = HashMap<PathBuf, (SystemTime, u64, Vec<UsageEntry>)>;

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
            let mut cache: UsageCache = HashMap::new();
            // None = fetch now. (Subtracting from Instant::now() would panic
            // shortly after boot, when the monotonic clock is still small.)
            let mut last_limits: Option<Instant> = None;
            loop {
                let since = last_limits.map(|t| t.elapsed().as_secs());
                let want_limits = since.is_none_or(|s| s >= m::CLAUDE_LIMITS_REFRESH_SECS)
                    || s.lock().unwrap().limits.is_empty() && since.is_some_and(|s| s > 60);
                if want_limits {
                    s.lock().unwrap().loading = true;
                    ctx.request_repaint();
                    fetch_cli(&s);
                    last_limits = Some(Instant::now());
                }
                let projects = scan_projects();
                let local = local_usage(&mut cache);
                let data = crate::kit::fs::dir_size(&dirs::home_dir().unwrap_or_default().join(".claude"));
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
                        last_limits = None;
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
fn read_usage(p: &Path) -> Vec<UsageEntry> {
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
        std::fs::write(&f, body).unwrap();
        let entries = read_usage(&f);
        let cwd = crate::kit::claude_code::first_cwd(&f);
        std::fs::remove_dir_all(&dir).unwrap();
        assert_eq!(entries.len(), 2);
        // cache reads are not counted
        assert_eq!(entries[0].1, 115);
        assert_eq!(entries[0].2, 5);
        assert_eq!(cwd, Some(PathBuf::from("/tmp/proj")));
    }
}
