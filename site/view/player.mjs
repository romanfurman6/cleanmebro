// Cmd+Meow video player: custom controls over a <video>. view.mjs decides
// what to load; this file only plays it.
import { fmt } from './core.mjs';

export function initPlayer(srcHref, onError) {
  const $ = function (id) { return document.getElementById(id); };
  var player = $('player'), v = $('v'), seek = $('seek');
  player.hidden = false; $('keys').hidden = false;
  // A tiny start offset makes Safari paint the first frame as a poster.
  v.src = srcHref + (srcHref.indexOf('#') >= 0 || srcHref.startsWith('blob:') ? '' : '#t=0.001');
  v.addEventListener('error', function () { onError(); });

  // ---- Play / pause -------------------------------------------------------
  function toggle() { if (v.paused || v.ended) v.play(); else v.pause(); }
  $('big').addEventListener('click', toggle);
  $('play').addEventListener('click', toggle);
  v.addEventListener('click', toggle);
  v.addEventListener('dblclick', toggleFs);
  v.addEventListener('play', function () { player.classList.add('playing'); $('play').setAttribute('aria-label', 'Pause'); poke(); });
  v.addEventListener('pause', function () { player.classList.remove('playing'); $('play').setAttribute('aria-label', 'Play'); wake(); });
  v.addEventListener('waiting', function () { $('spin').hidden = false; });
  ['playing', 'canplay', 'pause', 'seeked'].forEach(function (e) { v.addEventListener(e, function () { $('spin').hidden = true; }); });

  // ---- Time and seek ------------------------------------------------------
  var dragging = false;
  function setProgress(t) {
    var d = v.duration || 0, p = d ? Math.min(1, t / d) * 100 : 0;
    $('prog').style.width = p + '%';
    $('knob').style.left = p + '%';
    $('cur').textContent = fmt(t);
    seek.setAttribute('aria-valuenow', Math.floor(t));
    seek.setAttribute('aria-valuetext', fmt(t) + ' of ' + fmt(d));
  }
  v.addEventListener('loadedmetadata', function () {
    $('dur').textContent = fmt(v.duration);
    if (v.videoWidth && v.videoHeight) $('main').style.setProperty('--ar', (v.videoWidth / v.videoHeight).toFixed(4));
    seek.setAttribute('aria-valuemax', Math.floor(v.duration || 0));
    setProgress(v.currentTime);
  });
  v.addEventListener('durationchange', function () { $('dur').textContent = fmt(v.duration); });
  v.addEventListener('timeupdate', function () { if (!dragging) setProgress(v.currentTime); });
  v.addEventListener('progress', drawBuffered);
  function drawBuffered() {
    var b = v.buffered, d = v.duration, end = 0;
    for (var i = 0; i < b.length; i++) if (b.start(i) <= v.currentTime + 0.5) end = Math.max(end, b.end(i));
    $('buf').style.width = d ? (end / d * 100) + '%' : 0;
  }

  function timeAt(x) {
    var r = seek.getBoundingClientRect();
    return Math.max(0, Math.min(1, (x - r.left) / r.width)) * (v.duration || 0);
  }
  seek.addEventListener('pointermove', function (e) {
    var r = seek.getBoundingClientRect(), t = timeAt(e.clientX);
    var tip = $('tip'), half = tip.offsetWidth / 2;
    tip.textContent = fmt(t);
    tip.style.left = Math.max(half, Math.min(r.width - half, e.clientX - r.left)) + 'px';
    if (dragging) setProgress(t);
  });
  seek.addEventListener('pointerdown', function (e) {
    if (!v.duration) return;
    dragging = true; seek.classList.add('drag'); seek.setPointerCapture(e.pointerId);
    setProgress(timeAt(e.clientX));
  });
  function endDrag(e) {
    if (!dragging) return;
    dragging = false; seek.classList.remove('drag');
    v.currentTime = timeAt(e.clientX);
  }
  seek.addEventListener('pointerup', endDrag);
  seek.addEventListener('pointercancel', function () { dragging = false; seek.classList.remove('drag'); });

  function skip(s) {
    if (!v.duration) return;
    v.currentTime = Math.max(0, Math.min(v.duration, v.currentTime + s));
    flash((s > 0 ? '+' : '−') + Math.abs(s) + ' s');
  }
  $('back').addEventListener('click', function () { skip(-10); });
  $('fwd').addEventListener('click', function () { skip(10); });

  // ---- Volume -------------------------------------------------------------
  var vol = $('vol');
  function syncVol() {
    player.classList.toggle('muted', v.muted || v.volume === 0);
    vol.value = v.muted ? 0 : v.volume;
    $('mute').setAttribute('aria-label', v.muted ? 'Unmute' : 'Mute');
  }
  vol.addEventListener('input', function () { v.volume = +vol.value; v.muted = v.volume === 0; });
  $('mute').addEventListener('click', function () {
    if (v.muted && v.volume === 0) v.volume = 1;
    v.muted = !v.muted;
  });
  v.addEventListener('volumechange', syncVol);
  function nudgeVol(d) {
    v.muted = false; v.volume = Math.max(0, Math.min(1, Math.round((v.volume + d) * 20) / 20));
    flash(Math.round(v.volume * 100) + '%');
  }

  // ---- Speed --------------------------------------------------------------
  var rate = $('rate'), menu = $('rates');
  function setRate(r) {
    v.playbackRate = r;
    rate.textContent = r + '×';
    menu.querySelectorAll('button').forEach(function (b) { b.setAttribute('aria-checked', String(+b.dataset.r === r)); });
  }
  function closeMenu() { menu.hidden = true; rate.setAttribute('aria-expanded', 'false'); }
  rate.addEventListener('click', function (e) {
    e.stopPropagation();
    menu.hidden = !menu.hidden; rate.setAttribute('aria-expanded', String(!menu.hidden));
    if (!menu.hidden) menu.querySelector('[aria-checked="true"]').focus();
  });
  menu.addEventListener('click', function (e) {
    var b = e.target.closest('button'); if (!b) return;
    setRate(+b.dataset.r); closeMenu(); rate.focus();
  });
  document.addEventListener('click', function (e) { if (!menu.hidden && !menu.contains(e.target)) closeMenu(); });
  setRate(1);

  // ---- Picture in picture, full screen ------------------------------------
  if (document.pictureInPictureEnabled && !v.disablePictureInPicture) {
    $('pip').hidden = false;
    $('pip').addEventListener('click', function () {
      if (document.pictureInPictureElement) document.exitPictureInPicture(); else v.requestPictureInPicture().catch(function () {});
    });
  } else if (v.webkitSupportsPresentationMode && v.webkitSupportsPresentationMode('picture-in-picture')) {
    $('pip').hidden = false;
    $('pip').addEventListener('click', function () {
      v.webkitSetPresentationMode(v.webkitPresentationMode === 'picture-in-picture' ? 'inline' : 'picture-in-picture');
    });
  }

  function fsEl() { return document.fullscreenElement || document.webkitFullscreenElement; }
  function toggleFs() {
    if (fsEl()) (document.exitFullscreen || document.webkitExitFullscreen).call(document);
    else if (player.requestFullscreen) player.requestFullscreen();
    else if (player.webkitRequestFullscreen) player.webkitRequestFullscreen();
    else if (v.webkitEnterFullscreen) v.webkitEnterFullscreen(); // iPhone
  }
  $('fs').addEventListener('click', toggleFs);
  ['fullscreenchange', 'webkitfullscreenchange'].forEach(function (e) {
    document.addEventListener(e, function () {
      player.classList.toggle('fs', !!fsEl());
      $('fs').setAttribute('aria-label', fsEl() ? 'Exit full screen' : 'Full screen');
    });
  });

  // ---- Auto-hide controls while playing -----------------------------------
  var idleTimer;
  function wake() { player.classList.remove('idle'); clearTimeout(idleTimer); }
  function poke() {
    wake();
    idleTimer = setTimeout(function () {
      if (!v.paused && !dragging && menu.hidden && !$('controls').matches(':hover')) player.classList.add('idle');
    }, 2500);
  }
  player.addEventListener('pointermove', poke);
  player.addEventListener('pointerdown', poke);
  player.addEventListener('mouseleave', function () { if (!v.paused) player.classList.add('idle'); });
  player.addEventListener('focusin', poke);

  // ---- Feedback bubble ----------------------------------------------------
  var flashTimer;
  function flash(text) {
    var f = $('flash'); f.textContent = text; f.classList.add('on');
    clearTimeout(flashTimer); flashTimer = setTimeout(function () { f.classList.remove('on'); }, 600);
  }

  // ---- Keyboard -----------------------------------------------------------
  document.addEventListener('keydown', function (e) {
    if (e.metaKey || e.ctrlKey || e.altKey) return;
    var tag = (e.target.tagName || '').toLowerCase();
    if (tag === 'input' || tag === 'textarea' || tag === 'select') return;
    if (!menu.hidden && e.key === 'Escape') { closeMenu(); rate.focus(); return; }
    var onButton = tag === 'button' || tag === 'a';
    var handled = true;
    switch (e.key) {
      case ' ': if (onButton) return; toggle(); break;
      case 'k': case 'K': toggle(); break;
      case 'ArrowLeft': skip(-5); break;
      case 'ArrowRight': skip(5); break;
      case 'j': case 'J': skip(-10); break;
      case 'l': case 'L': skip(10); break;
      case 'ArrowUp': nudgeVol(0.05); break;
      case 'ArrowDown': nudgeVol(-0.05); break;
      case 'm': case 'M': v.muted = !v.muted; flash(v.muted ? 'Muted' : 'Sound on'); break;
      case 'f': case 'F': toggleFs(); break;
      case ',': if (v.paused) v.currentTime = Math.max(0, v.currentTime - 1 / 30); else handled = false; break;
      case '.': if (v.paused) v.currentTime = Math.min(v.duration || 0, v.currentTime + 1 / 30); else handled = false; break;
      case 'Home': v.currentTime = 0; break;
      case 'End': if (v.duration) v.currentTime = v.duration; break;
      default:
        if (/^[0-9]$/.test(e.key) && v.duration) v.currentTime = v.duration * (+e.key / 10);
        else handled = false;
    }
    if (handled) { e.preventDefault(); poke(); }
  });
  // Seek bar as a slider when focused: arrows already handled globally.

  syncVol();
  return { video: v, flash: flash, poke: poke };
}
