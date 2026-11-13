// Share page: /view/?u=<file url>&t=<title>. Shows a shot or plays a
// recording (View) and lets the visitor change a local copy (Edit).
// Also opens a file from this computer.
import { kindOf, readSource, safeUrl } from './core.mjs';
import { initPlayer } from './player.mjs';

const $ = (id) => document.getElementById(id);
let noteTimer = 0;
function note(text, ms) {
  const n = $('note');
  n.textContent = text; n.hidden = false;
  clearTimeout(noteTimer);
  if (ms) noteTimer = setTimeout(() => { n.hidden = true; }, ms);
}

function showEmpty(title, text) {
  // Nothing to act on: no View/Edit, no copy of a broken link
  ['modes', 'copy', 'dl'].forEach((id) => { $(id).hidden = true; });
  $('title').textContent = '';
  document.body.classList.remove('editing');
  $('empty').hidden = false;
  if (title) $('empty-h').textContent = title;
  if (text) $('empty-p').textContent = text;
}

// ---- Empty state: paste a link or open a file -----------------------------------
$('form').addEventListener('submit', (e) => {
  e.preventDefault();
  const u = safeUrl($('url').value);
  if (!u) { $('url').setCustomValidity('Use an http or https link'); $('url').reportValidity(); return; }
  location.search = '?u=' + encodeURIComponent(u.href);
});
$('url').addEventListener('input', function () { this.setCustomValidity(''); });
$('file').addEventListener('change', () => {
  const f = $('file').files[0];
  if (f) openItem({ blob: f, href: URL.createObjectURL(f), title: f.name, kind: kindOf(f.name, f.type), local: true });
});

// ---- What to show ---------------------------------------------------------------
const src = readSource(location.search, location.hash);
if (src.url) {
  openItem({ href: src.url.href, title: src.title, kind: kindOf(src.url.pathname), local: false });
} else if (src.raw) {
  showEmpty("This link can't be opened", 'The address is not a valid http or https link.');
} else {
  showEmpty();
}

async function openItem(item) {
  $('empty').hidden = true;
  document.title = item.title + ' · Cmd+Meow';
  $('title').textContent = item.title;
  if (!item.local) {
    $('dl').href = item.href; $('dl').hidden = false;
    $('copy').hidden = false;
    $('copy').onclick = function () {
      const b = this;
      navigator.clipboard.writeText(location.href).then(() => {
        b.textContent = 'Copied'; setTimeout(() => { b.textContent = 'Copy link'; }, 1500);
      });
    };
  }
  // No extension in the link: ask the server (works only with CORS), else try a picture.
  if (!item.kind) {
    try {
      const r = await fetch(item.href, { method: 'HEAD' });
      item.kind = kindOf('', r.headers.get('content-type'));
    } catch (e) { /* fall through */ }
  }
  if (item.kind === 'image' || !item.kind) openImage(item, !item.kind);
  else openVideo(item);
}

// ---- Mode switch ------------------------------------------------------------------
function setupModes(enter, leave) {
  const modes = $('modes'), bv = $('m-view'), be = $('m-edit');
  modes.hidden = false;
  let editing = false;
  async function set(edit) {
    if (edit === editing) return;
    if (edit) {
      be.disabled = true;
      const ok = await enter();
      be.disabled = false;
      if (!ok) return;
    } else leave();
    editing = edit;
    document.body.classList.toggle('editing', edit);
    bv.setAttribute('aria-selected', String(!edit));
    be.setAttribute('aria-selected', String(edit));
    const u = new URL(location.href);
    if (edit) u.searchParams.set('mode', 'edit'); else u.searchParams.delete('mode');
    history.replaceState(null, '', u);
  }
  bv.onclick = () => set(false);
  be.onclick = () => set(true);
  if (new URLSearchParams(location.search).get('mode') === 'edit') set(true);
  window.addEventListener('beforeunload', (e) => {
    if (editing && window.__dirty && window.__dirty()) { e.preventDefault(); e.returnValue = ''; }
  });
}

// A copy of the file in memory. A shared link needs the storage to allow
// this site (CORS); without it the page can only show the file.
async function fetchCopy(item) {
  if (item.blob) return item.blob;
  try {
    // no-store: the <img>/<video> above loaded it without CORS, and a cached
    // copy of that answer has no CORS headers, so it would fail here too.
    const r = await fetch(item.href, { mode: 'cors', cache: 'no-store' });
    if (!r.ok) throw new Error(String(r.status));
    item.blob = await r.blob();
    return item.blob;
  } catch (e) {
    note("Editing isn't available for this link: the storage doesn't let this site read the file.", 6000);
    return null;
  }
}

// ---- Shot ---------------------------------------------------------------------------
function openImage(item, guessing) {
  const shot = $('shot'), img = $('img');
  img.alt = item.title;
  img.onload = () => { shot.hidden = false; };
  img.onerror = () => {
    if (guessing) { item.kind = 'video'; openVideo(item); return; }
    showEmpty("Couldn't load the picture", 'The link may have expired. Try opening the file directly.');
    $('url').value = item.local ? '' : item.href;
  };
  img.src = item.href;
  shot.addEventListener('click', () => shot.classList.toggle('zoom'));

  let editor = null;
  setupModes(async () => {
    const blob = await fetchCopy(item);
    if (!blob) return false;
    if (!editor) {
      const { createImageEditor } = await import('./image-edit.mjs');
      let bitmap;
      try { bitmap = await createImageBitmap(blob); } catch (e) { note("This picture can't be edited here.", 5000); return false; }
      editor = createImageEditor(bitmap, item.title, note);
      window.__dirty = () => editor.dirty;
      window.__editor = editor;
    }
    shot.hidden = true;
    editor.show();
    return true;
  }, () => {
    editor && editor.hide();
    shot.hidden = false;
  });
}

// ---- Recording -------------------------------------------------------------------------
function openVideo(item) {
  const p = initPlayer(item.href, () => {
    $('player').hidden = true; $('keys').hidden = true; $('modes').hidden = true; $('vt').hidden = true;
    showEmpty("Couldn't load the video", "The link may have expired, or this browser can't play the format. Try opening the file directly.");
    $('url').value = item.local ? '' : item.href;
  });
  let editor = null;
  setupModes(async () => {
    if (!item.blob) {
      // Check the storage lets us read ranges of the file; the export streams it.
      try {
        const r = await fetch(item.href, { headers: { Range: 'bytes=0-1' } });
        if (!r.ok) throw new Error(String(r.status));
      } catch (e) {
        note("Editing isn't available for this link: the storage doesn't let this site read the file.", 6000);
        return false;
      }
    }
    if (!editor) {
      const { createVideoEditor } = await import('./video-edit.mjs');
      editor = createVideoEditor(p.video, () => item.blob || item.href, item.title, note);
      window.__dirty = () => editor.dirty;
    }
    $('keys').hidden = true;
    editor.show();
    return true;
  }, () => {
    editor && editor.hide();
    $('keys').hidden = false;
  });
}
