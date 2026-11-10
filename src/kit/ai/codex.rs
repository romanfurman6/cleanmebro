//! OpenAI Codex CLI as a `Provider`. Everything comes from its own files, no
//! CLI calls and no network: sign-in from `~/.codex/auth.json`, and both the
//! limit windows and token counts from the session logs in
//! `~/.codex/sessions/YYYY/MM/DD/rollout-*.jsonl`, whose `token_count`
//! events carry the tokens of each turn and the plan's rate limits as of
//! that turn.

use super::{find, recent_jsonl, Limit, LocalTokens, LogCache, Provider, Shared, UsageEntry, UsageInfo};
use crate::kit::activity::Kick;
use crate::kit::claude_code as cc;
use chrono::{DateTime, Local, TimeZone};
use serde_json::Value;
use std::collections::HashSet;
use std::path::{Path, PathBuf};

pub const NAME: &str = "Codex";

#[derive(Default)]
pub struct Codex {
    cache: LogCache,
}

/// Where Codex keeps its data (`$CODEX_HOME`, else `~/.codex`).
pub fn home() -> PathBuf {
    std::env::var_os("CODEX_HOME").map(PathBuf::from).unwrap_or_else(|| dirs::home_dir().unwrap_or_default().join(".codex"))
}

/// The `codex` command, if it's installed somewhere usual.
pub fn bin() -> Option<PathBuf> {
    let home = dirs::home_dir().unwrap_or_default();
    let mut dirs_: Vec<PathBuf> = std::env::var_os("PATH").map(|p| std::env::split_paths(&p).collect()).unwrap_or_default();
    dirs_.extend(["/opt/homebrew/bin", "/usr/local/bin"].map(PathBuf::from));
    dirs_.extend([".local/bin", ".npm-global/bin", ".bun/bin", ".volta/bin"].map(|d| home.join(d)));
    dirs_.into_iter().map(|d| d.join("codex")).find(|p| p.is_file())
}

impl Provider for Codex {
    fn name(&self) -> &'static str {
        NAME
    }
    // one switch and one refresh button for the whole tool
    fn enabled(&self) -> bool {
        cc::tool_on()
    }
    fn wake(&self) -> Option<&'static Kick> {
        Some(cc::recheck())
    }
    fn fetch_account(&mut self, s: &Shared) {
        let home = home();
        let installed = bin().is_some() || home.join("sessions").is_dir();
        let logged_in = home.join("auth.json").is_file();
        let latest = if installed { latest_limits(&home.join("sessions")) } else { None };
        let mut g = s.lock().unwrap();
        g.installed = installed;
        g.logged_in = logged_in;
        g.signed_out = installed && !logged_in;
        if !installed {
            g.limits.clear();
            g.error = Some("Codex isn't installed on this Mac. Install it with npm i -g @openai/codex or brew install codex, then run codex once to sign in.".into());
            return;
        }
        match latest {
            Some(l) => {
                g.error = None;
                if !l.plan.is_empty() {
                    g.plan = l.plan;
                }
                g.limits = l.limits;
                g.fetched = Some(l.at);
                g.note = "Codex reports its limits with each request, so these are as of your last one.".into();
            }
            None => {
                g.limits.clear();
                g.error = Some(if logged_in { "Codex shows your limits here after your next request.".into() } else { "Sign in to Codex to see your limits: run codex in Terminal.".into() });
            }
        }
    }
    fn local_tokens(&mut self) -> LocalTokens {
        self.cache.update(recent_jsonl(&home().join("sessions"), 3), usage_entry)
    }
    fn data_dir(&self) -> Option<PathBuf> {
        Some(home())
    }
    fn demo(&self) -> UsageInfo {
        demo_info()
    }
}

/// The limits in the newest session log.
struct Latest {
    plan: String,
    limits: Vec<Limit>,
    at: DateTime<Local>,
}

fn latest_limits(sessions: &Path) -> Option<Latest> {
    let mut files = recent_jsonl(sessions, 3);
    files.sort_by_key(|p| std::fs::metadata(p).and_then(|m| m.modified()).ok());
    // newest first; an old log may still hold limits when the newest has none yet
    files.iter().rev().take(5).find_map(|p| limits_in(&tail(p, 1 << 20)))
}

/// The last `max` bytes of a file, from the first whole line.
fn tail(p: &Path, max: u64) -> Vec<u8> {
    use std::io::{Read, Seek, SeekFrom};
    let Ok(mut f) = std::fs::File::open(p) else { return Vec::new() };
    let len = f.metadata().map(|m| m.len()).unwrap_or(0);
    let start = len.saturating_sub(max);
    if f.seek(SeekFrom::Start(start)).is_err() {
        return Vec::new();
    }
    let mut buf = Vec::new();
    let _ = f.read_to_end(&mut buf);
    if start > 0 {
        let cut = buf.iter().position(|&b| b == b'\n').map(|i| i + 1).unwrap_or(buf.len());
        buf.drain(..cut);
    }
    buf
}

/// The last `rate_limits` snapshot in a log.
fn limits_in(log: &[u8]) -> Option<Latest> {
    log.split(|&b| b == b'\n').rev().filter(|l| find(l, b"\"rate_limits\":{").is_some()).find_map(|line| {
        let v: Value = serde_json::from_slice(line).ok()?;
        let rl = &v["payload"]["rate_limits"];
        let at = v["timestamp"].as_str().and_then(|s| DateTime::parse_from_rfc3339(s).ok())?.with_timezone(&Local);
        let limits: Vec<Limit> = [&rl["primary"], &rl["secondary"]].into_iter().filter_map(|w| window(w, at)).collect();
        (!limits.is_empty()).then(|| Latest { plan: rl["plan_type"].as_str().unwrap_or("").to_string(), limits, at })
    })
}

/// One limit window, named the way the UI expects ("…session…", "…week…").
fn window(w: &Value, at: DateTime<Local>) -> Option<Limit> {
    let pct = w["used_percent"].as_f64()? as f32;
    let minutes = w["window_minutes"].as_i64().unwrap_or(0);
    let label = match minutes {
        m if (240..=360).contains(&m) => "Current session".to_string(),
        m if (9_000..=11_000).contains(&m) => "Current week (all models)".to_string(),
        m if m >= 60 => format!("Last {}h", m / 60),
        _ => "Limit".to_string(),
    };
    // newer logs say when it resets, older ones how long until then
    let resets = w["resets_at"].as_i64().or_else(|| w["resets_in_seconds"].as_i64().map(|s| at.timestamp() + s));
    let now = Local::now().timestamp();
    match resets {
        // the window rolled over since that request: nothing used in it yet
        Some(r) if r <= now => Some(Limit { label, pct: 0.0, resets: String::new() }),
        Some(r) => Some(Limit { label, pct, resets: reset_text(r) }),
        None => Some(Limit { label, pct, resets: String::new() }),
    }
}

/// "9:10pm" today, "Nov 8 at 5pm" on another day, like Claude's `/usage`.
fn reset_text(ts: i64) -> String {
    let Some(t) = Local.timestamp_opt(ts, 0).single() else { return String::new() };
    let time = if t.format("%M").to_string() == "00" { t.format("%-I%P").to_string() } else { t.format("%-I:%M%P").to_string() };
    if t.date_naive() == Local::now().date_naive() {
        time
    } else {
        // another day: the hour is enough
        format!("{} at {}", t.format("%b %-d"), t.format("%-I%P"))
    }
}

/// The tokens of one turn (fresh input + output; cached input isn't
/// counted, like cache reads for Claude). Codex can repeat a count, so
/// each running total counts once.
fn usage_entry(line: &[u8], seen: &mut HashSet<Vec<u8>>) -> Option<UsageEntry> {
    find(line, b"\"token_count\"")?;
    find(line, b"\"last_token_usage\"")?;
    let v: Value = serde_json::from_slice(line).ok()?;
    let info = &v["payload"]["info"];
    let total = info["total_token_usage"]["total_tokens"].as_i64()?;
    if !seen.insert(total.to_le_bytes().to_vec()) {
        return None;
    }
    let ts = v["timestamp"].as_str().and_then(|s| DateTime::parse_from_rfc3339(s).ok())?.timestamp();
    let last = &info["last_token_usage"];
    let g = |k: &str| last[k].as_i64().unwrap_or(0).max(0) as u64;
    let out = g("output_tokens");
    Some((ts, g("input_tokens").saturating_sub(g("cached_input_tokens")) + out, out))
}

pub fn demo_info() -> UsageInfo {
    let now = Local::now();
    let limits = vec![
        Limit { label: "Current session".into(), pct: 42.0, resets: reset_text(now.timestamp() + 2 * 3600 + 600) },
        Limit { label: "Current week (all models)".into(), pct: 18.0, resets: reset_text(now.timestamp() + 4 * 86_400) },
    ];
    UsageInfo {
        provider: NAME,
        installed: true,
        logged_in: true,
        plan: "plus".into(),
        limits,
        note: "Codex reports its limits with each request, so these are as of your last one.".into(),
        fetched: Some(now),
        local: LocalTokens { tokens_5h: 840_000, tokens_today: 1_900_000, tokens_7d: 9_600_000, output_7d: 410_000, requests_today: 212, daily: vec![900_000, 1_700_000, 600_000, 2_100_000, 1_300_000, 1_100_000, 1_900_000] },
        data_bytes: 1_300_000_000,
        ..Default::default()
    }
}

#[cfg(test)]
mod tests {
    use super::super::{read_on, LogTail};
    use super::*;

    fn token_line(ts: &str, total: i64, input: i64, cached: i64, out: i64, limits: &str) -> String {
        format!(
            "{{\"timestamp\":\"{ts}\",\"type\":\"event_msg\",\"payload\":{{\"type\":\"token_count\",\"info\":{{\"total_token_usage\":{{\"input_tokens\":1,\"cached_input_tokens\":0,\"output_tokens\":1,\"reasoning_output_tokens\":0,\"total_tokens\":{total}}},\"last_token_usage\":{{\"input_tokens\":{input},\"cached_input_tokens\":{cached},\"output_tokens\":{out},\"reasoning_output_tokens\":0,\"total_tokens\":0}},\"model_context_window\":272000}},\"rate_limits\":{limits}}}}}\n"
        )
    }

    #[test]
    fn tokens_count_once_per_total() {
        let dir = std::env::temp_dir().join(format!("cmb-codex-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let f = dir.join("rollout.jsonl");
        let body = "{\"timestamp\":\"2026-11-10T10:00:00Z\",\"type\":\"session_meta\",\"payload\":{\"cwd\":\"/tmp/p\"}}\n".to_string()
            + &token_line("2026-11-10T10:00:01Z", 100, 1000, 800, 50, "null")
            + &token_line("2026-11-10T10:00:02Z", 100, 1000, 800, 50, "null")
            + &token_line("2026-11-10T10:01:00Z", 300, 500, 0, 20, "null");
        std::fs::write(&f, &body).unwrap();
        let mut log = LogTail::default();
        read_on(&f, body.len() as u64, &mut log, usage_entry);
        std::fs::remove_dir_all(&dir).unwrap();
        assert_eq!(log.entries.len(), 2);
        assert_eq!(log.entries[0].1, 250);
        assert_eq!(log.entries[0].2, 50);
        assert_eq!(log.entries[1].1, 520);
    }

    #[test]
    fn limits_from_last_snapshot() {
        let future = Local::now().timestamp() + 3600;
        let past = Local::now().timestamp() - 60;
        let rl = |p: f64| format!("{{\"primary\":{{\"used_percent\":{p},\"window_minutes\":300,\"resets_at\":{future}}},\"secondary\":{{\"used_percent\":30.0,\"window_minutes\":10080,\"resets_at\":{past}}},\"plan_type\":\"pro\"}}");
        let log = token_line("2026-11-10T10:00:01Z", 1, 1, 0, 1, &rl(10.0)) + &token_line("2026-11-10T10:05:00Z", 2, 1, 0, 1, &rl(25.0)) + &token_line("2026-11-10T10:06:00Z", 3, 1, 0, 1, "null");
        let l = limits_in(log.as_bytes()).unwrap();
        assert_eq!(l.plan, "pro");
        assert_eq!(l.limits.len(), 2);
        assert_eq!(l.limits[0].label, "Current session");
        assert_eq!(l.limits[0].pct, 25.0);
        assert!(!l.limits[0].resets.is_empty());
        // the weekly window reset since: shown as unused
        assert_eq!(l.limits[1].label, "Current week (all models)");
        assert_eq!(l.limits[1].pct, 0.0);
        let mut info = demo_info();
        info.limits = l.limits;
        assert_eq!(info.session().unwrap().pct, 25.0);
        assert_eq!(info.weekly().unwrap().pct, 0.0);
    }

    #[test]
    fn old_logs_say_seconds_until_reset() {
        let w: Value = serde_json::from_str("{\"used_percent\":5.5,\"window_minutes\":299,\"resets_in_seconds\":600}").unwrap();
        let l = window(&w, Local::now()).unwrap();
        assert_eq!(l.pct, 5.5);
        assert!(!l.resets.is_empty());
        assert!(limits_in(b"{\"rate_limits\":{}}").is_none());
    }
}
