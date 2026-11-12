// Pure logic for the share page: links, file kinds, timeline cuts, drawing
// geometry. No DOM here, so node --test can run it (tests/site/core.test.mjs).

/** An http(s) URL from a string, or null. */
export function safeUrl(s) {
  if (!s) return null;
  try {
    const u = new URL(String(s).trim());
    return u.protocol === 'https:' || u.protocol === 'http:' ? u : null;
  } catch (e) {
    return null;
  }
}

/** A query value, falling back to the hash (#u=…), so a link survives
 *  tools that strip query strings. */
export function param(search, hash, name) {
  const q = new URLSearchParams(search || '').get(name);
  if (q) return q;
  return new URLSearchParams(String(hash || '').replace(/^#/, '')).get(name);
}

/** What the page was asked to show: `u` (old player links used `v`) and `t`. */
export function readSource(search, hash) {
  const raw = param(search, hash, 'u') || param(search, hash, 'v');
  const url = safeUrl(raw);
  return { raw, url, title: param(search, hash, 't') || (url ? fileName(url) : '') };
}

export function fileName(u) {
  let last = '';
  try { last = decodeURIComponent(u.pathname.split('/').pop() || ''); } catch (e) { last = u.pathname.split('/').pop() || ''; }
  return last || u.hostname;
}

const IMAGE = ['png', 'jpg', 'jpeg', 'gif', 'webp', 'avif', 'bmp'];
const VIDEO = ['mp4', 'm4v', 'mov', 'webm'];

/** 'image', 'video' or null, from a content type (wins) or the name's extension. */
export function kindOf(name, contentType) {
  const ct = String(contentType || '').toLowerCase().split(';')[0].trim();
  if (ct.startsWith('image/')) return 'image';
  if (ct.startsWith('video/')) return 'video';
  const m = /\.([a-z0-9]+)$/i.exec(String(name || '').split(/[?#]/)[0]);
  const ext = m ? m[1].toLowerCase() : '';
  if (IMAGE.includes(ext)) return 'image';
  if (VIDEO.includes(ext)) return 'video';
  return null;
}

/** "1:05", "1:02:03". */
export function fmt(t) {
  if (!isFinite(t) || t < 0) t = 0;
  t = Math.floor(t);
  const h = Math.floor(t / 3600), m = Math.floor(t / 60) % 60, s = t % 60;
  return (h ? h + ':' + String(m).padStart(2, '0') : m) + ':' + String(s).padStart(2, '0');
}

/** "0:12.4": for the timeline, where tenths matter. */
export function fmtFine(t) {
  if (!isFinite(t) || t < 0) t = 0;
  const tenths = Math.floor(t * 10 + 1e-6);
  return fmt(Math.floor(tenths / 10)) + '.' + (tenths % 10);
}

/** The name an edited copy is saved under. */
export function editedName(title, ext) {
  const base = String(title || 'shot').replace(/\.[a-z0-9]{2,4}$/i, '').replace(/[\\/:*?"<>|]+/g, ' ').trim() || 'shot';
  return base + ' edited.' + ext;
}

// ---- Timeline ---------------------------------------------------------------
// A timeline is { points: sorted split times, off: sorted indexes of removed
// segments }. Segment i runs from point i-1 (or 0) to point i (or the end).

export const MIN_PIECE = 0.1; // seconds; no split closer than this to another

export function emptyTimeline() {
  return { points: [], off: [] };
}

export function segmentsOf(dur, tl) {
  const edges = [0, ...tl.points.filter((p) => p > 0 && p < dur), dur];
  const out = [];
  for (let i = 0; i + 1 < edges.length; i++) out.push({ a: edges[i], b: edges[i + 1], off: tl.off.includes(i) });
  return out;
}

/** The segment index that holds time t. */
export function segmentAt(dur, tl, t) {
  const segs = segmentsOf(dur, tl);
  for (let i = 0; i < segs.length; i++) if (t < segs[i].b) return i;
  return segs.length - 1;
}

/** Split at t. Removed segments stay removed (both halves). */
export function splitAt(dur, tl, t) {
  if (!(t > MIN_PIECE && t < dur - MIN_PIECE)) return tl;
  if (tl.points.some((p) => Math.abs(p - t) < MIN_PIECE)) return tl;
  const i = segmentAt(dur, tl, t);
  const points = [...tl.points, t].sort((x, y) => x - y);
  const off = [];
  for (const k of tl.off) {
    if (k < i) off.push(k);
    else if (k === i) off.push(k, k + 1);
    else off.push(k + 1);
  }
  return { points, off };
}

/** Remove or bring back segment i. Never removes the last kept one. */
export function toggleSegment(dur, tl, i) {
  const n = segmentsOf(dur, tl).length;
  if (i < 0 || i >= n) return tl;
  if (tl.off.includes(i)) return { points: tl.points, off: tl.off.filter((k) => k !== i) };
  if (tl.off.length + 1 >= n) return tl;
  return { points: tl.points, off: [...tl.off, i].sort((x, y) => x - y) };
}

/** Drop everything before t (trim the start). */
export function trimStart(dur, tl, t) {
  let next = splitAt(dur, tl, t);
  const segs = segmentsOf(dur, next);
  for (let i = 0; i < segs.length; i++) {
    if (segs[i].b <= t + 1e-9 && !segs[i].off) next = toggleSegment(dur, next, i);
  }
  return next;
}

/** Drop everything after t (trim the end). */
export function trimEnd(dur, tl, t) {
  let next = splitAt(dur, tl, t);
  const segs = segmentsOf(dur, next);
  for (let i = segs.length - 1; i >= 0; i--) {
    if (segs[i].a >= t - 1e-9 && !segs[i].off) next = toggleSegment(dur, next, i);
  }
  return next;
}

/** The kept parts as [a, b] ranges, neighbours merged. */
export function keptRanges(dur, tl) {
  const out = [];
  for (const s of segmentsOf(dur, tl)) {
    if (s.off) continue;
    const last = out[out.length - 1];
    if (last && Math.abs(last[1] - s.a) < 1e-9) last[1] = s.b;
    else out.push([s.a, s.b]);
  }
  return out;
}

export function keptDuration(ranges) {
  return ranges.reduce((sum, [a, b]) => sum + (b - a), 0);
}

/** Where playback should be for time t: t itself when kept, else the start
 *  of the next kept range, else null (past the last one). */
export function playableAt(ranges, t) {
  for (const [a, b] of ranges) {
    if (t < a) return a;
    if (t < b - 0.02) return t;
  }
  return null;
}

/** Time in the source → time in the edited copy. */
export function toOutputTime(ranges, t) {
  let acc = 0;
  for (const [a, b] of ranges) {
    if (t < a) return acc;
    if (t <= b) return acc + (t - a);
    acc += b - a;
  }
  return acc;
}

// ---- Drawing geometry ---------------------------------------------------------

/** A rect with positive size from two corners. */
export function normRect(x1, y1, x2, y2) {
  return { x: Math.min(x1, x2), y: Math.min(y1, y2), w: Math.abs(x2 - x1), h: Math.abs(y2 - y1) };
}

/** A rect kept inside 0..W, 0..H. */
export function clampRect(r, W, H) {
  const x = Math.max(0, Math.min(W, r.x)), y = Math.max(0, Math.min(H, r.y));
  return { x, y, w: Math.max(0, Math.min(W, r.x + r.w) - x), h: Math.max(0, Math.min(H, r.y + r.h) - y) };
}

/** The two back corners of an arrow head at (x2, y2) for a line of width w. */
export function arrowHead(x1, y1, x2, y2, w) {
  const len = Math.max(10, w * 4.5);
  const ang = Math.atan2(y2 - y1, x2 - x1);
  const spread = Math.PI / 7;
  return [
    [x2 - len * Math.cos(ang - spread), y2 - len * Math.sin(ang - spread)],
    [x2 - len * Math.cos(ang + spread), y2 - len * Math.sin(ang + spread)],
  ];
}

/** Scale that fits an iw×ih picture in cw×ch, never above `max`. */
export function fitScale(iw, ih, cw, ch, max = 1) {
  if (!(iw > 0 && ih > 0 && cw > 0 && ch > 0)) return 1;
  return Math.min(max, cw / iw, ch / ih);
}

/** Line widths for S, M, L relative to the picture, so marks read the same
 *  on a small crop and a 5K screenshot. */
export function strokeWidths(W, H) {
  const base = Math.max(2, Math.round(Math.max(W, H) / 400));
  return [base, base * 2, base * 4];
}

/** A history of states with undo and redo. */
export class History {
  constructor(first, limit = 200) {
    this.items = [first];
    this.at = 0;
    this.limit = limit;
  }
  get now() { return this.items[this.at]; }
  push(state) {
    this.items = this.items.slice(0, this.at + 1);
    this.items.push(state);
    if (this.items.length > this.limit) this.items.shift();
    this.at = this.items.length - 1;
    return state;
  }
  get canUndo() { return this.at > 0; }
  get canRedo() { return this.at < this.items.length - 1; }
  undo() { if (this.canUndo) this.at--; return this.now; }
  redo() { if (this.canRedo) this.at++; return this.now; }
}
