//! macOS pieces of the Uninstaller: running apps, quitting them, app icons,
//! the drag pasteboard (to notice an app being dragged anywhere), the mouse
//! button, and running a command with the admin password. Elsewhere these
//! are inert so the tool builds and its tests run on Linux.
#![cfg_attr(not(target_os = "macos"), allow(dead_code))]

use std::path::PathBuf;

/// A `file://` URL as a path (percent-decoded).
pub fn file_url_path(url: &str) -> Option<PathBuf> {
    let rest = url.strip_prefix("file://")?;
    let rest = rest.strip_prefix("localhost").unwrap_or(rest);
    let bytes = rest.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            if let Some(b) = std::str::from_utf8(&bytes[i + 1..i + 3]).ok().and_then(|h| u8::from_str_radix(h, 16).ok()) {
                out.push(b);
                i += 3;
                continue;
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
    let s = String::from_utf8(out).ok()?;
    let s = s.trim_end_matches('/');
    (!s.is_empty()).then(|| PathBuf::from(s))
}

/// Quote for a POSIX shell.
pub fn sh_quote(s: &str) -> String {
    format!("'{}'", s.replace('\'', "'\\''"))
}

/// Quote for an AppleScript string literal.
pub fn as_quote(s: &str) -> String {
    format!("\"{}\"", s.replace('\\', "\\\\").replace('"', "\\\""))
}

/// Run AppleScript; Ok(stdout) or Err(message).
pub fn osascript(script: &str) -> Result<String, String> {
    let o = std::process::Command::new("/usr/bin/osascript").arg("-e").arg(script).output().map_err(|e| e.to_string())?;
    if o.status.success() {
        Ok(String::from_utf8_lossy(&o.stdout).into_owned())
    } else {
        let err = String::from_utf8_lossy(&o.stderr).trim().to_string();
        // "User canceled. (-128)"
        Err(if err.contains("-128") { "Cancelled".into() } else { err })
    }
}

/// Run a shell command as root after macOS asks for the admin password.
pub fn run_admin(cmd: &str, prompt: &str) -> Result<String, String> {
    osascript(&format!("do shell script {} with prompt {} with administrator privileges", as_quote(cmd), as_quote(prompt)))
}

pub fn uid() -> u32 {
    unsafe { libc::getuid() }
}

/// Open a URL or file with its default app.
pub fn open(target: &str) {
    let _ = std::process::Command::new("/usr/bin/open").arg(target).spawn();
}

/// Show a file in Finder.
pub fn reveal(path: &std::path::Path) {
    let _ = std::process::Command::new("/usr/bin/open").arg("-R").arg(path).spawn();
}

#[cfg(target_os = "macos")]
mod imp {
    use objc2::MainThreadMarker;
    use objc2_app_kit::{NSPasteboard, NSPasteboardNameDrag, NSPasteboardTypeFileURL, NSRunningApplication, NSWorkspace};
    use objc2_foundation::NSString;
    use std::path::{Path, PathBuf};

    #[link(name = "CoreGraphics", kind = "framework")]
    unsafe extern "C" {
        fn CGEventSourceButtonState(state: i32, button: u32) -> bool;
    }

    pub fn is_running(id: &str) -> bool {
        if id.is_empty() {
            return false;
        }
        let apps = NSRunningApplication::runningApplicationsWithBundleIdentifier(&NSString::from_str(id));
        apps.iter().any(|a| !a.isTerminated())
    }

    /// Ask every running copy to quit, the way the Dock's Quit does (the app
    /// can still ask to save or refuse). Returns whether any was running.
    pub fn ask_to_quit(id: &str) -> bool {
        if id.is_empty() {
            return false;
        }
        let apps = NSRunningApplication::runningApplicationsWithBundleIdentifier(&NSString::from_str(id));
        let mut any = false;
        for a in apps.iter() {
            if !a.isTerminated() {
                any = true;
                a.terminate();
            }
        }
        any
    }

    /// Whether the left mouse button is down right now.
    pub fn mouse_down() -> bool {
        // kCGEventSourceStateCombinedSessionState, kCGMouseButtonLeft
        unsafe { CGEventSourceButtonState(0, 0) }
    }

    pub fn drag_change_count() -> isize {
        unsafe { NSPasteboard::pasteboardWithName(NSPasteboardNameDrag) }.changeCount()
    }

    /// Files on the drag pasteboard (what is being, or was last, dragged).
    pub fn drag_files() -> Vec<PathBuf> {
        let pb = unsafe { NSPasteboard::pasteboardWithName(NSPasteboardNameDrag) };
        let mut out = Vec::new();
        if let Some(items) = pb.pasteboardItems() {
            for it in items.iter() {
                if let Some(u) = unsafe { it.stringForType(NSPasteboardTypeFileURL) } {
                    if let Some(p) = super::file_url_path(&u.to_string()) {
                        out.push(p);
                    }
                }
            }
        }
        out
    }

    /// The app's own icon as straight RGBA pixels, `px` square. Main thread only.
    pub fn app_icon(path: &Path, px: usize) -> Option<(usize, Vec<u8>)> {
        use objc2::AllocAnyThread;
        use objc2_app_kit::{NSBitmapImageRep, NSCompositingOperation, NSDeviceRGBColorSpace, NSGraphicsContext};
        use objc2_foundation::{NSPoint, NSRect, NSSize};
        MainThreadMarker::new()?;
        let img = NSWorkspace::sharedWorkspace().iconForFile(&NSString::from_str(&path.to_string_lossy()));
        let n = px as isize;
        let rep = unsafe {
            NSBitmapImageRep::initWithBitmapDataPlanes_pixelsWide_pixelsHigh_bitsPerSample_samplesPerPixel_hasAlpha_isPlanar_colorSpaceName_bytesPerRow_bitsPerPixel(
                NSBitmapImageRep::alloc(),
                std::ptr::null_mut(),
                n,
                n,
                8,
                4,
                true,
                false,
                NSDeviceRGBColorSpace,
                n * 4,
                32,
            )
        }?;
        let ctx = NSGraphicsContext::graphicsContextWithBitmapImageRep(&rep)?;
        let previous = NSGraphicsContext::currentContext();
        NSGraphicsContext::setCurrentContext(Some(&ctx));
        img.drawInRect_fromRect_operation_fraction(
            NSRect::new(NSPoint::new(0.0, 0.0), NSSize::new(px as f64, px as f64)),
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
        // premultiplied → straight
        let bytes = unsafe { std::slice::from_raw_parts(data, px * px * 4) };
        let mut out = bytes.to_vec();
        for p in out.chunks_exact_mut(4) {
            let a = p[3] as u32;
            if a > 0 && a < 255 {
                for c in &mut p[..3] {
                    *c = ((*c as u32 * 255 + a / 2) / a).min(255) as u8;
                }
            }
        }
        Some((px, out))
    }
}

#[cfg(not(target_os = "macos"))]
mod imp {
    use std::path::{Path, PathBuf};
    pub fn is_running(_id: &str) -> bool {
        false
    }
    pub fn ask_to_quit(_id: &str) -> bool {
        false
    }
    pub fn mouse_down() -> bool {
        false
    }
    pub fn drag_change_count() -> isize {
        0
    }
    pub fn drag_files() -> Vec<PathBuf> {
        Vec::new()
    }
    pub fn app_icon(_path: &Path, _px: usize) -> Option<(usize, Vec<u8>)> {
        None
    }
}

pub use imp::*;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quoting() {
        assert_eq!(sh_quote("/Users/a/It's here"), "'/Users/a/It'\\''s here'");
        assert_eq!(as_quote("say \"hi\" \\ ok"), "\"say \\\"hi\\\" \\\\ ok\"");
        assert_eq!(file_url_path("file:///Applications/Zoom%20Pro.app/"), Some(PathBuf::from("/Applications/Zoom Pro.app")));
    }
}
