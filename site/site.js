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
  if (!still) ['.hero', '.drop', '.float', '.awake-mini', '.clean-mini'].forEach((s, i) => {
    const el = document.querySelector(s); if (el) { el.classList.add('reveal'); el.style.animationDelay = (i * 0.12) + 's'; }
  });
  // Ten tools: the app window. Sidebar picks a pane; it cycles until someone clicks
  const app = $('app');
  if (app) {
    const navs = [...app.querySelectorAll('.nv[data-p]')], panes = [...app.querySelectorAll('.pane')];
    let sel = 0, held = false;
    const show = i => {
      sel = i; const key = navs[i].dataset.p;
      navs.forEach((b, k) => { b.classList.toggle('on', k === i); b.setAttribute('aria-pressed', k === i); });
      panes.forEach(p => { const on = p.dataset.p === key; p.hidden = !on; p.classList.toggle('on', on); });
      const row = navs[i].parentElement; // on phones the tabs scroll sideways: keep the picked one in view
      if (row.scrollWidth > row.clientWidth) row.scrollTo({ left: navs[i].offsetLeft - row.offsetLeft - 12, behavior: 'smooth' });
    };
    navs.forEach((b, i) => b.addEventListener('click', () => { held = true; show(i); }));
    // play.js: anything done inside the window stops the cycling; other parts can open a tool
    app.addEventListener('pointerdown', () => { held = true; });
    window.cmShow = key => { const i = navs.findIndex(b => b.dataset.p === key); if (i >= 0) { held = true; show(i); } };
    let inView = false;
    new IntersectionObserver(es => { inView = es[0].isIntersecting; }, { threshold: 0.4 }).observe(app);
    if (!still) setInterval(() => { if (!held && inView && !document.hidden) show((sel + 1) % navs.length); }, 4000);
    // Live numbers inside the window
    const hb = $('hc-bars'), hc = $('hc-cpu'), leg = $('hc-leg'), sbr = $('sb-rpm');
    let hl = 16;
    const fanTick = still => {
      const avg = window.cmFanDraw ? window.cmFanDraw(hl, still) : 1900 + hl * 20;
      if (sbr) sbr.textContent = Math.round(avg).toLocaleString('en-US').replace(',', ' ');
    };
    window.cmFanTick = () => fanTick(true);
    if (!still) setInterval(() => {
      hl = Math.max(6, Math.min(70, hl + (Math.random() - 0.5) * 12));
      if (hb) { const b = hb.firstElementChild; hb.appendChild(b); const h = 30 + hl; b.style.height = h + '%'; b.className = h > 90 ? 'hot' : ''; }
      if (hc) hc.textContent = Math.round(hl);
      fanTick();
      if (leg) { const u = Math.round(hl * 0.7); leg.innerHTML = `User ${u}%&nbsp;&nbsp; System ${Math.round(hl) - u}%&nbsp;&nbsp; Idle ${100 - Math.round(hl)}%`; }
    }, 900);
  }

  if (still) return;

  // Hero: the app it replaces rolls over, letter by letter
  const swap = $('swap');
  if (swap) {
    const names = ['CleanMyMac', 'Loom', 'CleanShot X', 'Paste', 'iStat Menus', 'Wispr Flow', 'DaisyDisk'];
    const word = t => { const w = document.createElement('span'); w.className = 'w'; [...t].forEach(ch => { const c = document.createElement('span'); c.className = 'c'; c.textContent = ch; w.append(c); }); return w; };
    let k = 0, cur = swap.querySelector('.w');
    cur.replaceWith(cur = word(names[0]));
    setInterval(() => {
      if (document.hidden) return;
      k = (k + 1) % names.length;
      const out = cur, next = word(names[k]); out.classList.add('out');
      [...out.children].forEach((c, i) => c.animate(
        [{ transform: 'none', opacity: 1, filter: 'blur(0)' }, { transform: 'translateY(-55%) rotateX(80deg)', opacity: 0, filter: 'blur(6px)' }],
        { duration: 380, delay: i * 22, easing: 'cubic-bezier(.5,0,.75,0)', fill: 'forwards' }));
      setTimeout(() => out.remove(), 380 + out.children.length * 22);
      swap.append(next); cur = next;
      [...next.children].forEach((c, i) => c.animate(
        [{ transform: 'translateY(60%) rotateX(-85deg)', opacity: 0, filter: 'blur(6px)' }, { transform: 'none', opacity: 1, filter: 'blur(0)' }],
        { duration: 620, delay: 140 + i * 28, easing: 'cubic-bezier(.2,1.4,.4,1)', fill: 'backwards' }));
    }, 2400);
  }
  const sleep = ms => new Promise(r => setTimeout(r, ms));

  // CPU in the menu bar and the panel, a new sample every 0.8s
  const cpu = $('cpu'), mbCpu = $('mb-cpu'), rpm = $('rpm');
  const cpuRing = cpu && cpu.closest('.rw2').querySelector('circle:last-of-type');
  const C = cpuRing ? +cpuRing.getAttribute('stroke-dasharray') : 0;
  let load = 16;
  setInterval(() => {
    load = Math.max(6, Math.min(78, load + (Math.random() - 0.48) * 14));
    const v = Math.round(load) + '%';
    if (cpu) cpu.textContent = v;
    if (mbCpu) mbCpu.textContent = v;
    if (cpuRing) cpuRing.setAttribute('stroke-dashoffset', (C * (1 - load / 100)).toFixed(1));
    if (rpm) rpm.textContent = (1800 + Math.round(load * 18)).toLocaleString('en-US').replace(',', ' ');
  }, 800);

  // Keep Awake counter
  const awT = $('aw-t'), ring = $('aw-ring');
  let secs = 1 * 3600 + 24 * 60 + 10;
  setInterval(() => {
    secs++;
    const h = Math.floor(secs / 3600), m = Math.floor(secs / 60) % 60, s = secs % 60;
    if (!awT) return;
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
  if (snap) (async () => { await sleep(1800); for (;;) { await shoot(); await sleep(3800); } })();
})();
