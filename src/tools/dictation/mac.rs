//! macOS-only pieces of Dictation: where the text caret is (Accessibility),
//! the app in front, pasting into it, and keeping the pill above everything.
//! Elsewhere these do nothing, so the tool builds and its tests run on Linux.
#![cfg_attr(not(target_os = "macos"), allow(dead_code))]

use egui::{Pos2, Rect};

/// The app the words go into.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Target {
    pub pid: i32,
    pub name: String,
}

/// What was on the clipboard before we pasted: every item, every type.
#[derive(Default)]
pub struct Saved {
    items: Vec<Vec<(String, Vec<u8>)>>,
}

/// Where to put the pill: the caret's rect in global points (top-left
/// origin) when the focused app tells it, else None.
pub fn caret(target: &Target) -> Option<Rect> {
    imp::caret(target.pid)
}

/// A rect that can't be a caret: empty at the origin, or absurdly big.
pub fn plausible_caret(r: Rect) -> bool {
    let at_origin = r.min.x.abs() < 1.0 && r.min.y.abs() < 1.0;
    r.is_finite() && !at_origin && r.height() > 2.0 && r.height() < 200.0 && r.width() < 2000.0
}

pub use imp::{accessibility, activate, float, front_app, mouse, paste, restore, screen_at, type_text};

#[cfg(target_os = "macos")]
mod imp {
    use super::{plausible_caret, Saved, Target};
    use egui::{Pos2, Rect, Vec2};
    use objc2::rc::Retained;
    use objc2::runtime::ProtocolObject;
    use objc2::MainThreadMarker;
    use objc2_app_kit::{NSApplication, NSApplicationActivationOptions, NSEvent, NSPasteboard, NSPasteboardItem, NSPasteboardTypeString, NSPasteboardWriting, NSRunningApplication, NSScreen, NSWindowCollectionBehavior, NSWorkspace};
    use objc2_foundation::{NSArray, NSData, NSString};
    use std::ffi::c_void;

    type CFTypeRef = *const c_void;

    #[repr(C)]
    #[derive(Clone, Copy, Default)]
    struct CfRange {
        location: isize,
        length: isize,
    }

    #[repr(C)]
    #[derive(Clone, Copy, Default)]
    struct CgRect {
        x: f64,
        y: f64,
        w: f64,
        h: f64,
    }

    #[repr(C)]
    #[derive(Clone, Copy, Default)]
    struct CgPoint {
        x: f64,
        y: f64,
    }

    const AX_VALUE_POINT: u32 = 1;
    const AX_VALUE_SIZE: u32 = 2;
    const AX_VALUE_RECT: u32 = 3;
    const AX_VALUE_RANGE: u32 = 4;

    #[link(name = "ApplicationServices", kind = "framework")]
    unsafe extern "C" {
        fn AXIsProcessTrusted() -> bool;
        fn AXUIElementCreateApplication(pid: i32) -> CFTypeRef;
        fn AXUIElementCopyAttributeValue(e: CFTypeRef, attr: CFTypeRef, out: *mut CFTypeRef) -> i32;
        fn AXUIElementCopyParameterizedAttributeValue(e: CFTypeRef, attr: CFTypeRef, param: CFTypeRef, out: *mut CFTypeRef) -> i32;
        fn AXUIElementSetAttributeValue(e: CFTypeRef, attr: CFTypeRef, value: CFTypeRef) -> i32;
        fn AXUIElementSetMessagingTimeout(e: CFTypeRef, secs: f32) -> i32;
        fn AXValueCreate(kind: u32, value: *const c_void) -> CFTypeRef;
        fn AXValueGetValue(v: CFTypeRef, kind: u32, out: *mut c_void) -> bool;
    }

    #[link(name = "CoreFoundation", kind = "framework")]
    unsafe extern "C" {
        fn CFRelease(x: CFTypeRef);
        static kCFBooleanTrue: CFTypeRef;
    }

    #[link(name = "CoreGraphics", kind = "framework")]
    unsafe extern "C" {
        fn CGEventSourceCreate(state: i32) -> CFTypeRef;
        fn CGEventCreateKeyboardEvent(source: CFTypeRef, key: u16, down: bool) -> CFTypeRef;
        fn CGEventSetFlags(e: CFTypeRef, flags: u64);
        fn CGEventPost(tap: u32, e: CFTypeRef);
        fn CGEventKeyboardSetUnicodeString(e: CFTypeRef, len: usize, chars: *const u16);
    }

    /// An owned CoreFoundation object, released on drop.
    struct Cf(CFTypeRef);
    impl Drop for Cf {
        fn drop(&mut self) {
            if !self.0.is_null() {
                unsafe { CFRelease(self.0) };
            }
        }
    }

    fn attr(e: CFTypeRef, name: &str) -> Option<Cf> {
        let key = NSString::from_str(name);
        let mut out: CFTypeRef = std::ptr::null();
        let err = unsafe { AXUIElementCopyAttributeValue(e, Retained::as_ptr(&key).cast(), &mut out) };
        (err == 0 && !out.is_null()).then_some(Cf(out))
    }

    fn param_attr(e: CFTypeRef, name: &str, param: CFTypeRef) -> Option<Cf> {
        let key = NSString::from_str(name);
        let mut out: CFTypeRef = std::ptr::null();
        let err = unsafe { AXUIElementCopyParameterizedAttributeValue(e, Retained::as_ptr(&key).cast(), param, &mut out) };
        (err == 0 && !out.is_null()).then_some(Cf(out))
    }

    fn value<T: Default>(v: &Cf, kind: u32) -> Option<T> {
        let mut out = T::default();
        unsafe { AXValueGetValue(v.0, kind, &mut out as *mut T as *mut c_void) }.then_some(out)
    }

    fn rect_for(elem: CFTypeRef, r: CfRange) -> Option<Rect> {
        let range = Cf(unsafe { AXValueCreate(AX_VALUE_RANGE, &r as *const CfRange as *const c_void) });
        let v = param_attr(elem, "AXBoundsForRange", range.0)?;
        let b: CgRect = value(&v, AX_VALUE_RECT)?;
        Some(Rect::from_min_size(Pos2::new(b.x as f32, b.y as f32), Vec2::new(b.w as f32, b.h as f32)))
    }

    pub fn accessibility() -> bool {
        unsafe { AXIsProcessTrusted() }
    }

    pub fn caret(pid: i32) -> Option<Rect> {
        if pid <= 0 || !accessibility() {
            return None;
        }
        let app = Cf(unsafe { AXUIElementCreateApplication(pid) });
        unsafe {
            AXUIElementSetMessagingTimeout(app.0, 0.25);
            // Chromium and Electron apps build their accessibility tree
            // only when asked to
            let key = NSString::from_str("AXManualAccessibility");
            AXUIElementSetAttributeValue(app.0, Retained::as_ptr(&key).cast(), kCFBooleanTrue);
        }
        let focused = attr(app.0, "AXFocusedUIElement")?;
        if let Some(sel) = attr(focused.0, "AXSelectedTextRange").and_then(|v| value::<CfRange>(&v, AX_VALUE_RANGE)) {
            let tries = [sel, CfRange { location: sel.location, length: 1 }, CfRange { location: (sel.location - 1).max(0), length: 1 }];
            for (k, r) in tries.iter().enumerate() {
                if let Some(b) = rect_for(focused.0, *r) {
                    if plausible_caret(b) {
                        // the character before the caret: its right edge
                        return Some(if k == 2 && sel.location > 0 { Rect::from_min_max(Pos2::new(b.right(), b.top()), b.max) } else { b });
                    }
                }
            }
        }
        // a one-line field without a caret rect: under its left edge
        let pos: CgPoint = attr(focused.0, "AXPosition").and_then(|v| value(&v, AX_VALUE_POINT))?;
        let size: CgPoint = attr(focused.0, "AXSize").and_then(|v| value(&v, AX_VALUE_SIZE))?;
        let r = Rect::from_min_size(Pos2::new(pos.x as f32, pos.y as f32), Vec2::new(size.x as f32, size.y as f32));
        (r.height() > 8.0 && r.height() < 60.0 && plausible_caret(Rect::from_min_size(r.min, Vec2::new(1.0, r.height())))).then(|| Rect::from_min_size(r.min, Vec2::new(1.0, r.height())))
    }

    pub fn front_app() -> Option<Target> {
        let app = NSWorkspace::sharedWorkspace().frontmostApplication()?;
        Some(Target { pid: app.processIdentifier(), name: app.localizedName().map(|n| n.to_string()).unwrap_or_default() })
    }

    /// Bring `t` to the front again (in case clicking our pill or panel
    /// took it away).
    pub fn activate(t: &super::Target) {
        if t.pid <= 0 || t.pid == std::process::id() as i32 {
            return;
        }
        if let Some(app) = NSRunningApplication::runningApplicationWithProcessIdentifier(t.pid) {
            if !app.isActive() {
                #[allow(deprecated)]
                let _ = app.activateWithOptions(NSApplicationActivationOptions::ActivateIgnoringOtherApps);
            }
        }
    }

    fn main_height(mtm: MainThreadMarker) -> f64 {
        NSScreen::screens(mtm).firstObject().map(|s| s.frame().size.height).unwrap_or(0.0)
    }

    pub fn mouse() -> Option<Pos2> {
        let mtm = MainThreadMarker::new()?;
        let p = NSEvent::mouseLocation();
        Some(Pos2::new(p.x as f32, (main_height(mtm) - p.y) as f32))
    }

    /// The visible part (no menu bar, no Dock) of the screen holding `p`,
    /// top-left origin.
    pub fn screen_at(p: Pos2) -> Option<Rect> {
        let mtm = MainThreadMarker::new()?;
        let top = main_height(mtm);
        let rects: Vec<(Rect, Rect)> = NSScreen::screens(mtm)
            .iter()
            .map(|s| {
                let flip = |f: objc2_foundation::NSRect| Rect::from_min_size(Pos2::new(f.origin.x as f32, (top - f.origin.y - f.size.height) as f32), Vec2::new(f.size.width as f32, f.size.height as f32));
                (flip(s.frame()), flip(s.visibleFrame()))
            })
            .collect();
        rects.iter().find(|(f, _)| f.contains(p)).or(rects.first()).map(|(_, v)| *v)
    }

    /// Keep our window `title` above other apps' windows, on every Space
    /// and over full-screen apps, and never key (typing stays where it was).
    pub fn float(title: &str) {
        let Some(mtm) = MainThreadMarker::new() else { return };
        for w in NSApplication::sharedApplication(mtm).windows().iter() {
            if w.title().to_string() != title {
                continue;
            }
            // NSStatusWindowLevel: above floating panels and the Dock
            if w.level() != 25 {
                w.setLevel(25);
            }
            let behavior = NSWindowCollectionBehavior::CanJoinAllSpaces | NSWindowCollectionBehavior::FullScreenAuxiliary | NSWindowCollectionBehavior::Stationary | NSWindowCollectionBehavior::IgnoresCycle;
            if w.collectionBehavior() != behavior {
                w.setCollectionBehavior(behavior);
            }
            if !w.ignoresMouseEvents() {
                w.setIgnoresMouseEvents(true);
            }
        }
    }

    /// Put `text` on the clipboard and press ⌘V in the app in front. The
    /// write is marked transient, so clipboard managers (ours too) skip it.
    /// Returns what was there before (for [`restore`]) and the clipboard's
    /// change count after our write; None if nothing could be written.
    /// `press`: false only puts it on the clipboard.
    pub fn paste(text: &str, press: bool) -> Option<(Saved, isize)> {
        let pb = NSPasteboard::generalPasteboard();
        let saved = save(&pb);
        pb.clearContents();
        let transient = NSString::from_str("org.nspasteboard.TransientType");
        let ok = unsafe { pb.setString_forType(&NSString::from_str(text), NSPasteboardTypeString) };
        if !ok {
            return None;
        }
        pb.setData_forType(Some(&NSData::new()), &transient);
        let count = pb.changeCount();
        if press {
            press_cmd_v();
        }
        Some((saved, count))
    }

    /// Press ⌫ `delete` times, then type `text`, as keystrokes into
    /// whatever has focus. Needs Accessibility.
    pub fn type_text(delete: usize, text: &str) {
        const KEY_DELETE: u16 = 51;
        unsafe {
            // a private source with no flags: the shortcut's ⌥, still held,
            // must not turn letters into symbols
            let src = Cf(CGEventSourceCreate(-1));
            let post = |key: u16, chars: Option<&[u16]>| {
                for down in [true, false] {
                    let e = Cf(CGEventCreateKeyboardEvent(src.0, key, down));
                    if e.0.is_null() {
                        return;
                    }
                    CGEventSetFlags(e.0, 0);
                    if let Some(c) = chars {
                        CGEventKeyboardSetUnicodeString(e.0, c.len(), c.as_ptr());
                    }
                    CGEventPost(0, e.0);
                }
            };
            for _ in 0..delete {
                post(KEY_DELETE, None);
            }
            // apps take at most 20 UTF-16 units an event
            let units: Vec<u16> = text.encode_utf16().collect();
            let mut k = 0;
            while k < units.len() {
                let mut end = (k + 20).min(units.len());
                // never split a surrogate pair
                if end < units.len() && (0xDC00..0xE000).contains(&units[end]) {
                    end -= 1;
                }
                post(0, Some(&units[k..end]));
                k = end;
            }
        }
    }

    fn press_cmd_v() {
        const KEY_V: u16 = 9;
        const FLAG_CMD: u64 = 0x0010_0000;
        unsafe {
            // a private source: modifiers still held on the keyboard (the
            // shortcut's ⌥) don't mix into our ⌘V
            let src = Cf(CGEventSourceCreate(-1));
            for down in [true, false] {
                let e = Cf(CGEventCreateKeyboardEvent(src.0, KEY_V, down));
                if e.0.is_null() {
                    return;
                }
                CGEventSetFlags(e.0, FLAG_CMD);
                // kCGHIDEventTap
                CGEventPost(0, e.0);
            }
        }
    }

    fn save(pb: &NSPasteboard) -> Saved {
        let mut items = Vec::new();
        if let Some(list) = pb.pasteboardItems() {
            for it in list.iter() {
                let mut kinds = Vec::new();
                for ty in it.types().iter() {
                    if let Some(d) = it.dataForType(&ty) {
                        kinds.push((ty.to_string(), d.to_vec()));
                    }
                }
                if !kinds.is_empty() {
                    items.push(kinds);
                }
            }
        }
        Saved { items }
    }

    /// Put back what [`paste`] replaced, unless someone copied since
    /// (`count` no longer matches).
    pub fn restore(saved: Saved, count: isize) {
        let pb = NSPasteboard::generalPasteboard();
        if pb.changeCount() != count {
            return;
        }
        pb.clearContents();
        // it was empty before: leave it empty, not holding what was said
        if saved.items.is_empty() {
            return;
        }
        let transient = NSString::from_str("org.nspasteboard.TransientType");
        let items: Vec<Retained<ProtocolObject<dyn NSPasteboardWriting>>> = saved
            .items
            .iter()
            .map(|kinds| {
                let it = NSPasteboardItem::new();
                for (ty, data) in kinds {
                    it.setData_forType(&NSData::with_bytes(data), &NSString::from_str(ty));
                }
                // it is already in clipboard history; don't add it twice
                it.setData_forType(&NSData::new(), &transient);
                ProtocolObject::from_retained(it)
            })
            .collect();
        pb.writeObjects(&NSArray::from_retained_slice(&items));
    }
}

#[cfg(not(target_os = "macos"))]
mod imp {
    use super::{Saved, Target};
    use egui::{Pos2, Rect};

    pub fn accessibility() -> bool {
        false
    }
    pub fn caret(_pid: i32) -> Option<Rect> {
        None
    }
    pub fn front_app() -> Option<Target> {
        None
    }
    pub fn activate(_t: &Target) {}
    pub fn mouse() -> Option<Pos2> {
        None
    }
    pub fn screen_at(_p: Pos2) -> Option<Rect> {
        None
    }
    pub fn float(_title: &str) {}
    pub fn paste(_text: &str, _press: bool) -> Option<(Saved, isize)> {
        None
    }
    pub fn restore(_saved: Saved, _count: isize) {}
    pub fn type_text(_delete: usize, _text: &str) {}
}

/// Top-left of a `size` pill: just under the caret, else under the mouse,
/// kept inside `screen`.
pub fn pill_pos(caret: Option<Rect>, mouse: Option<Pos2>, screen: Option<Rect>, size: egui::Vec2) -> Pos2 {
    let want = match (caret, mouse) {
        (Some(c), _) => Pos2::new(c.left() - 14.0, c.bottom() + 8.0),
        (None, Some(m)) => Pos2::new(m.x - size.x / 2.0, m.y + 22.0),
        (None, None) => Pos2::new(600.0, 400.0),
    };
    let Some(s) = screen else { return want };
    let mut p = want;
    // no room below: above the caret instead
    if p.y + size.y > s.bottom() - 4.0 {
        p.y = caret.map(|c| c.top() - 8.0 - size.y).unwrap_or(s.bottom() - 4.0 - size.y);
    }
    p.x = p.x.clamp(s.left() + 4.0, (s.right() - 4.0 - size.x).max(s.left() + 4.0));
    p.y = p.y.clamp(s.top() + 4.0, (s.bottom() - 4.0 - size.y).max(s.top() + 4.0));
    p
}

#[cfg(test)]
mod tests {
    use super::*;
    use egui::Vec2;

    #[test]
    fn carets_that_cant_be_real_are_refused() {
        assert!(plausible_caret(Rect::from_min_size(Pos2::new(300.0, 200.0), Vec2::new(1.0, 17.0))));
        assert!(!plausible_caret(Rect::from_min_size(Pos2::ZERO, Vec2::new(1.0, 17.0))));
        assert!(!plausible_caret(Rect::from_min_size(Pos2::new(300.0, 200.0), Vec2::new(0.0, 0.0))));
        assert!(!plausible_caret(Rect::from_min_size(Pos2::new(300.0, 200.0), Vec2::new(900.0, 700.0))));
    }

    #[test]
    fn the_pill_sits_under_the_caret_and_stays_on_screen() {
        let screen = Some(Rect::from_min_size(Pos2::new(0.0, 25.0), Vec2::new(1440.0, 850.0)));
        let size = Vec2::new(140.0, 36.0);
        let caret = Some(Rect::from_min_size(Pos2::new(500.0, 300.0), Vec2::new(1.0, 18.0)));
        assert_eq!(pill_pos(caret, None, screen, size), Pos2::new(486.0, 326.0));
        // at the bottom: above the caret
        let low = Some(Rect::from_min_size(Pos2::new(500.0, 850.0), Vec2::new(1.0, 18.0)));
        assert_eq!(pill_pos(low, None, screen, size).y, 850.0 - 8.0 - 36.0);
        // at the right edge: pulled in
        let right = Some(Rect::from_min_size(Pos2::new(1435.0, 300.0), Vec2::new(1.0, 18.0)));
        assert_eq!(pill_pos(right, None, screen, size).x, 1440.0 - 4.0 - 140.0);
        // no caret: under the mouse
        assert_eq!(pill_pos(None, Some(Pos2::new(700.0, 400.0)), screen, size), Pos2::new(630.0, 422.0));
    }
}
