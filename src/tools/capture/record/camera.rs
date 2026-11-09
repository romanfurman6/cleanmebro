//! The camera bubble: a round, always-on-top native window showing the
//! camera, mirrored like a mirror. It's a real window on screen, so the
//! recording picks it up; drag it anywhere. macOS only.

#[cfg(target_os = "macos")]
pub use imp::Camera;

#[cfg(not(target_os = "macos"))]
pub struct Camera;

#[cfg(not(target_os = "macos"))]
impl Camera {
    pub fn open(
        _device: Option<&str>,
        _diameter: f32,
        _center: egui::Pos2,
    ) -> Result<Camera, String> {
        Err("The camera needs macOS".into())
    }
    pub fn window_number(&self) -> u32 {
        0
    }
    pub fn set_diameter(&self, _d: f32) {}
    pub fn move_to(&self, _center: egui::Pos2) {}
}

#[cfg(target_os = "macos")]
mod imp {
    use egui::Pos2;
    use objc2::rc::Retained;
    use objc2::runtime::AnyObject;
    use objc2::{msg_send, MainThreadMarker, MainThreadOnly};
    use objc2_app_kit::{
        NSBackingStoreType, NSColor, NSScreen, NSWindow, NSWindowCollectionBehavior,
        NSWindowStyleMask,
    };
    use objc2_av_foundation::{
        AVCaptureDevice, AVCaptureDeviceInput, AVCaptureSession, AVCaptureVideoPreviewLayer,
        AVLayerVideoGravityResizeAspectFill, AVMediaTypeVideo,
    };
    use objc2_foundation::{NSPoint, NSRect, NSSize, NSString};

    pub struct Camera {
        window: Retained<NSWindow>,
        session: Retained<AVCaptureSession>,
        preview: Retained<AVCaptureVideoPreviewLayer>,
    }

    struct SendBox<T>(T);
    unsafe impl<T> Send for SendBox<T> {}

    /// A status-bar level window floats over other apps' windows too.
    const LEVEL: isize = 25;

    impl Camera {
        /// Show the bubble `diameter` points wide, centred at `center`
        /// (global points, top-left origin). Main thread only.
        pub fn open(device: Option<&str>, diameter: f32, center: Pos2) -> Result<Camera, String> {
            let mtm = MainThreadMarker::new().ok_or("not on the main thread")?;
            unsafe {
                let media = AVMediaTypeVideo.ok_or("no camera support")?;
                let dev = device
                    .and_then(|id| AVCaptureDevice::deviceWithUniqueID(&NSString::from_str(id)))
                    .or_else(|| AVCaptureDevice::defaultDeviceWithMediaType(media))
                    .ok_or("No camera found")?;
                let input = AVCaptureDeviceInput::deviceInputWithDevice_error(&dev)
                    .map_err(|e| e.localizedDescription().to_string())?;
                let session = AVCaptureSession::new();
                if !session.canAddInput(&input) {
                    return Err("The camera is busy".into());
                }
                session.addInput(&input);

                let top = NSScreen::screens(mtm)
                    .firstObject()
                    .map(|s| s.frame().size.height)
                    .unwrap_or(0.0);
                let d = diameter as f64;
                let origin =
                    NSPoint::new(center.x as f64 - d / 2.0, top - center.y as f64 - d / 2.0);
                let window = NSWindow::initWithContentRect_styleMask_backing_defer(
                    NSWindow::alloc(mtm),
                    NSRect::new(origin, NSSize::new(d, d)),
                    NSWindowStyleMask::Borderless,
                    NSBackingStoreType::Buffered,
                    false,
                );
                window.setReleasedWhenClosed(false);
                window.setOpaque(false);
                window.setBackgroundColor(Some(&NSColor::clearColor()));
                window.setHasShadow(true);
                window.setMovableByWindowBackground(true);
                window.setLevel(LEVEL);
                window.setCollectionBehavior(
                    NSWindowCollectionBehavior::CanJoinAllSpaces
                        | NSWindowCollectionBehavior::Stationary
                        | NSWindowCollectionBehavior::FullScreenAuxiliary,
                );
                let view = window.contentView().ok_or("no view")?;
                view.setWantsLayer(true);
                let preview = AVCaptureVideoPreviewLayer::layerWithSession(&session);
                if let Some(g) = AVLayerVideoGravityResizeAspectFill {
                    preview.setVideoGravity(g);
                }
                if let Some(c) = preview.connection() {
                    if c.isVideoMirroringSupported() {
                        c.setAutomaticallyAdjustsVideoMirroring(false);
                        c.setVideoMirrored(true);
                    }
                }
                let cam = Camera {
                    window,
                    session,
                    preview,
                };
                cam.lay_out(d);
                if let Some(root) = view.layer() {
                    root.addSublayer(&cam.preview);
                }
                cam.window.orderFrontRegardless();
                // starting the camera takes a moment; not on the UI thread
                let s = SendBox(cam.session.clone());
                std::thread::spawn(move || {
                    let s = s;
                    s.0.startRunning();
                });
                Ok(cam)
            }
        }

        fn lay_out(&self, d: f64) {
            unsafe {
                let r = NSRect::new(NSPoint::new(0.0, 0.0), NSSize::new(d, d));
                let layer: &AnyObject = &self.preview;
                let _: () = msg_send![layer, setFrame: r];
                let _: () = msg_send![layer, setCornerRadius: d / 2.0];
                let _: () = msg_send![layer, setMasksToBounds: true];
                let _: () = msg_send![layer, setBorderWidth: 3.0f64];
                let white = NSColor::whiteColor();
                let cg: *const std::ffi::c_void = msg_send![&*white, CGColor];
                let _: () = msg_send![layer, setBorderColor: cg];
            }
        }

        /// The window number ScreenCaptureKit knows it by.
        pub fn window_number(&self) -> u32 {
            self.window.windowNumber() as u32
        }

        /// Centre the bubble at `center` (global points, top-left origin).
        pub fn move_to(&self, center: Pos2) {
            let Some(mtm) = MainThreadMarker::new() else {
                return;
            };
            let top = NSScreen::screens(mtm)
                .firstObject()
                .map(|s| s.frame().size.height)
                .unwrap_or(0.0);
            let f = self.window.frame();
            let origin = NSPoint::new(
                center.x as f64 - f.size.width / 2.0,
                top - center.y as f64 - f.size.height / 2.0,
            );
            self.window.setFrameOrigin(origin);
        }

        pub fn set_diameter(&self, d: f32) {
            let d = d as f64;
            let f = self.window.frame();
            let c = NSPoint::new(
                f.origin.x + f.size.width / 2.0,
                f.origin.y + f.size.height / 2.0,
            );
            self.window.setFrame_display(
                NSRect::new(
                    NSPoint::new(c.x - d / 2.0, c.y - d / 2.0),
                    NSSize::new(d, d),
                ),
                true,
            );
            self.lay_out(d);
        }
    }

    impl Drop for Camera {
        fn drop(&mut self) {
            self.window.orderOut(None);
            self.window.close();
            let s = SendBox(self.session.clone());
            std::thread::spawn(move || {
                let s = s;
                unsafe { s.0.stopRunning() };
            });
        }
    }
}
