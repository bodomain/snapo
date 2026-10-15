"""Driver compatibility plus opt-in cloud transport acceptance tests.

The remote test requires a separate DISPOSABLE database and explicit opt-in.
It leaves its test history in that database for inspection; delete the database
through your Turso account afterward. It never uses the project's .env token.
"""
import os
from pathlib import Path
import urllib.error
from unittest.mock import patch

import pytest
import turso

import database
from database import Config, Store
from sessions import Session


def test_store_with_real_turso_engine(tmp_path, monkeypatch):
    def connect(config):
        return turso.connect(str(config.path)), None
    monkeypatch.setattr(database, 'open_connection', connect)
    store = Store(Config(tmp_path / 'turso.db', local=True))
    store.initialize()
    root = store.create(Session.now('Turso', 2.5))
    e = store.entries()[0]
    store.change(root.session_id, e.head_ids, deleted=True)
    assert store.entries()[0].current.deleted
    e = store.entries()[0]
    store.change(root.session_id, e.head_ids, deleted=False)
    assert not Store(store.config).entries()[0].current.deleted


@pytest.mark.remote
@pytest.mark.parametrize('first_device', ['a', 'b'])
def test_two_cloud_replicas_offline_conflict_restore_and_restart(tmp_path, first_device):
    url = os.environ.get('SNAPO_TEST_TURSO_URL')
    token = os.environ.get('SNAPO_TEST_TURSO_TOKEN')
    if os.environ.get('SNAPO_TEST_REMOTE_OK') != '1' or not url or not token:
        pytest.skip('Set SNAPO_TEST_REMOTE_OK=1 and disposable test database URL/token to exercise cloud transport')
    if url.replace('libsql://', 'https://') == database.TURSO_URL.replace('libsql://', 'https://'):
        pytest.fail('Refusing to use the production database for remote tests')
    # Unique event/session IDs isolate each scenario within the disposable store.
    a = Store(Config(tmp_path / 'a.db', url, token))
    a.initialize()
    a.sync()
    b = Store(Config(tmp_path / 'b.db', url, token))
    b.initialize()
    root = a.create(Session.now('Cloud test', 5))
    a.sync()
    b.sync()
    root_a = next(e for e in a.entries() if e.session_id == root.session_id)
    root_b = next(e for e in b.entries() if e.session_id == root.session_id)
    with patch('urllib.request.urlopen', side_effect=urllib.error.URLError('simulated offline')):
        a.change(root.session_id, root_a.head_ids, session=Session.now('Offline A', 10))
        b.change(root.session_id, root_b.head_ids, deleted=True)
        with pytest.raises(Exception):
            a.sync()
    stores = (a, b) if first_device == 'a' else (b, a)
    for store in (*stores, *stores):
        store.sync()
    conflict_a = next(e for e in a.entries() if e.session_id == root.session_id)
    conflict_b = next(e for e in b.entries() if e.session_id == root.session_id)
    assert conflict_a == conflict_b
    assert conflict_a.conflicted
    a.change(root.session_id, conflict_a.head_ids, session=Session.now('Resolved', 10), deleted=True, resolve=True)
    a.sync()
    b.sync()
    e = next(e for e in b.entries() if e.session_id == root.session_id)
    assert e.current.deleted
    b.change(root.session_id, e.head_ids, deleted=False)
    b.sync()
    a.sync()
    # New Store instances reopen the complete replica files, including sidecars.
    ar = next(e for e in Store(a.config).entries() if e.session_id == root.session_id)
    br = next(e for e in Store(b.config).entries() if e.session_id == root.session_id)
    assert ar == br
    assert not ar.current.deleted
    assert a.status()['pending_operations'] == 0
    assert b.status()['pending_operations'] == 0


def test_migration_with_real_turso_engine(tmp_path, monkeypatch):
    from migrate_db import migrate
    path = tmp_path / 'legacy-turso.db'
    conn = turso.connect(str(path))
    conn.execute('CREATE TABLE sessions (id INTEGER PRIMARY KEY, date TEXT, timestamp TEXT, activity TEXT, duration_minutes REAL, comment TEXT)')
    conn.execute('INSERT INTO sessions VALUES (1,?,?,?,?,?)',
                 ('2026-10-15', '2026-10-15T09:00:00', 'Preserved', 12.5, 'note'))
    conn.commit()
    conn.close()
    monkeypatch.setattr(database, 'open_connection', lambda config: (turso.connect(str(config.path)), None))
    config = Config(path, local=True)
    assert migrate(config, apply=True, old_clients_stopped=True)['migrated_sessions'] == 1
    store = Store(config)
    store.initialize()
    assert store.entries()[0].current.session.activity == 'Preserved'
    assert store.entries()[0].current.session.duration_minutes == 12.5
