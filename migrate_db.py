"""Back up and migrate legacy sessions on one designated computer.

Check first, stop old clients on every computer, then apply. Remote migration
compares a separately bootstrapped replica before making any changes.
"""
import argparse
from datetime import datetime
import json
from pathlib import Path
import shutil
import tempfile
import uuid

from database import (Config, SCHEMA_VERSION, SessionHistory, create_schema, locked,
                      open_connection, require_schema)
from sessions import Revision, Session


def snapshot(path):
    target = path.parent / "backups" / ("migration-" + datetime.now().strftime("%Y%m%d-%H%M%S-%f"))
    target.mkdir(parents=True, mode=0o700)
    for source in path.parent.glob(path.name + "*"):
        if source.is_file() and source.name != path.name + ".lock":
            shutil.copy2(source, target / source.name)
    return target


def legacy_rows(conn):
    tables = {r[0] for r in conn.execute("SELECT name FROM sqlite_master WHERE type='table'").fetchall()}
    if "sessions" not in tables:
        return []
    columns = {r[1] for r in conn.execute("PRAGMA table_info(sessions)").fetchall()}
    comment = "comment" if "comment" in columns else "NULL"
    return conn.execute(f"SELECT id,date,timestamp,activity,duration_minutes,{comment} FROM sessions ORDER BY id").fetchall()


def verify_remote(conn, sync, config):
    if sync.stats().cdc_operations:
        sync.push()
    sync.pull()
    with tempfile.TemporaryDirectory(prefix="snapo-migration-verify-") as directory:
        check, _ = open_connection(Config(Path(directory) / "verify.db", config.remote_url, config.token))
        try:
            local_settings = require_schema(conn)
            remote_settings = require_schema(check)
            if local_settings != remote_settings:
                raise RuntimeError("Remote dataset configuration differs after migration")
            expected = {r.revision_id: r for r in SessionHistory(conn, "verification").revisions()}
            actual = {r.revision_id: r for r in SessionHistory(check, "verification").revisions()}
            if any(actual.get(key) != revision for key, revision in expected.items()):
                raise RuntimeError("Remote revision verification failed; keep the backup and retry migration")
        finally:
            check.close()


def migrate(config, *, apply=False, old_clients_stopped=False, dataset_id=None):
    if apply and not old_clients_stopped:
        raise ValueError("Stop old clients on ALL computers, then pass --old-clients-stopped")
    if dataset_id:
        dataset_id = str(uuid.UUID(dataset_id))
    with locked(config.path):
        if not config.path.exists():
            raise ValueError("Source database does not exist; bootstrap a new replica with app.py instead")
        backup = snapshot(config.path) if apply else None
        conn, sync = open_connection(config)
        remote = None
        temp = None
        try:
            tables = {r[0] for r in conn.execute("SELECT name FROM sqlite_master WHERE type='table'").fetchall()}
            if "snapo_config" in tables:
                settings = require_schema(conn)
                if dataset_id and dataset_id != settings["dataset_id"]:
                    raise ValueError("Dataset namespace does not match the migrated database")
                if apply and sync:
                    verify_remote(conn, sync, config)
                return dict(already_migrated=True, dataset_id=settings["dataset_id"], backup=str(backup) if backup else None)
            local_rows = legacy_rows(conn)
            remote_rows = []
            if sync:
                temp = tempfile.TemporaryDirectory(prefix="snapo-migration-check-")
                remote, _ = open_connection(Config(Path(temp.name) / "remote.db", config.remote_url, config.token))
                remote_tables = {r[0] for r in remote.execute("SELECT name FROM sqlite_master WHERE type='table'").fetchall()}
                if "snapo_config" in remote_tables:
                    raise ValueError("Remote is already migrated. Reconcile this old replica before bootstrapping a fresh one; do not migrate it independently.")
                remote_rows = legacy_rows(remote)
                local_by_id = {r[0]:r for r in local_rows}
                remote_by_id = {r[0]:r for r in remote_rows}
                conflicts = [key for key in local_by_id.keys() & remote_by_id.keys()
                             if local_by_id[key] != remote_by_id[key]]
                if conflicts:
                    raise ValueError(f"Legacy records differ for IDs {conflicts}. Reconcile them before migration; nothing was overwritten.")
                # Preserve remote metadata beyond the session table too.
                if "metadata" in tables and "metadata" in remote_tables:
                    if sorted(conn.execute('SELECT * FROM metadata').fetchall()) != sorted(remote.execute('SELECT * FROM metadata').fetchall()):
                        raise ValueError("Local and remote metadata differ; reconcile before migration")
            merged = {r[0]: r for r in remote_rows}
            merged.update({r[0]: r for r in local_rows})
            result = dict(already_migrated=False, local_sessions=len(local_rows), remote_sessions=len(remote_rows),
                          merged_sessions=len(merged), local_only=len(set(r[0] for r in local_rows)-set(r[0] for r in remote_rows)) if sync else 0,
                          backup=str(backup) if backup else None, schema_version=SCHEMA_VERSION)
            if not apply:
                return result
            # First reconcile legacy local-only rows through the independent replica.
            # No old-session writes happen after schema migration.
            if remote:
                remote_ids = {r[0] for r in remote_rows}
                for key, row in merged.items():
                    if key not in remote_ids:
                        remote.execute("INSERT INTO sessions (id,date,timestamp,activity,duration_minutes,comment) VALUES (?,?,?,?,?,?)", row)
                remote.commit()
                remote.push()
                if sync.stats().cdc_operations:
                    sync.push()
                sync.pull()
                if legacy_rows(conn) != sorted(merged.values()):
                    raise ValueError("Legacy reconciliation did not converge; original files retained in backup")
            namespace = dataset_id or str(uuid.uuid4())
            conn.execute("BEGIN IMMEDIATE")
            create_schema(conn, namespace)
            history = SessionHistory(conn, "migration")
            for legacy_id, day, timestamp, activity, duration, comment in sorted(merged.values()):
                session = Session(day, timestamp, activity, float(duration), comment or "")
                # Preserve legacy data exactly; validation applies to future edits.
                sid = str(uuid.uuid5(uuid.UUID(namespace), f"session:{legacy_id}"))
                rid = str(uuid.uuid5(uuid.UUID(namespace), f"revision:{legacy_id}"))
                history.append(Revision(rid, sid, (), "migration", timestamp, session, False, legacy_id))
            conn.commit()
            if sync:
                verify_remote(conn, sync, config)
            result.update(dataset_id=namespace, migrated_sessions=len(merged))
            return result
        except BaseException:
            conn.rollback()
            raise
        finally:
            if remote:
                remote.close()
            conn.close()
            if temp:
                temp.cleanup()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--db", type=Path)
    parser.add_argument("--local", action="store_true")
    parser.add_argument("--apply", action="store_true", help="Apply migration; default only checks")
    parser.add_argument("--old-clients-stopped", action="store_true")
    parser.add_argument("--dataset-id", help="Optional shared UUID namespace; generated once if omitted")
    args = parser.parse_args()
    if args.local and args.db is None:
        parser.error("--local requires --db")
    try:
        result = migrate(Config.load(args.db, args.local), apply=args.apply,
                         old_clients_stopped=args.old_clients_stopped, dataset_id=args.dataset_id)
    except Exception as error:
        parser.exit(1, f"Migration stopped: {error}\n")
    print(json.dumps(result, indent=2))


if __name__ == "__main__":
    main()
