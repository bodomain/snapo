import sqlite3

import pytest

from database import Config, MigrationRequired, Store
from migrate_db import migrate
from install_service import unit


def legacy(path):
    conn = sqlite3.connect(path)
    conn.execute('CREATE TABLE sessions (id INTEGER PRIMARY KEY, date TEXT, timestamp TEXT, activity TEXT, duration_minutes REAL, comment TEXT)')
    conn.execute('INSERT INTO sessions VALUES (1,?,?,?,?,?)', ('2026-01-01','2026-01-01T12:00:00','Legacy',12.25,'original'))
    conn.execute('CREATE TABLE metadata (key TEXT PRIMARY KEY, value TEXT)')
    conn.execute("INSERT INTO metadata VALUES ('name','original dataset')")
    conn.commit()
    conn.close()


def test_migration_preserves_backup_legacy_and_is_repeatable(tmp_path):
    path = tmp_path / 'legacy.db'
    legacy(path)
    config = Config(path, local=True)
    with pytest.raises(MigrationRequired):
        Store(config).initialize()
    before = path.read_bytes()
    assert migrate(config)['merged_sessions'] == 1
    assert path.read_bytes() == before
    with pytest.raises(ValueError, match='ALL computers'):
        migrate(config, apply=True)
    result = migrate(config, apply=True, old_clients_stopped=True)
    store = Store(config)
    store.initialize()
    entry = store.entries()[0]
    assert entry.current.session.duration_minutes == 12.25
    assert entry.current.session.timestamp == '2026-01-01T12:00:00'
    assert entry.current.legacy_id == 1
    from pathlib import Path
    assert (Path(result['backup']) / 'legacy.db').read_bytes() == before
    assert migrate(config, apply=True, old_clients_stopped=True)['already_migrated']
    assert store.entries() == [entry]
    conn = sqlite3.connect(path)
    assert conn.execute('SELECT COUNT(*) FROM sessions').fetchone()[0] == 1
    assert conn.execute('SELECT value FROM metadata').fetchone()[0] == 'original dataset'
    conn.close()


def test_shared_namespace_produces_same_legacy_ids(tmp_path):
    namespace = '9118edc2-c9a3-4bf9-8f88-c368ce9d935e'
    results = []
    for name in ('a', 'b'):
        path = tmp_path / name
        legacy(path)
        config = Config(path, local=True)
        migrate(config, apply=True, old_clients_stopped=True, dataset_id=namespace)
        results.append(Store(config).entries())
    assert results[0] == results[1]


def test_service_paths_are_quoted_and_checkout_independent(tmp_path):
    project = tmp_path / 'with spaces %'
    text = unit(project, project / '.venv/bin/python', project / 'sessions.db')
    assert '%%' in text
    assert '"' in text
    assert '/home/user/Desktop' not in text
    assert '--db' in text


def test_failed_import_rolls_back_schema_and_can_be_retried(tmp_path):
    path = tmp_path / 'legacy.db'
    legacy(path)
    with sqlite3.connect(path) as conn:
        conn.execute("UPDATE sessions SET duration_minutes='invalid'")
    config = Config(path, local=True)
    with pytest.raises(ValueError):
        migrate(config, apply=True, old_clients_stopped=True)
    with sqlite3.connect(path) as conn:
        assert not conn.execute("SELECT name FROM sqlite_master WHERE name='snapo_config'").fetchall()
        conn.execute('UPDATE sessions SET duration_minutes=12.25')
    assert migrate(config, apply=True, old_clients_stopped=True)['migrated_sessions'] == 1
