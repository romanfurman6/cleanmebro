//! CleanMeBro — a glassy menu bar companion built from isolated tools (Mac
//! health, Claude limits, Cleanup, Disk Map) behind one panel of widgets.
//!
//! `--screenshot <file.png>` draws ~120 frames with sample data, saves the
//! window as a PNG and exits. `--view <name>` picks what it shows: panel,
//! storage (default), memory, battery, cpu, network, devices, claude-detail,
//! cleanup, cleanup-no-claude, results, lens, lens-live, claude, claude-missing,
//! health, customize, settings-tools, fan, fan-detail, fan-air, fan-air-detail, fan-curve, fan-fixed, fan-install.
//! `--fan-report` prints every fan and temperature sensor this Mac reports.
//! `--fan-install` / `--fan-uninstall` add or remove the fan helper,
//! `--fan-set 2400,3000|auto|hello` talks to it (the lease lasts 8 s).
//! `--fan-helper <uid>` is the helper itself (run by launchd as root).

mod core;
mod kit;
mod tools;

fn main() -> eframe::Result {
    if std::env::args().any(|a| a == "--fan-report") {
        tools::fan::probe::report();
        return Ok(());
    }
    {
        // the fan helper and its test switches
        let a: Vec<String> = std::env::args().collect();
        match a.get(1).map(String::as_str) {
            Some("--fan-helper") => tools::fan::helper::serve(a.get(2).and_then(|u| u.parse().ok()).unwrap_or(0)),
            Some("--fan-install") => {
                println!("{:?}", tools::fan::helper::install());
                return Ok(());
            }
            Some("--fan-uninstall") => {
                println!("{:?}", tools::fan::helper::uninstall());
                return Ok(());
            }
            Some("--fan-set") => {
                use tools::fan::helper::{ask, Request};
                let req = match a.get(2).map(String::as_str) {
                    None | Some("auto") => Request::Auto,
                    Some("hello") => Request::Hello,
                    Some(v) => Request::Set { rpm: v.split(',').map(|x| x.parse().ok()).collect() },
                };
                println!("{:?}", ask(&req));
                return Ok(());
            }
            _ => {}
        }
    }
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
