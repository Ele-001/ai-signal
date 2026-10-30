import { memo, useId } from 'react';
import type { SignalStyle, Status } from './model';

type SignalProps = { status: Status; style?: SignalStyle; motion?: boolean; className?: string };

export const Signal = memo(function Signal({status, style='classic', motion=true, className=''}: SignalProps) {
  const classes = `${className} signal-${status} ${motion?'signal-motion':'signal-static'}`;
  return style === 'classic' ? <ClassicSignal status={status} className={classes}/> : <MetalSignal status={status} style={style} className={classes}/>;
});

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

function MetalSignal({status, style, className=''}: SignalProps) {
  const id = `metal-${useId().replace(/:/g, '')}`;
  const mechanical = style === 'mechanical';
  const active = status === 'attention' ? 0 : status === 'repairing' ? 1 : status === 'running' || status === 'completed' ? 2 : -1;
  const colors = ['#ff392d', '#ffc526', '#24dc68'];
  const dark = ['#3a0d11', '#382909', '#092b1c'];
  return <svg className={`signal signal-${style} ${className}`} viewBox="0 0 150 292" role="img" aria-label={`信号灯：${status}`}>
    <defs>
      <linearGradient id={`${id}-body`} x1="0" y1="0" x2="1" y2=".2">
        {(mechanical ? ['#10171e','#46515d','#202830','#10161d'] : ['#fbfcfd','#d8dde2','#f4f6f7','#adb5be']).map((color,i)=><stop key={i} offset={i/3} stopColor={color}/>)}
      </linearGradient>
      <linearGradient id={`${id}-bezel`} x1="0" y1="0" x2=".8" y2="1">
        <stop stopColor={mechanical?'#85949f':'#fff'}/><stop offset=".28" stopColor={mechanical?'#151c22':'#939da5'}/><stop offset=".6" stopColor="#11181b"/><stop offset="1" stopColor={mechanical?'#60717d':'#fafcfd'}/>
      </linearGradient>
      <linearGradient id={`${id}-shine`} x2=".8" y2="1"><stop stopColor="#fff" stopOpacity=".85"/><stop offset="1" stopColor="#fff" stopOpacity="0"/></linearGradient>
      <pattern id={`${id}-mesh`} width="9" height="15.6" patternUnits="userSpaceOnUse"><path d="M4.5 0 9 2.6v5.2l-4.5 2.6L0 7.8V2.6ZM0 7.8v5.2l4.5 2.6L9 13V7.8" fill="none" stroke="#dfffee" strokeWidth=".45" opacity=".35"/></pattern>
      {colors.map((color,i)=><g key={i}>
        <radialGradient id={`${id}-on-${i}`} cx="57%" cy="73%" r="77%"><stop stopColor={i===1?'#fff18b':i===0?'#ff9c72':'#9bffbd'}/><stop offset=".4" stopColor={color}/><stop offset=".82" stopColor={dark[i]}/><stop offset="1" stopColor="#030906"/></radialGradient>
        <radialGradient id={`${id}-off-${i}`} cx="45%" cy="28%" r="80%"><stop stopColor={dark[i]}/><stop offset="1" stopColor="#080d11"/></radialGradient>
      </g>)}
    </defs>
    {mechanical ? <>
      <path d="M35 5h80l18 18v246l-18 18H35l-18-18V23Z" fill={`url(#${id}-body)`} stroke="#090e13" strokeWidth="2"/>
      <path d="M37 11h76l14 15v240l-14 15H37l-14-15V26Z" fill="none" stroke="#87949f" strokeOpacity=".55"/>
      <path d="M29 41v210m92-210v210M42 17h66M42 275h66" stroke="#0a1015" strokeWidth="2"/>
      {[24,268].flatMap(y=>[36,114].map(x=><g key={`${x}-${y}`}><circle cx={x} cy={y} r="3.4" fill="#10171e" stroke="#697782" strokeWidth=".8"/><path d={`m${x-1.3} ${y+1.3} 2.6-2.6`} stroke="#86939d" strokeWidth=".6"/></g>))}
    </> : <>
      <rect x="18" y="4" width="114" height="284" rx="48" fill={`url(#${id}-body)`} stroke="#a8afb6"/>
      <rect x="21" y="7" width="108" height="278" rx="45" fill="none" stroke="#fff" strokeOpacity=".85"/>
      <path d="M20 102h110m-110 87h110" stroke="#8c969f" strokeWidth=".7"/><path d="M21 103h108m-108 87h108" stroke="#fff" strokeOpacity=".7"/>
    </>}
    {colors.map((color,i)=>{
      const cy=60+i*86, lit=i===active;
      return <g key={i}>
        <circle cx="75" cy={cy} r="43" fill={`url(#${id}-bezel)`}/>
        <circle cx="75" cy={cy} r="39.5" fill="#070d10" stroke={mechanical?'#72818c':'#11191b'} strokeWidth={mechanical?1.2:.8}/>
        <circle cx="75" cy={cy} r="36.8" fill="none" stroke={color} strokeOpacity={lit?.48:.12} strokeWidth={mechanical?2:1}/>
        <g className={`lamp ${lit?'lamp-active':''}`} key={`${i}-${status}`}>
          <circle cx="75" cy={cy} r="35" fill={`url(#${id}-${lit?'on':'off'}-${i})`} stroke="#03080a"/>
          {mechanical&&<circle cx="75" cy={cy} r="33" fill={`url(#${id}-mesh)`} opacity={lit?.7:.15}/>}
          <path d={`M44 ${cy-10}a33 33 0 0 1 48-19L63 ${cy-1}Z`} fill={`url(#${id}-shine)`} opacity={lit?.8:.16}/>
          <path d={`M49 ${cy+22}a32 32 0 0 0 47-2`} fill="none" stroke={lit?color:'#46525b'} strokeWidth="2" strokeOpacity={lit?.9:.18}/>
          <path d={`M52 ${cy-22}a31 31 0 0 1 22-9`} fill="none" stroke="#fff" strokeWidth="1.6" strokeOpacity={lit?.85:.16}/>
        </g>
        {mechanical&&<><path d={`M35 ${cy-25}v-9h9m62 9v-9h-9M35 ${cy+25}v9h9m62-9v9h-9`} fill="none" stroke="#91a0aa" strokeOpacity=".5" strokeWidth=".8"/><path d={`M25 ${cy-10}v20m100-20v20`} stroke={lit?color:'#3e4a54'} strokeWidth="1.4"/></>}
        {lit&&i===0&&<circle className="attention-ring" cx="75" cy={cy} r="38" fill="none" stroke={color} strokeWidth="1.3"/>}
      </g>;
    })}
  </svg>;
}
