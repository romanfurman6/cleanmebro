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
      const keep = (from, to) => { ['tabindex', 'role'].forEach(a => from.hasAttribute(a) && to.setAttribute(a, from.getAttribute(a))); return to; };
      const had = document.activeElement === opt;
      if (cur) { const i = keep(cur, el('i', null, cur.innerHTML)); cur.replaceWith(i); }
      const b = keep(opt, el('b', null, opt.innerHTML)); opt.replaceWith(b); if (had) b.focus();
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
      if (k < 2) { b.classList.add('ok'); setTimeout(() => b.classList.remove('ok'), 1200); }
      if (k === 2) { row.classList.add('gone'); setTimeout(() => { row.remove(); n.textContent = $$('.shr', p).length; }, 250); }
    });
  })();

  window.cmShare = (name, video) => {
    const p = pane('sharing'), list = p && $('.shlist', p); if (!list) return; if ($$('.shr .cn b', list).some(b => b.textContent === name)) return;
    const first = $$('.shr', list).find(r => $('.cn b', r).textContent.endsWith(video ? '.mp4' : '.png')) || $('.shr', list); if (!first) return;
    const r = first.cloneNode(true); r.classList.remove('gone'); r.classList.add('new');
    const d = new Date(), hm = d.toLocaleTimeString('en-GB', { hour: '2-digit', minute: '2-digit' });
    $('.cn b', r).textContent = name; $('.cn code', r).textContent = `Nov 12, ${hm} · cmdmeow.com/${video ? 'play' : 'view'}/#${Math.random().toString(36).slice(2, 7)}`;
    list.prepend(r); $('#sh-n', p).textContent = $$('.shr', p).length;
  };

  // ---------- Cleanup ----------
  (() => {
    const p = pane('cleanup'); if (!p) return;
    const snap = p.innerHTML, badge = $('.nv[data-p=cleanup] .bd');
    const boot = () => {
    const L = $('.cuL', p), sfl = $('.sfl', p), kpl = $('.kpl', p), sel = $('.fb > span:first-child', p), go = $('.fb .act', p), title = $('.tb .dm', p);
    const CK = ICON('M5 12.5l4.5 4.5L19 7');
    const vis = r => !r.hidden;
    const sum = rows => rows.reduce((a, r) => a + parse($('.sz', r).textContent), 0);
    const upd = () => {
      const safe = $$('.sfr', sfl), on = safe.filter(r => vis(r) && !$('.ck', r).classList.contains('off'));
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
      if (go.textContent === 'Done') return rescan();
      if (!on.length || go.dataset.busy) return; go.dataset.busy = 1;
      const freed = sum(on);
      for (const r of on) { r.classList.add('gone'); await sleep(110); }
      await sleep(250); on.forEach(r => r.remove());
      upd(); title.textContent = 'Freed ' + fmt(freed); go.textContent = 'Done'; go.classList.remove('dim');
      const left = Math.max(0, parse(badge.textContent) - freed); badge.textContent = left ? fmt(left) : '';
      delete go.dataset.busy;
    });
    upd();
    $('.tb .btn2', p).addEventListener('click', rescan);
    };
    const rescan = async () => {
      if ($('.cscan', p)) return;
      const ov = el('div', 'cscan', '<div><div>Looking for junk</div><div class="pg"><i></i></div></div>'); p.append(ov);
      await sleep(30); $('.pg i', ov).style.width = '100%'; await sleep(1400);
      p.innerHTML = snap; badge.textContent = '18.3 GB'; boot();
    };
    boot();
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
    const body = $('.dm-split', p), intro = el('div', 'pb dmi', '<div class="card"><b>Map Home</b><div class="pg" hidden><i></i></div><div class="row2"><span class="act">Map disk</span><span class="btn2">Choose Folder…</span></div></div>'); intro.hidden = true; body.after(intro);
    const map = async () => { const pg = $('.pg', intro); pg.hidden = false; $('b', intro).textContent = 'Mapping Home'; await sleep(30); $('i', pg).style.width = '100%'; await sleep(1300);
      intro.hidden = true; body.hidden = foot.hidden = false; pg.hidden = true; $('i', pg).style.width = '0'; $('b', intro).textContent = 'Map Home'; path = [root]; draw(); };
    $$('.act, .btn2', intro).forEach(x => x.addEventListener('click', map));
    back.addEventListener('click', () => {
      if (path.length > 1) { path = path.slice(0, -1); return draw(); }
      body.hidden = foot.hidden = true; intro.hidden = false; hdr.textContent = 'Home'; back.textContent = 'Start over';
    });
    $('.fb .btn2', p).addEventListener('click', () => { picked.clear(); draw(); });
    const trash = $('.fb .act', p);
    if (trash) trash.addEventListener('click', async () => {
      if (!picked.size || trash.dataset.busy) return; trash.dataset.busy = 1; const o = trash.textContent; trash.textContent = 'Moved to Trash';
      const prune = n => { if (!n.c) return 0; let cut = 0; n.c = n.c.filter(c => picked.has(c) ? (cut += c.s, false) : true); n.c.forEach(c => { const d = prune(c); c.s -= d; cut += d; }); return cut; };
      root.s -= prune(root); picked.clear(); { const k = path.findIndex((n, i) => i && !path[i - 1].c.includes(n)); if (k > 0) path = path.slice(0, k); } draw(); await sleep(1200); trash.textContent = o; delete trash.dataset.busy;
    });
    draw();
  })();

  // ---------- Uninstaller ----------
  (() => {
    const p = pane('uninstall'); if (!p) return;
    const det = $('.det', p), files = $$('.fi', det).map(f => f.outerHTML);
    const ver = { 'Android Studio': '2024.2 · com.google.android.studio', 'CleanShot X': '4.7 · pl.maketheweb.cleanshotx', Docker: '4.36.0 · com.docker.docker', Figma: '124.5 · com.figma.Desktop', 'Final Cut Pro': '11.0 · com.apple.FinalCut', Slack: '4.41 · com.tinyspeck.slackmacgap', Spotify: '1.2.50 · com.spotify.client', VLC: '3.0.21 · org.videolan.vlc', Xcode: '16.1 · com.apple.dt.Xcode', Zoom: '6.2.10 · us.zoom.xos' };
    const pick = r => {
      det.style.visibility = '';
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
    const sub = $('#un-sub', p), badge = $('.nv[data-p=uninstall] .bd');
    const count = () => { const rows = $$('.arw', p); sub.textContent = `${rows.length} apps · ${fmt(rows.reduce((a, r) => a + parse($('.num', r).textContent), 0))}`; badge.textContent = rows.length; };
    const remove = rows => { rows.forEach(r => r.classList.add('out')); setTimeout(() => { const next = $$('.arw', p).find(x => !rows.includes(x) && !x.hidden); rows.forEach(r => r.remove()); det.style.visibility = next ? '' : 'hidden'; if (next) pick(next); count(); bar(); }, 260); };
    $('.db .act', det).addEventListener('click', () => { const r = $('.arw.on', p); if (r) remove([r]); });
    // Reset…: keeps the app, removes its data
    $('.db .btn2', det).addEventListener('click', () => {
      const r = $('.arw.on', p); if (!r) return; const app = $$('.fi', det)[0]; const keep = parse($('.num', app).textContent);
      $$('.fi', det).slice(1).forEach(f => f.remove()); $('.num', r).textContent = fmt(keep); $('.tot .num', det).textContent = fmt(keep);
      $('.sbar i', det).style.width = '100%'; $('.sbar b', det).style.width = '0'; $('.lg2 .c2', det).textContent = 'Data 0 MB'; $('.fh2 span', det).textContent = 'Moves to Trash · 1 of 1'; count();
    });
    // ticked apps: Review N apps… removes them together
    const list = $('.al', p), ub = el('div', 'ubar', '<span class="act"></span>'); ub.hidden = true; list.append(ub);
    const bar = () => { const n = $$('.arw .ck:not(.off)', p).length; ub.hidden = !n; $('.act', ub).textContent = `Review ${n} app${n === 1 ? '' : 's'}…`; };
    list.addEventListener('click', e => { if (e.target.closest('.arw .ck')) setTimeout(bar); });
    $('.act', ub).addEventListener('click', () => remove($$('.arw', p).filter(r => !$('.ck', r).classList.contains('off'))));
    // search, filters, sort
    const inp = $('.ufl .srch-in', p); let f = 'All', sort = 'Size';
    const age = r => { const t = $('.lu', r).textContent; return /year/.test(t) ? 400 : /month/.test(t) ? parseInt(t) * 30 : /day/.test(t) ? parseInt(t) : t === 'yesterday' ? 1 : 0; };
    const apply = () => { const q = inp.value.trim().toLowerCase(); $$('.arw', p).forEach(r => { const name = $('.an', r).textContent, tag = ($('.an i', r) || {}).textContent || '';
      const ok = f === 'All' || (f === 'Unused 90+ days' ? age(r) >= 90 : f === 'Large' ? parse($('.num', r).textContent) >= 1000 : tag === f); r.hidden = !(ok && (!q || name.toLowerCase().includes(q))); }); };
    inp.addEventListener('input', apply);
    $$('.ufl .fc', p).forEach(x => x.addEventListener('click', () => { $$('.ufl .fc', p).forEach(y => y.classList.toggle('on', y === x)); f = x.textContent; apply(); }));
    $('.usort', p).addEventListener('click', e => { sort = { Size: 'Name', Name: 'Last used', 'Last used': 'Size' }[sort]; e.currentTarget.textContent = sort;
      const rows = $$('.arw', p); rows.sort((a, b) => sort === 'Name' ? $('.an', a).textContent.localeCompare($('.an', b).textContent) : sort === 'Size' ? parse($('.num', b).textContent) - parse($('.num', a).textContent) : age(a) - age(b)); rows.forEach(r => ub.before(r)); });
    // Apps | Leftovers
    const apps = [$('.ufl', p), $('.un', p)], lo = $('.ulo', p);
    $('.tb .seg', p).addEventListener('click', e => { const o = e.target.closest('i'); if (!o) return; const cur = $('b', o.parentElement); cur.replaceWith(el('i', null, cur.innerHTML)); o.replaceWith(el('b', null, o.innerHTML)); const l = o.textContent.startsWith('Leftovers'); apps.forEach(x => x.hidden = l); lo.hidden = !l; sub.textContent = l ? 'Data left behind by apps that are no longer installed' : ''; if (!l) count(); });
    const loUpd = () => { const on = $$('.lor', lo).filter(r => !$('.ck', r).classList.contains('off')), tot = $$('.lor', lo).reduce((a, r) => a + parse($('.num', r).textContent), 0);
      $('#lo-n', lo).textContent = `${on.length} selected · ${fmt(on.reduce((a, r) => a + parse($('.num', r).textContent), 0))}`; const seg = $$('.tb .seg > *', p)[1]; seg.textContent = tot ? 'Leftovers · ' + fmt(tot) : 'Leftovers'; };
    lo.addEventListener('click', e => { const r = e.target.closest('.lor'); if (r) { const c = $('.ck', r); c.classList.toggle('off'); c.innerHTML = c.classList.contains('off') ? '' : ICON('M5 12.5l4.5 4.5L19 7'); loUpd(); } });
    $('.fb2 .act', lo).addEventListener('click', () => { const on = $$('.lor', lo).filter(r => !$('.ck', r).classList.contains('off')); on.forEach(r => r.classList.add('out')); setTimeout(() => { on.forEach(r => r.remove()); loUpd(); }, 260); });
    $('.sti', p).addEventListener('click', () => window.cmShow && window.cmShow('startup'));
  })();

  // ---------- Startup ----------
  (() => {
    const p = pane('startup'); if (!p) return;
    const btn = $('.sban .act', p), rows = $$('.str', p), seg = $('.tb .seg', p), sub = $('#st-sub', p), badge = $('.nv[data-p=startup] .bd');
    const off = r => $('.tg', r).classList.contains('off');
    let filt = 'All';
    const filter = () => {
      rows.forEach(r => r.hidden = !(filt === 'All' || (filt === 'Suggested' && r.classList.contains('sug')) || (filt === 'Flagged' && r.dataset.f) || (filt === 'Off' && off(r))));
      $$('.sgh', p).forEach(h => { let n = h.nextElementSibling, any = false; while (n && n.classList.contains('str')) { if (!n.hidden) any = true; n = n.nextElementSibling; } h.hidden = !any; });
    };
    const upd = () => {
      const sug = rows.filter(r => r.classList.contains('sug') && !off(r)).length;
      btn.textContent = sug ? `Turn off ${sug}` : 'Done'; btn.classList.toggle('dim', !sug);
      $('.sban b', p).textContent = sug ? `${sug} item${sug > 1 ? 's' : ''} you likely don’t need` : 'Nothing left to turn off'; $('.sban .dm', p).hidden = !sug;
      rows.forEach(r => r.classList.toggle('offd', off(r)));
      const lab = [`All ${rows.length}`, `Suggested ${rows.filter(r => r.classList.contains('sug')).length}`, `Flagged ${rows.filter(r => r.dataset.f).length}`, `Off ${rows.filter(off).length}`];
      [...seg.children].forEach((c, i) => { c.textContent = lab[i]; });
      const run = rows.filter(r => !off(r) && /running/.test($('.stt', r).textContent)).length;
      sub.textContent = `${rows.length} things start with your Mac · ${run} running now`; if (badge) badge.textContent = sug || '';
    };
    seg.addEventListener('click', e => {
      const o = e.target.closest('i'); if (!o) return;
      const cur = $('b', seg); cur.replaceWith(el('i', null, cur.innerHTML)); const nb = el('b', null, o.innerHTML); o.replaceWith(nb);
      filt = nb.textContent.split(' ')[0]; filter();
    });
    p.addEventListener('flip', upd);
    // More: Show in Finder, Copy label, Trust, Move file to Trash…
    const close = () => { $$('.smn', p).forEach(m => m.remove()); $$('.smo.on', p).forEach(x => x.classList.remove('on')); };
    p.addEventListener('click', e => {
      const mo = e.target.closest('.smo');
      if (mo) { const open = mo.classList.contains('on'); close(); if (open) return; const r = mo.closest('.str'); mo.classList.add('on');
        r.append(el('div', 'smn', `<span data-k="f">Show in Finder</span><span data-k="c">Copy label</span>${r.dataset.f ? '<span data-k="t">Trust</span>' : ''}<hr><span data-k="x" class="bad">Move file to Trash…</span>`)); e.stopPropagation(); return; }
      const it = e.target.closest('.smn span');
      if (it) { const r = it.closest('.str'), k = it.dataset.k; close();
        if (k === 't') { delete r.dataset.f; $$('.cn em', r).forEach(x => { if (!x.classList.contains('sug')) x.remove(); }); upd(); filter(); }
        if (k === 'x') { r.classList.add('out'); setTimeout(() => { r.remove(); rows.splice(rows.indexOf(r), 1); upd(); filter(); }, 250); }
        return; }
      if (!e.target.closest('.smn')) close();
    });
    btn.addEventListener('click', () => { rows.filter(r => r.classList.contains('sug')).forEach(r => $('.tg', r).classList.add('off')); upd(); });
    upd();
  })();

  // ---------- Mac Health: Overview | Settings, speed tests, hide a device ----------
  (() => {
    const p = pane('health'); if (!p) return;
    const grid = $('.hg', p), set = $('.hset', p);
    $('.tb .seg', p).addEventListener('click', e => {
      const o = e.target.closest('i'); if (!o) return;
      const cur = $('b', o.parentElement); cur.replaceWith(el('i', null, cur.innerHTML)); o.replaceWith(el('b', null, o.innerHTML));
      const s2 = o.textContent === 'Settings'; grid.hidden = s2; set.hidden = !s2;
    });
    // Charged alert: the 80/90/100 picker only while it is on
    const ca = $('.hca', p), catg = ca.nextElementSibling;
    catg.addEventListener('click', () => setTimeout(() => { ca.style.visibility = catg.classList.contains('off') ? 'hidden' : ''; }));
    // Quick / Full test: the Speed line counts up, then shows the result
    const spl = $('#nw-sp', p), btns = $$('.spt', p);
    btns.forEach(b => b.addEventListener('click', async () => {
      if (p.dataset.sp) return; p.dataset.sp = 1; btns.forEach(x => x.classList.add('dis'));
      const ms = b.dataset.m === 'f' ? 4200 : 2200, t0 = performance.now();
      for (;;) { const k = Math.min(1, (performance.now() - t0) / ms); spl.textContent = `Testing… ${Math.round(k * 100)}%`; if (k >= 1) break; await sleep(80); }
      const d = b.dataset.m === 'f' ? 486 : 471, u = b.dataset.m === 'f' ? 98 : 94;
      spl.textContent = `${d} down · ${u} up Mbps`; delete p.dataset.sp; btns.forEach(x => x.classList.remove('dis'));
    }));
    // ··· on a device hides it; Settings lists it under Hidden devices with Show
    const chip = $('.hdev .chip', p), hid = $('.hhl', p), hidT = $('.hhid', p);
    const note = () => { const rows = $$('.hdv', grid), low = rows.filter(r => r.classList.contains('low')).length; chip.textContent = low ? `${low} low` : `${rows.length} connected`; chip.className = 'chip ' + (low ? 'warn' : 'ok'); hid.hidden = hidT.hidden = !hid.children.length; };
    grid.addEventListener('click', e => {
      const d = e.target.closest('.hdv .dots'); if (!d) return;
      const row = d.closest('.hdv'); row.classList.add('out');
      setTimeout(() => {
        row.remove();
        const h = el('div', null, `<span>${$('.cn b', row).textContent}</span><span class="btn2">Show</span>`);
        $('.btn2', h).addEventListener('click', () => { row.classList.remove('out'); $('.hdl', grid).append(row); h.remove(); note(); });
        hid.append(h); note();
      }, 260);
    });
    // Review Cleanup / Free up space open the tool, like the app
    $$('[data-go2]', p).forEach(b => b.addEventListener('click', () => window.cmShow && window.cmShow(b.dataset.go2)));
  })();

  // ---------- AI Tool: All (a card per tool) or one tool (limits, Saved, chart, Models, Projects) ----------
  (() => {
    const p = pane('claude'); if (!p) return;
    const all = $('.ag', p), one = $('.a1', p), sub = $('.tb .dm', p), seg = $('.tb .seg', p);
    const T = {
      Claude: { plan: 'Claude Max', fresh: 'Live · 15:20', live: 1, menu: 1, usd: 30.73, tok: '3.4M', d7: 193, t7: '24.1M', t5: '1.1M', t30: '96.4M', req: 412, api30: 1412, paid: 200, data: '3.2 GB',
        lim: [{ l: 'Session', pct: 19, at: '18:10', fc: 'Resets 18:10', pace: 'Room to spare', exp: 34 }, { l: 'Week', pct: 62, at: 'Mon 05:00', fc: 'Out Sat 16:51', pace: 'Ahead of pace', exp: 48 }],
        models: [['Opus 4.5', 1022, '61.2M'], ['Sonnet 4.5', 318, '29.8M'], ['Haiku 4.5', 72, '5.4M']],
        proj: [['~/Projects/cmdmeow', 806, '52.1M'], ['~/Projects/site', 341, '23.9M'], ['~/work/api', 188, '14.2M'], ['~/notes', 77, '6.2M']] },
      Codex: { plan: 'Codex Plus', fresh: 'Local · 15:20', usd: 4.25, tok: '1.8M', d7: 27.39, t7: '9.6M', t5: '640K', t30: '31.0M', req: 96, api30: 118, paid: 20,
        lim: [{ l: 'Session', pct: 71, at: '16:30', fc: 'Resets 16:30', pace: 'On pace', exp: 68 }, { l: 'Week', pct: 34, at: 'Mon 15:20', fc: 'Resets Mon 15:20', pace: 'Room to spare', exp: 52 }],
        models: [['gpt-5-codex', 94, '26.4M'], ['gpt-5', 24, '4.6M']], proj: [['~/Projects/cmdmeow', 71, '19.0M'], ['~/work/api', 47, '12.0M']] },
      Gemini: { plan: 'Gemini', fresh: 'Local · 15:20', usd: 2.57, tok: '917K', d7: 16.94, t7: '5.5M', t5: '306K', t30: '19.8M', req: 58, api30: 61,
        models: [['gemini-2.5-pro', 49, '13.1M'], ['gemini-2.5-flash', 12, '6.7M']], proj: [['~/Projects/site', 38, '12.2M'], ['~/notes', 23, '7.6M']] },
      OpenCode: { plan: 'OpenCode', fresh: 'Local · 15:20', usd: 1.68, tok: '229K', d7: 9.99, t7: '1.4M', t5: '76K', t30: '5.9M', req: 31, api30: 36,
        models: [['Sonnet 4.5', 28, '4.1M'], ['gpt-5', 8, '1.8M']], proj: [['~/work/api', 36, '5.9M']] },
    };
    const usd = v => v >= 100 ? '$' + Math.round(v) : '$' + v.toFixed(2);
    const col = pct => pct >= 85 ? 'var(--bad)' : pct >= 60 ? 'var(--ap)' : 'var(--cy)';
    const line = t => `${usd(t.usd)} today · ${t.tok} tok · ${usd(t.d7)} / 7d`;
    const days = Array.from({ length: 30 }, (_, k) => 14 + Math.round(60 * Math.abs(Math.sin(k * 1.7) * Math.cos(k * 0.45))) + (k % 7 === 5 ? 18 : 0));
    const chart = (scale, id) => `<div class="card apd"><div class="kh"><b>At API prices · 30 days</b></div><div class="dm num ah"><b>Today</b> ${usd(scale * 1.6)} · ${(scale / 9).toFixed(1)}M tok</div>
      <div class="cb ab">${days.map((h, k) => `<i style="height:${Math.max(4, h * (0.5 + ((k * 7 + id) % 10) / 20)).toFixed(0)}%"${k === 29 ? ' class=on' : ''}></i>`).join('')}</div><div class="ax2 dm"><span>Oct 14</span><span>Today</span></div></div>`;
    const card = (n, t) => `<div class="card pv" data-n="${n}"><div class="kh"><b>${t.plan}</b>${t.menu ? '<span class="chip mb">In menu bar</span>' : ''}<span class="num fr2${t.live ? ' lv' : ''}">${t.fresh}</span></div>
      ${t.lim ? t.lim.map(l => `<div class="plr"><span>${l.l}</span><span class="num fc2${l.fc.startsWith('Out') ? ' out' : ''}">${l.fc}</span><b class="num">${l.pct}%</b><span class="pbar"><i style="width:${l.pct}%;background:${col(l.pct)}"></i><s style="left:${l.exp}%"></s></span></div>`).join('')
        : `<div class="tk3"><span><b class="num">${t.tok}</b><span class="dm">Today</span></span><span><b class="num">${t.t7}</b><span class="dm">7 days</span></span><span><b class="num">${t.t5}</b><span class="dm">5 hours</span></span></div>`}
      <div class="dm num pf">${line(t)}</div></div>`;
    all.innerHTML = Object.entries(T).map(([n, t]) => card(n, t)).join('') + chart(50, 0);
    const ring = (l) => { const r = 30, c = 2 * Math.PI * r, a = (l.exp / 100) * 2 * Math.PI - Math.PI / 2; const x1 = 39 + Math.cos(a) * 24, y1 = 39 + Math.sin(a) * 24, x2 = 39 + Math.cos(a) * 36, y2 = 39 + Math.sin(a) * 36;
      return `<svg width="78" height="78" viewBox="0 0 78 78"><circle cx="39" cy="39" r="${r}" fill="none" stroke="rgba(255,255,255,.1)" stroke-width="9"/><circle cx="39" cy="39" r="${r}" fill="none" stroke="${col(l.pct)}" stroke-width="9" stroke-linecap="round" stroke-dasharray="${c.toFixed(1)}" stroke-dashoffset="${(c * (1 - l.pct / 100)).toFixed(1)}" transform="rotate(-90 39 39)"/><line x1="${x1.toFixed(1)}" y1="${y1.toFixed(1)}" x2="${x2.toFixed(1)}" y2="${y2.toFixed(1)}" stroke="#fff" stroke-width="2"/><text x="39" y="44" text-anchor="middle">${l.pct}%</text></svg>`; };
    const rows = (list, title) => { const top = Math.max(...list.map(r => r[1])); return `<div class="card shc"><div class="kh"><b>${title}</b><span class="dm">30 days</span></div>${list.map(r => `<div class="shw"><span>${r[0]}</span><span class="dm num">${r[2]}</span><b class="num">${usd(r[1])}</b><span class="pbar"><i style="width:${(r[1] / top * 100).toFixed(0)}%"></i></span></div>`).join('')}</div>`; };
    const single = (n) => {
      const t = T[n], id = Object.keys(T).indexOf(n);
      let r1;
      if (t.lim) {
        r1 = t.lim.map(l => `<div class="card lmc"><div class="kh"><b>${l.l}</b><span class="dm num">${l.at}</span></div><div class="lmb">${ring(l)}<div><b class="${l.pace.startsWith('Ahead') ? 'bad' : l.pace.startsWith('Room') ? 'ok' : ''}">${l.pace}</b><span class="dm num">${l.fc}</span></div></div></div>`).join('')
          + `<div class="card svc"><div class="kh"><b>Saved · 30 days</b></div><div class="svh"><b class="num">${usd(t.api30 - t.paid)}</b><span class="dm">${(t.api30 / t.paid).toFixed(1)}× plan</span></div><div class="svr"><span class="dm">At API prices</span><span class="num">${usd(t.api30)}</span><span class="dm">You paid</span><span class="num">${usd(t.paid)}</span></div></div>`;
      } else {
        const st = (a, b, c) => `<div class="card stc"><b>${a}</b><span class="num">${b}</span><span class="dm num">${c}</span></div>`;
        r1 = st('Today', t.tok, `${usd(t.usd)} · ${t.req} req`) + st('7 days', t.t7, `${usd(t.d7)} at API prices`) + st('30 days', t.t30, `${usd(t.api30)} at API prices`) + st('Last 5 hours', t.t5, '');
      }
      one.innerHTML = `<div class="a1g${t.lim ? '' : ' tk'}">${r1}${chart(t.api30 / 28, id + 1)}<div class="a1s">${rows(t.models, 'Models')}${rows(t.proj, 'Projects')}</div></div>`
        + (n === 'Claude' ? `<div class="pft"><span class="dm">Claude data on this Mac ${t.data}</span><span class="act" data-go3="cleanup">Review in Cleanup</span></div>` : '');
      sub.textContent = `${t.plan} · ${t.fresh}`;
      hover(one);
      const go = $('[data-go3]', one); if (go) go.addEventListener('click', () => window.cmShow && window.cmShow('cleanup'));
    };
    // hovering a day shows its numbers, like the app's chart
    const start = new Date(2026, 9, 14);
    const hover = root => { const bars = $$('.ab i', root), ln = $('.ah', root); bars.forEach((b, j) => b.addEventListener('mouseenter', () => {
      bars.forEach(x => x.classList.toggle('on', x === b));
      const d = new Date(start); d.setDate(d.getDate() + j); const h = parseFloat(b.style.height);
      ln.innerHTML = `<b>${j === 29 ? 'Today' : d.toLocaleDateString('en-US', { weekday: 'short', month: 'short', day: 'numeric' }).replace(',', '')}</b> $${(h * 0.68).toFixed(2)} · ${(h * 0.11).toFixed(1)}M tok`;
    })); };
    hover(all);
    const pick = n => {
      $$('b, i', seg).forEach(o => { const want = o.textContent === n; if (want !== (o.tagName === 'B')) o.replaceWith(el(want ? 'b' : 'i', null, o.innerHTML)); });
      if (n === 'All') { all.hidden = false; one.hidden = true; sub.textContent = '$39.23 today'; } else { single(n); all.hidden = true; one.hidden = false; }
    };
    seg.addEventListener('click', e => { const o = e.target.closest('i'); if (o) pick(o.textContent); });
    all.addEventListener('click', e => { const c = e.target.closest('.pv'); if (c) pick(c.dataset.n); });
    $('.rbt', p).addEventListener('click', e => { const b = e.currentTarget; b.classList.add('rot'); $$('.fr2', p).forEach(f => f.textContent = f.textContent.replace(/\d\d:\d\d/, '15:21')); setTimeout(() => b.classList.remove('rot'), 700); });
  })();

  // ---------- Fan Control, like the app: two fan dials, chip heat map, Mode, last 10 minutes ----------
  (() => {
    const p = pane('fans'); if (!p) return;
    const fmt = n => Math.round(n).toLocaleString('en-US').replace(',', ' '), MIN = 1200, MAX = 5800;
    const heatC = c => { const t = Math.max(0, Math.min(1, (c - 30) / 70)); const st = [[111, 214, 232], [200, 240, 90], [247, 178, 103], [255, 107, 90]]; const k = Math.min(2, Math.floor(t * 3)), f = t * 3 - k, a = st[k], b = st[k + 1]; return `rgb(${a.map((v, i) => Math.round(v + (b[i] - v) * f)).join(',')})`; };
    // heat map: P-cores, E-cores, GPU blocks on the die; Memory, SSD, Battery on the side
    const die = $('#fh-die', p), side = $('#fh-side', p);
    const blocks = [['P-cores', 12, 62], ['E-cores', 4, 48], ['GPU', 16, 54]];
    die.innerHTML = blocks.map(([n, k, m]) => `<div class="blk"><span>${n}</span><b class="num" style="color:${heatC(m)}">${m}°</b><div class="cel" style="--n:${k > 8 ? 4 : 2}">${Array.from({ length: k }, (_, i) => `<i data-b="${m - 14 + ((i * 7) % 15)}"></i>`).join('')}</div></div>`).join('');
    side.innerHTML = [['Memory', 47], ['SSD', 40], ['Battery', 30]].map(([n, c]) => `<div class="sdr"><span>${n}</span><b class="num" style="color:${heatC(c)}">${c}°</b><i><s style="width:${(c - 30) / 70 * 100 + 4}%;background:${heatC(c)}"></s></i></div>`).join('');
    const paintCells = load => $$('.cel i', die).forEach(c => { const v = +c.dataset.b + load * 0.25 + (Math.random() - 0.5) * 4; c.style.background = heatC(v); });
    paintCells(16);
    // mode
    let mode = 'Auto', fixed = 0.43, preset = 'Balanced';
    const fx = $('.ffx', p), cv = $('.fcv', p), fst = $('.fst', fx), fv = $('#ff-v', p);
    const target = hl => mode === 'Quiet' ? 1300 : mode === 'Max' ? MAX : mode === 'Fixed' ? MIN + fixed * (MAX - MIN) : mode === 'Curve' ? curveAt(58) : 1900 + hl * 20;
    const setFixed = v => { fixed = Math.max(0, Math.min(1, v)); $('i', fst).style.width = fixed * 100 + '%'; $('b', fst).style.left = fixed * 100 + '%'; fv.textContent = fmt(MIN + fixed * (MAX - MIN)) + ' rpm'; };
    setFixed(fixed);
    const drag = e => { const r = fst.getBoundingClientRect(); setFixed((e.clientX - r.left) / r.width); window.cmFanTick && window.cmFanTick(); };
    fst.addEventListener('pointerdown', e => { fst.setPointerCapture(e.pointerId); drag(e); fst.onpointermove = drag; });
    fst.addEventListener('pointerup', () => { fst.onpointermove = null; });
    // curve: 3 points from a preset, auto above 95 °C
    const P = { Quiet: [[50, 0.1], [70, 0.3], [85, 0.65]], Balanced: [[45, 0.15], [65, 0.42], [80, 0.8]], Cool: [[40, 0.3], [55, 0.6], [70, 0.95]] };
    const curveAt = t => { const pts = [[30, P[preset][0][1]], ...P[preset], [95, P[preset][2][1]]]; for (let i = 1; i < pts.length; i++) if (t <= pts[i][0]) { const [a, b] = [pts[i - 1], pts[i]]; return MIN + (a[1] + (b[1] - a[1]) * (t - a[0]) / (b[0] - a[0])) * (MAX - MIN); } return MAX; };
    const svg = $('.fcg', p), X = t => 36 + (t - 30) / 70 * 380, Y = f => 166 - f * 156;
    const drawCurve = () => {
      const pts = [[30, P[preset][0][1]], ...P[preset], [95, P[preset][2][1]]];
      const line = pts.map(([t, f]) => `${X(t).toFixed(1)},${Y(f).toFixed(1)}`).join(' ');
      const grid = [0, 1, 2, 3, 4].map(k => `<line x1="36" x2="416" y1="${Y(k / 4)}" y2="${Y(k / 4)}"/><text x="30" y="${Y(k / 4) + 3}" text-anchor="end">${fmt(MIN + k / 4 * (MAX - MIN))}</text>`).join('');
      const xs = [30, 40, 50, 60, 70, 80, 90, 100].map(t => `<text x="${X(t)}" y="178" text-anchor="middle">${t}°</text>`).join('');
      const f58 = (curveAt(58) - MIN) / (MAX - MIN);
      svg.innerHTML = `<rect x="36" y="10" width="380" height="156" rx="6" class="bg"/><g class="gr">${grid}${xs}</g><rect x="${X(95)}" y="10" width="${416 - X(95)}" height="156" class="az"/><text x="${(X(95) + 416) / 2}" y="24" text-anchor="middle" class="azt">Auto</text>
        <polygon points="${line} ${X(95)},166 36,166" class="fl"/><polyline points="${line}" class="ln"/>
        <line x1="${X(58)}" x2="${X(58)}" y1="166" y2="${Y(f58)}" class="dl"/><circle cx="${X(58)}" cy="${Y(f58)}" r="5" class="now"/>
        ${P[preset].map(([t, f]) => `<circle cx="${X(t)}" cy="${Y(f)}" r="6" class="pt"/>`).join('')}`;
      $('#fc-t', p).textContent = fmt(curveAt(58));
    };
    drawCurve();
    $('.fpr', p).addEventListener('click', e => { const o = e.target.closest('i'); if (!o) return; const cur = $('b', o.parentElement); cur.replaceWith(el('i', null, cur.innerHTML)); o.replaceWith(el('b', null, o.innerHTML)); preset = o.textContent; drawCurve(); window.cmFanTick && window.cmFanTick(); });
    $('.fmd .fmh .seg', p).addEventListener('click', e => {
      const o = e.target.closest('i'); if (!o) return; const cur = $('b', o.parentElement); cur.replaceWith(el('i', null, cur.innerHTML)); o.replaceWith(el('b', null, o.innerHTML));
      mode = o.textContent; fx.hidden = mode !== 'Fixed'; cv.hidden = mode !== 'Curve'; p.classList.toggle('fcurve', mode === 'Curve'); window.cmFanTick && window.cmFanTick();
    });
    $('.fdel', p).addEventListener('click', e => { const b = e.currentTarget; b.classList.add('rot'); setTimeout(() => b.classList.remove('rot'), 700); });
    // live: dials, blades, history
    const hT = Array.from({ length: 60 }, (_, i) => 52 + Math.sin(i / 6) * 5), hF = Array.from({ length: 60 }, (_, i) => 2100 + Math.sin(i / 6 - 0.6) * 260);
    const ht = $('.ht', p), hf = $('.hf', p);
    const plot = () => { ht.setAttribute('points', hT.map((v, i) => `${(i / 59 * 600).toFixed(1)},${(100 - (v - 30) / 70 * 100).toFixed(1)}`).join(' ')); hf.setAttribute('points', hF.map((v, i) => `${(i / 59 * 600).toFixed(1)},${(100 - v / MAX * 100).toFixed(1)}`).join(' ')); };
    plot();
    window.cmFanDraw = (hl, still) => {
      const avg = target(hl) + (mode === 'Auto' ? 0 : (Math.random() - 0.5) * 40);
      [['l', avg - 85], ['r', avg + 85]].forEach(([k, v]) => {
        v = Math.max(MIN, Math.min(MAX, v));
        $('#fr-' + k, p).textContent = fmt(v);
        $('#fd-' + k, p).setAttribute('stroke-dasharray', `${(273.3 * (v / MAX)).toFixed(1)} 364.4`);
        $('#fb-' + k, p).style.animationDuration = (60 / v * 40).toFixed(2) + 's';
      });
      if (!still) { hT.shift(); hT.push(50 + hl * 0.2 + Math.random() * 3); hF.shift(); hF.push(avg); plot(); paintCells(hl); }
      return avg;
    };
    window.cmFanDraw(16, true);
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
    const flash = (b, t) => { const o = b.dataset.o || (b.dataset.o = b.textContent); b.textContent = t; clearTimeout(b._t); b._t = setTimeout(() => { b.textContent = o; }, 1200); };
    btn('Copy', e => flash(e.currentTarget, 'Copied'));
    btn('Save', e => flash(e.currentTarget, 'Saved'));
    btn('Pin', e => { shot.classList.toggle('pinned'); });
    document.addEventListener('keydown', e => { if ((e.metaKey || e.ctrlKey) && e.key === 'z' && !pane('capture').hidden && !ink.closest('[hidden]') && ink.lastChild) { e.preventDefault(); ink.lastChild.remove(); marks--; status.textContent = status.textContent.replace(/\d+ marks/, marks + ' marks'); } });
  })();

  // ---------- Screenshot page: History grid; Edit opens the editor; Capture area adds a shot ----------
  (() => {
    const p = pane('capture'); if (!p) return;
    const hv = $('.chv', p), ed = $('.ced', p), body = $('.cph', p), sett = $('.chs', p), sub = $('#cp-sub', p);
    const count = () => { const n = $$('.th', p).length; sub.textContent = `${n} screenshot${n === 1 ? '' : 's'} · ${(n * 2.6).toFixed(1)} MB · saving to Desktop`; $$('.thg', p).forEach(g => { g.previousElementSibling.hidden = g.hidden = !g.querySelector('.th:not([hidden])'); }); };
    const open = th => { hv.hidden = true; ed.hidden = false; $('.ced .tb.cap b', p).textContent = 'Capture from ' + th.dataset.t; };
    $('.cbk', p).addEventListener('click', () => { ed.hidden = true; hv.hidden = false; });
    document.addEventListener('keydown', e => { if (e.key === 'Escape' && !ed.hidden && !p.hidden) { ed.hidden = true; hv.hidden = false; } });
    // filters: All / Area / Window / Screen / Pinned
    let filt = 'All';
    const apply = () => { $$('.th', p).forEach(t => { t.hidden = !(filt === 'All' || (filt === 'Pinned' ? !!$('.thp', t) : t.dataset.k === filt.toLowerCase())); }); count(); };
    $('.cfr', p).addEventListener('click', e => { const f = e.target.closest('.fc'); if (!f) return; $$('.cfr .fc', p).forEach(x => x.classList.toggle('on', x === f)); filt = f.textContent; apply(); });
    $('.ckp .lk', p).addEventListener('click', () => pick('Settings'));
    const pick = n => { $$('.chv .tb .seg > *', p).forEach(o => { const want = o.textContent === n; if (want !== (o.tagName === 'B')) o.replaceWith(el(want ? 'b' : 'i', null, o.innerHTML)); }); body.hidden = n === 'Settings'; sett.hidden = n !== 'Settings'; $('.cfr', p).hidden = n === 'Folder'; $('.cpa', p).hidden = n === 'Settings'; if (n === 'Folder') { filt = 'All'; apply(); } };
    $('.chv .tb .seg', p).addEventListener('click', e => { const o = e.target.closest('i'); if (o) pick(o.textContent); });
    body.addEventListener('click', e => {
      const th = e.target.closest('.th'); if (!th) return;
      const a = e.target.closest('.tha i');
      if (!a) return open(th);
      const k = a.dataset.a;
      if (k === 'Edit') open(th);
      else if (k === 'Delete') { th.classList.add('out'); setTimeout(() => { th.remove(); count(); }, 250); }
      else if (k === 'Pin on screen') { const pin = $('.thp', th); if (pin) pin.remove(); else $('.thi', th).after(el('span', 'thp', ICON('M9 3h6l-1 6 3.5 3.5h-11L10 9zM12 12.5V21').replace(/14/g, '10'))); apply(); }
      else { if (k === 'Copy link' && window.cmShare) window.cmShare('Screenshot 2026-11-12 at ' + th.dataset.t.replace(' ', '') + '.png'); a.classList.add('ok'); setTimeout(() => a.classList.remove('ok'), 900); }
    });
    // Capture area: a flash, then the shot lands first under Today
    $('.cpa', p).addEventListener('click', () => {
      const f = el('div', 'cpf'); document.body.append(f); setTimeout(() => f.remove(), 400);
      const src = $('.th[data-k=area]', p) || $('.th', p); if (!src) return;
      const n = src.cloneNode(true); n.classList.remove('out'); n.hidden = false; n.classList.add('new');
      const d = new Date(), t = d.toLocaleTimeString('en-US', { hour: 'numeric', minute: '2-digit' }).toLowerCase();
      n.dataset.t = t; n.dataset.k = 'area'; n.dataset.m = '1280×720'; $('.thl span', n).textContent = t; $('.thl .dm', n).textContent = '1280×720'; const pin = $('.thp', n); if (pin) pin.remove();
      $('.thg', p).prepend(n); if (filt !== 'All' && filt !== 'Area') { filt = 'All'; $$('.cfr .fc', p).forEach(x => x.classList.toggle('on', x.textContent === 'All')); } apply();
    });
    count();
  })();

  // ---------- Video page: grid of recordings; Record screen adds one; Play and edit opens the player ----------
  (() => {
    const p = pane('video'); if (!p) return;
    const rec = $('.vrec', p), lab = $('span', rec), grid = $('.vgr', p), sett = $('.vst', p), sub = $('#vd-sub', p);
    const count = () => { const n = $$('.th', p).length; sub.textContent = `${n} recording${n === 1 ? '' : 's'} · saving to Desktop`; $$('.thg', p).forEach(g => { g.previousElementSibling.hidden = g.hidden = !g.children.length; }); };
    $('.tb .seg', p).addEventListener('click', e => { const o = e.target.closest('i'); if (!o) return; const cur = $('b', o.parentElement); cur.replaceWith(el('i', null, cur.innerHTML)); o.replaceWith(el('b', null, o.innerHTML)); const st = o.textContent === 'Settings'; grid.hidden = st; sett.hidden = !st; rec.hidden = st; sub.hidden = st; });
    let t = null, s = 0; const tpl = $('.th', p).cloneNode(true);
    rec.addEventListener('click', () => {
      if (!t) { s = 0; rec.classList.add('on'); lab.textContent = 'Stop · 0:00'; t = setInterval(() => { s++; lab.textContent = `Stop · ${Math.floor(s / 60)}:${String(s % 60).padStart(2, '0')}`; }, 1000); return; }
      clearInterval(t); t = null; rec.classList.remove('on'); lab.textContent = 'Record screen';
      const d = `${Math.floor(s / 60)}:${String(Math.max(1, s) % 60).padStart(2, '0')}`, now = new Date().toLocaleTimeString('en-US', { hour: 'numeric', minute: '2-digit' }).toLowerCase();
      const n = tpl.cloneNode(true); n.classList.add('new'); n.dataset.t = now; n.dataset.n = 'Screen recording';
      $('.thl span', n).textContent = now; $('.thl .dm', n).textContent = 'Video · ' + d; $('.thi', n).className = 'thi t-s';
      $('.thg', p).prepend(n); count();
    });
    grid.addEventListener('click', e => {
      const th = e.target.closest('.th'); if (!th) return; const a = e.target.closest('.tha i'); if (!a) return; const k = a.dataset.a;
      if (k === 'Delete') { th.classList.add('out'); setTimeout(() => { th.remove(); count(); }, 250); return; }
      if (k === 'Copy link' && window.cmShare) window.cmShare(th.dataset.n + '.mp4', true);
      a.classList.add('ok'); setTimeout(() => a.classList.remove('ok'), 900);
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
      det.innerHTML = `<div class="kh"><span><i class="ld"></i>${label}</span><span class="dm">${at}</span></div>${big}${k === 'Links' ? `<b>${t.split('/')[0]}</b><span class="url">https://${t}</span>` : ''}<div class="kv2"><span class="dm">From</span><span>${src}</span><span class="dm">Copied</span><span>once</span><span class="dm">Content</span><span>${t.length} characters</span></div><span class="cdel" role="button">Delete from history</span><div class="cb2"><span class="act">Copy</span><span class="btn2">Copy as plain text</span>${k === 'Links' ? '<span class="btn2">Open link</span>' : ''}<span class="btn2">${$('.pn', r) ? 'Unpin' : 'Pin'}</span></div>`;
    };
    rows.forEach(r => r.addEventListener('click', () => show(r)));
    const count = () => { const n = $$('.cpr', p).length, pin = $$('.cpr .pn', p).length; if (!pause || pause.textContent.startsWith('Pause')) st.textContent = `Recording · ${n} items · ${pin} pinned · ${(n * 0.15).toFixed(1)} MB`; const cs = $('#cs-n', p); if (cs) cs.textContent = `${n} items · ${(n * 0.15).toFixed(1)} MB on disk`; };
    const drop = r => { if (r.classList.contains('out')) return; r.classList.add('out'); setTimeout(() => { r.remove(); const k = rows.indexOf(r); if (k >= 0) rows.splice(k, 1); apply(); count(); }, 200); };
    det.addEventListener('click', async e => {
      if (e.target.closest('.cdel')) { const r = rows.find(x => x.classList.contains('on')); if (r) drop(r); return; }
      const b = e.target.closest('.act, .btn2'); if (!b) return; const o = b.textContent; if (/Copy/.test(o)) { b.textContent = 'Copied'; await sleep(1000); b.textContent = o; return; }
      const r = rows.find(x => x.classList.contains('on')); if (!r || !/pin/i.test(o)) return;
      const pn = $('.pn', r);
      if (pn) pn.remove(); else { const w = el('span', null, ICON('M9 3h6l-1 6 3.5 3.5h-11L10 9zM12 12.5V21')); const ic = w.firstElementChild; ic.classList.add('pn'); $('.t', r).after(ic); }
      show(r); apply(); count();
    });
    const pause = $$('.tb .btn2', p).find(b => /Pause/.test(b.textContent)), st = $('.tb .dm', p);
    if (pause) pause.addEventListener('click', () => { const on = pause.textContent.startsWith('Pause'); pause.textContent = on ? 'Resume' : 'Pause 15 min'; st.classList.toggle('amb', on); if (on) st.textContent = `Paused for 15 min · ${rows.length} items`; else count(); });
    // Clear unpinned: the first click asks, the second clears
    $$('.ccl', p).forEach(cl => cl.addEventListener('click', () => {
      if (!cl.classList.contains('warn')) { cl.classList.add('warn'); cl.textContent = 'Click again to clear'; setTimeout(() => { cl.classList.remove('warn'); cl.textContent = 'Clear unpinned'; }, 3000); return; }
      cl.classList.remove('warn'); cl.textContent = 'Clear unpinned'; rows.filter(r => !$('.pn', r)).forEach(drop);
    }));
    const list = $('.cbp', p), filt2 = $('.cfl', p), sett = $('.cst', p);
    $('.tb .seg', p).addEventListener('click', e => { const o = e.target.closest('i'); if (!o) return; const cur = $('b', o.parentElement); cur.replaceWith(el('i', null, cur.innerHTML)); o.replaceWith(el('b', null, o.innerHTML)); const s2 = o.textContent === 'Settings'; list.hidden = filt2.hidden = s2; sett.hidden = !s2; });
    sett.addEventListener('click', e => { const l = e.target.closest('.lk'); if (l) l.closest('div').remove(); });
    show(rows.find(r => r.classList.contains('on')) || rows[0]);
  })();

  // ---------- Dictation, like the app: History | Settings; ⌥D (or opening the page) runs a dictation ----------
  (() => {
    const p = pane('dictation'); if (!p) return;
    const list = $('.dcl', p), sett = $('.dst', p), sub = $('.tb .dm', p);
    const lines = ['We’ll ship the new version to the site this afternoon.', 'Remind me to answer Alex about the release notes tomorrow morning.', 'Add an install section to the README.'];
    let n = 0, busy = false, words = 69;
    const idle = () => { sub.classList.remove('live'); sub.textContent = `⌥D · Parakeet v3 · ${words} words today`; };
    const tab = name => { $$('.tb .seg > *', p).forEach(o => { const want = o.textContent === name; if (want !== (o.tagName === 'B')) o.replaceWith(el(want ? 'b' : 'i', null, o.innerHTML)); }); list.hidden = name !== 'History'; sett.hidden = name === 'History'; };
    $('.tb .seg', p).addEventListener('click', e => { const o = e.target.closest('i'); if (o) tab(o.textContent); });
    const TR = $('.dcr .dx', p).outerHTML;
    const go = async quiet => {
      if (busy || (quiet === true && !sett.hidden)) return; busy = true; if (quiet !== true && window.cmShow) window.cmShow('dictation'); tab('History');
      sub.classList.add('live'); sub.textContent = 'Listening';
      const row = el('div', 'dcr new', `<div><span class="dm">now · Notes · 3 s</span><p><span class="wave"><i></i><i></i><i></i><i></i><i></i></span></p></div>${TR}<span class="btn2">Copy</span>`);
      list.prepend(row); await sleep(1500);
      const pEl = $('p', row), text = lines[n++ % lines.length]; pEl.textContent = '';
      for (const ch of text) { pEl.textContent += ch; await sleep(22); }
      words += text.split(/\s+/).length; idle(); busy = false;
    };
    window.cmDictate = go;
    document.addEventListener('keydown', e => { if (e.altKey && e.code === 'KeyD') { const r = app.getBoundingClientRect(); if (r.top < innerHeight && r.bottom > 0) { e.preventDefault(); go(); } } });
    // the first time the page opens, it shows one dictation landing in the history
    let shown = false;
    new MutationObserver(() => { if (!p.hidden && !shown) { shown = true; setTimeout(() => go(true), 700); } }).observe(p, { attributes: true, attributeFilter: ['hidden'] });
    p.addEventListener('click', async e => {
      const x = e.target.closest('.dcr .dx'); if (x) { const r = x.closest('.dcr'); r.classList.add('out'); setTimeout(() => r.remove(), 250); return; }
      const b = e.target.closest('.dcr .btn2'); if (b) { b.textContent = 'Copied'; await sleep(1000); b.textContent = 'Copy'; return; }
      const w = e.target.closest('.dwl i'); if (w) { w.parentElement.remove(); return; }
      if (e.target.closest('.dwa .btn2')) { const t = ['Parakeet', 'Furman', 'GitHub', 'Reddit'].find(x => !$$('.dwl span', p).some(s => s.firstChild.textContent === x)); if (t) $('.dwl', p).append(el('span', null, `${t}<i>×</i>`)); return; }
      if (e.target.closest('.dcl2')) { $$('.dcr', p).forEach(r => r.remove()); }
    });
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
    // by hand: a grey "You" span on the Today strip grows from now
    const trk = $('.trk2', p), now = $('.now', trk); let mine = null;
    big.addEventListener('flip', () => { if (!big.classList.contains('off')) { mine = el('i', 'y'); mine.style.left = now.style.left; mine.style.width = '0.6%'; trk.insertBefore(mine, now); } else if (mine) { mine = null; } });
    big.addEventListener('flip', set); if (cl) cl.addEventListener('flip', set);
    // Shortcut: click, then press keys
    const key = $('.akey', p);
    key.addEventListener('click', () => { key.classList.add('rec'); key.textContent = 'Type a shortcut…'; });
    document.addEventListener('keydown', e => {
      if (!key.classList.contains('rec')) return; if (['Meta', 'Alt', 'Control', 'Shift'].includes(e.key)) return; e.preventDefault();
      key.classList.remove('rec'); if (e.key === 'Escape') { key.textContent = key.dataset.k || '⌥⌘A'; return; }
      const k = (e.ctrlKey ? '⌃' : '') + (e.altKey ? '⌥' : '') + (e.shiftKey ? '⇧' : '') + (e.metaKey ? '⌘' : '') + (e.code.startsWith('Key') ? e.code.slice(3) : e.key.toUpperCase());
      key.textContent = key.dataset.k = k;
    });
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


  // One bar: the menu bar icons jump into Cmd+Meow, each becomes a live widget.
  // Plays once when it comes into view and stays on the open panel; the round button plays it again.
  (() => {
    const bars = document.getElementById('bars'); if (!bars) return;
    const again = document.getElementById('bars-again'), ap = $$('.mi2.ap', bars), wg = $$('.wg', bars);
    ap.forEach((ic, k) => ic.style.setProperty('--k', k));
    const wOf = i => wg.find(w => +w.dataset.i === i);
    const end = () => { bars.className = 'bars b-one b-open b-done'; ap.forEach(ic => ic.classList.add('out')); wg.forEach(w => w.classList.add('on')); };
    const reset = () => { bars.className = 'bars'; ap.forEach(ic => ic.classList.remove('out')); wg.forEach(w => w.classList.remove('on')); };
    const clearFlying = () => $$('.mi2.fly', document).forEach(f => f.parentNode && f.parentNode.removeChild(f));
    if (matchMedia('(prefers-reduced-motion: reduce)').matches) { end(); return; }
    const ORDER = [7, 0, 12, 5, 11, 2, 13, 8, 1, 10, 4, 9, 3, 6];
    let gen = 0, state = 'idle';
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
      return new Promise(r => { an.onfinish = () => { if (f.parentNode) f.parentNode.removeChild(f); w.classList.add('on'); r(); }; });
    };
    // Starts on the menu bar full of icons (they wiggle at once), ends on the panel and stays there
    const play = async id => {
      again.hidden = true; reset(); bars.classList.add('b-wig'); await sleep(1100); if (id !== gen) return;
      bars.classList.add('b-one'); await sleep(420); bars.classList.add('b-open'); await sleep(380); if (id !== gen) return;
      bars.classList.remove('b-wig');
      const all = []; bars.classList.add('b-eat');
      for (const [k, i] of ORDER.entries()) {
        if (id !== gen) return;
        all.push(fly(ap[i], wOf(i), k));
        await sleep(60 + (k * 29) % 60);
      }
      await Promise.all(all); bars.classList.remove('b-eat'); if (id !== gen) return;
      bars.classList.add('b-done'); again.hidden = false; state = 'done';
    };
    const run = () => { state = 'run'; play(++gen); };
    // Scrolled away mid-way: jump to the end, so it never waits half-done
    const finish = () => { gen++; clearFlying(); end(); again.hidden = false; state = 'done'; };
    new IntersectionObserver(es => {
      if (es[0].isIntersecting && state === 'idle') run();
      else if (!es[0].isIntersecting && state === 'run') finish();
    }, { threshold: 0.45 }).observe(bars);
    again.addEventListener('click', run);
  })();

  document.querySelectorAll('[data-go]').forEach(a => a.addEventListener('click', e => { e.preventDefault(); window.cmShow && window.cmShow(a.dataset.go); app.scrollIntoView({ behavior: 'smooth', block: 'center' }); }));


  // Hero panel: a widget opens its tool in the window
  const go = { Clean: 'cleanup', Screenshot: 'capture', Claude: 'awake', Clipboard: 'clipboard', 'Macintosh HD': 'diskmap', Memory: 'health', CPU: 'health', Battery: 'health', 'Home Wi-Fi': 'health', 'AI Usage': 'claude', Fans: 'fans' };
  document.querySelectorAll('.app.pnl .wd').forEach(w => {
    const k = (w.querySelector('.dm, .kh b, .cl3 > span:last-child') || {}).textContent;
    const to = w.dataset.to || go[(k || '').trim()]; if (!to) return;
    w.classList.add('link');
    const open = () => { window.cmShow && window.cmShow(to); app.scrollIntoView({ behavior: 'smooth', block: 'center' }); };
    w.addEventListener('click', open);
  });
  // Keyboard: Tab walks the sidebar tabs only. The mock controls inside a pane are for the mouse,
  // so the page doesn't take ~30 extra stops on its way to the footer.
  const CTL = '.rb, .act, .btn2, .gch, .seg > i, .tg, .fc, .stb, .fr .ck, .rbt, .tha i, .lk, .dots, .dwl i, .pv, .cbk, .smn span, .cdel, .akey, .usort, .cpr, .th, [role=button]';
  const arm = () => {
    $$(CTL, app).forEach(c => {
      if (!c.hasAttribute('role')) c.setAttribute('role', c.classList.contains('tg') ? 'switch' : 'button');
      if (c.classList.contains('tg')) c.setAttribute('aria-checked', String(!c.classList.contains('off')));
    });
    $$('.pane button, .pane input, .pane select, .pane textarea, .pane a[href], .pane [tabindex]', app).forEach(c => { if (c.tabIndex !== -1) c.tabIndex = -1; });
  };
  let armQ = 0; arm(); new MutationObserver(() => { if (!armQ) armQ = requestAnimationFrame(() => { armQ = 0; arm(); }); }).observe(app, { subtree: true, childList: true, attributes: true, attributeFilter: ['class'] });
  app.addEventListener('keydown', e => { if ((e.key === 'Enter' || e.key === ' ') && e.target.matches && e.target.matches(CTL)) { e.preventDefault(); e.target.click(); } });
})();
