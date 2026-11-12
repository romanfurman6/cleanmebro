// Browser smoke test for the share page (site/view).
//   node tests/site/smoke.mjs
// Serves site/ plus generated media (ffmpeg) with CORS, then in Chromium:
// draws an arrow on a shot and checks the saved PNG; cuts a piece out of a
// recording and checks the saved video's length.
import http from 'node:http';
import fs from 'node:fs';
import path from 'node:path';
import os from 'node:os';
import { execFileSync } from 'node:child_process';
import { fileURLToPath } from 'node:url';
import { createRequire } from 'node:module';

// require() so a global install works too (NODE_PATH).
const { chromium } = createRequire(import.meta.url)('playwright');

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '../../site');
const fx = fs.mkdtempSync(path.join(os.tmpdir(), 'cmdmeow-view-'));
const ff = (args) => execFileSync('ffmpeg', ['-v', 'error', '-y', ...args]);
ff(['-f', 'lavfi', '-i', 'color=c=0x223344:s=640x400', '-frames:v', '1', path.join(fx, 'shot.png')]);
// Open-source Chromium has no H.264, so the test clip is VP9 + Opus.
ff(['-f', 'lavfi', '-i', 'testsrc2=s=320x240:r=30:d=4', '-f', 'lavfi', '-i', 'sine=f=440:d=4',
  '-c:v', 'libvpx-vp9', '-b:v', '400k', '-c:a', 'libopus', '-shortest', path.join(fx, 'clip.webm')]);

const TYPES = { '.html': 'text/html', '.css': 'text/css', '.mjs': 'text/javascript', '.js': 'text/javascript', '.svg': 'image/svg+xml', '.png': 'image/png', '.webm': 'video/webm' };
const server = http.createServer((req, res) => {
  const url = new URL(req.url, 'http://x');
  let file = url.pathname.startsWith('/fx/') ? path.join(fx, url.pathname.slice(4)) : path.join(root, url.pathname);
  if (file.endsWith('/')) file += 'index.html';
  res.setHeader('Access-Control-Allow-Origin', '*');
  res.setHeader('Access-Control-Allow-Headers', '*');
  res.setHeader('Access-Control-Expose-Headers', 'Content-Length, Content-Range, Accept-Ranges');
  if (req.method === 'OPTIONS') { res.writeHead(204); res.end(); return; }
  if (!file.startsWith(root) && !file.startsWith(fx)) { res.writeHead(403); res.end(); return; }
  fs.stat(file, (err, st) => {
    if (err) { res.writeHead(404); res.end(); return; }
    const type = TYPES[path.extname(file)] || 'application/octet-stream';
    const m = /bytes=(\d+)-(\d*)/.exec(req.headers.range || '');
    if (m) {
      const a = +m[1], b = m[2] ? Math.min(+m[2], st.size - 1) : st.size - 1;
      res.writeHead(206, { 'Content-Type': type, 'Content-Range': `bytes ${a}-${b}/${st.size}`, 'Content-Length': b - a + 1, 'Accept-Ranges': 'bytes' });
      if (req.method === 'HEAD') return res.end();
      fs.createReadStream(file, { start: a, end: b }).pipe(res);
    } else {
      res.writeHead(200, { 'Content-Type': type, 'Content-Length': st.size, 'Accept-Ranges': 'bytes' });
      if (req.method === 'HEAD') return res.end();
      fs.createReadStream(file).pipe(res);
    }
  });
});
await new Promise((r) => server.listen(0, '127.0.0.1', r));
const base = `http://127.0.0.1:${server.address().port}`;

const browser = await chromium.launch();
const page = await browser.newPage({ viewport: { width: 1280, height: 800 } });
// Offline runs: MEDIABUNNY_BUNDLE=/path/to/mediabunny.min.mjs serves the
// library locally instead of from the CDN.
if (process.env.MEDIABUNNY_BUNDLE) {
  await page.route('https://cdn.jsdelivr.net/npm/mediabunny@*/**', (r) => r.fulfill({
    status: 200, contentType: 'text/javascript', headers: { 'Access-Control-Allow-Origin': '*' }, body: fs.readFileSync(process.env.MEDIABUNNY_BUNDLE),
  }));
}
const errors = [];
page.on('pageerror', (e) => errors.push(e.message));
let failed = false;
const check = (ok, what) => { console.log((ok ? 'ok   ' : 'FAIL ') + what); if (!ok) failed = true; };

try {
  // ---- Shot ----
  await page.goto(`${base}/view/?u=${encodeURIComponent(base + '/fx/shot.png')}&t=Shot`);
  await page.waitForSelector('#img', { state: 'visible' });
  check(await page.isVisible('#modes'), 'shot: View/Edit switch shows');
  await page.click('#m-edit');
  await page.waitForSelector('#ie-canvas', { state: 'visible' });
  const box = await page.locator('#ie-canvas').boundingBox();
  await page.mouse.move(box.x + box.width * 0.2, box.y + box.height * 0.5);
  await page.mouse.down();
  await page.mouse.move(box.x + box.width * 0.8, box.y + box.height * 0.5, { steps: 5 });
  await page.mouse.up();
  const px = await page.evaluate(async () => {
    const b = await window.__editor.render();
    const bmp = await createImageBitmap(b);
    const c = new OffscreenCanvas(bmp.width, bmp.height), g = c.getContext('2d');
    g.drawImage(bmp, 0, 0);
    return { w: bmp.width, h: bmp.height, mid: [...g.getImageData(bmp.width / 2, bmp.height / 2, 1, 1).data], corner: [...g.getImageData(2, 2, 1, 1).data] };
  });
  check(px.w === 640 && px.h === 400, 'shot: saved at full size');
  check(px.mid[0] > 200 && px.mid[1] < 120, 'shot: arrow is drawn in red');
  check([0x22, 0x33, 0x44].every((v, i) => Math.abs(px.corner[i] - v) <= 3), 'shot: rest of the picture untouched ' + px.corner);
  // crop
  await page.keyboard.press('c');
  const box2 = await page.locator('#ie-canvas').boundingBox();
  await page.mouse.move(box2.x + 10, box2.y + 10);
  await page.mouse.down();
  await page.mouse.move(box2.x + box2.width / 2, box2.y + box2.height / 2, { steps: 4 });
  await page.mouse.up();
  const cropped = await page.evaluate(async () => { const b = await createImageBitmap(await window.__editor.render()); return [b.width, b.height]; });
  check(cropped[0] < 640 && cropped[0] > 250 && cropped[1] < 400 && cropped[1] > 150, `shot: crop applies (${cropped})`);
  await page.keyboard.press('Meta+z');
  await page.keyboard.press('Control+z');
  const undone = await page.evaluate(async () => (await createImageBitmap(await window.__editor.render())).width);
  check(undone === 640, 'shot: undo brings the full picture back');

  // ---- Old player link redirects ----
  await page.goto(`${base}/play/?v=${encodeURIComponent(base + '/fx/clip.webm')}`);
  await page.waitForURL(/\/view\//);
  check(true, 'old /play/ link opens /view/');

  // ---- Recording ----
  await page.goto(`${base}/view/?u=${encodeURIComponent(base + '/fx/clip.webm')}&t=Clip&mode=edit`);
  await page.waitForSelector('#vt', { state: 'visible' });
  await page.waitForFunction(() => document.getElementById('v').duration > 3);
  await page.evaluate(() => { document.getElementById('v').currentTime = 1; });
  await page.waitForFunction(() => Math.abs(document.getElementById('v').currentTime - 1) < 0.05);
  await page.keyboard.press('s');
  await page.evaluate(() => { document.getElementById('v').currentTime = 2.5; });
  await page.waitForFunction(() => Math.abs(document.getElementById('v').currentTime - 2.5) < 0.05);
  await page.keyboard.press('s');
  await page.evaluate(() => { document.getElementById('v').currentTime = 1.5; });
  await page.waitForFunction(() => Math.abs(document.getElementById('v').currentTime - 1.5) < 0.05);
  await page.keyboard.press('Delete');
  check((await page.locator('.vt-seg.off').count()) === 1, 'video: piece marked as removed');
  check((await page.textContent('#vt-len')).trim() === '0:02.5', 'video: length shows 2.5 s');
  const canSave = await page.isEnabled('#vt-export');
  check(canSave, 'video: Save is available');
  if (canSave) {
    await page.click('#vt-export');
    await page.waitForFunction(() => window.__lastExport || !document.getElementById('note').hidden, null, { timeout: 120000 });
    const note = await page.evaluate(() => (document.getElementById('note').hidden ? '' : document.getElementById('note').textContent));
    check(!note, 'video: saved without an error ' + note);
    if (!note) {
    const len = await page.evaluate(() => new Promise((res) => {
      const v = document.createElement('video');
      v.onloadedmetadata = () => res(v.duration);
      v.onerror = () => res(-1);
      v.src = URL.createObjectURL(window.__lastExport);
    }));
    check(Math.abs(len - 2.5) < 0.2, `video: saved copy is 2.5 s (${len})`);
    const tracks = await page.evaluate(async () => {
      const mb = await import('https://cdn.jsdelivr.net/npm/mediabunny@1.63.0/dist/bundles/mediabunny.min.mjs');
      const input = new mb.Input({ source: new mb.BlobSource(window.__lastExport), formats: mb.ALL_FORMATS });
      const a = await input.getPrimaryAudioTrack();
      return { mime: await input.getMimeType(), audio: a ? await a.computeDuration() : 0 };
    });
    check(tracks.mime.startsWith('video/mp4') && Math.abs(tracks.audio - 2.5) < 0.2, 'video: MP4 with sound of the same length ' + JSON.stringify(tracks));
    }
  }
  check(errors.length === 0, 'no page errors ' + errors.join('; '));
} finally {
  await browser.close();
  server.close();
  fs.rmSync(fx, { recursive: true, force: true });
}
process.exit(failed ? 1 : 0);
