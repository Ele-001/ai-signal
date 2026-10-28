import { useEffect, useRef, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import { getCurrentWindow } from '@tauri-apps/api/window';
import { Check, GripHorizontal, Pin, PinOff, Plus, SlidersHorizontal, Trash2, X } from 'lucide-react';
import { Signal } from './Signal';
import {Connections} from './Connections';
import {TaskSync} from './TaskSync';
import type {BridgeInfo} from './Connections';
import { aggregate, defaults, info, statuses } from './model';
import type { SignalStyle, Status, Task } from './model';
import { actions, isDesktop, useStore } from './store';

type Page = 'tasks' | 'appearance' | 'connection';
const widgetView = new URLSearchParams(location.search).get('view') === 'widget';
const labels:Record<Status,string> = {attention:'待处理',repairing:'修复中',running:'运行中',completed:'已完成',idle:'空闲'};
let audioContext: AudioContext | undefined;
async function playSound(kind: 'attention'|'completed') {
  if(isDesktop) return invoke('play_signal_sound',{kind});
  audioContext ??= new AudioContext();
  await audioContext.resume();
  const oscillator=audioContext.createOscillator(),gain=audioContext.createGain();
  oscillator.connect(gain);gain.connect(audioContext.destination);
  oscillator.frequency.value=kind==='attention'?740:520;
  gain.gain.setValueAtTime(.06,audioContext.currentTime);gain.gain.exponentialRampToValueAtTime(.001,audioContext.currentTime+.3);
  oscillator.start();oscillator.stop(audioContext.currentTime+.3);
}

export default function App() {
  const store=useStore();
  const tasks=[...store.tasks,...store.external,...store.monitoring,...store.transport];
  const status=aggregate(tasks);
  const [page,setPage]=useState<Page>('tasks');
  const [error,setError]=useState('');
  const [bridge,setBridge]=useState<BridgeInfo>();
  const previous=useRef(status);
  const [now,setNow]=useState(Date.now());
  function native(command:string,args?:Record<string,unknown>) {return invoke(command,args).catch(e=>setError(String(e)));}
  useEffect(()=>{
    document.body.classList.toggle('widget-mode',widgetView);
    if (!isDesktop) return;
    let disposed=false,refreshing=false;
    const removal=listen<string>('external-task-removed',event=>actions.removeExternal(event.payload));
    const subscription=listen<Task>('external-task',event=>actions.external(event.payload));
    const protection=listen<Task[]>('monitor-tasks',event=>actions.monitoring(event.payload));
    async function refresh() {
      if(refreshing)return;refreshing=true;
      let timeout:ReturnType<typeof setTimeout>|undefined;
      try {
        const [initial,alerts,bridgeInfo]=await Promise.race([Promise.all([invoke<Task[]>('list_external_tasks'),invoke<Task[]>('list_monitor_tasks'),invoke<BridgeInfo>('bridge_info')]),new Promise<never>((_,reject)=>{timeout=setTimeout(()=>reject(new Error('监测响应超时')),3000);})]);
        if(!disposed) {actions.externalSnapshot(initial);actions.monitoring(alerts);actions.transportError(false);setBridge(bridgeInfo);}
      } catch(e) {
        if(!disposed) actions.transportError(true);
      } finally {clearTimeout(timeout);refreshing=false;}
    }
    void Promise.all([subscription,removal,protection]).then(refresh);
    const timer=setInterval(()=>void refresh(),5000);
    return()=>{disposed=true;clearInterval(timer);for(const listener of [subscription,removal,protection])void listener.then(fn=>fn());};
  },[]);
  useEffect(()=>{if(isDesktop) void native('set_widget_preferences',{alwaysOnTop:store.settings.alwaysOnTop,scale:store.settings.scale});},[store.settings.alwaysOnTop,store.settings.scale]);
  useEffect(()=>{
    if(previous.current!==status && store.settings.sound && (!isDesktop || widgetView) && (status==='attention'||status==='completed')) void playSound(status).catch(e=>setError(`无法播放提示音：${String(e)}`));
    previous.current=status;
  },[status,store.settings.sound]);
  useEffect(()=>{const timer=setInterval(()=>setNow(Date.now()),30_000);return()=>clearInterval(timer);},[]);
  useEffect(()=>{if(!error)return;const timer=setTimeout(()=>setError(''),7000);return()=>clearTimeout(timer);},[error]);
  useEffect(()=>{const key=(e:KeyboardEvent)=>{if(e.key==='Escape' && isDesktop && !widgetView) void native('hide_panel');};window.addEventListener('keydown',key);return()=>window.removeEventListener('keydown',key);},[]);
  const openPanel=()=>isDesktop?void native('show_panel'):window.open('/?view=panel','ai-signal-panel');
  const showWidget=()=>isDesktop?void native('show_widget'):window.open('/?view=widget','ai-signal-widget','width=180,height=380');
  const togglePin=()=>actions.settings({alwaysOnTop:!store.settings.alwaysOnTop});
  const toggleSound=()=>{actions.settings({sound:!store.settings.sound});if(!store.settings.sound)void playSound('attention').catch(e=>setError(String(e)));};
  const drag=()=>{if(isDesktop) void getCurrentWindow().startDragging().catch(e=>setError(String(e)));};
  if(widgetView) return <div className="widget" style={{transform:`scale(${store.settings.scale})`,opacity:store.settings.opacity}}>
    <div className="widget-tools">
      <button onClick={togglePin} aria-label={store.settings.alwaysOnTop?'取消置顶':'置顶'} title="切换置顶">{store.settings.alwaysOnTop?<Pin size={13}/>:<PinOff size={13}/>}</button>
      <span onMouseDown={drag} title="拖动"><GripHorizontal size={15}/></span>
      <button onClick={openPanel} aria-label="打开控制面板" title="打开控制面板"><SlidersHorizontal size={13}/></button>
    </div>
    <div className="widget-drag" onMouseDown={e=>{if(e.button===0)drag();}} onDoubleClick={openPanel}><Signal status={status} style={store.settings.style} motion={store.settings.motion}/></div>
    <button className={`widget-caption ${status}`} onClick={openPanel}><span className="status-dot"/>{labels[status]}{tasks.filter(t=>t.status==='attention').length>1&&` · ${tasks.filter(t=>t.status==='attention').length}`}</button>
    {error&&<div className="widget-error" title={error}>操作失败</div>}
  </div>;
  const attention=tasks.filter(t=>t.status==='attention').length;
  return <div className="app-shell">
    <header className="app-header">
      <span className="app-name">AI Signal</span>
      <nav className="tabs" aria-label="主导航">{([['tasks','任务'],['appearance','外观'],['connection','连接']] as const).map(([value,label])=><button key={value} className={page===value?'selected':''} aria-current={page===value?'page':undefined} onClick={()=>setPage(value)}>{label}</button>)}</nav>
      <button className="button small" onClick={showWidget}>显示悬浮灯</button>
    </header>
    <main className="content">
      {!isDesktop&&<div className="preview-notice">浏览器预览<span>拖拽、置顶和托盘需使用桌面应用</span></div>}
      {store.storageError&&<div role="alert" className="notice">{store.storageError}</div>}
      {page==='tasks'&&<>
        <section className="summary" aria-label="汇总状态">
          <Signal status={status} style={store.settings.style} motion={store.settings.motion}/>
          <div><h1><span className={`status-dot ${status}`}/>{info[status].label}</h1>{tasks.length>0&&<p>{tasks.length} 项任务{attention>0&&` · ${attention} 项等待处理`}</p>}</div>
        </section>
        <section className="simulator" aria-label="手动模拟"><span>模拟状态</span><div className="status-options">{statuses.map(s=><button key={s} disabled={store.tasks.length>=100&&!store.tasks.some(t=>t.id==='manual-simulator')} aria-pressed={store.tasks.find(t=>t.id==='manual-simulator')?.status===s} className={store.tasks.find(t=>t.id==='manual-simulator')?.status===s?'selected':''} onClick={()=>actions.simulate(s)}><i className={`status-dot ${s}`}/>{labels[s]}</button>)}</div></section>
        <TaskSync onError={setError}/><TaskList tasks={tasks} now={now} onError={setError}/>
      </>}
      {page==='appearance'&&<>
        <h1 className="page-title">外观</h1>
        <section className="group style-section" aria-label="信号灯样式"><h2>灯体样式</h2><div className="style-options">{([['classic','经典'],['playful','趣味']] as const).map(([value,label])=><button key={value} aria-pressed={store.settings.style===value} className={`style-option ${store.settings.style===value?'selected':''}`} onClick={()=>actions.settings({style:value as SignalStyle})}><div><Signal status={status==='idle'?'completed':status} style={value} motion={store.settings.motion}/></div><span>{label}{store.settings.style===value&&<Check size={13}/>}</span></button>)}</div></section>
        <section className="group" aria-label="悬浮灯设置">
          <SettingRow title="始终置顶"><Toggle value={store.settings.alwaysOnTop} onChange={togglePin} label="始终置顶"/></SettingRow>
          <SettingRow title="大小"><div className="slider-control"><input aria-label="灯体缩放" type="range" min=".7" max="1.6" step=".05" value={store.settings.scale} onChange={e=>actions.settings({scale:Number(e.target.value)})}/><output>{Math.round(store.settings.scale*100)}%</output></div></SettingRow>
          <SettingRow title="不透明度"><div className="slider-control"><input aria-label="灯体不透明度" type="range" min=".4" max="1" step=".05" value={store.settings.opacity} onChange={e=>actions.settings({opacity:Number(e.target.value)})}/><output>{Math.round(store.settings.opacity*100)}%</output></div></SettingRow>
          <SettingRow title="动态灯效"><Toggle value={store.settings.motion} onChange={()=>actions.settings({motion:!store.settings.motion})} label="动态灯效"/></SettingRow>
          <SettingRow title="声音提醒"><Toggle value={store.settings.sound} onChange={toggleSound} label="声音提醒"/></SettingRow>
        </section>
        <div className="settings-foot"><span>设置自动保存</span><button className="text-button" onClick={()=>actions.settings({...defaults.settings})}>恢复默认</button></div>
      </>}
      {page==='connection'&&<Connections bridge={bridge} onError={setError}/>}
    </main>
    {error&&<div role="alert" className="toast"><span>{error}</span><button aria-label="关闭提示" onClick={()=>setError('')}><X size={16}/></button></div>}
  </div>;
}
function Toggle({value,onChange,label}:{value:boolean;onChange:()=>void;label:string}){return <button className={`toggle ${value?'checked':''}`} role="switch" aria-label={label} aria-checked={value} onClick={onChange}><span/></button>;}
function SettingRow({title,children}:{title:string;children:React.ReactNode}){return <div className="setting-row"><span>{title}</span>{children}</div>;}
function formatAge(at:number,now:number){const minutes=Math.max(0,Math.floor((now-at)/60_000));return minutes===0?'刚刚':minutes<60?`${minutes} 分钟前`:`${Math.floor(minutes/60)} 小时前`;}
function TaskList({tasks,now,onError}:{tasks:Task[];now:number;onError:(error:string)=>void}) {
  const [adding,setAdding]=useState(false),[title,setTitle]=useState('');
  const sorted=[...tasks].sort((a,b)=>statuses.indexOf(a.status)-statuses.indexOf(b.status)||b.updatedAt-a.updatedAt);
  return <section className="tasks"><div className="section-heading"><h2>任务</h2><div>{tasks.some(t=>!t.external&&(t.status==='idle'||t.status==='completed'))&&<button className="text-button" onClick={actions.clearCompleted}>清理已结束</button>}<button className="text-button" disabled={tasks.filter(t=>!t.external).length>=100} onClick={()=>setAdding(!adding)}><Plus size={14}/>添加</button></div></div>
    {adding&&<form className="add-form" onSubmit={e=>{e.preventDefault();if(title.trim()){actions.add(title);setTitle('');setAdding(false);}}}><input autoFocus maxLength={200} placeholder="任务名称" aria-label="新任务名称" value={title} onChange={e=>setTitle(e.target.value)}/><button className="button primary" type="submit" disabled={!title.trim()}>添加</button><button className="icon-button" type="button" aria-label="取消添加" onClick={()=>setAdding(false)}><X size={15}/></button></form>}
    <div className="group task-list">{sorted.length===0?<p className="empty">暂无任务</p>:sorted.map(task=><div className="task-row" key={`${task.external?'external:':'local:'}${task.id}`}><span className={`status-dot ${task.status}`}/><div className="task-title"><strong title={task.title}>{task.title}</strong><small title={task.message}>{task.source}{task.external&&now-task.updatedAt>900_000?' · 暂未收到新进展':''}</small>{(task.id.startsWith('monitor:')||task.status==='attention')&&task.message&&<p className="task-reason">{task.message}</p>}</div>{!task.external?<select aria-label={`${task.title}的状态`} value={task.status} className={`task-status ${task.status}`} onChange={e=>actions.status(task.id,e.target.value as Status)}>{statuses.map(s=><option key={s} value={s}>{labels[s]}</option>)}</select>:<span className={`external-status ${task.status}`} title={formatAge(task.updatedAt,now)}>{labels[task.status]}</span>}{task.id.startsWith('monitor:')?<button className="text-button acknowledge" onClick={()=>void invoke('acknowledge_monitor',{id:task.id}).catch(e=>onError(String(e)))}>已检查</button>:<button className="delete-task" title={task.external?'移除状态；来源再次上报会重新加入':'删除任务'} aria-label={`${task.external?'移除':'删除'}${task.title}`} onClick={()=>{if(task.external)void invoke('remove_external_task',{id:task.id}).catch(e=>onError(String(e)));else actions.remove(task.id);}}><Trash2 size={14}/></button>}</div>)}</div>
  </section>;
}
