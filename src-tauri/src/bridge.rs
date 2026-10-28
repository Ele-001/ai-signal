use crate::NativeState;
use serde::{Deserialize, Serialize};
use std::{
    fs::{self, OpenOptions},
    io::{Read, Write},
    net::{TcpListener, TcpStream},
    path::Path,
    thread,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};
use tauri::{Emitter, Manager};

const MAX_BODY: usize = 64 * 1024;
const MAX_HEADERS: usize = 8 * 1024;
const MAX_TASKS: usize = 512;

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum Status { Attention, Repairing, Running, Completed, Idle }

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ExternalTask {
    pub id: String,
    pub title: String,
    pub status: Status,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
    #[serde(default)]
    pub updated_at: u64,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BridgeInfo {
    pub available: bool,
    pub port: Option<u16>,
    pub token_path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

impl BridgeInfo {
    pub fn unavailable(error: impl Into<String>) -> Self {
        Self { available: false, port: None, token_path: None, error: Some(error.into()) }
    }
}

#[derive(Serialize)]
struct Discovery<'a> { port: u16, token: &'a str, pid: u32 }

fn now_ms() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_millis() as u64
}

fn validate_task(mut task: ExternalTask) -> Result<ExternalTask, &'static str> {
    task.id = task.id.trim().to_owned();
    task.title = task.title.trim().to_owned();
    if task.id.is_empty() || task.id.len() > 160 || task.id.chars().any(char::is_control) {
        return Err("id must contain 1 to 160 bytes without control characters");
    }
    if task.title.is_empty() || task.title.chars().count() > 200 || task.title.chars().any(char::is_control) {
        return Err("title must contain 1 to 200 characters without control characters");
    }
    let source = task.source.as_deref().unwrap_or_default().trim();
    task.source = Some(if source.is_empty() { "外部事件".into() } else { source.into() });
    if task.source.as_ref().is_some_and(|s| s.chars().count() > 100 || s.chars().any(char::is_control)) {
        return Err("source is too long or contains control characters");
    }
    if task.message.as_ref().is_some_and(|s| s.chars().count() > 6000 || s.chars().any(|c| c.is_control() && c != '\n' && c != '\t')) {
        return Err("message is too long or contains invalid control characters");
    }
    // Receiver time keeps stale or untrusted sender clocks from reordering tasks.
    task.updated_at = now_ms();
    Ok(task)
}

fn write_discovery(path: &Path, port: u16, token: &str) -> Result<(), String> {
    let parent = path.parent().ok_or("No bridge data directory")?;
    fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    #[cfg(unix)] {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(parent, fs::Permissions::from_mode(0o700)).map_err(|e| e.to_string())?;
    }
    let temporary = parent.join(format!("bridge-{}-{}.tmp", std::process::id(), &token[..16]));
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)] {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options.open(&temporary).map_err(|e| e.to_string())?;
    let result = (|| {
        serde_json::to_writer(&mut file, &Discovery { port, token, pid: std::process::id() }).map_err(|e| e.to_string())?;
        file.flush().map_err(|e| e.to_string())?;
        file.sync_all().map_err(|e| e.to_string())?;
        drop(file);
        // Windows rename cannot replace an existing file. Credentials rotate at each launch.
        if path.exists() { fs::remove_file(path).map_err(|e| e.to_string())?; }
        fs::rename(&temporary, path).map_err(|e| e.to_string())
    })();
    if result.is_err() { let _ = fs::remove_file(temporary); }
    result
}

pub fn start(app: tauri::AppHandle) {
    let result = (|| -> Result<_, String> {
        let listener = TcpListener::bind(("127.0.0.1", 47832)).or_else(|_| TcpListener::bind(("127.0.0.1", 0))).map_err(|e| e.to_string())?;
        let port = listener.local_addr().map_err(|e| e.to_string())?.port();
        let mut secret = [0u8; 32];
        getrandom::fill(&mut secret).map_err(|e| e.to_string())?;
        let token: String = secret.iter().map(|b| format!("{b:02x}")).collect();
        let path = app.path().app_data_dir().map_err(|e| e.to_string())?.join("bridge.json");
        write_discovery(&path, port, &token)?;
        Ok((listener, port, token, path))
    })();
    match result {
        Ok((listener, port, token, path)) => {
            *app.state::<NativeState>().bridge.lock().expect("bridge info lock") = BridgeInfo {
                available: true, port: Some(port), token_path: Some(path.to_string_lossy().into_owned()), error: None,
            };
            thread::Builder::new().name("ai-signal-bridge".into()).spawn(move || {
                // A small local event stream needs no server framework or thread pool.
                for stream in listener.incoming() {
                    match stream {
                        Ok(mut stream) => {
                            let _ = stream.set_read_timeout(Some(Duration::from_secs(2)));
                            let _ = stream.set_write_timeout(Some(Duration::from_secs(2)));
                            handle_stream(&app, &mut stream, &token, port);
                        }
                        Err(error) => eprintln!("AI红路灯 bridge: {error}"),
                    }
                }
            }).expect("AI红路灯 bridge thread could not start");
        }
        Err(error) => {
            eprintln!("AI红路灯 bridge unavailable: {error}");
            *app.state::<NativeState>().bridge.lock().expect("bridge info lock") = BridgeInfo::unavailable(error);
        }
    }
}

#[derive(Debug)]
struct Request { path: String, content_length: usize }

#[derive(Debug)]
struct HttpError(u16, &'static str);

fn parse_headers(bytes: &[u8], token: &str, port: u16) -> Result<Request, HttpError> {
    let header = std::str::from_utf8(bytes).map_err(|_| HttpError(400, "Invalid HTTP headers"))?;
    let mut lines = header.split("\r\n");
    let first: Vec<_> = lines.next().unwrap_or_default().split_whitespace().collect();
    if first.len() != 3 || first[2] != "HTTP/1.1" { return Err(HttpError(400, "HTTP/1.1 required")); }
    if first[0] != "POST" && first[0] != "GET" { return Err(HttpError(405, "Method not allowed")); }
    let mut host = None;
    let mut auth = None;
    let mut length = None;
    let mut content_type = None;
    for line in lines.filter(|line| !line.is_empty()) {
        let (key, value) = line.split_once(':').ok_or(HttpError(400, "Invalid HTTP header"))?;
        let value = value.trim();
        match key.to_ascii_lowercase().as_str() {
            "origin" | "transfer-encoding" => return Err(HttpError(403, "Browser origins and transfer encoding are not accepted")),
            "host" => { if host.replace(value).is_some() { return Err(HttpError(400, "Duplicate host")); } }
            "authorization" => { if auth.replace(value).is_some() { return Err(HttpError(400, "Duplicate authorization")); } }
            "content-length" => { if length.replace(value).is_some() { return Err(HttpError(400, "Duplicate content length")); } }
            "content-type" => { if content_type.replace(value).is_some() { return Err(HttpError(400, "Duplicate content type")); } }
            _ => {}
        }
    }
    let expected_host = format!("127.0.0.1:{port}");
    if host != Some(expected_host.as_str()) { return Err(HttpError(403, "Invalid host")); }
    let expected_auth = format!("Bearer {token}");
    if auth != Some(expected_auth.as_str()) { return Err(HttpError(401, "Unauthorized")); }
    if first[0] == "GET" && first[1] == "/health" {
        return Ok(Request { path: "/health".into(), content_length: 0 });
    }
    if first[0] != "POST" || first[1] != "/event" { return Err(HttpError(404, "Endpoint not found")); }
    if !content_type.is_some_and(|value| value.split(';').next().unwrap_or_default().trim().eq_ignore_ascii_case("application/json")) {
        return Err(HttpError(415, "Content-Type application/json required"));
    }
    let content_length = length.ok_or(HttpError(411, "Content-Length required"))?.parse::<usize>().map_err(|_| HttpError(400, "Invalid content length"))?;
    if content_length == 0 || content_length > MAX_BODY { return Err(HttpError(413, "Body must contain 1 to 65536 bytes")); }
    Ok(Request { path: "/event".into(), content_length })
}

fn read_request(stream: &mut TcpStream, token: &str, port: u16) -> Result<(Request, Vec<u8>), HttpError> {
    let deadline = Instant::now() + Duration::from_secs(3);
    let mut buffer = Vec::new();
    let header_end = loop {
        if let Some(index) = buffer.windows(4).position(|bytes| bytes == b"\r\n\r\n") { break index + 4; }
        if buffer.len() >= MAX_HEADERS { return Err(HttpError(431, "Headers too large")); }
        set_remaining_timeout(stream, deadline)?;
        let mut chunk = [0u8; 1024];
        let read = stream.read(&mut chunk).map_err(|_| HttpError(408, "Request timed out"))?;
        if read == 0 { return Err(HttpError(400, "Incomplete request")); }
        buffer.extend_from_slice(&chunk[..read]);
    };
    if header_end > MAX_HEADERS { return Err(HttpError(431, "Headers too large")); }
    let request = parse_headers(&buffer[..header_end], token, port)?;
    let mut body = buffer[header_end..].to_vec();
    if body.len() > request.content_length { return Err(HttpError(400, "Unexpected extra body")); }
    while body.len() < request.content_length {
        set_remaining_timeout(stream, deadline)?;
        let mut chunk = [0u8; 4096];
        let remaining = (request.content_length - body.len()).min(chunk.len());
        let read = stream.read(&mut chunk[..remaining]).map_err(|_| HttpError(408, "Request body timed out"))?;
        if read == 0 { return Err(HttpError(400, "Incomplete body")); }
        body.extend_from_slice(&chunk[..read]);
    }
    Ok((request, body))
}

fn set_remaining_timeout(stream: &TcpStream, deadline: Instant) -> Result<(), HttpError> {
    let remaining = deadline.checked_duration_since(Instant::now()).ok_or(HttpError(408, "Request timed out"))?;
    if remaining.is_zero() { return Err(HttpError(408, "Request timed out")); }
    stream.set_read_timeout(Some(remaining)).map_err(|_| HttpError(500, "Could not set socket timeout"))
}

pub(crate) fn publish_task(app: &tauri::AppHandle, task: ExternalTask) -> Result<(),String> {
    publish_at(app,task,None)
}

pub(crate) fn publish_observed_task(app: &tauri::AppHandle, task: ExternalTask, observed_at: u64) -> Result<(),String> {
    publish_at(app,task,Some(observed_at))
}

fn publish_at(app: &tauri::AppHandle, task: ExternalTask, observed_at: Option<u64>) -> Result<(),String> {
    let mut task = validate_task(task).map_err(str::to_owned)?;
    if let Some(at)=observed_at {task.updated_at=task.updated_at.min(at);}
        let state = app.state::<NativeState>();
        let connections=state.connections.lock().map_err(|e|e.to_string())?;
        if (task.id.starts_with("codex:")&&!connections.codex)||(task.id.starts_with("claude:")&&!connections.claude) {return Err("Source disconnected".into());}
        protect_attention(&mut task,&*state.attention_evidence.lock().map_err(|e|e.to_string())?);
        let mut evicted = None;
        {
            let mut tasks = state.tasks.lock().map_err(|_| "Task state unavailable".to_string())?;
            if !tasks.contains_key(&task.id) && tasks.len() >= MAX_TASKS {
                // Keep active tasks. Evict the oldest completed/idle task when full.
                let oldest = tasks.values().filter(|task| matches!(task.status, Status::Completed | Status::Idle)).min_by_key(|task| task.updated_at).map(|task| task.id.clone());
                if let Some(id) = oldest {
                    tasks.remove(&id);
                    evicted = Some(id);
                } else { return Err("Too many active tasks".into()); }
            }
            tasks.insert(task.id.clone(), task.clone());
        }
        drop(connections);
        if let Some(id) = evicted {
            app.emit("external-task-removed", id).map_err(|_| "Could not deliver removal event".to_string())?;
        }
        app.emit("external-task", &task).map_err(|_| "Could not deliver event".to_string())?;
    Ok(())
}

fn protect_attention(task: &mut ExternalTask, evidence: &std::collections::HashMap<String,String>) {
    if let Some(reason)=evidence.get(&task.id) {task.status=Status::Attention;task.message=Some(reason.clone());}
}

fn handle_stream(app: &tauri::AppHandle, stream: &mut TcpStream, token: &str, port: u16) {
    let result = (|| {
        let (request, body) = read_request(stream, token, port)?;
        if request.path == "/health" { return Ok(()); }
        let task: ExternalTask = serde_json::from_slice(&body).map_err(|_| HttpError(400, "Invalid task JSON"))?;
        let task=validate_task(task).map_err(|message|HttpError(400,message))?;
        if task.id.starts_with("monitor:") {return Err(HttpError(400,"Reserved task id"));}
        if !crate::connections::accepts(app,&task.id) {return Err(HttpError(409,"Source disconnected"));}
        publish_task(app,task).map_err(|e|if e=="Too many active tasks" {HttpError(429,"Too many active tasks")} else {HttpError(500,"Could not store task")})?;
        Ok(())
    })();
    let (status, payload) = match result {
        Ok(()) => (200, serde_json::json!({ "ok": true })),
        Err(HttpError(status, message)) => (status, serde_json::json!({ "ok": false, "error": message })),
    };
    let body = payload.to_string();
    let response = format!("HTTP/1.1 {status} {}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\nCache-Control: no-store\r\nX-Content-Type-Options: nosniff\r\n\r\n{body}", if status == 200 { "OK" } else { "Error" }, body.len());
    let _ = stream.write_all(response.as_bytes());
    let _ = stream.flush();
}

#[cfg(test)]
mod tests {
    use super::*;

    fn headers(extra: &str) -> String {
        format!("POST /event HTTP/1.1\r\nHost: 127.0.0.1:47832\r\nAuthorization: Bearer secret\r\nContent-Type: application/json\r\nContent-Length: 10\r\n{extra}\r\n")
    }

    #[test]
    fn bridge_requires_auth_loopback_host_and_no_browser_origin() {
        assert!(parse_headers(headers("").as_bytes(), "secret", 47832).is_ok());
        assert_eq!(parse_headers(headers("").as_bytes(), "wrong", 47832).unwrap_err().0, 401);
        assert_eq!(parse_headers(headers("Origin: https://example.com\r\n").as_bytes(), "secret", 47832).unwrap_err().0, 403);
        assert_eq!(parse_headers(headers("Transfer-Encoding: chunked\r\n").as_bytes(), "secret", 47832).unwrap_err().0, 403);
        assert_eq!(parse_headers(headers("").as_bytes(), "secret", 12345).unwrap_err().0, 403);
        assert_eq!(parse_headers(headers("Content-Length: 10\r\n").as_bytes(), "secret", 47832).unwrap_err().0, 400);
    }

    #[test]
    fn task_validation_rejects_invalid_inputs_and_trusts_receiver_clock() {
        let input = r#"{"id":" session-1 ","title":" 测试任务 ","status":"running","updatedAt":1}"#;
        let task = validate_task(serde_json::from_str(input).unwrap()).unwrap();
        assert_eq!(task.id, "session-1");
        assert_eq!(task.title, "测试任务");
        assert!(task.updated_at > 1);
        let bad_status = input.replace("running", "failed");
        assert!(serde_json::from_str::<ExternalTask>(&bad_status).is_err());
        let mut bad_task = task;
        bad_task.id = "\n".into();
        assert!(validate_task(bad_task).is_err());
    }

    #[test]
    fn body_limit_and_missing_content_type_are_rejected() {
        let large = headers("").replace("Content-Length: 10", "Content-Length: 65537");
        assert_eq!(parse_headers(large.as_bytes(), "secret", 47832).unwrap_err().0, 413);
        let no_type = headers("").replace("Content-Type: application/json\r\n", "");
        assert_eq!(parse_headers(no_type.as_bytes(), "secret", 47832).unwrap_err().0, 415);
    }

    #[test]
    fn task_source_defaults_and_unicode_lengths_match_frontend() {
        let input = r#"{"id":"test","title":"任务","status":"running"}"#;
        let mut task: ExternalTask = serde_json::from_str(input).unwrap();
        assert_eq!(validate_task(task.clone()).unwrap().source.as_deref(), Some("外部事件"));
        task.source = Some("  ".into());
        assert_eq!(validate_task(task.clone()).unwrap().source.as_deref(), Some("外部事件"));
        task.title = "测".repeat(200);
        task.source = Some("源".repeat(100));
        task.message = Some("文".repeat(6000));
        assert!(validate_task(task.clone()).is_ok());
        task.title.push('测');
        assert!(validate_task(task.clone()).is_err());
        task.title = "任务".into();
        task.source.as_mut().unwrap().push('源');
        assert!(validate_task(task.clone()).is_err());
        task.source = Some(" Codex Hook ".into());
        task.message.as_mut().unwrap().push('文');
        assert!(validate_task(task.clone()).is_err());
        task.message = None;
        assert_eq!(validate_task(task).unwrap().source.as_deref(), Some("Codex Hook"));
    }
    #[test] fn ordinary_progress_and_completion_never_override_unanswered_questions() {
        let evidence=std::collections::HashMap::from([("codex:test".into(),"等待回答".into())]);
        for status in [Status::Running,Status::Completed,Status::Idle] {
            let mut task=ExternalTask{id:"codex:test".into(),title:"test".into(),status,source:None,message:None,updated_at:1};
            protect_attention(&mut task,&evidence);assert_eq!(task.status,Status::Attention);assert_eq!(task.message.as_deref(),Some("等待回答"));
        }
    }

}
