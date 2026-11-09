//! The video viewer: play a recording, trim its ends, cut pieces out of the
//! middle, and save (over the file, or as a copy). Opened from the saved
//! card, History and the Folder.
//!
//! The cuts live in an [`edit::Edl`] until Save; playback skips them, and
//! the file is only written by [`media::export`] (no re-encoding).

pub mod edit;
pub mod media;
mod ui;

use edit::Edl;
use media::Player;
use std::path::PathBuf;
use std::sync::mpsc::{channel, Receiver, Sender};
use std::time::Instant;

pub use ui::show;

pub const SPEEDS: [f32; 3] = [1.0, 1.5, 2.0];
/// Frames in the timeline strip.
const STRIP: usize = 24;

/// What the viewer needs Capture to do.
pub enum Out {
    /// A file was written: over the original (`copy` false) or beside it.
    Saved { path: PathBuf, secs: f64, copy: bool },
}

enum Event {
    Strip(Vec<image::RgbaImage>),
    Saved(Result<(PathBuf, bool), String>),
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum Drag {
    /// Moving the start or end of what's kept.
    Trim { end: bool },
    /// Choosing a stretch, from where it began (source seconds).
    Select(f64),
    Scrub,
}

pub struct Viewer {
    pub path: PathBuf,
    pub edl: Edl,
    undo: Vec<Edl>,
    redo: Vec<Edl>,
    player: Option<Player>,
    error: Option<String>,
    tex: Option<egui::TextureHandle>,
    /// The picture's size in pixels (for fitting it).
    size: (u32, u32),
    strip: Vec<egui::TextureHandle>,
    strip_imgs: Vec<image::RgbaImage>,
    /// A chosen stretch, source seconds, a < b.
    pub sel: Option<(f64, f64)>,
    speed: usize,
    drag: Option<Drag>,
    saving: bool,
    status: Option<(String, Instant)>,
    /// Ask before closing with unsaved cuts.
    asking: bool,
    /// Repaint for a moment after a seek, so its frame shows up.
    kick: Instant,
    pub opened: bool,
    pub out: Vec<Out>,
    tx: Sender<Event>,
    rx: Receiver<Event>,
    ctx: egui::Context,
}

impl Viewer {
    pub fn open(ctx: &egui::Context, path: PathBuf) -> Viewer {
        let (tx, rx) = channel();
        let mut v = Viewer {
            path,
            edl: Edl::new(0.0),
            undo: Vec::new(),
            redo: Vec::new(),
            player: None,
            error: None,
            tex: None,
            size: (16, 9),
            strip: Vec::new(),
            strip_imgs: Vec::new(),
            sel: None,
            speed: 0,
            drag: None,
            saving: false,
            status: None,
            asking: false,
            kick: Instant::now(),
            opened: false,
            out: Vec::new(),
            tx,
            rx,
            ctx: ctx.clone(),
        };
        v.load();
        v
    }

    /// (Re)open the file: the player, a fresh edit, the strip.
    fn load(&mut self) {
        self.player = None;
        self.tex = None;
        self.strip.clear();
        self.strip_imgs.clear();
        self.undo.clear();
        self.redo.clear();
        self.sel = None;
        match Player::open(&self.path) {
            Ok(p) => {
                self.edl = Edl::new(p.duration);
                self.size = p.size;
                p.seek(0.0);
                self.player = Some(p);
                self.error = None;
            }
            Err(e) => {
                self.edl = Edl::new(0.0);
                self.error = Some(e);
            }
        }
        self.kick = Instant::now();
        let (tx, ctx, path) = (self.tx.clone(), self.ctx.clone(), self.path.clone());
        std::thread::spawn(move || {
            let imgs = media::strip(&path, STRIP, 112.0);
            let _ = tx.send(Event::Strip(imgs));
            ctx.request_repaint();
        });
    }

    pub fn title(&self) -> String {
        self.path.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_else(|| "Video".into())
    }

    pub fn edited(&self) -> bool {
        !self.edl.untouched()
    }

    pub fn duration(&self) -> f64 {
        self.edl.duration
    }

    pub fn time(&self) -> f64 {
        self.player.as_ref().map(|p| p.time()).unwrap_or(0.0)
    }

    pub fn playing(&self) -> bool {
        self.player.as_ref().is_some_and(|p| p.playing())
    }

    fn flash(&mut self, m: impl Into<String>) {
        self.status = Some((m.into(), Instant::now()));
    }

    // ------------------------------------------------------------ playback

    pub fn toggle_play(&mut self) {
        let Some(p) = &self.player else { return };
        if p.playing() {
            p.pause();
            return;
        }
        let t = p.time();
        let at_end = t >= self.edl.end() - 0.05;
        let from = if at_end { Some(self.edl.start()) } else { self.edl.playable(t) };
        if let Some(f) = from {
            if (f - t).abs() > 1e-3 {
                p.seek(f);
            }
        }
        p.play(SPEEDS[self.speed]);
    }

    pub fn seek(&mut self, t: f64) {
        if let Some(p) = &self.player {
            p.seek(t.clamp(0.0, self.edl.duration));
            self.kick = Instant::now();
        }
    }

    pub fn set_speed(&mut self, k: usize) {
        self.speed = k.min(SPEEDS.len() - 1);
        if let Some(p) = &self.player {
            if p.playing() {
                p.play(SPEEDS[self.speed]);
            }
        }
    }

    /// Step `d` seconds of the edited video.
    pub fn step(&mut self, d: f64) {
        let e = self.edl.to_edited(self.time()).unwrap_or_else(|| self.edl.to_edited(self.edl.playable(self.time()).unwrap_or(self.edl.end())).unwrap_or(0.0));
        let t = self.edl.to_source((e + d).clamp(0.0, self.edl.length()));
        self.seek(t);
    }

    // ------------------------------------------------------------ edits

    fn change(&mut self, f: impl FnOnce(&mut Edl) -> bool) -> bool {
        let before = self.edl.clone();
        if f(&mut self.edl) {
            self.undo.push(before);
            self.redo.clear();
            true
        } else {
            false
        }
    }

    pub fn cut_selection(&mut self) {
        let Some((a, b)) = self.sel else { return };
        if self.change(|e| e.cut(a, b)) {
            self.sel = None;
            if let Some(n) = self.edl.playable(a) {
                self.seek(n);
            }
        } else {
            self.flash("Can't cut all of it");
        }
    }

    /// Keep only the selection.
    pub fn keep_selection(&mut self) {
        let Some((a, b)) = self.sel else { return };
        if self.change(|e| {
            let x = e.trim_start(a);
            let y = e.trim_end(b);
            x || y
        }) {
            self.sel = None;
            self.seek(self.edl.start());
        }
    }

    pub fn undo(&mut self) {
        if let Some(e) = self.undo.pop() {
            self.redo.push(std::mem::replace(&mut self.edl, e));
        }
    }

    pub fn redo(&mut self) {
        if let Some(e) = self.redo.pop() {
            self.undo.push(std::mem::replace(&mut self.edl, e));
        }
    }

    /// Start of a trim-handle drag: one undo step for the whole drag.
    fn begin_trim(&mut self) {
        self.undo.push(self.edl.clone());
        self.redo.clear();
    }

    fn trim_to(&mut self, end: bool, t: f64) {
        if end {
            let t = t.max(self.edl.start() + 0.2);
            self.edl.trim_end(t);
        } else {
            let t = t.min(self.edl.end() - 0.2);
            self.edl.trim_start(t);
        }
    }

    /// A drag that changed nothing leaves no undo step.
    fn end_trim(&mut self) {
        if self.undo.last() == Some(&self.edl) {
            self.undo.pop();
        }
    }

    // ------------------------------------------------------------ saving

    /// Write the cuts: over the file, or as a copy beside it.
    pub fn save(&mut self, copy: bool) {
        if self.saving || self.player.is_none() || (!copy && !self.edited()) {
            return;
        }
        if let Some(p) = &self.player {
            p.pause();
        }
        self.saving = true;
        let dest = if copy { media::copy_name(&self.path) } else { media::temp_beside(&self.path) };
        let (tx, ctx) = (self.tx.clone(), self.ctx.clone());
        media::export(
            &self.path,
            &self.edl,
            dest,
            Box::new(move |r| {
                let _ = tx.send(Event::Saved(r.map(|p| (p, copy))));
                ctx.request_repaint();
            }),
        );
    }

    /// Background results; call once per pass.
    pub fn tick(&mut self) {
        while let Ok(e) = self.rx.try_recv() {
            match e {
                Event::Strip(imgs) => {
                    self.strip_imgs = imgs;
                    self.strip.clear();
                }
                Event::Saved(Ok((written, copy))) => {
                    self.saving = false;
                    let secs = self.edl.length();
                    if copy {
                        self.flash(format!("Saved a copy: {}", written.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default()));
                        self.out.push(Out::Saved { path: written, secs, copy: true });
                    } else {
                        // the player lets go of the file before it's replaced
                        self.player = None;
                        match std::fs::rename(&written, &self.path) {
                            Ok(()) => {
                                self.load();
                                self.flash("Saved");
                                self.out.push(Out::Saved { path: self.path.clone(), secs: self.edl.duration, copy: false });
                            }
                            Err(e) => {
                                let _ = std::fs::remove_file(&written);
                                self.load();
                                self.flash(format!("Couldn't save: {e}"));
                            }
                        }
                    }
                }
                Event::Saved(Err(e)) => {
                    self.saving = false;
                    self.flash(format!("Couldn't save: {e}"));
                }
            }
        }
        // playback jumps over cuts and stops at the end
        if let Some(p) = &self.player {
            if p.playing() {
                let t = p.time();
                if t >= self.edl.end() - 1e-3 {
                    p.pause();
                    p.seek(self.edl.end());
                } else if !self.edl.kept(t) {
                    match self.edl.playable(t) {
                        Some(n) => p.seek(n),
                        None => p.pause(),
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn viewer(d: f64) -> Viewer {
        let mut v = Viewer::open(&egui::Context::default(), PathBuf::from("/nowhere.mp4"));
        v.edl = Edl::new(d);
        v
    }

    #[test]
    fn a_missing_file_says_so() {
        let v = Viewer::open(&egui::Context::default(), PathBuf::from("/nowhere.mp4"));
        assert!(v.error.is_some());
        assert!(!v.edited());
    }

    #[test]
    fn cuts_undo_and_redo() {
        let mut v = viewer(10.0);
        v.sel = Some((2.0, 4.0));
        v.cut_selection();
        assert_eq!(v.edl.keep, vec![(0.0, 2.0), (4.0, 10.0)]);
        assert!(v.sel.is_none());
        v.undo();
        assert!(!v.edited());
        v.redo();
        assert!(v.edited());
    }

    #[test]
    fn keeping_a_selection_trims_both_ends() {
        let mut v = viewer(10.0);
        v.sel = Some((3.0, 7.0));
        v.keep_selection();
        assert_eq!(v.edl.keep, vec![(3.0, 7.0)]);
        v.undo();
        assert!(!v.edited(), "one step");
    }

    #[test]
    fn a_trim_drag_is_one_undo_step_and_none_if_nothing_moved() {
        let mut v = viewer(10.0);
        v.begin_trim();
        v.end_trim();
        assert!(v.undo.is_empty());
        v.begin_trim();
        v.trim_to(false, 1.0);
        v.trim_to(false, 2.0);
        v.end_trim();
        assert_eq!(v.undo.len(), 1);
        assert_eq!(v.edl.start(), 2.0);
        v.trim_to(true, 1.0);
        assert!(v.edl.end() >= 2.2, "the end stays after the start");
    }
}
