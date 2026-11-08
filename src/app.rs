//! The app shell: a menu-bar icon that drops down the glass health panel (with
//! a detail sheet to its left), and a full window for Cleanup, Space Lens and
//! Claude. One native window switches between the two shapes.

use crate::glass::{self, *};
use crate::mechanics as m;
use crate::{claude, cleanup, monitor, spacelens};
use egui::{Align2, Color32, Pos2, Rect, Sense, Vec2, ViewportCommand};
use std::path::PathBuf;
use std::sync::mpsc::{Receiver, Sender};
use std::sync::Arc;
use std::time::Instant;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Mode {
    Panel,
    Window,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Detail {
    Claude,
    Storage,
    Memory,
    Battery,
    Cpu,
    Network,
    Devices,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Page {
    Cleanup,
    SpaceLens,
    Claude,
}

pub struct Options {
    pub screenshot: Option<PathBuf>,
    pub view: String,
}

pub struct App {
    pub mode: Mode,
    pub detail: Option<Detail>,
    pub page: Page,
    pub mon: monitor::Shared,
    pub claude: claude::Shared,
    pub clean: cleanup::Shared,
    pub lens: spacelens::Shared,
    pub demo: bool,
    // cleanup UI state
    pub use_ai: bool,
    pub delete_now: bool,
    pub clean_group: Option<cleanup::Group>, // None = "Needs your OK"
    pub clean_confirm: bool,
    pub clean_log: bool,
    // space lens UI state
    pub lens_root: PathBuf,
    pub lens_path: PathBuf,
    pub lens_back: Vec<PathBuf>,
    pub lens_fwd: Vec<PathBuf>,
    pub lens_selected: Vec<(PathBuf, u64)>,
    pub lens_review: bool,
    pub lens_msg: Option<String>,
    // window/tray state
    tray: Option<tray_icon::TrayIcon>,
    tray_rx: Receiver<tray_icon::TrayIconEvent>,
    tray_title: String,
    anchor: Option<Pos2>,
    visible: bool,
    had_focus: bool,
    last_hide: Instant,
    pub gear_open: bool,
    pub app_open: bool,
    app_focus: bool,
    pending_shape: bool,
    screenshot: Option<PathBuf>,
    frame: u64,
    pub toast: Option<(String, Instant)>,
}

impl App {
    pub fn new(cc: &eframe::CreationContext<'_>, opts: Options) -> Self {
        install_fonts(&cc.egui_ctx);
        let ctx = cc.egui_ctx.clone();
        let demo = opts.screenshot.is_some();
        let mon = if demo {
            Arc::new(std::sync::Mutex::new(monitor::demo_snapshot()))
        } else {
            monitor::start(ctx.clone(), demo)
        };
        let claude = claude::start(ctx.clone(), demo);
        let clean: cleanup::Shared = Default::default();
        let lens: spacelens::Shared = Default::default();

        // the menu bar icon
        let (tx, tray_rx): (Sender<tray_icon::TrayIconEvent>, _) = std::sync::mpsc::channel();
        let c2 = ctx.clone();
        tray_icon::TrayIconEvent::set_event_handler(Some(move |e| {
            let _ = tx.send(e);
            c2.request_repaint();
        }));
        let tray = if demo {
            None
        } else {
            tray_icon::TrayIconBuilder::new()
                .with_icon(tray_image())
                .with_icon_as_template(true)
                .with_tooltip("CleanMeBro")
                .with_menu_on_left_click(false)
                .build()
                .ok()
        };

        let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("/"));
        let mut app = App {
            mode: Mode::Panel,
            detail: None,
            page: Page::Cleanup,
            mon,
            claude,
            clean,
            lens,
            demo,
            use_ai: true,
            delete_now: false,
            clean_group: Some(cleanup::Group::Claude),
            clean_confirm: false,
            clean_log: false,
            lens_root: home.clone(),
            lens_path: home,
            lens_back: vec![],
            lens_fwd: vec![],
            lens_selected: vec![],
            lens_review: false,
            lens_msg: None,
            tray,
            tray_rx,
            tray_title: String::new(),
            anchor: None,
            visible: true,
            had_focus: false,
            last_hide: Instant::now(),
            gear_open: false,
            app_open: false,
            app_focus: false,
            pending_shape: true,
            screenshot: opts.screenshot,
            frame: 0,
            toast: None,
        };
        match opts.view.as_str() {
            "panel" => {}
            "cleanup" => app.mode = Mode::Window,
            "results" => {
                app.mode = Mode::Window;
                *app.clean.lock().unwrap() = crate::ui_cleanup::demo_results();
            }
            "lens" => {
                app.mode = Mode::Window;
                app.page = Page::SpaceLens;
                crate::ui_lens::demo_tree(&mut app);
            }
            "claude" => {
                app.mode = Mode::Window;
                app.page = Page::Claude;
            }
            v => {
                app.detail = Some(match v {
                    "memory" => Detail::Memory,
                    "battery" => Detail::Battery,
                    "cpu" => Detail::Cpu,
                    "network" => Detail::Network,
                    "devices" => Detail::Devices,
                    "claude-detail" => Detail::Claude,
                    _ => Detail::Storage,
                })
            }
        }
        crate::native::set_dock_icon(app.mode == Mode::Window || demo);
        app
    }

    pub fn window_size(&self) -> Vec2 {
        match self.mode {
            Mode::Panel => {
                let w = m::PANEL_WIDTH + if self.detail.is_some() { m::DETAIL_WIDTH + 12.0 } else { 0.0 };
                Vec2::new(w, m::PANEL_HEIGHT)
            }
            Mode::Window => Vec2::new(m::APP_WIDTH, m::APP_HEIGHT),
        }
    }

    /// Resize/move the window for the current mode.
    pub fn reshape(&mut self, ctx: &egui::Context) {
        let size = self.window_size();
        ctx.send_viewport_cmd_to(egui::ViewportId::ROOT, ViewportCommand::InnerSize(size));
        if self.screenshot.is_some() {
            return;
        }
        let screen = ctx.input(|i| i.viewport().monitor_size).unwrap_or(Vec2::new(1512.0, 982.0));
        match self.mode {
            Mode::Panel => {
                let anchor = self.anchor.unwrap_or(Pos2::new(screen.x - m::PANEL_WIDTH / 2.0 - 16.0, 30.0));
                let right = (anchor.x + m::PANEL_WIDTH / 2.0).clamp(m::PANEL_WIDTH, screen.x - 8.0);
                let x = (right - size.x).max(8.0);
                ctx.send_viewport_cmd_to(egui::ViewportId::ROOT, ViewportCommand::OuterPosition(Pos2::new(x, anchor.y + 6.0)));
                ctx.send_viewport_cmd_to(egui::ViewportId::ROOT, ViewportCommand::WindowLevel(egui::WindowLevel::AlwaysOnTop));
                crate::native::set_dock_icon(false);
            }
            Mode::Window => {
                let pos = Pos2::new(((screen.x - size.x) / 2.0).max(0.0), ((screen.y - size.y) / 2.0).max(30.0));
                ctx.send_viewport_cmd_to(egui::ViewportId::ROOT, ViewportCommand::OuterPosition(pos));
                ctx.send_viewport_cmd_to(egui::ViewportId::ROOT, ViewportCommand::WindowLevel(egui::WindowLevel::Normal));
                crate::native::set_dock_icon(true);
            }
        }
    }

    pub fn show(&mut self, ctx: &egui::Context) {
        self.visible = true;
        self.had_focus = false;
        ctx.send_viewport_cmd_to(egui::ViewportId::ROOT, ViewportCommand::Visible(true));
        ctx.send_viewport_cmd_to(egui::ViewportId::ROOT, ViewportCommand::Focus);
        crate::native::activate();
    }

    pub fn hide(&mut self, ctx: &egui::Context) {
        self.visible = false;
        self.gear_open = false;
        self.last_hide = Instant::now();
        ctx.send_viewport_cmd_to(egui::ViewportId::ROOT, ViewportCommand::Visible(false));
    }

    /// Open the full app in its own normal window; the menu panel steps aside.
    pub fn open_window(&mut self, ctx: &egui::Context, page: Page) {
        self.page = page;
        self.gear_open = false;
        if self.screenshot.is_some() {
            return;
        }
        self.app_open = true;
        self.app_focus = true;
        crate::native::set_dock_icon(true);
        if self.visible {
            self.hide(ctx);
        }
    }

    pub fn open_panel(&mut self, ctx: &egui::Context) {
        self.mode = Mode::Panel;
        self.reshape(ctx);
        self.show(ctx);
    }

    pub fn set_detail(&mut self, ctx: &egui::Context, d: Option<Detail>) {
        self.detail = if self.detail == d { None } else { d };
        self.reshape(ctx);
    }

    fn handle_tray(&mut self, ctx: &egui::Context) {
        while let Ok(ev) = self.tray_rx.try_recv() {
            if let tray_icon::TrayIconEvent::Click { rect, button: tray_icon::MouseButton::Left, button_state: tray_icon::MouseButtonState::Up, .. } = ev {
                let ppp = ctx.input(|i| i.viewport().native_pixels_per_point).unwrap_or(2.0);
                let cx = (rect.position.x + rect.size.width as f64 / 2.0) as f32 / ppp;
                let bottom = (rect.position.y + rect.size.height as f64) as f32 / ppp;
                self.anchor = Some(Pos2::new(cx, bottom));
                let just_hid = self.last_hide.elapsed().as_millis() < 350;
                if self.visible && self.mode == Mode::Panel {
                    self.hide(ctx);
                } else if !just_hid || self.mode == Mode::Window {
                    self.open_panel(ctx);
                }
            }
        }
    }

    fn update_tray_title(&mut self) {
        let Some(tray) = &self.tray else { return };
        let title = self.claude.lock().unwrap().session().map(|l| format!("{:.0}%", l.pct)).unwrap_or_default();
        if title != self.tray_title {
            tray.set_title(if title.is_empty() { None } else { Some(&title) });
            self.tray_title = title;
        }
    }
}

impl eframe::App for App {
    fn logic(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        if self.app_open && !self.visible {
            ctx.send_viewport_cmd(ViewportCommand::RequestPaintWhileHidden);
        }
        self.handle_tray(ctx);
        self.update_tray_title();
        if self.pending_shape {
            self.pending_shape = false;
            self.reshape(ctx);
        }
        // closing the window keeps the app in the menu bar
        if ctx.input(|i| i.viewport().close_requested()) && self.screenshot.is_none() && !QUIT.load(std::sync::atomic::Ordering::SeqCst) {
            ctx.send_viewport_cmd(ViewportCommand::CancelClose);
            self.hide(ctx);
        }
        // the panel behaves like a popover: it hides when you click elsewhere
        let focused = ctx.input(|i| i.viewport().focused);
        if focused == Some(true) {
            self.had_focus = true;
        }
        if self.mode == Mode::Panel && self.visible && self.had_focus && focused == Some(false) && self.screenshot.is_none() {
            self.hide(ctx);
        }
    }

    fn clear_color(&self, _v: &egui::Visuals) -> [f32; 4] {
        [0.0, 0.0, 0.0, 0.0]
    }

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        self.frame += 1;
        if self.frame <= 3 || self.frame % 30 == 0 {
            let r = if self.mode == Mode::Panel { m::CARD_RADIUS as f64 + 6.0 } else { 16.0 };
            if self.screenshot.is_none() {
                crate::native::apply_glass(r);
            }
        }
        if std::env::var("CLEANMEBRO_DEBUG").is_ok() {
            match self.frame {
                30 => eprintln!("== start screen {:?}\n{}", ui.max_rect(), crate::native::debug_windows()),
                60 => self.set_detail(&ctx, Some(Detail::Storage)),
                120 => eprintln!("== detail screen {:?}\n{}", ui.max_rect(), crate::native::debug_windows()),
                150 => self.open_window(&ctx, Page::Cleanup),
                260 => eprintln!("== app frame alive {}", self.frame),
                220 => eprintln!("== app screen {:?}\n{}", ui.max_rect(), crate::native::debug_windows()),
                _ => {}
            }
            ctx.request_repaint();
        }
        let full = ui.max_rect();
        let opaque = self.screenshot.is_some();
        let t = ctx.input(|i| i.time);
        match self.mode {
            Mode::Panel => {
                // one glass sheet; the detail opens on its left behind a hairline
                glass::backdrop(ui.painter(), full, m::CARD_RADIUS + 6.0, opaque, t);
                let panel = Rect::from_min_size(Pos2::new(full.right() - m::PANEL_WIDTH, full.top()), Vec2::new(m::PANEL_WIDTH, full.height()));
                if let Some(d) = self.detail {
                    let sheet = Rect::from_min_size(full.left_top(), Vec2::new(m::DETAIL_WIDTH, full.height()));
                    let x = panel.left() - 6.0;
                    ui.painter().line_segment([Pos2::new(x, full.top() + 18.0), Pos2::new(x, full.bottom() - 18.0)], egui::Stroke::new(1.0, white(30)));
                    crate::ui_panel::detail(self, ui, sheet, d);
                }
                crate::ui_panel::panel(self, ui, panel);
            }
            Mode::Window => {
                glass::backdrop(ui.painter(), full, 14.0, opaque, t);
                self.window_ui(ui, full);
            }
        }
        if let Some((msg, at)) = &self.toast {
            if at.elapsed().as_secs_f32() < 3.0 {
                let p = ui.painter();
                let g = p.layout_no_wrap(msg.clone(), font(13.0), TEXT);
                let r = Rect::from_center_size(Pos2::new(full.center().x, full.bottom() - 40.0), g.size() + Vec2::new(28.0, 16.0));
                glass_card(p, r, r.height() / 2.0, 1.0, false);
                p.galley(r.center() - g.size() / 2.0, g, TEXT);
                ctx.request_repaint();
            } else {
                self.toast = None;
            }
        }
        if self.app_open {
            self.app_viewport(&ctx);
        }
        // keep animations and live numbers moving
        ctx.request_repaint_after(std::time::Duration::from_millis(if self.visible { 500 } else { 2000 }));
        self.screenshot_step(&ctx);
    }
}

pub static QUIT: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

impl App {
    pub fn quit(&mut self, ctx: &egui::Context) {
        QUIT.store(true, std::sync::atomic::Ordering::SeqCst);
        ctx.send_viewport_cmd(ViewportCommand::Close);
    }

    /// The full app in its own native window (traffic lights, resizable).
    fn app_viewport(&mut self, ctx: &egui::Context) {
        let mut builder = egui::ViewportBuilder::default()
            .with_title("CleanMeBro")
            .with_inner_size([m::APP_WIDTH, m::APP_HEIGHT])
            .with_min_inner_size([980.0, 640.0])
            .with_transparent(true)
            .with_fullsize_content_view(true)
            .with_titlebar_shown(false)
            .with_title_shown(false)
            .with_has_shadow(true);
        if self.app_focus {
            builder = builder.with_active(true);
        }
        let id = egui::ViewportId::from_hash_of("cleanmebro-app");
        ctx.show_viewport_immediate(id, builder, |ui, _class| {
            let vctx = ui.ctx().clone();
            if self.app_focus {
                self.app_focus = false;
                vctx.send_viewport_cmd(ViewportCommand::Focus);
                crate::native::activate();
            }
            if vctx.input(|i| i.viewport().close_requested()) {
                self.app_open = false;
                crate::native::set_dock_icon(false);
                return;
            }
            crate::native::apply_glass(16.0);
            let full = ui.max_rect();
            let t = vctx.input(|i| i.time);
            glass::backdrop(ui.painter(), full, 16.0, false, t);
            self.window_ui(ui, full);
            vctx.request_repaint_after(std::time::Duration::from_millis(500));
        });
    }

    fn screenshot_step(&mut self, ctx: &egui::Context) {
        let Some(path) = self.screenshot.clone() else { return };
        ctx.request_repaint();
        if self.frame == 120 {
            ctx.send_viewport_cmd(ViewportCommand::Screenshot(egui::ScreenshotCallback::new(move |img| {
                save_png(&img, &path);
                std::process::exit(0);
            })));
        }
        if self.frame > 400 {
            eprintln!("screenshot was not delivered");
            std::process::exit(1);
        }
    }

    /// The full window: traffic lights, a sidebar of modules and the page.
    fn window_ui(&mut self, ui: &mut egui::Ui, full: Rect) {
        let ctx = ui.ctx().clone();
        let side = Rect::from_min_size(full.min, Vec2::new(88.0, full.height()));
        // drag the window from the top strip
        let drag = Rect::from_min_size(full.min + Vec2::new(88.0, 0.0), Vec2::new(full.width() - 88.0, 44.0));
        let dr = ui.interact(drag, ui.id().with("drag"), Sense::click_and_drag());
        if dr.drag_started() {
            ctx.send_viewport_cmd(ViewportCommand::StartDrag);
        }
        // sidebar glass strip
        let p = ui.painter().clone();
        gradient_rect_dir(&p, side.shrink2(Vec2::new(0.0, 0.0)), 14.0, white(10), white(4), Vec2::new(1.0, 0.0));
        p.line_segment([side.right_top() + Vec2::new(0.0, 12.0), side.right_bottom() - Vec2::new(0.0, 12.0)], egui::Stroke::new(1.0, white(22)));
        // traffic lights
        let lights = [(Color32::from_rgb(255, 95, 87), 0), (Color32::from_rgb(254, 188, 46), 1), (Color32::from_rgb(40, 200, 64), 2)];
        let any_hover = ui.rect_contains_pointer(Rect::from_min_size(full.min + Vec2::new(10.0, 8.0), Vec2::new(70.0, 24.0)));
        let draw_lights = self.screenshot.is_some();
        for (c, k) in lights.into_iter().filter(|_| draw_lights) {
            let center = full.min + Vec2::new(22.0 + k as f32 * 20.0, 20.0);
            let r = Rect::from_center_size(center, Vec2::splat(13.0));
            let resp = ui.interact(r, ui.id().with(("light", k)), Sense::click());
            p.circle_filled(center, 6.2, c);
            p.circle_stroke(center, 6.2, egui::Stroke::new(0.6, Color32::from_black_alpha(60)));
            if any_hover {
                let s = egui::Stroke::new(1.2, Color32::from_black_alpha(150));
                match k {
                    0 => {
                        p.line_segment([center + Vec2::new(-2.5, -2.5), center + Vec2::new(2.5, 2.5)], s);
                        p.line_segment([center + Vec2::new(-2.5, 2.5), center + Vec2::new(2.5, -2.5)], s);
                    }
                    1 => {
                        p.line_segment([center + Vec2::new(-3.0, 0.0), center + Vec2::new(3.0, 0.0)], s);
                    }
                    _ => {
                        p.line_segment([center + Vec2::new(-2.5, 2.5), center + Vec2::new(2.5, -2.5)], s);
                    }
                }
            }
            if resp.clicked() {
                match k {
                    0 => ctx.send_viewport_cmd(ViewportCommand::Close),
                    1 => ctx.send_viewport_cmd(ViewportCommand::Minimized(true)),
                    _ => {}
                }
            }
        }
        // module buttons
        let items = [
            (Page::Cleanup, Icon::Broom, "Cleanup", Color32::from_rgb(110, 225, 140), Color32::from_rgb(30, 165, 80)),
            (Page::SpaceLens, Icon::Lens, "Space Lens", Color32::from_rgb(100, 170, 255), Color32::from_rgb(10, 110, 235)),
            (Page::Claude, Icon::Sparkle, "Claude", Color32::from_rgb(255, 180, 140), Color32::from_rgb(214, 100, 70)),
        ];
        let mut y = full.top() + 70.0;
        for (page, ic, label, top, bottom) in items {
            let r = Rect::from_center_size(Pos2::new(side.center().x, y + 32.0), Vec2::new(70.0, 70.0));
            let id = ui.id().with(("nav", label));
            let resp = ui.interact(r, id, Sense::click());
            let hov = ctx.animate_bool(id, resp.hovered());
            if self.page == page {
                glass_card(&p, r, 18.0, 0.6, false);
            } else if hov > 0.0 {
                p.rect_filled(r, 18.0, white((14.0 * hov) as u8));
            }
            let ir = Rect::from_center_size(r.center() - Vec2::new(0.0, 8.0), Vec2::splat(34.0));
            glass::tile_icon(&p, ir, ic, top, bottom);
            p.text(Pos2::new(r.center().x, r.bottom() - 9.0), Align2::CENTER_CENTER, label, font(10.5), if self.page == page { TEXT } else { TEXT_DIM });
            if resp.clicked() {
                self.page = page;
            }
            y += 84.0;
        }
        // back to the menu panel
        let r = Rect::from_center_size(Pos2::new(side.center().x, full.bottom() - 46.0), Vec2::splat(52.0));
        let resp = ui.interact(r, ui.id().with("to-panel"), Sense::click());
        let hov = ctx.animate_bool(ui.id().with("to-panel-h"), resp.hovered());
        p.rect_filled(r, 14.0, white((10.0 + 14.0 * hov) as u8));
        icon(&p, Icon::Monitor, r.shrink(14.0), TEXT_DIM);
        if resp.on_hover_text("Show the menu bar panel").clicked() {
            self.open_panel(&ctx.clone());
        }
        // page title
        let title = match self.page {
            Page::Cleanup => "Cleanup",
            Page::SpaceLens => "Space Lens",
            Page::Claude => "Claude",
        };
        p.text(Pos2::new(full.center().x + 44.0, full.top() + 20.0), Align2::CENTER_CENTER, title, bold(14.0), TEXT_DIM);
        let content = Rect::from_min_max(Pos2::new(side.right() + 20.0, full.top() + 44.0), full.max - Vec2::new(20.0, 18.0));
        match self.page {
            Page::Cleanup => crate::ui_cleanup::page(self, ui, content),
            Page::SpaceLens => crate::ui_lens::page(self, ui, content),
            Page::Claude => crate::ui_claude::page(self, ui, content),
        }
    }
}

// ---------------------------------------------------------------- helpers

fn save_png(img: &egui::ColorImage, path: &std::path::Path) {
    let [w, h] = img.size;
    let bg = [18u8, 10, 40];
    let mut buf = image::RgbaImage::new(w as u32, h as u32);
    for (i, px) in img.pixels.iter().enumerate() {
        let [r, g, b, a] = px.to_srgba_unmultiplied();
        let af = a as f32 / 255.0;
        let mix = |c: u8, k: u8| (c as f32 * af + k as f32 * (1.0 - af)).round() as u8;
        buf.put_pixel((i % w) as u32, (i / w) as u32, image::Rgba([mix(r, bg[0]), mix(g, bg[1]), mix(b, bg[2]), 255]));
    }
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    if let Err(e) = buf.save(path) {
        eprintln!("couldn't save screenshot: {e}");
    }
}

/// A 36x36 template image for the menu bar: a sparkle inside a rounded square.
fn tray_image() -> tray_icon::Icon {
    let n = 36usize;
    let mut px = vec![0u8; n * n * 4];
    let c = n as f32 / 2.0;
    for y in 0..n {
        for x in 0..n {
            let fx = x as f32 + 0.5 - c;
            let fy = y as f32 + 0.5 - c;
            // rounded-square outline
            let q = (fx.abs().max(fy.abs()) - 13.0).abs();
            let mut a: f32 = if q < 1.4 { 1.0 - (q - 0.4).max(0.0) } else { 0.0 };
            // four-point star in the middle
            let sx = fx.abs() / 8.5;
            let sy = fy.abs() / 8.5;
            let star = sx.sqrt() + sy.sqrt();
            if star < 1.0 {
                a = 1.0;
            } else if star < 1.08 {
                a = a.max((1.08 - star) / 0.08);
            }
            let i = (y * n + x) * 4;
            px[i + 3] = (a.clamp(0.0, 1.0) * 255.0) as u8;
        }
    }
    tray_icon::Icon::from_rgba(px, n as u32, n as u32).expect("tray icon")
}

fn install_fonts(ctx: &egui::Context) {
    use egui::epaint::text::VariationCoords;
    use egui::{FontData, FontDefinitions, FontFamily};
    let mut fonts = FontDefinitions::default();
    let sf = std::fs::read("/System/Library/Fonts/SFNS.ttf").ok();
    let mono = std::fs::read("/System/Library/Fonts/SFNSMono.ttf").ok();
    let mut bold_family = vec![];
    if let Some(bytes) = sf {
        let mut regular = FontData::from_owned(bytes.clone());
        regular.tweak.coords = VariationCoords::new([("wght", 430.0)]);
        let mut semibold = FontData::from_owned(bytes);
        semibold.tweak.coords = VariationCoords::new([("wght", 620.0)]);
        fonts.font_data.insert("sf".into(), Arc::new(regular));
        fonts.font_data.insert("sf-bold".into(), Arc::new(semibold));
        fonts.families.get_mut(&FontFamily::Proportional).unwrap().insert(0, "sf".into());
        bold_family.push("sf-bold".to_string());
    }
    if let Some(bytes) = mono {
        fonts.font_data.insert("sf-mono".into(), Arc::new(FontData::from_owned(bytes)));
        fonts.families.get_mut(&FontFamily::Monospace).unwrap().insert(0, "sf-mono".into());
    }
    let mut fallback = fonts.families.get(&FontFamily::Proportional).cloned().unwrap_or_default();
    bold_family.append(&mut fallback);
    fonts.families.insert(FontFamily::Name("bold".into()), bold_family);
    ctx.set_fonts(fonts);
    ctx.set_visuals(egui::Visuals::dark());
    ctx.all_styles_mut(|s| {
        s.visuals.panel_fill = Color32::TRANSPARENT;
        s.visuals.window_fill = Color32::from_rgb(30, 30, 34);
        s.spacing.scroll.bar_width = 6.0;
        s.spacing.scroll.floating = true;
        s.visuals.widgets.noninteractive.fg_stroke.color = TEXT;
    });
}
