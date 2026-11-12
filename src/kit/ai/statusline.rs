//! Live Claude limits from Claude Code's own status line. Claude Code runs
//! a status-line command after each reply and hands it a JSON summary on
//! stdin, including `rate_limits` (5-hour and 7-day use and reset times).
//! The hook is a tiny script in the app's data folder: it saves that JSON
//! and then runs the status line the person had before, so their status
//! line looks the same. Removing the hook puts the old one back.

use super::Limit;
use serde_json::{json, Value};
use std::path::{Path, PathBuf};

fn dir() -> PathBuf {
    crate::kit::fs::data_path("ai")
}

/// The hook script in `dir`.
fn script(dir: &Path) -> PathBuf {
    dir.join("statusline.sh")
}

/// The person's previous status line, run after saving.
fn prev_script(dir: &Path) -> PathBuf {
    dir.join("statusline-prev.sh")
}

/// What Claude Code last handed the hook.
pub fn capture() -> PathBuf {
    dir().join("claude-status.json")
}

fn claude_settings() -> PathBuf {
    dirs::home_dir().unwrap_or_default().join(".claude").join("settings.json")
}

const SCRIPT: &str = r#"#!/bin/sh
# Cmd+Meow: saves what Claude Code reports (plan limits) for the menu bar,
# then shows the status line you had before.
dir="$(cd "$(dirname "$0")" && pwd)"
input="$(cat)"
printf '%s' "$input" > "$dir/claude-status.json.tmp" && mv -f "$dir/claude-status.json.tmp" "$dir/claude-status.json"
if [ -f "$dir/statusline-prev.sh" ]; then
  printf '%s' "$input" | /bin/sh "$dir/statusline-prev.sh"
fi
"#;

/// The command Claude Code runs for the hook (quoted: the path has spaces).
fn command(dir: &Path) -> String {
    format!("\"{}\"", script(dir).display())
}

/// The hook is set in Claude Code's settings.
pub fn installed() -> bool {
    is_ours(&read_json(&claude_settings())["statusLine"], &dir())
}

fn is_ours(status_line: &Value, dir: &Path) -> bool {
    status_line["command"].as_str().is_some_and(|c| c.contains("statusline.sh") && c.contains(&*dir.to_string_lossy()))
}

fn read_json(p: &Path) -> Value {
    std::fs::read(p).ok().and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or(Value::Null)
}

/// Set the hook in `~/.claude/settings.json`, keeping every other setting
/// and running the previous status line (if any) after it.
pub fn install() -> Result<(), String> {
    install_at(&claude_settings(), &dir())
}

fn install_at(settings: &Path, dir: &Path) -> Result<(), String> {
    let mut v = read_json(settings);
    if settings.exists() && !v.is_object() {
        return Err("Claude Code's settings.json isn't valid JSON, so it was left alone.".into());
    }
    if !v.is_object() {
        v = json!({});
    }
    std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    if !is_ours(&v["statusLine"], dir) {
        // keep the person's own status line running after ours
        match v["statusLine"]["command"].as_str() {
            Some(cmd) if !cmd.trim().is_empty() => std::fs::write(prev_script(dir), format!("#!/bin/sh\n{cmd}\n")).map_err(|e| e.to_string())?,
            _ => {
                let _ = std::fs::remove_file(prev_script(dir));
            }
        }
        if settings.exists() {
            let _ = std::fs::copy(settings, settings.with_extension("json.cmdmeow-backup"));
        }
    }
    std::fs::write(script(dir), SCRIPT).map_err(|e| e.to_string())?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(script(dir), std::fs::Permissions::from_mode(0o755));
    }
    let padding = v["statusLine"]["padding"].clone();
    let mut line = json!({ "type": "command", "command": command(dir) });
    if !padding.is_null() {
        line["padding"] = padding;
    }
    v["statusLine"] = line;
    write_json(settings, &v)
}

/// Take the hook out and put the previous status line back.
pub fn remove() -> Result<(), String> {
    remove_at(&claude_settings(), &dir())
}

fn remove_at(settings: &Path, dir: &Path) -> Result<(), String> {
    let mut v = read_json(settings);
    if !v.is_object() || !is_ours(&v["statusLine"], dir) {
        return Ok(());
    }
    match std::fs::read_to_string(prev_script(dir)).ok().and_then(|s| s.strip_prefix("#!/bin/sh\n").map(|c| c.trim_end().to_string())) {
        Some(cmd) if !cmd.is_empty() => v["statusLine"] = json!({ "type": "command", "command": cmd }),
        _ => {
            if let Some(o) = v.as_object_mut() {
                o.remove("statusLine");
            }
        }
    }
    let _ = std::fs::remove_file(prev_script(dir));
    write_json(settings, &v)
}

fn write_json(p: &Path, v: &Value) -> Result<(), String> {
    if let Some(d) = p.parent() {
        std::fs::create_dir_all(d).map_err(|e| e.to_string())?;
    }
    let body = serde_json::to_vec_pretty(v).map_err(|e| e.to_string())?;
    let tmp = p.with_extension("json.cmdmeow-tmp");
    std::fs::write(&tmp, body).map_err(|e| e.to_string())?;
    std::fs::rename(&tmp, p).map_err(|e| e.to_string())
}

/// The limits in the last capture, and when it was saved (unix time).
pub fn latest() -> Option<(i64, Vec<Limit>)> {
    let p = capture();
    let at = std::fs::metadata(&p).ok()?.modified().ok()?.duration_since(std::time::UNIX_EPOCH).ok()?.as_secs() as i64;
    let limits = parse(&std::fs::read(&p).ok()?, chrono::Utc::now().timestamp());
    (!limits.is_empty()).then_some((at, limits))
}

/// The limit windows in a status-line JSON, named like `/usage` names them.
pub fn parse(json: &[u8], now: i64) -> Vec<Limit> {
    let v: Value = serde_json::from_slice(json).unwrap_or(Value::Null);
    let rl = &v["rate_limits"];
    let windows = [
        ("five_hour", "Current session", 5 * 3600),
        ("seven_day", "Current week (all models)", 7 * 86_400),
        ("seven_day_opus", "Current week (Opus)", 7 * 86_400),
        ("seven_day_sonnet", "Current week (Sonnet)", 7 * 86_400),
    ];
    windows
        .iter()
        .filter_map(|(key, label, secs)| {
            let w = &rl[*key];
            let pct = w["used_percentage"].as_f64().or_else(|| w["utilization"].as_f64())? as f32;
            if !pct.is_finite() || !(0.0..=1000.0).contains(&pct) {
                return None;
            }
            let resets_at = w["resets_at"].as_i64().or_else(|| w["resets_at"].as_f64().map(|f| f as i64)).or_else(|| w["resets_at"].as_str().and_then(|s| chrono::DateTime::parse_from_rfc3339(s).ok()).map(|d| d.timestamp()));
            // a window that rolled over since the capture is unused
            let (pct, resets_at) = match resets_at {
                Some(r) if r <= now => (0.0, None),
                r => (pct, r),
            };
            let resets = resets_at.map(super::codex::reset_text).unwrap_or_default();
            Some(Limit { label: label.to_string(), pct, resets, resets_at, window_secs: *secs })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn limits_from_the_status_json() {
        let now = 1_000_000;
        let j = format!(r#"{{"model":{{"display_name":"Opus"}},"rate_limits":{{"five_hour":{{"used_percentage":42.5,"resets_at":{}}},"seven_day":{{"used_percentage":18,"resets_at":{}}},"seven_day_opus":{{"used_percentage":3,"resets_at":{}}}}}}}"#, now + 600, now + 86_400, now - 5);
        let l = parse(j.as_bytes(), now);
        assert_eq!(l.len(), 3);
        assert_eq!((l[0].label.as_str(), l[0].pct, l[0].resets_at, l[0].window_secs), ("Current session", 42.5, Some(now + 600), 5 * 3600));
        assert_eq!(l[1].pct, 18.0);
        // reset since: unused
        assert_eq!((l[2].pct, l[2].resets_at), (0.0, None));
        assert!(parse(b"{\"model\":{}}", now).is_empty());
        assert!(parse(b"{\"rate_limits\":{\"five_hour\":\"bad\"}}", now).is_empty());
    }

    #[test]
    fn install_keeps_settings_and_the_old_status_line() {
        let d = std::env::temp_dir().join(format!("cmb-statusline-{}", std::process::id()));
        std::fs::create_dir_all(&d).unwrap();
        let f = d.join("settings.json");
        let hook = d.join("app data");
        std::fs::write(&f, r#"{"model":"opus","statusLine":{"type":"command","command":"~/bin/my-line","padding":1}}"#).unwrap();
        install_at(&f, &hook).unwrap();
        let v = read_json(&f);
        assert_eq!(v["model"], "opus");
        assert!(is_ours(&v["statusLine"], &hook));
        assert_eq!(v["statusLine"]["padding"], 1);
        assert!(script(&hook).is_file());
        assert!(std::fs::read_to_string(prev_script(&hook)).unwrap().contains("~/bin/my-line"));
        // installing twice keeps the person's line, not ours, as the previous one
        install_at(&f, &hook).unwrap();
        assert!(std::fs::read_to_string(prev_script(&hook)).unwrap().contains("~/bin/my-line"));
        remove_at(&f, &hook).unwrap();
        let v = read_json(&f);
        assert_eq!(v["statusLine"]["command"], "~/bin/my-line");
        assert_eq!(v["model"], "opus");
        // broken JSON is left alone
        std::fs::write(&f, "{oops").unwrap();
        assert!(install_at(&f, &hook).is_err());
        assert_eq!(std::fs::read_to_string(&f).unwrap(), "{oops");
        std::fs::remove_dir_all(&d).unwrap();
    }
}
