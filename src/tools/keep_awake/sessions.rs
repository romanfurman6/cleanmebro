//! Which Claude Code sessions are busy. Claude Code appends to a session log
//! (`~/.claude/projects/<project>/<session>.jsonl`, subagents in a folder
//! named after the session) as it works, so a log written in the last couple
//! of minutes means that session is working. A tool call still waiting for
//! its result (a long build, tests) also counts as working, even though the
//! log stays quiet while it runs.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

/// Sessions older than this aren't listed at all.
const RECENT_SECS: i64 = 24 * 3600;
/// A tool call silent for longer than this is taken as abandoned (Claude
/// quit or crashed mid-call).
pub const TOOL_CAP_SECS: i64 = 60 * 60;
/// How much of a log's end is read to find an open tool call.
const TAIL_BYTES: u64 = 256 * 1024;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Session {
    /// The project's folder name (where Claude Code ran).
    pub project: String,
    /// Unix seconds of the last write to the session's logs.
    pub last_write: i64,
    /// A tool call is under way (no result logged yet).
    pub tool_running: bool,
}

impl Session {
    /// When the session last showed it was busy: now while a tool runs
    /// (up to [`TOOL_CAP_SECS`]), else its last write.
    pub fn active_at(&self, now: i64) -> i64 {
        if self.tool_running && now - self.last_write <= TOOL_CAP_SECS {
            now
        } else {
            self.last_write
        }
    }
    pub fn working(&self, now: i64) -> bool {
        now - self.active_at(now) <= super::engine::WORKING_SECS
    }
}

/// Remembers project names so each log's first lines are read only once.
#[derive(Default)]
pub struct Scanner {
    names: HashMap<PathBuf, String>,
    /// Whether a log ends in an open tool call, by (log, mtime).
    open: HashMap<PathBuf, (i64, bool)>,
}

impl Scanner {
    /// Sessions written to in the last day, newest first.
    pub fn scan(&mut self, root: &Path, now: i64) -> Vec<Session> {
        let mut out = Vec::new();
        let Ok(rd) = std::fs::read_dir(root) else { return out };
        for e in rd.flatten() {
            let folder = e.path();
            if !folder.is_dir() {
                continue;
            }
            // session id → (newest write, a log to read the project from)
            let mut by_session: HashMap<String, (i64, PathBuf)> = HashMap::new();
            let Ok(files) = std::fs::read_dir(&folder) else { continue };
            for f in files.flatten() {
                let p = f.path();
                let Some(stem) = p.file_stem().map(|s| s.to_string_lossy().into_owned()) else { continue };
                let newest = if p.is_dir() { newest_log(&p, 2) } else if is_log(&p) { mtime(&p).map(|t| (t, p.clone())) } else { None };
                if let Some((t, log)) = newest {
                    let slot = by_session.entry(stem).or_insert((t, log.clone()));
                    if t > slot.0 {
                        *slot = (t, log);
                    }
                }
            }
            for (_, (t, log)) in by_session {
                if now - t > RECENT_SECS {
                    continue;
                }
                let project = self.name(&folder, &log);
                let tool_running = now - t <= TOOL_CAP_SECS && self.tool_open(&log, t);
                out.push(Session { project, last_write: t, tool_running });
            }
        }
        out.sort_by_key(|s| std::cmp::Reverse(s.last_write));
        out
    }

    fn tool_open(&mut self, log: &Path, mtime: i64) -> bool {
        if let Some((m, v)) = self.open.get(log) {
            if *m == mtime {
                return *v;
            }
        }
        let v = ends_in_open_tool_call(log);
        self.open.insert(log.to_path_buf(), (mtime, v));
        v
    }

    fn name(&mut self, folder: &Path, log: &Path) -> String {
        if let Some(n) = self.names.get(folder) {
            return n.clone();
        }
        // only a name read from a log is kept; the folder's guess is retried
        match crate::kit::claude_code::first_cwd(log).and_then(|c| c.file_name().map(|s| s.to_string_lossy().into_owned())) {
            Some(n) => {
                self.names.insert(folder.to_path_buf(), n.clone());
                n
            }
            None => fallback_name(folder),
        }
    }
}

/// Does the log end with Claude asking for a tool whose result isn't logged
/// yet? Reads only the end of the log.
fn ends_in_open_tool_call(log: &Path) -> bool {
    use std::io::{Read, Seek, SeekFrom};
    let Ok(mut f) = std::fs::File::open(log) else { return false };
    let len = f.metadata().map(|m| m.len()).unwrap_or(0);
    if f.seek(SeekFrom::Start(len.saturating_sub(TAIL_BYTES))).is_err() {
        return false;
    }
    let mut buf = Vec::new();
    if f.read_to_end(&mut buf).is_err() {
        return false;
    }
    open_tool_call(&buf)
}

/// The same, on the log's text: the last assistant or user entry decides.
fn open_tool_call(text: &[u8]) -> bool {
    let has = |l: &[u8], pat: &[u8]| l.windows(pat.len()).any(|w| w == pat);
    for line in text.split(|b| *b == b'\n').rev() {
        if has(line, b"\"tool_result\"") {
            return false;
        }
        if has(line, b"\"type\":\"assistant\"") {
            return has(line, b"\"type\":\"tool_use\"");
        }
        if has(line, b"\"type\":\"user\"") {
            return false;
        }
    }
    false
}

/// The last dash-separated part of the encoded folder name.
fn fallback_name(folder: &Path) -> String {
    let enc = folder.file_name().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
    enc.rsplit('-').find(|s| !s.is_empty()).unwrap_or(&enc).to_string()
}

fn is_log(p: &Path) -> bool {
    p.extension().is_some_and(|x| x == "jsonl")
}

fn mtime(p: &Path) -> Option<i64> {
    let t = p.metadata().ok()?.modified().ok()?;
    Some(t.duration_since(UNIX_EPOCH).ok()?.as_secs() as i64)
}

/// The newest `.jsonl` under `dir`, `depth` folders deep at most.
fn newest_log(dir: &Path, depth: u32) -> Option<(i64, PathBuf)> {
    let mut best: Option<(i64, PathBuf)> = None;
    for e in std::fs::read_dir(dir).ok()?.flatten() {
        let p = e.path();
        let found = if p.is_dir() {
            if depth > 0 { newest_log(&p, depth - 1) } else { None }
        } else if is_log(&p) {
            mtime(&p).map(|t| (t, p))
        } else {
            None
        };
        if let Some(f) = found {
            if best.as_ref().is_none_or(|b| f.0 > b.0) {
                best = Some(f);
            }
        }
    }
    best
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_sessions_and_subagent_logs() {
        let root = std::env::temp_dir().join(format!("cmb-awake-sessions-{}", std::process::id()));
        let proj = root.join("-Users-me-work-cmdmeow");
        std::fs::create_dir_all(proj.join("abc/subagents")).unwrap();
        std::fs::write(proj.join("abc.jsonl"), "{\"cwd\":\"/Users/me/work/cmdmeow\"}\n").unwrap();
        std::fs::write(proj.join("abc/subagents/agent-1.jsonl"), "{}\n").unwrap();
        std::fs::write(proj.join("def.jsonl"), "{}\n").unwrap();
        std::fs::write(proj.join("notes.txt"), "x").unwrap();
        let now = std::time::SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs() as i64;
        let mut s = Scanner::default();
        let found = s.scan(&root, now);
        assert_eq!(found.len(), 2, "{found:?}");
        assert!(found.iter().all(|x| x.project == "cmdmeow"));
        assert!(s.scan(&root, now + RECENT_SECS + 60).is_empty());
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn an_open_tool_call_counts_as_working() {
        let a_tool = br#"{"type":"assistant","message":{"content":[{"type":"tool_use","id":"t1","name":"Bash"}]}}"#;
        let its_result = br#"{"type":"user","message":{"content":[{"type":"tool_result","tool_use_id":"t1"}]}}"#;
        let text = br#"{"type":"assistant","message":{"content":[{"type":"text","text":"done"}]}}"#;
        let log = |ls: &[&[u8]]| ls.join(&b'\n');
        assert!(open_tool_call(&log(&[text, a_tool, b""])));
        assert!(!open_tool_call(&log(&[a_tool, its_result])));
        assert!(!open_tool_call(&log(&[a_tool, its_result, text])));
        assert!(!open_tool_call(b""));
        let s = Session { project: "x".into(), last_write: 1000, tool_running: true };
        assert!(s.working(1000 + 1800), "a long build keeps it working");
        assert!(!s.working(1000 + TOOL_CAP_SECS + 200), "an abandoned call stops counting");
        let quiet = Session { tool_running: false, ..s };
        assert!(!quiet.working(1000 + 1800));
    }

    #[test]
    fn name_from_folder_when_log_has_no_cwd() {
        assert_eq!(fallback_name(Path::new("/x/-Users-me-api-server")), "server");
        assert_eq!(fallback_name(Path::new("/x/-Users-me-app-")), "app");
    }
}
