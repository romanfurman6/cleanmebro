//! Claude Code as a `Provider`: plan and limits come from the Claude CLI
//! (`claude auth status`, `claude -p /usage`, which spends no tokens), local
//! activity comes from counting usage fields in ~/.claude/projects logs.

use crate::kit::sync::LockExt;
use super::{balanced, find, recent_jsonl, Contrib, Limit, LocalTokens, LogCache, Provider, Shared, UsageEntry, UsageInfo};
use crate::kit::activity::Kick;
use crate::kit::claude_code::{self as cc, claude_bin, claude_cmd, projects_dir, scan_projects, Project};
use chrono::{DateTime, Local};
use serde_json::Value;
use std::collections::HashSet;
use std::path::PathBuf;

pub const NAME: &str = "Claude";

#[derive(Default)]
pub struct ClaudeCode {
    cache: LogCache,
}

impl Provider for ClaudeCode {
    fn name(&self) -> &'static str {
        NAME
    }
    fn enabled(&self) -> bool {
        cc::tool_on()
    }
    fn wake(&self) -> Option<&'static Kick> {
        Some(cc::recheck())
    }
    fn fetch_account(&mut self, s: &Shared) {
        fetch_cli(s);
    }
    fn local_tokens(&mut self) -> LocalTokens {
        // project folders hold the session logs one level down
        // <project>/<session>.jsonl, and subagents' own logs in
        // <project>/<session>/subagents/, which count toward the limits too
        self.cache.update(recent_jsonl(&projects_dir(), 3), usage_entry)
    }
    fn projects(&mut self) -> Vec<Project> {
        scan_projects()
    }
    fn data_dir(&self) -> Option<PathBuf> {
        dirs::home_dir().map(|h| h.join(".claude"))
    }
    fn demo(&self) -> UsageInfo {
        demo_info()
    }
}

fn fetch_cli(s: &Shared) {
    let Some(bin) = claude_bin() else {
        cc::set_signed_in(None);
        let mut g = s.lock_ok();
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
    let mut g = s.lock_ok();
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

/// The usage in one log line (input + output + cache-write tokens, output
/// tokens), unless its message was counted already. Cache reads don't count.
fn usage_entry(line: &[u8], ids: &mut HashSet<Vec<u8>>) -> Option<UsageEntry> {
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

pub fn demo_info() -> UsageInfo {
    let (limits, contrib, note) = parse_usage(
        "You are currently using your subscription to power your Claude Code usage\n\n\
         Current session: 19% used · resets 9:10pm\n\
         Current week (all models): 5% used · resets Nov 8 at 5am\n\
         Current week (Opus): 0% used · resets Nov 8 at 5am\n\n\
         Last 24h · 637 requests · 8 sessions\n  95% of your usage was at >150k context\n  51% of your usage came from sessions active for 8+ hours\n",
    );
    UsageInfo {
        provider: NAME,
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
        local: LocalTokens { tokens_5h: 1_250_000, tokens_today: 3_400_000, tokens_7d: 21_000_000, output_7d: 900_000, requests_today: 637, daily: vec![2_100_000, 3_900_000, 1_200_000, 4_400_000, 2_800_000, 3_200_000, 3_400_000] },
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
    use super::super::{read_on, LogTail};
    use super::*;
    use std::path::Path;

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
        let (l, c, n) = parse_usage("");
        assert!(l.is_empty() && c.is_empty() && n.is_empty());
    }

    #[test]
    fn a_resumed_session_copying_old_replies_counts_them_once() {
        let dir = std::env::temp_dir().join(format!("cmb-resume-{}", std::process::id()));
        std::fs::create_dir_all(dir.join("p/s1/subagents")).unwrap();
        let now = chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true);
        let line = |id: &str, out: u64| format!("{{\"timestamp\":\"{now}\",\"message\":{{\"id\":\"{id}\",\"usage\":{{\"input_tokens\":10,\"output_tokens\":{out}}}}}}}\n");
        std::fs::write(dir.join("p/s1.jsonl"), line("msg_a", 1) + &line("msg_b", 2)).unwrap();
        // the resumed session starts with a copy of s1, then goes on
        std::fs::write(dir.join("p/s2.jsonl"), line("msg_a", 1) + &line("msg_b", 2) + &line("msg_c", 3)).unwrap();
        // a subagent's own replies count too
        std::fs::write(dir.join("p/s1/subagents/agent-x.jsonl"), line("msg_d", 4)).unwrap();
        let mut cache = LogCache::default();
        let t = cache.update(recent_jsonl(&dir, 3), usage_entry);
        std::fs::remove_dir_all(&dir).unwrap();
        assert_eq!(t.output_7d, 1 + 2 + 3 + 4);
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
        let mut log = LogTail::default();
        let len = |f: &Path| std::fs::metadata(f).unwrap().len();
        read_on(&f, len(&f), &mut log, usage_entry);
        let cwd = crate::kit::claude_code::first_cwd(&f);
        assert_eq!(log.entries.len(), 2);
        assert_eq!(log.read, body.len() as u64);
        // cache reads are not counted
        assert_eq!(log.entries[0].1, 115);
        assert_eq!(log.entries[0].2, 5);
        assert_eq!(cwd, Some(PathBuf::from("/tmp/proj")));
        // the log grows: only the new lines are read, repeats still count once
        std::fs::write(&f, body.clone() + &partial + &line("msg_2")).unwrap();
        read_on(&f, len(&f), &mut log, usage_entry);
        assert_eq!(log.entries.len(), 3);
        // the log was replaced by a shorter one: read again from the start
        std::fs::write(&f, line("msg_9")).unwrap();
        read_on(&f, len(&f), &mut log, usage_entry);
        std::fs::remove_dir_all(&dir).unwrap();
        assert_eq!(log.entries.len(), 1);
    }
}
