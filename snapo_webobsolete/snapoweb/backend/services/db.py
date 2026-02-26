import sqlite3
import datetime
import os
from pathlib import Path

# Use absolute path for safety, locating it in the original folder or backend folder
# We will locate it in the directory above backend so its shared if needed, 
# or just keep it in backend. Let's place it in /home/user/Desktop/snapo_dev/snapo/prodz.db
DB_NAME = os.path.join(os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__)))), "prodz.db")

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
            duration_minutes REAL NOT NULL
        )
    """)
    conn.commit()
    conn.close()

def log_session(activity: str, duration: float):
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
    return {"status": "success", "message": f"Logged session for '{activity}'"}

def merge_db(foreign_db_path: str):
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
        return {"status": "success", "inserted_count": inserted_count}
    except sqlite3.Error as e:
        return {"status": "error", "message": str(e)}
    finally:
        conn.close()
