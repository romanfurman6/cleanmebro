//! Disk Map: measure a folder tree in the background, then browse it as a
//! drill-down treemap; selected items go to the Trash after review.

use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

#[derive(Clone, Debug, Default)]
pub struct Node {
    pub name: String,
    pub path: PathBuf,
    pub bytes: u64,
    pub items: u64,
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
    let name = p.file_name().map(|x| x.to_string_lossy().into_owned()).unwrap_or_else(|| p.display().to_string());
    let mut node = Node {
        name,
        path: p.to_path_buf(),
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
                    children: vec![],
                    other: 0,
                });
            } else {
                node.other += b;
            }
        }
    }
    node.children.sort_by_key(|c| std::cmp::Reverse(c.bytes));
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

// ---------------------------------------------------------------- treemap

/// Squarified treemap: split `rect` into one block per value (same order,
/// biggest first works best) with areas proportional to the values and
/// sides as close to square as the order allows.
pub fn squarify(values: &[f64], rect: egui::Rect) -> Vec<egui::Rect> {
    let total: f64 = values.iter().sum();
    if values.is_empty() || total <= 0.0 || rect.width() <= 0.0 || rect.height() <= 0.0 {
        return values.iter().map(|_| egui::Rect::from_min_size(rect.min, egui::Vec2::ZERO)).collect();
    }
    let (mut x, mut y, mut w, mut h) = (rect.left() as f64, rect.top() as f64, rect.width() as f64, rect.height() as f64);
    let areas: Vec<f64> = values.iter().map(|v| v.max(0.0) * w * h / total).collect();
    let worst = |row: &[f64], side: f64| -> f64 {
        if row.is_empty() {
            return f64::INFINITY;
        }
        let s: f64 = row.iter().sum();
        let mx = row.iter().cloned().fold(f64::MIN, f64::max);
        let mn = row.iter().cloned().fold(f64::MAX, f64::min).max(1e-9);
        (side * side * mx / (s * s)).max((s * s) / (side * side * mn))
    };
    let mut out = Vec::with_capacity(values.len());
    let mut row: Vec<f64> = Vec::new();
    let mut place = |row: &mut Vec<f64>, x: &mut f64, y: &mut f64, w: &mut f64, h: &mut f64| {
        let s: f64 = row.iter().sum();
        if *w >= *h {
            let cw = if *h > 0.0 { s / *h } else { 0.0 };
            let mut yy = *y;
            for a in row.iter() {
                let hh = if cw > 0.0 { a / cw } else { 0.0 };
                out.push(egui::Rect::from_min_size(egui::pos2(*x as f32, yy as f32), egui::vec2(cw as f32, hh as f32)));
                yy += hh;
            }
            *x += cw;
            *w -= cw;
        } else {
            let rh = if *w > 0.0 { s / *w } else { 0.0 };
            let mut xx = *x;
            for a in row.iter() {
                let ww = if rh > 0.0 { a / rh } else { 0.0 };
                out.push(egui::Rect::from_min_size(egui::pos2(xx as f32, *y as f32), egui::vec2(ww as f32, rh as f32)));
                xx += ww;
            }
            *y += rh;
            *h -= rh;
        }
        row.clear();
    };
    let mut i = 0;
    while i < areas.len() {
        let side = w.min(h);
        let mut next = row.clone();
        next.push(areas[i]);
        if row.is_empty() || worst(&next, side) <= worst(&row, side) {
            row.push(areas[i]);
            i += 1;
        } else {
            place(&mut row, &mut x, &mut y, &mut w, &mut h);
        }
    }
    if !row.is_empty() {
        place(&mut row, &mut x, &mut y, &mut w, &mut h);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn node(path: &str, bytes: u64, children: Vec<Node>) -> Node {
        Node { name: path.rsplit('/').next().unwrap().into(), path: PathBuf::from(path), bytes, children, ..Default::default() }
    }

    fn tree() -> Node {
        node("/r", 100, vec![node("/r/a", 60, vec![node("/r/a/x", 50, vec![]), node("/r/a/y", 10, vec![])]), node("/r/ab", 40, vec![])])
    }

    #[test]
    fn find_by_path() {
        let t = tree();
        assert_eq!(find(&t, Path::new("/r/a/y")).unwrap().bytes, 10);
        assert_eq!(find(&t, Path::new("/r/ab")).unwrap().bytes, 40);
        assert!(find(&t, Path::new("/r/zz")).is_none());
    }

    #[test]
    fn prune_fixes_sizes_up_the_tree() {
        let mut t = tree();
        let freed = prune(&mut t, &[PathBuf::from("/r/a/x"), PathBuf::from("/r/ab")]);
        assert_eq!(freed, 90);
        assert_eq!(t.bytes, 10);
        assert_eq!(find(&t, Path::new("/r/a")).unwrap().bytes, 10);
        assert!(find(&t, Path::new("/r/a/x")).is_none());
    }

    #[test]
    fn essential_paths() {
        assert!(is_essential(Path::new("/System/Library")));
        assert!(is_essential(Path::new("/Applications")));
        assert!(!is_essential(Path::new("/Applications/Foo.app")));
        let h = dirs::home_dir().unwrap();
        assert!(is_essential(&h));
        assert!(is_essential(&h.join("Library")));
        assert!(!is_essential(&h.join("Downloads/big.iso")));
    }

    #[test]
    fn treemap_fills_the_rect_without_overlap() {
        let rect = egui::Rect::from_min_size(egui::pos2(10.0, 20.0), egui::vec2(600.0, 500.0));
        let vals = [182.0, 64.2, 38.4, 31.0, 22.5, 14.1, 6.3, 2.1, 4.4];
        let r = squarify(&vals, rect);
        assert_eq!(r.len(), vals.len());
        let total: f64 = vals.iter().sum();
        for (k, a) in r.iter().enumerate() {
            assert!(rect.expand(0.01).contains_rect(*a), "block {k} outside");
            let want = vals[k] / total * 600.0 * 500.0;
            assert!(((a.area() as f64) - want).abs() < want * 0.01 + 1.0, "block {k} area");
            for b in &r[k + 1..] {
                assert!(a.intersect(*b).area() < 0.5, "blocks overlap");
            }
        }
        assert!(squarify(&[], rect).is_empty());
        assert_eq!(squarify(&[0.0, 0.0], rect).len(), 2);
    }
}
