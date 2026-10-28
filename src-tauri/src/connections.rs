use crate::{bridge::{self, ExternalTask, Status}, NativeState};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{collections::{HashMap, HashSet}, fs::{self, File, OpenOptions}, io::{BufRead, BufReader, Read, Seek, SeekFrom, Write}, path::{Path, PathBuf}, time::{Duration, SystemTime, UNIX_EPOCH}};
use tauri::{Emitter, Manager};

#[cfg(test)]
const MAX_SCAN_BYTES: u64 = 8 * 1024 * 1024;
const MAX_RECORD_BYTES: u64 = 64 * 1024 * 1024;
const OWN_MARKER: &str = "--ai-signal-hook";
const CLAUDE_EVENTS: &[&str] = &["UserPromptSubmit", "PreToolUse", "PostToolUse", "PostToolUseFailure", "PermissionRequest", "Notification", "Stop", "SessionEnd"];

#[derive(Clone, Default, Deserialize, Serialize)]
#[serde(default)]
pub struct ConnectionSettings { pub codex: bool, pub claude: bool }

#[derive(Serialize)]
#[serde(rename_all="camelCase")]
pub struct ConnectionInfo { id: &'static str, enabled: bool, detected: bool, task_count: usize, received: bool, error: Option<String> }

fn now_ms() -> u64 { SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_millis() as u64 }
fn code_root(home: &Path) -> PathBuf { std::env::var_os("CODEX_HOME").map(PathBuf::from).filter(|p|p.is_absolute()).unwrap_or_else(||home.join(".codex")) }
fn claude_root(home: &Path) -> PathBuf { std::env::var_os("CLAUDE_CONFIG_DIR").map(PathBuf::from).filter(|p|p.is_absolute()).unwrap_or_else(||home.join(".claude")) }

pub fn initialize(app: &tauri::AppHandle) {
    if let Ok(dir)=app.path().app_data_dir() {
        if let Ok(bytes)=fs::read(dir.join("connections.json")) {
            if bytes.len()<4096 {if let Ok(settings)=serde_json::from_slice(&bytes) {*app.state::<NativeState>().connections.lock().expect("connections lock")=settings;}}
        }
    }
    let app=app.clone();
    std::thread::spawn(move || loop {
        let _=run_scan(&app,false);
        std::thread::sleep(Duration::from_secs(3));
    });
}

#[derive(Default, Clone, Serialize)]
#[serde(rename_all="camelCase")]
pub(crate) struct SyncInfo { scanning: bool, last_synced_at: u64, error: Option<String>, enabled: bool }
#[derive(Clone)]
pub(crate) struct CachedRollout { modified: SystemTime, len: u64, offset: u64, record: Rollout }

fn run_scan(app: &tauri::AppHandle, force: bool) -> Result<(),String> {
    let state=app.state::<NativeState>();
    let mut cache=state.scan_cache.lock().map_err(|e|e.to_string())?;
    if !state.connections.lock().map_err(|e|e.to_string())?.codex {cache.clear();return Ok(());}
    state.sync_info.lock().map_err(|e|e.to_string())?.scanning=true;
    if force {cache.clear();}
    let result=scan_codex(app,&mut cache);
    if let Ok(mut errors)=state.connection_errors.lock() {
        match &result {Ok(())=>{errors.remove("codex");},Err(e)=>{errors.insert("codex".into(),e.clone());}}
    }
    if let Ok(mut info)=state.sync_info.lock() {
        info.scanning=false;info.error=result.as_ref().err().cloned();
        if result.is_ok() {info.last_synced_at=now_ms();}
    }
    result
}
#[tauri::command]
pub fn task_sync_info(app: tauri::AppHandle) -> Result<SyncInfo,String> {
    let state=app.state::<NativeState>();
    let mut info=state.sync_info.lock().map_err(|e|e.to_string())?.clone();
    info.enabled=state.connections.lock().map_err(|e|e.to_string())?.codex;
    Ok(info)
}
#[tauri::command]
pub async fn refresh_connections(app: tauri::AppHandle) -> Result<(),String> {
    tauri::async_runtime::spawn_blocking(move ||run_scan(&app,true)).await.map_err(|e|e.to_string())?
}

pub fn accepts(app: &tauri::AppHandle, id: &str) -> bool {
    let state=app.state::<NativeState>();
    let Ok(settings)=state.connections.lock() else {return false;};
    if id.starts_with("codex:") {settings.codex} else if id.starts_with("claude:") {settings.claude} else {true}
}

#[tauri::command]
pub fn list_connections(app: tauri::AppHandle) -> Result<Vec<ConnectionInfo>,String> {
    let home=app.path().home_dir().map_err(|e|e.to_string())?;
    let state=app.state::<NativeState>();
    let settings=state.connections.lock().map_err(|e|e.to_string())?.clone();
    let errors=state.connection_errors.lock().map_err(|e|e.to_string())?.clone();
    let tasks=state.tasks.lock().map_err(|e|e.to_string())?;
    Ok([("codex",settings.codex,code_root(&home).join("sessions").is_dir()),("claude",settings.claude,claude_root(&home).is_dir())].into_iter().map(|(id,enabled,detected)|{
        let prefix=format!("{id}:");
        let matching:Vec<_>=tasks.values().filter(|t|t.id.starts_with(&prefix)).collect();
        ConnectionInfo{id,enabled,detected,task_count:matching.iter().filter(|t|matches!(t.status,Status::Running|Status::Repairing|Status::Attention)).count(),received:!matching.is_empty(),error:errors.get(id).cloned()}
    }).collect())
}

#[tauri::command]
pub async fn set_connection(app: tauri::AppHandle, provider: String, enabled: bool) -> Result<(),String> {
    if !matches!(provider.as_str(),"codex"|"claude") {return Err("不支持的软件来源".into());}
    let home=app.path().home_dir().map_err(|e|e.to_string())?;
    let dir=app.path().app_data_dir().map_err(|e|e.to_string())?;
    let state=app.state::<NativeState>();
    let mut settings=state.connections.lock().map_err(|e|e.to_string())?;
    if provider=="codex" && enabled && !code_root(&home).join("sessions").is_dir() {return Err("未找到 Codex 本机会话目录，请先运行一次 Codex。".into());}
    let mut next=settings.clone();
    if provider=="codex" {next.codex=enabled;} else {next.claude=enabled;}
    if provider=="claude" {
        if enabled && !state.bridge.lock().map_err(|e|e.to_string())?.available {return Err("本机接收端不可用，请重启应用后重试。".into());}
        let root=claude_root(&home);
        if enabled && !root.is_dir() {return Err("未找到 Claude Code 配置目录，请先安装并运行一次 Claude Code。".into());}
        let helper=dir.join(if cfg!(windows) {"signal-hook.exe"} else {"signal-hook"});
        if enabled {copy_helper(&helper)?;}
        configure_claude(&root.join("settings.json"), &helper, &dir.join("bridge.json"), enabled, &dir.join("backups"))?;
    }
    write_json(&dir.join("connections.json"), &serde_json::to_value(&next).map_err(|e|e.to_string())?)?;
    *settings=next;
    drop(settings);
    if provider=="codex" {state.scan_cache.lock().map_err(|e|e.to_string())?.clear();}
    if !enabled {
        let prefix=format!("{provider}:");
        state.attention_evidence.lock().map_err(|e|e.to_string())?.retain(|id,_|!id.starts_with(&prefix));
        let mut tasks=state.tasks.lock().map_err(|e|e.to_string())?;
        let ids:Vec<_>=tasks.keys().filter(|id|id.starts_with(&prefix)).cloned().collect();
        for id in ids {tasks.remove(&id);let _=app.emit("external-task-removed",id);}
    }
    let _=app.emit("connections-changed",());
    Ok(())
}

fn copy_helper(path: &Path) -> Result<(),String> {
    let parent=path.parent().ok_or("连接目录不可用")?;
    fs::create_dir_all(parent).map_err(|e|e.to_string())?;
    let temp=path.with_extension(format!("{}.tmp",now_ms()));
    fs::copy(std::env::current_exe().map_err(|e|e.to_string())?,&temp).map_err(|e|e.to_string())?;
    #[cfg(unix)] {use std::os::unix::fs::PermissionsExt;fs::set_permissions(&temp,fs::Permissions::from_mode(0o700)).map_err(|e|e.to_string())?;}
    if path.exists() {fs::remove_file(path).map_err(|e|e.to_string())?;}
    fs::rename(temp,path).map_err(|e|e.to_string())
}

pub(crate) fn write_json(path: &Path, data: &Value) -> Result<(),String> {
    let parent=path.parent().ok_or("配置路径不可用")?;
    fs::create_dir_all(parent).map_err(|e|e.to_string())?;
    if path.symlink_metadata().is_ok_and(|m|m.file_type().is_symlink()) {return Err("配置文件为符号链接，请使用手动接入，避免覆盖原配置。".into());}
    let temp=path.with_extension(format!("{}.tmp",now_ms()));
    let mut opts=OpenOptions::new();opts.write(true).create_new(true);
    #[cfg(unix)] {use std::os::unix::fs::OpenOptionsExt;opts.mode(0o600);}
    let mut file=opts.open(&temp).map_err(|e|e.to_string())?;
    serde_json::to_writer_pretty(&mut file,data).map_err(|e|e.to_string())?;
    file.flush().map_err(|e|e.to_string())?;file.sync_all().map_err(|e|e.to_string())?;drop(file);
    replace_file(&temp,path).map_err(|e|{let _=fs::remove_file(&temp);e})
}

fn replace_file(from: &Path, to: &Path) -> Result<(),String> {
    #[cfg(windows)] {
        use std::os::windows::ffi::OsStrExt;
        #[link(name="kernel32")] extern "system" {fn MoveFileExW(from: *const u16,to: *const u16,flags: u32)->i32;}
        let from:Vec<u16>=from.as_os_str().encode_wide().chain(Some(0)).collect();
        let to:Vec<u16>=to.as_os_str().encode_wide().chain(Some(0)).collect();
        if unsafe {MoveFileExW(from.as_ptr(),to.as_ptr(),1|8)}==0 {return Err(std::io::Error::last_os_error().to_string());}
        Ok(())
    }
    #[cfg(not(windows))] {fs::rename(from,to).map_err(|e|e.to_string())}
}

fn merge_claude(data: &mut Value, helper: &Path, bridge: &Path, enabled: bool) -> Result<(),String> {
    let root=data.as_object_mut().ok_or("Claude Code 配置必须是 JSON 对象，原文件未修改。")?;
    if enabled && root.get("disableAllHooks").and_then(Value::as_bool)==Some(true) {return Err("Claude Code 已禁用全部 Hooks，请先自行调整该设置。".into());}
    if !root.contains_key("hooks") && !enabled {return Ok(());}
    let hooks=root.entry("hooks").or_insert_with(||json!({})).as_object_mut().ok_or("hooks 配置格式不正确，原文件未修改。")?;
    for event in CLAUDE_EVENTS {
        if !hooks.contains_key(*event) && !enabled {continue;}
        let groups=hooks.entry(*event).or_insert_with(||json!([])).as_array_mut().ok_or("Hook 事件必须为数组，原文件未修改。")?;
        for group in groups.iter_mut() {
            if let Some(handlers)=group.get_mut("hooks").and_then(Value::as_array_mut) {
                handlers.retain(|handler|!is_own_hook(handler));
            }
        }
        groups.retain(|g|!g.get("hooks").and_then(Value::as_array).is_some_and(|a|a.is_empty()));
        if enabled {
            // Exec-form args avoid shell interpolation of spaces or non-ASCII paths.
            groups.push(json!({"hooks":[{"type":"command","command":helper,"args":[OWN_MARKER,"claude","--bridge",bridge],"timeout":2}]}));
        }
    }
    hooks.retain(|_,v|!v.as_array().is_some_and(|a|a.is_empty()));
    Ok(())
}

fn is_own_hook(handler: &Value) -> bool {
    handler.get("args").and_then(Value::as_array).is_some_and(|args|args.first().and_then(Value::as_str)==Some(OWN_MARKER)) || handler.get("command").and_then(Value::as_str).is_some_and(|s|s.contains(OWN_MARKER))
}

fn configure_claude(path: &Path, helper: &Path, bridge: &Path, enabled: bool, backup_dir: &Path) -> Result<(),String> {
    if path.symlink_metadata().is_ok_and(|m|m.file_type().is_symlink()) {return Err("Claude Code 配置为符号链接，请使用手动接入。".into());}
    let original=match fs::read(path) {Ok(bytes) if bytes.len()<=1024*1024=>Some(bytes),Ok(_)=>return Err("Claude Code 配置过大，未做修改。".into()),Err(e) if e.kind()==std::io::ErrorKind::NotFound=>None,Err(e)=>return Err(e.to_string())};
    if original.is_none() && !enabled {return Ok(());}
    let mut data=match &original {Some(bytes)=>serde_json::from_slice(bytes).map_err(|_|"Claude Code 配置不是有效 JSON，原文件未修改。".to_string())?,None=>json!({})};
    merge_claude(&mut data,helper,bridge,enabled)?;
    if let Some(bytes)=original {
        fs::create_dir_all(backup_dir).map_err(|e|e.to_string())?;
        #[cfg(unix)] {use std::os::unix::fs::PermissionsExt;fs::set_permissions(backup_dir,fs::Permissions::from_mode(0o700)).map_err(|e|e.to_string())?;}
        let backup=backup_dir.join(format!("claude-settings-{}-{}.json",now_ms(),std::time::SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().subsec_nanos()));
        let mut opts=OpenOptions::new();opts.write(true).create_new(true);
        #[cfg(unix)] {use std::os::unix::fs::OpenOptionsExt;opts.mode(0o600);}
        opts.open(backup).and_then(|mut f|f.write_all(&bytes)).map_err(|e|e.to_string())?;
    }
    write_json(path,&data)
}

#[derive(Clone, Default)]
pub(crate) struct Rollout {id: Option<String>, cwd: Option<String>, phase: Option<Status>, blocked: bool, questions: HashMap<String,(bool,HashSet<u64>)>, subagent: bool}
impl Rollout {
    fn effective_phase(&self) -> Option<Status> {
        if self.blocked || !self.questions.is_empty() {Some(Status::Attention)} else {self.phase.clone()}
    }
}
fn parse_rollout(bytes: &[u8], state: &mut Rollout) {
    for line in bytes.split(|b|*b==b'\n') {
        let Ok(value)=serde_json::from_slice::<Value>(line) else {continue;};
        let Some(payload)=value.get("payload") else {continue;};
        match value.get("type").and_then(Value::as_str) {
            Some("session_meta")=>{
                state.id=payload.get("id").or_else(||payload.get("session_id")).and_then(Value::as_str).map(str::to_owned);
                state.cwd=payload.get("cwd").and_then(Value::as_str).map(str::to_owned);
                state.subagent=payload.get("source").is_some_and(|s|s.get("subagent").is_some()) || payload.get("thread_source").is_some_and(|s|s.get("subagent").is_some());
            },
            Some("turn_context")=>{if let Some(cwd)=payload.get("cwd").and_then(Value::as_str) {state.cwd=Some(cwd.into());}},
            Some("event_msg")=>match payload.get("type").and_then(Value::as_str) {
                Some("task_started"|"turn_started")=>{state.phase=Some(Status::Running);state.blocked=false;},
                Some("task_complete"|"turn_complete")=>state.phase=Some(Status::Completed),
                Some("turn_aborted")=>state.phase=Some(Status::Idle),
                Some("user_message")=>{
                    if payload.get("message").and_then(Value::as_str).is_some_and(|text|!text.trim().starts_with("<send_user_message_question_reply>")) {state.questions.clear();}
                },
                _=>{}
            },
            Some("response_item")=>{
                match payload.get("type").and_then(Value::as_str) {
                    Some("function_call")=>{
                        let name=payload.get("name").and_then(Value::as_str).unwrap_or("").rsplit('.').next().unwrap_or("");
                        if matches!(name,"request_user_input"|"request_user_input_async") {
                            if let Some(id)=payload.get("call_id").and_then(Value::as_str) {
                                let args=payload.get("arguments").and_then(Value::as_str).and_then(|text|serde_json::from_str::<Value>(text).ok());
                                let count=args.as_ref().and_then(|args|args.get("questions")).and_then(Value::as_array).map(|q|q.len()).unwrap_or(1).clamp(1,64);
                                state.questions.insert(id.into(),(name.ends_with("_async"),(0..count as u64).collect()));
                            }
                        }
                    },
                    Some("function_call_output"|"custom_tool_call_output")=>{
                        if payload.get("output").is_some_and(blocked_output) {state.blocked=true;}
                        if let Some(id)=payload.get("call_id").and_then(Value::as_str) {
                            let rejected=payload.get("output").and_then(Value::as_str).and_then(|s|serde_json::from_str::<Value>(s).ok()).is_some_and(|v|v.get("accepted").and_then(Value::as_bool)==Some(false));
                            if rejected || state.questions.get(id).is_some_and(|(asynchronous,_)|!*asynchronous) {state.questions.remove(id);}
                        }
                    },
                    Some("message") if payload.get("role").and_then(Value::as_str)==Some("user")=>{
                        for content in payload.get("content").and_then(Value::as_array).into_iter().flatten() {
                            let Some(text)=content.get("text").and_then(Value::as_str) else {continue;};
                            let Some(text)=text.trim().strip_prefix("<send_user_message_question_reply>") else {continue;};
                            let Some((body,_))=text.split_once("</send_user_message_question_reply>") else {continue;};
                            let Ok(replies)=serde_json::from_str::<Vec<Value>>(body.trim()) else {continue;};
                            for reply in replies {
                                let Some(item)=reply.get("questionItemId").and_then(Value::as_str).and_then(|s|serde_json::from_str::<Vec<Value>>(s).ok()) else {continue;};
                                if let (Some(id),Some(index))=(item.get(1).and_then(Value::as_str),item.get(2).and_then(Value::as_u64)) {
                                    if let Some((_,questions))=state.questions.get_mut(id) {questions.remove(&index);}
                                }
                            }
                            state.questions.retain(|_,(_,questions)|!questions.is_empty());
                        }
                    },
                    _=>{}
                }
            },
            _=>{}
        }
    }
}

// Inspect only tool-result error fields, never user prompts or assistant prose.
fn blocked_output(value: &Value) -> bool {
    match value {
        Value::String(text)=>{
            if let Ok(parsed)=serde_json::from_str::<Value>(text) {if !parsed.is_string() {return blocked_output(&parsed);}}
            let text=text.trim().to_lowercase();
            let short_error=text.len()<500 || text.starts_with("error") || text.starts_with("computer use error");
            short_error && ["screen is locked","screen locked","computer is locked","desktop is locked","屏幕已锁定","电脑已锁定"].iter().any(|phrase|text.contains(phrase))
        },
        Value::Object(object)=>["error","message","output","text","content"].iter().any(|key|object.get(*key).is_some_and(blocked_output)),
        Value::Array(items)=>items.iter().any(blocked_output),
        _=>false
    }
}

// Replay lifecycle evidence on cold start; then consume only complete appended lines.
fn rollout_from_file(path: &Path, previous: Option<&CachedRollout>) -> Result<(Rollout,u64),String> {
    let mut file=File::open(path).map_err(|e|e.to_string())?;
    let mut offset=previous.map(|p|p.offset).unwrap_or(0);
    file.seek(SeekFrom::Start(offset)).map_err(|e|e.to_string())?;
    let mut state=previous.map(|p|p.record.clone()).unwrap_or_default();
    let mut reader=BufReader::new(file);
    let mut line=Vec::new();
    loop {
        line.clear();
        let count=Read::by_ref(&mut reader).take(MAX_RECORD_BYTES+1).read_until(b'\n',&mut line).map_err(|e|e.to_string())?;
        if count==0 {break;}
        if count as u64>MAX_RECORD_BYTES {return Err("会话中有超大记录，识别未完成，请检查任务。".into());}
        if line.last()!=Some(&b'\n') {break;}
        parse_rollout(&line,&mut state);offset+=count as u64;
    }
    Ok((state,offset))
}

fn collect_files(root: &Path, depth: u8, budget: &mut usize, files: &mut Vec<(SystemTime,PathBuf,u64)>) -> Result<(),String> {
    if depth>6 {return Ok(());}if *budget==0 {return Err("会话扫描达到上限，无法确认全部任务".into());}
    let entries=fs::read_dir(root).map_err(|_|"无法读取部分会话目录，请检查权限".to_string())?;
    for entry in entries {
        let entry=entry.map_err(|e|e.to_string())?;
        if *budget==0 {return Err("会话扫描达到上限，无法确认全部任务".into());}*budget-=1;
        let meta=entry.metadata().map_err(|e|e.to_string())?;
        if entry.file_type().is_ok_and(|t|t.is_symlink()) {continue;}
        if meta.is_dir() {collect_files(&entry.path(),depth+1,budget,files)?;} else if entry.path().extension().is_some_and(|e|e=="jsonl") {
            if let Ok(modified)=meta.modified() {if modified.elapsed().unwrap_or_default()<Duration::from_secs(24*60*60) {files.push((modified,entry.path(),meta.len()));}}
        }
    }
    Ok(())
}

fn read_thread_titles(root: &Path) -> HashMap<String,String> {
    let mut titles=HashMap::new();
    let Ok(mut file)=File::open(root.join("session_index.jsonl")) else {return titles;};
    let len=file.metadata().map(|m|m.len()).unwrap_or(0);
    if len>2*1024*1024 {let _=file.seek(SeekFrom::Start(len-2*1024*1024));}
    let mut bytes=Vec::new();if Read::by_ref(&mut file).take(2*1024*1024).read_to_end(&mut bytes).is_err() {return titles;}
    for line in bytes.split(|b|*b==b'\n') {
        let Ok(value)=serde_json::from_slice::<Value>(line) else {continue;};
        if let (Some(id),Some(title))=(value.get("id").and_then(Value::as_str),value.get("thread_name").and_then(Value::as_str)) {
            let title=title.strip_prefix("Continuing from ").and_then(|rest|rest.split_once(": ").map(|(name,_)|name)).unwrap_or(title);
            let title:String=title.chars().filter(|c|!c.is_control()).take(180).collect();
            if id.len()<=100 && !title.trim().is_empty() {titles.insert(id.to_owned(),title);}
        }
    }
    titles
}

fn scan_codex(app: &tauri::AppHandle, cache: &mut HashMap<PathBuf,CachedRollout>) -> Result<(),String> {
    let home=app.path().home_dir().map_err(|e|e.to_string())?;
    let root=code_root(&home).join("sessions");
    if !root.is_dir() {return Err("Codex 会话目录不可用".into());}
    fs::read_dir(&root).map_err(|_|"无法读取 Codex 会话目录，请检查磁盘和权限。".to_string())?;
    let titles=read_thread_titles(&code_root(&home));
    let mut files=Vec::new();collect_files(&root,0,&mut 10000,&mut files)?;
    let incomplete=files.len()>512;
    files.sort_by(|a,b|b.0.cmp(&a.0));files.truncate(512);
    cache.retain(|_,entry|entry.modified.elapsed().unwrap_or_default()<Duration::from_secs(24*60*60));
    let mut failures=Vec::new();
    for (modified,path,len) in files {
        if cache.get(&path).is_some_and(|old|old.modified==modified && old.len==len) {continue;}
        let previous_record=cache.get(&path).filter(|old|old.len<=len);
        let (record,offset)=match rollout_from_file(&path,previous_record) {Ok(value)=>value,Err(error)=>{failures.push(error);continue;}};
        let Some(id)=record.id.clone().filter(|id|!id.is_empty()&&id.len()<=100&&!id.chars().any(char::is_control)) else {continue;};
        if record.subagent {continue;}
        let task_id=format!("codex:{id}");
        let previous=cache.insert(path,CachedRollout{modified,len,offset,record:record.clone()});
        let Some(phase)=record.effective_phase() else {continue;};
        {
            let state=app.state::<NativeState>();let mut evidence=state.attention_evidence.lock().map_err(|e|e.to_string())?;
            if phase==Status::Attention {
                evidence.insert(task_id.clone(),if !record.questions.is_empty() {"正在等待用户回答，请打开 Codex 处理问题。"} else {"工具报告锁屏，请检查任务是否已恢复。"}.into());
            } else {evidence.remove(&task_id);}
        }
        if previous.is_none() && !matches!(phase,Status::Running|Status::Attention|Status::Repairing) && !app.state::<NativeState>().tasks.lock().map_err(|e|e.to_string())?.contains_key(&task_id) {continue;}
        let strong_source=app.state::<NativeState>().tasks.lock().map_err(|e|e.to_string())?.get(&task_id).is_some_and(|t|t.source.as_deref()==Some("Codex Hook"));
        if strong_source && phase!=Status::Attention {continue;}
        let title=titles.get(&id).cloned().unwrap_or_else(||record.cwd.as_deref().and_then(|p|Path::new(p).file_name()).map(|s|s.to_string_lossy().chars().take(180).collect::<String>()).unwrap_or_else(||"Codex 任务".into()));
        bridge::publish_observed_task(app,ExternalTask{id:task_id,title,status:phase,source:Some("Codex · 本机识别".into()),message:Some(if !record.questions.is_empty() {"正在等待用户回答，请打开 Codex 处理问题。"} else if record.blocked {"工具报告锁屏，任务可能仍需人工恢复。"} else {"根据本机会话事件识别；本地格式可能随 Codex 版本变化。"}.into()),updated_at:0},modified.duration_since(UNIX_EPOCH).unwrap_or_default().as_millis() as u64)?;
    }
    if incomplete {failures.push("近期会话超过 512 项，监测范围不完整，请检查任务。".into());}
    if !failures.is_empty() {return Err(format!("{} 项记录未完成识别：{}",failures.len(),failures[0]));}
    Ok(())
}

pub fn hook_event(payload: &Value, provider: &str) -> Result<Option<ExternalTask>,String> {
    if !matches!(provider,"claude"|"codex") {return Err("Unknown provider".into());}
    let event=payload.get("hook_event_name").and_then(Value::as_str).ok_or("Missing event")?;
    let status=match event {
        "PostToolUse"|"PostToolUseFailure" if payload.get("tool_response").or_else(||payload.get("error")).is_some_and(blocked_output)=>Status::Attention,
        "PostToolUseFailure"=>return Ok(None),
        "UserPromptSubmit"|"PostToolUse"=>Status::Running,
        "PreToolUse" if payload.get("tool_name").and_then(Value::as_str)==Some("AskUserQuestion")=>Status::Attention,
        "PreToolUse"=>Status::Running,
        "PermissionRequest"=>Status::Attention,
        "Notification" if payload.get("notification_type").and_then(Value::as_str).is_some_and(|s|s=="permission_prompt")=>Status::Attention,
        "Stop"=>Status::Completed,
        "Interrupt"|"SessionEnd"=>Status::Idle,
        _=>return Ok(None)
    };
    let id=payload.get("session_id").and_then(Value::as_str).filter(|s|!s.is_empty()&&s.len()<=100&&!s.chars().any(char::is_control)).ok_or("Invalid session id")?;
    let title=payload.get("cwd").and_then(Value::as_str).and_then(|s|s.rsplit(['/', '\\']).find(|p|!p.is_empty())).unwrap_or("AI 任务").chars().filter(|c|!c.is_control()).take(180).collect();
    Ok(Some(ExternalTask{id:format!("{provider}:{id}"),title,status:status.clone(),source:Some(if provider=="claude" {"Claude Code"} else {"Codex Hook"}.into()),message:Some(if status==Status::Attention {"任务等待人工处理，或电脑操作受阻，请检查来源软件。"} else if event=="Stop" {"本轮已结束，结果仍需验收。"} else {"来自软件任务事件。"}.into()),updated_at:0}))
}

pub fn run_hook_cli_if_requested() -> bool {
    let args:Vec<String>=std::env::args().skip(1).collect();
    let exe=std::env::current_exe().ok();
    let helper=exe.as_ref().and_then(|p|p.file_stem()).is_some_and(|s|s=="signal-hook");
    if args.first().is_none_or(|s|s!=OWN_MARKER) && !helper {return false;}
    let provider=args.get(1).map(String::as_str).unwrap_or("claude");
    let path=args.windows(2).find(|a|a[0]=="--bridge").map(|a|PathBuf::from(&a[1])).or_else(||exe.as_ref().and_then(|p|p.parent()).map(|p|p.join("bridge.json")));
    let _=(||->Result<(),String>{
        let mut bytes=Vec::new();std::io::stdin().take(2*1024*1024+1).read_to_end(&mut bytes).map_err(|e|e.to_string())?;
        if bytes.len()>2*1024*1024 {return Err("Input too large".into());}
        let payload:Value=serde_json::from_slice(&bytes).map_err(|e|e.to_string())?;
        if let Some(task)=hook_event(&payload,provider)? {send_hook(&path.ok_or("No bridge path")?,&task)?;}
        Ok(())
    })();
    // Advisory only: never approve, block, or continue the agent's operation.
    println!("{{}}");
    true
}

fn send_hook(path: &Path, task: &ExternalTask) -> Result<(),String> {
    if fs::metadata(path).map_err(|e|e.to_string())?.len()>4096 {return Err("Invalid discovery file".into());}
    let data:Value=serde_json::from_slice(&fs::read(path).map_err(|e|e.to_string())?).map_err(|e|e.to_string())?;
    let port=data.get("port").and_then(Value::as_u64).filter(|p|*p>0&&*p<=65535).ok_or("Invalid port")? as u16;
    let token=data.get("token").and_then(Value::as_str).filter(|s|s.len()>=16&&s.len()<=512&&s.bytes().all(|b|b.is_ascii_alphanumeric())).ok_or("Invalid token")?;
    let body=serde_json::to_vec(task).map_err(|e|e.to_string())?;
    let mut stream=std::net::TcpStream::connect_timeout(&std::net::SocketAddr::from(([127,0,0,1],port)),Duration::from_millis(700)).map_err(|e|e.to_string())?;
    stream.set_read_timeout(Some(Duration::from_millis(700))).map_err(|e|e.to_string())?;
    stream.set_write_timeout(Some(Duration::from_millis(700))).map_err(|e|e.to_string())?;
    let header=format!("POST /event HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nAuthorization: Bearer {token}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",body.len());
    stream.write_all(header.as_bytes()).and_then(|_|stream.write_all(&body)).map_err(|e|e.to_string())?;
    let mut response=[0u8;512];let count=stream.read(&mut response).map_err(|e|e.to_string())?;
    if !response[..count].starts_with(b"HTTP/1.1 200 ") {return Err("Event rejected".into());}
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test] fn rollout_parser_requires_lifecycle_evidence_and_ignores_prompt_text() {
        let mut state=Rollout::default();
        parse_rollout(b"{\"type\":\"session_meta\",\"payload\":{\"id\":\"s1\",\"cwd\":\"/work/project\"}}\n{\"type\":\"response_item\",\"payload\":{\"type\":\"task_started\"}}",&mut state);
        assert_eq!(state.phase,None);assert_eq!(state.id.as_deref(),Some("s1"));
        parse_rollout(b"{\"type\":\"event_msg\",\"payload\":{\"type\":\"task_started\"}}\n",&mut state);assert_eq!(state.phase,Some(Status::Running));
        parse_rollout(b"{\"type\":\"event_msg\",\"payload\":{\"type\":\"task_complete\"}}\n",&mut state);assert_eq!(state.phase,Some(Status::Completed));
    }
    #[test] fn claude_merge_is_idempotent_and_disconnect_preserves_other_settings() {
        let original=json!({"model":"keep", "hooks":{"Stop":[{"hooks":[{"type":"command","command":"my-validator"}]}]}});
        let mut data=original.clone();let helper=Path::new("/space folder/signal-hook");let bridge=Path::new("/space folder/bridge.json");
        merge_claude(&mut data,helper,bridge,true).unwrap();let once=data.clone();merge_claude(&mut data,helper,bridge,true).unwrap();assert_eq!(data,once);
        assert_eq!(data["hooks"]["Stop"].as_array().unwrap().len(),2);
        merge_claude(&mut data,helper,bridge,false).unwrap();assert_eq!(data,original);
    }
    #[test] fn disabled_hooks_and_malformed_configuration_are_not_replaced() {
        let mut disabled=json!({"disableAllHooks":true});assert!(merge_claude(&mut disabled,Path::new("h"),Path::new("b"),true).is_err());
        let mut invalid=json!({"hooks":{"Stop":{}}});assert!(merge_claude(&mut invalid,Path::new("h"),Path::new("b"),true).is_err());
    }
    #[test] fn hook_mapping_does_not_transmit_private_prompts_or_tool_content() {
        let payload=json!({"hook_event_name":"PermissionRequest","session_id":"session-1","cwd":"C:\\work\\demo","prompt":"secret","tool_input":{"secret":true}});
        let event=hook_event(&payload,"claude").unwrap().unwrap();assert_eq!(event.id,"claude:session-1");assert_eq!(event.title,"demo");assert_eq!(event.status,Status::Attention);
        let serialized=serde_json::to_string(&event).unwrap();assert!(!serialized.contains("secret"));
        assert!(hook_event(&json!({"hook_event_name":"SessionStart"}),"claude").unwrap().is_none());
        assert!(hook_event(&json!({"hook_event_name":"Notification","notification_type":"idle_prompt"}),"claude").unwrap().is_none());
        assert!(hook_event(&json!({"hook_event_name":"PreToolUse","session_id":""}),"claude").is_err());
    }
    #[test] fn settings_file_is_backed_up_and_invalid_json_is_left_intact() {
        let root=std::env::temp_dir().join(format!("ai-signal-config-test-{}-{}",std::process::id(),now_ms()));
        fs::create_dir_all(&root).unwrap();
        let path=root.join("settings.json");let backups=root.join("backups");
        fs::write(&path,b"{\"model\":\"keep\"}").unwrap();
        configure_claude(&path,&root.join("signal-hook"),&root.join("bridge.json"),true,&backups).unwrap();
        let data:Value=serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
        assert_eq!(data["model"],"keep");assert!(fs::read_dir(&backups).unwrap().next().is_some());
        configure_claude(&path,&root.join("signal-hook"),&root.join("bridge.json"),false,&backups).unwrap();
        let after:Value=serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
        assert_eq!(after["model"],"keep");assert!(after["hooks"].as_object().unwrap().is_empty());
        fs::write(&path,b"invalid JSON").unwrap();
        assert!(configure_claude(&path,&root.join("signal-hook"),&root.join("bridge.json"),true,&backups).is_err());
        assert_eq!(fs::read(&path).unwrap(),b"invalid JSON");
        fs::remove_dir_all(root).unwrap();
    }
    #[test] fn helper_sends_authenticated_loopback_events_without_a_gui_or_python() {
        use std::net::TcpListener;
        let listener=TcpListener::bind(("127.0.0.1",0)).unwrap();
        let port=listener.local_addr().unwrap().port();
        let root=std::env::temp_dir().join(format!("ai-signal-hook-test-{}-{}",std::process::id(),now_ms()));
        fs::create_dir_all(&root).unwrap();let path=root.join("bridge.json");
        fs::write(&path,serde_json::to_vec(&json!({"port":port,"token":"testtoken1234567890"})).unwrap()).unwrap();
        let server=std::thread::spawn(move||{
            let (mut stream,_)=listener.accept().unwrap();stream.set_read_timeout(Some(Duration::from_secs(2))).unwrap();
            let mut bytes=Vec::new();let mut chunk=[0u8;4096];
            loop {
                let count=stream.read(&mut chunk).unwrap();assert!(count>0);bytes.extend_from_slice(&chunk[..count]);
                if let Some(end)=bytes.windows(4).position(|b|b==b"\r\n\r\n") {
                    let headers=std::str::from_utf8(&bytes[..end]).unwrap();
                    let len:usize=headers.lines().find_map(|line|line.strip_prefix("Content-Length: ")).unwrap().parse().unwrap();
                    if bytes.len()>=end+4+len {
                        assert!(headers.contains("Authorization: Bearer testtoken1234567890"));
                        let event:Value=serde_json::from_slice(&bytes[end+4..end+4+len]).unwrap();assert_eq!(event["id"],"claude:test");break;
                    }
                }
            }
            stream.write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 11\r\nConnection: close\r\n\r\n{\"ok\":true}").unwrap();
        });
        let event=hook_event(&json!({"hook_event_name":"PreToolUse","session_id":"test","cwd":"/demo"}),"claude").unwrap().unwrap();
        send_hook(&path,&event).unwrap();server.join().unwrap();fs::remove_dir_all(root).unwrap();
    }

    #[test] fn cold_start_recovers_two_long_running_sessions_and_incremental_partial_lines() {
        let root=std::env::temp_dir().join(format!("signal-replay-{}-{}",std::process::id(),now_ms()));fs::create_dir_all(&root).unwrap();
        for id in ["first","second"] {
            let path=root.join(format!("{id}.jsonl"));
            let mut file=File::create(&path).unwrap();
            writeln!(file,"{}",json!({"type":"session_meta","payload":{"id":id}})).unwrap();
            writeln!(file,"{}",json!({"type":"event_msg","payload":{"type":"task_started"}})).unwrap();
            let filler=b"{\"type\":\"ignored\"}\n";
            for _ in 0..(MAX_SCAN_BYTES as usize/filler.len()+2) {file.write_all(filler).unwrap();}
            writeln!(file,"{}",json!({"type":"compacted","content":"x".repeat(MAX_SCAN_BYTES as usize+1)})).unwrap();
            file.write_all(b"{\"type\":\"event_msg\",").unwrap();drop(file);
            let (record,offset)=rollout_from_file(&path,None).unwrap();assert_eq!(record.id.as_deref(),Some(id));assert_eq!(record.phase,Some(Status::Running));
            let cached=CachedRollout{modified:SystemTime::now(),len:fs::metadata(&path).unwrap().len(),offset,record};
            OpenOptions::new().append(true).open(&path).unwrap().write_all(b"\"payload\":{\"type\":\"task_complete\"}}\n").unwrap();
            let (done,end)=rollout_from_file(&path,Some(&cached)).unwrap();assert_eq!(done.phase,Some(Status::Completed));assert_eq!(end,fs::metadata(&path).unwrap().len());
            assert_eq!(rollout_from_file(&path,None).unwrap().0.phase,Some(Status::Completed));
        }
        fs::remove_dir_all(root).unwrap();
    }

    #[test] fn task_names_use_the_latest_title_metadata_instead_of_prompt_content() {
        let root=std::env::temp_dir().join(format!("ai-signal-titles-test-{}-{}",std::process::id(),now_ms()));fs::create_dir_all(&root).unwrap();
        fs::write(root.join("session_index.jsonl"),b"{\"id\":\"s1\",\"thread_name\":\"old\"}\ninvalid\n{\"id\":\"s1\",\"thread_name\":\"Continuing from current: imported details\"}\n").unwrap();
        assert_eq!(read_thread_titles(&root).get("s1").map(String::as_str),Some("current"));fs::remove_dir_all(root).unwrap();
    }

    #[test] fn pending_user_questions_stay_red_after_turn_completion_until_every_answer_arrives() {
        let mut record=Rollout::default();
        let input=[json!({"type":"event_msg","payload":{"type":"task_started"}}),json!({"type":"response_item","payload":{"type":"function_call","name":"request_user_input_async","call_id":"q1","arguments":"{\"questions\":[{},{}]}"}}),json!({"type":"response_item","payload":{"type":"function_call_output","call_id":"q1","output":"{\"accepted\":true}"}}),json!({"type":"event_msg","payload":{"type":"task_complete"}})];
        for item in input {parse_rollout(serde_json::to_string(&item).unwrap().as_bytes(),&mut record);}
        assert_eq!(record.effective_phase(),Some(Status::Attention));
        for index in 0..2 {
            let reply=json!([{"questionItemId":serde_json::to_string(&json!(["request_user_input_async","q1",index])).unwrap(),"answer":"ok"}]);
            let item=json!({"type":"response_item","payload":{"type":"message","role":"user","content":[{"type":"input_text","text":format!("<send_user_message_question_reply>\n{reply}\n</send_user_message_question_reply>")}]}});
            parse_rollout(serde_json::to_string(&item).unwrap().as_bytes(),&mut record);
            assert_eq!(record.effective_phase(),Some(if index==0 {Status::Attention} else {Status::Completed}));
        }
    }
    #[test] fn tool_lock_errors_remain_red_even_if_the_turn_ends() {
        let mut record=Rollout::default();
        parse_rollout(b"{\"type\":\"event_msg\",\"payload\":{\"type\":\"task_started\"}}\n{\"type\":\"response_item\",\"payload\":{\"type\":\"function_call_output\",\"output\":\"Error: Screen is locked. Please unlock.\"}}\n{\"type\":\"event_msg\",\"payload\":{\"type\":\"task_complete\"}}",&mut record);
        assert_eq!(record.effective_phase(),Some(Status::Attention));
        parse_rollout(b"{\"type\":\"event_msg\",\"payload\":{\"type\":\"task_started\"}}",&mut record);assert_eq!(record.effective_phase(),Some(Status::Running));
        assert!(!blocked_output(&json!({"prompt":"Screen is locked"})));
    }

    #[test] fn rejected_questions_and_new_user_messages_do_not_leave_ghost_waits() {
        let mut record=Rollout::default();
        let question=json!({"type":"response_item","payload":{"type":"function_call","name":"request_user_input_async","call_id":"q1","arguments":"{}"}});
        parse_rollout(serde_json::to_string(&question).unwrap().as_bytes(),&mut record);
        parse_rollout(b"{\"type\":\"response_item\",\"payload\":{\"type\":\"function_call_output\",\"call_id\":\"q1\",\"output\":\"{\\\"accepted\\\":false}\"}}",&mut record);
        assert!(record.questions.is_empty());
        parse_rollout(serde_json::to_string(&question).unwrap().as_bytes(),&mut record);
        parse_rollout(b"{\"type\":\"event_msg\",\"payload\":{\"type\":\"user_message\",\"message\":\"Continue with this change\"}}",&mut record);
        assert!(record.questions.is_empty());
    }

    #[test]
    #[ignore = "read-only verification against explicitly selected local histories"]
    fn replay_selected_active_histories() {
        let paths:Vec<PathBuf>=serde_json::from_str(&std::env::var("AI_SIGNAL_REPLAY_PATHS").expect("selected paths")).unwrap();
        assert!(paths.len()>=2);
        for path in paths {
            let (record,offset)=rollout_from_file(&path,None).unwrap();
            assert!(record.id.is_some());assert!(!record.subagent);
            println!("task {:?}: phase {:?}, blocked {}, pending questions {}",record.id,record.phase,record.blocked,record.questions.len());
            assert!(matches!(record.effective_phase(),Some(Status::Running|Status::Attention|Status::Repairing)));
            assert!(offset>0);
        }
        println!("All selected real local tasks recovered on cold start.");
    }

    #[test]
    #[ignore = "manual reproduction against a user-selected local history"]
    fn replay_user_selected_history_before_and_after_answer() {
        let path=PathBuf::from(std::env::var("AI_SIGNAL_REPLAY_PATH").expect("replay path"));
        let cutoff=std::env::var("AI_SIGNAL_REPLAY_CUTOFF").expect("replay cutoff");
        let mut file=File::open(path).unwrap();let len=file.metadata().unwrap().len();
        if len>MAX_SCAN_BYTES {file.seek(SeekFrom::Start(len-MAX_SCAN_BYTES)).unwrap();}
        let mut bytes=Vec::new();file.read_to_end(&mut bytes).unwrap();
        let mut before=Rollout::default();let mut after=Rollout::default();
        for line in bytes.split(|b|*b==b'\n') {
            let Ok(value)=serde_json::from_slice::<Value>(line) else {continue;};
            if value.get("timestamp").and_then(Value::as_str).is_some_and(|at|at<=cutoff.as_str()) {parse_rollout(line,&mut before);}
            parse_rollout(line,&mut after);
        }
        assert!(!before.questions.is_empty());assert_eq!(before.effective_phase(),Some(Status::Attention));
        assert!(after.questions.is_empty());
        println!("Real local history: unanswered question stays red after completion; actual answer clears pending state.");
    }

}
