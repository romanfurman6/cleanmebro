// Cmd+Meow home page: the app window works. Every tool pane reacts to clicks,
// drags and typing, and a strip under the window lists what else each tool does.
(() => {
  const app = document.getElementById('app');
  if (!app) return;
  const $ = (s, r = app) => r.querySelector(s), $$ = (s, r = app) => [...r.querySelectorAll(s)];
  const pane = k => $(`.pane[data-p="${k}"]`);
  const sleep = ms => new Promise(r => setTimeout(r, ms));
  const d3 = v => v.toFixed(v >= 100 ? 0 : v >= 10 ? 1 : 2), fmt = mb => mb >= 1e6 ? d3(mb / 1e6) + ' TB' : mb >= 1e3 ? d3(mb / 1e3) + ' GB' : mb > 0 ? d3(mb) + ' MB' : '0 MB';
  const parse = t => { const m = /([\d.,]+)\s*(TB|GB|MB|KB)/.exec(t || ''); if (!m) return 0; const v = parseFloat(m[1].replace(',', '')); return v * { TB: 1e6, GB: 1e3, MB: 1, KB: 1e-3 }[m[2]]; };
  const el = (tag, cls, html) => { const e = document.createElement(tag); if (cls) e.className = cls; if (html != null) e.innerHTML = html; return e; };
  const ICON = d => `<svg class="i" width="14" height="14" viewBox="0 0 24 24" aria-hidden="true"><path d="${d}"/></svg>`;

  // Shared controls: segmented pickers and switches
  app.addEventListener('click', e => {
    const opt = e.target.closest('.seg > i');
    if (opt && !opt.closest('[data-own]')) {
      const seg = opt.parentElement, cur = seg.querySelector('b');
      if (cur) { const i = el('i', null, cur.innerHTML); cur.replaceWith(i); }
      const b = el('b', null, opt.innerHTML); opt.replaceWith(b);
      seg.dispatchEvent(new CustomEvent('pick', { detail: b.textContent.trim(), bubbles: true }));
    }
    const tg = e.target.closest('.tg');
    if (tg && !tg.closest('[data-own]')) { tg.classList.toggle('off'); tg.dispatchEvent(new CustomEvent('flip', { bubbles: true })); }
  });

  // ---------- Sharing: copy a link, open it, delete it ----------
  (() => {
    const p = pane('sharing'); if (!p) return;
    const n = $('#sh-n', p);
    p.addEventListener('click', e => {
      const b = e.target.closest('.rb'); if (!b) return;
      const row = b.closest('.shr'), k = [...row.querySelectorAll('.rb')].indexOf(b);
      if (k === 0) { b.classList.add('ok'); setTimeout(() => b.classList.remove('ok'), 1200); }
      if (k === 2) { row.classList.add('gone'); setTimeout(() => { row.remove(); n.textContent = $$('.shr', p).length; }, 250); }
    });
  })();

  // ---------- Cleanup ----------
  (() => {
    const p = pane('cleanup'); if (!p) return;
    const L = $('.cuL', p), sfl = $('.sfl', p), kpl = $('.kpl', p), sel = $('.fb > span:first-child', p), go = $('.fb .act', p), title = $('.tb .dm', p);
    const CK = ICON('M5 12.5l4.5 4.5L19 7');
    const vis = r => !r.hidden;
    const sum = rows => rows.reduce((a, r) => a + parse($('.sz', r).textContent), 0);
    const upd = () => {
      const safe = $$('.sfr', sfl), on = safe.filter(r => !$('.ck', r).classList.contains('off'));
      const v = sum(on); sel.textContent = fmt(v) + ' selected'; go.textContent = v ? 'Clean ' + fmt(v) : 'Clean'; go.classList.toggle('dim', !v);
      $('#cu-safe', p).textContent = `${safe.filter(vis).length} · ${fmt(sum(safe.filter(vis)))}`;
      const kept = $$('.sfr', kpl); $('#cu-kept', p).textContent = `${kept.length} · ${fmt(sum(kept))}`;
      const ask = $$('.akr', L).filter(vis);
      $('.sh2.ap .num', p).textContent = `${ask.length} · ${fmt(ask.reduce((a, r) => a + parse(r.dataset.s), 0))}`;
    };
    const decide = (card, allow) => {
      if (card.classList.contains('gone')) return;
      const name = $('.cn b', card).firstChild.textContent.trim(), size = card.dataset.s;
      card.classList.add('gone'); setTimeout(() => { card.remove(); upd(); }, 300);
      if (allow) { const r = el('div', 'sfr in', `<span class="ck">${CK}</span><span class="nm">${name}</span><span class="num sz">${size}</span>`); r.dataset.g = card.dataset.g; sfl.prepend(r); }
      else kpl.append(el('div', 'sfr kp in', `<span class="lck">${ICON('M6.5 11h11v9h-11zM9 11V8a3 3 0 0 1 6 0v3')}</span><span class="nm">${name}</span><span class="num sz">${size}</span>`));
      [$('.gch[data-g="All"] .cnt', p), $(`.gch[data-g="${card.dataset.g}"] .cnt`, p)].forEach(x => { if (x) { const n = +x.textContent - 1; if (n) x.textContent = n; else x.remove(); } });
    };
    L.addEventListener('click', e => {
      const card = e.target.closest('.akr');
      if (card && e.target.closest('.act')) return decide(card, true);
      if (card && e.target.closest('.btn2')) return decide(card, false);
      const lk = e.target.closest('.lk'); if (lk) $$('.akr', L).filter(vis).forEach(c => decide(c, lk.textContent === 'Allow all'));
    });
    sfl.addEventListener('click', e => { const r = e.target.closest('.sfr'); if (!r) return; const ck = $('.ck', r); const off = ck.classList.toggle('off'); ck.innerHTML = off ? '' : CK; upd(); });
    $('.sh2.lm .lk', p).addEventListener('click', e => { const none = e.target.textContent === 'None'; $$('.sfr', sfl).forEach(r => { const ck = $('.ck', r); ck.classList.toggle('off', none); ck.innerHTML = none ? '' : CK; }); e.target.textContent = none ? 'All' : 'None'; upd(); });
    $$('.gch', p).forEach(g => g.addEventListener('click', () => {
      $$('.gch', p).forEach(x => x.classList.toggle('on', x === g)); const k = g.dataset.g;
      $$('.akr, .sfr', $('.cu2', p)).forEach(r => { if (r.dataset.g) r.hidden = !(k === 'All' || r.dataset.g === k); });
      upd();
    }));
    go.addEventListener('click', async () => {
      const on = $$('.sfr', sfl).filter(r => vis(r) && !$('.ck', r).classList.contains('off'));
      if (!on.length || go.dataset.busy) return; go.dataset.busy = 1;
      const freed = sum(on);
      for (const r of on) { r.classList.add('gone'); await sleep(110); }
      await sleep(250); on.forEach(r => r.remove());
      upd(); title.textContent = 'Freed ' + fmt(freed); go.textContent = 'Done';
      delete go.dataset.busy;
    });
    upd();
  })();

  // ---------- Disk Map ----------
  (() => {
    const p = pane('diskmap'); if (!p) return;
    const svg = $('.bubbles', p), side = $('.fl', p), hdr = $('.tb .dm', p), foot = $('.fb', p), back = $('.tb .btn2', p);
    const kids = (pairs) => pairs.map(([n, s, c]) => ({ n, s, c: c ? kids(c) : null }));
    const root = { n: 'me', s: 365000, c: kids([
      ['Library', 182000, [['Containers', 61000, [['com.docker.docker', 46000], ['Slack', 6200], ['Telegram', 4800], ['Mail', 4000]]], ['Application Support', 52000, [['Claude', 21000], ['Google', 12000], ['Steam', 11000], ['Code', 8000]]], ['Developer', 38000, [['Xcode', 21000], ['CoreSimulator', 14000], ['Toolchains', 3000]]], ['Caches', 18000], ['Mail', 8000], ['Messages', 5000]]],
      ['Projects', 64200, [['cmdmeow', 21000, [['target', 18400], ['site', 1600], ['src', 400], ['vendor', 600]]], ['ios-app', 16000], ['api-server', 12000], ['old-experiment', 9200], ['blog', 6000]]],
      ['Downloads', 38400, [['Xcode_16.xip', 12000], ['macOS installer', 14000], ['Videos', 8000], ['PDFs', 4400]]],
      ['Movies', 31000, [['Final Cut Library', 24000], ['Screen recordings', 7000]]],
      ['Pictures', 22500, [['Photos Library', 21000], ['Screenshots', 1500]]],
      ['Documents', 14100, [['Taxes', 2100], ['Books', 6000], ['Scans', 6000]]],
      ['Music', 6300], ['Desktop', 2100], ['.cargo', 4200]]) };
    const picked = new Set(root.c.filter(c => c.n === 'Movies'));
    const kid = n => path[path.length - 1].c.find(c => c.n === n);
    const flip = node => { if (node) picked.has(node) ? picked.delete(node) : picked.add(node); };
    let path = [root];
    // Greedy circle packing: biggest in the middle, the rest on a spiral around it
    const pack = items => {
      const W = 470, H = 340, max = Math.max(...items.map(i => i.s));
      const out = [];
      items.slice(0, 12).forEach((it, k) => {
        const r = Math.max(9, 92 * Math.sqrt(it.s / max));
        if (!k) { out.push({ ...it, x: W / 2, y: H / 2, r }); return; }
        for (let a = 0, d = 0; d < 400; a += 0.35, d += 0.6) {
          const x = W / 2 + Math.cos(a + k) * d * 1.35, y = H / 2 + Math.sin(a + k) * d;
          if (x - r < 4 || x + r > W - 4 || y - r < 4 || y + r > H - 4) continue;
          if (out.every(o => Math.hypot(o.x - x, o.y - y) > o.r + r + 4)) { out.push({ ...it, x, y, r }); return; }
        }
      });
      return out;
    };
    const draw = () => {
      const node = path[path.length - 1];
      const items = [...node.c].sort((a, b) => b.s - a.s);
      svg.innerHTML = pack(items).map(b => {
        const pk = items.some(i => i.n === b.n && picked.has(i)) ? ' pk' : '', dk = b.c ? '' : ' leaf';
        const fo = b.c && b.r > 20 ? `<path class="fo" transform="translate(${(b.x - 10).toFixed(1)} ${(b.y - (b.r > 34 ? 30 : 22)).toFixed(1)})" d="M1 3.5h6l2 2h10v10.5H1z"/>` : '';
        const label = b.r > 34 ? `<text x="${b.x}" y="${b.y + 8}" text-anchor="middle"><tspan x="${b.x}" class="bn">${b.n}</tspan><tspan x="${b.x}" dy="14" class="bv">${fmt(b.s)}</tspan></text>`
          : b.r > 22 ? `<text x="${b.x}" y="${b.y + 6}" text-anchor="middle" class="sm"><tspan x="${b.x}">${b.n.slice(0, 11)}</tspan><tspan x="${b.x}" dy="11">${fmt(b.s)}</tspan></text>` : '';
        return `<g class="bb${pk}${dk}" data-n="${b.n}"><title>${b.n} · ${fmt(b.s)}</title><circle cx="${b.x.toFixed(1)}" cy="${b.y.toFixed(1)}" r="${b.r.toFixed(1)}"/>${fo}${label}</g>`;
      }).join('');
      const crumbs = path.map(n => n.n).join(' › ');
      hdr.textContent = `${crumbs} · ${fmt(node.s)}`;
      back.textContent = path.length > 1 ? 'Back' : 'Start over';
      side.innerHTML = `<div class="fh">${ICON('M3.5 6.5h6l2 2h9v10h-17z')}<div><b>${node.n}</b><span class="dm">${fmt(node.s)}</span></div></div>` +
        items.slice(0, 9).map(i => `<div class="fr" data-n="${i.n}"><span class="ck${picked.has(i) ? '' : ' off'}">${picked.has(i) ? ICON('M5 12.5l4.5 4.5L19 7') : ''}</span>${ICON(i.c ? 'M3.5 6.5h6l2 2h9v10h-17z' : 'M6 3.5h8l4 4v13H6z')}<span>${i.n}</span><span class="num dm">${fmt(i.s)}</span></div>`).join('');
      const all = []; const walk = n => { if (picked.has(n)) return all.push(n.s); (n.c || []).forEach(walk); }; walk(root);
      foot.firstElementChild.innerHTML = `<b class="num">${fmt(all.reduce((a, b) => a + b, 0))}</b> <span class="dm">${all.length} item${all.length === 1 ? '' : 's'} picked</span>`;
    };
    const open = n => { const node = path[path.length - 1].c.find(c => c.n === n); if (node && node.c) { path.push(node); draw(); } };
    svg.addEventListener('click', e => { const g = e.target.closest('.bb'); if (!g) return; const n = g.dataset.n; const node = path[path.length - 1].c.find(c => c.n === n); if (node && node.c) open(n); else { flip(node); draw(); } });
    side.addEventListener('click', e => {
      const r = e.target.closest('.fr'); if (!r) return;
      if (e.target.closest('.ck')) { flip(kid(r.dataset.n)); return draw(); }
      open(r.dataset.n);
    });
    back.addEventListener('click', () => { path = path.length > 1 ? path.slice(0, -1) : [root]; draw(); });
    $('.fb .btn2', p).addEventListener('click', () => { picked.clear(); draw(); });
    const trash = $('.fb .act', p);
    if (trash) trash.addEventListener('click', async () => {
      if (!picked.size || trash.dataset.busy) return; trash.dataset.busy = 1; const o = trash.textContent; trash.textContent = 'Moved to Trash';
      const prune = n => { if (!n.c) return 0; let cut = 0; n.c = n.c.filter(c => picked.has(c) ? (cut += c.s, false) : true); n.c.forEach(c => { const d = prune(c); c.s -= d; cut += d; }); return cut; };
      root.s -= prune(root); picked.clear(); draw(); await sleep(1200); trash.textContent = o; delete trash.dataset.busy;
    });
    draw();
  })();

  // ---------- Uninstaller ----------
  (() => {
    const p = pane('uninstall'); if (!p) return;
    const det = $('.det', p), files = $$('.fi', det).map(f => f.outerHTML);
    const ver = { 'Android Studio': '2024.2 · com.google.android.studio', 'CleanShot X': '4.7 · pl.maketheweb.cleanshotx', Docker: '4.36.0 · com.docker.docker', Figma: '124.5 · com.figma.Desktop', 'Final Cut Pro': '11.0 · com.apple.FinalCut', Slack: '4.41 · com.tinyspeck.slackmacgap', Spotify: '1.2.50 · com.spotify.client', VLC: '3.0.21 · org.videolan.vlc', Xcode: '16.1 · com.apple.dt.Xcode', Zoom: '6.2.10 · us.zoom.xos' };
    const pick = r => {
      $$('.arw', p).forEach(x => x.classList.toggle('on', x === r));
      const name = $('.an', r).firstChild.textContent.trim(), size = parse($('.num', r).textContent), ai = $('.ai', r);
      const id = (ver[name] || '1.0 · app').split(' · ')[1];
      const appMb = Math.min(size * 0.35, 2100), data = size - appMb;
      $('.dh .ai', det).style.background = ai.style.background; $('.dh .ai', det).textContent = ai.textContent;
      $('.dh b', det).textContent = name; $('.dh .dm', det).textContent = ver[name] || '';
      const run = $('.lu', r).textContent === 'today';
      const chip = $('.dh .chip', det); chip.textContent = run ? 'Running' : 'Not running'; chip.className = run ? 'chip ok' : 'chip';
      $('.tot .num', det).textContent = fmt(size);
      $('.sbar i', det).style.width = (appMb / size * 100).toFixed(0) + '%'; $('.sbar b', det).style.width = (data / size * 100).toFixed(0) + '%';
      $('.lg2 .c1', det).textContent = 'App ' + fmt(appMb); $('.lg2 .c2', det).textContent = 'Data ' + fmt(data);
      const parts = [[name + '.app', appMb], [`Containers/${id}`, data * 0.7], [`Caches/${id}`, data * 0.15], [`Application Support/${name}`, data * 0.1], [`Logs/${name}`, data * 0.04], [`Preferences/${id}.plist`, 0.004]];
      $$('.fi', det).forEach(f => f.remove());
      const anchor = $('.db', det);
      parts.forEach(([n, s]) => anchor.before(el('div', 'fi', `<span class="ck">${ICON('M5 12.5l4.5 4.5L19 7')}</span><span>${n}</span><span class="num dm">${s < 1 ? '4.00 KB' : fmt(s)}</span>`)));
      $('.fh2 span', det).textContent = `Moves to Trash · ${parts.length} of ${parts.length}`;
      $('.db .act', det).textContent = run ? 'Quit and uninstall' : 'Uninstall';
    };
    $$('.arw', p).forEach(r => r.addEventListener('click', e => { if (e.target.closest('.ck')) { const c = e.target.closest('.ck'); c.classList.toggle('off'); c.innerHTML = c.classList.contains('off') ? '' : ICON('M5 12.5l4.5 4.5L19 7'); return; } pick(r); }));
    det.addEventListener('click', e => { const c = e.target.closest('.fi .ck'); if (c) { c.classList.toggle('off'); c.innerHTML = c.classList.contains('off') ? '' : ICON('M5 12.5l4.5 4.5L19 7'); } });
    $('.db .act', det).addEventListener('click', async () => {
      const r = $('.arw.on', p); if (!r || r.dataset.busy) return; r.dataset.busy = 1;
      r.classList.add('gone'); $('.db .act', det).textContent = 'Removed'; await sleep(2200);
      r.classList.remove('gone'); delete r.dataset.busy; pick(r);
    });
  })();

  // ---------- Startup ----------
  (() => {
    const p = pane('startup'); if (!p) return;
    const btn = $('.sban .act', p), rows = $$('.str', p), seg = $('.tb .seg', p);
    const off = r => $('.tg', r).classList.contains('off');
    let filt = 'All';
    const filter = () => {
      rows.forEach(r => r.hidden = !(filt === 'All' || (filt === 'Suggested' && r.classList.contains('sug')) || (filt === 'Flagged' && r.dataset.f) || (filt === 'Off' && off(r))));
      $$('.sgh', p).forEach(h => { let n = h.nextElementSibling, any = false; while (n && n.classList.contains('str')) { if (!n.hidden) any = true; n = n.nextElementSibling; } h.hidden = !any; });
    };
    const upd = () => {
      const sug = rows.filter(r => r.classList.contains('sug') && !off(r)).length;
      btn.textContent = sug ? `Turn off ${sug}` : 'Done'; btn.classList.toggle('dim', !sug);
      $('.sban b', p).textContent = sug ? `${sug} item${sug > 1 ? 's' : ''} you likely don’t need` : 'Nothing left to turn off';
      rows.forEach(r => r.classList.toggle('offd', off(r)));
      const lab = [`All ${rows.length}`, `Suggested ${rows.filter(r => r.classList.contains('sug')).length}`, `Flagged ${rows.filter(r => r.dataset.f).length}`, `Off ${rows.filter(off).length}`];
      [...seg.children].forEach((c, i) => { c.textContent = lab[i]; });
    };
    seg.addEventListener('click', e => {
      const o = e.target.closest('i'); if (!o) return;
      const cur = $('b', seg); cur.replaceWith(el('i', null, cur.innerHTML)); const nb = el('b', null, o.innerHTML); o.replaceWith(nb);
      filt = nb.textContent.split(' ')[0]; filter();
    });
    p.addEventListener('flip', upd);
    btn.addEventListener('click', () => { rows.filter(r => r.classList.contains('sug')).forEach(r => $('.tg', r).classList.add('off')); upd(); });
    upd();
  })();

  // ---------- Mac Health: speed test, devices ----------
  (() => {
    const p = pane('health'); if (!p) return;
    const wf = $('.card.wf', p); if (!wf) return;
    const st = el('span', 'btn2 stb', 'Speed test'); $('.kh', wf).append(st);
    const wv = $('.wv', wf), keep = wv.innerHTML;
    st.addEventListener('click', async () => {
      if (st.dataset.busy) return; st.dataset.busy = 1; st.textContent = 'Testing…';
      wv.innerHTML = '<span><b class="num" id="sp-d">0</b><span class="dm">Mbps down</span></span><span><b class="num cy" id="sp-u">0</b><span class="dm">Mbps up</span></span><span><b class="num" id="sp-p">–</b><span class="dm">ms ping</span></span>';
      await sleep(400); $('#sp-p', wv).textContent = '14';
      const run = async (id, top, ms) => { const t0 = performance.now(); for (;;) { const k = Math.min(1, (performance.now() - t0) / ms); $(id, wv).textContent = Math.round(top * (1 - Math.pow(1 - k, 3)) * (0.94 + Math.random() * 0.06)); if (k >= 1) break; await sleep(60); } };
      await run('#sp-d', 482, 2200); await run('#sp-u', 96, 1600);
      st.textContent = 'Again'; delete st.dataset.busy;
    });
    const dev = $('.tb .btn2', p), grid = $('.hg', p);
    dev.addEventListener('click', () => {
      let sh = $('.devs', p);
      if (sh) { sh.remove(); dev.textContent = 'Devices…'; return; }
      const d = (n, v, extra) => `<div class="dv"><b>${n}</b>${extra || '<span></span>'}<span class="dvb"><i style="width:${v}%"></i></span><span class="num">${v}%</span></div>`;
      sh = el('div', 'card devs', `<div class="kh"><b>Devices</b><span class="dm">Bluetooth</span></div>${d('AirPods Pro · Left', 82)}${d('AirPods Pro · Right', 79)}${d('AirPods Pro · Case', 44, '<span class="chip ok">Charging</span>')}${d('Magic Keyboard', 63)}${d('MX Master 3S', 18, '<span class="chip warn">Low</span>')}${d('DualSense', 71)}`);
      grid.append(sh); dev.textContent = 'Close';
    });
  })();

  // ---------- AI Tool ----------
  (() => {
    const p = pane('claude'); if (!p) return;
    const cards = $$('.pv', p), chart = $('.ab', p), line = $('.ah', p), today = $('.tb .dm', p);
    const names = cards.map(c => $('.kh b', c).textContent.split(' ')[0]);
    p.addEventListener('pick', e => {
      const k = e.detail; let sum = 0;
      cards.forEach((c, i) => { const on = k === 'All' || names[i] === k; c.hidden = !on; if (on) sum += parseFloat(/\$([\d.]+) today/.exec($('.pf', c).textContent)[1]); });
      today.textContent = '$' + sum.toFixed(2) + ' today';
      p.querySelector('.ag').classList.toggle('one', k !== 'All');
      $$('i', chart).forEach((b, j) => { const base = parseFloat(b.dataset.h || (b.dataset.h = b.style.height)); b.style.height = (k === 'All' ? base : Math.max(6, base * (0.3 + ((j * 7 + k.length * 13) % 10) / 14))).toFixed(0) + '%'; });
    });
    const start = new Date(2026, 9, 14);
    const bars = $$('i', chart);
    bars.forEach((b, j) => b.addEventListener('mouseenter', () => {
      bars.forEach(x => x.classList.toggle('on', x === b));
      const d = new Date(start); d.setDate(d.getDate() + j);
      const h = parseFloat(b.style.height), usd = (h * 0.68).toFixed(2), tok = (h * 0.11).toFixed(1);
      line.innerHTML = `<b>${d.toLocaleDateString('en-US', { weekday: 'short', month: 'short', day: 'numeric' }).replace(',', '')}</b> $${usd} · ${tok}M tok · ${Math.round(h * 2.2)} req`;
    }));
  })();

  // ---------- Fan Control ----------
  (() => {
    const p = pane('fans'); if (!p) return;
    const seg = el('span', 'seg sm', '<b>Auto</b><i>Quiet</i><i>Fixed</i><i>Max</i>'); $('.tb', p).append(seg);
    const spin = $('.spin', p);
    p.addEventListener('pick', e => {
      const t = { Auto: 0, Quiet: 1500, Fixed: 3200, Max: 5800 }[e.detail];
      window.cmFanTarget = t || undefined; if (window.cmFanTick) window.cmFanTick();
      spin.style.animationDuration = t ? (6000 / t).toFixed(2) + 's' : '';
    });
  })();

  // ---------- Screenshot: draw on the shot ----------
  (() => {
    const p = pane('capture'); if (!p) return;
    const shot = $('.shot', p), tools = $$('.tls i', p), status = $('.sbar2 span', p);
    const NS = 'http://www.w3.org/2000/svg';
    const ink = document.createElementNS(NS, 'svg'); ink.setAttribute('class', 'ink'); shot.append(ink);
    const kinds = ['move', 'arrow', 'box', 'line', 'pen', 'blur', 'step', 'crop'];
    let tool = 'arrow', marks = 7, step = 3;
    const pickTool = i => { tool = kinds[i] || 'arrow'; tools.forEach((t, k) => t.classList.toggle('on', k === i)); };
    tools.forEach((t, i) => t.addEventListener('click', () => pickTool(i)));
    pickTool(1);
    const pt = e => { const r = shot.getBoundingClientRect(); return [e.clientX - r.left, e.clientY - r.top]; };
    const count = () => { marks++; status.textContent = status.textContent.replace(/\d+ marks/, marks + ' marks'); };
    let cur = null, o = null;
    shot.addEventListener('pointerdown', e => {
      if (tool === 'move' || tool === 'crop') return;
      e.preventDefault(); shot.setPointerCapture(e.pointerId); o = pt(e);
      if (tool === 'step') { const g = document.createElementNS(NS, 'g'); g.innerHTML = `<circle cx="${o[0]}" cy="${o[1]}" r="11" fill="#FF3B30"/><text x="${o[0]}" y="${o[1] + 4}" text-anchor="middle" fill="#fff" font-size="12" font-weight="600">${step++}</text>`; ink.append(g); count(); o = null; return; }
      cur = document.createElementNS(NS, tool === 'pen' ? 'polyline' : tool === 'box' || tool === 'blur' ? 'rect' : 'path');
      cur.setAttribute('class', tool === 'blur' ? 'bl' : 'mk'); if (tool === 'pen') cur.setAttribute('points', o.join(','));
      ink.append(cur);
    });
    shot.addEventListener('pointermove', e => {
      if (!cur) return; const [x, y] = pt(e), [a, b] = o;
      if (tool === 'pen') cur.setAttribute('points', cur.getAttribute('points') + ` ${x},${y}`);
      else if (tool === 'box' || tool === 'blur') { cur.setAttribute('x', Math.min(a, x)); cur.setAttribute('y', Math.min(b, y)); cur.setAttribute('width', Math.abs(x - a)); cur.setAttribute('height', Math.abs(y - b)); }
      else if (tool === 'line') cur.setAttribute('d', `M${a} ${b}L${x} ${y}`);
      else { const ang = Math.atan2(y - b, x - a), h = 12; cur.setAttribute('d', `M${a} ${b}L${x} ${y}M${x} ${y}L${x - h * Math.cos(ang - 0.5)} ${y - h * Math.sin(ang - 0.5)}M${x} ${y}L${x - h * Math.cos(ang + 0.5)} ${y - h * Math.sin(ang + 0.5)}`); }
    });
    const end = () => { if (cur) { count(); cur = null; } };
    shot.addEventListener('pointerup', end); shot.addEventListener('pointercancel', end);
    const btn = (t, f) => { const b = $$('.ctb .btn2, .ctb .act', p).find(x => x.textContent.includes(t)); if (b) b.addEventListener('click', f); };
    const flash = async (b, t) => { const o = b.textContent; b.textContent = t; await sleep(1200); b.textContent = o; };
    btn('Copy', e => flash(e.currentTarget, 'Copied'));
    btn('Save', e => flash(e.currentTarget, 'Saved'));
    btn('Pin', e => { shot.classList.toggle('pinned'); });
    document.addEventListener('keydown', e => { if ((e.metaKey || e.ctrlKey) && e.key === 'z' && !pane('capture').hidden && ink.lastChild) { e.preventDefault(); ink.lastChild.remove(); marks--; status.textContent = status.textContent.replace(/\d+ marks/, marks + ' marks'); } });
  })();

  // ---------- Video: record ----------
  (() => {
    const p = pane('video'); if (!p) return;
    const rec = $('.tb .act', p), list = $('.vl', p), sub = $('.tb .dm', p);
    $$('.vrc .chip', p).forEach(c => c.addEventListener('click', () => c.classList.toggle('ok')));
    let t = null, s = 0;
    rec.addEventListener('click', () => {
      if (!t) {
        s = 0; rec.classList.add('recn'); rec.textContent = 'Stop · 0:00';
        t = setInterval(() => { s++; rec.textContent = `Stop · ${Math.floor(s / 60)}:${String(s % 60).padStart(2, '0')}`; }, 1000);
        return;
      }
      clearInterval(t); t = null; rec.classList.remove('recn'); rec.textContent = 'Record screen ⇧⌘0';
      const d = `${Math.floor(s / 60)}:${String(Math.max(1, s) % 60).padStart(2, '0')}`, now = new Date().toLocaleTimeString('en-GB', { hour: '2-digit', minute: '2-digit' });
      const area = $('.vrc .seg b', p).textContent;
      const r = el('div', 'vr new', `<span class="vt"><i></i><em class="num">${d}</em></span><div class="cn"><b>${area} recording</b><span class="dm num">Today ${now} · ${fmt(Math.max(1, s) * 0.45)}</span></div><span></span><span class="btn2">Share</span>`);
      $('.vrc', p).after(r); sub.textContent = `${$$('.vr', p).length} recordings`;
    });
    list.addEventListener('click', async e => {
      const b = e.target.closest('.vr .btn2'); if (!b || b.dataset.busy) return; b.dataset.busy = 1;
      b.textContent = 'Uploading…'; await sleep(1100); b.textContent = 'Link copied';
      const chip = b.previousElementSibling; if (chip && !chip.textContent) { chip.className = 'chip ok'; chip.textContent = 'Shared'; }
      await sleep(1500); b.textContent = 'Share'; delete b.dataset.busy;
    });
  })();

  // ---------- Clipboard ----------
  (() => {
    const p = pane('clipboard'); if (!p) return;
    const kind = r => { const t = $('.t', r).textContent; return $('.sw2.im', r) ? 'Images' : $('.sw2', r) ? 'Colors' : $('.mono', r) ? 'Code' : /\.(pdf|zip|png)$/.test(t) ? 'Files' : /^[\w.-]+\.[a-z]{2,}\//.test(t) ? 'Links' : 'Text'; };
    const rows = $$('.cpr', p); rows.forEach(r => { r.dataset.k = kind(r); r.dataset.t = $('.t', r).textContent; });
    const s = $('.srch2', p); const inp = el('input', 'srch-in'); inp.type = 'search'; inp.placeholder = 'Search'; inp.setAttribute('aria-label', 'Search clipboard');
    s.lastChild.remove(); s.append(inp);
    let filt = 'All';
    const apply = () => {
      const q = inp.value.trim().toLowerCase();
      rows.forEach(r => {
        const okK = filt === 'All' || (filt === 'Pinned' ? !!$('.pn', r) : r.dataset.k === filt);
        const okQ = !q || r.dataset.t.toLowerCase().includes(q);
        r.hidden = !(okK && okQ);
        $('.t', r).innerHTML = q && okQ ? r.dataset.t.replace(new RegExp(q.replace(/[.*+?^${}()|[\]\\]/g, '\\$&'), 'ig'), m => `<mark>${m}</mark>`) : r.dataset.t;
      });
      const cur = rows.find(r => r.classList.contains('on'));
      if (!cur || cur.hidden) { const f = rows.find(r => !r.hidden); if (f) show(f); else { rows.forEach(r => r.classList.remove('on')); det.innerHTML = ''; } }
      $$('.ch', p).forEach(h => { let n = h.nextElementSibling, any = false; while (n && !n.classList.contains('ch')) { if (!n.hidden) any = true; n = n.nextElementSibling; } h.hidden = !any; });
    };
    inp.addEventListener('input', apply);
    $$('.fc', p).forEach(f => f.addEventListener('click', () => { $$('.fc', p).forEach(x => x.classList.toggle('on', x === f)); filt = f.textContent.trim(); apply(); }));
    const det = $('.cdt', p);
    const show = r => {
      rows.forEach(x => x.classList.toggle('on', x === r));
      const k = r.dataset.k, t = r.dataset.t, src = $('.src', r).textContent, at = $('.num', r).textContent;
      const label = { Links: 'Link', Code: 'Code', Colors: 'Color', Images: 'Image', Files: 'File', Text: 'Text' }[k];
      const big = k === 'Colors' ? `<div class="og" style="background:${t}"></div>` : k === 'Images' ? '<div class="og im2"></div>' : k === 'Links' ? `<div class="og"><span>${t[0].toUpperCase()}</span></div>` : `<pre class="cpre">${t}</pre>`;
      det.innerHTML = `<div class="kh"><span><i class="ld"></i>${label}</span><span class="dm">${at}</span></div>${big}${k === 'Links' ? `<b>${t.split('/')[0]}</b><span class="url">https://${t}</span>` : ''}<div class="kv2"><span class="dm">From</span><span>${src}</span><span class="dm">Copied</span><span>once</span><span class="dm">Content</span><span>${t.length} characters</span></div><div class="cb2"><span class="act">Copy</span><span class="btn2">Copy as plain text</span>${k === 'Links' ? '<span class="btn2">Open link</span>' : ''}<span class="btn2">${$('.pn', r) ? 'Unpin' : 'Pin'}</span></div>`;
    };
    rows.forEach(r => r.addEventListener('click', () => show(r)));
    det.addEventListener('click', async e => { const b = e.target.closest('.act, .btn2'); if (!b) return; const o = b.textContent; if (/Copy/.test(o)) { b.textContent = 'Copied'; await sleep(1000); b.textContent = o; return; }
      const r = rows.find(x => x.classList.contains('on')); if (!r || !/pin/i.test(o)) return;
      const pn = $('.pn', r);
      if (pn) pn.remove(); else { const w = el('span', null, ICON('M9 3h6l-1 6 3.5 3.5h-11L10 9zM12 12.5V21')); const ic = w.firstElementChild; ic.classList.add('pn'); $('.t', r).after(ic); }
      show(r); apply();
    });
    const pause = $$('.tb .btn2', p).find(b => /Pause/.test(b.textContent)), st = $('.tb .dm', p);
    if (pause) pause.addEventListener('click', () => { const on = pause.textContent.startsWith('Pause'); pause.textContent = on ? 'Resume' : 'Pause 15 min'; st.textContent = on ? 'Paused · 15 min left' : 'Recording · 8 items · 2 pinned'; });
  })();

  // ---------- Dictation: ⌥D or the mic ----------
  (() => {
    const p = pane('dictation'); if (!p) return;
    const mic = el('span', 'act sm mic', ICON('M9 4.5a3 3 0 0 1 6 0v6.5a3 3 0 0 1-6 0zM5.5 11a6.5 6.5 0 0 0 13 0M12 17.5V21') + '<span>⌥D</span>');
    $('.tb', p).append(mic);
    const lines = ['Сьогодні після обіду викладемо нову версію на сайт.', 'Remind me to answer Alex about the release notes tomorrow morning.', 'Додай у README розділ про встановлення.'];
    let n = 0, busy = false;
    const go = async () => {
      if (busy) return; busy = true; window.cmShow && window.cmShow('dictation');
      mic.classList.add('recn'); mic.lastChild.textContent = 'Listening';
      const row = el('div', 'dcr new', `<div><span class="dm">now · Notes · 3 s</span><p><span class="wave"><i></i><i></i><i></i><i></i><i></i></span></p></div><span class="btn2">Copy</span>`);
      $('.dcl', p).prepend(row); await sleep(1500);
      mic.lastChild.textContent = 'Typing';
      const pEl = $('p', row), text = lines[n++ % lines.length]; pEl.textContent = '';
      for (const ch of text) { pEl.textContent += ch; await sleep(22); }
      mic.classList.remove('recn'); mic.lastChild.textContent = '⌥D';
      const sub = $('.tb .dm', p); sub.textContent = sub.textContent.replace(/(\d+) words/, (m, k) => (+k + text.split(/\s+/).length) + ' words');
      busy = false;
    };
    mic.addEventListener('click', go);
    document.addEventListener('keydown', e => { if (e.altKey && e.code === 'KeyD') { const r = app.getBoundingClientRect(); if (r.top < innerHeight && r.bottom > 0) { e.preventDefault(); go(); } } });
    p.addEventListener('click', async e => { const b = e.target.closest('.dcr .btn2'); if (!b) return; b.textContent = 'Copied'; await sleep(1000); b.textContent = 'Copy'; });
  })();

  // ---------- Keep Awake ----------
  (() => {
    const p = pane('awake'); if (!p) return;
    const big = $('.tg.big', p), h = $('.aw-h', p), ring = $('.ahero .ring circle:last-child', p), why = $('.ahero span', p);
    const cl = $$('.opt .or', p).find(r => /while Claude/.test(r.textContent));
    const set = () => {
      const man = !big.classList.contains('off'), ai = cl && !$('.tg', cl).classList.contains('off');
      h.textContent = man ? 'Awake' : ai ? 'Awake for Claude' : 'Asleep';
      why.textContent = man ? 'Until you turn it off' : ai ? 'Claude is working · 2 sessions' : 'Your Mac sleeps as usual';
      ring.setAttribute('stroke-dashoffset', man ? '0' : ai ? '96.1' : '282.7');
    };
    big.addEventListener('flip', set); if (cl) cl.addEventListener('flip', set);
  })();

  // ---------- Displays: drag the sliders ----------
  (() => {
    const p = pane('displays'); if (!p) return;
    $$('.dsl', p).forEach(s => {
      const tr = $('.dtr', s), b = $('b', tr), v = $('.num', s);
      const at = e => { const r = tr.getBoundingClientRect(); const k = Math.max(0, Math.min(1, (e.clientX - r.left) / r.width)); b.style.width = (k * 100) + '%'; v.textContent = Math.round(k * 100) + '%'; };
      s.addEventListener('pointerdown', e => { s.setPointerCapture(e.pointerId); at(e); const mv = ev => at(ev), up = () => { s.removeEventListener('pointermove', mv); s.removeEventListener('pointerup', up); }; s.addEventListener('pointermove', mv); s.addEventListener('pointerup', up); });
    });
  })();


  // One bar: the menu bar icons jump into Cmd+Meow, each becomes a live widget
  (() => {
    const bars = document.getElementById('bars'); if (!bars) return;
    const n = document.getElementById('bars-n'), ap = $$('.mi2.ap', bars), wg = $$('.wg', bars);
    ap.forEach((ic, k) => ic.style.setProperty('--k', k));
    const wOf = i => wg.find(w => +w.dataset.i === i);
    const label = async t => { n.classList.add('flip'); await sleep(300); n.textContent = t; n.classList.remove('flip'); };
    const reset = () => { bars.className = 'bars'; ap.forEach(ic => ic.classList.remove('out')); wg.forEach(w => w.classList.remove('on')); n.textContent = '11 apps'; };
    if (matchMedia('(prefers-reduced-motion: reduce)').matches) { bars.classList.add('b-one', 'b-open'); ap.forEach(ic => ic.classList.add('out')); wg.forEach(w => w.classList.add('on')); n.textContent = '1 app'; return; }
    const ORDER = [5, 0, 9, 3, 7, 2, 10, 6, 1, 8, 4];
    let gen = 0, on = false;
    const fly = (ic, w, i) => {
      const a = ic.getBoundingClientRect(), b = $('.wi', w).getBoundingClientRect();
      const dx = b.left + b.width / 2 - (a.left + a.width / 2), dy = b.top + b.height / 2 - (a.top + a.height / 2);
      const f = ic.cloneNode(true); f.classList.remove('ap'); f.classList.add('fly'); f.style.left = a.left + 'px'; f.style.top = a.top + 'px'; f.style.width = a.width + 'px';
      document.body.append(f); ic.classList.add('out');
      const side = (i % 2 ? 1 : -1) * (40 + (i * 37) % 70), lift = -18 - (i * 13) % 22, tilt = (i % 2 ? 1 : -1) * (14 + (i * 7) % 16);
      const an = f.animate([
        { transform: 'translate(0,0) scale(1) rotate(0deg)', easing: 'cubic-bezier(.2,.7,.4,1)' },
        { transform: `translate(${dx * 0.35 + side}px, ${lift}px) scale(1.7) rotate(${tilt}deg)`, offset: 0.32, easing: 'cubic-bezier(.5,0,.3,1)' },
        { transform: `translate(${dx}px, ${dy}px) scale(1.05) rotate(0deg)` }
      ], { duration: 820 + (i * 53) % 220, fill: 'forwards' });
      return new Promise(r => { an.onfinish = () => { f.remove(); w.classList.add('on'); r(); }; });
    };
    const play = async id => {
      reset(); await sleep(700); if (id !== gen) return;
      bars.classList.add('b-wig'); await sleep(600); if (id !== gen) return;
      bars.classList.add('b-one'); await sleep(420); bars.classList.add('b-open'); await sleep(380); if (id !== gen) return;
      bars.classList.remove('b-wig');
      const all = []; bars.classList.add('b-eat');
      for (const [k, i] of ORDER.entries()) {
        if (id !== gen) return;
        all.push(fly(ap[i], wOf(i), k));
        await sleep(70 + (k * 29) % 70);
      }
      await Promise.all(all); bars.classList.remove('b-eat'); if (id !== gen) return;
      label('1 app');
      await sleep(2600); if (id !== gen) return;
      bars.classList.remove('b-open'); await sleep(1000); if (id === gen) play(id);
    };
    new IntersectionObserver(es => {
      if (es[0].isIntersecting) { if (!on) { on = true; play(++gen); } }
      else { on = false; gen++; $$('.mi2.fly').forEach(f => f.remove()); reset(); }
    }, { threshold: 0.45 }).observe(bars);
  })();

  document.querySelectorAll('[data-go]').forEach(a => a.addEventListener('click', e => { e.preventDefault(); window.cmShow && window.cmShow(a.dataset.go); app.scrollIntoView({ behavior: 'smooth', block: 'center' }); }));


  // Hero panel: a widget opens its tool in the window
  const go = { Clean: 'cleanup', Screenshot: 'capture', Claude: 'awake', Clipboard: 'clipboard', 'Macintosh HD': 'diskmap', Memory: 'health', CPU: 'health', Battery: 'health', 'Home Wi-Fi': 'health', 'Claude Max': 'claude', Fans: 'fans' };
  document.querySelectorAll('.app.pnl .wd').forEach(w => {
    const k = (w.querySelector('.dm, .kh b, .cl3 > span:last-child') || {}).textContent;
    const to = w.dataset.to || go[(k || '').trim()]; if (!to) return;
    w.classList.add('link'); w.setAttribute('role', 'button'); w.tabIndex = 0;
    const open = () => { window.cmShow && window.cmShow(to); app.scrollIntoView({ behavior: 'smooth', block: 'center' }); };
    w.addEventListener('click', open); w.addEventListener('keydown', e => { if (e.key === 'Enter' || e.key === ' ') { e.preventDefault(); open(); } });
  });
  // Keyboard: span controls in the window act like buttons
  const CTL = '.rb, .act, .btn2, .gch, .seg > i, .tg, .fc, .mic, .stb, .fr .ck';
  const arm = () => $$(CTL, app).forEach(c => {
    if (c.tabIndex < 0) { c.tabIndex = 0; c.setAttribute('role', c.classList.contains('tg') ? 'switch' : 'button'); }
    if (c.classList.contains('tg')) c.setAttribute('aria-checked', String(!c.classList.contains('off')));
  });
  arm(); new MutationObserver(arm).observe(app, { subtree: true, childList: true, attributes: true, attributeFilter: ['class'] });
  app.addEventListener('keydown', e => { if ((e.key === 'Enter' || e.key === ' ') && e.target.matches && e.target.matches(CTL)) { e.preventDefault(); e.target.click(); } });
})();
