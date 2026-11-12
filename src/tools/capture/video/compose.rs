//! Renders a recording with its look: the raw video (no cursor in it) goes
//! frame by frame through [`scene::Compositor`] and out as a new H.264 file;
//! the sound is copied as it is. A recording that needs this keeps its raw
//! video and what was logged beside it (a "project"), so the look can be
//! changed and the video rendered again later.

use super::scene::{Recorded, Shape, Sprite, Sprites, Style};
use super::speech::Line;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU32};

/// What a project keeps beside its raw video.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Meta {
    /// The rendered file (in the Capture folder).
    pub out: PathBuf,
    pub recorded: Recorded,
    pub style: Style,
    /// The raw video's pixels per point.
    pub ppp: f32,
}

/// A recording's project folder: raw.mp4 and meta.json.
pub struct Project {
    pub dir: PathBuf,
}

impl Project {
    /// The folder for the recording that will be saved as `out`.
    pub fn for_output(root: &Path, out: &Path) -> Project {
        let stem = out.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_else(|| "Recording".into());
        Project { dir: root.join(stem) }
    }

    /// The project of a rendered file, if it has one.
    pub fn find(root: &Path, out: &Path) -> Option<(Project, Meta)> {
        let p = Project::for_output(root, out);
        let meta = p.meta()?;
        (meta.out == out && p.raw().exists()).then_some((p, meta))
    }

    pub fn raw(&self) -> PathBuf {
        self.dir.join("raw.mp4")
    }

    pub fn meta(&self) -> Option<Meta> {
        let s = std::fs::read_to_string(self.dir.join("meta.json")).ok()?;
        serde_json::from_str(&s).ok()
    }

    pub fn save(&self, meta: &Meta) -> bool {
        std::fs::create_dir_all(&self.dir).is_ok() && crate::kit::fs::save_json(&self.dir.join("meta.json"), meta, true)
    }
}

/// Projects whose rendered file is gone (deleted from the folder or
/// History) are removed. Returns how many.
pub fn prune(root: &Path) -> usize {
    let Ok(rd) = std::fs::read_dir(root) else { return 0 };
    let mut n = 0;
    for e in rd.flatten() {
        let p = Project { dir: e.path() };
        let gone = match p.meta() {
            Some(m) => !m.out.exists(),
            // a recording still being written has no meta yet: leave it a day
            None => e.metadata().and_then(|m| m.modified()).ok().and_then(|t| t.elapsed().ok()).is_some_and(|age| age.as_secs() > 86_400),
        };
        if gone && std::fs::remove_dir_all(&p.dir).is_ok() {
            n += 1;
        }
    }
    n
}

/// The system's cursor pictures (RGBA, 2 px per point) with their hot
/// spots in pixels. Sendable; turned into [`Sprites`] on the render thread.
#[derive(Clone, Default)]
pub struct Pictures(pub Vec<(Shape, image::RgbaImage, (f32, f32))>);

impl Pictures {
    pub fn sprites(&self) -> Sprites {
        let get = |shape: Shape| self.0.iter().find(|p| p.0 == shape).and_then(|(_, img, hot)| Sprite::from_rgba(img, *hot, 2.0));
        Sprites { arrow: get(Shape::Arrow).unwrap_or_else(Sprite::arrow), text: get(Shape::Text), hand: get(Shape::Hand) }
    }
}

/// One render.
pub struct Job {
    pub raw: PathBuf,
    pub out: PathBuf,
    pub meta: Meta,
    pub lines: Vec<Line>,
    pub pictures: Pictures,
}

/// Tells the renderer how far it is (per mille) and when to give up.
#[derive(Default)]
pub struct Progress {
    pub done: AtomicU32,
    pub cancel: AtomicBool,
}

/// Output frames for `secs` at `fps`: one at every 1/fps from 0, all
/// before the end.
pub fn frame_count(secs: f64, fps: u32) -> u64 {
    let fps = fps.max(1) as f64;
    ((secs * fps) - 1e-6).ceil().max(1.0) as u64
}

#[cfg(target_os = "macos")]
pub use imp::{cursor_pictures, render, still};

#[cfg(not(target_os = "macos"))]
pub fn cursor_pictures() -> Pictures {
    Pictures::default()
}

/// One frame of `job` at `t` (seconds of the raw video), RGBA, for a
/// preview. Slow-ish: off the UI thread.
#[cfg(not(target_os = "macos"))]
pub fn still(_job: &Job, _t: f64) -> Result<image::RgbaImage, String> {
    Err("Needs macOS".into())
}

#[cfg(not(target_os = "macos"))]
pub fn render(_job: &Job, _progress: &Progress) -> Result<(u32, u32), String> {
    Err("Rendering video needs macOS".into())
}

#[cfg(target_os = "macos")]
mod imp {
    use super::super::scene::{Compositor, FrameIn, Shape};
    use super::{Job, Pictures, Progress};
    use crate::tools::capture::record::timeline;
    use crate::tools::capture::settings::RecQuality;
    use block2::RcBlock;
    use objc2::rc::Retained;
    use objc2::runtime::AnyObject;
    use objc2::{AllocAnyThread, MainThreadMarker};
    use objc2_av_foundation::{
        AVAssetReader, AVAssetReaderStatus, AVAssetReaderTrackOutput, AVAssetTrack, AVAssetWriter, AVAssetWriterInput, AVAssetWriterInputPixelBufferAdaptor, AVAssetWriterStatus, AVFileTypeMPEG4, AVMediaTypeAudio, AVMediaTypeVideo,
        AVURLAsset,
    };
    use objc2_core_media::{CMFormatDescription, CMSampleBuffer, CMTime, CMTimeFlags};
    use objc2_core_video::CVPixelBuffer;
    use objc2_foundation::{NSDictionary, NSError, NSNumber, NSString, NSURL};
    use std::ffi::c_void;
    use std::sync::atomic::Ordering;

    #[repr(C)]
    #[derive(Clone, Copy)]
    struct RawTime {
        value: i64,
        timescale: i32,
        flags: u32,
        epoch: i64,
    }

    #[link(name = "CoreMedia", kind = "framework")]
    unsafe extern "C" {
        fn CMSampleBufferGetPresentationTimeStamp(sb: *const c_void) -> RawTime;
        fn CMSampleBufferGetImageBuffer(sb: *const c_void) -> *mut c_void;
    }

    #[link(name = "CoreVideo", kind = "framework")]
    unsafe extern "C" {
        fn CVPixelBufferLockBaseAddress(pb: *mut c_void, flags: u64) -> i32;
        fn CVPixelBufferUnlockBaseAddress(pb: *mut c_void, flags: u64) -> i32;
        fn CVPixelBufferGetBaseAddress(pb: *mut c_void) -> *const u8;
        fn CVPixelBufferGetBytesPerRow(pb: *mut c_void) -> usize;
        fn CVPixelBufferGetWidth(pb: *mut c_void) -> usize;
        fn CVPixelBufferGetHeight(pb: *mut c_void) -> usize;
        fn CVPixelBufferPoolCreatePixelBuffer(alloc: *const c_void, pool: *const c_void, out: *mut *mut c_void) -> i32;
    }

    #[link(name = "CoreFoundation", kind = "framework")]
    unsafe extern "C" {
        fn CFRelease(p: *const c_void);
    }

    const BGRA: u32 = u32::from_be_bytes(*b"BGRA");

    fn cm(secs: f64) -> CMTime {
        CMTime { value: (secs.max(0.0) * 60_000.0).round() as i64, timescale: 60_000, flags: CMTimeFlags::Valid, epoch: 0 }
    }

    fn secs(t: CMTime) -> f64 {
        let s = unsafe { t.seconds() };
        if s.is_finite() {
            s
        } else {
            0.0
        }
    }

    fn ns_err(e: &NSError) -> String {
        e.localizedDescription().to_string()
    }

    fn any<T: objc2::Message>(o: &T) -> &AnyObject {
        unsafe { &*(o as *const T as *const AnyObject) }
    }

    fn dict(pairs: &[(&str, &AnyObject)]) -> Retained<NSDictionary<NSString, AnyObject>> {
        let keys: Vec<Retained<NSString>> = pairs.iter().map(|(k, _)| NSString::from_str(k)).collect();
        let key_refs: Vec<&NSString> = keys.iter().map(|k| &**k).collect();
        let vals: Vec<&AnyObject> = pairs.iter().map(|(_, v)| *v).collect();
        NSDictionary::from_slices(&key_refs, &vals)
    }

    /// A video track read as BGRA, one frame ahead so we know when the
    /// next one starts.
    struct Frames {
        out: Retained<AVAssetReaderTrackOutput>,
        next: Option<(f64, Retained<CMSampleBuffer>)>,
        /// The frame on screen now: tight BGRA.
        cur: Vec<u8>,
        size: (u32, u32),
        ended: bool,
    }

    impl Frames {
        fn pull(&mut self) {
            if self.ended || self.next.is_some() {
                return;
            }
            match unsafe { self.out.copyNextSampleBuffer() } {
                Some(sb) => {
                    let raw = &*sb as *const CMSampleBuffer as *const c_void;
                    let pts = unsafe { CMSampleBufferGetPresentationTimeStamp(raw) };
                    let t = if pts.flags & 1 == 1 && pts.timescale > 0 { pts.value as f64 / pts.timescale as f64 } else { 0.0 };
                    self.next = Some((t, sb));
                }
                None => self.ended = true,
            }
        }

        /// Move to the frame shown at `t`.
        fn seek(&mut self, t: f64) {
            loop {
                self.pull();
                let take = match &self.next {
                    // the first frame shows from the start whenever it came
                    Some((s, _)) => *s <= t + 1e-6 || self.cur.is_empty(),
                    None => false,
                };
                if !take {
                    return;
                }
                if let Some((_, sb)) = self.next.take() {
                    self.copy(&sb);
                }
            }
        }

        fn copy(&mut self, sb: &CMSampleBuffer) {
            unsafe {
                let pb = CMSampleBufferGetImageBuffer(sb as *const CMSampleBuffer as *const c_void);
                if pb.is_null() || CVPixelBufferLockBaseAddress(pb, 1) != 0 {
                    return;
                }
                let (w, h, row) = (CVPixelBufferGetWidth(pb), CVPixelBufferGetHeight(pb), CVPixelBufferGetBytesPerRow(pb));
                let base = CVPixelBufferGetBaseAddress(pb);
                if !base.is_null() && w > 0 && h > 0 {
                    let src = std::slice::from_raw_parts(base, row * h);
                    self.cur.resize(w * h * 4, 0);
                    for y in 0..h {
                        self.cur[y * w * 4..(y + 1) * w * 4].copy_from_slice(&src[y * row..y * row + w * 4]);
                    }
                    self.size = (w as u32, h as u32);
                }
                CVPixelBufferUnlockBaseAddress(pb, 1);
            }
        }
    }

    /// Sound copied across untouched.
    struct Lane {
        out: Retained<AVAssetReaderTrackOutput>,
        input: Retained<AVAssetWriterInput>,
        done: bool,
    }

    /// Render `job`; blocks (run it on its own thread). Gives the video's
    /// size.
    #[allow(deprecated)]
    pub fn render(job: &Job, progress: &Progress) -> Result<(u32, u32), String> {
        let t0 = std::time::Instant::now();
        let r = unsafe { run(job, progress) };
        eprintln!("[rec] render {:?} → {:?} in {:.1} s: {:?}", job.raw, job.out, t0.elapsed().as_secs_f32(), r);
        if r.is_err() {
            let _ = std::fs::remove_file(&job.out);
        }
        r
    }

    #[allow(deprecated)]
    unsafe fn run(job: &Job, progress: &Progress) -> Result<(u32, u32), String> {
        unsafe {
            let asset = AVURLAsset::URLAssetWithURL_options(&NSURL::fileURLWithPath(&NSString::from_str(&job.raw.to_string_lossy())), None);
            let (vm, am) = (AVMediaTypeVideo.ok_or("no video")?, AVMediaTypeAudio.ok_or("no audio")?);
            let duration = secs(asset.duration());
            if duration <= 0.0 {
                return Err("The recording is empty".into());
            }
            let videos: Vec<Retained<AVAssetTrack>> = asset.tracksWithMediaType(vm).iter().collect();
            // the recorder writes the screen first, the camera second
            let screen = videos.first().ok_or("No picture in the recording")?.clone();
            let camera = if job.meta.recorded.camera { videos.get(1).cloned() } else { None };
            let ns = screen.naturalSize();
            let src = ((ns.width.round() as u32).max(2), (ns.height.round() as u32).max(2));

            let fps = job.meta.recorded.fps.clamp(1, 120);
            let comp = Compositor::new(job.meta.style, src, job.meta.ppp, job.meta.recorded.clone(), job.pictures.sprites(), job.lines.clone());
            let (ow, oh) = comp.out_size();

            // reading
            let reader = AVAssetReader::assetReaderWithAsset_error(&asset).map_err(|e| ns_err(&e))?;
            let fmt = NSNumber::new_u32(BGRA);
            let bgra = dict(&[("PixelFormatType", any(&*fmt))]);
            let video_out = |track: &AVAssetTrack| -> Result<Retained<AVAssetReaderTrackOutput>, String> {
                let o = AVAssetReaderTrackOutput::assetReaderTrackOutputWithTrack_outputSettings(track, Some(&bgra));
                o.setAlwaysCopiesSampleData(false);
                if !reader.canAddOutput(&o) {
                    return Err("The recording can't be read".into());
                }
                reader.addOutput(&o);
                Ok(o)
            };
            let mut scr = Frames { out: video_out(&screen)?, next: None, cur: Vec::new(), size: src, ended: false };
            let mut cam = match &camera {
                Some(t) => Some(Frames { out: video_out(t)?, next: None, cur: Vec::new(), size: (0, 0), ended: false }),
                None => None,
            };

            // writing
            let _ = std::fs::remove_file(&job.out);
            if let Some(dir) = job.out.parent() {
                let _ = std::fs::create_dir_all(dir);
            }
            let url = NSURL::fileURLWithPath(&NSString::from_str(&job.out.to_string_lossy()));
            let writer = AVAssetWriter::assetWriterWithURL_fileType_error(&url, AVFileTypeMPEG4.ok_or("mp4 isn't available")?).map_err(|e| ns_err(&e))?;
            let num = |v: f64| NSNumber::new_f64(v);
            let (bitrate, nfps, keys) = (num(timeline::bitrate(ow, oh, fps, RecQuality::High) as f64), num(fps as f64), num((fps * 2) as f64));
            let profile = NSString::from_str("H264_High_AutoLevel");
            let compression = dict(&[("AverageBitRate", any(&*bitrate)), ("ExpectedFrameRate", any(&*nfps)), ("MaxKeyFrameInterval", any(&*keys)), ("ProfileLevel", any(&*profile))]);
            let (w, h, codec) = (num(ow as f64), num(oh as f64), NSString::from_str("avc1"));
            let vset = dict(&[("AVVideoCodecKey", any(&*codec)), ("AVVideoWidthKey", any(&*w)), ("AVVideoHeightKey", any(&*h)), ("AVVideoCompressionPropertiesKey", any(&*compression))]);
            let video = AVAssetWriterInput::assetWriterInputWithMediaType_outputSettings(vm, Some(&vset));
            video.setExpectsMediaDataInRealTime(false);
            if !writer.canAddInput(&video) {
                return Err("This Mac can't encode video at that size".into());
            }
            writer.addInput(&video);
            let attrs = dict(&[("PixelFormatType", any(&*fmt)), ("Width", any(&*w)), ("Height", any(&*h))]);
            let adaptor = AVAssetWriterInputPixelBufferAdaptor::assetWriterInputPixelBufferAdaptorWithAssetWriterInput_sourcePixelBufferAttributes(&video, Some(&attrs));

            let mut lanes = Vec::new();
            for t in asset.tracksWithMediaType(am).iter() {
                let o = AVAssetReaderTrackOutput::assetReaderTrackOutputWithTrack_outputSettings(&t, None);
                let descs = t.formatDescriptions();
                let first = descs.firstObject();
                // the array holds CMFormatDescriptions
                let hint = first.as_ref().map(|d| &*(Retained::as_ptr(d) as *const CMFormatDescription));
                let input = AVAssetWriterInput::assetWriterInputWithMediaType_outputSettings_sourceFormatHint(am, None, hint);
                input.setExpectsMediaDataInRealTime(false);
                input.setMarksOutputTrackAsEnabled(t.isEnabled());
                if reader.canAddOutput(&o) && writer.canAddInput(&input) {
                    reader.addOutput(&o);
                    writer.addInput(&input);
                    lanes.push(Lane { out: o, input, done: false });
                }
            }

            if !reader.startReading() {
                return Err(reader.error().map(|e| ns_err(&e)).unwrap_or_else(|| "The recording can't be read".into()));
            }
            if !writer.startWriting() {
                reader.cancelReading();
                return Err(writer.error().map(|e| ns_err(&e)).unwrap_or_else(|| "Couldn't start the video file".into()));
            }
            writer.startSessionAtSourceTime(cm(0.0));
            let pool = adaptor.pixelBufferPool().ok_or("No frames to draw into")?;
            let pool_ptr = Retained::as_ptr(&pool) as *const c_void;

            let total = super::frame_count(duration, fps);
            let mut k = 0u64;
            let mut video_done = false;
            let mut scratch = Vec::new();
            let fail = |why: String| -> Result<(u32, u32), String> {
                reader.cancelReading();
                writer.cancelWriting();
                Err(why)
            };
            while !(video_done && lanes.iter().all(|l| l.done)) {
                if progress.cancel.load(Ordering::Relaxed) {
                    return fail("Cancelled".into());
                }
                if writer.status() == AVAssetWriterStatus::Failed {
                    return fail(writer.error().map(|e| ns_err(&e)).unwrap_or_else(|| "The video couldn't be written".into()));
                }
                if reader.status() == AVAssetReaderStatus::Failed {
                    return fail(reader.error().map(|e| ns_err(&e)).unwrap_or_else(|| "The recording couldn't be read".into()));
                }
                let mut moved = false;
                if !video_done && video.isReadyForMoreMediaData() {
                    moved = true;
                    if k >= total {
                        video.markAsFinished();
                        video_done = true;
                    } else {
                        let t = k as f64 / fps as f64;
                        scr.seek(t);
                        if let Some(c) = &mut cam {
                            c.seek(t);
                        }
                        let mut pb: *mut c_void = std::ptr::null_mut();
                        if CVPixelBufferPoolCreatePixelBuffer(std::ptr::null(), pool_ptr, &mut pb) != 0 || pb.is_null() {
                            return fail("Out of memory for frames".into());
                        }
                        if !scr.cur.is_empty() && CVPixelBufferLockBaseAddress(pb, 0) == 0 {
                            let (bw, bh, row) = (CVPixelBufferGetWidth(pb), CVPixelBufferGetHeight(pb), CVPixelBufferGetBytesPerRow(pb));
                            let base = CVPixelBufferGetBaseAddress(pb);
                            if !base.is_null() && (bw, bh) == (ow as usize, oh as usize) {
                                let f = FrameIn {
                                    screen: &scr.cur,
                                    screen_size: scr.size,
                                    camera: cam.as_ref().filter(|c| !c.cur.is_empty()).map(|c| (&c.cur[..], c.size)),
                                    t,
                                    source_t: t,
                                };
                                // locked for writing: ours until unlocked
                                let dst = std::slice::from_raw_parts_mut(base as *mut u8, row * bh);
                                if row == bw * 4 {
                                    comp.draw(&f, dst);
                                } else {
                                    scratch.resize(bw * bh * 4, 0);
                                    comp.draw(&f, &mut scratch);
                                    for y in 0..bh {
                                        dst[y * row..y * row + bw * 4].copy_from_slice(&scratch[y * bw * 4..(y + 1) * bw * 4]);
                                    }
                                }
                            }
                            CVPixelBufferUnlockBaseAddress(pb, 0);
                        }
                        let ok = adaptor.appendPixelBuffer_withPresentationTime(&*(pb as *const CVPixelBuffer), cm(t));
                        CFRelease(pb);
                        if !ok {
                            return fail(writer.error().map(|e| ns_err(&e)).unwrap_or_else(|| "A frame couldn't be written".into()));
                        }
                        k += 1;
                        progress.done.store((k * 1000 / total) as u32, Ordering::Relaxed);
                    }
                }
                for l in lanes.iter_mut().filter(|l| !l.done) {
                    while l.input.isReadyForMoreMediaData() {
                        moved = true;
                        match l.out.copyNextSampleBuffer() {
                            Some(sb) => {
                                if !l.input.appendSampleBuffer(&sb) {
                                    return fail(writer.error().map(|e| ns_err(&e)).unwrap_or_else(|| "The sound couldn't be written".into()));
                                }
                            }
                            None => {
                                l.input.markAsFinished();
                                l.done = true;
                                break;
                            }
                        }
                    }
                }
                if !moved {
                    std::thread::sleep(std::time::Duration::from_millis(2));
                }
            }
            reader.cancelReading();
            writer.endSessionAtSourceTime(cm(duration));
            let (tx, rx) = std::sync::mpsc::channel::<()>();
            let tx = std::sync::Mutex::new(Some(tx));
            let block = RcBlock::new(move || {
                if let Some(tx) = tx.lock().ok().and_then(|mut t| t.take()) {
                    let _ = tx.send(());
                }
            });
            writer.finishWritingWithCompletionHandler(&block);
            let _ = rx.recv_timeout(std::time::Duration::from_secs(120));
            if writer.status() == AVAssetWriterStatus::Completed {
                Ok((ow, oh))
            } else {
                Err(writer.error().map(|e| ns_err(&e)).unwrap_or_else(|| "The video couldn't be finished".into()))
            }
        }
    }

    /// One frame of `job` at `t`, RGBA.
    #[allow(deprecated)]
    pub fn still(job: &Job, t: f64) -> Result<image::RgbaImage, String> {
        unsafe {
            let asset = AVURLAsset::URLAssetWithURL_options(&NSURL::fileURLWithPath(&NSString::from_str(&job.raw.to_string_lossy())), None);
            let vm = AVMediaTypeVideo.ok_or("no video")?;
            let videos: Vec<Retained<AVAssetTrack>> = asset.tracksWithMediaType(vm).iter().collect();
            let screen = videos.first().ok_or("No picture in the recording")?.clone();
            let camera = if job.meta.recorded.camera { videos.get(1).cloned() } else { None };
            let ns = screen.naturalSize();
            let src = ((ns.width.round() as u32).max(2), (ns.height.round() as u32).max(2));
            let comp = Compositor::new(job.meta.style, src, job.meta.ppp, job.meta.recorded.clone(), job.pictures.sprites(), job.lines.clone());
            let (ow, oh) = comp.out_size();
            let fmt = NSNumber::new_u32(BGRA);
            let bgra = dict(&[("PixelFormatType", any(&*fmt))]);
            // a frame or two before t, so a still screen still has one
            let from = (t - 1.0).max(0.0);
            let grab = |track: &AVAssetTrack| -> Option<(Vec<u8>, (u32, u32))> {
                let reader = AVAssetReader::assetReaderWithAsset_error(&asset).ok()?;
                reader.setTimeRange(objc2_core_media::CMTimeRange { start: cm(from), duration: cm(t - from + 0.05) });
                let o = AVAssetReaderTrackOutput::assetReaderTrackOutputWithTrack_outputSettings(track, Some(&bgra));
                if !reader.canAddOutput(&o) {
                    return None;
                }
                reader.addOutput(&o);
                if !reader.startReading() {
                    return None;
                }
                let mut f = Frames { out: o, next: None, cur: Vec::new(), size: (0, 0), ended: false };
                f.seek(t);
                reader.cancelReading();
                (!f.cur.is_empty()).then(|| (f.cur, f.size))
            };
            let (scr, size) = grab(&screen).ok_or("No frame there")?;
            let cam = camera.as_ref().and_then(|c| grab(c));
            let mut out = vec![0u8; (ow * oh * 4) as usize];
            comp.draw(&FrameIn { screen: &scr, screen_size: size, camera: cam.as_ref().map(|(b, s)| (&b[..], *s)), t, source_t: t }, &mut out);
            for p in out.chunks_exact_mut(4) {
                p.swap(0, 2);
            }
            image::RgbaImage::from_raw(ow, oh, out).ok_or_else(|| "No frame".into())
        }
    }

    /// The system's arrow, I-beam and hand, drawn at 2 px per point.
    /// Main thread.
    pub fn cursor_pictures() -> Pictures {
        use objc2_app_kit::{NSBitmapImageRep, NSCompositingOperation, NSCursor, NSDeviceRGBColorSpace, NSGraphicsContext};
        use objc2_foundation::{NSPoint, NSRect, NSSize};
        if MainThreadMarker::new().is_none() {
            return Pictures::default();
        }
        let draw = |c: &NSCursor| -> Option<(image::RgbaImage, (f32, f32))> {
            let img = c.image();
            let size = img.size();
            let (w, h) = ((size.width * 2.0).round() as isize, (size.height * 2.0).round() as isize);
            if w <= 0 || h <= 0 || w > 512 || h > 512 {
                return None;
            }
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
            img.drawInRect_fromRect_operation_fraction(NSRect::new(NSPoint::new(0.0, 0.0), NSSize::new(w as f64, h as f64)), NSRect::new(NSPoint::new(0.0, 0.0), NSSize::new(0.0, 0.0)), NSCompositingOperation::SourceOver, 1.0);
            ctx.flushGraphics();
            NSGraphicsContext::setCurrentContext(previous.as_deref());
            let data = rep.bitmapData();
            if data.is_null() {
                return None;
            }
            // premultiplied → straight
            let mut px = unsafe { std::slice::from_raw_parts(data, (w * h * 4) as usize) }.to_vec();
            for p in px.chunks_exact_mut(4) {
                let a = p[3] as u32;
                if a > 0 && a < 255 {
                    for c in &mut p[..3] {
                        *c = ((*c as u32 * 255 + a / 2) / a).min(255) as u8;
                    }
                }
            }
            let hot = c.hotSpot();
            Some((image::RgbaImage::from_raw(w as u32, h as u32, px)?, ((hot.x * 2.0) as f32, (hot.y * 2.0) as f32)))
        };
        let mut out = Vec::new();
        for (shape, c) in [(Shape::Arrow, NSCursor::arrowCursor()), (Shape::Text, NSCursor::IBeamCursor()), (Shape::Hand, NSCursor::pointingHandCursor())] {
            if let Some((img, hot)) = draw(&c) {
                out.push((shape, img, hot));
            }
        }
        Pictures(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frames_cover_the_length() {
        assert_eq!(frame_count(1.0, 60), 60);
        assert_eq!(frame_count(1.001, 60), 61);
        assert_eq!(frame_count(0.0, 30), 1);
    }

    #[test]
    fn projects_are_found_by_their_file_and_pruned_when_it_goes() {
        let root = std::env::temp_dir().join(format!("cmdmeow-proj-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let out = root.join("out").join("Recording 1.mp4");
        std::fs::create_dir_all(out.parent().unwrap()).unwrap();
        std::fs::write(&out, b"x").unwrap();
        let p = Project::for_output(&root.join("projects"), &out);
        assert!(p.save(&Meta { out: out.clone(), ppp: 2.0, ..Default::default() }));
        std::fs::write(p.raw(), b"raw").unwrap();
        let (found, meta) = Project::find(&root.join("projects"), &out).expect("found");
        assert_eq!(found.dir, p.dir);
        assert_eq!(meta.ppp, 2.0);
        assert!(Project::find(&root.join("projects"), &root.join("other.mp4")).is_none());
        assert_eq!(prune(&root.join("projects")), 0);
        std::fs::remove_file(&out).unwrap();
        assert_eq!(prune(&root.join("projects")), 1);
        assert!(!p.dir.exists());
        let _ = std::fs::remove_dir_all(&root);
    }
}
