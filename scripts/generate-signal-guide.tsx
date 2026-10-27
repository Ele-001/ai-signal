/** Reuse the shipped lamp artwork so the README matches the desktop app. */
import React from 'react';
Object.assign(globalThis,{React});
import {renderToStaticMarkup} from 'react-dom/server';
import {writeFileSync} from 'node:fs';
import {Signal} from '../src/Signal';
import type {Status} from '../src/model';
const font='-apple-system,BlinkMacSystemFont,"PingFang SC","Microsoft YaHei","Noto Sans CJK SC",sans-serif';
const escape=(s:string)=>s.replaceAll('&','&amp;').replaceAll('<','&lt;');
const text=(x:number,y:number,s:string,size=20,fill='#1d1d1f',weight=400)=>`<text x="${x}" y="${y}" font-size="${size}" fill="${fill}" font-weight="${weight}">${escape(s)}</text>`;
function lamp(status:Status,x:number,y:number,width=112,height=218) {
  let svg=renderToStaticMarkup(<Signal status={status} motion={false}/>);
  svg=svg.replace(/id="([^"]+)"/g,(_,id)=>`id="guide-${status}-${id}"`).replace(/url\(#([^)]+)\)/g,(_,id)=>`url(#guide-${status}-${id})`);
  return svg.replace('<svg ',`<svg x="${x}" y="${y}" width="${width}" height="${height}" `);
}
function page(height:number,body:string) {
  return `<svg xmlns="http://www.w3.org/2000/svg" width="1320" height="${height}" viewBox="0 0 1320 ${height}" role="img"><style>text{font-family:${font}}.attention-ring{opacity:0}.signal-completed .lamp-active{opacity:.58}</style><rect width="1320" height="${height}" fill="#f5f5f7"/>${body}</svg>`;
}
const states:{status:Status;title:string;effect:string;color:string;lines:string[];rhythm:number[]}[]=[
 {status:'attention',title:'需要处理',effect:'红灯常亮',color:'#e44338',lines:['等待回答或已接入的审批','锁屏、连接异常或状态待确认','有任一待处理项，就保持红灯'],rhythm:Array(24).fill(1)},
 {status:'repairing',title:'自动修复',effect:'黄灯规律双闪',color:'#c68a16',lines:['来源明确上报正在修复','或手动切换到修复状态','普通错误不会自动变黄灯'],rhythm:[1,1,.16,1,1,.16,.16,.16,.16,.16,.16,.16,1,1,.16,1,1,.16,.16,.16,.16,.16,.16,.16]},
 {status:'running',title:'正在运行',effect:'绿灯不规则短闪',color:'#319648',lines:['AI 正在处理已识别的任务','或原任务已恢复并有新进展','闪烁节奏不代表实时负载'],rhythm:[1,.18,.18,1,1,.18,1,.18,.18,.18,1,.18,1,1,.18,.18,1,.18,.18,.18,.18,1,.18,.18]},
 {status:'completed',title:'已完成',effect:'绿灯确认双闪后常亮',color:'#319648',lines:['手动标记完成','或来源报告本轮结束','本轮结束不等于结果已验收'],rhythm:[1,1,.2,.2,1,1,.58,.58,.58,.58,.58,.58,.58,.58,.58,.58,.58,.58,.58,.58,.58,.58,.58,.58]},
 {status:'idle',title:'空闲',effect:'三盏灯全部熄灭',color:'#8b8b91',lines:['任务已被明确标记为空闲','或列表中没有任务','未收到事件不代表一切正常'],rhythm:Array(24).fill(.14)},
];
let body=text(40,65,'灯色与状态',34,'#1d1d1f',650)+text(40,102,'适用于已连接且可读取的任务；以下灯效为默认动态模式。',19,'#6e6e73');
states.forEach((s,index)=>{
 const x=40+index*252;
 if(index)body+=`<path d="M${x-18} 145V625" stroke="#dedee3"/>`;
 body+=lamp(s.status,x+58,143);
 body+=text(x,405,s.title,25,'#1d1d1f',600)+text(x,443,s.effect,18,s.color,500);
 s.rhythm.forEach((opacity,i)=>body+=`<rect x="${x+i*9}" y="467" width="6" height="14" rx="2" fill="${s.color}" opacity="${opacity}"/>`);
 s.lines.forEach((line,i)=>body+=text(x,523+i*33,line,17,'#626268'));
});
body+=`<path d="M40 655H1280" stroke="#dedee3"/>`+text(40,695,'汇总优先级：需要处理  ›  自动修复  ›  正在运行  ›  已完成  ›  空闲',21,'#1d1d1f',500)+text(40,733,'节奏条为示意。关闭动态灯效或启用系统“减少动态效果”后，灯静态显示，状态以文字区分。',17,'#6e6e73');
writeFileSync('docs/images/signal-states.svg',page(770,body));
const rows=[
 ['等待用户处理','回答问题，或处理已接入的审批','全部待答项解除后，按任务当前状态切换。'],
 ['工作期间锁屏或监测中断','桌面恢复可用，且所有受影响原任务恢复或结束','仅解锁不会清除仍然受阻的任务；无关任务的进度不算恢复。'],
 ['连续 2 分钟没有新进展','同一个任务收到新进展，或明确报告结束','长耗时操作也可能触发“状态待确认”，不代表已经确认失败。'],
 ['连接或状态读取异常','对应来源、接收端或内部连接实际恢复','故障恢复后自动解除，无需手动清除提醒。'],
];
let recovery=text(40,65,'红灯何时自动解除',34,'#1d1d1f',650)+text(40,102,'只有阻塞解除或出现可靠的新状态，才会重新汇总灯色。',19,'#6e6e73');
recovery+=lamp('attention',68,205,142,276)+text(69,530,'红灯常亮',22,'#bd342c',600);
rows.forEach((row,i)=>{
 const y=164+i*122;
 recovery+=`<circle cx="300" cy="${y-7}" r="5" fill="#e44338"/>`+text(320,y,row[0],21,'#1d1d1f',600)+text(320,y+36,'解除条件：'+row[1],19,'#1d1d1f')+text(320,y+66,row[2],17,'#6e6e73');
 if(i<3)recovery+=`<path d="M300 ${y+90}H1280" stroke="#dedee3"/>`;
});
recovery+=`<path d="M40 660H1280" stroke="#dedee3"/>`+text(40,700,'恢复运行 → 绿闪　　明确结束 → 绿常亮　　空闲 → 全灭',22,'#287a3d',500)+text(40,739,'仍有其他待处理项时，汇总继续红灯。手动模拟任务需手动切换；未接入的事件无法保证即时识别。',17,'#6e6e73');
writeFileSync('docs/images/signal-recovery.svg',page(775,recovery));
console.log('Generated two SVG guides from the actual Signal component.');
