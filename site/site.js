// Cmd+Meow home page: copy button and the live desktop in the first screen.
(() => {
  // Copy
  document.querySelectorAll('[data-copy]').forEach(b => b.addEventListener('click', () => {
    const el = document.getElementById(b.dataset.copy), label = b.querySelector('span');
    const done = () => { if (!label) return; const o = label.textContent; label.textContent = 'Copied'; setTimeout(() => label.textContent = o, 1500); };
    const select = () => { const r = document.createRange(); r.selectNodeContents(el); const s = getSelection(); s.removeAllRanges(); s.addRange(r); };
    (navigator.clipboard ? navigator.clipboard.writeText(el.textContent) : Promise.reject()).then(done, select);
  }));

  const $ = id => document.getElementById(id);
  const still = matchMedia('(prefers-reduced-motion: reduce)').matches;

  // Clock in the menu bar
  const clock = $('clock');
  const tick = () => { if (clock) clock.textContent = new Date().toLocaleString('en-US', { weekday: 'short', hour: '2-digit', minute: '2-digit', hour12: false }).replace(',', ''); };
  tick(); setInterval(tick, 15000);

  // Windows rise in one after another
  if (!still) ['.hero', '.drop', '.float', '.awake-mini'].forEach((s, i) => {
    const el = document.querySelector(s); if (el) { el.classList.add('reveal'); el.style.animationDelay = (i * 0.12) + 's'; }
  });
  // Tool switcher: list on the left, window on the right; cycles until a click
  const swData = $('sw-data');
  if (swData) {
    const tools = JSON.parse(swData.textContent), items = [...document.querySelectorAll('.sw-item')];
    const esc = s => String(s).replace(/[&<>]/g, c => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;' })[c]);
    let sel = 0, held = false;
    const show = i => {
      sel = i; const t = tools[i];
      items.forEach((b, k) => b.classList.toggle('on', k === i));
      $('sw-n').textContent = t.n; $('sw-big').textContent = t.big; $('sw-sub').textContent = t.sub;
      $('sw-rows').innerHTML = t.rows.map(([l, v, w]) => `<div class="sr"><span>${esc(l)}</span><span class="trk"><b style="width:0%"></b></span><span class="num dm">${esc(v)}</span></div>`).join('');
      const bs = $('sw-rows').querySelectorAll('b');
      requestAnimationFrame(() => requestAnimationFrame(() => t.rows.forEach(([, , w], k) => { if (w) bs[k].style.width = w + '%'; })));
    };
    items.forEach((b, i) => b.addEventListener('click', () => { held = true; show(i); }));
    if (!still) setInterval(() => { if (!held && !document.hidden) show((sel + 1) % tools.length); }, 3500);
  }

  if (still) return;
  const sleep = ms => new Promise(r => setTimeout(r, ms));

  // CPU: bars scroll left, a new sample every 0.8s
  const bars = $('bars'), cpu = $('cpu'), mbCpu = $('mb-cpu'), rpm = $('rpm');
  let load = 16;
  setInterval(() => {
    if (!bars) return;
    load = Math.max(6, Math.min(78, load + (Math.random() - 0.48) * 14));
    const b = bars.firstElementChild; bars.appendChild(b);
    b.style.height = load + '%'; b.className = load > 60 ? 'hot' : '';
    const v = Math.round(load) + '%'; cpu.textContent = v; mbCpu.textContent = v;
    rpm.textContent = (1800 + Math.round(load * 18)).toLocaleString('en-US');
  }, 800);

  // Keep Awake counter
  const awT = $('aw-t'), ring = $('aw-ring');
  let secs = 1 * 3600 + 24 * 60 + 10;
  setInterval(() => {
    secs++;
    const h = Math.floor(secs / 3600), m = Math.floor(secs / 60) % 60, s = secs % 60;
    awT.textContent = `${h}:${String(m).padStart(2, '0')}:${String(s).padStart(2, '0')}`;
    ring.setAttribute('stroke-dashoffset', (125.7 * (1 - (secs % 7200) / 7200)).toFixed(1));
  }, 1000);

  // Clean Up: a scan counts up, then starts over
  const sizes = [1900, 1300, 1000], total = $('cl-total'), state = $('cl-state');
  const fmt = mb => mb >= 1000 ? (mb / 1000).toFixed(1) + ' GB' : Math.round(mb) + ' MB';
  async function scan() {
    const t0 = performance.now(), dur = 3200;
    state.textContent = 'Scanning';
    await new Promise(done => {
      const step = now => {
        const k = Math.min(1, (now - t0) / dur); let sum = 0;
        sizes.forEach((s, i) => {
          const part = Math.min(1, Math.max(0, k * 1.6 - i * 0.3)), v = s * part; sum += v;
          $('cl-s' + i).textContent = fmt(v); $('cl-b' + i).style.width = (v / 1900 * 100) + '%';
        });
        total.textContent = fmt(sum);
        k < 1 ? requestAnimationFrame(step) : done();
      };
      requestAnimationFrame(step);
    });
    state.textContent = 'Ready to clean';
  }
  if (total) (async () => { for (;;) { await scan(); await sleep(5000); } })();

  // Capture: the cursor drags a selection, the shot lands at the top of Clipboard
  const snap = $('snap'), dims = $('dims'), cur = $('cursor'), toast = $('toast'), clip = $('clip');
  const visible = el => el && getComputedStyle(el).display !== 'none';
  async function shoot() {
    if (!visible(snap) || document.hidden) return;
    const W = 220 + Math.round(Math.random() * 120), H = 120 + Math.round(Math.random() * 60);
    snap.style.width = snap.style.height = '0px'; snap.classList.add('on');
    cur.style.transition = 'none'; cur.style.transform = 'translate(-40px,-30px)'; await sleep(50);
    cur.style.transition = ''; cur.style.transform = 'translate(0,0)'; await sleep(700);
    const t0 = performance.now(), dur = 1100;
    await new Promise(done => {
      const step = now => {
        const k = Math.min(1, (now - t0) / dur), e = 1 - Math.pow(1 - k, 3);
        const w = Math.round(W * e), h = Math.round(H * e);
        snap.style.width = w + 'px'; snap.style.height = h + 'px';
        cur.style.transition = 'none'; cur.style.transform = `translate(${w}px,${h}px)`;
        dims.textContent = `${w * 2} × ${h * 2}`;
        k < 1 ? requestAnimationFrame(step) : done();
      };
      requestAnimationFrame(step);
    });
    cur.style.transition = '';
    snap.classList.add('flash'); await sleep(400); snap.classList.remove('flash');
    toast.classList.add('on');
    const row = document.createElement('div');
    row.className = 'cp new';
    row.innerHTML = `<span class="sq" style="background:linear-gradient(135deg,#3b4a6b,#1b2030)"></span><span class="t">Screenshot ${W * 2} × ${H * 2}</span><kbd>⌘1</kbd>`;
    clip.querySelectorAll('.cp').forEach(c => c.classList.remove('cur'));
    row.classList.add('cur'); clip.prepend(row);
    const rows = clip.querySelectorAll('.cp');
    rows.forEach((c, i) => { const k = c.querySelector('kbd'); if (k) k.textContent = '⌘' + (i + 1); });
    if (rows.length > 4) rows[rows.length - 1].remove();
    await sleep(1600);
    toast.classList.remove('on'); snap.classList.remove('on'); cur.style.transform = 'translate(-40px,-30px)';
  }
  (async () => { await sleep(1800); for (;;) { await shoot(); await sleep(3800); } })();
})();
