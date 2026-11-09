//! Disk Map: measure a folder tree in the background, then browse it as a
//! drill-down treemap; selected items go to the Trash after review.

use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

#[derive(Clone, Debug, Default, serde::Serialize, serde::Deserialize)]
pub struct Node {
    pub name: String,
    pub path: PathBuf,
    pub bytes: u64,
    pub items: u64,
    pub children: Vec<Node>,
    pub other: u64, // small files folded together ("Other items")
}

/// A top-level folder while mapping: its live byte count, and whether it's
/// finished.
pub struct LiveChild {
    pub name: String,
    pub path: PathBuf,
    pub bytes: Arc<AtomicU64>,
    pub done: bool,
}

/// One row of the live list while mapping (cheap to copy: no subtree).
#[derive(Clone, Debug)]
pub struct LiveRow {
    pub name: String,
    pub path: PathBuf,
    pub bytes: u64,
    pub done: bool,
}

#[derive(Default)]
pub struct Scan {
    pub running: AtomicBool,
    pub stop: AtomicBool,
    pub files: AtomicU64,
    pub bytes: AtomicU64,
    pub current: Mutex<String>,
    pub result: Mutex<Option<Node>>,
    /// When the map in `result` was made (Unix seconds).
    pub when: Mutex<Option<i64>>,
    /// The saved map is being read back from disk.
    pub loading: AtomicBool,
    /// Reading the saved map was already tried this run.
    pub loaded: AtomicBool,
    /// The root's folders while mapping, so the map can grow live.
    pub live: Mutex<Option<(PathBuf, Vec<LiveChild>)>>,
}

pub type Shared = Arc<Scan>;

/// What a folder at `p` is called on the map ("/" is the startup disk).
pub fn display_name(p: &Path) -> String {
    p.file_name()
        .map(|x| x.to_string_lossy().into_owned())
        .unwrap_or_else(|| "Macintosh HD".into())
}

/// The root's folders so far while mapping, with what's been counted in
/// each. Only sizes are copied, never the finished trees, so this stays cheap
/// however big the disk is.
pub fn partial(s: &Scan) -> Option<(PathBuf, Vec<LiveRow>)> {
    let g = s.live.lock().unwrap();
    let (root, kids) = g.as_ref()?;
    let rows = kids
        .iter()
        .map(|k| LiveRow {
            name: k.name.clone(),
            path: k.path.clone(),
            bytes: k.bytes.load(Ordering::Relaxed),
            done: k.done,
        })
        .filter(|r| r.bytes > 0)
        .collect();
    Some((root.clone(), rows))
}

/// Folders macOS guards behind their own permission prompt (Music and TV
/// media libraries, Contacts, Calendars, Reminders). Reading inside them
/// pops a system dialog even with Full Disk Access, so the map never
/// enters them.
pub fn asks_permission(p: &Path) -> bool {
    if p.extension()
        .is_some_and(|e| e == "musiclibrary" || e == "tvlibrary")
    {
        return true;
    }
    let Some(home) = dirs::home_dir() else {
        return false;
    };
    [
        "Music/Music",
        "Music/iTunes",
        "Movies/TV",
        "Library/Calendars",
        "Library/Reminders",
        "Library/Application Support/AddressBook",
        "Library/Application Support/CallHistoryDB",
    ]
    .iter()
    .any(|x| p == home.join(x))
}

/// Run the current thread just below the UI, so mapping never makes the
/// window stutter.
fn below_ui() {
    #[cfg(target_os = "macos")]
    unsafe {
        libc::pthread_set_qos_class_self_np(libc::qos_class_t::QOS_CLASS_USER_INITIATED, 0);
    }
}

/// Folders that the system needs; they can be browsed but not selected.
pub fn is_essential(p: &Path) -> bool {
    let home = dirs::home_dir().unwrap_or_default();
    let fixed = [
        "/System",
        "/usr",
        "/bin",
        "/sbin",
        "/private",
        "/Library",
        "/Applications",
        "/opt",
        "/cores",
        "/dev",
        "/etc",
        "/var",
        "/tmp",
        "/Volumes",
        "/Users",
    ];
    if fixed.iter().any(|f| p == Path::new(f)) {
        return true;
    }
    if p.starts_with("/System")
        || p.starts_with("/usr")
        || p.starts_with("/bin")
        || p.starts_with("/sbin")
        || p.starts_with("/private/var/db")
    {
        return true;
    }
    p == home
        || p == home.join("Library")
        || p == home.join("Library/Application Support")
        || p == home.join("Library/Containers")
        || p == home.join("Library/Preferences")
        || p == home.join("Library/Keychains")
}

const KEEP_FILES_OVER: u64 = 8_000_000;

/// Walk `root` on background threads. Big trees are split across the CPU
/// cores: a folder's subfolders go to free threads while any are left.
pub fn start(shared: Shared, root: PathBuf, ctx: egui::Context) {
    if shared.running.swap(true, Ordering::SeqCst) {
        return;
    }
    shared.stop.store(false, Ordering::SeqCst);
    shared.files.store(0, Ordering::SeqCst);
    shared.bytes.store(0, Ordering::SeqCst);
    *shared.result.lock().unwrap() = None;
    *shared.live.lock().unwrap() = None;
    std::thread::spawn(move || {
        below_ui();
        let w = Walker::new(&root, &shared, &ctx);
        let node = w.root(&root);
        let now = chrono::Utc::now().timestamp();
        if root == Path::new("/") {
            save(&node, now);
        }
        *shared.when.lock().unwrap() = Some(now);
        *shared.result.lock().unwrap() = Some(node);
        *shared.live.lock().unwrap() = None;
        shared.running.store(false, Ordering::SeqCst);
        ctx.request_repaint();
    });
}

// ---------------------------------------------------------------- saved map

/// Where the last map of the disk is kept between runs.
fn saved_path() -> PathBuf {
    crate::kit::fs::data_path("diskmap.json")
}

#[derive(serde::Serialize, serde::Deserialize)]
struct Saved {
    when: i64,
    root: Node,
}

/// Keep `root` as the last map (written to a temp file, then moved in place).
pub fn save(root: &Node, when: i64) {
    let path = saved_path();
    let _ = std::fs::create_dir_all(path.parent().unwrap());
    let tmp = path.with_extension("json.tmp");
    let ok = std::fs::File::create(&tmp).ok().is_some_and(|f| {
        #[derive(serde::Serialize)]
        struct SavedRef<'a> {
            when: i64,
            root: &'a Node,
        }
        serde_json::to_writer(std::io::BufWriter::new(f), &SavedRef { when, root }).is_ok()
    });
    if ok {
        let _ = std::fs::rename(&tmp, &path);
    }
}

/// Save the current map in the background (after items were removed from it).
pub fn save_later(shared: &Shared) {
    let s = shared.clone();
    std::thread::spawn(move || {
        let node = s.result.lock().unwrap().clone();
        let when = *s.when.lock().unwrap();
        if let (Some(node), Some(when)) = (node, when) {
            if node.path == Path::new("/") {
                save(&node, when);
            }
        }
    });
}

/// Read the last map back, once per run, in the background. Returns false
/// when there's no saved map (so the caller can start one).
pub fn load(shared: &Shared, ctx: &egui::Context) -> bool {
    if shared.loaded.swap(true, Ordering::SeqCst) {
        return true;
    }
    let path = saved_path();
    if !path.exists() {
        return false;
    }
    shared.loading.store(true, Ordering::SeqCst);
    let (s, ctx) = (shared.clone(), ctx.clone());
    std::thread::spawn(move || {
        below_ui();
        let saved: Option<Saved> = std::fs::File::open(&path)
            .ok()
            .and_then(|f| serde_json::from_reader(std::io::BufReader::new(f)).ok());
        if let Some(saved) = saved {
            let mut r = s.result.lock().unwrap();
            if r.is_none() && !s.running.load(Ordering::SeqCst) {
                *r = Some(saved.root);
                *s.when.lock().unwrap() = Some(saved.when);
            }
        }
        s.loading.store(false, Ordering::SeqCst);
        ctx.request_repaint();
    });
    true
}

/// Volumes and folders a walk stays inside, and the threads it may use.
struct Walker<'a> {
    s: &'a Scan,
    ctx: &'a egui::Context,
    /// Devices to stay on: the root's, plus the Data volume when mapping "/"
    /// (macOS reaches /Users, /Applications… through firmlinks onto it).
    devs: Vec<u64>,
    /// Folders never entered (the Data volume's own mount point, so nothing
    /// is counted twice).
    skip: Vec<PathBuf>,
    /// Extra threads still free.
    free: AtomicU64,
    last: Mutex<std::time::Instant>,
}

impl<'a> Walker<'a> {
    fn new(root: &Path, s: &'a Scan, ctx: &'a egui::Context) -> Self {
        let dev_of = |p: &str| Path::new(p).symlink_metadata().map(|m| m.dev()).ok();
        let mut devs: Vec<u64> = root
            .symlink_metadata()
            .map(|m| m.dev())
            .into_iter()
            .collect();
        let mut skip = Vec::new();
        if root == Path::new("/") {
            devs.extend(dev_of("/System/Volumes/Data"));
            skip.push(PathBuf::from("/System/Volumes"));
            skip.push(PathBuf::from("/Volumes"));
        }
        let cores = std::thread::available_parallelism()
            .map(|n| n.get())
            .unwrap_or(4) as u64;
        // leave a core for the window
        Walker {
            s,
            ctx,
            devs,
            skip,
            free: AtomicU64::new(cores.saturating_sub(2).max(1)),
            last: Mutex::new(std::time::Instant::now()),
        }
    }

    fn stay(&self, md: &std::fs::Metadata, p: &Path) -> bool {
        (self.devs.is_empty() || self.devs.contains(&md.dev())) && !self.skip.iter().any(|x| x == p)
    }

    /// Take a free thread if there is one.
    fn try_thread(&self) -> bool {
        self.free
            .fetch_update(Ordering::AcqRel, Ordering::Acquire, |n| n.checked_sub(1))
            .is_ok()
    }

    /// The root: its folders are published live and each one counts into
    /// its own total while it's walked.
    fn root(&self, root: &Path) -> Node {
        let mut node = Node {
            name: display_name(root),
            path: root.to_path_buf(),
            ..Default::default()
        };
        let (dirs, files) = self.list(root);
        self.add_files(&mut node, files, None);
        let live: Vec<LiveChild> = dirs
            .iter()
            .map(|d| LiveChild {
                name: d
                    .file_name()
                    .map(|x| x.to_string_lossy().into_owned())
                    .unwrap_or_default(),
                path: d.clone(),
                bytes: Arc::new(AtomicU64::new(0)),
                done: false,
            })
            .collect();
        let counters: Vec<Arc<AtomicU64>> = live.iter().map(|l| l.bytes.clone()).collect();
        *self.s.live.lock().unwrap() = Some((root.to_path_buf(), live));
        let next = AtomicU64::new(0);
        // every core pulls the next top-level folder; inside one, free threads help
        let workers = std::thread::available_parallelism()
            .map(|n| n.get())
            .unwrap_or(4)
            .saturating_sub(1)
            .max(1)
            .min(dirs.len().max(1));
        let results: Mutex<Vec<(usize, Node)>> = Mutex::new(Vec::new());
        std::thread::scope(|sc| {
            for _ in 0..workers {
                sc.spawn(|| loop {
                    below_ui();
                    let k = next.fetch_add(1, Ordering::SeqCst) as usize;
                    if k >= dirs.len() {
                        break;
                    }
                    let child = self.walk(&dirs[k], 1, Some(&counters[k]));
                    counters[k].store(child.bytes, Ordering::Relaxed);
                    if let Some((_, live)) = self.s.live.lock().unwrap().as_mut() {
                        live[k].done = true;
                    }
                    self.ctx.request_repaint();
                    results.lock().unwrap().push((k, child));
                });
            }
        });
        for (_, child) in results.into_inner().unwrap() {
            node.bytes += child.bytes;
            node.items += child.items + 1;
            node.children.push(child);
        }
        node.children.sort_by_key(|c| std::cmp::Reverse(c.bytes));
        node
    }

    /// Subfolders and (path, allocated bytes, name) files of `p`, staying on
    /// the allowed volumes.
    fn list(&self, p: &Path) -> (Vec<PathBuf>, Vec<(PathBuf, u64, String)>) {
        let (mut dirs, mut files) = (Vec::new(), Vec::new());
        let Ok(rd) = std::fs::read_dir(p) else {
            return (dirs, files);
        };
        for e in rd.flatten() {
            let cp = e.path();
            if asks_permission(&cp) {
                continue; // never touch folders behind a permission prompt
            }
            let Ok(cm) = cp.symlink_metadata() else {
                continue;
            };
            if !self.stay(&cm, &cp) {
                continue; // other volumes (network mounts, backups) and the Data mount point
            }
            if cm.is_dir() {
                dirs.push(cp);
            } else {
                files.push((
                    cp,
                    cm.blocks() * 512,
                    e.file_name().to_string_lossy().into_owned(),
                ));
            }
        }
        (dirs, files)
    }

    fn add_files(
        &self,
        node: &mut Node,
        files: Vec<(PathBuf, u64, String)>,
        top: Option<&AtomicU64>,
    ) {
        let mut sum = 0;
        for (path, b, name) in files {
            node.bytes += b;
            node.items += 1;
            sum += b;
            if b >= KEEP_FILES_OVER {
                node.children.push(Node {
                    name,
                    path,
                    bytes: b,
                    items: 1,
                    children: vec![],
                    other: 0,
                });
            } else {
                node.other += b;
            }
        }
        if let Some(t) = top {
            t.fetch_add(sum, Ordering::Relaxed);
        }
        self.s.bytes.fetch_add(sum, Ordering::Relaxed);
    }

    fn walk(&self, p: &Path, depth: usize, top: Option<&AtomicU64>) -> Node {
        let name = p
            .file_name()
            .map(|x| x.to_string_lossy().into_owned())
            .unwrap_or_else(|| p.display().to_string());
        let mut node = Node {
            name,
            path: p.to_path_buf(),
            ..Default::default()
        };
        if self.s.stop.load(Ordering::Relaxed) {
            return node;
        }
        if let Ok(mut last) = self.last.try_lock() {
            if last.elapsed().as_millis() > 120 {
                *self.s.current.lock().unwrap() = p.display().to_string();
                self.ctx.request_repaint();
                *last = std::time::Instant::now();
            }
        }
        let (dirs, files) = self.list(p);
        self.s
            .files
            .fetch_add(files.len() as u64, Ordering::Relaxed);
        self.add_files(&mut node, files, top);
        let kids: Vec<Node> = std::thread::scope(|sc| {
            let mut handles = Vec::new();
            let mut done = Vec::new();
            for d in &dirs {
                if self.try_thread() {
                    handles.push(sc.spawn(move || {
                        below_ui();
                        let n = self.walk(d, depth + 1, top);
                        self.free.fetch_add(1, Ordering::AcqRel);
                        n
                    }));
                } else {
                    done.push(self.walk(d, depth + 1, top));
                }
            }
            done.extend(handles.into_iter().filter_map(|h| h.join().ok()));
            done
        });
        for child in kids {
            node.bytes += child.bytes;
            node.items += child.items + 1;
            node.children.push(child);
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
}

/// Find the node at `path` inside `root`.
pub fn find<'a>(root: &'a Node, path: &Path) -> Option<&'a Node> {
    if root.path == path {
        return Some(root);
    }
    root.children
        .iter()
        .filter(|c| path.starts_with(&c.path))
        .find_map(|c| find(c, path))
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
        return values
            .iter()
            .map(|_| egui::Rect::from_min_size(rect.min, egui::Vec2::ZERO))
            .collect();
    }
    let (mut x, mut y, mut w, mut h) = (
        rect.left() as f64,
        rect.top() as f64,
        rect.width() as f64,
        rect.height() as f64,
    );
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
                out.push(egui::Rect::from_min_size(
                    egui::pos2(*x as f32, yy as f32),
                    egui::vec2(cw as f32, hh as f32),
                ));
                yy += hh;
            }
            *x += cw;
            *w -= cw;
        } else {
            let rh = if *w > 0.0 { s / *w } else { 0.0 };
            let mut xx = *x;
            for a in row.iter() {
                let ww = if rh > 0.0 { a / rh } else { 0.0 };
                out.push(egui::Rect::from_min_size(
                    egui::pos2(xx as f32, *y as f32),
                    egui::vec2(ww as f32, rh as f32),
                ));
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
        Node {
            name: path.rsplit('/').next().unwrap().into(),
            path: PathBuf::from(path),
            bytes,
            children,
            ..Default::default()
        }
    }

    fn tree() -> Node {
        node(
            "/r",
            100,
            vec![
                node(
                    "/r/a",
                    60,
                    vec![node("/r/a/x", 50, vec![]), node("/r/a/y", 10, vec![])],
                ),
                node("/r/ab", 40, vec![]),
            ],
        )
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
    fn permission_folders_are_skipped() {
        let h = dirs::home_dir().unwrap();
        assert!(asks_permission(&h.join("Music/Music")));
        assert!(asks_permission(Path::new("/Volumes/X/Old.musiclibrary")));
        assert!(asks_permission(&h.join("Library/Calendars")));
        assert!(!asks_permission(&h.join("Music")));
        assert!(!asks_permission(&h.join("Downloads")));
    }

    #[test]
    fn saved_map_round_trips() {
        let t = tree();
        let json = serde_json::to_string(&Saved {
            when: 7,
            root: t.clone(),
        })
        .unwrap();
        let back: Saved = serde_json::from_str(&json).unwrap();
        assert_eq!(back.when, 7);
        assert_eq!(find(&back.root, Path::new("/r/a/x")).unwrap().bytes, 50);
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
    fn parallel_walk_counts_every_file_once() {
        let base = std::env::temp_dir().join(format!("cmb-scan-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        // 6 top folders x 5 subfolders x 4 files of 10 kB, plus one big file
        for a in 0..6 {
            for b in 0..5 {
                let d = base.join(format!("t{a}/s{b}"));
                std::fs::create_dir_all(&d).unwrap();
                for c in 0..4 {
                    std::fs::write(d.join(format!("f{c}")), vec![1u8; 10_000]).unwrap();
                }
            }
        }
        std::fs::write(base.join("t0/big"), vec![1u8; 9_000_000]).unwrap();
        let shared: Shared = Default::default();
        start(shared.clone(), base.clone(), egui::Context::default());
        let t0 = std::time::Instant::now();
        while shared.running.load(Ordering::SeqCst) && t0.elapsed().as_secs() < 20 {
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        let root = shared.result.lock().unwrap().take().unwrap();
        assert_eq!(shared.files.load(Ordering::SeqCst), 6 * 5 * 4 + 1);
        assert_eq!(root.children.len(), 6);
        assert_eq!(root.children[0].name, "t0"); // biggest first
        assert!(root.children[0].children.iter().any(|c| c.name == "big"));
        let sum: u64 = root.children.iter().map(|c| c.bytes).sum();
        assert_eq!(root.bytes, sum);
        assert_eq!(shared.bytes.load(Ordering::SeqCst), root.bytes);
        assert!(shared.live.lock().unwrap().is_none());
        let _ = std::fs::remove_dir_all(&base);
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
            assert!(
                ((a.area() as f64) - want).abs() < want * 0.01 + 1.0,
                "block {k} area"
            );
            for b in &r[k + 1..] {
                assert!(a.intersect(*b).area() < 0.5, "blocks overlap");
            }
        }
        assert!(squarify(&[], rect).is_empty());
        assert_eq!(squarify(&[0.0, 0.0], rect).len(), 2);
    }
}
