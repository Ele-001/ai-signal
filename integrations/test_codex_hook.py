"""Standard-library contract tests; no Codex or live app configuration is changed."""

from contextlib import contextmanager
import http.server
import json
from pathlib import Path
import subprocess
import sys
import tempfile
import threading
import unittest

import codex_hook
import generate_hooks


@contextmanager
def local_receiver(status=200):
    received = []

    class Handler(http.server.BaseHTTPRequestHandler):
        def do_POST(self):
            body = self.rfile.read(int(self.headers["Content-Length"]))
            received.append((self.path, self.headers["Authorization"], json.loads(body)))
            self.send_response(status)
            if status == 302:
                self.send_header("Location", "http://127.0.0.1:1/do-not-follow")
            self.end_headers()
            self.wfile.write(b"{}")

        def log_message(self, *args):
            pass

    server = http.server.ThreadingHTTPServer(("127.0.0.1", 0), Handler)
    thread = threading.Thread(target=server.serve_forever, daemon=True)
    thread.start()
    try:
        with tempfile.TemporaryDirectory() as directory:
            bridge = Path(directory) / "bridge.json"
            bridge.write_text(json.dumps({"port": server.server_port, "token": "a" * 32}), encoding="utf-8")
            yield bridge, received
    finally:
        server.shutdown()
        server.server_close()
        thread.join()


class HookTests(unittest.TestCase):
    def payload(self, event):
        return {"session_id": "session-1", "cwd": "/private/project/客户项目", "hook_event_name": event}

    def run_adapter(self, *args, stdin=""):
        return subprocess.run([sys.executable, str(Path(codex_hook.__file__)), *args], input=stdin, capture_output=True, text=True, timeout=5)

    def test_all_lifecycle_mappings_and_private_content(self):
        for name, status in codex_hook.EVENT_STATUS.items():
            with self.subTest(event=name):
                payload = self.payload(name)
                payload.update({"prompt": "SECRET", "tool_input": {"command": "SECRET"}, "tool_response": {"error": "SECRET"}})
                event = codex_hook.event_from_hook(payload)
                self.assertEqual(event["status"], status)
                self.assertEqual(event["id"], "codex:session-1")
                self.assertEqual(event["title"], "客户项目")
                self.assertNotIn("SECRET", json.dumps(event))
        self.assertEqual(codex_hook.event_from_hook(self.payload("PostToolUse"))["status"], "running")

    def test_question_tool_is_attention_instead_of_running(self):
        event = codex_hook.event_from_hook({**self.payload("PreToolUse"), "tool_name": "AskUserQuestion"})
        self.assertEqual(event["status"], "attention")

    def test_subagent_stop_does_not_complete_parent(self):
        self.assertIsNone(codex_hook.event_from_hook(self.payload("SubagentStop")))
        self.assertIsNone(codex_hook.event_from_hook(self.payload("SessionStart")))

    def test_input_validation_and_windows_title(self):
        for payload in [[], {}, {"hook_event_name": "Stop", "session_id": 1}, {**self.payload("Stop"), "cwd": False}]:
            with self.subTest(payload=payload), self.assertRaises(ValueError):
                codex_hook.event_from_hook(payload)
        with self.assertRaises(ValueError):
            codex_hook.event_from_hook(self.payload("Stop"), "PermissionRequest")
        self.assertEqual(codex_hook.event_from_hook({**self.payload("Stop"), "cwd": r"C:\work\demo"})["title"], "demo")

    def test_authenticated_http_transport(self):
        with local_receiver() as (bridge, received):
            event = codex_hook.event_from_hook(self.payload("PermissionRequest"))
            codex_hook.send_event(event, bridge)
            self.assertEqual(received, [("/event", "Bearer " + "a" * 32, event)])

    def test_redirect_is_refused(self):
        with local_receiver(302) as (bridge, received):
            with self.assertRaisesRegex(ValueError, "redirect refused"):
                codex_hook.send_event(codex_hook.event_from_hook(self.payload("Stop")), bridge)
            self.assertEqual(len(received), 1)

    def test_discovery_validation(self):
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "bridge.json"
            for value in [{"port": True, "token": "a" * 32}, {"port": 70000, "token": "a" * 32}, {"port": 1111, "token": "short"}, {"port": 1111, "token": "a" * 20 + "\r\n"}, []]:
                path.write_text(json.dumps(value))
                with self.subTest(value=value), self.assertRaises(ValueError):
                    codex_hook.read_bridge(path)

    def test_stop_failure_is_advisory_with_valid_json_stdout(self):
        result = self.run_adapter("--event", "Stop", "--bridge", "/not-an-ai-signal-bridge", stdin=json.dumps(self.payload("Stop")))
        self.assertEqual(result.returncode, 0)
        self.assertEqual(json.loads(result.stdout), {})
        self.assertIn("not delivered", result.stderr)
        malformed = self.run_adapter("--event", "Stop", stdin="not json")
        self.assertEqual(malformed.returncode, 0)
        self.assertEqual(json.loads(malformed.stdout), {})

    def test_manual_event_and_strict_failure(self):
        with local_receiver() as (bridge, received):
            result = self.run_adapter("--bridge", str(bridge), "--status", "repairing", "--task-id", "build", "--title", "构建", "--strict")
            self.assertEqual(result.returncode, 0, result.stderr)
            self.assertEqual(result.stdout, "")
            self.assertEqual(received[0][2]["status"], "repairing")
            self.assertEqual(received[0][2]["id"], "external:build")
        result = self.run_adapter("--bridge", "/not-an-ai-signal-bridge", "--status", "running", "--strict")
        self.assertEqual(result.returncode, 1)
        self.assertEqual(result.stdout, "")

    def test_generated_commands_preserve_paths(self):
        script = Path("/tmp/AI Signal/it's local/codex_hook.py")
        config = generate_hooks.configuration(script, "/usr/bin/python3", r"C:\AI Signal\codex_hook.py")
        import shlex
        for event, groups in config["hooks"].items():
            handler = groups[0]["hooks"][0]
            self.assertEqual(shlex.split(handler["command"]), ["/usr/bin/python3", str(script), "--event", event])
            self.assertIn('"C:\\AI Signal\\codex_hook.py"', handler["commandWindows"])
            self.assertEqual(handler["timeout"], 3)
            self.assertNotIn("async", handler)


if __name__ == "__main__":
    unittest.main()
