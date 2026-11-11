//! AI coding assistants on this Mac, behind one `Provider` trait: plan and
//! limit windows, local token counts from session logs, and the projects
//! the assistant worked in. Each provider fills the same `UsageInfo`, and
//! whatever it can't tell stays empty, so one UI shows any of them.

pub mod claude;
pub mod codex;

use crate::kit::activity::Kick;
use crate::kit::claude_code::{Availability, Project};
use chrono::{DateTime, Local, Utc};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::SystemTime;

/// One limit window, e.g. "Current session" at 19%, resets 9:10pm.
#[derive(Clone, Debug, Default)]
pub struct Limit {
    pub label: String,
    pub pct: f32,
    pub resets: String,
}

/// A block of extra lines about where usage came from.
#[derive(Clone, Debug, Default)]
pub struct Contrib {
    pub title: String,
    pub lines: Vec<String>,
}

/// Tokens counted from local session logs over the last 7 days.
#[derive(Clone, Debug, Default)]
pub struct LocalTokens {
    pub tokens_5h: u64,
    pub tokens_today: u64,
    pub tokens_7d: u64,
    pub output_7d: u64,
    pub requests_today: u64,
    pub daily: Vec<u64>, // 7 buckets, oldest first
}

/// Everything one provider reported.
#[derive(Clone, Debug, Default)]
pub struct UsageInfo {
    /// The provider's display name, e.g. "Claude".
    pub provider: &'static str,
    pub loading: bool,
    pub installed: bool,
    pub logged_in: bool,
    /// The provider said plainly that nobody is signed in.
    pub signed_out: bool,
    pub plan: String,
    /// Limit windows. Providers name the 5-hour one "…session…" and the
    /// weekly ones "…week…", so `session()` and `weekly()` find them.
    pub limits: Vec<Limit>,
    pub contrib: Vec<Contrib>,
    pub note: String,
    pub error: Option<String>,
    pub fetched: Option<DateTime<Local>>,
    pub local: LocalTokens,
    pub projects: Vec<Project>,
    pub data_bytes: u64,
}

impl UsageInfo {
    pub fn new(provider: &'static str) -> Self {
        UsageInfo { provider, ..Default::default() }
    }
    /// The assistant is missing or signed out (known only after the first check).
    pub fn unavailable(&self) -> Option<Availability> {
        if self.loading && self.fetched.is_none() {
            None
        } else if !self.installed {
            Some(Availability::NotInstalled)
        } else if self.signed_out && self.limits.is_empty() {
            Some(Availability::SignedOut)
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
    /// "Claude Max", or just the provider name when the plan is unknown.
    pub fn plan_title(&self) -> String {
        if self.plan.is_empty() {
            return self.provider.into();
        }
        let mut p = self.plan.clone();
        if let Some(f) = p.get_mut(0..1) {
            f.make_ascii_uppercase();
        }
        format!("{} {p}", self.provider)
    }
}

pub type Shared = Arc<Mutex<UsageInfo>>;

/// One AI coding assistant. Calls run on the tool's background thread.
pub trait Provider: Send {
    fn name(&self) -> &'static str;
    /// Turned on in Settings. Off: no CLI calls and no log reading.
    fn enabled(&self) -> bool {
        true
    }
    /// Kicked when someone asks to check again (refresh, sign-in, switch).
    fn wake(&self) -> Option<&'static Kick> {
        None
    }
    /// Plan, sign-in and limit windows, written into `s`. May run a CLI, so
    /// it locks `s` only to write the result.
    fn fetch_account(&mut self, s: &Shared);
    /// Token counts from local session logs.
    fn local_tokens(&mut self) -> LocalTokens;
    /// Projects the assistant worked in.
    fn projects(&mut self) -> Vec<Project> {
        Vec::new()
    }
    /// The folder where the assistant keeps its data, for the size shown.
    fn data_dir(&self) -> Option<PathBuf> {
        None
    }
    /// Sample data for screenshots and demos.
    fn demo(&self) -> UsageInfo;
}

// ---------------------------------------------------------------- providers

/// What the UI says about a provider when it's missing or signed out.
pub struct Meta {
    pub id: &'static str,
    pub name: &'static str,
    /// The command people type, e.g. "claude".
    pub cli: &'static str,
    pub install_url: &'static str,
    pub install_steps: &'static [&'static str],
    pub sign_in_steps: &'static [&'static str],
}

pub const PROVIDERS: &[Meta] = &[
    Meta {
        id: "claude",
        name: claude::NAME,
        cli: "claude",
        install_url: "https://claude.com/claude-code",
        install_steps: &["Install Claude Code from claude.com/claude-code", "Open Terminal and run claude once to sign in", "Come back and press Check again"],
        sign_in_steps: &["Open Terminal and run claude", "Sign in with your Claude account", "Come back and press Check again"],
    },
    Meta {
        id: "codex",
        name: codex::NAME,
        cli: "codex",
        install_url: "https://developers.openai.com/codex/cli",
        install_steps: &["Install Codex: npm i -g @openai/codex or brew install codex", "Open Terminal and run codex once to sign in", "Come back and press Check again"],
        sign_in_steps: &["Open Terminal and run codex", "Sign in with your ChatGPT account", "Come back and press Check again"],
    },
];

/// The provider the person picked, as an index into `PROVIDERS`.
static SELECTED: AtomicUsize = AtomicUsize::new(usize::MAX);

#[derive(serde::Serialize, serde::Deserialize, Default)]
struct Choice {
    #[serde(default)]
    provider: String,
}

fn choice_path() -> PathBuf {
    crate::kit::fs::data_path("ai").join("settings.json")
}

/// Index into `PROVIDERS` of the main provider (Claude until one is picked).
pub fn selected() -> usize {
    let i = SELECTED.load(Ordering::Relaxed);
    if i != usize::MAX {
        return i;
    }
    let c: Choice = crate::kit::fs::load_json(&choice_path());
    let i = index_of(&c.provider);
    SELECTED.store(i, Ordering::Relaxed);
    i
}

pub fn index_of(id: &str) -> usize {
    PROVIDERS.iter().position(|m| m.id == id).unwrap_or(0)
}

pub fn selected_meta() -> &'static Meta {
    &PROVIDERS[selected()]
}

/// The meta for a provider by display name (Claude when unknown).
pub fn meta(name: &str) -> &'static Meta {
    PROVIDERS.iter().find(|m| m.name == name).unwrap_or(&PROVIDERS[0])
}

/// Make `i` the main provider, save it (unless `save` is false, for
/// demos), and wake the loop to switch.
pub fn select(i: usize, save: bool) {
    let i = i.min(PROVIDERS.len() - 1);
    if SELECTED.swap(i, Ordering::Relaxed) == i || !save {
        return;
    }
    crate::kit::fs::save_json(&choice_path(), &Choice { provider: PROVIDERS[i].id.into() }, true);
    crate::kit::claude_code::request_recheck();
}

/// A fresh provider for index `i`.
pub fn make(i: usize) -> Box<dyn Provider> {
    match PROVIDERS.get(i).map(|m| m.id) {
        Some("codex") => Box::new(codex::Codex::default()),
        _ => Box::new(claude::ClaudeCode::default()),
    }
}

// ---------------------------------------------------------------- session logs

/// One reply: (unix time, counted tokens, output tokens).
pub type UsageEntry = (i64, u64, u64);

/// What one append-only session log held so far. Logs only grow, so a
/// longer log is read on from where the last read stopped.
#[derive(Default)]
pub struct LogTail {
    /// Modification time at the last read.
    pub mtime: Option<SystemTime>,
    /// Bytes read so far (whole lines only).
    pub read: u64,
    pub entries: Vec<UsageEntry>,
    /// Message ids seen, so a reply logged over several lines counts once.
    pub ids: HashSet<Vec<u8>>,
}

/// Parsed logs by path.
#[derive(Default)]
pub struct LogCache(HashMap<PathBuf, LogTail>);

impl LogCache {
    /// Bring the cache up to date with `files` (logs touched in the last 7
    /// days), reading each line of new bytes with `parse`, and drop logs
    /// that are gone. Then sum the entries.
    pub fn update(&mut self, files: Vec<PathBuf>, parse: fn(&[u8], &mut HashSet<Vec<u8>>) -> Option<UsageEntry>) -> LocalTokens {
        let mut seen = HashSet::new();
        for p in files {
            let Ok(md) = std::fs::metadata(&p) else { continue };
            let Ok(mt) = md.modified() else { continue };
            seen.insert(p.clone());
            let log = self.0.entry(p.clone()).or_default();
            if log.mtime != Some(mt) || log.read != md.len() {
                read_on(&p, md.len(), log, parse);
                log.mtime = Some(mt);
            }
        }
        self.0.retain(|k, _| seen.contains(k));
        // a resumed or forked session starts its log with a copy of the old
        // one: the same reply (same time, same counts) in two files counts once
        let mut once = HashSet::new();
        sum(self.0.values().flat_map(|l| l.entries.iter().copied()).filter(|e| once.insert(*e)))
    }
}

/// `.jsonl` files under `dir` (down to `depth` folder levels) changed in the last 7 days.
pub fn recent_jsonl(dir: &Path, depth: usize) -> Vec<PathBuf> {
    let week = Utc::now().timestamp() - 7 * 86_400;
    let mut out = Vec::new();
    fn walk(d: &Path, depth: usize, week: i64, out: &mut Vec<PathBuf>) {
        let Ok(rd) = std::fs::read_dir(d) else { return };
        for e in rd.flatten() {
            let p = e.path();
            let Ok(ft) = e.file_type() else { continue };
            if ft.is_dir() {
                if depth > 0 {
                    walk(&p, depth - 1, week, out);
                }
                continue;
            }
            if p.extension().map(|x| x != "jsonl").unwrap_or(true) {
                continue;
            }
            let mts = e.metadata().ok().and_then(|m| m.modified().ok()).and_then(|t| t.duration_since(SystemTime::UNIX_EPOCH).ok()).map(|d| d.as_secs() as i64).unwrap_or(0);
            if mts >= week {
                out.push(p);
            }
        }
    }
    walk(dir, depth, week, &mut out);
    out
}

/// Read on in a log of `len` bytes from where `log` stopped, adding an
/// entry for each line `parse` accepts. A log that got shorter is read
/// again from the start.
pub fn read_on(p: &Path, len: u64, log: &mut LogTail, parse: fn(&[u8], &mut HashSet<Vec<u8>>) -> Option<UsageEntry>) {
    use std::io::{BufRead, BufReader, Seek, SeekFrom};
    if len < log.read {
        *log = LogTail::default();
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
        if let Some(e) = parse(&line, &mut log.ids) {
            log.entries.push(e);
        }
    }
}

/// Sum entries into 5-hour, today, 7-day and per-day totals.
pub fn sum(entries: impl Iterator<Item = UsageEntry>) -> LocalTokens {
    let now = Utc::now().timestamp();
    let week = now - 7 * 86_400;
    let today_start = Local::now().date_naive().and_hms_opt(0, 0, 0).and_then(|d| d.and_local_timezone(Local).single()).map(|d| d.timestamp()).unwrap_or(now - 86_400);
    let mut l = LocalTokens { daily: vec![0; 7], ..Default::default() };
    for (ts, tokens, out) in entries {
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
    l
}

// ---------------------------------------------------------------- formatting

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

pub fn find(hay: &[u8], needle: &[u8]) -> Option<usize> {
    hay.windows(needle.len()).position(|w| w == needle)
}

/// The JSON object at the start of `s`, up to its closing brace.
pub fn balanced(s: &[u8]) -> &[u8] {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plan_titles() {
        let mut i = UsageInfo::new("Claude");
        assert_eq!(i.plan_title(), "Claude");
        i.plan = "max".into();
        assert_eq!(i.plan_title(), "Claude Max");
    }

    #[test]
    fn providers_by_id_and_name() {
        assert_eq!(PROVIDERS[index_of("codex")].name, "Codex");
        assert_eq!(index_of("nope"), 0);
        assert_eq!(meta("Codex").cli, "codex");
        assert_eq!(meta("").id, "claude");
        for (i, m) in PROVIDERS.iter().enumerate() {
            assert_eq!(make(i).name(), m.name);
        }
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
    fn sums_by_window() {
        let now = Utc::now().timestamp();
        let l = sum([(now - 60, 10, 1), (now - 8 * 86_400, 99, 9)].into_iter());
        assert_eq!(l.tokens_5h, 10);
        assert_eq!(l.tokens_7d, 10);
        assert_eq!(l.output_7d, 1);
        assert_eq!(l.daily[6], l.tokens_today);
    }
}
