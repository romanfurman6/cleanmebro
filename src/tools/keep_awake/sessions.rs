//! Which Claude Code sessions are busy. Claude Code appends to a session log
//! (`~/.claude/projects/<project>/<session>.jsonl`, subagents in a folder
//! named after the session) as it works, so a log written in the last couple
//! of minutes means that session is working.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

/// Sessions older than this aren't listed at all.
const RECENT_SECS: i64 = 24 * 3600;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Session {
    /// The project's folder name (where Claude Code ran).
    pub project: String,
    /// Unix seconds of the last write to the session's logs.
    pub last_write: i64,
}

/// Remembers project names so each log's first lines are read only once.
#[derive(Default)]
pub struct Scanner {
    names: HashMap<PathBuf, String>,
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
                out.push(Session { project, last_write: t });
            }
        }
        out.sort_by_key(|s| std::cmp::Reverse(s.last_write));
        out
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
        let proj = root.join("-Users-me-work-cleanmebro");
        std::fs::create_dir_all(proj.join("abc/subagents")).unwrap();
        std::fs::write(proj.join("abc.jsonl"), "{\"cwd\":\"/Users/me/work/cleanmebro\"}\n").unwrap();
        std::fs::write(proj.join("abc/subagents/agent-1.jsonl"), "{}\n").unwrap();
        std::fs::write(proj.join("def.jsonl"), "{}\n").unwrap();
        std::fs::write(proj.join("notes.txt"), "x").unwrap();
        let now = std::time::SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs() as i64;
        let mut s = Scanner::default();
        let found = s.scan(&root, now);
        assert_eq!(found.len(), 2, "{found:?}");
        assert!(found.iter().all(|x| x.project == "cleanmebro"));
        assert!(s.scan(&root, now + RECENT_SECS + 60).is_empty());
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn name_from_folder_when_log_has_no_cwd() {
        assert_eq!(fallback_name(Path::new("/x/-Users-me-api-server")), "server");
        assert_eq!(fallback_name(Path::new("/x/-Users-me-app-")), "app");
    }
}
