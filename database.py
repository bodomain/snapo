import datetime
import fcntl
import os
import sqlite3
from contextlib import contextmanager
from functools import wraps
from pathlib import Path

DB_NAME = Path(__file__).with_name("prodz.db")
TURSO_URL = "libsql://prodz-schwobotech.aws-eu-west-1.turso.io"
ENV_FILE = Path(__file__).with_name(".env")


@contextmanager
def _database_lock():
    """Serialize replica access across the timer, charts, and sync worker."""
    with Path(str(DB_NAME) + ".lock").open("a") as handle:
        fcntl.flock(handle, fcntl.LOCK_EX)
        try:
            yield
        finally:
            fcntl.flock(handle, fcntl.LOCK_UN)


def _locked(operation):
    @wraps(operation)
    def run(*args, **kwargs):
        with _database_lock():
            return operation(*args, **kwargs)
    return run


def _load_turso_token():
    """Load a token from the environment or the project's .env file."""
    token = os.environ.get("TURSO_AUTH_TOKEN")
    if token:
        return token

    if not ENV_FILE.exists():
        return None

    for line in ENV_FILE.read_text().splitlines():
        line = line.strip()
        if not line or line.startswith("#"):
            continue
        if "=" in line:
            key, value = line.split("=", 1)
            if key.strip() == "TURSO_AUTH_TOKEN":
                return value.strip().strip("\"'")
        else:
            # Existing .env files may contain only the Turso token.
            return line.strip("\"'")
    return None


def _sync_connection():
    """Open a local SQLite-compatible connection with Turso sync when configured."""
    token = _load_turso_token()
    if not token:
        return sqlite3.connect(DB_NAME), None

    try:
        from turso.lib_sync import connect_sync
    except ImportError as error:
        raise RuntimeError(
            "Turso sync requires pyturso. Run with .venv/bin/python or install "
            "requirements.txt in your Python environment."
        ) from error

    try:
        connection = connect_sync(
            str(DB_NAME), remote_url=TURSO_URL, auth_token=token,
            long_poll_timeout_ms=1000,
        )
    except Exception as error:
        # SQLite writes bypass Turso's change tracking and cannot be queued for sync.
        raise RuntimeError(
            "Could not open the Turso replica. An initial online connection is "
            "required before offline use."
        ) from error
    return connection, connection


def _pull(sync_handle):
    if sync_handle is not None:
        try:
            sync_handle.pull()
        except Exception as error:
            print(f"Turso sync unavailable; using local database: {error}")


def _push(sync_handle):
    if sync_handle is not None:
        try:
            sync_handle.push()
        except Exception as error:
            print(f"Could not sync changes to Turso; they remain local: {error}")


@_locked
def init_db():
    """Initialize the local database and pull remote changes when online."""
    conn, sync_handle = _sync_connection()
    try:
        _pull(sync_handle)
        _push(sync_handle)
        cursor = conn.cursor()
        cursor.execute(
            """
            CREATE TABLE IF NOT EXISTS sessions (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                date TEXT NOT NULL,
                timestamp TEXT NOT NULL,
                activity TEXT NOT NULL,
                duration_minutes INTEGER NOT NULL,
                comment TEXT
            )
            """
        )
        columns = {row[1] for row in cursor.execute("PRAGMA table_info(sessions)").fetchall()}
        if "comment" not in columns:
            cursor.execute("ALTER TABLE sessions ADD COLUMN comment TEXT")
        conn.commit()
        _push(sync_handle)
    finally:
        conn.close()


@_locked
def log_session(activity, duration, comment=""):
    """Log a completed work session locally and sync it to Turso when online."""
    conn, sync_handle = _sync_connection()
    try:
        _pull(sync_handle)
        _push(sync_handle)
        now = datetime.datetime.now()
        conn.execute(
            """
            INSERT INTO sessions (date, timestamp, activity, duration_minutes, comment)
            VALUES (?, ?, ?, ?, ?)
            """,
            (now.strftime("%Y-%m-%d"), now.isoformat(), activity, duration, comment),
        )
        conn.commit()
        _push(sync_handle)
    finally:
        conn.close()
    print(f"Logged session for '{activity}' to database.")


@_locked
def get_sessions():
    """Return session rows after pulling any available changes from Turso."""
    conn, sync_handle = _sync_connection()
    try:
        _pull(sync_handle)
        _push(sync_handle)
        return conn.execute(
            "SELECT date, activity, duration_minutes FROM sessions ORDER BY date ASC"
        ).fetchall()
    finally:
        conn.close()


@_locked
def sync_once():
    """Upload queued changes and refresh the local replica, raising on failure."""
    if not _load_turso_token():
        raise RuntimeError("TURSO_AUTH_TOKEN is missing from the environment or .env")
    conn, sync_handle = _sync_connection()
    try:
        if sync_handle.stats().cdc_operations:
            sync_handle.push()
        sync_handle.pull()
    finally:
        conn.close()
