#!/usr/bin/env python3
"""Forward a Codex lifecycle event to the local AI红路灯 desktop bridge."""

import argparse
import json
import os
from pathlib import Path, PureWindowsPath
import sys
import time
import urllib.error
import urllib.request

APP_ID = "io.aisignal.desktop"
MAX_INPUT = 2 * 1024 * 1024
STATUSES = ("attention", "repairing", "running", "completed", "idle")
EVENT_STATUS = {
    "UserPromptSubmit": "running",
    "PreToolUse": "running",
    "PermissionRequest": "attention",
    "PostToolUse": "running",
    "Stop": "completed",
    "Interrupt": "idle",
    "SessionEnd": "idle",
}
MESSAGES = {
    "UserPromptSubmit": "已提交请求，AI 开始工作",
    "PreToolUse": "AI 正在执行工具",
    "PermissionRequest": "Codex 请求审批，请检查是否需要你处理",
    "PostToolUse": "工具已返回，AI 继续工作",
    "Stop": "本轮已结束；不代表整个任务已验收成功",
    "Interrupt": "本轮已被中断",
    "SessionEnd": "会话已结束",
}


def default_bridge_path():
    """Match Tauri app_data_dir for the configured application identifier."""
    if sys.platform == "darwin":
        base = Path.home() / "Library" / "Application Support"
    elif os.name == "nt":
        value = os.environ.get("APPDATA")
        if not value:
            raise ValueError("APPDATA is missing; use --bridge with an explicit path")
        base = Path(value)
    else:
        base = Path(os.environ.get("XDG_DATA_HOME", Path.home() / ".local" / "share"))
    return base / APP_ID / "bridge.json"


def nonempty_string(value, name, limit):
    if not isinstance(value, str) or not value.strip() or len(value) > limit:
        raise ValueError(f"{name} must be a non-empty string of at most {limit} characters")
    return value.strip()


def event_from_hook(payload, expected_event=None):
    if not isinstance(payload, dict):
        raise ValueError("hook input must be a JSON object")
    name = nonempty_string(payload.get("hook_event_name"), "hook_event_name", 64)
    if expected_event is not None and name != expected_event:
        raise ValueError("hook_event_name does not match --event")
    if name not in EVENT_STATUS:
        return None
    session = nonempty_string(payload.get("session_id"), "session_id", 100)
    cwd = payload.get("cwd", "")
    if not isinstance(cwd, str):
        raise ValueError("cwd must be a string")
    # Only a directory label is sent: prompt text, tool arguments and outputs stay private.
    title = (PureWindowsPath(cwd).name if "\\" in cwd else Path(cwd).name) or "Codex 会话"
    status = EVENT_STATUS[name]
    if name == "PreToolUse" and payload.get("tool_name") == "AskUserQuestion":
        status = "attention"
    return {
        "id": "codex:" + session,
        "title": title[:120],
        "status": status,
        "source": "Codex Hook",
        "message": "等待用户回答，请检查来源软件" if status == "attention" and name == "PreToolUse" else MESSAGES[name],
        "updatedAt": int(time.time() * 1000),
    }


def read_bridge(path):
    if path.stat().st_size > 4096:
        raise ValueError("bridge discovery file is unexpectedly large")
    data = json.loads(path.read_text(encoding="utf-8"))
    if not isinstance(data, dict):
        raise ValueError("bridge discovery must be a JSON object")
    port = data.get("port")
    token = data.get("token")
    if type(port) is not int or not 1 <= port <= 65535:
        raise ValueError("bridge port must be an integer from 1 to 65535")
    nonempty_string(token, "bridge token", 512)
    if len(token) < 16 or any(char.isspace() for char in token):
        raise ValueError("bridge token is invalid")
    return port, token


class NoRedirect(urllib.request.HTTPRedirectHandler):
    def redirect_request(self, req, fp, code, msg, headers, newurl):
        raise ValueError("bridge redirect refused")


def send_event(event, bridge_path):
    port, token = read_bridge(bridge_path)
    request = urllib.request.Request(
        f"http://127.0.0.1:{port}/event",
        data=json.dumps(event, ensure_ascii=False).encode("utf-8"),
        headers={"Content-Type": "application/json", "Authorization": "Bearer " + token},
        method="POST",
    )
    # No proxy and no redirect: the per-launch token must stay on loopback.
    opener = urllib.request.build_opener(urllib.request.ProxyHandler({}), NoRedirect())
    with opener.open(request, timeout=0.8) as response:
        if not 200 <= response.status < 300:
            raise ValueError(f"bridge returned HTTP {response.status}")


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--bridge", type=Path, help="explicit bridge.json discovery file")
    parser.add_argument("--event", choices=tuple(EVENT_STATUS), help="expected hook event (for safe Stop output)")
    parser.add_argument("--status", choices=STATUSES, help="send an explicit status instead of reading a hook")
    parser.add_argument("--task-id", default="manual", help="stable task identifier for --status")
    parser.add_argument("--title", default="我的 AI 任务", help="task label for --status")
    parser.add_argument("--message", default="手动发送的状态", help="short explanation for --status")
    parser.add_argument("--strict", action="store_true", help="exit 1 on delivery failure; omit in Codex hooks")
    args = parser.parse_args(argv)
    stop_output = args.event == "Stop"
    try:
        if args.status:
            if args.event:
                raise ValueError("--status and --event cannot be combined")
            event = {
                "id": "external:" + nonempty_string(args.task_id, "task id", 100),
                "title": nonempty_string(args.title, "title", 120),
                "status": args.status,
                "source": "外部脚本",
                "message": nonempty_string(args.message, "message", 500),
                "updatedAt": int(time.time() * 1000),
            }
        else:
            raw = sys.stdin.buffer.read(MAX_INPUT + 1)
            if len(raw) > MAX_INPUT:
                raise ValueError("hook input exceeds 2 MiB")
            payload = json.loads(raw)
            if isinstance(payload, dict) and payload.get("hook_event_name") == "Stop":
                stop_output = True
            event = event_from_hook(payload, args.event)
        if event is not None:
            send_event(event, args.bridge or default_bridge_path())
        return 0
    except (OSError, ValueError, urllib.error.URLError) as error:
        # Monitoring must never approve, deny or block an AI action.
        print(f"AI红路灯: event was not delivered ({error}). Open the desktop app first.", file=sys.stderr)
        return 1 if args.strict else 0
    finally:
        if stop_output:
            print("{}")


if __name__ == "__main__":
    sys.exit(main())
