import {useEffect,useState} from 'react';
import {invoke} from '@tauri-apps/api/core';
import {ExternalLink} from 'lucide-react';
import {isDesktop} from './store';

export type BridgeInfo={available:boolean;port:number|null;tokenPath:string|null;error?:string};
type Provider='codex'|'claude';
type Connection={id:Provider;enabled:boolean;detected:boolean;taskCount:number;received:boolean;error:string|null};
const names:Record<Provider,string>={codex:'Codex',claude:'Claude Code'};
const empty:Connection[]=(['codex','claude'] as const).map(id=>({id,enabled:false,detected:false,taskCount:0,received:false,error:null}));

export function Connections({bridge,onError}:{bridge?:BridgeInfo;onError:(error:string)=>void}) {
  const [connections,setConnections]=useState<Connection[]>(empty);
  const [busy,setBusy]=useState<Provider>();
  useEffect(()=>{
    if(!isDesktop)return;
    let disposed=false;
    const refresh=()=>void invoke<Connection[]>('list_connections').then(value=>{if(!disposed)setConnections(value);}).catch(e=>{if(!disposed)onError(String(e));});
    refresh();const timer=setInterval(refresh,3000);
    return()=>{disposed=true;clearInterval(timer);};
  },[onError]);
  async function toggle(connection:Connection) {
    setBusy(connection.id);
    try {
      await invoke('set_connection',{provider:connection.id,enabled:!connection.enabled});
      setConnections(await invoke<Connection[]>('list_connections'));
    } catch(e) {onError(String(e));}
    finally {setBusy(undefined);}
  }
  function status(connection:Connection) {
    if(!isDesktop)return '请使用桌面应用连接';
    if(connection.error&&connection.enabled)return connection.error;
    if(!connection.detected&&!connection.enabled)return `请先运行一次 ${names[connection.id]}`;
    if(!connection.enabled)return connection.id==='codex'?'自动识别本机正在运行的任务':'通过任务事件自动更新状态';
    if(connection.taskCount>0)return `${connection.taskCount} 项任务进行中`;
    return connection.id==='codex'?'已启用 · 暂无可识别的运行任务':connection.received?'已收到事件 · 当前无运行任务':'已配置 · 等待任务事件';
  }
  return <>
    <h1 className="page-title">连接</h1>
    <section className="group" aria-label="软件连接">
      {connections.map(connection=><div className="provider-row" key={connection.id}>
        <div className="provider-info"><h2>{names[connection.id]}{connection.enabled&&<span className={`provider-indicator ${connection.received?'received':''}`} title={connection.received?'已收到任务':'已启用'}/>}</h2><p>{status(connection)}</p></div>
        <button className={`button ${connection.enabled?'':'primary'}`} disabled={!isDesktop||Boolean(busy)||(!connection.detected&&!connection.enabled)||(connection.id==='claude'&&!bridge?.available&&!connection.enabled)} onClick={()=>void toggle(connection)} aria-label={`${connection.enabled?'断开':'连接'} ${names[connection.id]}`}>{busy===connection.id?'处理中…':connection.enabled?'断开':'连接'}</button>
      </div>)}
    </section>
    <p className="footnote">Codex 仅识别本机可读取的会话。Claude Code 配置后，请在已信任的工作区继续或开始一次任务。</p>
    {bridge?.error&&<div className="notice" role="alert">{bridge.error}</div>}
    <section className="group disclosures">
      <details><summary>识别范围</summary><div className="details-body"><p>Codex 使用本机会话生命周期记录兼容识别。连接时会扫描近期仍在运行的任务，软件版本、存储位置和云端执行可能影响结果；不会推断所有 Codex Desktop 会话。</p><p>本机识别不能可靠判断所有审批或修复状态。可通过高级 Hooks 接入补充精确事件。耗时操作、网络慢或长时间无新事件会保留最近明确状态，不会仅因此亮红灯。明确识别到运行期间锁屏时，原任务恢复进展或结束后自动解除；仅解锁不会误清除仍受阻的任务。</p><p>Claude Code 连接会合并本机状态 Hooks，保留现有配置并自动备份。需要软件支持命令参数 Hooks，并完成其工作区信任；收到真实事件后才显示已识别任务。</p><p>断开时只移除本应用的连接与任务，不改动其他软件设置。Codex 优先使用会话标题，其他来源使用项目目录名，不上传对话内容。</p></div></details>
      <details><summary>高级接入</summary><div className="details-body"><p>其他支持事件或 Hooks 的软件可通过本机接口上报任务。</p><p>{bridge?.available?`接收地址：127.0.0.1:${bridge.port}`:'本机接收端仅在桌面应用中可用。'}</p><p>Codex Hooks 配置生成：</p><code>python3 integrations/generate_hooks.py</code><p>先连接 Codex，再审阅配置并在 Codex 中信任 Hooks。完整步骤见 docs/INTEGRATION.md。</p><p>黄灯需明确上报；Stop 只表示本轮结束。</p><a href="https://learn.chatgpt.com/docs/hooks" target="_blank" rel="noreferrer">Codex 官方文档<ExternalLink size={12}/></a><br/><a href="https://code.claude.com/docs/en/hooks" target="_blank" rel="noreferrer">Claude Code 官方文档<ExternalLink size={12}/></a></div></details>
    </section>
  </>;
}
