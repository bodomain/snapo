#!/usr/bin/env python3
"""Waybar module for the running prodz_cli session.

Shows the current phase and remaining time, hides itself when no session is
running, and toggles pause/resume on left-click via SIGUSR1. A stale state
file from a killed session (dead pid) is ignored, so the bar never shows a
frozen timer.

Waybar configuration (~/.config/waybar/config):

    "custom/prodz": {
        "exec": "python3 /path/to/scripts/prodz_waybar.py",
        "return-type": "json",
        "interval": 1,
        "on-click": "python3 /path/to/scripts/prodz_waybar.py --toggle"
    }

Optional styling (style.css) via the classes prodz-running, prodz-paused and
prodz-critical (less than a minute left).
"""
import json
import os
import signal
import sys
from datetime import datetime
from pathlib import Path

STATE_DIR = Path(os.environ.get("XDG_STATE_HOME", Path.home() / ".local/state")) / "prodz"


def state_file():
    override = os.environ.get("PRODZ_STATE_FILE")
    return Path(override) if override else STATE_DIR / "session.json"


def _pid_alive(pid):
    try:
        os.kill(pid, 0)
        return True
    except ProcessLookupError:
        return False
    except PermissionError:
        return True


def _load_snapshot():
    """The session snapshot, or None when no live session exists."""
    path = state_file()
    try:
        snapshot = json.loads(path.read_text())
    except (OSError, ValueError):
        return None
    pid = snapshot.get("pid")
    if not isinstance(pid, int) or not _pid_alive(pid):
        return None  # stale file from a killed session
    return snapshot


def _clock(seconds):
    return f"{int(seconds) // 60:02d}:{int(seconds) % 60:02d}"


def module(snapshot):
    """Turns a session snapshot into Waybar's JSON output format."""
    phase = snapshot.get("phase", "?")
    paused = snapshot.get("state") == "paused"
    remaining = snapshot.get("remaining")
    endless = remaining is None
    seconds = snapshot.get("elapsed", 0) if endless else remaining
    prefix = "\u23f8 " if paused else ""  # pause symbol
    suffix = " elapsed" if endless else ""
    classes = ["prodz-paused" if paused else "prodz-running"]
    if not endless and 0 < remaining <= 60:
        classes.append("prodz-critical")
    tooltip = _tooltip(snapshot, endless)
    return {
        "text": f"{prefix}{phase} {_clock(seconds)}{suffix}",
        "tooltip": tooltip,
        "class": classes,
        "alt": phase,
    }


def _tooltip(snapshot, endless):
    activity = snapshot.get("activity") or "?"
    comment = snapshot.get("comment") or ""
    phase = snapshot.get("phase", "?")
    state = "pausiert" if snapshot.get("state") == "paused" else "läuft"
    cycle = snapshot.get("cycle", "?")
    cycles = snapshot.get("cycles", "?")
    started = ""
    try:
        started = datetime.fromisoformat(snapshot["started_at"]).strftime("%H:%M")
        started = f"\nGestartet {started}"
    except (KeyError, TypeError, ValueError):
        pass
    lines = [comment] if comment else []
    lines.append(
        f"{activity} · {phase} · Zyklus {cycle}/{cycles} · {state}"
        + (" (endlos)" if endless else "")
        + started
    )
    return "\n".join(lines)


def toggle():
    """Sends SIGUSR1 to the running prodz_cli session (pause/resume)."""
    snapshot = _load_snapshot()
    if snapshot:
        os.kill(snapshot["pid"], signal.SIGUSR1)


def main():
    if "--toggle" in sys.argv[1:]:
        toggle()
        return
    snapshot = _load_snapshot()
    if snapshot is not None:
        print(json.dumps(module(snapshot)))


if __name__ == "__main__":
    main()
