use crate::{bridge::{ExternalTask, Status}, NativeState};
use serde::{Deserialize, Serialize};
use std::{collections::HashMap, time::{Duration, SystemTime, UNIX_EPOCH}};
use tauri::{Emitter, Manager};

fn now_ms() -> u64 {SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_millis() as u64}
fn working(task: &ExternalTask) -> bool {matches!(task.status, Status::Running | Status::Repairing)}
fn sticky(id: &str) -> bool {id == "monitor:screen"}

#[derive(Default, Deserialize, Serialize)]
struct SavedMonitor {
    alerts: Vec<ExternalTask>,
    #[serde(default)]
    targets: HashMap<String,HashMap<String,u64>>,
}
#[derive(Default)]
pub(crate) struct Monitor {
    alerts: HashMap<String, ExternalTask>,
    targets: HashMap<String,HashMap<String,u64>>,
    recovery_after: HashMap<String,u64>,
    desktop_blocked: bool,
}
impl Monitor {
    fn raise(&mut self, id: &str, title: &str, message: &str, now: u64) {
        self.alerts.entry(id.into()).or_insert_with(||ExternalTask {
            id:id.into(), title:title.chars().take(200).collect(), status:Status::Attention,
            source:Some("监测保护".into()), message:Some(message.into()), updated_at:now,
        });
    }
    fn track_affected(&mut self, id: &str, tasks: &[ExternalTask]) {
        let affected=self.targets.entry(id.into()).or_default();
        for task in tasks.iter().filter(|task|working(task)) {affected.insert(task.id.clone(),task.updated_at);}
        self.recovery_after.remove(id);
    }
    fn saved(&self) -> SavedMonitor {
        SavedMonitor {alerts:self.snapshot().into_iter().filter(|task|sticky(&task.id)).collect(),targets:self.targets.clone()}
    }
    fn tick(&mut self, now: u64, desktop: Result<bool,String>, tasks: &[ExternalTask], faults: &HashMap<String,String>) {
        let busy=tasks.iter().any(working);
        self.desktop_blocked=desktop.as_ref().map(|locked|*locked).unwrap_or(true);
        if busy {
            match desktop {
                Ok(true)=>{
                    self.raise("monitor:screen", "工作期间屏幕被锁定", "电脑操作可能已被阻止。解锁且原任务恢复或结束后自动解除。", now);
                    self.track_affected("monitor:screen",tasks);
                },
                Err(_)=>self.raise("monitor:desktop", "无法确认桌面是否可用", "系统状态读取失败，无法保证电脑操作可继续。请检查任务与系统权限。", now),
                _=>{}
            }
        }
        if !self.desktop_blocked {
            let mut recovered=Vec::new();
            for id in ["monitor:screen"] {
                let Some(alert)=self.alerts.get(id) else {continue;};
                let after=*self.recovery_after.entry(id.into()).or_insert(now);
                // Migrate older reminders without treating an unrelated task as recovery evidence.
                let affected=self.targets.entry(id.into()).or_default();
                if affected.is_empty() {affected.extend(tasks.iter().map(|task|(task.id.clone(),alert.updated_at)));}
                if !affected.is_empty() && affected.iter().all(|(task_id,baseline)|tasks.iter().any(|task|{
                    task.id==*task_id && task.updated_at>*baseline &&
                    (matches!(task.status,Status::Completed|Status::Idle) || (working(task) && task.updated_at>=after))
                })) {recovered.push(id.to_owned());}
            }
            for id in recovered {self.alerts.remove(&id);self.targets.remove(&id);self.recovery_after.remove(&id);}
        }
        let mut keep=Vec::new();
        if busy && self.desktop_blocked {keep.push("monitor:desktop".to_owned());}
        for (provider,error) in faults {
            let id=format!("monitor:connection:{provider}");
            self.raise(&id,"任务连接异常", &format!("{provider}：{error}。当前状态无法确认，请检查来源软件。"),now);
            keep.push(id);
        }
        // Silence is not evidence of a blocker: preserve the last explicit task state.
        // This also drops timeout-only reminders created by older versions.
        self.alerts.retain(|id,_|sticky(id)||keep.contains(id));
    }
    fn acknowledge(&mut self, id: &str, _now: u64) -> Result<(),String> {
        if !self.alerts.contains_key(id) {return Ok(());}
        if id.starts_with("monitor:connection:") || id=="monitor:desktop" {return Err("请先恢复连接或系统状态；恢复后提醒会自动解除。".into());}
        if self.desktop_blocked && sticky(id) {return Err("请先解锁并恢复桌面，再检查原任务。".into());}
        self.alerts.remove(id);
        self.targets.remove(id);self.recovery_after.remove(id);
        Ok(())
    }
    fn snapshot(&self) -> Vec<ExternalTask> {
        let mut tasks:Vec<_>=self.alerts.values().cloned().collect();
        tasks.sort_by(|a,b|a.id.cmp(&b.id));tasks
    }
}

#[tauri::command]
pub fn list_monitor_tasks(state: tauri::State<'_, NativeState>) -> Result<Vec<ExternalTask>,String> {
    Ok(state.monitor.lock().map_err(|e|e.to_string())?.snapshot())
}
#[tauri::command]
pub fn acknowledge_monitor(app: tauri::AppHandle, id: String) -> Result<(),String> {
    let (tasks,saved)={let state=app.state::<NativeState>();let mut monitor=state.monitor.lock().map_err(|e|e.to_string())?;monitor.acknowledge(&id,now_ms())?;(monitor.snapshot(),monitor.saved())};
    persist(&app,&saved)?;
    app.emit("monitor-tasks",tasks).map_err(|e|e.to_string())
}
fn persist(app: &tauri::AppHandle, saved: &SavedMonitor) -> Result<(),String> {
    let path=app.path().app_data_dir().map_err(|e|e.to_string())?.join("monitor-alerts.json");
    crate::connections::write_json(&path,&serde_json::to_value(saved).map_err(|e|e.to_string())?)
}

pub fn initialize(app: &tauri::AppHandle) {
    if let Ok(dir)=app.path().app_data_dir() {
        if let Ok(bytes)=std::fs::read(dir.join("monitor-alerts.json")) {
            if bytes.len()<=32*1024 {
                let saved=serde_json::from_slice::<SavedMonitor>(&bytes).or_else(|_|serde_json::from_slice::<Vec<ExternalTask>>(&bytes).map(|alerts|SavedMonitor{alerts,targets:HashMap::new()}));
                if let Ok(saved)=saved {
                    let state=app.state::<NativeState>();let mut monitor=state.monitor.lock().expect("monitor lock");
                    for mut task in saved.alerts.into_iter().filter(|task|sticky(&task.id)).take(2) {
                        task.status=Status::Attention;
                        task.message=Some("原任务恢复或结束后自动解除；若仍未解除，请检查原任务。".into());
                        monitor.alerts.insert(task.id.clone(),task);
                    }
                    monitor.targets=saved.targets.into_iter().filter(|(id,_)|sticky(id)).collect();
                }
            }
        }
    }
    let app=app.clone();
    std::thread::spawn(move||loop {
        let state=app.state::<NativeState>();
        // Do not hold any producer locks while checking or publishing monitor state.
        let settings=state.connections.lock().expect("connections lock").clone();
        let tasks:Vec<_>=state.tasks.lock().expect("tasks lock").values().cloned().collect();
        let mut faults=state.connection_errors.lock().expect("connection errors lock").clone();
        faults.retain(|provider,_|if provider=="codex" {settings.codex} else {settings.claude});
        if settings.claude && !state.bridge.lock().expect("bridge lock").available {faults.insert("claude".into(),"本机事件接收端不可用".into());}
        let (changed,alerts,saved)={
            let mut monitor=state.monitor.lock().expect("monitor lock");
            let before=serde_json::to_vec(&monitor.saved()).unwrap_or_default();
            let before_alerts=serde_json::to_vec(&monitor.snapshot()).unwrap_or_default();
            monitor.tick(now_ms(),desktop_locked(),&tasks,&faults);
            let alerts=monitor.snapshot();
            let saved=monitor.saved();
            (before!=serde_json::to_vec(&saved).unwrap_or_default() || before_alerts!=serde_json::to_vec(&alerts).unwrap_or_default(),alerts,saved)
        };
        if changed {
            if let Err(error)=persist(&app,&saved) {eprintln!("AI Signal monitor persistence: {error}");}
            let _=app.emit("monitor-tasks",&alerts);
        }
        std::thread::sleep(Duration::from_secs(2));
    });
}

// Only read the session/input desktop; never unlock or switch the user's desktop.
#[cfg(target_os="macos")]
fn desktop_locked() -> Result<bool,String> {
    use std::ffi::c_void;
    type Ref=*const c_void;
    #[link(name="CoreGraphics",kind="framework")] extern "C" {fn CGSessionCopyCurrentDictionary()->Ref;}
    #[link(name="CoreFoundation",kind="framework")] extern "C" {
        fn CFStringCreateWithCString(allocator:Ref,text:*const i8,encoding:u32)->Ref;
        fn CFDictionaryGetValue(dictionary:Ref,key:Ref)->Ref;
        fn CFBooleanGetTypeID()->usize;
        fn CFGetTypeID(value:Ref)->usize;
        fn CFBooleanGetValue(value:Ref)->bool;
        fn CFRelease(value:Ref);
    }
    unsafe {
        let session=CGSessionCopyCurrentDictionary();
        if session.is_null() {return Err("Window Server session unavailable".into());}
        let boolean=|name:&[u8]| {
            let key=CFStringCreateWithCString(std::ptr::null(),name.as_ptr().cast(),0x08000100);
            if key.is_null() {return None;}
            let value=CFDictionaryGetValue(session,key);CFRelease(key);
            if value.is_null() || CFGetTypeID(value)!=CFBooleanGetTypeID() {None} else {Some(CFBooleanGetValue(value))}
        };
        // shortcut: the lock flag is a compatibility field; retain the inactivity watchdog if macOS changes it.
        let locked=boolean(b"CGSSessionScreenIsLocked\0");
        let console=boolean(b"kCGSSessionOnConsoleKey\0");
        CFRelease(session);
        match (locked,console) {
            (Some(true),_)|(_,Some(false))=>Ok(true),
            (Some(false),Some(true))=>Ok(false),
            // macOS omits the lock key in an unlocked session.
            (None,Some(true))=>Ok(false),
            _=>Err("Session state unavailable".into())
        }
    }
}
#[cfg(target_os="windows")]
fn desktop_locked() -> Result<bool,String> {
    use std::ffi::c_void;
    #[link(name="user32")] extern "system" {
        fn OpenInputDesktop(flags:u32,inherit:i32,access:u32)->*mut c_void;
        fn CloseDesktop(desktop:*mut c_void)->i32;
        fn GetUserObjectInformationW(handle:*mut c_void,index:i32,info:*mut c_void,len:u32,needed:*mut u32)->i32;
    }
    unsafe {
        let desktop=OpenInputDesktop(0,0,1);
        if desktop.is_null() {return Err("Input desktop unavailable".into());}
        let mut name=[0u16;256];let mut needed=0;
        let ok=GetUserObjectInformationW(desktop,2,name.as_mut_ptr().cast(),512,&mut needed);
        CloseDesktop(desktop);
        if ok==0 {return Err("Input desktop name unavailable".into());}
        let end=name.iter().position(|c|*c==0).unwrap_or(name.len());
        Ok(!String::from_utf16_lossy(&name[..end]).eq_ignore_ascii_case("default"))
    }
}
#[cfg(not(any(target_os="macos",target_os="windows")))]
fn desktop_locked() -> Result<bool,String> {Err("Desktop monitoring unsupported".into())}

#[cfg(test)]
mod tests {
    use super::*;
    fn task(at:u64)->ExternalTask {ExternalTask{id:"codex:test".into(),title:"测试任务".into(),status:Status::Running,source:Some("test".into()),message:None,updated_at:at}}
    #[test] fn locked_work_automatically_clears_only_after_real_recovery() {
        let mut m=Monitor::default();m.tick(1000,Ok(true),&[task(1000)],&HashMap::new());
        assert!(m.alerts.contains_key("monitor:screen"));assert!(m.acknowledge("monitor:screen",1100).is_err());
        m.tick(3000,Ok(false),&[task(1000)],&HashMap::new());assert!(m.alerts.contains_key("monitor:screen"));
        m.tick(5000,Ok(false),&[task(5000)],&HashMap::new());assert!(m.alerts.is_empty());
    }
    #[test] fn idle_lock_does_not_alert_but_starting_while_locked_does() {
        let mut m=Monitor::default();m.tick(1000,Ok(true),&[],&HashMap::new());assert!(m.alerts.is_empty());
        m.tick(3000,Ok(true),&[task(3000)],&HashMap::new());assert!(m.alerts.contains_key("monitor:screen"));
    }
    #[test] fn quiet_running_and_repairing_tasks_never_turn_red_from_elapsed_time() {
        for status in [Status::Running,Status::Repairing] {
            let mut m=Monitor::default();let mut t=task(1000);t.status=status;
            for now in (1000..=1_201_000).step_by(2000) {
                m.tick(now,Ok(false),&[t.clone()],&HashMap::new());assert!(m.alerts.is_empty());
            }
            m.tick(1_203_000,Ok(true),&[t],&HashMap::new());
            assert!(m.alerts.contains_key("monitor:screen"));
        }
    }
    #[test] fn old_timeout_only_reminders_clear_without_manual_acknowledgement() {
        let mut m=Monitor::default();m.raise("monitor:stale:codex:test","old timeout","old timeout",1000);
        m.tick(3000,Ok(false),&[task(1000)],&HashMap::new());assert!(m.alerts.is_empty());
    }
    #[test] fn unreadable_desktop_alert_recovers_automatically() {
        let mut m=Monitor::default();m.tick(1000,Ok(false),&[task(1000)],&HashMap::new());
        m.tick(31000,Err("unavailable".into()),&[task(31000)],&HashMap::new());
        assert!(!m.alerts.contains_key("monitor:suspend"));assert!(m.alerts.contains_key("monitor:desktop"));
        m.tick(33000,Ok(false),&[task(33000)],&HashMap::new());assert!(!m.alerts.contains_key("monitor:desktop"));assert!(!m.alerts.contains_key("monitor:suspend"));
    }
    #[test] fn connection_fault_requires_actual_recovery() {
        let mut m=Monitor::default();let errors=HashMap::from([("codex".into(),"无法读取".into())]);
        m.tick(1000,Ok(false),&[],&errors);assert!(m.alerts.contains_key("monitor:connection:codex"));
        assert!(m.acknowledge("monitor:connection:codex",1000).is_err());
        m.tick(3000,Ok(false),&[],&HashMap::new());assert!(m.alerts.is_empty());
    }
    #[test] fn native_desktop_probe_returns_a_state_on_this_machine() {
        #[cfg(target_os="macos")] assert!(desktop_locked().is_ok());
    }
    #[test] fn unrelated_progress_cannot_clear_an_unrecovered_task() {
        let mut m=Monitor::default();let blocked=task(1000);let mut other=task(1000);other.id="codex:other".into();
        m.tick(1000,Ok(true),&[blocked.clone(),other.clone()],&HashMap::new());
        other.updated_at=3000;m.tick(3000,Ok(false),&[blocked.clone(),other.clone()],&HashMap::new());assert!(m.alerts.contains_key("monitor:screen"));
        let mut completed=blocked;completed.status=Status::Completed;completed.updated_at=4000;
        m.tick(5000,Ok(false),&[completed,other],&HashMap::new());assert!(!m.alerts.contains_key("monitor:screen"));
    }
    #[test] fn a_waiting_task_remains_red_and_saved_targets_survive_restart() {
        let mut m=Monitor::default();m.tick(1000,Ok(true),&[task(1000)],&HashMap::new());
        let saved:SavedMonitor=serde_json::from_slice(&serde_json::to_vec(&m.saved()).unwrap()).unwrap();
        let mut restored=Monitor{alerts:saved.alerts.into_iter().map(|task|(task.id.clone(),task)).collect(),targets:saved.targets,..Monitor::default()};
        let mut waiting=task(3000);waiting.status=Status::Attention;
        restored.tick(3000,Ok(false),&[waiting],&HashMap::new());assert!(restored.alerts.contains_key("monitor:screen"));
        restored.tick(5000,Ok(false),&[task(5000)],&HashMap::new());assert!(restored.alerts.is_empty());
    }

    #[test] fn old_reminder_waits_for_discovery_then_auto_recovers() {
        let mut m=Monitor::default();m.raise("monitor:screen","old","old",1000);
        m.tick(3000,Ok(false),&[],&HashMap::new());assert!(m.alerts.contains_key("monitor:screen"));
        m.tick(5000,Ok(false),&[task(5000)],&HashMap::new());assert!(m.alerts.is_empty());
    }
    #[test] fn completion_during_a_monitoring_gap_is_recovery_evidence() {
        let mut m=Monitor::default();m.tick(1000,Ok(false),&[task(1000)],&HashMap::new());
        let mut completed=task(30000);completed.status=Status::Completed;
        m.tick(31000,Ok(false),&[completed],&HashMap::new());assert!(m.alerts.is_empty());
    }

    #[test] fn a_monitoring_gap_alone_does_not_require_human_intervention() {
        let mut m=Monitor::default();m.tick(1000,Ok(false),&[task(1000)],&HashMap::new());
        m.tick(1_201_000,Ok(false),&[task(1000)],&HashMap::new());assert!(m.alerts.is_empty());
        m.raise("monitor:suspend","old gap","old gap",1000);
        m.tick(1_203_000,Ok(false),&[task(1000)],&HashMap::new());assert!(m.alerts.is_empty());
    }

}
