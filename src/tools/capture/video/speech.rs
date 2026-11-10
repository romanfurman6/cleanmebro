//! Speech to text on this Mac with whisper.cpp: the models (downloaded on
//! request, resumable), turning a recording's voice into timed lines, and
//! those lines as captions (.srt) or plain text. Nothing leaves the Mac
//! except the one-time model download.

use super::edit::Edl;
use serde::{Deserialize, Serialize};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Model {
    /// Whisper large-v3-turbo, quantized: close to the best, still quick on
    /// Apple silicon.
    #[default]
    Accurate,
    /// Whisper small: a third of the size, rougher.
    Fast,
}

impl Model {
    pub const ALL: [Model; 2] = [Model::Accurate, Model::Fast];

    pub fn file(self) -> &'static str {
        match self {
            Model::Accurate => "ggml-large-v3-turbo-q5_0.bin",
            Model::Fast => "ggml-small-q5_1.bin",
        }
    }

    pub fn url(self) -> String {
        format!("https://huggingface.co/ggerganov/whisper.cpp/resolve/main/{}", self.file())
    }

    /// About how big the download is.
    pub fn bytes(self) -> u64 {
        match self {
            Model::Accurate => 574_041_195,
            Model::Fast => 190_085_487,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Model::Accurate => "Accurate",
            Model::Fast => "Fast",
        }
    }

    pub fn about(self) -> &'static str {
        match self {
            Model::Accurate => "Whisper large-v3 turbo · best for most voices and languages",
            Model::Fast => "Whisper small · quicker, rougher",
        }
    }

    pub fn path(self) -> PathBuf {
        models_dir().join(self.file())
    }

    pub fn ready(self) -> bool {
        self.path().is_file()
    }

    /// Delete the downloaded model.
    pub fn remove(self) {
        let _ = std::fs::remove_file(self.path());
        let _ = std::fs::remove_file(part_path(self));
    }
}

pub fn models_dir() -> PathBuf {
    crate::kit::fs::data_path("models")
}

fn part_path(m: Model) -> PathBuf {
    models_dir().join(format!("{}.part", m.file()))
}

/// Shared between a job's thread and the window: how far, and a way to stop.
#[derive(Default)]
pub struct Progress {
    pub done: AtomicU64,
    pub total: AtomicU64,
    pub cancel: AtomicBool,
}

impl Progress {
    pub fn fraction(&self) -> f32 {
        let t = self.total.load(Ordering::Relaxed);
        if t == 0 {
            0.0
        } else {
            (self.done.load(Ordering::Relaxed) as f64 / t as f64).clamp(0.0, 1.0) as f32
        }
    }

    pub fn cancelled(&self) -> bool {
        self.cancel.load(Ordering::Relaxed)
    }
}

/// Fetch `m`, picking up a download that stopped part way. Blocks.
pub fn download(m: Model, p: &Progress) -> Result<(), String> {
    std::fs::create_dir_all(models_dir()).map_err(|e| e.to_string())?;
    let part = part_path(m);
    let have = std::fs::metadata(&part).map(|x| x.len()).unwrap_or(0);
    let agent = ureq::Agent::new_with_config(ureq::Agent::config_builder().timeout_connect(Some(std::time::Duration::from_secs(20))).http_status_as_error(false).build());
    let mut req = agent.get(&m.url()).header("User-Agent", "Cmd+Meow");
    if have > 0 {
        req = req.header("Range", format!("bytes={have}-"));
    }
    let resp = req.call().map_err(|e| format!("Couldn't reach the download: {e}"))?;
    let status = resp.status().as_u16();
    let resumed = status == 206;
    if status != 200 && !resumed {
        return Err(format!("The download answered {status}"));
    }
    let len = resp.headers().get("content-length").and_then(|v| v.to_str().ok()).and_then(|v| v.parse::<u64>().ok());
    let start = if resumed { have } else { 0 };
    let total = len.map(|l| l + start).unwrap_or(m.bytes());
    p.total.store(total, Ordering::Relaxed);
    p.done.store(start, Ordering::Relaxed);
    let mut file = std::fs::OpenOptions::new().create(true).append(resumed).write(true).truncate(!resumed).open(&part).map_err(|e| e.to_string())?;
    let mut body = resp.into_body();
    let mut reader = body.as_reader();
    let mut buf = vec![0u8; 1 << 16];
    let mut done = start;
    loop {
        if p.cancelled() {
            return Err("Stopped".into());
        }
        let n = reader.read(&mut buf).map_err(|e| format!("The download broke off: {e}"))?;
        if n == 0 {
            break;
        }
        file.write_all(&buf[..n]).map_err(|e| e.to_string())?;
        done += n as u64;
        p.done.store(done, Ordering::Relaxed);
    }
    file.flush().map_err(|e| e.to_string())?;
    drop(file);
    if len.is_some_and(|l| done < l + start) {
        return Err("The download ended early; try again to pick it up".into());
    }
    // a whisper.cpp model starts with "lmgg"
    let mut magic = [0u8; 4];
    std::fs::File::open(&part).and_then(|mut f| f.read_exact(&mut magic)).map_err(|e| e.to_string())?;
    if &magic != b"lmgg" {
        let _ = std::fs::remove_file(&part);
        return Err("That wasn't a speech model; try again".into());
    }
    std::fs::rename(&part, m.path()).map_err(|e| e.to_string())
}

/// One timed line of speech, source seconds.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Line {
    pub start: f64,
    pub end: f64,
    pub text: String,
}

/// A transcript kept for a file (in the app's data, not beside the video).
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Saved {
    pub path: PathBuf,
    pub len: u64,
    pub modified: u64,
    pub lines: Vec<Line>,
}

fn stamp(path: &Path) -> (u64, u64) {
    let m = std::fs::metadata(path).ok();
    let len = m.as_ref().map(|m| m.len()).unwrap_or(0);
    let modified = m.and_then(|m| m.modified().ok()).and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok()).map(|d| d.as_secs()).unwrap_or(0);
    (len, modified)
}

fn cache_path(video: &Path) -> PathBuf {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    video.hash(&mut h);
    crate::kit::fs::data_path("transcripts").join(format!("{:016x}.json", h.finish()))
}

/// The transcript made for this file as it is now, if any.
pub fn load(video: &Path) -> Option<Vec<Line>> {
    let s: Saved = serde_json::from_slice(&std::fs::read(cache_path(video)).ok()?).ok()?;
    ((s.len, s.modified) == stamp(video) && s.path == video).then_some(s.lines)
}

pub fn store(video: &Path, lines: &[Line]) {
    let (len, modified) = stamp(video);
    let p = cache_path(video);
    if let Some(dir) = p.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    crate::kit::fs::save_json(&p, &Saved { path: video.to_path_buf(), len, modified, lines: lines.to_vec() }, false);
}

/// The lines as they fall in the edited video: times moved up past the
/// cuts, lines (almost) all cut away dropped.
pub fn edited_lines(lines: &[Line], edl: &Edl) -> Vec<Line> {
    lines
        .iter()
        .filter_map(|l| {
            let (s, e) = (edl.edited_at(l.start), edl.edited_at(l.end));
            (e - s >= 0.2).then(|| Line { start: s, end: e, text: l.text.clone() })
        })
        .collect()
}

/// Is most of the line cut away?
pub fn line_cut(l: &Line, edl: &Edl) -> bool {
    edl.edited_at(l.end) - edl.edited_at(l.start) < 0.2
}

/// The line being spoken at source `t`.
pub fn line_at(lines: &[Line], t: f64) -> Option<usize> {
    lines.iter().position(|l| t >= l.start && t < l.end)
}

fn srt_time(t: f64) -> String {
    let ms = (t.max(0.0) * 1000.0).round() as u64;
    format!("{:02}:{:02}:{:02},{:03}", ms / 3_600_000, ms / 60_000 % 60, ms / 1000 % 60, ms % 1000)
}

pub fn srt(lines: &[Line]) -> String {
    let mut out = String::new();
    for (k, l) in lines.iter().enumerate() {
        out.push_str(&format!("{}\n{} --> {}\n{}\n\n", k + 1, srt_time(l.start), srt_time(l.end), l.text.trim()));
    }
    out
}

pub fn txt(lines: &[Line]) -> String {
    lines.iter().map(|l| l.text.trim()).filter(|t| !t.is_empty()).collect::<Vec<_>>().join("\n")
}

/// Whisper's raw segments → clean lines (no blanks, no "[music]" asides).
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
pub fn tidy(raw: Vec<(f64, f64, String)>) -> Vec<Line> {
    raw.into_iter()
        .filter_map(|(start, end, text)| {
            let t = text.trim().to_string();
            let aside = (t.starts_with('[') && t.ends_with(']')) || (t.starts_with('(') && t.ends_with(')'));
            (!t.is_empty() && !aside && end > start).then_some(Line { start, end, text: t })
        })
        .collect()
}

/// Transcribe the voice in `video` with model `m`. Blocks; progress is in
/// percent (`total` 100).
#[cfg(target_os = "macos")]
pub fn transcribe(video: &Path, m: Model, p: &std::sync::Arc<Progress>) -> Result<Vec<Line>, String> {
    use whisper_rs::{FullParams, SamplingStrategy, WhisperContext, WhisperContextParameters};
    p.total.store(100, Ordering::Relaxed);
    p.done.store(0, Ordering::Relaxed);
    // the mix: everyone in a call, not only this Mac's microphone
    let pcm = super::sound::read_audio(video, super::sound::Track::Mix, 16_000, 1)?;
    if pcm.is_empty() {
        return Err("This video has no sound".into());
    }
    if p.cancelled() {
        return Err("Stopped".into());
    }
    let ctx = WhisperContext::new_with_params(m.path(), WhisperContextParameters::default()).map_err(|e| format!("The speech model didn't load: {e}"))?;
    let mut state = ctx.create_state().map_err(|e| e.to_string())?;
    let mut params = FullParams::new(SamplingStrategy::Greedy { best_of: 1 });
    params.set_language(Some("auto"));
    params.set_n_threads(std::thread::available_parallelism().map(|n| n.get()).unwrap_or(4).min(8) as i32);
    params.set_print_progress(false);
    params.set_print_realtime(false);
    params.set_print_special(false);
    params.set_print_timestamps(false);
    let pa = p.clone();
    params.set_progress_callback_safe(move |pct: i32| pa.done.store(pct.clamp(0, 100) as u64, Ordering::Relaxed));
    // whisper-rs 0.16's safe abort callback reads its closure wrongly (and
    // aborts every encode), so the raw one, on the cancel flag itself
    unsafe extern "C" fn stop(flag: *mut std::ffi::c_void) -> bool {
        unsafe { (*(flag as *const AtomicBool)).load(Ordering::Relaxed) }
    }
    // `p` outlives `full` below, so the flag stays where it is
    unsafe {
        params.set_abort_callback(Some(stop));
        params.set_abort_callback_user_data(&p.cancel as *const AtomicBool as *mut std::ffi::c_void);
    }
    state.full(params, &pcm).map_err(|e| if p.cancelled() { "Stopped".to_string() } else { format!("Transcribing failed: {e}") })?;
    let raw = (0..state.full_n_segments())
        .filter_map(|i| state.get_segment(i))
        .map(|s| (s.start_timestamp() as f64 / 100.0, s.end_timestamp() as f64 / 100.0, s.to_str_lossy().map(|t| t.to_string()).unwrap_or_default()))
        .collect();
    p.done.store(100, Ordering::Relaxed);
    Ok(tidy(raw))
}

#[cfg(not(target_os = "macos"))]
pub fn transcribe(_video: &Path, _m: Model, _p: &std::sync::Arc<Progress>) -> Result<Vec<Line>, String> {
    Err("Transcribing needs macOS".into())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn line(s: f64, e: f64, t: &str) -> Line {
        Line { start: s, end: e, text: t.into() }
    }

    #[test]
    fn captions_follow_the_cuts() {
        let mut edl = Edl::new(10.0);
        edl.cut(2.0, 5.0);
        let lines = vec![line(0.5, 1.5, "hello"), line(2.5, 4.5, "gone"), line(5.0, 6.0, "after")];
        let e = edited_lines(&lines, &edl);
        assert_eq!(e, vec![line(0.5, 1.5, "hello"), line(2.0, 3.0, "after")]);
        assert!(line_cut(&lines[1], &edl));
        assert!(!line_cut(&lines[2], &edl));
    }

    #[test]
    fn srt_and_text_read_right() {
        let lines = vec![line(1.0, 2.5, " Hi there "), line(61.25, 3725.0, "Bye")];
        assert_eq!(srt(&lines), "1\n00:00:01,000 --> 00:00:02,500\nHi there\n\n2\n00:01:01,250 --> 01:02:05,000\nBye\n\n");
        assert_eq!(txt(&lines), "Hi there\nBye");
    }

    #[test]
    fn asides_and_blanks_are_dropped() {
        let raw = vec![(0.0, 1.0, " [Music] ".to_string()), (1.0, 2.0, " Okay ".into()), (2.0, 2.0, "x".into()), (3.0, 4.0, "  ".into())];
        assert_eq!(tidy(raw), vec![line(1.0, 2.0, "Okay")]);
    }

    #[test]
    fn the_spoken_line_is_found() {
        let lines = vec![line(0.0, 1.0, "a"), line(2.0, 3.0, "b")];
        assert_eq!(line_at(&lines, 2.5), Some(1));
        assert_eq!(line_at(&lines, 1.5), None);
    }

    #[test]
    fn a_transcript_is_only_used_for_the_file_it_was_made_for() {
        let dir = std::env::temp_dir().join(format!("cmb-speech-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let v = dir.join("a.mp4");
        std::fs::write(&v, b"one").unwrap();
        store(&v, &[line(0.0, 1.0, "x")]);
        assert_eq!(load(&v).map(|l| l.len()), Some(1));
        std::fs::write(&v, b"changed!").unwrap();
        assert!(load(&v).is_none());
        let _ = std::fs::remove_file(cache_path(&v));
        let _ = std::fs::remove_dir_all(dir);
    }
}
