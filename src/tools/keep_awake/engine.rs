//! The Keep Awake state machine: the switch, Claude Code activity, the wait
//! after Claude goes quiet, the assertions that follow from them and today's
//! log. Times are unix seconds so tests can drive it.

use super::power::{Power, Want};
use super::sessions::Session;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// A session that wrote to its log this recently is working.
pub const WORKING_SECS: i64 = 120;
/// Choices for the wait after Claude goes quiet, in minutes.
pub const GRACE_CHOICES: [u32; 3] = [1, 5, 15];

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    /// Stay awake while Claude Code works.
    pub claude: bool,
    /// Keep the screen on when turned on by hand.
    pub manual_screen: bool,
    /// Keep the screen on while Claude works.
    pub claude_screen: bool,
    /// Minutes to stay awake after Claude goes quiet.
    pub grace_min: u32,
}

impl Default for Settings {
    fn default() -> Self {
        Settings { claude: true, manual_screen: true, claude_screen: false, grace_min: 5 }
    }
}

impl Settings {
    fn sanitize(mut self) -> Self {
        if !GRACE_CHOICES.contains(&self.grace_min) {
            self.grace_min = 5;
        }
        self
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum State {
    Off,
    /// The switch is on.
    Manual,
    /// Claude Code is working.
    Claude,
    /// Claude went quiet; sleep is allowed again in `left` seconds.
    WindDown { left: i64 },
}

impl State {
    pub fn awake(self) -> bool {
        self != State::Off
    }
}

/// A stretch of awake time today.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Span {
    pub start: i64,
    pub end: i64,
    /// Kept awake for Claude (else by hand).
    pub claude: bool,
}

pub struct Engine {
    pub settings: Settings,
    /// When the switch was turned on.
    pub manual_since: Option<i64>,
    /// Recent sessions, newest first.
    pub sessions: Vec<Session>,
    /// When the Mac last went from sleeping-allowed to awake.
    pub awake_since: Option<i64>,
    /// Closed spans of today (and the open one is `open`).
    pub log: Vec<Span>,
    open: Option<(i64, bool)>,
    /// When Claude was last seen working, while its streak lasts.
    claude_last: Option<i64>,
    /// The tool is turned on in Settings.
    enabled: bool,
    power: Power,
    dir: Option<PathBuf>,
}

impl Engine {
    pub fn new(dir: Option<PathBuf>) -> Self {
        let (settings, log) = match &dir {
            Some(d) => (load_settings(d), load_log(d, today_start(now()))),
            None => (Settings::default(), Vec::new()),
        };
        Engine { settings, manual_since: None, sessions: Vec::new(), awake_since: None, log, open: None, claude_last: None, enabled: true, power: Power::default(), dir }
    }

    pub fn working(&self, now: i64) -> Vec<&Session> {
        self.sessions.iter().filter(|s| now - s.last_write <= WORKING_SECS).collect()
    }

    pub fn state(&self, now: i64) -> State {
        if !self.enabled {
            return State::Off;
        }
        if self.manual_since.is_some() {
            return State::Manual;
        }
        if !self.settings.claude {
            return State::Off;
        }
        if !self.working(now).is_empty() {
            return State::Claude;
        }
        match self.claude_last {
            Some(t) => {
                let left = t + self.settings.grace_min as i64 * 60 - now;
                if left > 0 { State::WindDown { left } } else { State::Off }
            }
            None => State::Off,
        }
    }

    pub fn held(&self) -> Want {
        self.power.held()
    }

    /// Bring everything in line with the state at `now`.
    pub fn step(&mut self, now: i64) {
        if self.enabled && self.settings.claude {
            if let Some(last) = self.working(now).iter().map(|s| s.last_write).max() {
                // Claude counts as working until WORKING_SECS after its last write
                self.claude_last = Some(self.claude_last.unwrap_or(0).max(last + WORKING_SECS));
            }
        }
        let st = self.state(now);
        if st == State::Off {
            self.claude_last = None;
        }
        let display = match st {
            State::Off => false,
            State::Manual => self.settings.manual_screen,
            State::Claude | State::WindDown { .. } => self.settings.claude_screen,
        };
        self.power.apply(Want { system: st.awake(), display });
        match (st.awake(), self.awake_since) {
            (true, None) => self.awake_since = Some(now),
            (false, Some(_)) => self.awake_since = None,
            _ => {}
        }
        let kind = st.awake().then_some(st != State::Manual);
        if self.open.map(|o| o.1) != kind {
            self.close_span(now);
            self.open = kind.map(|k| (now, k));
        }
    }

    pub fn set_manual(&mut self, on: bool, now: i64) {
        self.manual_since = if on { Some(self.manual_since.unwrap_or(now)) } else { None };
        self.step(now);
    }

    pub fn set_enabled(&mut self, on: bool, now: i64) {
        self.enabled = on;
        if !on {
            self.manual_since = None;
        }
        self.step(now);
    }

    pub fn change(&mut self, now: i64, f: impl FnOnce(&mut Settings)) {
        f(&mut self.settings);
        self.settings = self.settings.clone().sanitize();
        if let Some(d) = &self.dir {
            save_json(d, "settings.json", &self.settings);
        }
        self.step(now);
    }

    /// Today's spans, the open one included up to `now`.
    pub fn today(&self, now: i64) -> Vec<Span> {
        let day = today_start(now);
        let mut v: Vec<Span> = self.log.iter().filter(|s| s.end > day).copied().collect();
        if let Some((start, claude)) = self.open {
            v.push(Span { start, end: now, claude });
        }
        for s in &mut v {
            s.start = s.start.max(day);
        }
        v
    }

    fn close_span(&mut self, now: i64) {
        let Some((start, claude)) = self.open.take() else { return };
        if now - start < 30 {
            return;
        }
        let day = today_start(now);
        self.log.retain(|s| s.end > day);
        self.log.push(Span { start, end: now, claude });
        if let Some(d) = &self.dir {
            save_json(d, "today.json", &self.log);
        }
    }
}

impl Drop for Engine {
    fn drop(&mut self) {
        self.close_span(now());
    }
}

pub fn now() -> i64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs() as i64).unwrap_or(0)
}

/// Unix seconds of local midnight before `t`.
pub fn today_start(t: i64) -> i64 {
    use chrono::{Local, TimeZone};
    Local
        .timestamp_opt(t, 0)
        .single()
        .and_then(|d| d.date_naive().and_hms_opt(0, 0, 0))
        .and_then(|m| Local.from_local_datetime(&m).earliest())
        .map(|m| m.timestamp())
        .unwrap_or(t - t.rem_euclid(86_400))
}

pub fn data_dir() -> PathBuf {
    dirs::data_dir().unwrap_or_else(std::env::temp_dir).join("cleanmebro").join("keep_awake")
}

fn load_settings(dir: &Path) -> Settings {
    std::fs::read_to_string(dir.join("settings.json")).ok().and_then(|s| serde_json::from_str::<Settings>(&s).ok()).unwrap_or_default().sanitize()
}

fn load_log(dir: &Path, day: i64) -> Vec<Span> {
    let v: Vec<Span> = std::fs::read_to_string(dir.join("today.json")).ok().and_then(|s| serde_json::from_str(&s).ok()).unwrap_or_default();
    v.into_iter().filter(|s| s.end > day).collect()
}

fn save_json<T: Serialize>(dir: &Path, name: &str, v: &T) {
    let _ = std::fs::create_dir_all(dir);
    if let Ok(json) = serde_json::to_string_pretty(v) {
        let tmp = dir.join(format!("{name}.tmp"));
        if std::fs::write(&tmp, json).is_ok() {
            let _ = std::fs::rename(tmp, dir.join(name));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn session(t: i64) -> Session {
        Session { project: "cleanmebro".into(), last_write: t }
    }

    #[test]
    fn switch_keeps_awake_with_screen() {
        let mut e = Engine::new(None);
        e.step(1000);
        assert_eq!(e.state(1000), State::Off);
        assert_eq!(e.held(), Want::default());
        e.set_manual(true, 1000);
        assert_eq!(e.state(1000), State::Manual);
        assert_eq!(e.held(), Want { system: true, display: true });
        e.set_manual(false, 1100);
        assert_eq!(e.held(), Want::default());
    }

    #[test]
    fn claude_work_then_wind_down_then_sleep() {
        let mut e = Engine::new(None);
        e.sessions = vec![session(1000)];
        e.step(1010);
        assert_eq!(e.state(1010), State::Claude);
        assert_eq!(e.held(), Want { system: true, display: false });
        // quiet: working until 1000 + 120, then 5 min of grace
        e.step(1200);
        assert_eq!(e.state(1200), State::WindDown { left: 1120 + 300 - 1200 });
        assert!(e.held().system);
        e.step(1500);
        assert_eq!(e.state(1500), State::Off);
        assert_eq!(e.held(), Want::default());
        // a new write starts a fresh streak
        e.sessions = vec![session(2000)];
        e.step(2001);
        assert_eq!(e.state(2001), State::Claude);
    }

    #[test]
    fn claude_off_or_tool_off_means_no_trigger() {
        let mut e = Engine::new(None);
        e.sessions = vec![session(1000)];
        e.change(1001, |s| s.claude = false);
        assert_eq!(e.state(1001), State::Off);
        e.change(1002, |s| s.claude = true);
        assert_eq!(e.state(1002), State::Claude);
        e.set_manual(true, 1003);
        e.set_enabled(false, 1004);
        assert_eq!(e.state(1004), State::Off);
        assert_eq!(e.manual_since, None);
        assert_eq!(e.held(), Want::default());
    }

    #[test]
    fn claude_screen_setting_turns_display_on() {
        let mut e = Engine::new(None);
        e.sessions = vec![session(1000)];
        e.change(1001, |s| s.claude_screen = true);
        assert_eq!(e.held(), Want { system: true, display: true });
    }

    #[test]
    fn spans_follow_the_reason() {
        let mut e = Engine::new(None);
        let day = today_start(now());
        let t = day + 3600;
        e.set_manual(true, t);
        e.set_manual(false, t + 600);
        e.sessions = vec![session(t + 1000)];
        e.step(t + 1000);
        let v = e.today(t + 2000);
        assert_eq!(v.len(), 2);
        assert_eq!((v[0].start, v[0].end, v[0].claude), (t, t + 600, false));
        assert_eq!((v[1].start, v[1].claude), (t + 1000, true));
    }

    #[test]
    fn bad_grace_falls_back() {
        let s: Settings = serde_json::from_str("{\"grace_min\": 7}").unwrap();
        assert_eq!(s.sanitize().grace_min, 5);
    }
}
