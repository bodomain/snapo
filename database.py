"""Local session operations and serialized, explicit Turso synchronization.

Normal reads/writes never wait for the network. The worker syncs separately.
A missing credential never silently switches a configured replica to SQLite.
"""
from contextlib import contextmanager
from dataclasses import dataclass
from datetime import datetime
import fcntl
import json
import logging
import os
from pathlib import Path
import socket
import sqlite3
import uuid

from sessions import Session, SessionHistory

PROJECT = Path(__file__).resolve().parent
DB_NAME = PROJECT / "prodz.db"
TURSO_URL = "libsql://prodz-schwobotech.aws-eu-west-1.turso.io"
ENV_FILE = PROJECT / ".env"
SCHEMA_VERSION = "2"
REVISION_SCHEMA = """CREATE TABLE IF NOT EXISTS session_revisions (
    revision_id TEXT PRIMARY KEY,
    session_id TEXT NOT NULL,
    parents TEXT NOT NULL,
    device_id TEXT NOT NULL,
    created_at TEXT NOT NULL,
    payload TEXT NOT NULL,
    deleted INTEGER NOT NULL,
    legacy_id INTEGER
)"""


class MigrationRequired(RuntimeError):
    pass


def _env_values():
    values = {}
    if ENV_FILE.exists():
        for line in ENV_FILE.read_text().splitlines():
            line = line.strip()
            if not line or line.startswith("#"):
                continue
            if "=" in line:
                key, value = line.split("=", 1)
                values[key.strip()] = value.strip().strip("\"'")
            else:
                values["TURSO_AUTH_TOKEN"] = line.strip("\"'")
    return values


def _setting(name, default=None):
    return os.environ.get(name, _env_values().get(name, default))


def _load_turso_token():
    return _setting("TURSO_AUTH_TOKEN")


@dataclass(frozen=True)
class Config:
    path: Path
    remote_url: str | None = None
    token: str | None = None
    local: bool = False

    @classmethod
    def load(cls, path=None, local=False):
        return cls(Path(path or _setting("SNAPO_DB_PATH", str(DB_NAME))).expanduser().resolve(),
                   _setting("TURSO_DATABASE_URL", TURSO_URL), _load_turso_token(),
                   local or _setting("SNAPO_LOCAL", "0") == "1")

    def __post_init__(self):
        object.__setattr__(self, "path", Path(self.path).expanduser().resolve())


def _atomic_json(path, value):
    temp = path.with_name(path.name + ".tmp")
    with temp.open("w", encoding="utf-8") as handle:
        json.dump(value, handle)
        handle.flush()
        os.fsync(handle.fileno())
    temp.replace(path)


@contextmanager
def locked(path):
    path.parent.mkdir(parents=True, exist_ok=True)
    with Path(str(path) + ".lock").open("a") as handle:
        fcntl.flock(handle, fcntl.LOCK_EX)
        try:
            yield
        finally:
            fcntl.flock(handle, fcntl.LOCK_UN)


@contextmanager
def _database_lock():
    with locked(Config.load().path):
        yield


def open_connection(config):
    if config.local:
        if Path(str(config.path) + "-info").exists():
            raise RuntimeError("This is a Turso replica; --local cannot safely open it with SQLite")
        return sqlite3.connect(config.path), None
    if not config.token:
        raise RuntimeError("TURSO_AUTH_TOKEN is required. Use --local with a separate database for local-only use.")
    from turso.lib_sync import connect_sync
    # pyturso's HTTP transport uses urllib and honors the socket default.
    socket.setdefaulttimeout(5)
    try:
        conn = connect_sync(str(config.path), remote_url=config.remote_url,
                            auth_token=config.token, long_poll_timeout_ms=1000)
    except Exception as error:
        raise RuntimeError("Cannot open replica. First use requires an online bootstrap; "
                           "check credentials and connectivity. Existing files were retained.") from error
    return conn, conn


def create_schema(conn, dataset_id):
    conn.execute("CREATE TABLE IF NOT EXISTS snapo_config (name TEXT PRIMARY KEY, value TEXT NOT NULL)")
    conn.execute(REVISION_SCHEMA)
    conn.execute("CREATE INDEX IF NOT EXISTS session_revisions_session ON session_revisions(session_id)")
    for key, value in (("schema_version", SCHEMA_VERSION), ("dataset_id", dataset_id)):
        conn.execute("INSERT INTO snapo_config (name,value) VALUES (?,?)", (key, value))


def require_schema(conn):
    tables = {r[0] for r in conn.execute("SELECT name FROM sqlite_master WHERE type='table'").fetchall()}
    if "snapo_config" not in tables:
        raise MigrationRequired("This database needs migration. Stop old clients and run migrate_db.py first.")
    values = dict(conn.execute("SELECT name,value FROM snapo_config").fetchall())
    if values.get("schema_version") != SCHEMA_VERSION:
        raise MigrationRequired("Unsupported database version; upgrade Snapo before writing.")
    return values


class Store:
    def __init__(self, config=None):
        self.config = config or Config.load()
        self.status_path = Path(str(self.config.path) + ".sync.json")
        self.device_path = Path(str(self.config.path) + ".device.json")

    def _device_id(self):
        if not self.device_path.exists():
            _atomic_json(self.device_path, {"device_id": str(uuid.uuid4())})
        return json.loads(self.device_path.read_text())["device_id"]

    @contextmanager
    def _connection(self, *, check=True):
        with locked(self.config.path):
            conn, sync = open_connection(self.config)
            try:
                if check:
                    require_schema(conn)
                yield conn, sync
            except BaseException:
                conn.rollback()
                raise
            finally:
                conn.close()

    def initialize(self):
        with self._connection(check=False) as (conn, _):
            tables = {r[0] for r in conn.execute("SELECT name FROM sqlite_master WHERE type='table'").fetchall()}
            if "snapo_config" not in tables:
                if "sessions" in tables:
                    raise MigrationRequired("Legacy sessions found. Run migrate_db.py before using this version.")
                conn.execute("BEGIN IMMEDIATE")
                create_schema(conn, str(uuid.uuid4()))
                conn.commit()
            require_schema(conn)
            self._device_id()

    def entries(self):
        with self._connection() as (conn, _):
            return SessionHistory(conn, self._device_id()).entries()

    def _mark_saved(self, sync):
        # A status-file failure must never turn a committed save into an apparent
        # failure that encourages the user to submit the same record again.
        try:
            status = self.status()
            status.update(mode="local" if sync is None else "remote",
                          pending_operations=0 if sync is None else sync.stats().cdc_operations,
                          last_local_save=datetime.now().astimezone().isoformat())
            _atomic_json(self.status_path, status)
        except Exception:
            logging.warning("Session saved, but sync status could not be updated")

    def create(self, session, *, session_id=None):
        with self._connection() as (conn, sync):
            history = SessionHistory(conn, self._device_id())
            if session_id:
                matches = [e for e in history.entries() if e.session_id == session_id]
                if matches:
                    # Timer retries are idempotent, even after a status-write failure.
                    roots = [r for r in history.revisions() if r.session_id == session_id and not r.parents]
                    if len(roots) == 1 and roots[0].session == session.validated():
                        return roots[0]
                    raise ValueError("Session ID already has different contents")
            revision = history.create(session, session_id=session_id)
            conn.commit()
            self._mark_saved(sync)
            return revision

    def change(self, session_id, base_heads, **changes):
        with self._connection() as (conn, sync):
            revision = SessionHistory(conn, self._device_id()).change(session_id, base_heads, **changes)
            conn.commit()
            self._mark_saved(sync)
            return revision

    def status(self):
        try:
            status = json.loads(self.status_path.read_text())
            if not isinstance(status, dict):
                raise ValueError("Invalid sync status")
            return status
        except (OSError, ValueError):
            return {"mode": "local" if self.config.local else "remote", "pending_operations": None,
                    "last_success": None, "error": None}

    def sync(self):
        with locked(self.config.path):
            status = self.status()
            conn = sync = None
            try:
                conn, sync = open_connection(self.config)
                require_schema(conn)
                if sync is None:
                    status.update(mode="local", pending_operations=0, error=None)
                else:
                    if sync.stats().cdc_operations:
                        sync.push()
                    sync.pull()
                    require_schema(conn)
                    status.update(mode="remote", pending_operations=sync.stats().cdc_operations,
                                  last_success=datetime.now().astimezone().isoformat(), error=None)
                _atomic_json(self.status_path, status)
                return status
            except Exception as error:
                message = str(error)
                if self.config.token:
                    message = message.replace(self.config.token, "<REDACTED>")
                status["error"] = message
                if sync is not None:
                    try:
                        status["pending_operations"] = sync.stats().cdc_operations
                    except Exception:
                        pass  # Retain the last known count if the engine cannot answer.
                try:
                    _atomic_json(self.status_path, status)
                except OSError:
                    logging.warning("Could not persist sync failure status")
                raise
            finally:
                if conn is not None:
                    conn.close()


# Compatibility adapters: callers now use revision history exclusively.
def init_db():
    Store().initialize()


def log_session(activity, duration, comment=""):
    store = Store()
    revision = store.create(Session.now(activity, duration, comment))
    print(f"Saved session for '{activity}' locally; background sync will upload it.")
    return revision


def get_sessions():
    return [(e.current.session.date, e.current.session.activity, e.current.session.duration_minutes)
            for e in Store().entries() if not e.conflicted and not e.current.deleted]


def sync_once():
    return Store().sync()
