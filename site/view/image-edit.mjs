// Edit mode for a shot: draw on a local copy (arrow, line, box, pen,
// highlighter, text), crop, undo; save as PNG or copy. Marks are kept in
// picture pixels, so the saved copy is full size whatever the zoom.
import { arrowHead, clampRect, editedName, fitScale, History, normRect, strokeWidths } from './core.mjs';

const COLORS = ['#FF453A', '#FF9F0A', '#FFD60A', '#30D158', '#0A84FF', '#FFFFFF', '#1C1C1E'];
const KEYS = { a: 'arrow', l: 'line', r: 'rect', p: 'pen', h: 'mark', t: 'text', c: 'crop' };

export function createImageEditor(bitmap, title, note) {
  const $ = (id) => document.getElementById(id);
  const stage = $('ie-stage'), canvas = $('ie-canvas'), ctx = canvas.getContext('2d'), textIn = $('ie-text');
  const W = bitmap.width, H = bitmap.height;
  const widths = strokeWidths(W, H);
  const hist = new History({ crop: null, shapes: [] });
  let tool = 'arrow', color = COLORS[0], size = 1;
  let draft = null; // the shape being drawn
  let view = { s: 1, ox: 0, oy: 0 }; // picture px → canvas css px
  let textAt = null;

  // ---- Toolbar ---------------------------------------------------------------
  const colorBox = $('ie-colors');
  colorBox.textContent = '';
  for (const c of COLORS) {
    const b = document.createElement('button');
    b.type = 'button'; b.className = 'tbb'; b.dataset.color = c; b.setAttribute('aria-label', c);
    b.innerHTML = '<span class="sw" style="--c:' + c + '"></span>';
    colorBox.appendChild(b);
  }
  function syncBar() {
    document.querySelectorAll('#ie-tools [data-tool]').forEach((b) => b.setAttribute('aria-pressed', String(b.dataset.tool === tool)));
    document.querySelectorAll('#ie-colors [data-color]').forEach((b) => b.setAttribute('aria-pressed', String(b.dataset.color === color)));
    document.querySelectorAll('#ie-sizes [data-size]').forEach((b) => b.setAttribute('aria-pressed', String(+b.dataset.size === size)));
    $('ie-undo').disabled = !hist.canUndo;
    $('ie-redo').disabled = !hist.canRedo;
    stage.dataset.tool = tool;
  }
  function setTool(t) { commitText(); tool = t; syncBar(); layout(); }
  const onBar = (e) => {
    const b = e.target.closest('button'); if (!b) return;
    if (b.dataset.tool) setTool(b.dataset.tool);
    else if (b.dataset.color) { color = b.dataset.color; syncBar(); }
    else if (b.dataset.size) { size = +b.dataset.size; syncBar(); }
  };
  $('ie-tools').addEventListener('click', onBar);
  $('ie-undo').onclick = () => { commitText(); hist.undo(); syncBar(); layout(); };
  $('ie-redo').onclick = () => { commitText(); hist.redo(); syncBar(); layout(); };
  $('ie-save').onclick = save;
  $('ie-copy').onclick = copy;

  // ---- Geometry --------------------------------------------------------------
  // While cropping the whole picture shows, so the crop can be redrawn.
  function frame() {
    const c = hist.now.crop;
    return tool === 'crop' || !c ? { x: 0, y: 0, w: W, h: H } : c;
  }
  function layout() {
    const r = stage.getBoundingClientRect();
    const f = frame();
    const dpr = window.devicePixelRatio || 1;
    const s = fitScale(f.w, f.h, r.width, r.height, 1);
    const cw = Math.max(1, Math.round(f.w * s)), ch = Math.max(1, Math.round(f.h * s));
    canvas.style.width = cw + 'px'; canvas.style.height = ch + 'px';
    canvas.width = Math.round(cw * dpr); canvas.height = Math.round(ch * dpr);
    view = { s, ox: f.x, oy: f.y, dpr };
    draw();
  }
  function toPic(e) {
    const r = canvas.getBoundingClientRect();
    return [(e.clientX - r.left) / view.s + view.ox, (e.clientY - r.top) / view.s + view.oy];
  }

  // ---- Drawing ---------------------------------------------------------------
  function paint(g, sh) {
    g.save();
    g.strokeStyle = g.fillStyle = sh.c;
    g.lineWidth = sh.w; g.lineCap = 'round'; g.lineJoin = 'round';
    if (sh.t === 'line' || sh.t === 'arrow') {
      const [[x1, y1], [x2, y2]] = sh.p;
      g.beginPath(); g.moveTo(x1, y1); g.lineTo(x2, y2); g.stroke();
      if (sh.t === 'arrow' && Math.hypot(x2 - x1, y2 - y1) > 2) {
        const [a, b] = arrowHead(x1, y1, x2, y2, sh.w);
        g.beginPath(); g.moveTo(x2, y2); g.lineTo(a[0], a[1]); g.lineTo(b[0], b[1]); g.closePath(); g.fill(); g.stroke();
      }
    } else if (sh.t === 'rect') {
      const r = normRect(sh.p[0][0], sh.p[0][1], sh.p[1][0], sh.p[1][1]);
      g.beginPath();
      if (g.roundRect) g.roundRect(r.x, r.y, r.w, r.h, Math.min(sh.w * 1.5, r.w / 2, r.h / 2)); else g.rect(r.x, r.y, r.w, r.h);
      g.stroke();
    } else if (sh.t === 'pen' || sh.t === 'mark') {
      if (sh.t === 'mark') { g.globalAlpha = 0.4; g.lineWidth = sh.w * 4; g.lineCap = 'butt'; }
      g.beginPath();
      sh.p.forEach(([x, y], i) => (i ? g.lineTo(x, y) : g.moveTo(x, y)));
      if (sh.p.length === 1) g.lineTo(sh.p[0][0] + 0.1, sh.p[0][1]);
      g.stroke();
    } else if (sh.t === 'text') {
      g.font = '600 ' + sh.size + 'px -apple-system, BlinkMacSystemFont, "Helvetica Neue", Arial, sans-serif';
      g.textBaseline = 'top';
      g.lineWidth = Math.max(2, sh.size / 8);
      g.strokeStyle = sh.c === '#1C1C1E' ? 'rgba(255,255,255,.9)' : 'rgba(0,0,0,.55)';
      g.strokeText(sh.text, sh.p[0][0], sh.p[0][1]);
      g.fillText(sh.text, sh.p[0][0], sh.p[0][1]);
    }
    g.restore();
  }
  function paintAll(g, shapes) {
    g.drawImage(bitmap, 0, 0);
    for (const sh of shapes) paint(g, sh);
  }
  function draw() {
    const g = ctx, k = view.s * view.dpr;
    g.setTransform(1, 0, 0, 1, 0, 0);
    g.clearRect(0, 0, canvas.width, canvas.height);
    g.setTransform(k, 0, 0, k, -view.ox * k, -view.oy * k);
    g.imageSmoothingQuality = 'high';
    paintAll(g, draft && draft.t !== 'crop' ? [...hist.now.shapes, draft] : hist.now.shapes);
    if (tool === 'crop') {
      const c = draft && draft.t === 'crop' ? clampRect(normRect(draft.p[0][0], draft.p[0][1], draft.p[1][0], draft.p[1][1]), W, H) : hist.now.crop;
      if (c) {
        g.fillStyle = 'rgba(0,0,0,.55)';
        g.beginPath(); g.rect(0, 0, W, H); g.rect(c.x, c.y, c.w, c.h); g.fill('evenodd');
        g.strokeStyle = '#fff'; g.lineWidth = 1.5 / view.s; g.setLineDash([6 / view.s, 4 / view.s]);
        g.strokeRect(c.x, c.y, c.w, c.h);
      }
    }
  }

  // ---- Pointer ---------------------------------------------------------------
  canvas.addEventListener('pointerdown', (e) => {
    if (e.button !== 0) return;
    const p = toPic(e);
    if (tool === 'text') { e.preventDefault(); startText(p); return; }
    commitText();
    canvas.setPointerCapture(e.pointerId);
    draft = { t: tool, c: color, w: widths[size], p: tool === 'pen' || tool === 'mark' ? [p] : [p, p] };
    draw();
  });
  canvas.addEventListener('pointermove', (e) => {
    if (!draft) return;
    const p = toPic(e);
    if (draft.t === 'pen' || draft.t === 'mark') {
      const last = draft.p[draft.p.length - 1];
      if (Math.hypot(p[0] - last[0], p[1] - last[1]) * view.s >= 1.5) draft.p.push(p);
    } else {
      // Shift: straight lines snap to 45°, boxes become squares.
      if (e.shiftKey && (draft.t === 'line' || draft.t === 'arrow')) {
        const [x0, y0] = draft.p[0], a = Math.round(Math.atan2(p[1] - y0, p[0] - x0) / (Math.PI / 4)) * (Math.PI / 4), d = Math.hypot(p[0] - x0, p[1] - y0);
        draft.p[1] = [x0 + d * Math.cos(a), y0 + d * Math.sin(a)];
      } else if (e.shiftKey && draft.t === 'rect') {
        const [x0, y0] = draft.p[0], d = Math.max(Math.abs(p[0] - x0), Math.abs(p[1] - y0));
        draft.p[1] = [x0 + Math.sign(p[0] - x0) * d, y0 + Math.sign(p[1] - y0) * d];
      } else draft.p[1] = p;
    }
    draw();
  });
  function finish() {
    if (!draft) return;
    const d = draft; draft = null;
    const span = d.p.length > 1 ? Math.hypot(d.p[d.p.length - 1][0] - d.p[0][0], d.p[d.p.length - 1][1] - d.p[0][1]) * view.s : 0;
    if (d.t === 'crop') {
      const c = clampRect(normRect(d.p[0][0], d.p[0][1], d.p[1][0], d.p[1][1]), W, H);
      if (c.w * view.s >= 8 && c.h * view.s >= 8) {
        hist.push({ crop: { x: Math.round(c.x), y: Math.round(c.y), w: Math.round(c.w), h: Math.round(c.h) }, shapes: hist.now.shapes });
        tool = 'arrow';
      }
    } else if (d.t === 'pen' || d.t === 'mark' || span >= 3) {
      hist.push({ crop: hist.now.crop, shapes: [...hist.now.shapes, d] });
    }
    syncBar(); layout();
  }
  canvas.addEventListener('pointerup', finish);
  canvas.addEventListener('pointercancel', () => { draft = null; draw(); });

  // ---- Text ------------------------------------------------------------------
  function startText(p) {
    commitText();
    textAt = { p, c: color, size: widths[size] * 6 + 8 };
    const r = canvas.getBoundingClientRect(), sr = stage.getBoundingClientRect();
    textIn.hidden = false;
    textIn.value = '';
    textIn.style.left = (r.left - sr.left + (p[0] - view.ox) * view.s) + 'px';
    textIn.style.top = (r.top - sr.top + (p[1] - view.oy) * view.s) + 'px';
    textIn.style.fontSize = Math.max(12, textAt.size * view.s) + 'px';
    textIn.style.color = color;
    textIn.focus();
  }
  function commitText() {
    if (!textAt) return;
    const text = textIn.value.trim();
    if (text) hist.push({ crop: hist.now.crop, shapes: [...hist.now.shapes, { t: 'text', c: textAt.c, size: textAt.size, text, p: [textAt.p] }] });
    textAt = null; textIn.hidden = true;
    syncBar(); draw();
  }
  textIn.addEventListener('keydown', (e) => {
    if (e.key === 'Enter') { e.preventDefault(); commitText(); }
    else if (e.key === 'Escape') { textAt = null; textIn.hidden = true; }
    e.stopPropagation();
  });
  textIn.addEventListener('blur', commitText);

  // ---- Keys ------------------------------------------------------------------
  function onKey(e) {
    const tag = (e.target.tagName || '').toLowerCase();
    if (tag === 'input' || tag === 'textarea') return;
    const k = e.key.toLowerCase();
    if ((e.metaKey || e.ctrlKey) && k === 'z') { e.preventDefault(); (e.shiftKey ? $('ie-redo') : $('ie-undo')).click(); return; }
    if ((e.metaKey || e.ctrlKey) && k === 'c' && !String(window.getSelection())) { e.preventDefault(); copy(); return; }
    if ((e.metaKey || e.ctrlKey) && k === 's') { e.preventDefault(); save(); return; }
    if (e.metaKey || e.ctrlKey || e.altKey) return;
    if (KEYS[k]) { setTool(KEYS[k]); e.preventDefault(); }
    else if (k >= '1' && k <= '3') { size = +k - 1; syncBar(); }
    else if (k === 'escape' && draft) { draft = null; draw(); }
  }

  // ---- Output ----------------------------------------------------------------
  function render() {
    const c = hist.now.crop || { x: 0, y: 0, w: W, h: H };
    const out = document.createElement('canvas');
    out.width = c.w; out.height = c.h;
    const g = out.getContext('2d');
    g.translate(-c.x, -c.y);
    paintAll(g, hist.now.shapes);
    return new Promise((res, rej) => out.toBlob((b) => (b ? res(b) : rej(new Error('PNG failed'))), 'image/png'));
  }
  async function save() {
    commitText();
    const blob = await render();
    const a = document.createElement('a');
    a.href = URL.createObjectURL(blob);
    a.download = editedName(title, 'png');
    document.body.appendChild(a); a.click(); a.remove();
    setTimeout(() => URL.revokeObjectURL(a.href), 30000);
  }
  async function copy() {
    commitText();
    try {
      await navigator.clipboard.write([new ClipboardItem({ 'image/png': render() })]);
      note('Copied', 1500);
    } catch (e) {
      note("This browser can't copy pictures. Use Save PNG.", 4000);
    }
  }

  const ro = new ResizeObserver(() => layout());
  return {
    show() {
      $('ie').hidden = false;
      document.addEventListener('keydown', onKey);
      ro.observe(stage);
      syncBar(); layout();
    },
    hide() {
      commitText();
      $('ie').hidden = true;
      document.removeEventListener('keydown', onKey);
      ro.disconnect();
    },
    get dirty() { return hist.canUndo; },
    render, // for tests
  };
}
