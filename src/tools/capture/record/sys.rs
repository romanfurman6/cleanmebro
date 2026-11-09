//! What the recorder asks of macOS besides the recording itself: the
//! displays and windows to pick from, cameras and microphones, their
//! permissions, and handing a finished file to Finder or the clipboard.
//! Elsewhere: one sample display and nothing else.

use egui::{Pos2, Rect};

#[derive(Clone, Debug, PartialEq)]
pub struct Display {
    pub id: u32,
    pub name: String,
    /// In global points, top-left origin (as egui places windows).
    pub frame: Rect,
    pub scale: f32,
    pub fps: u32,
}

impl Display {
    pub fn pixels(&self) -> (u32, u32) {
        ((self.frame.width() * self.scale).round() as u32, (self.frame.height() * self.scale).round() as u32)
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Win {
    pub id: u32,
    pub app: String,
    pub title: String,
    /// In global points, top-left origin.
    pub frame: Rect,
}

impl Win {
    pub fn label(&self) -> String {
        if self.title.is_empty() || self.title == self.app {
            self.app.clone()
        } else {
            format!("{} · {}", self.app, self.title)
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Device {
    pub id: String,
    pub name: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
pub enum Access {
    Allowed,
    Ask,
    Denied,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Media {
    Camera,
    Mic,
}

/// The display under `p` (global points), else the first.
pub fn display_at(list: &[Display], p: Option<Pos2>) -> usize {
    p.and_then(|p| list.iter().position(|d| d.frame.contains(p))).unwrap_or(0)
}

/// The front-most window under `p`; `list` is front to back.
pub fn window_at(list: &[Win], p: Pos2) -> Option<&Win> {
    list.iter().find(|w| w.frame.contains(p))
}

#[cfg(target_os = "macos")]
pub use imp::*;

#[cfg(target_os = "macos")]
mod imp {
    use super::{Access, Device, Display, Media, Win};
    use egui::{Pos2, Rect, Vec2};
    use objc2::rc::Retained;
    use objc2::runtime::AnyObject;
    use objc2::MainThreadMarker;
    use objc2_app_kit::{NSEvent, NSPasteboard, NSScreen, NSWorkspace};
    use objc2_av_foundation::{AVAuthorizationStatus, AVCaptureDevice, AVMediaTypeAudio, AVMediaTypeVideo};
    use objc2_foundation::{NSArray, NSDictionary, NSNumber, NSString, NSURL};
    use std::ffi::c_void;

    #[link(name = "CoreGraphics", kind = "framework")]
    unsafe extern "C" {
        fn CGWindowListCopyWindowInfo(option: u32, relative_to: u32) -> *mut c_void;
    }

    fn main_height(mtm: MainThreadMarker) -> f64 {
        NSScreen::screens(mtm).firstObject().map(|s| s.frame().size.height).unwrap_or(0.0)
    }

    /// Every display, the main one first. Main thread only.
    pub fn displays() -> Vec<Display> {
        let Some(mtm) = MainThreadMarker::new() else { return Vec::new() };
        let top = main_height(mtm);
        NSScreen::screens(mtm)
            .iter()
            .map(|s| {
                let f = s.frame();
                let id = s
                    .deviceDescription()
                    .objectForKey(&NSString::from_str("NSScreenNumber"))
                    .and_then(|n| n.downcast::<NSNumber>().ok())
                    .map(|n| n.unsignedIntValue())
                    .unwrap_or(0);
                let y = top - (f.origin.y + f.size.height);
                Display {
                    id,
                    name: s.localizedName().to_string(),
                    frame: Rect::from_min_size(Pos2::new(f.origin.x as f32, y as f32), Vec2::new(f.size.width as f32, f.size.height as f32)),
                    scale: s.backingScaleFactor() as f32,
                    fps: (s.maximumFramesPerSecond() as u32).max(30),
                }
            })
            .collect()
    }

    /// The mouse, in global points, top-left origin. Main thread only.
    pub fn mouse() -> Option<Pos2> {
        let mtm = MainThreadMarker::new()?;
        let p = NSEvent::mouseLocation();
        Some(Pos2::new(p.x as f32, (main_height(mtm) - p.y) as f32))
    }

    fn num(d: &NSDictionary<NSString, AnyObject>, key: &str) -> Option<f64> {
        d.objectForKey(&NSString::from_str(key)).and_then(|o| o.downcast::<NSNumber>().ok()).map(|n| n.doubleValue())
    }

    fn text(d: &NSDictionary<NSString, AnyObject>, key: &str) -> String {
        d.objectForKey(&NSString::from_str(key)).and_then(|o| o.downcast::<NSString>().ok()).map(|s| s.to_string()).unwrap_or_default()
    }

    /// Normal app windows on screen, front to back, not ours.
    pub fn windows() -> Vec<Win> {
        // kCGWindowListOptionOnScreenOnly | kCGWindowListExcludeDesktopElements
        let raw = unsafe { CGWindowListCopyWindowInfo(1 | 16, 0) };
        if raw.is_null() {
            return Vec::new();
        }
        // CFArray of CFDictionary is toll-free bridged to NSArray of NSDictionary
        let list: Retained<NSArray<NSDictionary<NSString, AnyObject>>> = unsafe { Retained::from_raw(raw as *mut _) }.expect("non-null");
        let me = std::process::id() as f64;
        let mut out = Vec::new();
        for d in list.iter() {
            if num(&d, "kCGWindowLayer") != Some(0.0) || num(&d, "kCGWindowOwnerPID") == Some(me) {
                continue;
            }
            let Some(b) = d.objectForKey(&NSString::from_str("kCGWindowBounds")).and_then(|o| o.downcast::<NSDictionary>().ok()) else { continue };
            let b: Retained<NSDictionary<NSString, AnyObject>> = unsafe { Retained::cast_unchecked(b) };
            let (x, y, w, h) = (num(&b, "X").unwrap_or(0.0), num(&b, "Y").unwrap_or(0.0), num(&b, "Width").unwrap_or(0.0), num(&b, "Height").unwrap_or(0.0));
            if w < 40.0 || h < 40.0 {
                continue;
            }
            let Some(id) = num(&d, "kCGWindowNumber") else { continue };
            out.push(Win {
                id: id as u32,
                app: text(&d, "kCGWindowOwnerName"),
                title: text(&d, "kCGWindowName"),
                frame: Rect::from_min_size(Pos2::new(x as f32, y as f32), Vec2::new(w as f32, h as f32)),
            });
        }
        out
    }

    fn media_type(m: Media) -> &'static NSString {
        match m {
            Media::Camera => unsafe { AVMediaTypeVideo }.expect("AVFoundation"),
            Media::Mic => unsafe { AVMediaTypeAudio }.expect("AVFoundation"),
        }
    }

    #[allow(deprecated)]
    pub fn devices(m: Media) -> Vec<Device> {
        unsafe { AVCaptureDevice::devicesWithMediaType(media_type(m)) }.iter().map(|d| Device { id: unsafe { d.uniqueID() }.to_string(), name: unsafe { d.localizedName() }.to_string() }).collect()
    }

    pub fn access(m: Media) -> Access {
        match unsafe { AVCaptureDevice::authorizationStatusForMediaType(media_type(m)) } {
            AVAuthorizationStatus::Authorized => Access::Allowed,
            AVAuthorizationStatus::NotDetermined => Access::Ask,
            _ => Access::Denied,
        }
    }

    /// Show macOS's question; `done` gets the answer on some thread.
    pub fn ask(m: Media, done: impl Fn(bool) + Send + Sync + 'static) {
        let block = block2::RcBlock::new(move |ok: objc2::runtime::Bool| done(ok.as_bool()));
        unsafe { AVCaptureDevice::requestAccessForMediaType_completionHandler(media_type(m), &block) };
    }

    pub fn open_privacy(m: Media) {
        let pane = match m {
            Media::Camera => "Privacy_Camera",
            Media::Mic => "Privacy_Microphone",
        };
        let _ = std::process::Command::new("open").arg(format!("x-apple.systempreferences:com.apple.preference.security?{pane}")).spawn();
    }

    pub fn open_file(p: &std::path::Path) {
        let _ = std::process::Command::new("open").arg(p).spawn();
    }

    /// Put the file itself on the clipboard, so pasting into a chat attaches it.
    pub fn copy_file(p: &std::path::Path) -> bool {
        let url = NSURL::fileURLWithPath(&NSString::from_str(&p.to_string_lossy()));
        let pb = NSPasteboard::generalPasteboard();
        pb.clearContents();
        let obj: Retained<objc2::runtime::ProtocolObject<dyn objc2_app_kit::NSPasteboardWriting>> = objc2::runtime::ProtocolObject::from_retained(url);
        pb.writeObjects(&NSArray::from_retained_slice(&[obj]))
    }

    pub fn reveal(p: &std::path::Path) {
        let url = NSURL::fileURLWithPath(&NSString::from_str(&p.to_string_lossy()));
        NSWorkspace::sharedWorkspace().activateFileViewerSelectingURLs(&NSArray::from_retained_slice(&[url]));
    }

    /// Lift our window titled `title` over everything on every Space; with
    /// `cover`, also over the menu bar and sized to that display.
    pub fn lift(title: &str, cover: Option<u32>) {
        use objc2_app_kit::{NSApplication, NSWindowCollectionBehavior};
        let Some(mtm) = MainThreadMarker::new() else { return };
        for w in NSApplication::sharedApplication(mtm).windows().iter() {
            if w.title().to_string() != title {
                continue;
            }
            // called every frame: only touch the window when it differs,
            // each change makes the window server redo its stacking
            let level = if cover.is_some() { 1000 } else { 25 };
            if w.level() != level {
                w.setLevel(level);
            }
            let behavior = NSWindowCollectionBehavior::CanJoinAllSpaces | NSWindowCollectionBehavior::FullScreenAuxiliary;
            if w.collectionBehavior() != behavior {
                w.setCollectionBehavior(behavior);
            }
            if let Some(id) = cover {
                let screen = NSScreen::screens(mtm).iter().find(|s| {
                    s.deviceDescription().objectForKey(&NSString::from_str("NSScreenNumber")).and_then(|n| n.downcast::<NSNumber>().ok()).map(|n| n.unsignedIntValue()) == Some(id)
                });
                if let Some(s) = screen {
                    if w.frame() != s.frame() {
                        w.setFrame_display(s.frame(), true);
                    }
                }
            }
        }
    }
}

#[cfg(not(target_os = "macos"))]
mod other {
    use super::{Access, Device, Display, Media, Win};
    use egui::{Pos2, Rect, Vec2};

    pub fn displays() -> Vec<Display> {
        vec![Display { id: 1, name: "Built-in Display".into(), frame: Rect::from_min_size(Pos2::ZERO, Vec2::new(1512.0, 982.0)), scale: 2.0, fps: 120 }]
    }
    pub fn mouse() -> Option<Pos2> {
        None
    }
    pub fn windows() -> Vec<Win> {
        Vec::new()
    }
    pub fn devices(_m: Media) -> Vec<Device> {
        Vec::new()
    }
    pub fn access(_m: Media) -> Access {
        Access::Denied
    }
    pub fn ask(_m: Media, done: impl Fn(bool) + Send + Sync + 'static) {
        done(false)
    }
    pub fn open_privacy(_m: Media) {}
    pub fn open_file(_p: &std::path::Path) {}
    pub fn copy_file(_p: &std::path::Path) -> bool {
        false
    }
    pub fn reveal(_p: &std::path::Path) {}
    pub fn lift(_title: &str, _cover: Option<u32>) {}
}

#[cfg(not(target_os = "macos"))]
pub use other::*;

#[cfg(test)]
mod tests {
    use super::*;
    use egui::Vec2;

    fn win(id: u32, x: f32, y: f32, w: f32, h: f32) -> Win {
        Win { id, app: "App".into(), title: String::new(), frame: Rect::from_min_size(Pos2::new(x, y), Vec2::new(w, h)) }
    }

    #[test]
    fn the_front_window_wins() {
        let list = [win(1, 100.0, 100.0, 200.0, 200.0), win(2, 0.0, 0.0, 800.0, 600.0)];
        assert_eq!(window_at(&list, Pos2::new(150.0, 150.0)).map(|w| w.id), Some(1));
        assert_eq!(window_at(&list, Pos2::new(500.0, 500.0)).map(|w| w.id), Some(2));
        assert!(window_at(&list, Pos2::new(900.0, 10.0)).is_none());
    }

    #[test]
    fn the_display_under_the_mouse() {
        let d = |id, x| Display { id, name: String::new(), frame: Rect::from_min_size(Pos2::new(x, 0.0), Vec2::new(1000.0, 800.0)), scale: 2.0, fps: 60 };
        let list = [d(1, 0.0), d(2, 1000.0)];
        assert_eq!(display_at(&list, Some(Pos2::new(1500.0, 10.0))), 1);
        assert_eq!(display_at(&list, None), 0);
        assert_eq!(list[1].pixels(), (2000, 1600));
    }
}
