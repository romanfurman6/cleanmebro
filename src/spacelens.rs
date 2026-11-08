//! Space Lens: measure a folder tree in the background, then browse it as a
//! sorted list and a bubble map; selected items go to the Trash after review.

use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::SystemTime;

#[derive(Clone, Debug, Default)]
pub struct Node {
    pub name: String,
    pub path: PathBuf,
    pub bytes: u64,
    pub items: u64,
    pub is_dir: bool,
    pub modified: Option<SystemTime>,
    pub children: Vec<Node>,
    pub other: u64, // small files folded together ("Other items")
}

#[derive(Default)]
pub struct Scan {
    pub running: AtomicBool,
    pub stop: AtomicBool,
    pub files: AtomicU64,
    pub bytes: AtomicU64,
    pub current: Mutex<String>,
    pub result: Mutex<Option<Node>>,
}

pub type Shared = Arc<Scan>;

/// Folders that the system needs; they can be browsed but not selected.
pub fn is_essential(p: &Path) -> bool {
    let home = dirs::home_dir().unwrap_or_default();
    let fixed = ["/System", "/usr", "/bin", "/sbin", "/private", "/Library", "/Applications", "/opt", "/cores", "/dev", "/etc", "/var", "/tmp", "/Volumes", "/Users"];
    if fixed.iter().any(|f| p == Path::new(f)) {
        return true;
    }
    if p.starts_with("/System") || p.starts_with("/usr") || p.starts_with("/bin") || p.starts_with("/sbin") || p.starts_with("/private/var/db") {
        return true;
    }
    p == home || p == home.join("Library") || p == home.join("Library/Application Support") || p == home.join("Library/Containers") || p == home.join("Library/Preferences") || p == home.join("Library/Keychains")
}

const KEEP_FILES_OVER: u64 = 8_000_000;

pub fn start(shared: Shared, root: PathBuf, ctx: egui::Context) {
    if shared.running.swap(true, Ordering::SeqCst) {
        return;
    }
    shared.stop.store(false, Ordering::SeqCst);
    shared.files.store(0, Ordering::SeqCst);
    shared.bytes.store(0, Ordering::SeqCst);
    *shared.result.lock().unwrap() = None;
    std::thread::spawn(move || {
        let dev = root.symlink_metadata().map(|m| m.dev()).unwrap_or(0);
        let mut last = std::time::Instant::now();
        let node = walk(&root, dev, &shared, &ctx, &mut last, 0);
        if !shared.stop.load(Ordering::SeqCst) {
            *shared.result.lock().unwrap() = Some(node);
        }
        shared.running.store(false, Ordering::SeqCst);
        ctx.request_repaint();
    });
}

fn walk(p: &Path, dev: u64, s: &Scan, ctx: &egui::Context, last: &mut std::time::Instant, depth: usize) -> Node {
    let md = p.symlink_metadata().ok();
    let name = p.file_name().map(|x| x.to_string_lossy().into_owned()).unwrap_or_else(|| p.display().to_string());
    let mut node = Node {
        name,
        path: p.to_path_buf(),
        is_dir: md.as_ref().map(|m| m.is_dir()).unwrap_or(false),
        modified: md.as_ref().and_then(|m| m.modified().ok()),
        ..Default::default()
    };
    if s.stop.load(Ordering::Relaxed) {
        return node;
    }
    if last.elapsed().as_millis() > 120 {
        *s.current.lock().unwrap() = p.display().to_string();
        ctx.request_repaint();
        *last = std::time::Instant::now();
    }
    let Ok(rd) = std::fs::read_dir(p) else { return node };
    for e in rd.flatten() {
        let cp = e.path();
        let Ok(cm) = cp.symlink_metadata() else { continue };
        if cm.dev() != dev && dev != 0 {
            continue; // stay on this volume (skips /System/Volumes/Data firmlinks, network mounts)
        }
        if cm.is_dir() {
            let child = walk(&cp, dev, s, ctx, last, depth + 1);
            node.bytes += child.bytes;
            node.items += child.items + 1;
            node.children.push(child);
        } else {
            let b = cm.blocks() * 512;
            s.files.fetch_add(1, Ordering::Relaxed);
            s.bytes.fetch_add(b, Ordering::Relaxed);
            node.bytes += b;
            node.items += 1;
            if b >= KEEP_FILES_OVER {
                node.children.push(Node {
                    name: e.file_name().to_string_lossy().into_owned(),
                    path: cp,
                    bytes: b,
                    items: 1,
                    is_dir: false,
                    modified: cm.modified().ok(),
                    children: vec![],
                    other: 0,
                });
            } else {
                node.other += b;
            }
        }
    }
    node.children.sort_by(|a, b| b.bytes.cmp(&a.bytes));
    // keep memory bounded deep down: fold tiny folders away
    if depth > 3 {
        let mut kept = Vec::new();
        for c in node.children.drain(..) {
            if c.bytes >= 1_000_000 || kept.len() < 40 {
                kept.push(c);
            } else {
                node.other += c.bytes;
            }
        }
        node.children = kept;
    }
    node
}

/// Find the node at `path` inside `root`.
pub fn find<'a>(root: &'a Node, path: &Path) -> Option<&'a Node> {
    if root.path == path {
        return Some(root);
    }
    root.children.iter().filter(|c| path.starts_with(&c.path)).find_map(|c| find(c, path))
}

/// Remove nodes whose paths were deleted, fixing sizes on the way up.
pub fn prune(node: &mut Node, removed: &[PathBuf]) -> u64 {
    let mut freed = 0;
    node.children.retain(|c| {
        if removed.contains(&c.path) {
            freed += c.bytes;
            false
        } else {
            true
        }
    });
    for c in node.children.iter_mut() {
        freed += prune(c, removed);
    }
    node.bytes = node.bytes.saturating_sub(freed);
    freed
}

// ---------------------------------------------------------------- bubbles

/// Pack up to `max` circles around the centre of `rect`, biggest in the middle,
/// each new one touching an earlier one as close to the centre as it fits.
/// Radius grows with size^0.42 so small folders stay readable.
/// Returns (child index, centre, radius).
pub fn pack(sizes: &[u64], rect: egui::Rect, max: usize) -> Vec<(usize, egui::Pos2, f32)> {
    let n = sizes.len().min(max);
    if n == 0 {
        return vec![];
    }
    let weights: Vec<f32> = sizes[..n].iter().map(|&b| (b.max(1) as f32).powf(0.42)).collect();
    let mut scale = rect.width().min(rect.height()) * 0.30 / weights[0];
    let center = rect.center();
    let margin = 10.0;
    for _attempt in 0..8 {
        let radii: Vec<f32> = weights.iter().map(|w| (w * scale).max(30.0)).collect();
        let mut placed: Vec<(usize, egui::Pos2, f32)> = vec![(0, center, radii[0])];
        for (i, &r) in radii.iter().enumerate().skip(1) {
            let mut best: Option<(f32, egui::Pos2)> = None;
            for &(_, pc, pr) in &placed {
                for k in 0..48 {
                    let a = k as f32 / 48.0 * std::f32::consts::TAU;
                    let c = pc + egui::vec2(a.cos(), a.sin()) * (pr + r + margin);
                    if c.x - r < rect.left() || c.x + r > rect.right() || c.y - r < rect.top() || c.y + r > rect.bottom() {
                        continue;
                    }
                    if placed.iter().any(|&(_, q, qr)| (c - q).length() < r + qr + margin - 0.5) {
                        continue;
                    }
                    let d = (c - center).length();
                    if best.map(|b| d < b.0).unwrap_or(true) {
                        best = Some((d, c));
                    }
                }
            }
            if let Some((_, c)) = best {
                placed.push((i, c, r));
            }
        }
        // shrink until everything fits (or we give up and show what fits)
        if placed.len() == n {
            return placed;
        }
        scale *= 0.88;
        if _attempt == 7 {
            return placed;
        }
    }
    vec![]
}
