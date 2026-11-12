// Edit mode for a recording: split at the playhead, remove or restore pieces,
// cut before/after the playhead, undo; playback skips removed pieces; save
// the result as a new MP4 made in the browser.
import { editedName, emptyTimeline, fmtFine, History, keptDuration, keptRanges, playableAt, segmentAt, segmentsOf, splitAt, toggleSegment, trimEnd, trimStart } from './core.mjs';
import { canExport, exportCut } from './export.mjs';

export function createVideoEditor(video, source, title, note) {
  const $ = (id) => document.getElementById(id);
  const track = $('vt-track'), segsEl = $('vt-segs'), head = $('vt-head');
  const hist = new History(emptyTimeline());
  let on = false, job = null, raf = 0;
  const dur = () => video.duration || 0;
  const ranges = () => keptRanges(dur(), hist.now);

  function change(next) {
    if (next !== hist.now) hist.push(next);
    render();
  }

  function render() {
    const d = dur();
    segsEl.textContent = '';
    if (!d) return;
    segmentsOf(d, hist.now).forEach((s, i) => {
      const el = document.createElement('div');
      el.className = 'vt-seg' + (s.off ? ' off' : '');
      el.style.left = (s.a / d * 100) + '%';
      el.style.width = ((s.b - s.a) / d * 100) + '%';
      el.dataset.i = i;
      segsEl.appendChild(el);
    });
    $('vt-len').textContent = fmtFine(keptDuration(ranges()));
    $('vt-undo').disabled = !hist.canUndo;
    $('vt-redo').disabled = !hist.canRedo;
    const cur = segmentsOf(d, hist.now)[segmentAt(d, hist.now, video.currentTime)];
    $('vt-del').classList.toggle('restore', !!(cur && cur.off));
    $('vt-export').disabled = !!job || !canExport();
    moveHead();
  }
  function moveHead() {
    const d = dur();
    head.style.left = (d ? video.currentTime / d * 100 : 0) + '%';
  }

  // Skip removed pieces while playing.
  function tick() {
    if (on && !video.paused) {
      const t = playableAt(ranges(), video.currentTime);
      if (t === null) {
        video.pause();
        const r = ranges(); video.currentTime = r.length ? r[r.length - 1][1] : 0;
      } else if (t !== video.currentTime && t - video.currentTime > 0.03) video.currentTime = t;
    }
    moveHead();
    raf = on ? requestAnimationFrame(tick) : 0;
  }
  video.addEventListener('play', () => {
    if (!on) return;
    const t = playableAt(ranges(), video.currentTime);
    if (t === null) { const r = ranges(); if (r.length) video.currentTime = r[0][0]; }
  });
  video.addEventListener('seeked', () => on && render());

  // ---- Timeline pointer: click or drag to seek ---------------------------------
  let dragging = false;
  function seekTo(e) {
    const r = track.getBoundingClientRect();
    video.currentTime = Math.max(0, Math.min(1, (e.clientX - r.left) / r.width)) * dur();
    moveHead();
  }
  track.addEventListener('pointerdown', (e) => { dragging = true; track.setPointerCapture(e.pointerId); seekTo(e); });
  track.addEventListener('pointermove', (e) => dragging && seekTo(e));
  track.addEventListener('pointerup', () => { dragging = false; render(); });

  // ---- Actions -------------------------------------------------------------------
  const split = () => change(splitAt(dur(), hist.now, video.currentTime));
  const del = () => change(toggleSegment(dur(), hist.now, segmentAt(dur(), hist.now, video.currentTime)));
  const cutIn = () => change(trimStart(dur(), hist.now, video.currentTime));
  const cutOut = () => change(trimEnd(dur(), hist.now, video.currentTime));
  $('vt-split').onclick = split;
  $('vt-del').onclick = del;
  $('vt-in').onclick = cutIn;
  $('vt-out').onclick = cutOut;
  $('vt-undo').onclick = () => { hist.undo(); render(); };
  $('vt-redo').onclick = () => { hist.redo(); render(); };
  $('vt-export').onclick = save;
  $('vt-cancel').onclick = () => job && job.cancel();
  if (!canExport()) $('vt-export').title = "This browser can't make video files";

  function onKey(e) {
    const tag = (e.target.tagName || '').toLowerCase();
    if (tag === 'input' || tag === 'textarea') return;
    const k = e.key.toLowerCase();
    if ((e.metaKey || e.ctrlKey) && k === 'z') { e.preventDefault(); (e.shiftKey ? $('vt-redo') : $('vt-undo')).click(); return; }
    if ((e.metaKey || e.ctrlKey) && k === 's') { e.preventDefault(); save(); return; }
    if (e.metaKey || e.ctrlKey || e.altKey) return;
    if (k === 's') split();
    else if (k === 'i') cutIn();
    else if (k === 'o') cutOut();
    else if (k === 'delete' || k === 'backspace' || k === 'x') del();
    else return;
    e.preventDefault();
  }

  // ---- Save ------------------------------------------------------------------
  async function save() {
    if (job || !canExport()) return;
    video.pause();
    const prog = $('vt-prog'), fill = $('vt-fill'), pct = $('vt-pct');
    prog.hidden = false; fill.style.width = '0%'; pct.textContent = '0%';
    job = exportCut(source(), ranges(), (p) => { fill.style.width = (p * 100) + '%'; pct.textContent = Math.round(p * 100) + '%'; });
    render();
    try {
      const blob = await job.promise;
      const a = document.createElement('a');
      a.href = URL.createObjectURL(blob);
      a.download = editedName(title, 'mp4');
      document.body.appendChild(a); a.click(); a.remove();
      setTimeout(() => URL.revokeObjectURL(a.href), 60000);
      window.__lastExport = blob; // read by the smoke test
    } catch (e) {
      if (e && e.name !== 'AbortError') note(e.message || String(e), 6000);
    } finally {
      job = null; prog.hidden = true; render();
    }
  }

  return {
    show() {
      on = true; $('vt').hidden = false;
      document.addEventListener('keydown', onKey);
      render();
      if (!raf) raf = requestAnimationFrame(tick);
      video.addEventListener('loadedmetadata', render);
    },
    hide() {
      on = false; $('vt').hidden = true;
      document.removeEventListener('keydown', onKey);
      if (job) job.cancel();
    },
    get dirty() { return hist.canUndo; },
  };
}
