//! The video viewer: play a recording, trim its ends, cut pieces out of the
//! middle, and save (over the file, or as a copy). Opened from the saved
//! card, History and the Folder.
//!
//! The cuts live in an [`edit::Edl`] until Save; playback skips them, and
//! the file is only written by [`media::export`] (no re-encoding).
//!
//! The side panel transcribes the voice on this Mac ([`speech`], whisper,
//! the model downloaded once on request) and cleans it up ([`voice`],
//! DeepFilterNet3, built in); the cleaned sound goes into the file on Save.

pub mod edit;
pub mod media;
pub mod sound;
pub mod speech;
pub mod voice;
mod ui;

use edit::Edl;
use media::Player;
use serde::{Deserialize, Serialize};
use speech::{Line, Model, Progress};
use std::path::PathBuf;
use std::sync::atomic::Ordering;
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::Arc;
use std::time::Instant;

pub use ui::show;

pub const SPEEDS: [f32; 3] = [1.0, 1.5, 2.0];
/// Frames in the timeline strip.
const STRIP: usize = 24;

/// Buckets in the voice waveform.
const PEAKS: usize = 1200;

/// What the viewer needs Capture to do.
pub enum Out {
    /// A file was written: over the original (`copy` false) or beside it.
    Saved { path: PathBuf, secs: f64, copy: bool },
    CopyText(String),
    /// Upload the file for a link (Share).
    Share(PathBuf),
}

enum Event {
    Strip(Vec<image::RgbaImage>),
    Saved(Result<(PathBuf, bool), String>),
    Peaks(usize, Vec<f32>),
    Downloaded(Model, Result<(), String>),
    Transcribed(Result<Vec<Line>, String>),
    Cleaned(Result<(PathBuf, Vec<f32>), String>),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tab {
    Transcript,
    Audio,
}

/// Work running on a thread.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum JobKind {
    Download(Model),
    Transcribe,
    Clean,
}

pub struct Job {
    pub kind: JobKind,
    pub progress: Arc<Progress>,
}

/// The viewer's choices, kept between videos.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct Prefs {
    pub model: Model,
    pub captions: bool,
    pub strength: f32,
}

impl Default for Prefs {
    fn default() -> Prefs {
        Prefs { model: Model::Accurate, captions: true, strength: 0.7 }
    }
}

fn prefs_path() -> PathBuf {
    crate::kit::fs::data_path("video-viewer.json")
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
    pub tab: Tab,
    pub prefs: Prefs,
    /// The transcript (source seconds), when there is one.
    pub lines: Vec<Line>,
    /// Lines chosen in the transcript (first, last).
    pub picked: Option<(usize, usize)>,
    pub job: Option<Job>,
    /// The last job's failure, shown in the panel.
    pub job_error: Option<String>,
    /// Audio tracks in the file (3: mix, system, mic).
    pub tracks: usize,
    /// The voice's loudness, and the cleaned voice's.
    pub peaks: Vec<f32>,
    pub clean_peaks: Vec<f32>,
    /// The cleaned sound, and whether it's what plays (and gets saved).
    pub voice: Option<PathBuf>,
    pub voice_on: bool,
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
            tab: Tab::Transcript,
            prefs: crate::kit::fs::load_json(&prefs_path()),
            lines: Vec::new(),
            picked: None,
            job: None,
            job_error: None,
            tracks: 0,
            peaks: Vec::new(),
            clean_peaks: Vec::new(),
            voice: None,
            voice_on: false,
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
        self.picked = None;
        self.voice = None;
        self.voice_on = false;
        self.clean_peaks.clear();
        self.peaks.clear();
        self.lines = speech::load(&self.path).unwrap_or_default();
        match Player::open(&self.path, None) {
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
            let tracks = sound::audio_tracks(&path);
            let _ = tx.send(Event::Peaks(tracks, voice::voice_peaks(&path, PEAKS)));
            ctx.request_repaint();
        });
    }

    /// Open the player again (the cleaned sound on or off), where it was.
    fn reopen(&mut self) {
        let t = self.time();
        let voice = self.voice.clone().filter(|_| self.voice_on);
        self.player = None;
        match Player::open(&self.path, voice.as_deref()) {
            Ok(p) => {
                p.seek(t);
                self.player = Some(p);
            }
            Err(e) => self.error = Some(e),
        }
        self.kick = Instant::now();
    }

    pub fn save_prefs(&self) {
        crate::kit::fs::save_json(&prefs_path(), &self.prefs, true);
    }

    // ------------------------------------------------------------ jobs

    pub fn busy(&self) -> bool {
        self.job.is_some()
    }

    fn spawn(&mut self, kind: JobKind, work: impl FnOnce(&Arc<Progress>) -> Event + Send + 'static) {
        if self.job.is_some() {
            return;
        }
        let progress = Arc::new(Progress::default());
        self.job = Some(Job { kind, progress: progress.clone() });
        self.job_error = None;
        let (tx, ctx) = (self.tx.clone(), self.ctx.clone());
        std::thread::spawn(move || {
            let e = work(&progress);
            let _ = tx.send(e);
            ctx.request_repaint();
        });
    }

    pub fn cancel_job(&mut self) {
        if let Some(j) = &self.job {
            j.progress.cancel.store(true, Ordering::Relaxed);
        }
    }

    /// Fetch the chosen speech model.
    pub fn download(&mut self, m: Model) {
        self.spawn(JobKind::Download(m), move |p| Event::Downloaded(m, speech::download(m, p)));
    }

    /// Turn the voice into timed lines with the chosen model.
    pub fn transcribe(&mut self) {
        let m = self.prefs.model;
        if !m.ready() {
            self.download(m);
            return;
        }
        let path = self.path.clone();
        self.spawn(JobKind::Transcribe, move |p| Event::Transcribed(speech::transcribe(&path, m, p)));
    }

    /// Take the noise out of the voice (at the chosen strength).
    pub fn clean_voice(&mut self) {
        let (path, strength) = (self.path.clone(), self.prefs.strength);
        self.spawn(JobKind::Clean, move |p| Event::Cleaned(voice::clean(&path, strength, p).map(|f| {
            let peaks = voice::cleaned_peaks(&f, PEAKS);
            (f, peaks)
        })));
    }

    /// Hear (and save) the cleaned voice, or the original.
    pub fn set_voice_on(&mut self, on: bool) {
        if self.voice.is_some() && on != self.voice_on {
            self.voice_on = on;
            self.reopen();
        }
    }

    // ------------------------------------------------------------ transcript

    /// Pick line `i`; with `extend`, from the line picked before to it.
    pub fn pick_line(&mut self, i: usize, extend: bool) {
        if i >= self.lines.len() {
            return;
        }
        let (a, b) = match (self.picked, extend) {
            (Some((a, _)), true) => (a.min(i), a.max(i)),
            _ => (i, i),
        };
        self.picked = Some((a, b));
        self.sel = Some((self.lines[a].start, self.lines[b].end));
        self.seek(self.lines[a].start);
    }

    /// The lines as they'll be in the saved video.
    pub fn edited_lines(&self) -> Vec<Line> {
        speech::edited_lines(&self.lines, &self.edl)
    }

    pub fn copy_text(&mut self) {
        let t = speech::txt(&self.edited_lines());
        self.out.push(Out::CopyText(t));
        self.flash("Copied the text");
    }

    /// Write captions (.srt) or text (.txt) beside the video.
    pub fn export_text(&mut self, srt: bool) {
        let lines = self.edited_lines();
        let (body, ext) = if srt { (speech::srt(&lines), "srt") } else { (speech::txt(&lines), "txt") };
        let path = self.path.with_extension(ext);
        match std::fs::write(&path, body) {
            Ok(()) => self.flash(format!("Saved {}", path.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default())),
            Err(e) => self.flash(format!("Couldn't save: {e}")),
        }
    }

    /// The line to show over the picture now.
    pub fn caption(&self) -> Option<&str> {
        if !self.prefs.captions {
            return None;
        }
        let t = self.time();
        if !self.edl.kept(t) {
            return None;
        }
        speech::line_at(&self.lines, t).map(|i| self.lines[i].text.as_str())
    }

    pub fn title(&self) -> String {
        self.path.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_else(|| "Video".into())
    }

    /// Cuts not saved yet.
    pub fn edited(&self) -> bool {
        !self.edl.untouched()
    }

    /// Anything Save would write: cuts or the cleaned voice.
    pub fn dirty(&self) -> bool {
        self.edited() || (self.voice_on && self.voice.is_some())
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
        if self.saving || self.player.is_none() || self.busy() || (!copy && !self.dirty()) {
            return;
        }
        if let Some(p) = &self.player {
            p.pause();
        }
        self.saving = true;
        let dest = if copy { media::copy_name(&self.path) } else { media::temp_beside(&self.path) };
        let (tx, ctx) = (self.tx.clone(), self.ctx.clone());
        let voice = self.voice.clone().filter(|_| self.voice_on);
        media::export(
            &self.path,
            voice.as_deref(),
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
                    let lines = self.edited_lines();
                    if copy {
                        if !lines.is_empty() {
                            speech::store(&written, &lines);
                        }
                        self.flash(format!("Saved a copy: {}", written.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default()));
                        self.out.push(Out::Saved { path: written, secs, copy: true });
                    } else {
                        // the player lets go of the file before it's replaced
                        self.player = None;
                        match std::fs::rename(&written, &self.path) {
                            Ok(()) => {
                                if let Some(v) = self.voice.take() {
                                    let _ = std::fs::remove_file(v);
                                }
                                if !lines.is_empty() {
                                    speech::store(&self.path, &lines);
                                }
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
                Event::Peaks(n, p) => {
                    self.tracks = n;
                    self.peaks = p;
                }
                Event::Downloaded(m, r) => {
                    self.job = None;
                    match r {
                        // straight on to what the download was for
                        Ok(()) if m == self.prefs.model && self.lines.is_empty() => self.transcribe(),
                        Ok(()) => {}
                        Err(e) => self.job_error = Some(e),
                    }
                }
                Event::Transcribed(r) => {
                    self.job = None;
                    match r {
                        Ok(lines) => {
                            speech::store(&self.path, &lines);
                            if lines.is_empty() {
                                self.job_error = Some("No speech found".into());
                            }
                            self.lines = lines;
                            self.picked = None;
                        }
                        Err(e) => self.job_error = Some(e),
                    }
                }
                Event::Cleaned(r) => {
                    self.job = None;
                    match r {
                        Ok((f, peaks)) => {
                            self.voice = Some(f);
                            self.clean_peaks = peaks;
                            self.voice_on = false;
                            self.set_voice_on(true);
                            self.flash("Voice cleaned up: Save puts it in the video");
                        }
                        Err(e) => self.job_error = Some(e),
                    }
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

/// `--video-probe`: the viewer's work on `path` without UI, printed to
/// stderr. Writes only temp files and "<name> edited.mp4" beside it.
pub fn probe(path: &std::path::Path, download: bool) {
    let t = Instant::now();
    let n = sound::audio_tracks(path);
    eprintln!("[video] {} audio tracks: {n}", path.display());
    for (k, name) in ["mix", "system", "mic"].iter().enumerate().take(n) {
        if let Ok(s) = sound::read_nth(path, k, 8_000, 1) {
            eprintln!("[video] track {k} ({name}): peak {:.3}", s.iter().fold(0.0f32, |m, v| m.max(v.abs())));
        }
    }
    match sound::read_audio(path, sound::voice_track(n), 16_000, 1) {
        Ok(s) => eprintln!("[video] voice at 16 kHz: {} samples ({:.2} s), peak {:.3} ({:?})", s.len(), s.len() as f64 / 16_000.0, s.iter().fold(0.0f32, |m, v| m.max(v.abs())), t.elapsed()),
        Err(e) => eprintln!("[video] reading the voice failed: {e}"),
    }
    let p = Progress::default();
    let t = Instant::now();
    let cleaned = match voice::clean(path, 0.7, &p) {
        Ok(f) => {
            let peaks = voice::cleaned_peaks(&f, 50);
            eprintln!("[video] cleaned voice: {} ({} bytes, {} audio tracks, peak {:.3}) in {:?}", f.display(), std::fs::metadata(&f).map(|m| m.len()).unwrap_or(0), sound::audio_tracks(&f), peaks.iter().fold(0.0f32, |a, b| a.max(*b)), t.elapsed());
            Some(f)
        }
        Err(e) => {
            eprintln!("[video] voice cleanup failed: {e}");
            None
        }
    };
    let d = media::duration(path);
    eprintln!("[video] length {d:.2} s");
    let mut edl = Edl::new(d);
    edl.cut(d * 0.25, d * 0.5);
    let dest = media::copy_name(path);
    let (tx, rx) = channel();
    let t = Instant::now();
    media::export(path, cleaned.as_deref(), &edl, dest, Box::new(move |r| drop(tx.send(r))));
    match rx.recv_timeout(std::time::Duration::from_secs(120)) {
        Ok(Ok(f)) => eprintln!("[video] saved a copy with a quarter cut and the cleaned voice: {} ({} audio tracks) in {:?}", f.display(), sound::audio_tracks(&f), t.elapsed()),
        Ok(Err(e)) => eprintln!("[video] saving failed: {e}"),
        Err(_) => eprintln!("[video] saving timed out"),
    }
    let m = Model::Fast;
    if !m.ready() && download {
        let p = Progress::default();
        eprintln!("[video] downloading {}…", m.file());
        if let Err(e) = speech::download(m, &p) {
            eprintln!("[video] download failed: {e}");
        }
    }
    if let Some(m) = [Model::Accurate, Model::Fast].into_iter().find(|m| m.ready()) {
        let t = Instant::now();
        match speech::transcribe(path, m, &Arc::new(Progress::default())) {
            Ok(lines) => {
                eprintln!("[video] {} lines with {} in {:?}:", lines.len(), m.name(), t.elapsed());
                for l in lines.iter().take(10) {
                    eprintln!("[video]   {:6.2}–{:6.2} {}", l.start, l.end, l.text);
                }
            }
            Err(e) => eprintln!("[video] transcribing failed: {e}"),
        }
    } else {
        eprintln!("[video] no speech model (pass --download to fetch the fast one)");
    }
    if let Some(f) = cleaned {
        let _ = std::fs::remove_file(f);
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
