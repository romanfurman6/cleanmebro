// node --test tests/site
import test from 'node:test';
import assert from 'node:assert/strict';
import * as c from '../../site/view/core.mjs';

test('links: u, old v, hash fallback, only http(s)', () => {
  const s = c.readSource('?u=' + encodeURIComponent('https://b.fsn1.example.com/shots/ab12.png') + '&t=Shot%201', '');
  assert.equal(s.url.href, 'https://b.fsn1.example.com/shots/ab12.png');
  assert.equal(s.title, 'Shot 1');
  assert.equal(c.readSource('?v=https://x.io/a%20b.mp4', '').title, 'a b.mp4');
  assert.equal(c.readSource('', '#u=https://x.io/c.mp4').url.host, 'x.io');
  assert.equal(c.readSource('?u=javascript:alert(1)', '').url, null);
  assert.equal(c.readSource('', '').raw, null);
});

test('kinds from content type or extension', () => {
  assert.equal(c.kindOf('/a/b.PNG'), 'image');
  assert.equal(c.kindOf('/a/b.mov?x=1'), 'video');
  assert.equal(c.kindOf('/a/b', 'image/webp; q=1'), 'image');
  assert.equal(c.kindOf('/a/b.png', 'video/mp4'), 'video');
  assert.equal(c.kindOf('/a/b.txt'), null);
});

test('time formats and saved names', () => {
  assert.equal(c.fmt(65), '1:05');
  assert.equal(c.fmt(3723), '1:02:03');
  assert.equal(c.fmtFine(12.46), '0:12.4');
  assert.equal(c.editedName('Recording 1.mp4', 'mp4'), 'Recording 1 edited.mp4');
  assert.equal(c.editedName('a/b:c', 'png'), 'a b c edited.png');
  assert.equal(c.editedName('', 'png'), 'shot edited.png');
});

test('split, remove, restore', () => {
  const d = 10;
  let tl = c.emptyTimeline();
  tl = c.splitAt(d, tl, 4);
  tl = c.splitAt(d, tl, 6);
  assert.deepEqual(c.segmentsOf(d, tl).map((s) => [s.a, s.b]), [[0, 4], [4, 6], [6, 10]]);
  tl = c.toggleSegment(d, tl, 1);
  assert.deepEqual(c.keptRanges(d, tl), [[0, 4], [6, 10]]);
  assert.equal(c.keptDuration(c.keptRanges(d, tl)), 8);
  // splitting a removed piece keeps both halves removed
  const t2 = c.splitAt(d, tl, 5);
  assert.deepEqual(t2.off, [1, 2]);
  assert.deepEqual(c.keptRanges(d, t2), [[0, 4], [6, 10]]);
  // restore
  assert.deepEqual(c.keptRanges(d, c.toggleSegment(d, tl, 1)), [[0, 10]]);
});

test('splits too close or at the ends are ignored', () => {
  const tl = c.splitAt(10, c.emptyTimeline(), 3);
  assert.equal(c.splitAt(10, tl, 3.05), tl);
  assert.equal(c.splitAt(10, tl, 0.05), tl);
  assert.equal(c.splitAt(10, tl, 9.99), tl);
});

test('the last kept piece cannot be removed', () => {
  let tl = c.splitAt(10, c.emptyTimeline(), 5);
  tl = c.toggleSegment(10, tl, 0);
  assert.equal(c.toggleSegment(10, tl, 1), tl);
  assert.equal(c.toggleSegment(10, c.emptyTimeline(), 0).off.length, 0);
});

test('trim start and end', () => {
  let tl = c.trimStart(10, c.emptyTimeline(), 2);
  tl = c.trimEnd(10, tl, 7.5);
  assert.deepEqual(c.keptRanges(10, tl), [[2, 7.5]]);
  // trimming again further in
  tl = c.trimStart(10, tl, 3);
  assert.deepEqual(c.keptRanges(10, tl), [[3, 7.5]]);
});

test('playback skips removed pieces and maps time', () => {
  const r = [[0, 4], [6, 10]];
  assert.equal(c.playableAt(r, 2), 2);
  assert.equal(c.playableAt(r, 4.5), 6);
  assert.equal(c.playableAt(r, 10), null);
  assert.equal(c.toOutputTime(r, 7), 5);
  assert.equal(c.toOutputTime(r, 5), 4);
});

test('geometry', () => {
  assert.deepEqual(c.normRect(10, 20, 4, 2), { x: 4, y: 2, w: 6, h: 18 });
  assert.deepEqual(c.clampRect({ x: -5, y: 5, w: 20, h: 200 }, 10, 100), { x: 0, y: 5, w: 10, h: 95 });
  const [a, b] = c.arrowHead(0, 0, 100, 0, 4);
  assert.ok(a[0] < 100 && b[0] < 100 && Math.abs(a[1] + b[1]) < 1e-9, 'head is symmetric behind the tip');
  assert.equal(c.fitScale(2000, 1000, 1000, 1000), 0.5);
  assert.equal(c.fitScale(200, 100, 1000, 1000), 1);
  assert.deepEqual(c.strokeWidths(2880, 1800), [7, 14, 28]);
  assert.deepEqual(c.strokeWidths(300, 200), [2, 4, 8]);
});

test('history', () => {
  const h = new c.History(0, 3);
  h.push(1); h.push(2);
  assert.equal(h.undo(), 1);
  h.push(5);
  assert.equal(h.canRedo, false);
  h.push(6);
  assert.deepEqual(h.items, [1, 5, 6]);
  assert.equal(h.undo(), 5);
  assert.equal(h.redo(), 6);
});
