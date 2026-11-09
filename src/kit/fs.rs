//! File helpers every tool may use: folder sizes, the hard-kept locations
//! nothing may remove, moving to the Trash, and the app's own data files.

use serde::de::DeserializeOwned;
use serde::Serialize;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{Receiver, Sender};
use std::sync::{Mutex, OnceLock};

pub fn home() -> PathBuf {
    dirs::home_dir().unwrap_or_else(|| PathBuf::from("/"))
}

/// How deep `dir_size` walks; deeper content is not counted.
const DIR_SIZE_MAX_DEPTH: usize = 64;

/// Allocated size of a file or folder, not following links or leaving its volume.
pub fn dir_size(path: &Path) -> u64 {
    use std::os::unix::fs::MetadataExt;
    fn walk(p: &Path, depth: usize, max: usize, dev: u64) -> u64 {
        let Ok(rd) = std::fs::read_dir(p) else { return 0 };
        let mut total = 0;
        for e in rd.flatten() {
            let Ok(md) = e.path().symlink_metadata() else { continue };
            if md.dev() != dev {
                continue;
            }
            total += md.blocks() * 512;
            if md.is_dir() && depth < max {
                total += walk(&e.path(), depth + 1, max, dev);
            }
        }
        total
    }
    match path.symlink_metadata() {
        Ok(md) if md.is_dir() => walk(path, 0, DIR_SIZE_MAX_DEPTH, md.dev()),
        Ok(md) => md.blocks() * 512,
        Err(_) => 0,
    }
}

// ---------------------------------------------------------------- app data

/// A file or folder of the app's own data:
/// `~/Library/Application Support/cleanmebro/<name>`.
pub fn data_path(name: &str) -> PathBuf {
    dirs::data_dir().unwrap_or_else(std::env::temp_dir).join("cleanmebro").join(name)
}

/// A JSON file's value, or the default when it is missing or unreadable.
pub fn load_json<T: DeserializeOwned + Default>(path: &Path) -> T {
    std::fs::read(path).ok().and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_default()
}

/// `v` as JSON; `pretty` for files people may open.
fn to_json<T: Serialize + ?Sized>(v: &T, pretty: bool) -> Option<Vec<u8>> {
    if pretty { serde_json::to_vec_pretty(v) } else { serde_json::to_vec(v) }.ok()
}

/// Write `bytes` through a temp file beside `path` and move it in place,
/// so a crash never leaves half a file. Creates the folder.
pub fn write_atomic(path: &Path, bytes: &[u8]) -> bool {
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    let mut tmp = path.as_os_str().to_owned();
    tmp.push(".tmp");
    let tmp = PathBuf::from(tmp);
    std::fs::write(&tmp, bytes).is_ok() && std::fs::rename(&tmp, path).is_ok()
}

/// Save `v` as JSON to `path` (see [`write_atomic`]).
pub fn save_json<T: Serialize + ?Sized>(path: &Path, v: &T, pretty: bool) -> bool {
    to_json(v, pretty).is_some_and(|b| write_atomic(path, &b))
}

enum Write {
    File(PathBuf, Vec<u8>),
    Flush(Sender<()>),
}

/// The background writer: the newest bytes per file win, so a burst of
/// saves writes each file once.
fn writer() -> &'static Mutex<Sender<Write>> {
    static TX: OnceLock<Mutex<Sender<Write>>> = OnceLock::new();
    TX.get_or_init(|| {
        let (tx, rx) = std::sync::mpsc::channel::<Write>();
        std::thread::Builder::new().name("writer".into()).spawn(move || write_loop(rx)).ok();
        Mutex::new(tx)
    })
}

fn write_loop(rx: Receiver<Write>) {
    while let Ok(first) = rx.recv() {
        let mut files: Vec<(PathBuf, Vec<u8>)> = Vec::new();
        let mut flushes = Vec::new();
        for w in std::iter::once(first).chain(rx.try_iter()) {
            match w {
                Write::File(p, b) => {
                    files.retain(|(q, _)| *q != p);
                    files.push((p, b));
                }
                Write::Flush(done) => flushes.push(done),
            }
        }
        for (p, b) in files {
            write_atomic(&p, &b);
        }
        for done in flushes {
            let _ = done.send(());
        }
    }
}

/// Save `v` as JSON to `path` on the background writer, off the UI thread.
/// Serializing happens here, so later changes to `v` don't leak in.
pub fn save_json_later<T: Serialize + ?Sized>(path: &Path, v: &T, pretty: bool) {
    if let Some(b) = to_json(v, pretty) {
        let _ = writer().lock().unwrap_or_else(|e| e.into_inner()).send(Write::File(path.to_path_buf(), b));
    }
}

/// Wait (up to 2 s) until every background save so far is on disk.
pub fn flush_writes() {
    let (tx, rx) = std::sync::mpsc::channel();
    if writer().lock().unwrap_or_else(|e| e.into_inner()).send(Write::Flush(tx)).is_ok() {
        let _ = rx.recv_timeout(std::time::Duration::from_secs(2));
    }
}

/// (total, available) bytes of the volume holding `path`.
pub fn volume_space(path: &str) -> (u64, u64) {
    let c = std::ffi::CString::new(path).unwrap();
    let mut st: libc::statvfs = unsafe { std::mem::zeroed() };
    if unsafe { libc::statvfs(c.as_ptr(), &mut st) } == 0 {
        let total = st.f_blocks as u64 * st.f_frsize as u64;
        let free = st.f_bavail as u64 * st.f_frsize as u64;
        (total, free)
    } else {
        (0, 0)
    }
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

/// Move to the Trash (so it can be put back). Never falls back to a permanent
/// delete: an item that can't be trashed is reported as failed.
pub fn move_to_trash(p: &Path) -> Result<(), String> {
    crate::kit::native::trash_item(p)
}

/// Like `move_to_trash`, and returns where in the Trash the item landed.
pub fn move_to_trash_at(p: &Path) -> Result<PathBuf, String> {
    crate::kit::native::trash_item_at(p)
}

#[cfg(test)]
mod tests {
    use super::*;

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
    fn json_round_trips_atomically_and_in_the_background() {
        let dir = std::env::temp_dir().join(format!("cmb-json-{}", std::process::id()));
        let f = dir.join("sub/v.json");
        assert!(save_json(&f, &vec![1, 2, 3], true));
        assert_eq!(load_json::<Vec<i32>>(&f), vec![1, 2, 3]);
        assert!(!dir.join("sub/v.json.tmp").exists());
        // a burst of background saves: the last one wins
        for i in 0..20 {
            save_json_later(&f, &vec![i], false);
        }
        flush_writes();
        assert_eq!(load_json::<Vec<i32>>(&f), vec![19]);
        assert_eq!(load_json::<Vec<i32>>(&dir.join("missing.json")), Vec::<i32>::new());
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn dir_size_counts_nested_files() {
        let dir = std::env::temp_dir().join(format!("cmb-dirsize-{}", std::process::id()));
        let deep = dir.join("a/b/c/d/e/f/g/h");
        std::fs::create_dir_all(&deep).unwrap();
        std::fs::write(deep.join("big.bin"), vec![1u8; 200_000]).unwrap();
        let size = dir_size(&dir);
        std::fs::remove_dir_all(&dir).unwrap();
        // the file sits 8 levels down and must be counted
        assert!(size >= 200_000, "size {size}");
        assert_eq!(dir_size(Path::new("/definitely/not/here")), 0);
    }
}
