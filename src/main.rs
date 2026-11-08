//! CleanMeBro — a glassy menu bar companion built from isolated tools (Mac
//! health, Claude limits, Cleanup, Disk Map) behind one panel of widgets.
//!
//! `--screenshot <file.png>` draws ~120 frames with sample data, saves the
//! window as a PNG and exits. `--view <name>` picks what it shows: panel,
//! storage (default), memory, battery, cpu, network, devices, claude-detail,
//! cleanup, results, lens, lens-live, claude, customize, settings-tools.

mod core;
mod kit;
mod tools;

fn main() -> eframe::Result {
    if std::env::args().any(|a| a == "--scan-report") {
        tools::cleanup::scan_report(std::env::args().any(|a| a == "--ai"));
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
    let opts = core::shell::Options { screenshot, view };
    // the shell sizes the window for the chosen view on its first frame
    let viewport = egui::ViewportBuilder::default()
        .with_title("CleanMeBro")
        .with_app_id("com.cleanmebro.app")
        .with_inner_size([kit::mechanics::PANEL_WIDTH, kit::mechanics::PANEL_HEIGHT])
        .with_transparent(true)
        .with_decorations(false)
        .with_resizable(false)
        .with_has_shadow(true);
    let options = eframe::NativeOptions { viewport, ..Default::default() };
    eframe::run_native("CleanMeBro", options, Box::new(move |cc| Ok(Box::new(core::shell::App::new(cc, opts)))))
}
