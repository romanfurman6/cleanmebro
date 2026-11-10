//! The background loop behind the usage tool. It drives one `Provider`
//! (today Claude Code): account and limits on a slow timer, local tokens
//! and projects while the panel or window is open.

use crate::kit::activity::{self, Every, Wake};
use crate::kit::ai::{Provider, UsageInfo};
use crate::kit::claude_code as cc;
use crate::kit::mechanics as m;
use std::sync::{Arc, Mutex};
use std::time::Duration;

pub use crate::kit::ai::{human_tokens, short_label, Shared};

pub fn start(ctx: egui::Context, demo: bool, mut p: Box<dyn Provider>) -> Shared {
    if demo {
        return Arc::new(Mutex::new(p.demo()));
    }
    let shared: Shared = Arc::new(Mutex::new(UsageInfo { loading: true, ..UsageInfo::new(p.name()) }));
    let s = shared.clone();
    std::thread::Builder::new()
        .name(p.name().to_lowercase())
        .spawn(move || {
            activity::low_priority();
            // due right away the first time
            let mut limits_at = Every::default();
            let mut local_at = Every::default();
            let mut size_at = Every::default();
            loop {
                // turned off in Settings: no CLI calls, no log reading
                if !p.enabled() {
                    activity::nap(Duration::from_secs(3600), p.wake());
                    limits_at.reset();
                    continue;
                }
                // the limits also feed the menu-bar title, so they refresh hidden too
                let retry_empty = s.lock().unwrap().limits.is_empty() && limits_at.age().is_some_and(|a| a.as_secs() > 60);
                if retry_empty || limits_at.due(Duration::from_secs(m::CLAUDE_LIMITS_REFRESH_SECS)) {
                    limits_at.mark();
                    s.lock().unwrap().loading = true;
                    activity::repaint(&ctx);
                    p.fetch_account(&s);
                }
                // the rest is only shown in the panel and the window: hidden,
                // it waits until they open (the nap below ends then)
                if (activity::visible() || local_at.age().is_none()) && local_at.due(Duration::from_secs(m::CLAUDE_LOCAL_REFRESH_SECS)) {
                    let projects = p.projects();
                    let local = p.local_tokens();
                    // the data folder walks every file: once per launch, and again on Refresh
                    let data = (size_at.age().is_none() && !activity::recording()).then(|| {
                        size_at.mark();
                        p.data_dir().map(|d| crate::kit::fs::dir_size(&d)).unwrap_or(0)
                    });
                    let mut g = s.lock().unwrap();
                    g.projects = projects;
                    g.local = local;
                    if let Some(d) = data {
                        g.data_bytes = d;
                    }
                }
                s.lock().unwrap().loading = false;
                activity::repaint(&ctx);
                // wait, but wake early when someone asks for a refresh or the
                // panel opens
                if activity::nap(Duration::from_secs(m::CLAUDE_LOCAL_REFRESH_SECS), p.wake()) == Wake::Kicked {
                    limits_at.reset();
                    local_at.reset();
                    size_at.reset();
                }
            }
        })
        .ok();
    shared
}

pub fn request_refresh() {
    cc::request_recheck();
}
