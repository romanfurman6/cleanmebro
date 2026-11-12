//! The background loop behind the usage tool. It drives every AI provider
//! that's turned on: account and limits on a slow timer (they also feed
//! the menu bar and the notifications, so they refresh while hidden),
//! local tokens while the panel or window is open, the status pages now
//! and then. Each provider's last limits and daily totals are kept, so the
//! history chart has more than this month's logs.

use crate::kit::activity::{self, Every, Wake};
use crate::kit::ai::{self, alerts, history, settings, status, UsageInfo};
use crate::kit::claude_code as cc;
use crate::kit::mechanics as m;
use crate::kit::sync::LockExt;
use std::sync::{Arc, Mutex};
use std::time::Duration;

pub use crate::kit::ai::{human_tokens, short_label, Shared};

/// One `Shared` per provider, in `ai::PROVIDERS` order.
pub type All = Arc<Vec<Shared>>;

const STATUS_SECS: u64 = 15 * 60;

/// The provider shown: the picked one, or the first that's on when "All"
/// is picked.
pub fn current(all: &All) -> &Shared {
    let i = ai::selected();
    if i < all.len() {
        return &all[i];
    }
    let s = settings::get();
    ai::PROVIDERS.iter().position(|m| s.is_on(m.id)).map(|i| &all[i]).unwrap_or(&all[0])
}

/// The providers turned on in Settings, with their data.
pub fn enabled(all: &All) -> Vec<UsageInfo> {
    let s = settings::get();
    ai::PROVIDERS.iter().zip(all.iter()).filter(|(m, _)| s.is_on(m.id)).map(|(_, i)| i.lock_ok().clone()).collect()
}

pub fn start(ctx: egui::Context, demo: bool) -> All {
    let providers: Vec<_> = (0..ai::ALL).map(ai::make).collect();
    if demo {
        return Arc::new(providers.iter().map(|p| Arc::new(Mutex::new(p.demo()))).collect());
    }
    let all: All = Arc::new(providers.iter().map(|p| Arc::new(Mutex::new(UsageInfo { loading: true, ..UsageInfo::new(p.name()) }))).collect());
    let shared = all.clone();
    std::thread::Builder::new()
        .name("ai-usage".into())
        .spawn(move || run(ctx, providers, shared))
        .ok();
    all
}

fn run(ctx: egui::Context, mut providers: Vec<Box<dyn ai::Provider>>, all: All) {
    activity::low_priority();
    let n = providers.len();
    // due right away the first time
    let mut limits_at = vec![Every::default(); n];
    let mut local_at = vec![Every::default(); n];
    let mut size_at = vec![Every::default(); n];
    let mut status_at = Every::default();
    let mut memory: Vec<alerts::Memory> = (0..n).map(|_| alerts::Memory::default()).collect();
    let mut primed = vec![false; n];
    loop {
        let set = settings::get();
        let shown = ai::selected();
        let now = chrono::Utc::now().timestamp();
        for i in 0..n {
            let (p, s) = (&mut providers[i], &all[i]);
            let id = ai::PROVIDERS[i].id;
            // turned off: no CLI calls, no log reading
            if !set.is_on(id) || !p.enabled() {
                limits_at[i].reset();
                s.lock_ok().loading = false;
                continue;
            }
            let retry_empty = ai::PROVIDERS[i].limits && s.lock_ok().limits.is_empty() && limits_at[i].age().is_some_and(|a| a.as_secs() > 60);
            if retry_empty || limits_at[i].due(Duration::from_secs(m::CLAUDE_LIMITS_REFRESH_SECS)) {
                limits_at[i].mark();
                s.lock_ok().loading = true;
                activity::repaint(&ctx);
                p.fetch_account(s);
            } else if id == "claude" {
                // Claude Code's status line writes after every reply: cheap to re-read
                ai::claude::apply_statusline(s);
            }
            // the rest is only shown in the panel and the window: hidden,
            // it waits until they open (the nap below ends then)
            let wanted = shown == i || shown == ai::ALL || local_at[i].age().is_none();
            if wanted && (activity::visible() || local_at[i].age().is_none()) && local_at[i].due(Duration::from_secs(m::CLAUDE_LOCAL_REFRESH_SECS)) {
                local_at[i].mark();
                let projects = p.projects();
                let mut local = p.local_tokens();
                history::with(id, |h| {
                    h.merge_days(&mut local, chrono::Local::now().date_naive());
                    ((), true)
                });
                // the data folder walks every file: once per launch, and again on Refresh
                let data = (size_at[i].age().is_none() && !activity::recording()).then(|| {
                    size_at[i].mark();
                    p.data_dir().map(|d| crate::kit::fs::dir_size(&d)).unwrap_or(0)
                });
                let mut g = s.lock_ok();
                g.projects = projects;
                g.local = local;
                if let Some(d) = data {
                    g.data_bytes = d;
                }
            }
            let info = {
                let mut g = s.lock_ok();
                g.loading = false;
                g.clone()
            };
            if !info.limits.is_empty() {
                history::with(id, |h| ((), h.record(&info.limits, now)));
            }
            // the first check after launch only learns what's already past,
            // so a restart doesn't repeat alerts
            let first = !primed[i];
            let due = alerts::check(&info, &set, &mut memory[i], now);
            if !info.limits.is_empty() || !info.local.days.is_empty() {
                primed[i] = true;
            }
            if !first {
                for a in due {
                    crate::kit::notify::send(&a.title, &a.body);
                }
            }
            activity::repaint(&ctx);
        }
        // outages: a status page per provider that has one
        if set.show_outages && status_at.due(Duration::from_secs(STATUS_SECS)) {
            status_at.mark();
            for (i, meta) in ai::PROVIDERS.iter().enumerate() {
                if set.is_on(meta.id) && !meta.status_url.is_empty() {
                    let incident = status::fetch(meta.status_url);
                    all[i].lock_ok().incident = incident;
                }
            }
            activity::repaint(&ctx);
        } else if !set.show_outages {
            for s in all.iter() {
                s.lock_ok().incident = None;
            }
        }
        // wait, but wake early when someone asks for a refresh or the panel opens
        if activity::nap(Duration::from_secs(m::CLAUDE_LOCAL_REFRESH_SECS), Some(cc::recheck())) == Wake::Kicked {
            for e in limits_at.iter_mut().chain(local_at.iter_mut()).chain(size_at.iter_mut()) {
                e.reset();
            }
            status_at.reset();
        }
    }
}

pub fn request_refresh() {
    cc::request_recheck();
}

/// Show `i` (an index into `ai::PROVIDERS`, or `ai::ALL`) from now on.
pub fn select(i: usize, demo: bool) {
    ai::select(i, !demo);
}
