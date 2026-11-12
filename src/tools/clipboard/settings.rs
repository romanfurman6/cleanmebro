//! Clipboard's own settings, saved as JSON in the app's data folder.

use super::store::Limits;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// History length choices bound the stepper.
pub const KEEP_MIN: usize = 20;
pub const KEEP_MAX: usize = 1000;
/// Choices for "Remove items older than" (0 = never).
pub const AGES: [u32; 4] = [1, 7, 30, 0];
/// Biggest image kept, in bytes.
pub const MAX_IMAGE: usize = 10 * 1024 * 1024;

/// An app whose copies are never recorded.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Ignored {
    pub bundle: String,
    pub name: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub keep: usize,
    pub max_days: u32,
    pub keep_images: bool,
    pub dedupe: bool,
    /// Also record passwords (concealed clips), in memory only.
    pub record_concealed: bool,
    /// Also record API tokens, private keys and card numbers.
    pub record_secrets: bool,
    /// Record what arrives from an iPhone or another Mac (Universal Clipboard).
    pub record_remote: bool,
    pub ignore: Vec<Ignored>,
}

/// Password managers skipped out of the box.
pub fn default_ignored() -> Vec<Ignored> {
    [
        ("com.1password.1password", "1Password"),
        ("com.agilebits.onepassword7", "1Password 7"),
        ("com.apple.keychainaccess", "Keychain Access"),
        ("com.apple.Passwords", "Passwords"),
        ("com.bitwarden.desktop", "Bitwarden"),
    ]
    .iter()
    .map(|(b, n)| Ignored { bundle: (*b).into(), name: (*n).into() })
    .collect()
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            keep: 200,
            max_days: 30,
            keep_images: true,
            dedupe: true,
            record_concealed: false,
            record_secrets: false,
            record_remote: true,
            ignore: default_ignored(),
        }
    }
}

impl Settings {
    pub fn limits(&self) -> Limits {
        Limits { keep: self.keep, max_days: self.max_days, dedupe: self.dedupe }
    }

    pub fn ignores(&self, bundle: &str) -> bool {
        !bundle.is_empty() && self.ignore.iter().any(|i| i.bundle == bundle)
    }

    /// Keep values in range after loading a file someone edited by hand.
    fn sanitize(mut self) -> Self {
        self.keep = self.keep.clamp(KEEP_MIN, KEEP_MAX);
        if !AGES.contains(&self.max_days) {
            self.max_days = 30;
        }
        self
    }
}

/// `~/Library/Application Support/cmdmeow/clipboard` (the platform's data dir elsewhere).
pub fn data_dir() -> PathBuf {
    crate::kit::fs::data_path("clipboard")
}

pub fn load(dir: &Path) -> Settings {
    crate::kit::fs::load_json::<Settings>(&dir.join("settings.json")).sanitize()
}

pub fn save(dir: &Path, s: &Settings) {
    crate::kit::fs::save_json(&dir.join("settings.json"), s, true);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults() {
        let s = Settings::default();
        assert!(!s.record_concealed, "passwords stay out unless turned on");
        assert!(!s.record_secrets, "tokens stay out unless turned on");
        assert!(s.record_remote);
        assert!(s.ignores("com.1password.1password"));
        assert!(!s.ignores(""));
    }

    #[test]
    fn saves_and_loads() {
        let dir = std::env::temp_dir().join(format!("cmb-clip-settings-{}", std::process::id()));
        let s = Settings { keep: 5, max_days: 3, dedupe: false, ..Default::default() };
        save(&dir, &s);
        let back = load(&dir);
        assert_eq!(back.keep, KEEP_MIN);
        assert_eq!(back.max_days, 30);
        assert!(!back.dedupe);
        let _ = std::fs::remove_dir_all(dir);
    }
}
