import React, {useCallback, useRef} from 'react';
import {AbsoluteFill, Audio, Img, staticFile, useCurrentFrame, useVideoConfig, interpolate, Easing} from 'remotion';
import {Scene} from './Scene';
import {opener, main} from './scenes';
import {START, END} from './timing';

const eio = Easing.inOut(Easing.cubic);
const clamp = {extrapolateLeft: 'clamp', extrapolateRight: 'clamp'} as const;

// Phone layout: big title on top, the Mac screen as a card below (zoomed on the
// right part where the action is), cmdmeow.com at the bottom.
export const Promo9x16: React.FC = () => {
  const frame = useCurrentFrame();
  const {fps} = useVideoConfig();
  const t = frame / fps;
  const z = t < START ? 0 : interpolate(t, [START, START + 0.6], [0, 1], {...clamp, easing: eio});
  const e = interpolate(t, [END, END + 0.5], [0, 1], {...clamp, easing: eio});
  const cw = 1920 + (1220 - 1920) * z;
  const cx = (700 + (350 - 700) * e) * z;
  const s = 1080 / cw;
  const h = 1080 * s;
  const top = (1920 / 2 - h / 2 + 60) * (1 - z) + 600 * z;
  const url = t < START ? 0 : interpolate(t, [START + 0.6, START + 1, END, END + 0.3], [0, 1, 1, 0], clamp);

  const ttl = useRef<HTMLDivElement>(null);
  const keys = useRef<HTMLDivElement>(null);
  const copyTitle = useCallback((root: HTMLElement) => {
    const L = root.querySelector('#lab') as HTMLElement | null;
    const T = ttl.current!, Kd = keys.current!;
    const show = !!L && L.style.display !== 'none' && t < END;
    T.parentElement!.style.display = show ? 'flex' : 'none';
    if (!L) return;
    L.style.visibility = 'hidden';
    if (!show) return;
    const tt = root.querySelector('#ttl') as HTMLElement, kk = root.querySelector('#keys') as HTMLElement;
    T.textContent = tt.textContent;
    T.style.opacity = tt.style.opacity;
    T.style.transform = tt.style.transform;
    T.style.filter = tt.style.filter;
    if (Kd.innerHTML !== kk.innerHTML) Kd.innerHTML = kk.innerHTML;
    Kd.style.opacity = kk.style.opacity;
    Kd.style.setProperty('--kc', kk.style.getPropertyValue('--kc'));
  }, [t]);

  return (
    <AbsoluteFill style={{background: '#000', overflow: 'hidden'}}>
      <Img src={staticFile('desk.jpg')} style={{position: 'absolute', width: '100%', height: '100%', objectFit: 'cover', filter: 'brightness(.42)'}} />
      <div
        style={{
          position: 'absolute', left: 0, top: 0, width: 1920, height: 1080, transformOrigin: '0 0', overflow: 'hidden',
          transform: `translate(${-cx * s}px,${top}px) scale(${s})`,
          clipPath: `inset(0 ${1920 - cx - cw}px 0 ${cx}px round ${28 / s}px)`,
          boxShadow: '0 40px 120px rgba(0,0,0,.7)',
        }}
      >
        {t < START ? <Scene key="opener" mod={opener} t={t} /> : <Scene key="main" mod={main} t={t} onDrawn={copyTitle} />}
      </div>
      <style>{`.vkeys .key{min-width:84px;height:84px;padding:0 22px;border-radius:18px;background:#1b1f1a;border:2px solid rgba(255,255,255,.14);box-shadow:inset 0 -5px 0 rgba(0,0,0,.45);display:flex;align-items:center;justify-content:center;font:600 40px Inter;color:#fff}.vkeys .key.on{background:var(--kc,#C8F05A);color:#0E1405;border-color:transparent}`}</style>
      <div style={{position: 'absolute', left: 0, top: 0, width: 1080, height: 560, display: 'none', flexDirection: 'column', alignItems: 'center', justifyContent: 'flex-end', gap: 26, paddingBottom: 40, color: '#fff'}}>
        <div ref={ttl} style={{font: '700 150px/1 "Inter Display"', letterSpacing: '-.035em', whiteSpace: 'nowrap', textShadow: '0 6px 40px rgba(0,0,0,.55)'}} />
        <div ref={keys} className="vkeys" style={{display: 'flex', gap: 14}} />
      </div>
      <div style={{position: 'absolute', left: 0, width: 1080, top: 1640, textAlign: 'center', font: '600 44px Inter', color: 'rgba(255,255,255,.8)', opacity: url}}>cmdmeow.com</div>
      <Audio src={staticFile('music.mp3')} />
    </AbsoluteFill>
  );
};
