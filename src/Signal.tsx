import { useId } from 'react';
import type { SignalStyle, Status } from './model';

type SignalProps = { status: Status; style?: SignalStyle; motion?: boolean; className?: string };

export function Signal({status, style='classic', motion=true, className=''}: SignalProps) {
  const classes = `${className} signal-${status} ${motion?'signal-motion':'signal-static'}`;
  return style === 'playful' ? <PlayfulSignal status={status} className={classes}/> : <ClassicSignal status={status} className={classes}/>;
}

function ClassicSignal({status, className=''}: SignalProps) {
  const id = `signal-${useId().replace(/:/g, '')}`;
  const active = status === 'attention' ? 0 : status === 'repairing' ? 1 : status === 'running' || status === 'completed' ? 2 : -1;
  const lenses = [
    {on: ['#ff9a85', '#f34135', '#8b1917'], off: ['#493035', '#28171b', '#150d10']},
    {on: ['#fff8b3', '#f5c439', '#8b6016'], off: ['#403a29', '#282315', '#16130c']},
    {on: ['#c8ffc0', '#59d16b', '#176434'], off: ['#2b4235', '#172a20', '#0d1711']},
  ];
  return <svg className={`signal signal-classic ${className}`} viewBox="0 0 150 292" role="img" aria-label={`信号灯：${status}`}>
    <defs>
      <linearGradient id={`${id}-rim`} x1="0" y1="0" x2="1" y2=".8">
        <stop stopColor="#d0d1d1"/><stop offset=".13" stopColor="#a3a5a5"/><stop offset=".42" stopColor="#bfc1c1"/><stop offset=".73" stopColor="#7d8082"/><stop offset="1" stopColor="#b4b6b7"/>
      </linearGradient>
      <linearGradient id={`${id}-body`} x1="0" y1="0" x2="1" y2=".4">
        <stop stopColor="#303134"/><stop offset=".25" stopColor="#222326"/><stop offset=".7" stopColor="#161719"/><stop offset="1" stopColor="#202123"/>
      </linearGradient>
      <linearGradient id={`${id}-socket`} x1="0" y1="0" x2="0" y2="1">
        <stop stopColor="#08090a"/><stop offset=".48" stopColor="#141517"/><stop offset="1" stopColor="#4c4d4f"/>
      </linearGradient>
      <linearGradient id={`${id}-glass-reflection`} x1="0" y1="0" x2="0" y2="1">
        <stop stopColor="#ffffff" stopOpacity=".42"/><stop offset="1" stopColor="#ffffff" stopOpacity="0"/>
      </linearGradient>
      {lenses.map((lens, index) => <g key={index}>
        <radialGradient id={`${id}-on-${index}`} cx="38%" cy="27%" r="78%">
          <stop stopColor={lens.on[0]}/><stop offset=".45" stopColor={lens.on[1]}/><stop offset="1" stopColor={lens.on[2]}/>
        </radialGradient>
        <radialGradient id={`${id}-off-${index}`} cx="40%" cy="24%" r="85%">
          <stop stopColor={lens.off[0]}/><stop offset=".55" stopColor={lens.off[1]}/><stop offset="1" stopColor={lens.off[2]}/>
        </radialGradient>
      </g>)}
    </defs>
    <rect x="20" y="4" width="110" height="284" rx="55" fill={`url(#${id}-rim)`} stroke="#9b9d9f" strokeWidth="1"/>
    <rect x="26" y="10" width="98" height="272" rx="49" fill={`url(#${id}-body)`} stroke="#0a0b0c" strokeWidth="1"/>
    <path d="M28 59a47 47 0 0 1 47-47" fill="none" stroke="#b5b6b7" strokeOpacity=".16" strokeWidth="1.2" strokeLinecap="round"/>
    <path d="M126 231a51 51 0 0 1-51 53" fill="none" stroke="#77797b" strokeOpacity=".22" strokeWidth="1" strokeLinecap="round"/>
    {lenses.map((_, index) => {
      const cy = 60 + index * 83;
      const lit = active === index;
      return <g key={index}>
        <circle cx="75" cy={cy} r="38" fill={`url(#${id}-socket)`} stroke="#0b0c0d" strokeWidth="1.2"/>
        <circle cx="75" cy={cy} r="34" fill="#080a09" stroke="#636568" strokeOpacity=".3" strokeWidth=".7"/>
        <g className={`lamp ${lit?'lamp-active':''}`} key={`${index}-${status}`}>
          <circle cx="75" cy={cy} r="32" fill={`url(#${id}-${lit ? 'on' : 'off'}-${index})`} stroke="#060808" strokeWidth=".8"/>
          <path d={`M49 ${cy-5}c1-16 12-25 26-25s25 9 26 25c-8-7-18-10-26-10S57 ${cy-12} 49 ${cy-5}Z`} fill={`url(#${id}-glass-reflection)`} opacity={lit ? .8 : .18}/>
          <ellipse cx="64" cy={cy-18} rx="9" ry="3.2" fill="#ffffff" opacity={lit ? .23 : .045} transform={`rotate(-27 64 ${cy-18})`}/>
          <path d={`M56 ${cy+23}a29 29 0 0 0 35-1`} fill="none" stroke="#ffffff" strokeOpacity={lit ? .16 : .035} strokeWidth=".9" strokeLinecap="round"/>
        </g>
        {lit && index===0 && <circle className="attention-ring" cx="75" cy={cy} r="36.5" fill="none" stroke="#ff6b5d" strokeWidth="1.5"/>}
      </g>;
    })}
  </svg>;
}

function PlayfulSignal({status, className=''}: {status:Status;className?:string}) {
  const active = status === 'attention' ? 0 : status === 'repairing' ? 1 : status === 'running' || status === 'completed' ? 2 : -1;
  const colors = ['#f47566','#f4cd65','#9ec68a'];
  return <svg className={`signal ${className}`} viewBox="0 0 150 292" role="img" aria-label={`信号灯：${status}`}>
    <path d="M66 264h18v24H66z" fill="#898e83" stroke="#30382f" strokeWidth="3.5"/>
    <rect x="25" y="9" width="100" height="266" rx="48" fill="#a7ada0" stroke="#30382f" strokeWidth="3.5"/>
    <path d="M36 204v18c0 26 17 43 39 43 16 0 27-6 36-19" fill="none" stroke="#8a9485" strokeWidth="3" strokeLinecap="round"/>
    {[0,1,2].map((i) => <g key={`${i}-${status}`} className={`lamp ${active===i?'lamp-active':''}`} >
      <circle cx="75" cy={62+i*80} r="35" fill={active===i?colors[i]:'#798172'} stroke="#30382f" strokeWidth="3.5"/>
      <path d={`M42 ${53+i*80}c3-18 15-27 33-27s30 9 33 27`} fill="none" stroke={active===i?'#ffffff85':'#b4bdad80'} strokeWidth="5" strokeLinecap="round"/>
      <g opacity={active===i ? 1 : .38} stroke="#30382f" strokeWidth="3.4" strokeLinecap="round" fill="none" transform={`translate(0 ${i*80})`}>
        {i===0?<><path d="M64 58v4q0 4 3 0v-4M83 58v4q0 4 3 0v-4"/><path d="M72 73h6"/></>:i===1?<><path d="m62 58 5 3-5 3m24-6-5 3 5 3"/><circle cx="75" cy="73" r="1.5" fill="#30382f" stroke="none"/></>:<><path d="M64 59v2m22-2v2M63 71q12 14 24 0"/></>}
      </g>
      {active===0 && i===0 && <circle className="attention-ring" cx="75" cy="62" r="37" fill="none" stroke="#e25c4b" strokeWidth="1.8"/>}
    </g>)}
    <path d="M54 289h42" stroke="#30382f" strokeWidth="3.5" strokeLinecap="round"/>
  </svg>;
}
