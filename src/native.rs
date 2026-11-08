//! macOS bits: the real frosted-glass material behind our window, menu-bar app
//! behaviour, thermal state and the list of open apps.

#[cfg(target_os = "macos")]
mod mac {
    use objc2::rc::Retained;
    use objc2::MainThreadMarker;
    use objc2_app_kit::{
        NSAppearance, NSAppearanceCustomization, NSAppearanceNameDarkAqua, NSApplication, NSApplicationActivationPolicy,
        NSAutoresizingMaskOptions, NSColor, NSGlassEffectView, NSGlassEffectViewStyle, NSView, NSVisualEffectBlendingMode,
        NSVisualEffectMaterial, NSVisualEffectState, NSVisualEffectView, NSWindowOrderingMode, NSWorkspace,
    };
    use objc2_foundation::NSProcessInfo;
    use std::cell::RefCell;

    thread_local! {
        static GLASS: RefCell<Vec<(isize, Retained<NSView>)>> = const { RefCell::new(Vec::new()) };
    }

    fn has_liquid_glass() -> bool {
        objc2::runtime::AnyClass::get(c"NSGlassEffectView").is_some()
    }

    /// Put the system's own glass behind every egui window: Liquid Glass
    /// (NSGlassEffectView, macOS 26+) with its own rounded corners, or the
    /// older frosted NSVisualEffectView. Our drawing stays on top, mostly clear.
    pub fn apply_glass(corner_radius: f64) {
        let Some(mtm) = MainThreadMarker::new() else { return };
        let app = NSApplication::sharedApplication(mtm);
        let windows = app.windows();
        for w in windows.iter() {
            let Some(content) = w.contentView() else { continue };
            let class = content.class().name().to_string_lossy().to_string();
            if !class.contains("Winit") {
                continue;
            }
            let num = w.windowNumber();
            let known = GLASS.with(|g| g.borrow().iter().any(|(n, _)| *n == num));
            if known {
                continue;
            }
            let Some(superview) = (unsafe { content.superview() }) else { continue };
            w.setOpaque(false);
            w.setBackgroundColor(Some(&NSColor::clearColor()));
            // dark glass so white text stays readable on any wallpaper
            if let Some(dark) = NSAppearance::appearanceNamed(unsafe { NSAppearanceNameDarkAqua }) {
                w.setAppearance(Some(&dark));
            }
            let frame = content.frame();
            let view: Retained<NSView> = if has_liquid_glass() {
                let g = NSGlassEffectView::initWithFrame(mtm.alloc::<NSGlassEffectView>(), frame);
                g.setStyle(NSGlassEffectViewStyle::Regular);
                g.setCornerRadius(corner_radius);
                Retained::into_super(g)
            } else {
                let e = NSVisualEffectView::initWithFrame(mtm.alloc::<NSVisualEffectView>(), frame);
                e.setMaterial(NSVisualEffectMaterial::HUDWindow);
                e.setBlendingMode(NSVisualEffectBlendingMode::BehindWindow);
                e.setState(NSVisualEffectState::Active);
                Retained::into_super(e)
            };
            view.setAutoresizingMask(NSAutoresizingMaskOptions::ViewWidthSizable | NSAutoresizingMaskOptions::ViewHeightSizable);
            superview.addSubview_positioned_relativeTo(&view, NSWindowOrderingMode::Below, Some(&content));
            w.invalidateShadow();
            GLASS.with(|g| g.borrow_mut().push((num, view)));
        }
    }

    /// Diagnostic: describe our windows (frames, view classes, glass layers).
    pub fn debug_windows() -> String {
        let Some(mtm) = MainThreadMarker::new() else { return String::new() };
        let app = NSApplication::sharedApplication(mtm);
        let mut out = String::new();
        for w in app.windows().iter() {
            let f = w.frame();
            out.push_str(&format!("win #{} frame {:?} opaque {} visible {}\n", w.windowNumber(), (f.origin.x, f.origin.y, f.size.width, f.size.height), w.isOpaque(), w.isVisible()));
            if let Some(c) = w.contentView() {
                let cf = c.frame();
                out.push_str(&format!("  content {} {:?}\n", c.class().name().to_string_lossy(), (cf.size.width, cf.size.height)));
                if let Some(sv) = unsafe { c.superview() } {
                    out.push_str(&format!("  super {}\n", sv.class().name().to_string_lossy()));
                    for v in sv.subviews().iter() {
                        let vf = v.frame();
                        out.push_str(&format!("    sub {} {:?} hidden {}\n", v.class().name().to_string_lossy(), (vf.origin.x, vf.origin.y, vf.size.width, vf.size.height), v.isHidden()));
                    }
                }
            }
        }
        out
    }

    /// Accessory = menu bar app with no Dock icon; Regular = normal app window.
    pub fn set_dock_icon(visible: bool) {
        let Some(mtm) = MainThreadMarker::new() else { return };
        let app = NSApplication::sharedApplication(mtm);
        let policy = if visible { NSApplicationActivationPolicy::Regular } else { NSApplicationActivationPolicy::Accessory };
        let _ = app.setActivationPolicy(policy);
    }

    pub fn activate() {
        let Some(mtm) = MainThreadMarker::new() else { return };
        let app = NSApplication::sharedApplication(mtm);
        #[allow(deprecated)]
        app.activateIgnoringOtherApps(true);
    }

    /// Render an SF Symbol (Apple's own icon set) to straight RGBA pixels, white
    /// with alpha, `px` pixels tall. None when the name doesn't exist.
    pub fn render_symbol(name: &str, px: f64, weight: f64) -> Option<(usize, usize, Vec<u8>)> {
        use objc2::AllocAnyThread;
        use objc2_app_kit::{NSBitmapImageRep, NSCompositingOperation, NSDeviceRGBColorSpace, NSGraphicsContext, NSImage, NSImageSymbolConfiguration};
        use objc2_foundation::{NSPoint, NSRect, NSSize, NSString};
        MainThreadMarker::new()?;
        let img = NSImage::imageWithSystemSymbolName_accessibilityDescription(&NSString::from_str(name), None)?;
        let cfg = NSImageSymbolConfiguration::configurationWithPointSize_weight(px, weight);
        let img = img.imageWithSymbolConfiguration(&cfg)?;
        let size = img.size();
        if size.width <= 0.0 || size.height <= 0.0 {
            return None;
        }
        let w = (size.width.ceil() as isize) + 2;
        let h = (size.height.ceil() as isize) + 2;
        let rep = unsafe {
            NSBitmapImageRep::initWithBitmapDataPlanes_pixelsWide_pixelsHigh_bitsPerSample_samplesPerPixel_hasAlpha_isPlanar_colorSpaceName_bytesPerRow_bitsPerPixel(
                NSBitmapImageRep::alloc(),
                std::ptr::null_mut(),
                w,
                h,
                8,
                4,
                true,
                false,
                NSDeviceRGBColorSpace,
                w * 4,
                32,
            )
        }?;
        let ctx = NSGraphicsContext::graphicsContextWithBitmapImageRep(&rep)?;
        let previous = NSGraphicsContext::currentContext();
        NSGraphicsContext::setCurrentContext(Some(&ctx));
        img.drawInRect_fromRect_operation_fraction(
            NSRect::new(NSPoint::new(1.0, 1.0), NSSize::new(size.width, size.height)),
            NSRect::new(NSPoint::new(0.0, 0.0), NSSize::new(0.0, 0.0)),
            NSCompositingOperation::SourceOver,
            1.0,
        );
        ctx.flushGraphics();
        NSGraphicsContext::setCurrentContext(previous.as_deref());
        let data = rep.bitmapData();
        if data.is_null() {
            return None;
        }
        let bytes = unsafe { std::slice::from_raw_parts(data, (w * h * 4) as usize) };
        // keep only the coverage; colour is applied when drawing
        let mut out = vec![0u8; bytes.len()];
        for i in 0..(w * h) as usize {
            let a = bytes[i * 4 + 3];
            out[i * 4] = 255;
            out[i * 4 + 1] = 255;
            out[i * 4 + 2] = 255;
            out[i * 4 + 3] = a;
        }
        Some((w as usize, h as usize, out))
    }

    pub fn weight_semibold() -> f64 {
        unsafe { objc2_app_kit::NSFontWeightSemibold }
    }

    pub fn thermal_state() -> u8 {
        let s = NSProcessInfo::processInfo().thermalState();
        s.0.clamp(0, 3) as u8
    }

    pub fn running_apps() -> (Vec<String>, Vec<String>) {
        let ws = NSWorkspace::sharedWorkspace();
        let apps = ws.runningApplications();
        let mut ids = Vec::new();
        let mut names = Vec::new();
        for a in apps.iter() {
            if let Some(id) = a.bundleIdentifier() {
                ids.push(id.to_string());
            }
            if let Some(n) = a.localizedName() {
                names.push(n.to_string());
            }
        }
        (ids, names)
    }

    /// Move a file or folder to the Trash the way Finder does: it lands in the
    /// Trash of its own volume (so external drives work) and can be put back.
    pub fn trash_item(p: &std::path::Path) -> Result<(), String> {
        use objc2_foundation::{NSFileManager, NSString, NSURL};
        let url = NSURL::fileURLWithPath(&NSString::from_str(&p.to_string_lossy()));
        NSFileManager::defaultManager()
            .trashItemAtURL_resultingItemURL_error(&url, None)
            .map_err(|e| e.localizedDescription().to_string())
    }
}

#[cfg(target_os = "macos")]
pub use mac::*;

#[cfg(not(target_os = "macos"))]
mod other {
    pub fn apply_glass(_r: f64) {}
    pub fn debug_windows() -> String {
        String::new()
    }
    pub fn set_dock_icon(_v: bool) {}
    pub fn activate() {}
    pub fn thermal_state() -> u8 {
        0
    }
    pub fn render_symbol(_n: &str, _px: f64, _w: f64) -> Option<(usize, usize, Vec<u8>)> {
        None
    }
    pub fn weight_semibold() -> f64 {
        0.3
    }
    pub fn running_apps() -> (Vec<String>, Vec<String>) {
        (vec![], vec![])
    }
    /// Move into ~/.Trash. Refuses (instead of deleting) when the item is on
    /// another volume, so "Move to Trash" never turns into a permanent delete.
    pub fn trash_item(p: &std::path::Path) -> Result<(), String> {
        let trash = dirs::home_dir().ok_or("No home folder")?.join(".Trash");
        std::fs::create_dir_all(&trash).map_err(|e| e.to_string())?;
        let base = p.file_name().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
        let mut dest = trash.join(&base);
        let mut k = 1;
        while dest.symlink_metadata().is_ok() {
            dest = trash.join(format!("{base} {k}"));
            k += 1;
        }
        std::fs::rename(p, &dest).map_err(|e| e.to_string())
    }
}

#[cfg(not(target_os = "macos"))]
pub use other::*;
