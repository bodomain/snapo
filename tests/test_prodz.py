"""Tests for the unified `prodz` command-line entrypoint."""
import subprocess
import sys
import uuid
from datetime import datetime
from pathlib import Path

import pytest

from database import Config, Store
from sessions import Revision, Session, SessionHistory

REPO = Path(__file__).resolve().parent.parent
PRODZ = REPO / "prodz"


def _make_db(tmp_path, count=2):
    db = tmp_path / "test.db"
    store = Store(Config.load(db, local=True))
    store.initialize()
    ids = []
    for i in range(count):
        revision = store.create(Session.now(f"Activity{i}", float(10 + i), f"comment {i}"))
        ids.append(revision.session_id)
    return db, ids


def _run(db, *args):
    return subprocess.run(
        [sys.executable, str(PRODZ), "--db", str(db), *args],
        capture_output=True, text=True, timeout=30, cwd=str(REPO),
    )


def test_status_and_sync(tmp_path):
    db, _ = _make_db(tmp_path)
    status = _run(db, "status")
    assert status.returncode == 0
    assert "Mode:" in status.stdout
    assert "Pending operations:" in status.stdout
    sync = _run(db, "sync")
    assert sync.returncode == 0
    assert "Mode:" in sync.stdout


def test_history_lists_sessions(tmp_path):
    db, ids = _make_db(tmp_path)
    result = _run(db, "history")
    assert result.returncode == 0
    assert "Activity0" in result.stdout
    assert "Activity1" in result.stdout
    assert "2 session(s)" in result.stdout


def test_history_filters(tmp_path):
    db, _ = _make_db(tmp_path)
    by_activity = _run(db, "history", "--activity", "Activity0")
    assert by_activity.returncode == 0
    assert "Activity0" in by_activity.stdout
    assert "Activity1" not in by_activity.stdout
    by_search = _run(db, "history", "--search", "comment 1")
    assert "Activity1" in by_search.stdout
    assert "Activity0" not in by_search.stdout


def test_show_session(tmp_path):
    db, ids = _make_db(tmp_path)
    result = _run(db, "show", ids[0])
    assert result.returncode == 0
    assert ids[0] in result.stdout
    assert "Activity0" in result.stdout
    assert "10.00" in result.stdout


def test_trash_and_restore(tmp_path):
    db, ids = _make_db(tmp_path)
    sid = ids[0]
    assert _run(db, "trash", sid).returncode == 0
    history = _run(db, "history")
    assert "Activity0" not in history.stdout
    trashed = _run(db, "history", "--view", "trash")
    assert "Activity0" in trashed.stdout
    assert _run(db, "restore", sid).returncode == 0
    history = _run(db, "history")
    assert "Activity0" in history.stdout


def test_trash_conflicted_is_rejected(tmp_path):
    db, ids = _make_db(tmp_path)
    store = Store(Config.load(db, local=True))
    with store._connection() as (conn, _):
        hist = SessionHistory(conn, store._device_id())
        entry = hist.get(ids[0])
        competing = Revision(
            str(uuid.uuid4()), ids[0], (), store._device_id(),
            datetime.now().astimezone().isoformat(),
            entry.current.session.validated(), False, None,
        )
        hist.append(competing)
        conn.commit()
    result = _run(db, "trash", ids[0])
    assert result.returncode != 0
    assert "Resolve competing versions" in result.stderr


def test_export_csv(tmp_path):
    db, _ = _make_db(tmp_path)
    out = tmp_path / "out.csv"
    result = _run(db, "export", str(out))
    assert result.returncode == 0
    text = out.read_text()
    assert text.startswith("session_id,date,timestamp,activity,duration_minutes,comment")
    assert "Activity0" in text
    assert "Activity1" in text


def test_report(tmp_path):
    db, _ = _make_db(tmp_path)
    result = _run(db, "report")
    assert result.returncode == 0
    assert "Activity0" in result.stdout
    assert "TOTAL" in result.stdout


def test_history_empty_database(tmp_path):
    db = tmp_path / "empty.db"
    store = Store(Config.load(db, local=True))
    store.initialize()
    result = _run(db, "history")
    assert result.returncode == 0
    assert "No sessions." in result.stdout


def test_missing_session_errors(tmp_path):
    db, _ = _make_db(tmp_path)
    for cmd in ("show", "trash", "restore"):
        result = _run(db, cmd, "does-not-exist")
        assert result.returncode != 0
        assert "not found" in result.stderr.lower() or "not found" in result.stdout.lower()
