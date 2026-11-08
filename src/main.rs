//! CleanMeBro — a glassy menu bar companion: Mac health at a glance, Claude
//! plan limits, an AI-judged Cleanup and Space Lens.
//!
//! `--screenshot <file.png>` draws ~120 frames with sample data, saves the
//! window as a PNG and exits. `--view <name>` picks what it shows:
//! panel, storage (default), memory, battery, cpu, network, devices,
//! claude-detail, cleanup, results, lens, claude.

mod app;
mod claude;
mod cleanup;
mod glass;
mod mechanics;
mod monitor;
mod native;
mod spacelens;
mod ui_claude;
mod ui_cleanup;
mod ui_lens;
mod ui_panel;

fn main() -> eframe::Result {
    if std::env::args().any(|a| a == "--scan-report") {
        scan_report(std::env::args().any(|a| a == "--ai"));
        return Ok(());
    }
    let mut screenshot = None;
    let mut view = "storage".to_string();
    let mut args = std::env::args().skip(1);
    while let Some(a) = args.next() {
        match a.as_str() {
            "--screenshot" => screenshot = args.next().map(std::path::PathBuf::from),
            "--view" => view = args.next().unwrap_or(view),
            _ => {}
        }
    }
    if screenshot.is_none() && !std::env::args().any(|a| a == "--view") {
        view = "panel".into();
    }
    let opts = app::Options { screenshot: screenshot.clone(), view: view.clone() };
    let window_mode = matches!(view.as_str(), "cleanup" | "results" | "lens" | "claude");
    let size = if window_mode {
        [mechanics::APP_WIDTH, mechanics::APP_HEIGHT]
    } else if view == "panel" {
        [mechanics::PANEL_WIDTH, mechanics::PANEL_HEIGHT]
    } else {
        [mechanics::PANEL_WIDTH + mechanics::DETAIL_WIDTH + 12.0, mechanics::PANEL_HEIGHT]
    };
    let viewport = egui::ViewportBuilder::default()
        .with_title("CleanMeBro")
        .with_app_id("com.cleanmebro.app")
        .with_inner_size(size)
        .with_transparent(true)
        .with_decorations(false)
        .with_resizable(false)
        .with_has_shadow(true);
    let options = eframe::NativeOptions { viewport, ..Default::default() };
    eframe::run_native("CleanMeBro", options, Box::new(move |cc| Ok(Box::new(app::App::new(cc, opts)))))
}

/// Read-only check of the Cleanup rules on this Mac: scans, optionally asks
/// Claude, prints what it would offer. Nothing is removed.
fn scan_report(ai: bool) {
    let state: cleanup::Shared = Default::default();
    cleanup::start_scan(state.clone(), egui::Context::default(), ai);
    loop {
        std::thread::sleep(std::time::Duration::from_millis(300));
        if state.lock().unwrap().phase == cleanup::Phase::Results {
            break;
        }
    }
    let g = state.lock().unwrap();
    let home = dirs::home_dir().unwrap_or_default().display().to_string();
    for it in &g.items {
        println!(
            "{:<12} {:>10} {:<14} {:<5} {} | {}",
            format!("{:?}", it.group),
            glass::human_bytes(it.bytes),
            format!("{:?}", it.verdict),
            format!("{:?}", it.by),
            it.path.display().to_string().replacen(&home, "~", 1),
            it.reason
        );
    }
    println!("total offered {} · ai used {} · ai error {:?}", glass::human_bytes(g.total()), g.ai_used, g.ai_error);
}
