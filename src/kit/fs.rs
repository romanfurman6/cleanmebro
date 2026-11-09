//! File helpers every tool may use: folder sizes, the hard-kept locations
//! nothing may remove, and moving to the Trash.

use std::path::{Path, PathBuf};

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
