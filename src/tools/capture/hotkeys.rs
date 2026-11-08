//! System-wide shortcuts. They fire even when another app is in front; each
//! press wakes the app (`request_repaint`), and the tool picks the commands up
//! in its `tick`.

use super::settings::{key_code, Command, Shortcut};
use global_hotkey::hotkey::{Code, HotKey, Modifiers};
use global_hotkey::{GlobalHotKeyEvent, GlobalHotKeyManager, HotKeyState};
use std::str::FromStr;
use std::sync::mpsc::{channel, Receiver};

pub struct Keys {
    mgr: GlobalHotKeyManager,
    rx: Receiver<GlobalHotKeyEvent>,
    bound: Vec<(HotKey, Command)>,
}

/// The library's hotkey for one of our shortcuts.
pub fn to_hotkey(s: &Shortcut) -> Option<HotKey> {
    let code = Code::from_str(&key_code(&s.key)?).ok()?;
    let mut m = Modifiers::empty();
    if s.cmd {
        m |= Modifiers::SUPER;
    }
    if s.shift {
        m |= Modifiers::SHIFT;
    }
    if s.alt {
        m |= Modifiers::ALT;
    }
    if s.ctrl {
        m |= Modifiers::CONTROL;
    }
    Some(HotKey::new(Some(m), code))
}

impl Keys {
    pub fn new(ctx: egui::Context) -> Option<Keys> {
        let mgr = GlobalHotKeyManager::new().ok()?;
        let (tx, rx) = channel();
        GlobalHotKeyEvent::set_event_handler(Some(move |e: GlobalHotKeyEvent| {
            let _ = tx.send(e);
            ctx.request_repaint();
        }));
        Some(Keys { mgr, rx, bound: Vec::new() })
    }

    /// Register exactly these shortcuts. Returns the ones the system refused
    /// (usually: another app holds them).
    pub fn apply(&mut self, wanted: &[(Command, Shortcut)]) -> Vec<(Command, String)> {
        for (hk, _) in self.bound.drain(..) {
            let _ = self.mgr.unregister(hk);
        }
        let mut failed = Vec::new();
        for (cmd, s) in wanted {
            let Some(hk) = to_hotkey(s) else {
                failed.push((*cmd, format!("{} can't be used as a shortcut", s.display())));
                continue;
            };
            match self.mgr.register(hk) {
                Ok(()) => self.bound.push((hk, *cmd)),
                Err(e) => failed.push((*cmd, format!("{} is taken: {e}", s.display()))),
            }
        }
        failed
    }

    /// Stop listening (while the user records a new shortcut).
    pub fn clear(&mut self) {
        for (hk, _) in self.bound.drain(..) {
            let _ = self.mgr.unregister(hk);
        }
    }

    /// Commands pressed since the last call.
    pub fn poll(&self) -> Vec<Command> {
        let mut out = Vec::new();
        while let Ok(e) = self.rx.try_recv() {
            if e.state == HotKeyState::Pressed {
                if let Some((_, c)) = self.bound.iter().find(|(hk, _)| hk.id() == e.id) {
                    out.push(*c);
                }
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shortcuts_map_to_distinct_hotkeys() {
        let s = super::super::settings::Settings::default();
        let mut ids: Vec<u32> = s.bound().iter().map(|(_, k)| to_hotkey(k).expect("default maps").id()).collect();
        let n = ids.len();
        ids.sort();
        ids.dedup();
        assert_eq!(ids.len(), n);
        let k = Shortcut { cmd: true, shift: true, alt: false, ctrl: false, key: "2".into() };
        assert_eq!(to_hotkey(&k), Some(HotKey::new(Some(Modifiers::SUPER | Modifiers::SHIFT), Code::Digit2)));
    }
}
