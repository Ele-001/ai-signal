#!/usr/bin/env python3
"""Generate a reviewable hooks file; never install or trust hooks automatically."""

import argparse
import json
from pathlib import Path
import shlex
import subprocess
import sys

from codex_hook import EVENT_STATUS


def configuration(script, python_posix="python3", script_windows=None, python_windows="python"):
    result = {"description": "AI红路灯 local status indicator. Review these commands before trusting them.", "hooks": {}}
    for name in EVENT_STATUS:
        handler = {
            "type": "command",
            "command": " ".join(shlex.quote(arg) for arg in [python_posix, str(script), "--event", name]),
            "timeout": 3,
        }
        if script_windows:
            handler["commandWindows"] = subprocess.list2cmdline([python_windows, str(script_windows), "--event", name])
        result["hooks"][name] = [{"hooks": [handler]}]
    return result


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output", type=Path, default=Path(__file__).with_name("hooks.example.json"))
    parser.add_argument("--script", type=Path, default=Path(__file__).with_name("codex_hook.py").resolve())
    parser.add_argument("--python", default="python3", help="POSIX Python executable")
    parser.add_argument("--script-windows", help="absolute Windows path when generating on macOS")
    parser.add_argument("--python-windows", default="python", help="Windows Python executable")
    args = parser.parse_args()
    script = args.script.resolve()
    windows_script = args.script_windows or (str(script) if sys.platform == "win32" else None)
    args.output.write_text(json.dumps(configuration(script, args.python, windows_script, args.python_windows), ensure_ascii=False, indent=2) + "\n", encoding="utf-8")
    print(f"Generated {args.output.resolve()}. Review before merging into Codex hooks.json.")


if __name__ == "__main__":
    main()
