//! The recording's own clock and output size. Pure, so it's tested on any
//! platform.
// only the macOS recorder uses all of this outside tests
#![cfg_attr(not(target_os = "macos"), allow(dead_code))]

use crate::tools::capture::settings::RecQuality;

/// Maps the capture clock (seconds, as on the sample buffers) to time in
/// the video: from the first frame, with paused stretches cut out.
#[derive(Clone, Debug, Default)]
pub struct Clock {
    t0: Option<f64>,
    /// Paused time so far.
    cut: f64,
    paused_at: Option<f64>,
    /// Nothing before this belongs in the video (buffers that were already
    /// on their way when the pause ended).
    resumed_at: f64,
}

impl Clock {
    /// The first video frame starts the video.
    pub fn start(&mut self, t: f64) {
        if self.t0.is_none() {
            self.t0 = Some(t);
            self.resumed_at = t;
        }
    }

    pub fn pause(&mut self, now: f64) {
        if self.paused_at.is_none() {
            self.paused_at = Some(now);
        }
    }

    pub fn resume(&mut self, now: f64) {
        if let Some(p) = self.paused_at.take() {
            if self.t0.is_some() {
                self.cut += (now - p).max(0.0);
            }
            self.resumed_at = now;
        }
    }

    /// Video time of something captured at `t`; None when it isn't part of
    /// the video (before the start, while paused).
    pub fn map(&self, t: f64) -> Option<f64> {
        let t0 = self.t0?;
        if self.paused_at.is_some_and(|p| t >= p) || t < self.resumed_at {
            return None;
        }
        Some(t - t0 - self.cut)
    }

    /// Length of the video so far, at capture-clock time `now`.
    pub fn length(&self, now: f64) -> f64 {
        let Some(t0) = self.t0 else { return 0.0 };
        let end = self.paused_at.unwrap_or(now);
        (end - t0 - self.cut).max(0.0)
    }
}

/// The video size for a source of `w`×`h` pixels: never bigger than the
/// quality allows (H.264 tops out around 4K), aspect kept, even numbers.
pub fn out_size(w: u32, h: u32, q: RecQuality) -> (u32, u32) {
    let (long, short) = match q {
        RecQuality::High => (3840.0, 2160.0),
        RecQuality::Smaller => (1920.0, 1080.0),
    };
    let (w, h) = (w.max(2) as f64, h.max(2) as f64);
    let (a, b) = if w >= h { (long, short) } else { (short, long) };
    let k = (a / w).min(b / h).min(1.0);
    let even = |v: f64| ((v * k / 2.0).round() as u32 * 2).max(2);
    (even(w), even(h))
}

/// Bits per second for the video: screen content compresses well, so this
/// stays well under camera-style rates.
pub fn bitrate(w: u32, h: u32, fps: u32, q: RecQuality) -> u32 {
    let bpp = match q {
        RecQuality::High => 0.05,
        RecQuality::Smaller => 0.035,
    };
    ((w as f64 * h as f64 * fps.max(1) as f64 * bpp) as u32).clamp(1_000_000, 40_000_000)
}

/// "0:42", "12:05", "1:02:03".
pub fn clock_label(secs: f64) -> String {
    let s = secs.max(0.0) as u64;
    if s >= 3600 {
        format!("{}:{:02}:{:02}", s / 3600, s / 60 % 60, s % 60)
    } else {
        format!("{}:{:02}", s / 60, s % 60)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn video_time_skips_pauses() {
        let mut c = Clock::default();
        assert_eq!(c.map(5.0), None, "nothing before the first frame");
        c.start(10.0);
        assert_eq!(c.map(12.0), Some(2.0));
        c.pause(13.0);
        assert_eq!(c.map(13.5), None);
        assert_eq!(c.length(20.0), 3.0, "the timer stops while paused");
        c.resume(20.0);
        assert_eq!(c.map(19.9), None, "late buffers from the pause are dropped");
        assert_eq!(c.map(21.0), Some(4.0));
        assert_eq!(c.length(22.0), 5.0);
    }

    #[test]
    fn output_size_is_capped_even_and_keeps_aspect() {
        assert_eq!(out_size(5120, 2880, RecQuality::High), (3840, 2160));
        assert_eq!(out_size(5120, 2880, RecQuality::Smaller), (1920, 1080));
        assert_eq!(out_size(1001, 601, RecQuality::High), (1002, 602));
        assert_eq!(out_size(1600, 3000, RecQuality::Smaller), (1024, 1920));
        let (w, h) = out_size(3024, 1964, RecQuality::High);
        assert!(w <= 3840 && h <= 2160 && w % 2 == 0 && h % 2 == 0);
    }

    #[test]
    fn labels() {
        assert_eq!(clock_label(42.9), "0:42");
        assert_eq!(clock_label(725.0), "12:05");
        assert_eq!(clock_label(3723.0), "1:02:03");
    }

    #[test]
    fn bitrates_stay_in_range() {
        assert_eq!(bitrate(3840, 2160, 60, RecQuality::High), 24_883_200);
        assert_eq!(bitrate(320, 200, 30, RecQuality::Smaller), 1_000_000);
    }
}
