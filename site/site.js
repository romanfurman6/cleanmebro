// Cmd+Meow site: live demos. Each demo starts when it scrolls into view and
// loops while visible. With reduced motion every demo shows its final frame.
(function () {
  'use strict';
  var $ = function (id) { return document.getElementById(id); };
  var still = matchMedia('(prefers-reduced-motion: reduce)').matches;
  var wait = function (ms) { return new Promise(function (r) { setTimeout(r, still ? 0 : ms); }); };
  function el(tag, cls, html) { var e = document.createElement(tag); if (cls) e.className = cls; if (html != null) e.innerHTML = html; return e; }
  function gb(mb) { return mb >= 1000 ? (mb / 1000).toFixed(1) + ' GB' : Math.round(mb) + ' MB'; }
  function tween(ms, fn) {
    return new Promise(function (done) {
      if (still) { fn(1); return done(); }
      var t0 = performance.now();
      (function step(t) {
        var k = Math.min(1, (t - t0) / ms), e = 1 - Math.pow(1 - k, 3);
        fn(e); if (k < 1) requestAnimationFrame(step); else done();
      })(t0);
    });
  }

  // ---- Hero: clock, CPU history, fan --------------------------------------
  var d = new Date(), days = ['Sun', 'Mon', 'Tue', 'Wed', 'Thu', 'Fri', 'Sat'];
  $('clock').textContent = days[d.getDay()] + ' ' + String(d.getHours()).padStart(2, '0') + ':' + String(d.getMinutes()).padStart(2, '0');
  var bars = $('bars'), v = [];
  for (var i = 0; i < 40; i++) { v.push(12 + Math.random() * 30); bars.appendChild(el('i')); }
  function drawCpu() {
    Array.prototype.forEach.call(bars.children, function (b, i) { b.style.height = v[i] + '%'; b.className = v[i] > 70 ? 'hot' : ''; });
    $('cpu').textContent = Math.round(v[v.length - 1]) + '%';
    var rpm = 1800 + Math.round(v[v.length - 1] * 28);
    $('rpm').textContent = rpm.toLocaleString('en-US');
    $('fan').style.setProperty('--spin', (60 / (rpm / 60) * 6).toFixed(2) + 's');
  }
  drawCpu();
  if (!still) setInterval(function () {
    var last = v[v.length - 1], n = last + (24 - last) * 0.35 + (Math.random() - 0.5) * 18;
    if (Math.random() < 0.07) n = 70 + Math.random() * 22;
    v.push(Math.max(6, Math.min(95, n))); v.shift(); drawCpu();
  }, 900);

  // ---- Clean Up: scan fills rows, total counts up ---------------------------
  var CLEAN = [
    ['User caches', '~/Library/Caches', 1840],
    ['Xcode DerivedData', '~/Library/Developer/Xcode/DerivedData', 1260],
    ['Browser caches', '~/Library/Caches/Google/Chrome', 620],
    ['Logs', '~/Library/Logs', 410],
    ['Homebrew downloads', '~/Library/Caches/Homebrew', 96]
  ];
  async function clean() {
    var rows = $('cl-rows'), total = 0, max = 1840;
    rows.innerHTML = ''; $('cl-total').textContent = '0 MB'; $('cl-state').textContent = 'Scanning…';
    $('cl-btn').classList.add('off');
    for (var i = 0; i < CLEAN.length; i++) {
      var c = CLEAN[i];
      var r = el('div', 'cl-row', '<span class="cb on"></span><span class="nm">' + c[0] + '</span><span class="track"><b></b></span><span class="num sz">0 MB</span>');
      rows.appendChild(r);
      $('cl-path').textContent = c[1];
      var bar = r.querySelector('b'), sz = r.querySelector('.sz'), base = total;
      await tween(900, function (e) {
        bar.style.width = (c[2] / max * 100 * e) + '%';
        sz.textContent = gb(c[2] * e);
        $('cl-total').textContent = gb(base + c[2] * e);
      });
      total += c[2];
    }
    $('cl-state').textContent = 'Ready to clean · 5 groups';
    $('cl-path').textContent = 'Nothing is deleted until you confirm';
    $('cl-btn').classList.remove('off');
  }

  // ---- Disk Map: sunburst rings ---------------------------------------------
  var TREE = { n: 'Macintosh HD', s: 688, c: [
    { n: 'Users', s: 412, c: [
      { n: 'Library', s: 168, c: [{ n: 'Developer', s: 74 }, { n: 'Caches', s: 31 }, { n: 'Containers', s: 38 }, { n: 'Application Support', s: 25 }] },
      { n: 'Movies', s: 96, c: [{ n: 'Screen recordings', s: 61 }, { n: 'Final Cut', s: 35 }] },
      { n: 'Projects', s: 88, c: [{ n: 'node_modules ×214', s: 41 }, { n: 'target', s: 29 }, { n: 'src', s: 18 }] },
      { n: 'Downloads', s: 60, c: [{ n: 'Installers', s: 38 }, { n: 'Other', s: 22 }] } ] },
    { n: 'Applications', s: 128, c: [{ n: 'Xcode', s: 41 }, { n: 'Docker', s: 23 }, { n: 'Other apps', s: 64 }] },
    { n: 'System', s: 96, c: [{ n: 'Cryptexes', s: 52 }, { n: 'Library', s: 44 }] },
    { n: 'Library', s: 52, c: [{ n: 'Developer', s: 30 }, { n: 'Other', s: 22 }] }
  ] };
  var R = [[38, 74], [78, 112], [116, 150]];
  var SHADES = ['#0A84FF', '#3D9BFF', '#6FB4FF'];
  function arc(r0, r1, a0, a1) {
    var g = 0.006; a0 += g; a1 -= g; if (a1 < a0) a1 = a0;
    var p = function (r, a) { return (r * Math.sin(a)).toFixed(2) + ' ' + (-r * Math.cos(a)).toFixed(2); };
    var big = a1 - a0 > Math.PI ? 1 : 0;
    return 'M' + p(r1, a0) + 'A' + r1 + ' ' + r1 + ' 0 ' + big + ' 1 ' + p(r1, a1) + 'L' + p(r0, a1) + 'A' + r0 + ' ' + r0 + ' 0 ' + big + ' 0 ' + p(r0, a0) + 'Z';
  }
  var segs = [];
  (function lay(node, depth, a0, span) {
    if (!node.c) return;
    var a = a0;
    node.c.forEach(function (ch) {
      var w = span * ch.s / node.s;
      segs.push({ node: ch, parent: node, depth: depth, a0: a, a1: a + w });
      lay(ch, depth + 1, a, w);
      a += w;
    });
  })(TREE, 0, 0, Math.PI * 2 * TREE.s / 1000);
  var sun = $('sun'), NS = 'http://www.w3.org/2000/svg';
  var free = document.createElementNS(NS, 'path');
  free.setAttribute('class', 'free');
  sun.appendChild(free);
  segs.forEach(function (s) {
    var p = document.createElementNS(NS, 'path');
    var hot = /node_modules|DerivedData|Screen recordings|Developer/.test(s.node.n) && s.depth === 2;
    p.setAttribute('fill', hot ? '#FF9F0A' : SHADES[s.depth]);
    p.setAttribute('fill-opacity', String(1 - s.depth * 0.12));
    p.addEventListener('mouseenter', function () { pick(s, p); });
    p.addEventListener('mouseleave', function () { pick(null); });
    sun.appendChild(p); s.el = p;
  });
  function pick(s, p) {
    segs.forEach(function (x) { x.el.classList.toggle('dimmed', !!s && x !== s); });
    $('map-name').textContent = s ? s.node.n : 'Macintosh HD';
    $('map-size').textContent = (s ? s.node.s : 688) + ' GB';
    $('map-hint').textContent = s ? 'in ' + s.parent.n : 'of 1 TB used';
  }
  async function map() {
    var full = Math.PI * 2;
    await tween(1600, function (e) {
      var lim = full * e;
      free.setAttribute('d', arc(38, 150, Math.PI * 2 * 0.688, Math.PI * 2 * 0.688 + (full * 0.312) * e));
      segs.forEach(function (s) {
        var a1 = Math.min(s.a1, lim);
        s.el.setAttribute('d', a1 > s.a0 ? arc(R[s.depth][0], R[s.depth][1], s.a0, a1) : '');
      });
    });
  }

  // ---- Capture: draw a selection, record, share ----------------------------
  async function capture() {
    var sel = $('cap-sel'), cross = $('cap-cross'), rec = $('cap-rec'), toast = $('cap-toast'), scr = $('cap-screen');
    var W = scr.clientWidth, H = scr.clientHeight;
    [sel, rec, toast].forEach(function (x) { x.classList.remove('on'); });
    sel.classList.remove('rec'); cross.classList.add('on');
    var x0 = W * 0.18, y0 = H * 0.2, x1 = W * 0.78, y1 = H * 0.78;
    cross.style.transform = 'translate(' + x0 + 'px,' + y0 + 'px)';
    await wait(500);
    sel.classList.add('on');
    await tween(1200, function (e) {
      var x = x0 + (x1 - x0) * e, y = y0 + (y1 - y0) * e;
      sel.style.left = x0 + 'px'; sel.style.top = y0 + 'px';
      sel.style.width = (x - x0) + 'px'; sel.style.height = (y - y0) + 'px';
      cross.style.transform = 'translate(' + x + 'px,' + y + 'px)';
      $('cap-dims').textContent = Math.round((x - x0) * 2.2) + ' × ' + Math.round((y - y0) * 2.2);
    });
    cross.classList.remove('on');
    await wait(400);
    sel.classList.add('rec'); rec.classList.add('on');
    for (var t = 0; t <= 4; t++) { $('cap-time').textContent = '0:0' + t; await wait(600); }
    rec.classList.remove('on'); sel.classList.remove('rec', 'on');
    toast.classList.add('on');
  }

  // ---- Clipboard: new items arrive on top ----------------------------------
  var CLIPS = [
    ['txt', 'Meet at 6, north exit', 'Messages'],
    ['link', 'github.com/romanfurman6/cmdmeow', 'Safari'],
    ['img', 'Screenshot 1440 × 940', 'Capture'],
    ['code', 'fn scan(root: &Path) -> Result<Vec<Group>>', 'Xcode'],
    ['color', '#0A84FF', 'Figma'],
    ['txt', '4 Rue de la Paix, Paris', 'Mail'],
    ['hidden', 'Hidden: looks like a password', '1Password']
  ];
  var ICON = {
    txt: '<path d="M3 3h10M8 3v10M6 13h4"/>',
    link: '<path d="M6.5 9.5l3-3M7 4.5l1.5-1.5a2.5 2.5 0 0 1 3.5 3.5L10.5 8M9 11.5 7.5 13A2.5 2.5 0 0 1 4 9.5L5.5 8"/>',
    img: '<path d="M1.5 2.5h13v11h-13zM1.5 11l4-4 3 3 2-2 4 4"/>',
    code: '<path d="M5.5 4 2 8l3.5 4M10.5 4 14 8l-3.5 4"/>',
    hidden: '<path d="M4.5 7.5v-2a3.5 3.5 0 0 1 7 0v2M3.5 7.5h9v6h-9z"/>'
  };
  function clipRow(c) {
    var ic = c[0] === 'color' ? '<span class="sw-c"></span>' : '<svg class="ic" width="15" height="15" viewBox="0 0 16 16">' + ICON[c[0]] + '</svg>';
    var cls = c[0] === 'code' ? 'nm mono' : c[0] === 'hidden' ? 'nm dim' : 'nm';
    return el('div', 'clip-row', ic + '<div class="cl-t"><div class="' + cls + '"></div><div class="cap">' + c[2] + '</div></div><kbd></kbd>');
  }
  async function clip() {
    var list = $('clip-list'); list.innerHTML = '';
    for (var i = CLIPS.length - 1; i >= 0; i--) {
      var r = clipRow(CLIPS[i]);
      r.querySelector('.nm').textContent = CLIPS[i][1];
      list.insertBefore(r, list.firstChild);
      Array.prototype.forEach.call(list.children, function (row, k) { row.querySelector('kbd').textContent = '⌘' + (k + 1); row.classList.toggle('cur', k === 0); });
      if (!still) r.classList.add('in');
      await wait(i === CLIPS.length - 1 ? 300 : 700);
    }
  }

  // ---- Keep Awake: click to toggle, ring runs the grace period -------------
  var aw = $('awake'), ring = $('aw-ring'), L = 2 * Math.PI * 86;
  ring.style.strokeDasharray = L;
  function setAwake(on) {
    aw.classList.toggle('on', on); aw.setAttribute('aria-pressed', String(on));
    $('aw-state').textContent = on ? 'Awake' : 'Can sleep';
    $('aw-sub').textContent = on ? 'Claude Code is working' : 'Normal sleep rules';
    ring.style.strokeDashoffset = on ? 0 : L;
  }
  aw.addEventListener('click', function () { setAwake(!aw.classList.contains('on')); });
  async function awake() { setAwake(false); await wait(500); setAwake(true); }

  // ---- Run each demo while on screen ---------------------------------------
  var DEMOS = { clean: [clean, 6000], map: [map, 0], capture: [capture, 3500], clip: [clip, 4000], awake: [awake, 0] };
  var io = new IntersectionObserver(function (es) {
    es.forEach(function (e) {
      var st = e.target, d = DEMOS[st.dataset.demo];
      st.visible = e.isIntersecting;
      if (!e.isIntersecting || st.running) return;
      st.running = true;
      (async function loop() {
        do { await d[0](); if (!d[1] || still) break; await wait(d[1]); } while (st.visible);
        st.running = false;
      })();
    });
  }, { threshold: 0.35 });
  document.querySelectorAll('[data-demo]').forEach(function (s) { io.observe(s); });

  document.querySelectorAll('[data-copy]').forEach(function (b) {
    b.addEventListener('click', function () {
      navigator.clipboard.writeText($(b.dataset.copy).textContent).then(function () {
        b.textContent = 'Copied'; setTimeout(function () { b.textContent = 'Copy'; }, 1500);
      });
    });
  });
})();
