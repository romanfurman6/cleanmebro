//! Capture's own settings: global shortcuts, what happens after a capture,
//! where files go and how much history to keep. Saved as JSON in the app's
//! data folder, separate from the shell's widget settings.

pub use crate::kit::hotkeys::Shortcut;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// Something a global shortcut can start.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Command {
    Area,
    Window,
    Screen,
    Delayed,
    Text,
    Color,
    Ruler,
    History,
}

impl Command {
    pub const ALL: [Command; 8] = [Command::Area, Command::Window, Command::Screen, Command::Delayed, Command::Text, Command::Color, Command::Ruler, Command::History];

    pub fn label(self) -> &'static str {
        match self {
            Command::Area => "Capture area",
            Command::Window => "Capture window",
            Command::Screen => "Capture screen",
            Command::Delayed => "Delayed capture",
            Command::Text => "Copy text from screen",
            Command::Color => "Pick a color",
            Command::Ruler => "Measure (ruler)",
            Command::History => "Open history",
        }
    }

    /// Shortcut on first launch. ⌘⇧3/4/5 belong to macOS, so they're avoided.
    pub fn default_shortcut(self) -> Option<Shortcut> {
        let key = match self {
            Command::Screen => "1",
            Command::Area => "2",
            Command::Window => "6",
            Command::Text => "7",
            Command::Color => "8",
            Command::Ruler => "9",
            Command::Delayed | Command::History => return None,
        };
        Some(Shortcut { cmd: true, shift: true, alt: false, ctrl: false, key: key.into() })
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum After {
    Preview,
    Editor,
    Nothing,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Format {
    Png,
    Jpg,
}

impl Format {
    pub fn ext(self) -> &'static str {
        match self {
            Format::Png => "png",
            Format::Jpg => "jpg",
        }
    }
}

/// How Blur hides what's under it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum BlurStyle {
    Pixelate,
    Blur,
    Solid,
}

/// What Blur and Eraser cover: the whole box, or only the text OCR finds in it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Cover {
    Box,
    Text,
}

/// Delay choices for the panel's switch, in seconds (0 = off).
pub const DELAYS: [u32; 4] = [0, 3, 5, 10];
/// The "Delayed capture" shortcut waits this long when the switch is off.
pub const DEFAULT_DELAY: u32 = 5;
/// History length choices bound the stepper.
pub const KEEP_MIN: usize = 10;
pub const KEEP_MAX: usize = 500;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    /// Command → shortcut; a missing or `None` entry means no shortcut.
    pub shortcuts: Vec<(Command, Option<Shortcut>)>,
    pub after: After,
    pub copy: bool,
    pub save: bool,
    /// Seconds the preview stays up; None keeps it until closed.
    pub preview_secs: Option<u32>,
    pub sound: bool,
    pub folder: PathBuf,
    pub format: Format,
    pub keep: usize,
    pub delay: u32,
    pub blur: BlurStyle,
    pub cover: Cover,
    pub keep_breaks: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            shortcuts: Command::ALL.iter().map(|&c| (c, c.default_shortcut())).collect(),
            after: After::Preview,
            copy: true,
            save: true,
            preview_secs: Some(6),
            sound: false,
            folder: crate::kit::fs::home().join("Desktop"),
            format: Format::Png,
            keep: 50,
            delay: 0,
            blur: BlurStyle::Pixelate,
            cover: Cover::Box,
            keep_breaks: true,
        }
    }
}

impl Settings {
    pub fn shortcut(&self, c: Command) -> Option<&Shortcut> {
        self.shortcuts.iter().find(|(k, _)| *k == c).and_then(|(_, s)| s.as_ref())
    }

    /// Set (or clear) a command's shortcut. Another command holding the same
    /// keys loses them, so one combination never starts two things.
    pub fn set_shortcut(&mut self, c: Command, s: Option<Shortcut>) {
        if let Some(new) = &s {
            for (_, other) in self.shortcuts.iter_mut() {
                if other.as_ref() == Some(new) {
                    *other = None;
                }
            }
        }
        match self.shortcuts.iter_mut().find(|(k, _)| *k == c) {
            Some(e) => e.1 = s,
            None => self.shortcuts.push((c, s)),
        }
    }

    /// Commands that have a shortcut, for registering.
    pub fn bound(&self) -> Vec<(Command, Shortcut)> {
        Command::ALL.iter().filter_map(|&c| self.shortcut(c).map(|s| (c, s.clone()))).collect()
    }

    /// File name for a capture taken at `t`: "Capture 2026-11-08 at 17.41.12.png".
    pub fn file_name(&self, t: chrono::DateTime<chrono::Local>) -> String {
        format!("Capture {}.{}", t.format("%Y-%m-%d at %H.%M.%S"), self.format.ext())
    }

    /// Keep values in range after loading a file someone edited by hand.
    fn sanitize(mut self) -> Self {
        self.keep = self.keep.clamp(KEEP_MIN, KEEP_MAX);
        if !DELAYS.contains(&self.delay) {
            self.delay = 0;
        }
        for c in Command::ALL {
            if !self.shortcuts.iter().any(|(k, _)| *k == c) {
                self.shortcuts.push((c, None));
            }
        }
        self
    }
}

/// `~/Library/Application Support/cleanmebro/capture` (the platform's data dir elsewhere).
pub fn data_dir() -> PathBuf {
    dirs::data_dir().unwrap_or_else(std::env::temp_dir).join("cleanmebro").join("capture")
}

pub fn load(dir: &Path) -> Settings {
    std::fs::read_to_string(dir.join("settings.json")).ok().and_then(|s| serde_json::from_str::<Settings>(&s).ok()).unwrap_or_default().sanitize()
}

pub fn save(dir: &Path, s: &Settings) {
    let _ = std::fs::create_dir_all(dir);
    if let Ok(json) = serde_json::to_string_pretty(s) {
        let tmp = dir.join("settings.json.tmp");
        if std::fs::write(&tmp, json).is_ok() {
            let _ = std::fs::rename(tmp, dir.join("settings.json"));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::kit::hotkeys::key_code;

    fn sc(key: &str) -> Shortcut {
        Shortcut { cmd: true, shift: true, alt: false, ctrl: false, key: key.into() }
    }

    #[test]
    fn defaults_avoid_macos_screenshot_keys() {
        let s = Settings::default();
        for (c, k) in s.bound() {
            assert!(k.conflict().is_none(), "{c:?} default {} conflicts", k.display());
        }
        assert_eq!(s.shortcut(Command::Area).unwrap().display(), "⇧⌘2");
        assert!(s.shortcut(Command::History).is_none());
        assert!(sc("3").conflict().is_some());
    }

    #[test]
    fn a_shortcut_moves_instead_of_duplicating() {
        let mut s = Settings::default();
        s.set_shortcut(Command::History, Some(sc("2")));
        assert_eq!(s.shortcut(Command::History), Some(&sc("2")));
        assert!(s.shortcut(Command::Area).is_none());
        s.set_shortcut(Command::History, None);
        assert!(s.shortcut(Command::History).is_none());
    }

    #[test]
    fn key_codes() {
        assert_eq!(key_code("2").as_deref(), Some("Digit2"));
        assert_eq!(key_code("K").as_deref(), Some("KeyK"));
        assert_eq!(key_code("F12").as_deref(), Some("F12"));
        assert_eq!(key_code("F99"), None);
        assert_eq!(key_code("Escape"), None);
    }

    #[test]
    fn recorder_needs_a_modifier() {
        let none = egui::Modifiers::NONE;
        assert!(Shortcut::from_press(egui::Key::A, none).is_none());
        let cmd_shift = egui::Modifiers { mac_cmd: true, command: true, shift: true, ..Default::default() };
        assert_eq!(Shortcut::from_press(egui::Key::Num2, cmd_shift), Some(sc("2")));
        assert!(Shortcut::from_press(egui::Key::Escape, cmd_shift).is_none());
    }

    #[test]
    fn saves_and_loads() {
        let dir = std::env::temp_dir().join(format!("cmb-capture-settings-{}", std::process::id()));
        let mut s = Settings::default();
        s.keep = 9999;
        s.format = Format::Jpg;
        s.set_shortcut(Command::Ruler, None);
        save(&dir, &s);
        let back = load(&dir);
        assert_eq!(back.format, Format::Jpg);
        assert_eq!(back.keep, KEEP_MAX);
        assert!(back.shortcut(Command::Ruler).is_none());
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn file_names() {
        use chrono::TimeZone;
        let t = chrono::Local.with_ymd_and_hms(2026, 11, 8, 17, 41, 12).unwrap();
        assert_eq!(Settings::default().file_name(t), "Capture 2026-11-08 at 17.41.12.png");
    }
}
