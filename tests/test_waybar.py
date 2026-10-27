"""State export for Waybar: snapshot contents, reader output, pause signal."""
import json
import os
import re
import subprocess
import sys
import time
from datetime import datetime
from pathlib import Path

import prodz_cli
from timer import Timer

SCRIPT = Path(__file__).resolve().parent.parent / "scripts" / "prodz_waybar.py"
STARTED_AT = datetime.fromisoformat("2026-10-27T14:03:00+02:00")


def _snapshot(path):
    return json.loads(path.read_text())


def _write_state(tmp_path, monkeypatch, timer, phase="Work", cycle=2, cycles=4):
    monkeypatch.setenv("PRODZ_STATE_FILE", str(tmp_path / "session.json"))
    prodz_cli._write_state(phase, timer, cycle, cycles)
    return tmp_path / "session.json"


def _run_waybar(state_path, *args):
    return subprocess.run(
        [sys.executable, str(SCRIPT), *args],
        capture_output=True,
        text=True,
        env={**os.environ, "PRODZ_STATE_FILE": str(state_path)},
        timeout=10,
    )


def test_state_snapshot_matches_timer(tmp_path, monkeypatch):
    now = [0.0]
    timer = Timer(150, activity="Lernen", comment="Vokabeln", clock=lambda: now[0],
                  started_at=STARTED_AT)
    now[0] = 30
    path = _write_state(tmp_path, monkeypatch, timer)
    state = _snapshot(path)
    assert state["phase"] == "Work"
    assert state["state"] == "running"
    assert state["activity"] == "Lernen"
    assert state["comment"] == "Vokabeln"
    assert state["cycle"] == 2
    assert state["cycles"] == 4
    assert state["elapsed"] == 30
    assert state["remaining"] == 120
    assert state["endless"] is False
    assert state["started_at"] == STARTED_AT.isoformat()
    assert state["pid"] == os.getpid()
    # The write is atomic: no partial file is left behind.
    assert not path.with_suffix(path.suffix + ".tmp").exists()


def test_state_snapshot_paused_and_endless(tmp_path, monkeypatch):
    now = [0.0]
    timer = Timer(90, activity="Lernen", clock=lambda: now[0], started_at=STARTED_AT)
    now[0] = 20
    timer.pause()
    now[0] = 500
    path = _write_state(tmp_path, monkeypatch, timer)
    state = _snapshot(path)
    assert state["state"] == "paused"
    assert state["elapsed"] == 20
    assert state["remaining"] == 70

    endless = Timer(activity="Lesen", clock=lambda: now[0], started_at=STARTED_AT)
    now[0] = 505
    prodz_cli._write_state("Work", endless, 1, 1)
    state = _snapshot(path)
    assert state["state"] == "running"
    assert state["remaining"] is None
    assert state["endless"] is True


def test_clear_state_removes_the_snapshot(tmp_path, monkeypatch):
    timer = Timer(60, activity="Lernen", started_at=STARTED_AT)
    path = _write_state(tmp_path, monkeypatch, timer)
    assert path.exists()
    prodz_cli._clear_state()
    assert not path.exists()
    prodz_cli._clear_state()  # idempotent after the session ended


def test_waybar_hides_without_a_live_session(tmp_path, monkeypatch):
    missing = tmp_path / "nowhere.json"
    monkeypatch.setenv("PRODZ_STATE_FILE", str(missing))
    result = _run_waybar(missing)
    assert result.returncode == 0
    assert result.stdout == ""


def test_waybar_hides_for_a_dead_pid(tmp_path, monkeypatch):
    child = subprocess.Popen([sys.executable, "-c", "import time; time.sleep(0)"])
    child.wait()
    path = tmp_path / "session.json"
    path.write_text(json.dumps({"phase": "Work", "state": "running", "pid": child.pid}))
    monkeypatch.setenv("PRODZ_STATE_FILE", str(path))
    result = _run_waybar(path)
    assert result.returncode == 0
    assert result.stdout == "", "A killed session must not keep the module visible."


def test_waybar_formats_running_paused_and_critical(tmp_path, monkeypatch):
    timer = Timer(150, activity="Lernen", comment="Vokabeln", started_at=STARTED_AT)
    path = _write_state(tmp_path, monkeypatch, timer)
    result = _run_waybar(path)
    module = json.loads(result.stdout)
    assert re.fullmatch(r"Work \d{2}:\d{2}", module["text"])
    assert "prodz-running" in module["class"]
    assert "Lernen" in module["tooltip"]
    assert "Vokabeln" in module["tooltip"]
    assert "2/4" in module["tooltip"]
    assert "läuft" in module["tooltip"]

    timer.pause()
    prodz_cli._write_state("Work", timer, 2, 4)
    module = json.loads(_run_waybar(path).stdout)
    assert module["text"].startswith("\u23f8")
    assert "prodz-paused" in module["class"]
    assert "pausiert" in module["tooltip"]

    critical = Timer(45, activity="Lernen", started_at=STARTED_AT)
    prodz_cli._write_state("Work", critical, 2, 4)
    module = json.loads(_run_waybar(path).stdout)
    assert "prodz-critical" in module["class"]


def test_waybar_formats_endless_sessions(tmp_path, monkeypatch):
    endless = Timer(activity="Lesen", started_at=STARTED_AT)
    path = _write_state(tmp_path, monkeypatch, endless)
    module = json.loads(_run_waybar(path).stdout)
    assert re.fullmatch(r"Work \d{2}:\d{2} elapsed", module["text"])
    assert "endlos" in module["tooltip"]


def test_toggle_pauses_the_running_session(tmp_path, monkeypatch):
    state = tmp_path / "session.json"
    marker = tmp_path / "toggled"
    child = subprocess.Popen(
        [
            sys.executable,
            "-c",
            "import json, os, signal, sys, time\n"
            "from pathlib import Path\n"
            "signal.signal(signal.SIGUSR1, lambda *_: Path(sys.argv[1]).write_text('toggled'))\n"
            "Path(sys.argv[2]).write_text(json.dumps({'phase': 'Work', 'state': 'running', 'pid': os.getpid()}))\n"
            "time.sleep(10)\n",
            str(marker),
            str(state),
        ]
    )
    try:
        for _ in range(100):
            if state.exists():
                break
            time.sleep(0.05)
        assert _run_waybar(state, "--toggle").returncode == 0
        for _ in range(100):
            if marker.exists():
                break
            time.sleep(0.05)
        assert marker.exists(), "SIGUSR1 must reach the live session process."
    finally:
        child.terminate()
        child.wait()
