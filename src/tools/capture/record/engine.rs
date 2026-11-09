//! The recorder itself: ScreenCaptureKit streams the screen, the system's
//! sound and the microphone; the frames go to an H.264 track and both sounds,
//! mixed, to one AAC track of an mp4 (AVAssetWriter). Our own windows (the
//! control bar, the frame around the area) are left out of the picture; the
//! camera bubble is kept in. Off macOS, starting fails with a message.

use std::path::PathBuf;

/// What to record.
#[derive(Clone, Debug, PartialEq)]
pub enum Target {
    /// A whole display (by its display id).
    Display(u32),
    /// One window (by its window number), wherever it moves.
    Window(u32),
    /// Part of a display: x, y, w, h in points from its top-left corner.
    Area { display: u32, rect: [f64; 4] },
}

#[derive(Clone, Debug)]
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
pub struct Config {
    pub target: Target,
    /// Video size in pixels.
    pub size: (u32, u32),
    pub fps: u32,
    pub bitrate: u32,
    pub cursor: bool,
    pub clicks: bool,
    pub system_audio: bool,
    /// The microphone to record: None = off, Some(None) = the default one.
    pub mic: Option<Option<String>>,
    /// Our window that stays in the video (the camera bubble).
    pub keep_window: Option<u32>,
    pub path: PathBuf,
}

/// A finished recording.
pub struct Stopped {
    pub path: PathBuf,
    pub secs: f64,
    pub size: (u32, u32),
    /// A frame from near the end, small.
    pub thumb: Option<image::RgbaImage>,
}

pub type Reply<T> = Box<dyn FnOnce(Result<T, String>) + Send + 'static>;

#[cfg(target_os = "macos")]
pub use imp::{start, Session};

#[cfg(not(target_os = "macos"))]
pub struct Session;

#[cfg(not(target_os = "macos"))]
pub fn start(_cfg: Config, reply: Reply<Session>) {
    reply(Err("Screen recording needs macOS".into()));
}

#[cfg(not(target_os = "macos"))]
impl Session {
    pub fn pause(&self) {}
    pub fn resume(&self) {}
    pub fn set_mic_muted(&self, _muted: bool) {}
    pub fn length(&self) -> f64 {
        0.0
    }
    pub fn failure(&self) -> Option<String> {
        None
    }
    pub fn stop(self, done: Reply<Stopped>) {
        done(Err("Screen recording needs macOS".into()));
    }
    pub fn discard(self) {}
}

#[cfg(target_os = "macos")]
mod imp {
    use super::super::mixer::{self, Mixer, Pcm, Src};
    use super::super::timeline::Clock;
    use super::{Config, Reply, Stopped, Target};
    use block2::RcBlock;
    use dispatch2::DispatchQueue;
    use objc2::rc::Retained;
    use objc2::runtime::{AnyObject, ProtocolObject};
    use objc2::{define_class, msg_send, sel, AnyThread, DefinedClass, Message};
    use objc2_av_foundation::{AVAssetWriter, AVAssetWriterInput, AVAssetWriterInputPixelBufferAdaptor, AVAssetWriterStatus, AVFileTypeMPEG4, AVMediaTypeAudio, AVMediaTypeVideo};
    use objc2_core_foundation::{CGPoint, CGRect, CGSize};
    use objc2_core_media::{CMSampleBuffer, CMTime, CMTimeFlags};
    use objc2_core_video::CVPixelBuffer;
    use objc2_foundation::{NSArray, NSDictionary, NSError, NSNumber, NSObject, NSObjectProtocol, NSString, NSURL};
    use objc2_screen_capture_kit::{SCContentFilter, SCShareableContent, SCStream, SCStreamConfiguration, SCStreamDelegate, SCStreamOutput, SCStreamOutputType};
    use std::ffi::c_void;
    use std::sync::{Arc, Mutex};

    // ------------------------------------------------------------ C pieces

    #[repr(C)]
    #[derive(Clone, Copy)]
    struct RawTime {
        value: i64,
        timescale: i32,
        flags: u32,
        epoch: i64,
    }

    impl RawTime {
        fn secs(self) -> Option<f64> {
            (self.flags & 1 == 1 && self.timescale > 0).then(|| self.value as f64 / self.timescale as f64)
        }
    }

    #[repr(C)]
    #[derive(Clone, Copy, Default)]
    struct Asbd {
        sample_rate: f64,
        format_id: u32,
        format_flags: u32,
        bytes_per_packet: u32,
        frames_per_packet: u32,
        bytes_per_frame: u32,
        channels_per_frame: u32,
        bits_per_channel: u32,
        reserved: u32,
    }

    #[repr(C)]
    #[derive(Clone, Copy)]
    struct AudioBuf {
        channels: u32,
        size: u32,
        data: *mut c_void,
    }

    #[repr(C)]
    struct BufList {
        n: u32,
        bufs: [AudioBuf; 8],
    }

    const LPCM: u32 = u32::from_be_bytes(*b"lpcm");
    const AAC: u32 = u32::from_be_bytes(*b"aac ");
    const FLAG_FLOAT: u32 = 1;
    const FLAG_SIGNED: u32 = 1 << 2;
    const FLAG_PACKED: u32 = 1 << 3;
    const FLAG_PLANAR: u32 = 1 << 5;
    /// '420v': what the H.264 encoder wants, no conversion on the way.
    const PIXELS_420V: u32 = u32::from_be_bytes(*b"420v");

    #[link(name = "CoreMedia", kind = "framework")]
    unsafe extern "C" {
        fn CMSampleBufferGetPresentationTimeStamp(sb: *const c_void) -> RawTime;
        fn CMSampleBufferGetImageBuffer(sb: *const c_void) -> *mut c_void;
        fn CMSampleBufferGetFormatDescription(sb: *const c_void) -> *const c_void;
        fn CMSampleBufferIsValid(sb: *const c_void) -> u8;
        fn CMAudioFormatDescriptionGetStreamBasicDescription(desc: *const c_void) -> *const Asbd;
        fn CMSampleBufferGetAudioBufferListWithRetainedBlockBuffer(
            sb: *const c_void,
            size_needed: *mut usize,
            list: *mut BufList,
            list_size: usize,
            alloc: *const c_void,
            block_alloc: *const c_void,
            flags: u32,
            block_out: *mut *mut c_void,
        ) -> i32;
        fn CMAudioFormatDescriptionCreate(
            alloc: *const c_void,
            asbd: *const Asbd,
            layout_size: usize,
            layout: *const c_void,
            cookie_size: usize,
            cookie: *const c_void,
            extensions: *const c_void,
            out: *mut *mut c_void,
        ) -> i32;
        fn CMBlockBufferCreateWithMemoryBlock(
            alloc: *const c_void,
            memory: *mut c_void,
            len: usize,
            block_alloc: *const c_void,
            custom: *const c_void,
            offset: usize,
            data_len: usize,
            flags: u32,
            out: *mut *mut c_void,
        ) -> i32;
        fn CMBlockBufferReplaceDataBytes(src: *const c_void, dest: *mut c_void, offset: usize, len: usize) -> i32;
        fn CMAudioSampleBufferCreateReadyWithPacketDescriptions(
            alloc: *const c_void,
            data: *mut c_void,
            format: *const c_void,
            samples: isize,
            pts: RawTime,
            packets: *const c_void,
            out: *mut *mut c_void,
        ) -> i32;
        fn CMClockGetHostTimeClock() -> *const c_void;
        fn CMClockGetTime(clock: *const c_void) -> RawTime;
    }

    #[link(name = "CoreFoundation", kind = "framework")]
    unsafe extern "C" {
        fn CFRelease(p: *const c_void);
    }

    #[link(name = "CoreVideo", kind = "framework")]
    unsafe extern "C" {
        fn CVPixelBufferLockBaseAddress(pb: *mut c_void, flags: u64) -> i32;
        fn CVPixelBufferUnlockBaseAddress(pb: *mut c_void, flags: u64) -> i32;
        fn CVPixelBufferGetBaseAddressOfPlane(pb: *mut c_void, plane: usize) -> *const u8;
        fn CVPixelBufferGetBytesPerRowOfPlane(pb: *mut c_void, plane: usize) -> usize;
        fn CVPixelBufferGetWidth(pb: *mut c_void) -> usize;
        fn CVPixelBufferGetHeight(pb: *mut c_void) -> usize;
        fn CVPixelBufferGetPixelFormatType(pb: *mut c_void) -> u32;
    }

    /// Now on the clock the sample buffers use.
    fn host_now() -> f64 {
        unsafe { CMClockGetTime(CMClockGetHostTimeClock()) }.secs().unwrap_or(0.0)
    }

    fn cm_time(secs: f64) -> CMTime {
        CMTime { value: (secs * 60_000.0).round() as i64, timescale: 60_000, flags: CMTimeFlags::Valid, epoch: 0 }
    }

    fn ns_err(e: &NSError) -> String {
        e.localizedDescription().to_string()
    }

    fn any<T: Message>(o: &T) -> &AnyObject {
        // every Objective-C object is an AnyObject
        unsafe { &*(o as *const T as *const AnyObject) }
    }

    fn dict(pairs: &[(&str, &AnyObject)]) -> Retained<NSDictionary<NSString, AnyObject>> {
        let keys: Vec<Retained<NSString>> = pairs.iter().map(|(k, _)| NSString::from_str(k)).collect();
        let key_refs: Vec<&NSString> = keys.iter().map(|k| &**k).collect();
        let vals: Vec<&AnyObject> = pairs.iter().map(|(_, v)| *v).collect();
        NSDictionary::from_slices(&key_refs, &vals)
    }

    // ------------------------------------------------------------ writer side

    /// Everything the sample handler touches, behind one lock.
    struct Shared {
        writer: Retained<AVAssetWriter>,
        video: Retained<AVAssetWriterInput>,
        adaptor: Retained<AVAssetWriterInputPixelBufferAdaptor>,
        audio: Option<Retained<AVAssetWriterInput>>,
        /// The format of the mixed audio we hand the writer.
        audio_fmt: *mut c_void,
        clock: Clock,
        mixer: Mixer,
        last_video: f64,
        frames: u64,
        thumb: Option<image::RgbaImage>,
        thumb_at: f64,
        failed: Option<String>,
        done: bool,
    }

    // The writer objects are thread-safe for the use here (one serial queue
    // appends, other threads only pause/stop under the same lock).
    unsafe impl Send for Shared {}

    impl Drop for Shared {
        fn drop(&mut self) {
            if !self.audio_fmt.is_null() {
                unsafe { CFRelease(self.audio_fmt) };
            }
        }
    }

    impl Shared {
        fn on_sample(&mut self, sb: &CMSampleBuffer, kind: SCStreamOutputType) {
            if self.done || self.failed.is_some() {
                return;
            }
            let raw = sb as *const CMSampleBuffer as *const c_void;
            if unsafe { CMSampleBufferIsValid(raw) } == 0 {
                return;
            }
            let Some(t) = unsafe { CMSampleBufferGetPresentationTimeStamp(raw) }.secs() else { return };
            if kind == SCStreamOutputType::Screen {
                self.on_frame(raw, t);
            } else if kind == SCStreamOutputType::Audio {
                self.on_audio(raw, t, Src::System);
            } else if kind == SCStreamOutputType::Microphone {
                self.on_audio(raw, t, Src::Mic);
            }
            if unsafe { self.writer.status() } == AVAssetWriterStatus::Failed {
                let why = unsafe { self.writer.error() }.map(|e| ns_err(&e)).unwrap_or_else(|| "the video file couldn't be written".into());
                self.failed = Some(why);
            }
        }

        fn on_frame(&mut self, raw: *const c_void, t: f64) {
            // frames where nothing changed come without a picture
            let pb = unsafe { CMSampleBufferGetImageBuffer(raw) };
            if pb.is_null() {
                return;
            }
            self.clock.start(t);
            let Some(vt) = self.clock.map(t) else { return };
            if self.frames > 0 && vt <= self.last_video {
                return;
            }
            if unsafe { self.video.isReadyForMoreMediaData() } {
                let pixels = unsafe { &*(pb as *const CVPixelBuffer) };
                if unsafe { self.adaptor.appendPixelBuffer_withPresentationTime(pixels, cm_time(vt)) } {
                    self.last_video = vt;
                    self.frames += 1;
                }
            }
            if self.thumb.is_none() || vt - self.thumb_at >= 1.0 {
                self.thumb = thumbnail(pb, 640);
                self.thumb_at = vt;
            }
        }

        fn on_audio(&mut self, raw: *const c_void, t: f64, src: Src) {
            if self.audio.is_none() {
                return;
            }
            let Some(vt) = self.clock.map(t) else { return };
            let Some(stereo) = read_pcm(raw) else { return };
            let at = (vt * mixer::RATE as f64).round() as i64;
            self.mixer.push(src, at, &stereo);
            self.drain(false);
        }

        /// Hand the mixed audio that's ready to the writer.
        fn drain(&mut self, flush: bool) {
            while let Some((at, frames)) = self.mixer.pull(flush) {
                self.append_audio(at, &frames);
            }
        }

        fn append_audio(&mut self, at: i64, stereo: &[f32]) {
            let Some(input) = &self.audio else { return };
            if stereo.is_empty() || !unsafe { input.isReadyForMoreMediaData() } {
                return;
            }
            unsafe {
                if self.audio_fmt.is_null() {
                    let asbd = Asbd {
                        sample_rate: mixer::RATE as f64,
                        format_id: LPCM,
                        format_flags: FLAG_FLOAT | FLAG_PACKED,
                        bytes_per_packet: 8,
                        frames_per_packet: 1,
                        bytes_per_frame: 8,
                        channels_per_frame: 2,
                        bits_per_channel: 32,
                        reserved: 0,
                    };
                    let mut fmt = std::ptr::null_mut();
                    if CMAudioFormatDescriptionCreate(std::ptr::null(), &asbd, 0, std::ptr::null(), 0, std::ptr::null(), std::ptr::null(), &mut fmt) != 0 {
                        return;
                    }
                    self.audio_fmt = fmt;
                }
                let len = std::mem::size_of_val(stereo);
                let mut block = std::ptr::null_mut();
                // kCMBlockBufferAssureMemoryNowFlag
                if CMBlockBufferCreateWithMemoryBlock(std::ptr::null(), std::ptr::null_mut(), len, std::ptr::null(), std::ptr::null(), 0, len, 1, &mut block) != 0 {
                    return;
                }
                if CMBlockBufferReplaceDataBytes(stereo.as_ptr() as *const c_void, block, 0, len) != 0 {
                    CFRelease(block);
                    return;
                }
                let pts = RawTime { value: at, timescale: mixer::RATE as i32, flags: 1, epoch: 0 };
                let mut sb = std::ptr::null_mut();
                let ok = CMAudioSampleBufferCreateReadyWithPacketDescriptions(std::ptr::null(), block, self.audio_fmt, (stereo.len() / 2) as isize, pts, std::ptr::null(), &mut sb) == 0;
                CFRelease(block);
                if ok && !sb.is_null() {
                    input.appendSampleBuffer(&*(sb as *const CMSampleBuffer));
                    CFRelease(sb);
                }
            }
        }
    }

    /// The audio in a sample buffer as interleaved stereo at the mixer's rate.
    fn read_pcm(raw: *const c_void) -> Option<Vec<f32>> {
        unsafe {
            let desc = CMSampleBufferGetFormatDescription(raw);
            if desc.is_null() {
                return None;
            }
            let asbd = *CMAudioFormatDescriptionGetStreamBasicDescription(desc).as_ref()?;
            if asbd.format_id != LPCM || asbd.channels_per_frame == 0 {
                return None;
            }
            let mut list: BufList = std::mem::zeroed();
            let mut block = std::ptr::null_mut();
            // kCMSampleBufferFlag_AudioBufferList_Assure16ByteAlignment
            let st = CMSampleBufferGetAudioBufferListWithRetainedBlockBuffer(raw, std::ptr::null_mut(), &mut list, std::mem::size_of::<BufList>(), std::ptr::null(), std::ptr::null(), 1, &mut block);
            if st != 0 {
                return None;
            }
            let float = asbd.format_flags & FLAG_FLOAT != 0;
            let int16 = !float && asbd.format_flags & FLAG_SIGNED != 0 && asbd.bits_per_channel == 16;
            let planar = asbd.format_flags & FLAG_PLANAR != 0;
            let n = (list.n as usize).min(8);
            let mut owned: Vec<Vec<f32>> = Vec::with_capacity(n);
            for b in &list.bufs[..n] {
                if b.data.is_null() {
                    owned.push(Vec::new());
                } else if float && asbd.bits_per_channel == 32 {
                    owned.push(std::slice::from_raw_parts(b.data as *const f32, b.size as usize / 4).to_vec());
                } else if int16 {
                    owned.push(std::slice::from_raw_parts(b.data as *const i16, b.size as usize / 2).iter().map(|&v| v as f32 / 32768.0).collect());
                } else {
                    owned.push(Vec::new());
                }
            }
            if !block.is_null() {
                CFRelease(block);
            }
            let pcm = Pcm { rate: asbd.sample_rate, channels: asbd.channels_per_frame as usize, planes: owned.iter().map(|v| v.as_slice()).collect(), interleaved: !planar };
            Some(mixer::to_stereo(&pcm))
        }
    }

    /// A small RGB copy of a '420v' (or BGRA) frame, at most `max_w` wide.
    fn thumbnail(pb: *mut c_void, max_w: u32) -> Option<image::RgbaImage> {
        unsafe {
            if CVPixelBufferLockBaseAddress(pb, 1) != 0 {
                return None;
            }
            let (w, h) = (CVPixelBufferGetWidth(pb), CVPixelBufferGetHeight(pb));
            let fmt = CVPixelBufferGetPixelFormatType(pb);
            let k = (max_w as f64 / w.max(1) as f64).min(1.0);
            let (tw, th) = (((w as f64 * k) as u32).max(1), ((h as f64 * k) as u32).max(1));
            let mut img = image::RgbaImage::new(tw, th);
            let y_base = CVPixelBufferGetBaseAddressOfPlane(pb, 0);
            let y_row = CVPixelBufferGetBytesPerRowOfPlane(pb, 0);
            let ok = !y_base.is_null();
            if ok && fmt == PIXELS_420V {
                let c_base = CVPixelBufferGetBaseAddressOfPlane(pb, 1);
                let c_row = CVPixelBufferGetBytesPerRowOfPlane(pb, 1);
                for ty in 0..th {
                    let sy = ((ty as f64 / k) as usize).min(h - 1);
                    for tx in 0..tw {
                        let sx = ((tx as f64 / k) as usize).min(w - 1);
                        let y = *y_base.add(sy * y_row + sx) as f32;
                        let (cb, cr) = if c_base.is_null() {
                            (128.0, 128.0)
                        } else {
                            let c = c_base.add((sy / 2) * c_row + (sx / 2) * 2);
                            (*c as f32, *c.add(1) as f32)
                        };
                        // BT.709, video range
                        let yy = 1.164 * (y - 16.0);
                        let r = yy + 1.793 * (cr - 128.0);
                        let g = yy - 0.213 * (cb - 128.0) - 0.533 * (cr - 128.0);
                        let b = yy + 2.112 * (cb - 128.0);
                        img.put_pixel(tx, ty, image::Rgba([r.clamp(0.0, 255.0) as u8, g.clamp(0.0, 255.0) as u8, b.clamp(0.0, 255.0) as u8, 255]));
                    }
                }
            } else if ok {
                // BGRA in one plane
                for ty in 0..th {
                    let sy = ((ty as f64 / k) as usize).min(h - 1);
                    for tx in 0..tw {
                        let sx = ((tx as f64 / k) as usize).min(w - 1);
                        let p = y_base.add(sy * y_row + sx * 4);
                        img.put_pixel(tx, ty, image::Rgba([*p.add(2), *p.add(1), *p, 255]));
                    }
                }
            }
            CVPixelBufferUnlockBaseAddress(pb, 1);
            ok.then_some(img)
        }
    }

    // ------------------------------------------------------------ stream output

    define_class!(
        #[unsafe(super(NSObject))]
        #[name = "CleanMeBroRecordOutput"]
        #[ivars = Arc<Mutex<Shared>>]
        struct Output;

        unsafe impl NSObjectProtocol for Output {}

        unsafe impl SCStreamOutput for Output {
            #[unsafe(method(stream:didOutputSampleBuffer:ofType:))]
            fn did_output(&self, _stream: &SCStream, sample: &CMSampleBuffer, kind: SCStreamOutputType) {
                if let Ok(mut s) = self.ivars().lock() {
                    s.on_sample(sample, kind);
                }
            }
        }

        unsafe impl SCStreamDelegate for Output {
            #[unsafe(method(stream:didStopWithError:))]
            fn did_stop(&self, _stream: &SCStream, error: &NSError) {
                if let Ok(mut s) = self.ivars().lock() {
                    if !s.done {
                        s.failed = Some(ns_err(error));
                    }
                }
            }
        }
    );

    impl Output {
        fn new(shared: Arc<Mutex<Shared>>) -> Retained<Self> {
            let this = Self::alloc().set_ivars(shared);
            unsafe { msg_send![super(this), init] }
        }
    }

    // ------------------------------------------------------------ session

    /// A running recording.
    pub struct Session {
        stream: Retained<SCStream>,
        output: Retained<Output>,
        shared: Arc<Mutex<Shared>>,
        path: std::path::PathBuf,
        size: (u32, u32),
    }

    // Only handed between threads; ScreenCaptureKit and AVFoundation objects
    // used here may be called from any thread.
    unsafe impl Send for Session {}

    struct SendBox<T>(T);
    unsafe impl<T> Send for SendBox<T> {}

    impl Session {
        pub fn pause(&self) {
            if let Ok(mut s) = self.shared.lock() {
                s.clock.pause(host_now());
            }
        }

        pub fn resume(&self) {
            if let Ok(mut s) = self.shared.lock() {
                s.clock.resume(host_now());
            }
        }

        pub fn set_mic_muted(&self, muted: bool) {
            if let Ok(mut s) = self.shared.lock() {
                s.mixer.mic_gain = if muted { 0.0 } else { 1.0 };
            }
        }

        /// Length of the video so far, in seconds.
        pub fn length(&self) -> f64 {
            self.shared.lock().map(|s| s.clock.length(host_now())).unwrap_or(0.0)
        }

        /// Why the recording broke off, if it did.
        pub fn failure(&self) -> Option<String> {
            self.shared.lock().ok().and_then(|s| s.failed.clone())
        }

        /// Stop and finish the file; `done` runs on a background thread.
        pub fn stop(self, done: Reply<Stopped>) {
            let Session { stream, output, shared, path, size } = self;
            let cell = Mutex::new(Some(SendBox((done, output, shared, path))));
            let block = RcBlock::new(move |_err: *mut NSError| {
                let Some(SendBox((done, _output, shared, path))) = cell.lock().ok().and_then(|mut c| c.take()) else { return };
                finish(shared, path, size, done);
            });
            unsafe { stream.stopCaptureWithCompletionHandler(Some(&block)) };
        }

        /// Stop and throw the file away.
        pub fn discard(self) {
            let path = self.path.clone();
            if let Ok(mut s) = self.shared.lock() {
                s.done = true;
                unsafe { s.writer.cancelWriting() };
            }
            let keep = SendBox(self.output.clone());
            let block = RcBlock::new(move |_err: *mut NSError| {
                let _ = &keep;
                let _ = std::fs::remove_file(&path);
            });
            unsafe { self.stream.stopCaptureWithCompletionHandler(Some(&block)) };
        }
    }

    fn finish(shared: Arc<Mutex<Shared>>, path: std::path::PathBuf, size: (u32, u32), done: Reply<Stopped>) {
        let mut s = match shared.lock() {
            Ok(s) => s,
            Err(_) => return done(Err("The recorder broke".into())),
        };
        s.done = true;
        if let Some(why) = s.failed.clone() {
            unsafe { s.writer.cancelWriting() };
            let _ = std::fs::remove_file(&path);
            return done(Err(why));
        }
        if s.frames == 0 {
            unsafe { s.writer.cancelWriting() };
            let _ = std::fs::remove_file(&path);
            return done(Err("Nothing was recorded".into()));
        }
        s.drain(true);
        let secs = s.clock.length(host_now()).max(s.last_video);
        let thumb = s.thumb.take();
        unsafe {
            s.video.markAsFinished();
            if let Some(a) = &s.audio {
                a.markAsFinished();
            }
            s.writer.endSessionAtSourceTime(cm_time(secs));
        }
        let writer = s.writer.clone();
        drop(s);
        let cell = Mutex::new(Some(SendBox((done, thumb, writer.clone(), shared))));
        let block = RcBlock::new(move || {
            let Some(SendBox((done, thumb, writer, _shared))) = cell.lock().ok().and_then(|mut c| c.take()) else { return };
            if unsafe { writer.status() } == AVAssetWriterStatus::Completed {
                done(Ok(Stopped { path: path.clone(), secs, size, thumb }));
            } else {
                let why = unsafe { writer.error() }.map(|e| ns_err(&e)).unwrap_or_else(|| "the video file couldn't be finished".into());
                done(Err(why));
            }
        });
        unsafe { writer.finishWritingWithCompletionHandler(&block) };
    }

    // ------------------------------------------------------------ start

    /// Start recording; `reply` gets the session (or why not) on a
    /// background thread once frames are flowing.
    pub fn start(cfg: Config, reply: Reply<Session>) {
        let cell = Mutex::new(Some(SendBox((cfg, reply))));
        let block = RcBlock::new(move |content: *mut SCShareableContent, err: *mut NSError| {
            let Some(SendBox((cfg, reply))) = cell.lock().ok().and_then(|mut c| c.take()) else { return };
            let content = unsafe { content.as_ref() };
            match (content, unsafe { err.as_ref() }) {
                (Some(content), _) => match build(&cfg, content) {
                    Ok((stream, output, shared)) => run(stream, output, shared, cfg, reply),
                    Err(e) => reply(Err(e)),
                },
                (None, Some(e)) => reply(Err(ns_err(e))),
                (None, None) => reply(Err("Screen Recording isn't allowed".into())),
            }
        });
        unsafe { SCShareableContent::getShareableContentExcludingDesktopWindows_onScreenWindowsOnly_completionHandler(false, true, &block) };
    }

    type Built = (Retained<SCStream>, Retained<Output>, Arc<Mutex<Shared>>);

    fn responds(obj: &AnyObject, sel: objc2::runtime::Sel) -> bool {
        unsafe { msg_send![obj, respondsToSelector: sel] }
    }

    fn build(cfg: &Config, content: &SCShareableContent) -> Result<Built, String> {
        unsafe {
            let me = std::process::id() as i32;
            let apps = content.applications();
            let ours: Vec<_> = apps.iter().filter(|a| a.processID() == me).collect();
            let windows = content.windows();
            let keep: Vec<_> = windows.iter().filter(|w| Some(w.windowID()) == cfg.keep_window).collect();
            let display_of = |id: u32| content.displays().iter().find(|d| d.displayID() == id);
            let filter = match &cfg.target {
                Target::Display(id) | Target::Area { display: id, .. } => {
                    let d = display_of(*id).ok_or("That display is gone")?;
                    let ours = NSArray::from_retained_slice(&ours);
                    let keep = NSArray::from_retained_slice(&keep);
                    SCContentFilter::initWithDisplay_excludingApplications_exceptingWindows(SCContentFilter::alloc(), &d, &ours, &keep)
                }
                Target::Window(id) => {
                    let w = windows.iter().find(|w| w.windowID() == *id).ok_or("That window is gone")?;
                    SCContentFilter::initWithDesktopIndependentWindow(SCContentFilter::alloc(), &w)
                }
            };

            let conf = SCStreamConfiguration::new();
            conf.setWidth(cfg.size.0 as usize);
            conf.setHeight(cfg.size.1 as usize);
            conf.setMinimumFrameInterval(CMTime { value: 1, timescale: cfg.fps.max(1) as i32, flags: CMTimeFlags::Valid, epoch: 0 });
            conf.setPixelFormat(PIXELS_420V);
            conf.setQueueDepth(6);
            conf.setShowsCursor(cfg.cursor);
            if responds(any(&*conf), sel!(setShowMouseClicks:)) {
                conf.setShowMouseClicks(cfg.clicks);
            }
            if let Target::Area { rect, .. } = &cfg.target {
                conf.setSourceRect(CGRect { origin: CGPoint { x: rect[0], y: rect[1] }, size: CGSize { width: rect[2], height: rect[3] } });
            }
            conf.setCapturesAudio(cfg.system_audio);
            conf.setSampleRate(mixer::RATE as isize);
            conf.setChannelCount(2);
            conf.setExcludesCurrentProcessAudio(true);
            let mic = cfg.mic.is_some() && responds(any(&*conf), sel!(setCaptureMicrophone:));
            if mic {
                conf.setCaptureMicrophone(true);
                if let Some(Some(dev)) = &cfg.mic {
                    conf.setMicrophoneCaptureDeviceID(Some(&NSString::from_str(dev)));
                }
            }

            // the file
            let _ = std::fs::remove_file(&cfg.path);
            if let Some(dir) = cfg.path.parent() {
                std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
            }
            let url = NSURL::fileURLWithPath(&NSString::from_str(&cfg.path.to_string_lossy()));
            let file_type = AVFileTypeMPEG4.ok_or("mp4 isn't available")?;
            let writer = AVAssetWriter::assetWriterWithURL_fileType_error(&url, file_type).map_err(|e| ns_err(&e))?;

            let num = |v: f64| NSNumber::new_f64(v);
            let (bitrate, fps, keyframes) = (num(cfg.bitrate as f64), num(cfg.fps as f64), num((cfg.fps * 2) as f64));
            let profile = NSString::from_str("H264_High_AutoLevel");
            let compression = dict(&[("AverageBitRate", any(&*bitrate)), ("ExpectedFrameRate", any(&*fps)), ("MaxKeyFrameInterval", any(&*keyframes)), ("ProfileLevel", any(&*profile))]);
            let (w, h) = (num(cfg.size.0 as f64), num(cfg.size.1 as f64));
            let codec = NSString::from_str("avc1");
            let vset = dict(&[("AVVideoCodecKey", any(&*codec)), ("AVVideoWidthKey", any(&*w)), ("AVVideoHeightKey", any(&*h)), ("AVVideoCompressionPropertiesKey", any(&*compression))]);
            let video = AVAssetWriterInput::assetWriterInputWithMediaType_outputSettings(AVMediaTypeVideo.ok_or("no video")?, Some(&vset));
            video.setExpectsMediaDataInRealTime(true);
            if !writer.canAddInput(&video) {
                return Err("This Mac can't encode video at that size".into());
            }
            writer.addInput(&video);
            let adaptor = AVAssetWriterInputPixelBufferAdaptor::assetWriterInputPixelBufferAdaptorWithAssetWriterInput_sourcePixelBufferAttributes(&video, None);

            let audio = if cfg.system_audio || mic {
                let (fmt, rate, ch, br) = (NSNumber::new_u32(AAC), num(mixer::RATE as f64), num(2.0), num(160_000.0));
                let aset = dict(&[("AVFormatIDKey", any(&*fmt)), ("AVSampleRateKey", any(&*rate)), ("AVNumberOfChannelsKey", any(&*ch)), ("AVEncoderBitRateKey", any(&*br))]);
                let a = AVAssetWriterInput::assetWriterInputWithMediaType_outputSettings(AVMediaTypeAudio.ok_or("no audio")?, Some(&aset));
                a.setExpectsMediaDataInRealTime(true);
                if writer.canAddInput(&a) {
                    writer.addInput(&a);
                    Some(a)
                } else {
                    None
                }
            } else {
                None
            };
            if !writer.startWriting() {
                return Err(writer.error().map(|e| ns_err(&e)).unwrap_or_else(|| "Couldn't start the video file".into()));
            }
            writer.startSessionAtSourceTime(cm_time(0.0));

            let shared = Arc::new(Mutex::new(Shared {
                writer,
                video,
                adaptor,
                audio,
                audio_fmt: std::ptr::null_mut(),
                clock: Clock::default(),
                mixer: Mixer::new(cfg.system_audio, mic),
                last_video: 0.0,
                frames: 0,
                thumb: None,
                thumb_at: 0.0,
                failed: None,
                done: false,
            }));
            let output = Output::new(shared.clone());
            let delegate: &ProtocolObject<dyn SCStreamDelegate> = ProtocolObject::from_ref(&*output);
            let stream = SCStream::initWithFilter_configuration_delegate(SCStream::alloc(), &filter, &conf, Some(delegate));
            let queue = DispatchQueue::new("cleanmebro.record", None);
            let out: &ProtocolObject<dyn SCStreamOutput> = ProtocolObject::from_ref(&*output);
            stream.addStreamOutput_type_sampleHandlerQueue_error(out, SCStreamOutputType::Screen, Some(&queue)).map_err(|e| ns_err(&e))?;
            if cfg.system_audio {
                stream.addStreamOutput_type_sampleHandlerQueue_error(out, SCStreamOutputType::Audio, Some(&queue)).map_err(|e| ns_err(&e))?;
            }
            if mic {
                stream.addStreamOutput_type_sampleHandlerQueue_error(out, SCStreamOutputType::Microphone, Some(&queue)).map_err(|e| ns_err(&e))?;
            }
            Ok((stream, output, shared))
        }
    }

    fn run(stream: Retained<SCStream>, output: Retained<Output>, shared: Arc<Mutex<Shared>>, cfg: Config, reply: Reply<Session>) {
        let session = Session { stream: stream.clone(), output, shared, path: cfg.path.clone(), size: cfg.size };
        let cell = Mutex::new(Some(SendBox((session, reply))));
        let block = RcBlock::new(move |err: *mut NSError| {
            let Some(SendBox((session, reply))) = cell.lock().ok().and_then(|mut c| c.take()) else { return };
            match unsafe { err.as_ref() } {
                Some(e) => {
                    let why = ns_err(e);
                    session.discard();
                    reply(Err(why));
                }
                None => reply(Ok(session)),
            }
        });
        unsafe { stream.startCaptureWithCompletionHandler(Some(&block)) };
    }
}
