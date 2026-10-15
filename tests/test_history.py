import csv
import sqlite3

import pytest

from database import Config, Store, create_schema
from sessions import ConflictError, Revision, Session, SessionHistory, export_csv, filtered, totals


@pytest.fixture
def store(tmp_path):
    result = Store(Config(tmp_path / 'test.db', local=True))
    result.initialize()
    return result


def sample(activity='Study', minutes=25):
    return Session('2026-10-15', '2026-10-15T09:00:00+02:00', activity, minutes, 'notes')


def test_edit_trash_restore_and_totals(store):
    r = store.create(sample())
    e = store.entries()[0]
    store.change(r.session_id, e.head_ids, session=sample('Python', 12.5))
    e = store.entries()[0]
    assert totals([e])[0] == {('2026-10-15', 'Python'): 12.5}
    store.change(e.session_id, e.head_ids, deleted=True)
    assert not filtered(store.entries())
    assert len(filtered(store.entries(), view='trash')) == 1
    assert totals(store.entries()) == ({}, {})
    e = store.entries()[0]
    store.change(e.session_id, e.head_ids, deleted=False)
    assert len(filtered(store.entries())) == 1
    assert len(store.entries()[0].heads) == 1


def test_stale_editor_cannot_consume_newer_edit(store):
    store.create(sample())
    base = store.entries()[0]
    store.change(base.session_id, base.head_ids, session=sample('First edit'))
    with pytest.raises(ConflictError):
        store.change(base.session_id, base.head_ids, session=sample('Stale edit'))
    assert store.entries()[0].current.session.activity == 'First edit'


def test_idempotent_timer_save(store):
    a = store.create(sample(), session_id='stable-timer-id')
    b = store.create(sample(), session_id='stable-timer-id')
    assert a == b
    assert len(store.entries()) == 1
    with pytest.raises(ValueError):
        store.create(sample('different'), session_id='stable-timer-id')


def make_history():
    conn = sqlite3.connect(':memory:')
    create_schema(conn, 'dataset')
    return SessionHistory(conn, 'device')


def exchange(a, b, reverse=False):
    ar, br = a.revisions(), b.revisions()
    for revision in reversed(br) if reverse else br:
        a.append(revision)
    for revision in reversed(ar) if reverse else ar:
        b.append(revision)


@pytest.mark.parametrize('reverse', [False, True])
def test_offline_edits_delete_conflict_resolve_and_restore(reverse):
    a, b = make_history(), make_history()
    root = a.create(sample())
    exchange(a, b)
    base_a, base_b = a.get(root.session_id), b.get(root.session_id)
    a.change(root.session_id, base_a.head_ids, session=sample('Offline edit'))
    b.change(root.session_id, base_b.head_ids, deleted=True)
    exchange(a, b, reverse)
    assert a.entries() == b.entries()
    assert a.entries()[0].conflicted
    assert totals(a.entries()) == ({}, {})
    with pytest.raises(ConflictError):
        a.change(root.session_id, a.get(root.session_id).head_ids, deleted=False)
    conflict = a.get(root.session_id)
    a.change(root.session_id, conflict.head_ids, session=sample('Chosen'), deleted=True, resolve=True)
    exchange(a, b, reverse)
    assert not b.entries()[0].conflicted
    assert b.entries()[0].current.deleted
    b.change(root.session_id, b.get(root.session_id).head_ids, deleted=False)
    exchange(a, b, reverse)
    assert a.entries() == b.entries()
    assert not a.entries()[0].current.deleted
    # Repeated delivery never creates duplicates or revives old heads.
    before = a.revisions()
    exchange(a, b, reverse)
    assert a.revisions() == before


def test_concurrent_resolutions_remain_reviewable():
    a, b = make_history(), make_history()
    r = a.create(sample())
    exchange(a, b)
    a.change(r.session_id, (r.revision_id,), session=sample('A'))
    b.change(r.session_id, (r.revision_id,), session=sample('B'))
    exchange(a, b)
    for h, activity in ((a, 'Resolution A'), (b, 'Resolution B')):
        h.change(r.session_id, h.get(r.session_id).head_ids, session=sample(activity), deleted=False, resolve=True)
    exchange(a, b)
    assert a.entries()[0].conflicted
    assert {r.session.activity for r in a.entries()[0].heads} == {'Resolution A', 'Resolution B'}


def test_independent_offline_creates_do_not_collide():
    a, b = make_history(), make_history()
    a.create(sample())
    b.create(sample())
    exchange(a, b)
    assert len(a.entries()) == 2
    assert a.entries() == b.entries()


def test_filters_exports_and_validation(store, tmp_path):
    store.create(sample('=SUM(A1)', 1.25))
    store.create(sample('Other', 2))
    entries = filtered(store.entries(), search='sum', since='2026-10-01', until='2026-10-31')
    assert len(entries) == 1
    target = tmp_path / 'report.csv'
    export_csv(entries, target)
    with target.open() as handle:
        rows = list(csv.DictReader(handle))
    assert rows[0]['activity'] == "'=SUM(A1)"
    assert rows[0]['duration_minutes'] == '1.25'
    with pytest.raises(FileExistsError):
        export_csv(entries, target)
    for minutes in (0, -1, float('nan'), float('inf')):
        with pytest.raises(ValueError):
            store.create(sample(minutes=minutes))
    with pytest.raises(ValueError):
        filtered(store.entries(), since='2026-10-20', until='2026-10-01')


def test_replica_cannot_be_opened_as_plain_sqlite(tmp_path):
    config = Config(tmp_path / 'replica.db', local=True)
    (tmp_path / 'replica.db-info').write_text('{}')
    with pytest.raises(RuntimeError, match='Turso replica'):
        Store(config).initialize()


def test_status_write_failure_does_not_report_failed_save(store, monkeypatch):
    import database
    def failed_status(*args):
        raise OSError('disk status write failed')
    monkeypatch.setattr(database, '_atomic_json', failed_status)
    r = store.create(sample())
    assert store.entries()[0].session_id == r.session_id
    e = store.entries()[0]
    store.change(e.session_id, e.head_ids, deleted=True)
    assert store.entries()[0].current.deleted


def test_malformed_status_does_not_break_history(store):
    store.status_path.write_text('incomplete')
    assert store.status()['pending_operations'] is None
    store.create(sample())
    assert len(store.entries()) == 1


def test_background_connection_error_is_persisted_and_token_redacted(store, monkeypatch):
    import database
    from dataclasses import replace
    store.config = replace(store.config, token='secret-test-token')
    def unavailable(config):
        raise RuntimeError('connection failed: secret-test-token')
    monkeypatch.setattr(database, 'open_connection', unavailable)
    with pytest.raises(RuntimeError):
        store.sync()
    assert store.status()['error'] == 'connection failed: <REDACTED>'
