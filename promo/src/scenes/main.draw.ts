/* eslint-disable */
// @ts-nocheck
// Generated from the original render(t) page, then kept as source: the scene's drawing code.
// Every element lookup is scoped to `root`; rects are measured in stage pixels (Studio zoom safe).
export function create(root: HTMLElement): {render:(t:number)=>void; init:()=>void} {
const __r=(el:any)=>{const r=el.getBoundingClientRect(),R=root.getBoundingClientRect(),s=R.width/root.offsetWidth||1;const left=(r.left-R.left)/s,top=(r.top-R.top)/s,width=r.width/s,height=r.height/s;return {left,top,width,height,right:left+width,bottom:top+height,x:left,y:top};};

const $=id=>root.querySelector("#"+id);
const clamp=(x,a=0,b=1)=>Math.min(b,Math.max(a,x)), lerp=(a,b,p)=>a+(b-a)*p, P=(t,a,b)=>clamp((t-a)/(b-a));
const eo=p=>1-Math.pow(1-p,3), eo5=p=>1-Math.pow(1-p,5), eio=p=>p<.5?4*p*p*p:1-Math.pow(-2*p+2,3)/2;
const spring=(dt,k=7,w=13)=>dt<=0?0:1-Math.exp(-k*dt)*Math.cos(w*dt);
// grid: track 2, 115 BPM; bar k starts at 3 + k*B
const B=2.0870056, b=B/4, K=k=>3+k*B;
const CX=1250, CY=610;
const COL={create:'#F7B267',clean:'#C8F05A',monitor:'#6FD6E8',util:'#E8E1FF'};
// ---- keycap cat ----
function cmdPath(cx,cy,q){const a=cx-q,bb=cx+q,c=cy-q,d=cy+q,r=q*.93;return `M${a} ${c-r}V${d+r}M${bb} ${c-r}V${d+r}M${a-r} ${c}H${bb+r}M${a-r} ${d}H${bb+r}M${a} ${c-r}A${r} ${r} 0 1 0 ${a-r} ${c}M${bb} ${c-r}A${r} ${r} 0 1 1 ${bb+r} ${c}M${a} ${d+r}A${r} ${r} 0 1 1 ${a-r} ${d}M${bb} ${d+r}A${r} ${r} 0 1 0 ${bb+r} ${d}`;}
const EARS='M17 36 L23.500 14 Q25.500 9.500 29.500 13 L42 25 Z M83 36 L76.500 14 Q74.500 9.500 70.500 13 L58 25 Z';
const kc=(id,cmd)=>`<rect x="15" y="30" width="70" height="60" rx="13" fill="#CBBFA8"/><g id="${id}ears"><path d="${EARS}" fill="#F2ECE3" stroke="#F2ECE3" stroke-width="4" stroke-linejoin="round"/></g><rect x="15" y="24" width="70" height="60" rx="13" fill="#F2ECE3"/>`+(cmd?`<path id="${id}cmd" d="${cmdPath(50,54,7)}" fill="none" stroke="#E39A35" stroke-width="4.2"/>`:'')+`<rect id="${id}el" x="36" y="45" width="8" height="18" rx="4" fill="#14181A"/><rect id="${id}er" x="56" y="45" width="8" height="18" rx="4" fill="#14181A"/>`;
$('cat').innerHTML=kc('k',true);$('sloticon').innerHTML=kc('m',false); $('camcat').innerHTML=kc('c',false);
function cat(x,y,size,o=1,eyesOn=1,blink=0,cmd=0){const c=$('cat');c.style.display=o>0?'block':'none';c.style.opacity=o;c.style.width=c.style.height=size+'px';c.style.transform=`translate(${x-size/2}px,${y-size/2}px)`;
  const h=18*(1-blink*.85);['kel','ker'].forEach(id=>{$(id).setAttribute('height',h);$(id).setAttribute('y',54-h/2);$(id).style.opacity=eyesOn});$('kcmd').style.opacity=cmd;}
function ring(r,p,col,sw=5){const C=2*Math.PI*r,sz=2*r+sw+2;return `<svg width="${sz}" height="${sz}" viewBox="0 0 ${sz} ${sz}" style="transform:rotate(-90deg)"><circle cx="${sz/2}" cy="${sz/2}" r="${r}" fill="none" stroke="rgba(255,255,255,.1)" stroke-width="${sw}"/><circle cx="${sz/2}" cy="${sz/2}" r="${r}" fill="none" stroke="${col}" stroke-width="${sw}" stroke-linecap="round" stroke-dasharray="${C*p} ${C}"/></svg>`;}
const I={
 broom:'<svg width="20" height="20" viewBox="0 0 24 24"><path d="M14 3l7 7M9 9l6 6-5 6-8-8z" fill="none" stroke="#C8F05A" stroke-width="2" stroke-linejoin="round"/></svg>',
 crop:'<svg width="22" height="22" viewBox="0 0 24 24"><path d="M6 2v16h16M2 6h16v16" fill="none" stroke="#F7B267" stroke-width="2.2"/></svg>',
 clip:'<svg width="20" height="22" viewBox="0 0 22 24"><rect x="3" y="4" width="16" height="18" rx="3" fill="none" stroke="#E8E1FF" stroke-width="2.2"/><rect x="7" y="1.5" width="8" height="5" rx="1.5" fill="#E8E1FF"/></svg>',
 mic:'<svg width="20" height="22" viewBox="0 0 24 24"><rect x="9" y="3" width="6" height="11" rx="3" fill="none" stroke="#E8E1FF" stroke-width="2"/><path d="M5 11a7 7 0 0014 0M12 18v3" fill="none" stroke="#E8E1FF" stroke-width="2" stroke-linecap="round"/></svg>',
 rec:'<svg width="22" height="22" viewBox="0 0 24 24"><rect x="3" y="7" width="12" height="10" rx="2" fill="none" stroke="#F7B267" stroke-width="2.2"/><path d="M15 10.500l6-3v9l-6-3" fill="none" stroke="#F7B267" stroke-width="2.2" stroke-linejoin="round"/></svg>',
 fan:(s=26,c='#6FD6E8',id='')=>`<svg ${id?`id="${id}"`:''} width="${s}" height="${s}" viewBox="0 0 24 24"><g fill="${c}"><circle cx="12" cy="12" r="2"/><path d="M12 10C11 5 13 2 16 3c1.500 3-1 6-4 7zM14 13c5 1 7 4 5 6-3 .5-6-2-5-6zM10 13c-4 3-8 2-8-1 2-2 5-2 8 1z"/></g></svg>`,
};
const AIP=[['Claude','#E8A27A',.19],['Codex','#9FB4FF',.12],['Gemini','#7FD7C4',.08]];
// ---- panel: only the tools in the video, fully filled 4-col grid ----
$('panel').innerHTML=`<div class="hd">MacBook Pro<span><span class="hb"><svg width="16" height="16" viewBox="0 0 24 24"><circle cx="12" cy="12" r="3.5" fill="none" stroke="#fff" stroke-width="2"/><path d="M12 2v3M12 19v3M2 12h3M19 12h3" stroke="#fff" stroke-width="2" stroke-linecap="round"/></svg></span></span></div>
<div class="grid">
 <div class="cd c2 pw" id="t-clean"><div class="rowi"><span class="tile-ic" style="background:rgba(200,240,90,.12)">${I.broom}</span><div><div class="l">Clean</div><div class="n">15.5 GB</div></div></div></div>
 <div class="cd c1 pw" id="t-cap" style="padding:12px 10px">${I.crop}<div class="n" style="margin-top:12px;font-size:15px">Capture</div><div class="l">⇧⌘2</div></div>
 <div class="cd c1 pw" id="t-rec" style="padding:12px 10px">${I.rec}<div class="n" style="margin-top:12px;font-size:15px">Record</div><div class="l">⇧⌘5</div></div>
 <div class="cd c1 pw" id="t-clip" style="padding:12px 10px">${I.clip}<div class="n" style="margin-top:12px;font-size:15px">Clipboard</div><div class="l">⌥⌘V</div></div>
 <div class="cd c1 pw" id="t-dict" style="padding:12px 10px">${I.mic}<div class="n" style="margin-top:12px;font-size:15px">Dictation</div><div class="l">⌥D</div></div>
 <div class="cd c2 pw" id="t-bat"><div class="rowi">${ring(17,.8,'#C8F05A')}<div><div class="l">Battery</div><div class="n num">80%</div></div></div></div>
 <div class="cd c2 pw" id="t-cpu"><div class="rowi">${ring(17,.16,'#6FD6E8')}<div><div class="l">CPU</div><div class="n num">16%</div></div></div></div>
 <div class="cd c2 pw" id="t-mem"><div class="rowi">${ring(17,.42,'#6FD6E8')}<div><div class="l">Memory</div><div class="n num">42%</div></div></div></div>
 <div class="cd c2 pw" id="t-fan"><div class="rowi"><span class="tile-ic" style="width:40px;height:40px;background:rgba(111,214,232,.12)">${I.fan()}</span><div><div class="l">Fans</div><div class="n num">1 850 rpm</div></div></div></div>
 <div class="cd c2 pw" id="t-hd"><div class="rowi">${ring(17,.56,'#6FD6E8')}<div><div class="l">Macintosh HD</div><div class="n num">1.44 TB</div></div></div></div>
 <div class="cd c4 pw" id="t-ai" style="min-height:0"><div style="display:flex;justify-content:space-between;font-weight:600"><span>AI usage</span><span class="l num">3.4M tokens today</span></div><div style="display:grid;grid-template-columns:70px 1fr 44px;gap:8px 10px;align-items:center;margin-top:10px;font-size:14px">${AIP.map(([n,c,p])=>`<span class="l">${n}</span><span class="bar"><i style="width:${p*100}%;background:${c}"></i></span><span class="num">${Math.round(p*100)}%</span>`).join('')}</div></div>
</div>`;
const pws=[...root.querySelectorAll('#panel .pw')];
// ---- editor ----
$('tools').innerHTML=['Arrow','Blur','Text','Frame'].map(n=>`<span class="tool">${n}</span>`).join('')+'<span class="tool" id="copyb" style="margin-left:auto">Copy</span>';
const toolEls=[...root.querySelectorAll('#tools .tool')].slice(0,4);
$('bars').innerHTML=[42,58,51,77,64,92,86].map((h,i)=>`<div style="flex:1;border-radius:10px 10px 4px 4px;background:${i===5?'#F7B267':'#cfcabf'};height:${h}%"></div>`).join('');
const CLIP=[
 ['shot',`<span class="th" style="background:#F6F3EC"><b style="width:36px;height:5px;background:#1d1d1b;border-radius:2px"></b></span><span>Screenshot 17:32</span><span class="k">now</span>`],
 ['a',`<span class="th" style="background:rgba(255,255,255,.07);font:600 22px Inter;color:rgba(255,255,255,.6)">Aa</span><span>October report — final version</span><span class="k">2 min</span>`],
 ['b',`<span class="th" style="background:rgba(247,178,103,.14);font:700 20px Inter;color:#F7B267">↗</span><span>cmdmeow.com</span><span class="k">5 min</span>`],
 ['c',`<span class="th" style="background:#C8F05A"></span><span class="num">#C8F05A</span><span class="k">12 min</span>`],
 ['d',`<span class="th" style="background:rgba(255,255,255,.07);font:600 20px Inter;color:rgba(255,255,255,.55)">•••</span><span style="letter-spacing:.2em">••••••••••</span><span class="k">password</span>`],
 ['e',`<span class="th" style="background:rgba(255,255,255,.07);font:600 22px Inter;color:rgba(255,255,255,.6)">Aa</span><span>Meeting moved to 3:30 PM</span><span class="k">1 h</span>`],
];
$('cliplist').innerHTML=CLIP.map(c=>`<div class="clip-row" id="cr-${c[0]}">${c[1]}</div>`).join('');
$('wave').innerHTML=Array.from({length:9},()=>'<i></i>').join('');const waveEls=[...root.querySelectorAll('#wave i')];
// ---- clean ----
const GROUPS=[['AI Tools',4.1,'#C8F05A'],['App Caches',3.6,'#A6DE5A'],['Developer',3.9,'#86CA5A'],['Old app leftovers',1.3,'#6AB357'],['Logs & Trash',2.6,'#549852']];const TOT=15.5;
$('legend').innerHTML=GROUPS.map((g,i)=>`<div class="li" id="lgi${i}"><span class="dot" style="background:${g[2]}"></span>${g[0]}<span class="sz num">${g[1].toFixed(1)} GB</span></div>`).join('');
{const R=180;let s=`<circle cx="220" cy="220" r="${R}" fill="none" stroke="rgba(255,255,255,.07)" stroke-width="34"/>`;GROUPS.forEach((g,i)=>s+=`<circle id="seg${i}" cx="220" cy="220" r="${R}" fill="none" stroke="${g[2]}" stroke-width="34" transform="rotate(-90 220 220)"/>`);$('ring').innerHTML=s;}
const SIDE=[['Cleanup','',I.broom],['Disk Map','1.44 TB','<svg width="20" height="20" viewBox="0 0 24 24"><circle cx="12" cy="12" r="9" fill="none" stroke="currentColor" stroke-width="2"/><circle cx="12" cy="12" r="4" fill="none" stroke="currentColor" stroke-width="2"/></svg>'],['Uninstaller','3 apps','<svg width="20" height="20" viewBox="0 0 24 24"><path d="M4 7h16M9 7V4h6v3M6 7l1 13h10l1-13" fill="none" stroke="currentColor" stroke-width="2" stroke-linejoin="round"/></svg>'],['Startup','4 items','<svg width="20" height="20" viewBox="0 0 24 24"><path d="M12 3v8M6.300 7a8 8 0 1011.400 0" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round"/></svg>']];
$('side').innerHTML=SIDE.map((s,i)=>`<div class="si${i===0?' on':''}">${s[2]}<span>${s[0]}</span><span class="st num" id="sst${i}">${s[1]}</span></div>`).join('');
$('bg0').innerHTML=`<div style="display:flex;gap:16px;align-items:center"><svg width="110" height="110" viewBox="0 0 110 110"><circle cx="55" cy="55" r="50" fill="#86CA5A" opacity=".25"/><circle cx="42" cy="48" r="28" fill="#C8F05A"/><circle cx="76" cy="66" r="18" fill="#A6DE5A"/><circle cx="70" cy="30" r="11" fill="#F7B267"/><circle cx="34" cy="82" r="10" fill="#6AB357"/></svg><div><div style="font-weight:600">Disk Map</div><div style="color:rgba(255,255,255,.55);font-size:16px;margin-top:4px">Library 38 GB<br>Movies 22 GB</div></div></div>`;
$('bg1').innerHTML=`<div style="display:flex;gap:16px;align-items:center"><div style="width:76px;height:76px;border-radius:18px;background:linear-gradient(145deg,#6a7cff,#a35bff);display:flex;align-items:center;justify-content:center;font:800 32px Inter">Pe</div><div><div style="font-weight:600">PixelEdit</div><div style="color:rgba(255,255,255,.55);font-size:16px;margin-top:4px">3 leftovers · 1.3 GB</div></div></div><div style="margin-top:14px;display:flex;gap:8px">${['Caches','Support','Logs'].map(x=>`<span style="padding:5px 10px;border-radius:8px;background:rgba(255,255,255,.07);font-size:14px">${x}</span>`).join('')}</div>`;
$('bg2').innerHTML=`<div style="font-weight:600;margin-bottom:10px">Startup</div>${['Steam Helper','Adobe Sync','Zoom Launcher'].map((n,i)=>`<div style="display:flex;align-items:center;height:34px;font-size:16px">${n}<span id="bsw${i}" style="margin-left:auto;width:40px;height:24px;border-radius:12px;background:#C8F05A;position:relative"><i style="position:absolute;top:2px;left:18px;width:20px;height:20px;border-radius:50%;background:#fff"></i></span></div>`).join('')}`;
// ---- health ----
$('hgrid').innerHTML=`
<div class="hc" style="left:30px;top:30px;width:410px;height:214px;display:flex;align-items:center;gap:28px"><div id="hcpuR"></div><div><div class="l">CPU</div><div class="v num" id="hcpu">16%</div></div></div>
<div class="hc" style="left:460px;top:30px;width:410px;height:214px;display:flex;align-items:center;gap:28px"><div id="hmemR"></div><div><div class="l">Memory</div><div class="v num" id="hmem">42%</div></div></div>
<div class="hc" style="left:30px;top:264px;width:410px;height:214px;display:flex;align-items:center;gap:28px"><div id="hbatR"></div><div><div class="l">Battery · health 92%</div><div class="v num">80%</div></div></div>
<div class="hc" style="left:460px;top:264px;width:410px;height:214px;display:flex;align-items:center;gap:28px"><div style="width:130px;height:130px;border-radius:30px;background:rgba(111,214,232,.12);display:flex;align-items:center;justify-content:center">${I.fan(96,'#6FD6E8','hfan')}</div><div><div class="l" id="htemp">Fans · 52°C</div><div class="v num" id="hrpm">1 850</div><div class="l">rpm</div></div></div>`;
$('hbatR').innerHTML=ring(56,.8,'#C8F05A',12);
// ---- AI (like the app: limits with pace, today, saved, 30-day chart, budget) ----
const ACOL='#E8A27A';
$('aitabs').innerHTML=[['Claude',1],['Codex',0],['Gemini',0]].map(([n,on])=>`<span style="height:34px;padding:0 14px;border-radius:17px;display:flex;align-items:center;gap:8px;font:600 15px Inter;${on?'background:rgba(232,162,122,.16);color:#E8A27A':'background:rgba(255,255,255,.05);color:rgba(255,255,255,.6)'}">${n}</span>`).join('');
const LIM=[['Session','Resets 21:10',.64,'On pace','#fff'],['Week','Resets Mon 05:00',.31,'Room to spare','#8BD46A']];
LIM.forEach((l,k)=>$('lim'+k).innerHTML=`<div style="display:flex;justify-content:space-between"><span style="font:600 18px Inter">${l[0]}</span><span class="l" style="font-size:15px">${l[1]}</span></div><div style="display:flex;align-items:center;gap:18px;margin-top:12px"><div style="position:relative;width:80px;height:80px"><div id="lr${k}" class="abs"></div><div id="lp${k}" class="abs num" style="width:80px;height:80px;display:flex;align-items:center;justify-content:center;font:600 20px Inter">0%</div></div><div><div style="font:600 18px Inter;color:${l[4]}">${l[3]}</div><div class="l" style="font-size:15px;margin-top:2px">${k?'Resets before you run out':'Out at 20:40'}</div></div></div>`);
const DAYS=Array.from({length:30},(_,i)=>{const x=Math.sin(i*1.7)*.5+.5,y=Math.sin(i*.37+1)*.5+.5;return .25+.6*x*y+(i%7===5||i%7===6?-.15:0)+(i===29?.25:0)});
$('aibars').innerHTML=DAYS.map((v,i)=>`<i id="db${i}" style="position:absolute;bottom:0;left:${i*(100/30)}%;width:calc(${100/30}% - 4px);border-radius:3px;background:${i===29?ACOL:v>.72?'rgba(255,120,100,.55)':'rgba(232,162,122,.3)'}"></i>`).join('')+`<b class="abs" style="left:0;right:0;bottom:${.72*100}%;border-top:1.5px dashed rgba(255,255,255,.45)"></b>`;
// ---- and more ----
const MC=[
 `<div class="h"><svg width="22" height="22" viewBox="0 0 24 24"><path d="M7 7l10 10-5 5V2l5 5L7 17" fill="none" stroke="#6FD6E8" stroke-width="2" stroke-linejoin="round"/></svg>Bluetooth</div><div style="display:flex;justify-content:space-around;margin-top:22px">${[['AirPods',.38,'#C8F05A'],['Mouse',.14,'#FF9B7A'],['Keyboard',.82,'#C8F05A']].map(([n,p,c])=>`<div style="text-align:center"><div style="display:inline-block">${ring(30,p,c,7)}</div><div style="font:500 15px Inter;color:rgba(255,255,255,.6);margin-top:6px">${n}</div><div class="num" style="font:600 18px Inter;color:${c==='#FF9B7A'?c:'#fff'}">${Math.round(p*100)}%</div></div>`).join('')}</div>`,
 `<div class="h"><svg width="22" height="22" viewBox="0 0 24 24"><path d="M3 16a9 9 0 0118 0M12 16l5-6" fill="none" stroke="#6FD6E8" stroke-width="2" stroke-linecap="round"/></svg>Speed test</div><div style="display:flex;align-items:flex-end;gap:12px;margin-top:34px"><span id="spd" class="num" style="font:300 86px/1 'Inter Display'">0</span><span style="font:500 22px Inter;color:rgba(255,255,255,.6);padding-bottom:10px">Mbps ↓</span></div>`,
 `<div class="h"><svg width="22" height="22" viewBox="0 0 24 24"><path d="M8 7l-5 5 5 5M16 7l5 5-5 5" fill="none" stroke="#F7B267" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"/></svg>Developer</div>${[[':3000','node'],[':5432','postgres'],[':8080','python']].map(([p,n],i)=>`<div style="display:flex;align-items:center;height:44px;font:500 19px Inter;margin-top:${i?0:10}px"><span class="num" style="width:90px;color:#F7B267">${p}</span>${n}<span style="margin-left:auto;padding:4px 12px;border-radius:8px;background:rgba(255,155,122,.16);color:#FF9B7A;font-size:15px">Kill</span></div>`).join('')}`,
 `<div class="h"><svg width="22" height="22" viewBox="0 0 24 24"><path d="M10 14a4 4 0 005.700 0l3-3a4 4 0 00-5.700-5.700l-1 1M14 10a4 4 0 00-5.700 0l-3 3a4 4 0 005.700 5.700l1-1" fill="none" stroke="#C8F05A" stroke-width="2" stroke-linecap="round"/></svg>Share link</div><div style="margin-top:30px;height:64px;border-radius:14px;background:rgba(200,240,90,.1);border:1.5px solid rgba(200,240,90,.4);display:flex;align-items:center;gap:14px;padding:0 18px;font:500 21px Inter"><span style="width:32px;height:32px;border-radius:50%;background:#C8F05A;display:flex;align-items:center;justify-content:center"><svg width="18" height="18" viewBox="0 0 24 24"><path d="M5 12.500l4.500 4.500L19 7.500" fill="none" stroke="#0E1405" stroke-width="3" stroke-linecap="round"/></svg></span>cmdmeow.com/v/8f2k</div><div style="margin-top:10px;font:500 16px Inter;color:rgba(255,255,255,.5)">Link copied</div>`];
MC.forEach((h,i)=>$('mc'+i).innerHTML=h);

// ---- panel geometry ----
const PS=1.4;let SX=0,SY=29,PX=0;const TILE={};
function panelXf(open){$('panel').style.transformOrigin='top left';$('panel').style.transform=`translate(${PX}px,70px) scale(${PS},${PS*lerp(.6,1,open)})`;}
function measure(){const sl=__r($('slot'));SX=sl.left+sl.width/2;PX=Math.min(SX-200*PS,1920-400*PS-24);
  $('panel').style.display='block';panelXf(1);pws.forEach(e=>{e.style.transform='';e.style.opacity=1});
  pws.forEach(e=>{const r=__r(e);TILE[e.id]=[r.left+r.width/2,r.top+r.height/2]});}
// ---- helpers ----
function setCur(x,y,press=0,o=1){const c=$('cur');c.style.display=o>0?'block':'none';c.style.opacity=o;c.style.transform=`translate(${x}px,${y}px) scale(${press?.84:1})`;}
const near=(t,a,w=.07)=>Math.abs(t-a)<w;
function openW(el,t,o,c,from,cx=CX,cy=CY){ // macOS-like zoom from the clicked tile
  if(t<o||t>c){el.style.display='none';return false;}el.style.display='block';
  const w=el.offsetWidth,h=el.offsetHeight,po=eo5(P(t,o,o+.42)),pc=eio(P(t,c-.24,c));
  el.style.left=(cx-w/2)+'px';el.style.top=(cy-h/2)+'px';
  const fx=from?from[0]-cx:0,fy=from?from[1]-cy:0;
  el.style.transform=`translate(${fx*(1-po)}px,${fy*(1-po)}px) scale(${lerp(.12,1,po)*(1-pc*.06)})`;el.style.opacity=clamp(po*2.2)*(1-pc);return true;}
function uiPt(el,x,y){const r=__r(el),s=r.width/el.offsetWidth;return [r.left+x*s,r.top+y*s];}
function ctr(el){const r=__r(el);return [r.left+r.width/2,r.top+r.height/2];}
// panel visit: click menu bar icon at ic, click tile at tc
function visit(t,from,fromT,ic,tc,tile,hold=0){
  const pn=$('panel');const op=eo5(P(t,ic,ic+.3)),cl=eio(P(t,tc+.04,tc+.22));
  if(t>=ic&&t<tc+.22){pn.style.display='block';panelXf(op);pn.style.opacity=clamp(op*3)*(1-cl);}
  const [tx,ty]=TILE[tile];const ix=SX+4,iy=SY+6;
  if(t<ic){const p=eio(P(t,fromT,ic-.04));setCur(lerp(from[0],ix,p),lerp(from[1],iy,p),near(t,ic,.05));}
  else if(t<tc+.3){const p=eio(P(t,ic+.12+hold,tc-.05));setCur(lerp(ix,tx,p),lerp(iy,ty,p),near(t,tc,.06)||near(t,ic,.05));}
  $('slot').style.background=t>=ic&&t<tc+.1?'rgba(255,255,255,.22)':'';
  const te=$(tile);te.style.background=near(t,tc,.12)?'rgba(255,255,255,.2)':'';
}
// left title
const LBL={intro:['One click.',null,'#fff'],shot:['Screenshot',['⇧','⌘','2'],COL.create],mark:['Mark it up',null,COL.create],rec:['Record',['⇧','⌘','5'],COL.create],clip:['Clipboard',['⌥','⌘','V'],COL.util],say:['Dictation',['⌥','D'],COL.util],clean:['Clean up',null,COL.clean],health:['Mac Health',null,COL.monitor],ai:['AI usage',null,COL.monitor],more:['And more.',null,'#fff']};
let curL='';
function lab2(t,s,e,id,keyOn){const L=$('lab');L.style.display='flex';const [txt,keys,col]=LBL[id];
  if(curL!==id){$('ttl').textContent=txt;$('keys').innerHTML=(keys||[]).map(x=>`<span class="key">${x}</span>`).join('');$('keys').style.setProperty('--kc',col);curL=id;}
  const p=eo5(P(t,s,s+.35)),q=eio(P(t,e-.25,e));$('ttl').style.opacity=p*(1-q);$('ttl').style.transform=`translateY(${lerp(40,0,p)-q*30}px)`;$('ttl').style.filter=`blur(${lerp(10,0,p)+q*8}px)`;
  $('keys').style.opacity=eo5(P(t,s+.1,s+.45))*(1-q);root.querySelectorAll('#keys .key').forEach(k=>k.classList.toggle('on',!!keyOn));}

// ================== TIMELINE ==================
const T={intro:K(2),shot:K(3),recV:K(5),rec:K(5.5),clipV:K(7),clip:K(7.5),sayV:K(8.5),say:K(9),cleanV:K(10.5),clean:K(11),hV:K(12.25),h:K(12.75),aiV:K(13.75),ai:K(14.25),more:K(15.25),end:K(16)};
Object.assign($('notesw').style,{left:(1200-410)+'px',top:(640-280)+'px'});
const vic=s0=>s0+.75*b, vtc=s0=>s0+2*b;
function render(t){
  ['lab','cur'].forEach(i=>$(i).style.display='none');$('cat').style.display='none';$('panel').style.display='none';
  root.querySelectorAll('.w,.bgc,.mc').forEach(e=>e.style.display='none');['sel','endwm','endurl','enddl'].forEach(i=>$(i).style.display='none');$('flash').style.opacity=0;
  $('sloticon').style.opacity=t>T.intro+b*1.2?1:0;
  $('slot').style.background='';pws.forEach(e=>{e.style.background='';});
  $('mb').style.display='flex';$('mb').style.opacity=1;$('desk').style.filter='';$('glow').style.background='';
  $('shade').style.opacity=t<T.end?lerp(.55,.28,eo(P(t,T.intro,T.intro+.6))):lerp(.28,.5,eo(P(t,T.end,T.end+.6)));
  // ---- intro: cat flies into menu bar, click, panel cascade ----
  if(t<T.shot+.3){const s=T.intro,ic=s+2*b,tc=T.shot;
    if(t<ic+.1){const g=eio(P(t,s,s+b*1.2));cat(lerp(960,SX,g),lerp(430,SY,g),lerp(280,40,g),1-P(t,s+b*1.1,s+b*1.25),1,0,0);}
    visit(t,[1215,720],s+.3,ic,tc,'t-cap',.5*b);
    pws.forEach((el,i)=>{const d=t-ic-i*(b/5);const p=eo5(clamp(d/.3));el.style.opacity=p;el.style.transform=`translateY(${lerp(16,0,p)}px) scale(${lerp(.92,1,p)})`;});
    if(t>=ic&&t<tc)lab2(t,ic,tc,'intro');
    $('notesw').style.display='block';$('notesw').style.opacity=clamp((t-s-.2)/.4);
    return;}
  pws.forEach(el=>{el.style.opacity=1;el.style.transform='';});
  // ---- screenshot + mark up ----
  if(t<T.recV){const f=T.shot,me=f+2.5*b,e=T.recV;
    const nv=t<me+.4;$('notesw').style.display=nv?'block':'none';
    if(t<me)lab2(t,f,me,'shot',t<f+.3);else lab2(t,me,e,'mark');
    const x0=840,y0=380,x1=1430,y1=720,sp=eio(P(t,f+.5*b,f+2*b));
    if(t<f+2*b+.05){const sel=$('sel');sel.style.display=t>f+.45*b?'block':'none';Object.assign(sel.style,{left:x0+'px',top:y0+'px',width:lerp(0,x1-x0,sp)+'px',height:lerp(0,y1-y0,sp)+'px'});$('seldim').textContent=`${Math.round(lerp(0,x1-x0,sp)*2)} × ${Math.round(lerp(0,y1-y0,sp)*2)}`;
      const [tx,ty]=TILE['t-cap'];const cp=eio(P(t,f+.1,f+.45*b));setCur(lerp(tx,x0,cp)+(t>f+.45*b?lerp(0,x1-x0,sp):0),lerp(ty,y0,cp)+(t>f+.45*b?lerp(0,y1-y0,sp):0));}
    $('flash').style.opacity=t>f+2*b?Math.exp(-(t-f-2*b)*9)*.7:0;
    if(openW($('markw'),t,me,e,[(x0+x1)/2,(y0+y1)/2])){const s=me+.3*b;
      const step=Math.floor((t-s)/b);toolEls.forEach((el,i)=>el.classList.toggle('on',t>s&&i===Math.min(3,step)));
      const fr=eio(P(t,s+3*b,s+3*b+.35)),pad=lerp(0,52,fr);
      Object.assign($('shot').style,{left:pad+'px',top:pad*.8+'px',width:(860-2*pad)+'px',height:(529-1.6*pad)+'px',borderRadius:lerp(0,16,fr)+'px',boxShadow:`0 ${30*fr}px ${60*fr}px rgba(0,0,0,${.55*fr})`});
      $('frame').style.background=`linear-gradient(135deg,rgba(247,178,103,${fr}),rgba(200,240,90,${fr}))`;
      const ap=eo(P(t,s+.1,s+.4)),ax0=560,ay0=230,ax1=380,ay1=112,cxq=540,cyq=120;const ex=lerp(ax0,ax1,ap),ey=lerp(ay0,ay1,ap);
      $('arrp').setAttribute('d',ap>0?`M${ax0} ${ay0} Q${lerp(ax0,cxq,ap)} ${lerp(ay0,cyq,ap)} ${ex} ${ey}`:'');const ang=Math.atan2(ey-lerp(ay0,cyq,ap),ex-lerp(ax0,cxq,ap)),hl=22*clamp((ap-.8)/.2);
      $('arrh').setAttribute('d',hl>0?`M${ex-hl*Math.cos(ang-.5)} ${ey-hl*Math.sin(ang-.5)} L${ex} ${ey} L${ex-hl*Math.cos(ang+.5)} ${ey-hl*Math.sin(ang+.5)}`:'');
      $('blur').style.width=eio(P(t,s+b,s+b+.25))*100+'%';
      const tp=spring(t-s-2*b,9,14);$('txt').style.opacity=t>s+2*b?1:0;$('txt').style.transform=`scale(${tp}) rotate(-4deg)`;
      const ck=e-.5*b;$('copyb').classList.toggle('on',near(t,ck,.15));
      const [bx,by]=ctr($('copyb'));const cp=eio(P(t,s+3.4*b,ck-.05));if(t>s+3.2*b)setCur(lerp(bx-160,bx,cp),lerp(by+260,by,cp),near(t,ck,.06));}
    return;}
  // ---- record ----
  if(t<T.clipV){const s0=T.recV,f=T.rec,e=T.clipV;
    if(t<f+.3)visit(t,[1560,470],s0,vic(s0),vtc(s0),'t-rec');
    if(openW($('recw'),t,f,e,TILE['t-rec'])){const ui=$('recw');lab2(t,f,e,'rec',t<f+.3);const st=1.2*b;
      const rp=eo5(P(t,f+.2,f+.5));$('recpill').style.opacity=rp;$('recpill').style.transform=`translateY(${lerp(-30,0,rp)}px)`;
      $('recdot').style.opacity=(Math.floor((t-f)*3)%2===0)?1:.25;$('rectime').textContent=`0:${String(Math.floor(clamp(t-f-.3,0,9)*2.5)).padStart(2,'0')}`;
      $('cam').style.transform=`scale(${spring(t-f-.35,8,12)})`;
      const ce=Math.abs(((t-f)*2.2)%3-1.5)<.08?1:0;['cel','cer'].forEach(i=>{const h=18*(1-ce*.85);$(i).setAttribute('height',h);$(i).setAttribute('y',54-h/2)});
      const pts=[[200,420],[560,300],[640,0]];
      let seg=clamp(Math.floor((t-f-.3)/st),0,3);const from=seg===0?[480,560]:pts[seg-1],to=pts[Math.min(seg,2)];const q=eio(P(t-f-.3-seg*st,0,st*.6));
      const [cx,cy]=uiPt(ui,lerp(from[0],to[0],q),lerp(from[1],to[1],q));if(t>f+.25)setCur(cx,cy,near(t,f+.3+3*st,.06),1-P(t,e-.3,e-.05));
      $('ripples').innerHTML=[1,2].map(k=>{const d=t-(f+.3+k*st);if(d<0||d>.5)return'';const p=pts[k-1];const R=lerp(10,70,eo(d/.5));return `<i class="abs" style="left:${p[0]-R}px;top:${p[1]-R}px;width:${2*R}px;height:${2*R}px;border-radius:50%;border:4px solid rgba(247,178,103,${1-d/.5})"></i>`}).join('');
      const stopT=f+.3+3*st;$('stopb').style.transform=`scale(${near(t,stopT)?.9:1})`;$('recdot').style.background=t>stopT?'#888':'#FF453A';}
    return;}
  // ---- clipboard ----
  if(t<T.sayV){const s0=T.clipV,f=T.clip,e=T.sayV;
    if(t<f+.3)visit(t,[1250,350],s0,vic(s0),vtc(s0),'t-clip');
    if(openW($('clipw'),t,f,e,TILE['t-clip'])){lab2(t,f,e,'clip',t<f+.3);
      const ins=eo5(P(t,f+.15,f+.5)),rowH=92;let sel=t>f+b?0:-1;if(t>f+1.6*b&&t<f+2.4*b)sel=1;if(t>f+2.4*b)sel=0;
      CLIP.forEach((c,i)=>{const el=$('cr-'+c[0]);let y=12+(c[0]==='shot'?lerp(-40,0,ins):(i-1+ins)*rowH);el.style.opacity=c[0]==='shot'?ins:1;el.style.top=y+'px';el.classList.toggle('on',i===sel);});
      const pk=f+3*b;const [rx,ry]=ctr($('cr-shot'));const cp=eio(P(t,f+.5*b,pk-.05));setCur(lerp(TILE['t-clip'][0],rx+120,cp),lerp(TILE['t-clip'][1],ry,cp),near(t,pk,.06),1-P(t,e-.3,e-.05));}
    return;}
  // ---- dictation: messenger is open first, then Cmd+Meow > Dictation ----
  if(t<T.cleanV){const s0=T.sayV,f=T.say,e=T.cleanV;const ui=$('sayw');
    if(t<f+.3)visit(t,[1250,560],s0+.15,vic(s0)+.15,vtc(s0),'t-dict');
    if(openW(ui,t,s0-.15,e,null)){ui.style.transform=`scale(${lerp(.96,1,eo5(P(t,s0-.15,s0+.25)))*(1-.06*eio(P(t,e-.24,e)))})`;
      if(t>=f)lab2(t,f,e,'say',t>f+.05&&t<f+.3);
      const TXT=[..."Yes! Sending it tomorrow morning 🙌"],t0=f+.4,t1=f+1.6,send=f+1.8;
      const typed=t<send?TXT.slice(0,Math.floor(clamp((t-t0)/(t1-t0))*TXT.length)).join(''):'';
      $('saytext').textContent=typed;$('ph').style.display=typed?'none':'block';$('caret').style.opacity=t>f&&t<send?1:0;
      const ob=$('outb');const op=spring(t-send,8,14);ob.style.display=t>send?'block':'none';$('outt').textContent=TXT.join('');ob.style.opacity=clamp((t-send)/.1);ob.style.transform=`scale(${op})`;
      const pill=$('pill'),cr=__r($('caret')),u=__r(ui);const sc2=u.width/ui.offsetWidth;const px=(cr.left-u.left)/sc2,py=(cr.top-u.top)/sc2;
      const pp=eo5(P(t,f+.15,f+.4))*(1-eo(P(t,t1+.05,t1+.2)));pill.style.left=Math.min(px-20,640)+'px';pill.style.top=(py-74)+'px';pill.style.opacity=pp;pill.style.transform=`scale(${lerp(.7,1,pp)})`;
      waveEls.forEach((el,i)=>{const a=t>t0&&t<t1?.35+.65*Math.abs(Math.sin(t*9+i*1.7)*Math.sin(t*4.3+i)):.15;el.style.height=(6+24*a)+'px';});}
    return;}
  // ---- clean up (+ Disk Map, Uninstaller, Startup in background) ----
  if(t<T.hV){const s0=T.cleanV,f=T.clean,e=T.hV;
    if(t<f+.3)visit(t,[1400,800],s0,vic(s0),vtc(s0),'t-clean');
    const ck=f+3.25*b,em=eio(P(t,ck+.1,ck+.7)),done=t>ck+.75;
    if(openW($('cleanw'),t,f,e,TILE['t-clean'])){const ui=$('cleanw');lab2(t,f,e,'clean');
      const R=180,C=2*Math.PI*R,fp=eo(P(t,f+.2,f+2.5*b));let acc=0;
      GROUPS.forEach((g,i)=>{const sh=g[1]/TOT,gp=clamp((fp-acc)/sh);$('seg'+i).setAttribute('stroke-dasharray',`${Math.max(0,C*sh*eo(gp)*(1-em)-3)} ${C}`);$('seg'+i).setAttribute('transform',`rotate(${-90+acc*360} 220 220)`);$('lgi'+i).style.opacity=clamp(gp*3)*(1-em*.5);acc+=sh;});
      [1,2,3].forEach(i=>{const a=f+(.5+.6*(i-1))*b;$('sst'+i).textContent=t<a?'···':SIDE[i][1];});
      $('cleannum').textContent=(done?TOT:TOT*fp*(1-em)).toFixed(1)+' GB';$('cleannum').style.color=done?'#C8F05A':'#fff';
      $('cleanlbl').textContent=t<f+2.5*b?'Scanning':t<ck+.1?'Found':done?'Freed':'Cleaning';
      $('cleanbtn').textContent=`Clean ${TOT.toFixed(1)} GB`;$('cleanbtn').style.opacity=t>f+2.5*b?(done?.3:1):.3;$('cleanbtn').style.transform=`scale(${near(t,ck)?.92:1})`;
      $('cleannum').style.transform=done?`scale(${1+.1*Math.exp(-(t-ck-.75)*6)*Math.cos((t-ck-.75)*18)})`:'';
      const [bx,by]=ctr($('cleanbtn'));const cp=eio(P(t,f+2*b,ck-.05));if(t>f+.25)setCur(lerp(TILE['t-clean'][0],bx,cp),lerp(TILE['t-clean'][1],by,cp),near(t,ck,.06),1-P(t,e-.3,e-.05));}
    return;}
  // ---- mac health ----
  if(t<T.aiV){const s0=T.hV,f=T.h,e=T.aiV;
    if(t<f+.3)visit(t,[1560,780],s0,vic(s0),vtc(s0),'t-cpu');
    if(openW($('healthw'),t,f,e,TILE['t-cpu'])){lab2(t,f,e,'health');
      const u=P(t,f+.25,e-.2),ld=Math.sin(Math.PI*Math.pow(u,.8));
      const cpu=16+56*ld,mem=42+14*ld,rpm=1850+2450*Math.sin(Math.PI*Math.pow(u,.65)),temp=52+26*ld;
      $('hcpu').textContent=Math.round(cpu)+'%';$('hcpuR').innerHTML=ring(56,cpu/100,cpu>60?'#F7B267':'#6FD6E8',12);
      $('hmem').textContent=Math.round(mem)+'%';$('hmemR').innerHTML=ring(56,mem/100,'#6FD6E8',12);
      $('hrpm').textContent=Math.round(rpm).toLocaleString('en-US').replace(',',' ');$('htemp').textContent=`Fans · ${Math.round(temp)}°C`;
      $('hfan').style.transform=`rotate(${(t-f)*700+u*900}deg)`;$('hfan').style.transformOrigin='50% 50%';
      setCur(...TILE['t-cpu'],0,1-P(t,f,f+.3));}
    return;}
  // ---- AI usage ----
  if(t<T.more){const s0=T.aiV,f=T.ai,e=T.more;
    if(t<f+.3)visit(t,[1500,820],s0,vic(s0),vtc(s0),'t-ai');
    if(openW($('aiw'),t,f,e,TILE['t-ai'])){lab2(t,f,e,'ai');
      LIM.forEach((l,k)=>{const p=eo(P(t,f+.25+k*.12,f+1.1+k*.12));$('lr'+k).innerHTML=ring(34,l[2]*p,ACOL,9);$('lp'+k).textContent=Math.round(l[2]*p*100)+'%';});
      DAYS.forEach((v,i)=>{const p=eo5(P(t,f+.3+i*.018,f+.75+i*.018));$('db'+i).style.height=(v*p*100)+'%';});
      const q=eo(P(t,f+.3,f+1.4));$('aitok').textContent=(5.1*q).toFixed(1)+'M';$('aiusd').textContent='$'+Math.round(38*q);$('aisaved').textContent='$'+Math.round(312*q);
      $('aibud').style.width=(34*eo(P(t,f+.5,f+1.3)))+'%';
      setCur(...TILE['t-ai'],0,1-P(t,f,f+.3));}
    return;}
  // ---- and more ----
  if(t<T.end){const f=T.more,e=T.end;lab2(t,f,e,'more');
    MC.forEach((_,i)=>{const el=$('mc'+i);el.style.display='block';const a=f+i*b*.6;const p=spring(t-a,8,13);
      el.style.left=(CX-434+(i%2)*448)+'px';el.style.top=(CY-244+Math.floor(i/2)*258)+'px';el.style.opacity=clamp((t-a)/.12)*(1-P(t,e-.22,e));el.style.transform=`translateY(${(1-eo5(P(t,a,a+.35)))*40}px) scale(${t<a?0:lerp(.85,1,p)})`;});
    $('spd').textContent=Math.round(940*eo(P(t,f+b*.6+.1,f+b*.6+1)));
    return;}
  // ---- end card ----
  {const s=T.end,hit=s+b;$('mb').style.opacity=1-P(t,s,s+.4);
    const size=300,cy=400;const pop=spring(t-s,6,10)*(t>hit?1+.12*Math.exp(-(t-hit)*7)*Math.cos((t-hit)*22):1);
    cat(960,cy,size*pop,1,0,0,1);$('kears').setAttribute('transform',`translate(0 ${t>hit+.3?(1-spring(t-hit-.3,9,22))*6:6})`);
    const a=eo5(P(t,hit+.35,hit+.7)),c=eo5(P(t,hit+.6,hit+.95)),d=eo5(P(t,hit+.85,hit+1.2));
    ['endwm','endurl'].forEach(i=>$(i).style.display='block');$('enddl').style.display='inline-flex';
    const wm=$('endwm');wm.style.opacity=a;wm.style.transform=`translate(${960-wm.offsetWidth/2+16}px,${600+lerp(20,0,a)}px)`;
    const u=$('endurl');u.style.opacity=c;u.style.transform=`translate(${960-u.offsetWidth/2}px,${700+lerp(20,0,c)}px)`;
    const dl=$('enddl');dl.style.opacity=d;dl.style.transform=`translate(${960-dl.offsetWidth/2}px,${780+lerp(20,0,d)}px)`;
    $('glow').style.background=`radial-gradient(900px 700px at 960px 480px, rgba(34,64,29,${.6*a}) 0%, transparent 70%)`;}
}
return {render, init: measure};
}
