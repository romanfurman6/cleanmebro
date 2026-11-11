//! `--dictation-probe` runs dictation's work from the command line, without
//! the hotkey or the pill, so a script can check it on a real Mac:
//!
//! `--dictation-probe <file.wav | mic:SECS> [--download] [--whisper]
//! [--lang uk|en|ru] [--paste]` reads a WAV (16-bit or float) or listens to
//! the microphone for SECS seconds, turns it into text with the chosen model
//! (downloaded first with `--download`) and prints it. `--paste` then pastes
//! it into the front app the way dictation does, and puts the clipboard back.

use super::audio::{self, Mic, MODEL_RATE};
use super::mac;
use super::settings::Engine;
use crate::kit::whisper::Progress;
use std::sync::atomic::AtomicBool;
use std::time::{Duration, Instant};

pub fn probe(a: &[String]) -> Result<(), String> {
    let input = a.first().ok_or("say a WAV file or mic:SECS")?;
    let has = |f: &str| a.iter().any(|v| v == f);
    let engine = if has("--whisper") { Engine::Whisper } else { Engine::Parakeet };
    let lang = a.iter().position(|v| v == "--lang").and_then(|k| a.get(k + 1)).map(String::as_str);
    if !engine.ready() {
        if !has("--download") {
            return Err(format!("{} isn't downloaded; add --download", engine.name()));
        }
        let t = Instant::now();
        let p = Progress::default();
        match engine {
            Engine::Parakeet => crate::kit::parakeet::download(&p, &AtomicBool::new(false))?,
            Engine::Whisper => crate::kit::whisper::download(crate::kit::whisper::Model::Accurate, &p)?,
        }
        eprintln!("[dictation] downloaded {} in {:?}", engine.name(), t.elapsed());
    }
    let pcm = match input.strip_prefix("mic:") {
        Some(secs) => {
            let secs: f32 = secs.parse().map_err(|_| "mic:SECS needs a number")?;
            let mic = Mic::start()?;
            eprintln!("[dictation] listening {secs} s");
            std::thread::sleep(Duration::from_secs_f32(secs));
            if let Some(e) = mic.failed() {
                return Err(e);
            }
            mic.stop()
        }
        None => read_wav(&std::fs::read(input).map_err(|e| format!("{input}: {e}"))?)?,
    };
    let secs = pcm.len() as f32 / MODEL_RATE as f32;
    let peak = pcm.iter().fold(0.0f32, |m, v| m.max(v.abs()));
    eprintln!("[dictation] {secs:.2} s of voice, peak {peak:.3}, silent {}", audio::silent(&pcm, MODEL_RATE));
    let t = Instant::now();
    let text = match engine {
        Engine::Parakeet => {
            let m = crate::kit::parakeet::Loaded::load()?;
            eprintln!("[dictation] loaded {} in {:?}", engine.name(), t.elapsed());
            let t = Instant::now();
            let text = audio::tidy_text(&[m.run(&pcm)?]);
            eprintln!("[dictation] ran in {:?}", t.elapsed());
            text
        }
        Engine::Whisper => {
            let m = crate::kit::whisper::Loaded::load(crate::kit::whisper::Model::Accurate)?;
            eprintln!("[dictation] loaded {} in {:?}", engine.name(), t.elapsed());
            let t = Instant::now();
            let segs = m.run(&pcm, lang, None)?;
            eprintln!("[dictation] ran in {:?}", t.elapsed());
            audio::tidy_text(&segs.into_iter().map(|s| s.2).collect::<Vec<_>>())
        }
    };
    println!("{text}");
    if has("--paste") {
        eprintln!("[dictation] accessibility {}", mac::accessibility());
        let app = mac::front_app().map(|t| t.name).unwrap_or_default();
        let (saved, count) = mac::paste(&text, true).ok_or("pasting failed")?;
        std::thread::sleep(Duration::from_millis(600));
        mac::restore(saved, count);
        eprintln!("[dictation] pasted into {app:?}, clipboard put back");
    }
    Ok(())
}

/// The samples of a PCM WAV (16-bit or 32-bit float), at 16 kHz mono.
pub fn read_wav(b: &[u8]) -> Result<Vec<f32>, String> {
    if b.len() < 12 || &b[0..4] != b"RIFF" || &b[8..12] != b"WAVE" {
        return Err("not a WAV file".into());
    }
    let u16_at = |k: usize| u16::from_le_bytes([b[k], b[k + 1]]);
    let u32_at = |k: usize| u32::from_le_bytes([b[k], b[k + 1], b[k + 2], b[k + 3]]);
    let (mut fmt, mut data) = (None, None);
    let mut k = 12;
    while k + 8 <= b.len() {
        let len = u32_at(k + 4) as usize;
        let body = k + 8..(k + 8 + len).min(b.len());
        match &b[k..k + 4] {
            b"fmt " if len >= 16 => fmt = Some((u16_at(k + 8), u16_at(k + 10) as usize, u32_at(k + 12), u16_at(k + 22))),
            b"data" => data = Some(body),
            _ => {}
        }
        k += 8 + len + (len & 1);
    }
    let (format, channels, rate, bits) = fmt.ok_or("WAV without a format")?;
    let data = &b[data.ok_or("WAV without samples")?];
    let samples: Vec<f32> = match (format, bits) {
        (1, 16) => data.chunks_exact(2).map(|c| i16::from_le_bytes([c[0], c[1]]) as f32 / 32768.0).collect(),
        (3, 32) | (0xFFFE, 32) => data.chunks_exact(4).map(|c| f32::from_le_bytes([c[0], c[1], c[2], c[3]])).collect(),
        _ => return Err(format!("WAV format {format} at {bits} bits isn't read; use 16-bit PCM")),
    };
    Ok(audio::resample(&audio::to_mono(&samples, channels.max(1), false), rate as f64, MODEL_RATE))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn wav(rate: u32, channels: u16, samples: &[i16]) -> Vec<u8> {
        let data: Vec<u8> = samples.iter().flat_map(|s| s.to_le_bytes()).collect();
        let mut b = Vec::new();
        b.extend_from_slice(b"RIFF");
        b.extend_from_slice(&(36 + data.len() as u32).to_le_bytes());
        b.extend_from_slice(b"WAVEfmt ");
        b.extend_from_slice(&16u32.to_le_bytes());
        b.extend_from_slice(&1u16.to_le_bytes());
        b.extend_from_slice(&channels.to_le_bytes());
        b.extend_from_slice(&rate.to_le_bytes());
        b.extend_from_slice(&(rate * channels as u32 * 2).to_le_bytes());
        b.extend_from_slice(&(channels * 2).to_le_bytes());
        b.extend_from_slice(&16u16.to_le_bytes());
        b.extend_from_slice(b"data");
        b.extend_from_slice(&(data.len() as u32).to_le_bytes());
        b.extend_from_slice(&data);
        b
    }

    #[test]
    fn reads_wav_at_model_rate() {
        let pcm = read_wav(&wav(16_000, 1, &[16384; 1600])).unwrap();
        assert_eq!(pcm.len(), 1600);
        assert!((pcm[10] - 0.5).abs() < 1e-3);
        // stereo at 32 kHz: half the frames, one channel
        let pcm = read_wav(&wav(32_000, 2, &[8192; 6400])).unwrap();
        assert!((pcm.len() as i64 - 1600).abs() <= 2, "{}", pcm.len());
        assert!(read_wav(b"nope").is_err());
    }
}
