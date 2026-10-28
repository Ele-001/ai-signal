import {useEffect,useState} from 'react';
import {invoke} from '@tauri-apps/api/core';
import {RefreshCw} from 'lucide-react';
import {actions,isDesktop} from './store';
import type {Task} from './model';
type SyncInfo={enabled:boolean;scanning:boolean;lastSyncedAt:number;error:string|null};
export function TaskSync({onError}:{onError:(error:string)=>void}) {
  const [info,setInfo]=useState<SyncInfo>();
  const [busy,setBusy]=useState(false);
  useEffect(()=>{
    if(!isDesktop)return;
    let disposed=false;
    const read=()=>void invoke<SyncInfo>('task_sync_info').then(value=>{if(!disposed)setInfo(value);}).catch(e=>{if(!disposed)onError(String(e));});
    read();const timer=setInterval(read,1000);
    return()=>{disposed=true;clearInterval(timer);};
  },[onError]);
  if(!isDesktop)return null;
  async function refresh() {
    setBusy(true);
    try {
      await invoke('refresh_connections');
      actions.externalSnapshot(await invoke<Task[]>('list_external_tasks'));
      setInfo(await invoke<SyncInfo>('task_sync_info'));
    } catch(e) {onError(`刷新失败：${String(e)}`);}
    finally {setBusy(false);}
  }
  const scanning=busy||info?.scanning;
  return <div className="task-sync"><span role="status">{!info?'正在读取连接状态…':!info.enabled?'请在“连接”中启用 Codex':scanning?'正在同步 Codex 任务…':info.error?'同步未完成，请重试':info.lastSyncedAt?`自动同步 · ${new Date(info.lastSyncedAt).toLocaleTimeString('zh-CN',{hour:'2-digit',minute:'2-digit',second:'2-digit',hour12:false})}`:'等待首次同步…'}</span><button className="text-button" disabled={!info?.enabled||scanning} onClick={()=>void refresh()}><RefreshCw size={12}/>{scanning?'同步中…':'刷新任务'}</button></div>;
}
