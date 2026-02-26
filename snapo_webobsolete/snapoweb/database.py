import sqlite3
import datetime

DB_NAME = "prodz.db"


def init_db():
    """Initializes the database and creates the table if it doesn't exist."""
    conn = sqlite3.connect(DB_NAME)
    cursor = conn.cursor()
    cursor.execute("""
        CREATE TABLE IF NOT EXISTS sessions (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            date TEXT NOT NULL,
            timestamp TEXT NOT NULL,
            activity TEXT NOT NULL,
            duration_minutes INTEGER NOT NULL
        )
    """)
    conn.commit()
    conn.close()


def log_session(activity, duration):
    """Logs a completed work session to the database."""
    conn = sqlite3.connect(DB_NAME)
    cursor = conn.cursor()

    now = datetime.datetime.now()
    date_str = now.strftime("%Y-%m-%d")
    timestamp_str = now.isoformat()

    cursor.execute(
        """
        INSERT INTO sessions (date, timestamp, activity, duration_minutes)
        VALUES (?, ?, ?, ?)
    """,
        (date_str, timestamp_str, activity, duration),
    )

    conn.commit()
    conn.close()
    print(f"Logged session for '{activity}' to database.")


def merge_db(foreign_db_path):
    """Merges entries from another database file into the current one."""
    conn = sqlite3.connect(DB_NAME)
    cursor = conn.cursor()

    try:
        # Attach the foreign database
        cursor.execute(f"ATTACH DATABASE ? AS foreign_db", (foreign_db_path,))

        # Insert records that don't exist in the current database
        # We use timestamp as the unique identifier for a session
        cursor.execute("""
            INSERT INTO sessions (date, timestamp, activity, duration_minutes)
            SELECT date, timestamp, activity, duration_minutes
            FROM foreign_db.sessions
            WHERE timestamp NOT IN (SELECT timestamp FROM sessions)
        """)

        inserted_count = cursor.rowcount
        conn.commit()
        print(
            f"Successfully merged {inserted_count} new sessions from '{foreign_db_path}'."
        )
    except sqlite3.Error as e:
        print(f"Error merging databases: {e}")
    finally:
        conn.close()
