// Save an edited copy of a video: keep only `ranges` of the source and write
// a new MP4 in the browser (WebCodecs, through the Mediabunny library).
// Nothing leaves the computer.

const LIB = 'https://cdn.jsdelivr.net/npm/mediabunny@1.63.0/dist/bundles/mediabunny.min.mjs';

let lib = null;
function load() {
  if (!lib) lib = import(LIB);
  return lib;
}

/** Can this browser write video at all? */
export function canExport() {
  return typeof window !== 'undefined' && 'VideoEncoder' in window && 'VideoDecoder' in window;
}

/**
 * src: a Blob or an http(s) URL string (the storage must allow CORS).
 * ranges: [[a, b], ...] seconds to keep, sorted, not overlapping.
 * onProgress(0..1). Returns { promise: Promise<Blob>, cancel() }.
 */
export function exportCut(src, ranges, onProgress) {
  let output = null, stopped = false;
  const promise = (async () => {
    const mb = await load();
    const input = new mb.Input({
      source: src instanceof Blob ? new mb.BlobSource(src) : new mb.UrlSource(src),
      formats: mb.ALL_FORMATS,
    });
    const vt = await input.getPrimaryVideoTrack();
    if (!vt) throw new Error('No video track in this file');
    const at = await input.getPrimaryAudioTrack();
    if (!(await vt.canDecode())) throw new Error("This browser can't read this video");

    const format = new mb.Mp4OutputFormat({ fastStart: 'in-memory' });
    const vcodec = await mb.getFirstEncodableVideoCodec(
      format.getSupportedVideoCodecs().filter((c) => c !== 'prores'),
      { width: vt.codedWidth, height: vt.codedHeight },
    );
    if (!vcodec) throw new Error("This browser can't write video");

    output = new mb.Output({ format, target: new mb.BufferTarget() });
    const vsrc = new mb.VideoSampleSource({ codec: vcodec, quality: mb.QUALITY_HIGH });
    output.addVideoTrack(vsrc, { rotation: vt.rotation });

    let asrc = null, asink = null;
    if (at && (await at.canDecode())) {
      const acodec = await mb.getFirstEncodableAudioCodec(format.getSupportedAudioCodecs(), {
        numberOfChannels: at.numberOfChannels, sampleRate: at.sampleRate,
      });
      if (acodec) {
        asrc = new mb.AudioSampleSource({ codec: acodec, quality: mb.QUALITY_HIGH });
        output.addAudioTrack(asrc);
        asink = new mb.AudioSampleSink(at);
      }
    }
    await output.start();

    const total = ranges.reduce((s, [a, b]) => s + (b - a), 0) || 1;
    const vsink = new mb.VideoSampleSink(vt);

    const video = (async () => {
      let offset = 0;
      for (const [a, b] of ranges) {
        for await (const s of vsink.samples(a, b)) {
          if (stopped) { s.close(); return; }
          const t = offset + Math.max(0, s.timestamp - a);
          s.setTimestamp(t);
          await vsrc.add(s);
          s.close();
          if (onProgress) onProgress(Math.min(0.99, t / total));
        }
        offset += b - a;
      }
      vsrc.close();
    })();

    const audio = (async () => {
      if (!asink) return;
      let offset = 0;
      for (const [a, b] of ranges) {
        for await (let s of asink.samples(a, b)) {
          if (stopped) { s.close(); return; }
          const sr = s.sampleRate, n = s.numberOfFrames;
          const from = Math.max(0, Math.round((a - s.timestamp) * sr));
          const to = Math.min(n, Math.round((b - s.timestamp) * sr));
          if (to <= from) { s.close(); continue; }
          if (from > 0 || to < n) {
            const t = s.trim(from, to);
            s.close();
            s = t;
          }
          s.setTimestamp(offset + Math.max(0, s.timestamp - a));
          await asrc.add(s);
          s.close();
        }
        offset += b - a;
      }
      asrc.close();
    })();

    await Promise.all([video, audio]);
    if (stopped) throw new DOMException('Stopped', 'AbortError');
    await output.finalize();
    onProgress && onProgress(1);
    return new Blob([output.target.buffer], { type: 'video/mp4' });
  })();

  return {
    promise,
    cancel() {
      stopped = true;
      if (output && output.state !== 'finalized' && output.state !== 'canceled') output.cancel().catch(() => {});
    },
  };
}
