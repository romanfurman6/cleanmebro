//! The camera bubble: a round, always-on-top native window showing the
//! camera, mirrored like a mirror; drag it anywhere. Its frames also go to
//! the recorder (when it asks, through [`set_sink`]) to be written as their
//! own track, so the bubble's look can be changed after recording. macOS
//! only.

#[cfg(target_os = "macos")]
pub use imp::{bubble, set_sink, Camera};

#[cfg(not(target_os = "macos"))]
pub struct Camera;

#[cfg(not(target_os = "macos"))]
impl Camera {
    pub fn open(_device: Option<&str>, _diameter: f32, _center: egui::Pos2) -> Result<Camera, String> {
        Err("The camera needs macOS".into())
    }
    pub fn window_number(&self) -> u32 {
        0
    }
    pub fn set_diameter(&self, _d: f32) {}
    pub fn move_to(&self, _center: egui::Pos2) {}
    pub fn frame(&self) -> egui::Rect {
        egui::Rect::NOTHING
    }
    pub fn visible(&self) -> bool {
        false
    }
    pub fn set_visible(&self, _on: bool) {}
}

#[cfg(target_os = "macos")]
mod imp {
    use egui::Pos2;
    use dispatch2::DispatchQueue;
    use objc2::rc::Retained;
    use objc2::runtime::{AnyObject, ProtocolObject};
    use objc2::{define_class, msg_send, AnyThread, MainThreadMarker, MainThreadOnly};
    use objc2_app_kit::{NSBackingStoreType, NSColor, NSScreen, NSWindow, NSWindowCollectionBehavior, NSWindowStyleMask};
    use objc2_av_foundation::{
        AVCaptureConnection, AVCaptureDevice, AVCaptureDeviceInput, AVCaptureOutput, AVCaptureSession, AVCaptureSessionPreset1280x720, AVCaptureVideoDataOutput, AVCaptureVideoDataOutputSampleBufferDelegate,
        AVCaptureVideoPreviewLayer, AVLayerVideoGravityResizeAspectFill, AVMediaTypeVideo,
    };
    use objc2_core_media::CMSampleBuffer;
    use objc2_foundation::{NSDictionary, NSNumber, NSObject, NSObjectProtocol, NSPoint, NSRect, NSSize, NSString};
    use std::cell::RefCell;
    use std::sync::Mutex;

    pub type Sink = Box<dyn FnMut(&CMSampleBuffer) + Send>;

    /// Who gets the camera's frames (the recorder, while it records).
    static SINK: Mutex<Option<Sink>> = Mutex::new(None);

    pub fn set_sink(sink: Option<Sink>) {
        if let Ok(mut s) = SINK.lock() {
            *s = sink;
        }
    }

    thread_local! {
        /// The bubble's window, for [`bubble`] (main thread).
        static BUBBLE: RefCell<Option<Retained<NSWindow>>> = const { RefCell::new(None) };
    }

    /// The bubble now: its frame (global points, top-left origin) and
    /// whether it shows. Main thread.
    pub fn bubble() -> Option<(egui::Rect, bool)> {
        let mtm = MainThreadMarker::new()?;
        let top = NSScreen::screens(mtm).firstObject().map(|s| s.frame().size.height).unwrap_or(0.0);
        BUBBLE.with(|b| {
            let b = b.borrow();
            let w = b.as_ref()?;
            let f = w.frame();
            Some((egui::Rect::from_min_size(Pos2::new(f.origin.x as f32, (top - f.origin.y - f.size.height) as f32), egui::vec2(f.size.width as f32, f.size.height as f32)), w.isVisible()))
        })
    }

    define_class!(
        #[unsafe(super(NSObject))]
        #[name = "CmdMeowCameraFrames"]
        struct Frames;

        unsafe impl NSObjectProtocol for Frames {}

        unsafe impl AVCaptureVideoDataOutputSampleBufferDelegate for Frames {
            #[unsafe(method(captureOutput:didOutputSampleBuffer:fromConnection:))]
            fn did_output(&self, _output: &AVCaptureOutput, sample: &CMSampleBuffer, _connection: &AVCaptureConnection) {
                if let Ok(mut s) = SINK.lock() {
                    if let Some(f) = s.as_mut() {
                        f(sample);
                    }
                }
            }
        }
    );

    impl Frames {
        fn new() -> Retained<Self> {
            let this = Self::alloc().set_ivars(());
            unsafe { msg_send![super(this), init] }
        }
    }

    pub struct Camera {
        window: Retained<NSWindow>,
        session: Retained<AVCaptureSession>,
        preview: Retained<AVCaptureVideoPreviewLayer>,
        _frames: Option<(Retained<AVCaptureVideoDataOutput>, Retained<Frames>)>,
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
                let input = AVCaptureDeviceInput::deviceInputWithDevice_error(&dev).map_err(|e| e.localizedDescription().to_string())?;
                let session = AVCaptureSession::new();
                if !session.canAddInput(&input) {
                    return Err("The camera is busy".into());
                }
                session.addInput(&input);
                // 720p is plenty for a bubble, and the recorder's track is that size
                if session.canSetSessionPreset(AVCaptureSessionPreset1280x720) {
                    session.setSessionPreset(AVCaptureSessionPreset1280x720);
                }
                let frames = {
                    let out = AVCaptureVideoDataOutput::new();
                    let fmt = NSNumber::new_u32(u32::from_be_bytes(*b"420v"));
                    let key = NSString::from_str("PixelFormatType");
                    out.setVideoSettings(Some(&NSDictionary::from_slices(&[&*key], &[&*fmt as &AnyObject])));
                    out.setAlwaysDiscardsLateVideoFrames(true);
                    let delegate = Frames::new();
                    let queue = DispatchQueue::new("cmdmeow.camera", None);
                    out.setSampleBufferDelegate_queue(Some(ProtocolObject::from_ref(&*delegate)), Some(&queue));
                    session.canAddOutput(&out).then(|| {
                        session.addOutput(&out);
                        (out, delegate)
                    })
                };

                let top = NSScreen::screens(mtm).firstObject().map(|s| s.frame().size.height).unwrap_or(0.0);
                let d = diameter as f64;
                let origin = NSPoint::new(center.x as f64 - d / 2.0, top - center.y as f64 - d / 2.0);
                let window = NSWindow::initWithContentRect_styleMask_backing_defer(NSWindow::alloc(mtm), NSRect::new(origin, NSSize::new(d, d)), NSWindowStyleMask::Borderless, NSBackingStoreType::Buffered, false);
                window.setReleasedWhenClosed(false);
                window.setOpaque(false);
                window.setBackgroundColor(Some(&NSColor::clearColor()));
                window.setHasShadow(true);
                window.setMovableByWindowBackground(true);
                window.setLevel(LEVEL);
                window.setCollectionBehavior(NSWindowCollectionBehavior::CanJoinAllSpaces | NSWindowCollectionBehavior::Stationary | NSWindowCollectionBehavior::FullScreenAuxiliary);
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
                BUBBLE.with(|b| *b.borrow_mut() = Some(window.clone()));
                let cam = Camera { window, session, preview, _frames: frames };
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
            let Some(mtm) = MainThreadMarker::new() else { return };
            let top = NSScreen::screens(mtm).firstObject().map(|s| s.frame().size.height).unwrap_or(0.0);
            let f = self.window.frame();
            let origin = NSPoint::new(center.x as f64 - f.size.width / 2.0, top - center.y as f64 - f.size.height / 2.0);
            self.window.setFrameOrigin(origin);
        }

        /// Where the bubble is now (global points, top-left origin): the
        /// person may have dragged it.
        pub fn frame(&self) -> egui::Rect {
            let Some(mtm) = MainThreadMarker::new() else { return egui::Rect::NOTHING };
            let top = NSScreen::screens(mtm).firstObject().map(|s| s.frame().size.height).unwrap_or(0.0);
            let f = self.window.frame();
            egui::Rect::from_min_size(Pos2::new(f.origin.x as f32, (top - f.origin.y - f.size.height) as f32), egui::vec2(f.size.width as f32, f.size.height as f32))
        }

        pub fn visible(&self) -> bool {
            self.window.isVisible()
        }

        /// Hide the bubble (and turn the camera off) or bring it back. The
        /// window stays, so a running recording keeps it in the video.
        pub fn set_visible(&self, on: bool) {
            if on == self.visible() {
                return;
            }
            if on {
                self.window.orderFrontRegardless();
            } else {
                self.window.orderOut(None);
            }
            let s = SendBox(self.session.clone());
            std::thread::spawn(move || {
                let s = s;
                unsafe {
                    if on {
                        s.0.startRunning();
                    } else {
                        s.0.stopRunning();
                    }
                }
            });
        }

        pub fn set_diameter(&self, d: f32) {
            let d = d as f64;
            let f = self.window.frame();
            let c = NSPoint::new(f.origin.x + f.size.width / 2.0, f.origin.y + f.size.height / 2.0);
            self.window.setFrame_display(NSRect::new(NSPoint::new(c.x - d / 2.0, c.y - d / 2.0), NSSize::new(d, d)), true);
            self.lay_out(d);
        }
    }

    impl Drop for Camera {
        fn drop(&mut self) {
            BUBBLE.with(|b| {
                let mut b = b.borrow_mut();
                if b.as_ref().is_some_and(|w| Retained::as_ptr(w) == Retained::as_ptr(&self.window)) {
                    *b = None;
                }
            });
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
