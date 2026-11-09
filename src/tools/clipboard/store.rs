//! Clipboard history on disk: `index.json` with every entry's text and
//! metadata, plus a `blobs/` folder for images and rich text. Newest first;
//! pinned entries never age out.

use egui::Color32;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// What an entry holds. Decides its card, its filter and how it pastes.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Kind {
    Text,
    Link,
    Code,
    Color,
    Image,
    File,
}

impl Kind {
    #[cfg(test)]
    pub const ALL: [Kind; 6] = [Kind::Text, Kind::Code, Kind::Link, Kind::Image, Kind::Color, Kind::File];

    pub fn label(self) -> &'static str {
        match self {
            Kind::Text => "Text",
            Kind::Link => "Link",
            Kind::Code => "Code",
            Kind::Color => "Color",
            Kind::Image => "Image",
            Kind::File => "File",
        }
    }

    /// The color strip on the entry's card.
    pub fn tint(self) -> Color32 {
        match self {
            Kind::Text => Color32::from_rgb(0x63, 0x63, 0x66),
            Kind::Code => Color32::from_rgb(0x24, 0x8A, 0x3D),
            Kind::Link => Color32::from_rgb(0x0B, 0x6B, 0xD8),
            Kind::Image => Color32::from_rgb(0x8E, 0x44, 0xAD),
            Kind::Color => Color32::from_rgb(0x2E, 0x9E, 0x8F),
            Kind::File => Color32::from_rgb(0xC7, 0x80, 0x1F),
        }
    }
}

/// What the pasteboard held, as read by the watcher.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Clip {
    pub text: Option<String>,
    pub rtf: Option<Vec<u8>>,
    pub html: Option<Vec<u8>>,
    pub png: Option<Vec<u8>>,
    pub files: Vec<PathBuf>,
    /// Marked by the copying app as a password or as not meant for history.
    pub concealed: bool,
    /// The app in front when it was copied.
    pub app: String,
    pub bundle: String,
}

impl Clip {
    pub fn is_empty(&self) -> bool {
        self.text.as_deref().is_none_or(|t| t.trim().is_empty()) && self.png.is_none() && self.files.is_empty()
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Entry {
    pub id: u64,
    pub kind: Kind,
    /// The text; for a file the paths, one per line; for an image a caption.
    pub text: String,
    pub app: String,
    #[serde(default)]
    pub bundle: String,
    /// Copied first and last, in ms since 1970.
    pub first: i64,
    pub last: i64,
    pub count: u32,
    #[serde(default)]
    pub pinned: bool,
    /// Image size in pixels.
    #[serde(default)]
    pub w: u32,
    #[serde(default)]
    pub h: u32,
    #[serde(default)]
    pub rtf: bool,
    #[serde(default)]
    pub html: bool,
    /// Bytes on disk (blobs) or of the text.
    #[serde(default)]
    pub bytes: u64,
    pub hash: u64,
}

impl Entry {
    pub fn time(&self) -> chrono::DateTime<chrono::Local> {
        chrono::DateTime::from_timestamp_millis(self.last).map(|t| t.with_timezone(&chrono::Local)).unwrap_or_else(chrono::Local::now)
    }

    pub fn files(&self) -> Vec<PathBuf> {
        if self.kind == Kind::File {
            self.text.lines().filter(|l| !l.is_empty()).map(PathBuf::from).collect()
        } else {
            Vec::new()
        }
    }

    /// One line for lists.
    pub fn title(&self) -> String {
        match self.kind {
            Kind::File => {
                let f = self.files();
                let name = f.first().and_then(|p| p.file_name()).map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
                if f.len() > 1 {
                    format!("{name} and {} more", f.len() - 1)
                } else {
                    name
                }
            }
            Kind::Link => self.text.trim().trim_start_matches("https://").trim_start_matches("http://").trim_end_matches('/').to_string(),
            _ => self.text.split_whitespace().collect::<Vec<_>>().join(" "),
        }
    }

    /// Whether the search words all appear in the entry.
    pub fn matches(&self, query: &str) -> bool {
        let hay = format!("{} {}", self.text, self.app).to_lowercase();
        query.to_lowercase().split_whitespace().all(|w| hay.contains(w))
    }

    /// Facts for the detail view: "2 lines · 92 characters".
    pub fn summary(&self) -> String {
        match self.kind {
            Kind::Image => format!("{}×{} · {}", self.w, self.h, crate::kit::glass::human_bytes(self.bytes)),
            Kind::File => {
                let n = self.files().len();
                format!("{n} file{}", if n == 1 { "" } else { "s" })
            }
            Kind::Color => self.text.trim().to_string(),
            _ => {
                let lines = self.text.lines().count().max(1);
                let chars = self.text.chars().count();
                if lines > 1 {
                    format!("{lines} lines · {chars} characters")
                } else {
                    format!("{chars} characters")
                }
            }
        }
    }
}

/// FNV-1a: a stable hash for spotting repeats (std's hasher changes between runs).
pub fn hash(parts: &[&[u8]]) -> u64 {
    let mut h: u64 = 0xcbf29ce484222325;
    for p in parts {
        for b in *p {
            h ^= *b as u64;
            h = h.wrapping_mul(0x100000001b3);
        }
        h ^= 0xff;
        h = h.wrapping_mul(0x100000001b3);
    }
    h
}

/// A CSS-style color in the text: `#RGB`, `#RRGGBB`, `#RRGGBBAA`, `rgb(…)`, `rgba(…)`.
pub fn parse_color(s: &str) -> Option<Color32> {
    let s = s.trim();
    if let Some(hex) = s.strip_prefix('#') {
        if !hex.chars().all(|c| c.is_ascii_hexdigit()) {
            return None;
        }
        let v = |i: usize, n: usize| u8::from_str_radix(&hex[i..i + n], 16).ok();
        return match hex.len() {
            3 => Some(Color32::from_rgb(v(0, 1)? * 17, v(1, 1)? * 17, v(2, 1)? * 17)),
            6 => Some(Color32::from_rgb(v(0, 2)?, v(2, 2)?, v(4, 2)?)),
            8 => Some(Color32::from_rgba_unmultiplied(v(0, 2)?, v(2, 2)?, v(4, 2)?, v(6, 2)?)),
            _ => None,
        };
    }
    let lower = s.to_ascii_lowercase();
    let inner = lower.strip_prefix("rgba(").or_else(|| lower.strip_prefix("rgb(")).and_then(|r| r.strip_suffix(')'))?;
    let parts: Vec<&str> = inner.split([',', ' ', '/']).filter(|p| !p.is_empty()).collect();
    if parts.len() < 3 || parts.len() > 4 {
        return None;
    }
    let ch = |p: &str| p.parse::<f32>().ok().filter(|v| (0.0..=255.0).contains(v)).map(|v| v.round() as u8);
    let a = match parts.get(3) {
        Some(p) => {
            let f = p.trim_end_matches('%').parse::<f32>().ok()?;
            let f = if p.ends_with('%') { f / 100.0 } else { f };
            (f.clamp(0.0, 1.0) * 255.0).round() as u8
        }
        None => 255,
    };
    Some(Color32::from_rgba_unmultiplied(ch(parts[0])?, ch(parts[1])?, ch(parts[2])?, a))
}

fn is_link(s: &str) -> bool {
    let s = s.trim();
    let rest = s.strip_prefix("https://").or_else(|| s.strip_prefix("http://")).or_else(|| s.strip_prefix("www."));
    rest.is_some_and(|r| r.contains('.') || r.contains('/') || r.starts_with("localhost")) && !s.chars().any(char::is_whitespace)
}

/// Apps whose text is almost always code.
const CODE_APPS: [&str; 12] = [
    "com.apple.Terminal",
    "com.googlecode.iterm2",
    "com.apple.dt.Xcode",
    "com.microsoft.VSCode",
    "dev.zed.Zed",
    "com.todesktop.230313mzl4w4u92",
    "com.jetbrains.intellij",
    "com.jetbrains.rustrover",
    "com.sublimetext.4",
    "dev.warp.Warp-Stable",
    "com.mitchellh.ghostty",
    "net.kovidgoyal.kitty",
];

fn looks_like_code(s: &str, bundle: &str) -> bool {
    if CODE_APPS.contains(&bundle) {
        return true;
    }
    let t = s.trim();
    let first = t.split_whitespace().next().unwrap_or("");
    const CMDS: [&str; 16] = ["cargo", "git", "npm", "npx", "yarn", "pnpm", "brew", "sudo", "cd", "ls", "python", "python3", "pip", "docker", "kubectl", "curl"];
    if !t.contains('\n') && (CMDS.contains(&first) || t.starts_with("./") || t.starts_with("$ ")) {
        return true;
    }
    let marks = ["{\n", "};", "=>", "fn ", "def ", "func ", "const ", "let ", "import ", "#include", "</", "();", "){", ") {", "->"];
    let hits = marks.iter().filter(|m| t.contains(*m)).count();
    hits >= 2 || (hits == 1 && t.lines().count() > 1)
}

/// What kind of entry a clip makes.
pub fn classify(c: &Clip) -> Kind {
    if !c.files.is_empty() {
        return Kind::File;
    }
    if c.png.is_some() {
        return Kind::Image;
    }
    let t = c.text.as_deref().unwrap_or("");
    if parse_color(t).is_some() {
        Kind::Color
    } else if is_link(t) {
        Kind::Link
    } else if looks_like_code(t, &c.bundle) {
        Kind::Code
    } else {
        Kind::Text
    }
}

/// Size of a PNG from its header.
pub fn png_size(png: &[u8]) -> Option<(u32, u32)> {
    if png.len() < 24 || &png[..8] != b"\x89PNG\r\n\x1a\n" {
        return None;
    }
    let w = u32::from_be_bytes(png[16..20].try_into().ok()?);
    let h = u32::from_be_bytes(png[20..24].try_into().ok()?);
    Some((w, h))
}

/// Limits applied when adding and when settings change.
#[derive(Clone, Copy, Debug)]
pub struct Limits {
    /// Unpinned entries kept.
    pub keep: usize,
    /// Unpinned entries older than this many days go (0 = never).
    pub max_days: u32,
    /// Copying something again moves it to the top instead of adding it twice.
    pub dedupe: bool,
}

pub struct Store {
    dir: PathBuf,
    /// Newest first.
    pub entries: Vec<Entry>,
    /// None: sample data that never saves.
    saves: bool,
}

impl Store {
    pub fn open(dir: PathBuf) -> Store {
        let entries = std::fs::read_to_string(dir.join("index.json")).ok().and_then(|s| serde_json::from_str::<Vec<Entry>>(&s).ok()).unwrap_or_default();
        Store { dir, entries, saves: true }
    }

    /// In memory only (tests that don't touch the disk).
    #[cfg(test)]
    pub fn memory(dir: PathBuf) -> Store {
        Store { dir, entries: Vec::new(), saves: false }
    }

    fn blob(&self, id: u64, ext: &str) -> PathBuf {
        self.dir.join("blobs").join(format!("{id}.{ext}"))
    }

    pub fn image_path(&self, id: u64) -> PathBuf {
        self.blob(id, "png")
    }

    pub fn get(&self, id: u64) -> Option<&Entry> {
        self.entries.iter().find(|e| e.id == id)
    }

    pub fn png(&self, id: u64) -> Option<Vec<u8>> {
        std::fs::read(self.image_path(id)).ok()
    }

    pub fn rtf(&self, id: u64) -> Option<Vec<u8>> {
        self.get(id).filter(|e| e.rtf).and_then(|_| std::fs::read(self.blob(id, "rtf")).ok())
    }

    pub fn html(&self, id: u64) -> Option<Vec<u8>> {
        self.get(id).filter(|e| e.html).and_then(|_| std::fs::read(self.blob(id, "html")).ok())
    }

    pub fn bytes_on_disk(&self) -> u64 {
        self.entries.iter().map(|e| e.bytes).sum()
    }

    fn save(&self) {
        if !self.saves {
            return;
        }
        // off the UI thread: the index (every clip's text) is rewritten on
        // every copy, and it can be megabytes
        crate::kit::fs::save_json_later(&self.dir.join("index.json"), &self.entries, false);
    }

    /// Add a clip copied at `now` (ms). Returns its id, or None for an empty
    /// clip. A repeat moves the old entry to the top when `dedupe` is on.
    pub fn add(&mut self, clip: &Clip, now: i64, lim: Limits) -> Option<u64> {
        if clip.is_empty() {
            return None;
        }
        let kind = classify(clip);
        let (text, h, w, ht) = match kind {
            Kind::File => {
                let t = clip.files.iter().map(|p| p.display().to_string()).collect::<Vec<_>>().join("\n");
                let h = hash(&[b"file", t.as_bytes()]);
                (t, h, 0, 0)
            }
            Kind::Image => {
                let png = clip.png.as_deref().unwrap_or_default();
                let (w, ht) = png_size(png).unwrap_or((0, 0));
                (format!("Image {w}×{ht}"), hash(&[b"png", png]), w, ht)
            }
            _ => {
                let t = clip.text.clone().unwrap_or_default();
                let h = hash(&[b"text", t.as_bytes()]);
                (t, h, 0, 0)
            }
        };
        if lim.dedupe {
            if let Some(i) = self.entries.iter().position(|e| e.hash == h) {
                let mut e = self.entries.remove(i);
                e.last = now;
                e.count += 1;
                e.app = clip.app.clone();
                e.bundle = clip.bundle.clone();
                let id = e.id;
                self.entries.insert(0, e);
                self.trim(lim, now);
                self.save();
                return Some(id);
            }
        }
        // ids are copy times in ms, nudged to stay unique
        let mut id = now.max(1) as u64;
        while self.entries.iter().any(|e| e.id == id) {
            id += 1;
        }
        let mut bytes = text.len() as u64;
        let mut e = Entry { id, kind, text, app: clip.app.clone(), bundle: clip.bundle.clone(), first: now, last: now, count: 1, pinned: false, w, h: ht, rtf: false, html: false, bytes: 0, hash: h };
        if self.saves {
            let _ = std::fs::create_dir_all(self.dir.join("blobs"));
            if let (Kind::Image, Some(png)) = (kind, &clip.png) {
                if std::fs::write(self.blob(id, "png"), png).is_err() {
                    return None;
                }
                bytes = png.len() as u64;
            }
            if kind != Kind::Image && kind != Kind::File {
                if let Some(r) = &clip.rtf {
                    e.rtf = std::fs::write(self.blob(id, "rtf"), r).is_ok();
                    bytes += r.len() as u64;
                }
                if let Some(hh) = &clip.html {
                    e.html = std::fs::write(self.blob(id, "html"), hh).is_ok();
                    bytes += hh.len() as u64;
                }
            }
        }
        e.bytes = bytes;
        self.entries.insert(0, e);
        self.trim(lim, now);
        self.save();
        Some(id)
    }

    fn drop_files(&self, e: &Entry) {
        for ext in ["png", "rtf", "html"] {
            let _ = std::fs::remove_file(self.blob(e.id, ext));
        }
    }

    /// Apply the limits: unpinned entries beyond `keep` or too old go.
    pub fn trim(&mut self, lim: Limits, now: i64) {
        let max_age = lim.max_days as i64 * 86_400_000;
        let mut kept = 0usize;
        let mut gone = Vec::new();
        self.entries.retain(|e| {
            if e.pinned {
                return true;
            }
            let old = lim.max_days > 0 && now - e.last > max_age;
            if old || kept >= lim.keep {
                gone.push(e.clone());
                return false;
            }
            kept += 1;
            true
        });
        for e in &gone {
            self.drop_files(e);
        }
        if !gone.is_empty() {
            self.save();
        }
    }

    pub fn remove(&mut self, id: u64) {
        if let Some(i) = self.entries.iter().position(|e| e.id == id) {
            let e = self.entries.remove(i);
            self.drop_files(&e);
            self.save();
        }
    }

    pub fn set_pinned(&mut self, id: u64, on: bool) {
        if let Some(e) = self.entries.iter_mut().find(|e| e.id == id) {
            e.pinned = on;
            self.save();
        }
    }

    /// Move an entry to the top, as if it was just copied (after pasting it).
    pub fn touch(&mut self, id: u64, now: i64) {
        if let Some(i) = self.entries.iter().position(|e| e.id == id) {
            let mut e = self.entries.remove(i);
            e.last = now;
            self.entries.insert(0, e);
            self.save();
        }
    }

    /// Remove everything that isn't pinned.
    pub fn clear_unpinned(&mut self) {
        let gone: Vec<Entry> = self.entries.iter().filter(|e| !e.pinned).cloned().collect();
        self.entries.retain(|e| e.pinned);
        for e in &gone {
            self.drop_files(e);
        }
        self.save();
    }

    /// Pinned first, then the rest; both newest first.
    pub fn ordered(&self) -> Vec<&Entry> {
        self.entries.iter().filter(|e| e.pinned).chain(self.entries.iter().filter(|e| !e.pinned)).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn text(t: &str) -> Clip {
        Clip { text: Some(t.into()), app: "Notes".into(), bundle: "com.apple.Notes".into(), ..Default::default() }
    }

    const LIM: Limits = Limits { keep: 3, max_days: 30, dedupe: true };

    #[test]
    fn kinds() {
        assert_eq!(classify(&text("hello there")), Kind::Text);
        assert_eq!(classify(&text("https://github.com/x")), Kind::Link);
        assert_eq!(classify(&text("www.apple.com")), Kind::Link);
        assert_eq!(classify(&text("see https://x.com now")), Kind::Text);
        assert_eq!(classify(&text("#0B6BD8")), Kind::Color);
        assert_eq!(classify(&text("rgb(11, 107, 216)")), Kind::Color);
        assert_eq!(classify(&text("cargo test --workspace")), Kind::Code);
        assert_eq!(classify(&text("fn main() {\n    let x = 1;\n}")), Kind::Code);
        let mut t = text("anything");
        t.bundle = "com.apple.Terminal".into();
        assert_eq!(classify(&t), Kind::Code);
        let f = Clip { files: vec!["/a/b.pdf".into()], ..Default::default() };
        assert_eq!(classify(&f), Kind::File);
    }

    #[test]
    fn colors() {
        assert_eq!(parse_color("#fff"), Some(Color32::WHITE));
        assert_eq!(parse_color(" #0B6BD8 "), Some(Color32::from_rgb(11, 107, 216)));
        assert_eq!(parse_color("rgba(11, 107, 216, 0.5)").map(|c| c.a()), Some(128));
        assert_eq!(parse_color("#12345"), None);
        assert_eq!(parse_color("#ggg"), None);
        assert_eq!(parse_color("rgb(300, 0, 0)"), None);
    }

    #[test]
    fn png_header() {
        let mut png = Vec::new();
        image::RgbaImage::new(7, 5).write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png).unwrap();
        assert_eq!(png_size(&png), Some((7, 5)));
        assert_eq!(png_size(b"nope"), None);
    }

    #[test]
    fn repeats_move_to_top_and_limits_apply() {
        let mut s = Store::memory(std::env::temp_dir());
        let a = s.add(&text("a"), 1_000, LIM).unwrap();
        s.add(&text("b"), 2_000, LIM).unwrap();
        assert_eq!(s.add(&text("a"), 3_000, LIM), Some(a));
        assert_eq!(s.entries[0].id, a);
        assert_eq!(s.entries[0].count, 2);
        assert_eq!(s.entries.len(), 2);
        assert_eq!(s.add(&text("  "), 3_500, LIM), None);
        s.set_pinned(a, true);
        for (k, t) in ["c", "d", "e", "f"].iter().enumerate() {
            s.add(&text(t), 4_000 + k as i64, LIM);
        }
        // 3 unpinned kept + the pinned one
        assert_eq!(s.entries.len(), 4);
        assert!(s.get(a).is_some());
        assert_eq!(s.ordered()[0].id, a);
        let titles: Vec<String> = s.ordered().iter().skip(1).map(|e| e.text.clone()).collect();
        assert_eq!(titles, vec!["f", "e", "d"]);
    }

    #[test]
    fn without_dedupe_repeats_add() {
        let mut s = Store::memory(std::env::temp_dir());
        let lim = Limits { dedupe: false, ..LIM };
        s.add(&text("a"), 1, lim);
        s.add(&text("a"), 2, lim);
        assert_eq!(s.entries.len(), 2);
        assert_ne!(s.entries[0].id, s.entries[1].id);
    }

    #[test]
    fn old_entries_age_out_unless_pinned() {
        let mut s = Store::memory(std::env::temp_dir());
        let day = 86_400_000;
        let old = s.add(&text("old"), 0, LIM).unwrap();
        let keep = s.add(&text("pinned"), 1, LIM).unwrap();
        s.set_pinned(keep, true);
        s.trim(Limits { keep: 100, max_days: 1, dedupe: true }, 2 * day);
        assert!(s.get(old).is_none());
        assert!(s.get(keep).is_some());
        s.trim(Limits { keep: 100, max_days: 0, dedupe: true }, 900 * day);
        assert!(s.get(keep).is_some());
    }

    #[test]
    fn saves_blobs_and_reloads() {
        let dir = std::env::temp_dir().join(format!("cmb-clip-store-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let mut png = Vec::new();
        image::RgbaImage::new(4, 3).write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png).unwrap();
        let mut s = Store::open(dir.clone());
        let img = s.add(&Clip { png: Some(png.clone()), app: "Capture".into(), ..Default::default() }, 10, LIM).unwrap();
        let rich = s.add(&Clip { text: Some("bold".into()), rtf: Some(b"{\\rtf1 bold}".to_vec()), ..Default::default() }, 11, LIM).unwrap();
        crate::kit::fs::flush_writes();
        let back = Store::open(dir.clone());
        assert_eq!(back.entries.len(), 2);
        assert_eq!(back.get(img).map(|e| (e.kind, e.w, e.h)), Some((Kind::Image, 4, 3)));
        assert_eq!(back.png(img), Some(png));
        assert_eq!(back.rtf(rich).as_deref(), Some(&b"{\\rtf1 bold}"[..]));
        let mut back = back;
        back.clear_unpinned();
        assert!(back.entries.is_empty());
        assert!(!back.image_path(img).exists());
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn titles_and_search() {
        let mut s = Store::memory(std::env::temp_dir());
        s.add(&Clip { files: vec!["/x/Q4 report.pdf".into(), "/x/b.txt".into()], app: "Finder".into(), ..Default::default() }, 1, LIM);
        assert_eq!(s.entries[0].title(), "Q4 report.pdf and 1 more");
        assert!(s.entries[0].matches("finder q4"));
        assert!(!s.entries[0].matches("slack"));
        s.add(&text("https://github.com/romanfurman6/"), 2, LIM);
        assert_eq!(s.entries[0].title(), "github.com/romanfurman6");
        s.add(&text("line one\nline two"), 3, LIM);
        assert_eq!(s.entries[0].summary(), "2 lines · 17 characters");
    }
}
