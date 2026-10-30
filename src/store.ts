import { useSyncExternalStore } from 'react';
import { emit, listen } from '@tauri-apps/api/event';
import { defaults, mergeTask, restore, sameTasks, validTask } from './model';
import type { LocalState, Settings, Status, Task } from './model';
export const isDesktop = '__TAURI_INTERNALS__' in window;
const key = 'ai-signal-v1';
let state: LocalState;
try { state = restore(localStorage.getItem(key)); } catch { state = structuredClone(defaults); }
let external: Task[] = [];
let monitoring: Task[] = [];
let transport: Task[] = [];
let snapshot = {...state, external, monitoring, transport, storageError: ''};
const subscribers = new Set<() => void>();
function notify() { snapshot = {...state, external, monitoring, transport, storageError: snapshot.storageError}; subscribers.forEach(fn => fn()); }
function commit(next: LocalState) {
  state = next;
  try {localStorage.setItem(key, JSON.stringify(state)); snapshot.storageError = '';} catch {snapshot.storageError = '设置未能保存，请检查本机可用空间。';}
  notify();
  if (isDesktop) void emit('local-state-changed', state).catch(console.error);
}
export const actions = {
  settings(patch: Partial<Settings>) {commit({...state, settings: {...state.settings, ...patch}});},
  add(title: string) {if (state.tasks.length >= 100) return; commit({...state, tasks: [...state.tasks, {id: crypto.randomUUID(), title: title.trim().slice(0, 200) || '新的任务', status: 'idle', source: '手动', updatedAt: Date.now()}]});},
  status(id: string, status: Status) {commit({...state, tasks: state.tasks.map(t => t.id === id ? {...t,status,updatedAt:Date.now()} : t)});},
  remove(id: string) {commit({...state,tasks:state.tasks.filter(t=>t.id!==id)});},
  simulate(status: Status) {
    const id = 'manual-simulator';
    if (!state.tasks.some(t=>t.id===id) && state.tasks.length>=100) return;
    commit({...state,tasks:mergeTask(state.tasks,{id,title:'模拟任务',status,source:'手动模拟',updatedAt:Date.now()})});
  },
  clearCompleted() {commit({...state,tasks:state.tasks.filter(t=>t.status!=='completed' && t.status!=='idle')});},
  transportError(failed: boolean) {if(failed===Boolean(transport.length))return;transport=failed?[{id:'monitor:ipc',title:'无法读取监测状态',status:'attention',source:'监测保护',message:'应用内部连接异常，请重新打开 AI红路灯 并检查原任务。',updatedAt:Date.now(),external:true}]:[];notify();},
  monitoring(tasks: Task[]) {const next=tasks.filter(validTask).map(task=>({...task,external:true}));if(sameTasks(monitoring,next))return;monitoring=next;notify();},
  externalSnapshot(tasks: Task[]) {const next=tasks.filter(validTask).slice(0,512).map(task=>({...task,external:true}));if(sameTasks(external,next))return;external=next;notify();},
  removeExternal(id: string) {external=external.filter(t=>t.id!==id);notify();},
  external(task: Task) {if (validTask(task)) {external = mergeTask(external,{...task,external:true});notify();}},
};
window.addEventListener('storage', event => {if(event.key===key) {state=restore(event.newValue);notify();}});
if (isDesktop) {
  void listen<LocalState>('local-state-changed', event => {state=restore(JSON.stringify(event.payload));notify();});
  void listen<{alwaysOnTop:boolean}>('preferences-changed', event => actions.settings({alwaysOnTop:event.payload.alwaysOnTop}));
}
export function useStore() {return useSyncExternalStore(fn=>{subscribers.add(fn);return()=>subscribers.delete(fn);},()=>snapshot);}
