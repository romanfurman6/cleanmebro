//! The AI usage tool's own settings, kept in `<data>/ai/settings.json`:
//! the main provider, which providers are on, budgets for API keys,
//! notifications and what the menu bar shows.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Mutex, OnceLock};

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(default)]
pub struct Settings {
    /// Id of the provider shown on the page ("claude", "codex", …), or
    /// "all" for every provider at once.
    pub provider: String,
    /// Monthly budget in dollars, by provider name, for API keys.
    pub budgets: HashMap<String, f64>,
    /// Monthly plan price in dollars, by provider name, when the plan name
    /// alone doesn't say it.
    pub plan_usd: HashMap<String, f64>,
    /// Ids of providers turned off.
    pub off: Vec<String>,
    /// Show dollar amounts.
    pub show_cost: bool,
    /// Notify when a limit (or budget) passes these percentages.
    pub notify_at: Vec<u8>,
    /// Notify when a limit resets after being over the first threshold.
    pub notify_reset: bool,
    /// Notify a day before a weekly limit resets with much of it unused.
    pub notify_unused: bool,
    /// Show outages from the providers' status pages.
    pub show_outages: bool,
    /// What the menu bar shows: "closest" (the provider nearest a limit)
    /// or a provider id.
    pub menu_bar: String,
    /// Where Claude's limits come from: "statusline" (Claude Code's own
    /// status line, when the hook is installed) or "cli" (`claude /usage`).
    pub claude_source: String,
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            provider: String::new(),
            budgets: HashMap::new(),
            plan_usd: HashMap::new(),
            off: Vec::new(),
            show_cost: true,
            notify_at: vec![80, 95],
            notify_reset: true,
            notify_unused: true,
            show_outages: true,
            menu_bar: "closest".into(),
            claude_source: "statusline".into(),
        }
    }
}

impl Settings {
    pub fn is_on(&self, id: &str) -> bool {
        !self.off.iter().any(|o| o == id)
    }
}

fn path() -> PathBuf {
    crate::kit::fs::data_path("ai").join("settings.json")
}

fn cell() -> &'static Mutex<Settings> {
    static S: OnceLock<Mutex<Settings>> = OnceLock::new();
    S.get_or_init(|| Mutex::new(crate::kit::fs::load_json(&path())))
}

/// The current settings.
pub fn get() -> Settings {
    cell().lock().unwrap_or_else(|e| e.into_inner()).clone()
}

/// Change the settings and save them (unless `save` is false, for demos).
pub fn update(save: bool, f: impl FnOnce(&mut Settings)) {
    let mut g = cell().lock().unwrap_or_else(|e| e.into_inner());
    f(&mut g);
    if save {
        crate::kit::fs::save_json(&path(), &*g, true);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn old_files_keep_their_provider_and_get_defaults() {
        let s: Settings = serde_json::from_str("{\"provider\":\"codex\"}").unwrap();
        assert_eq!(s.provider, "codex");
        assert!(s.show_cost);
        assert_eq!(s.notify_at, vec![80, 95]);
        assert!(s.is_on("claude"));
        let s: Settings = serde_json::from_str("{\"off\":[\"gemini\"]}").unwrap();
        assert!(!s.is_on("gemini"));
    }
}
