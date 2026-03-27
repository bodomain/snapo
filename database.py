import sqlite3
import datetime

DB_NAME = "prodz.db"

def init_db():
    """Initializes the database and creates the table if it doesn't exist."""
    conn = sqlite3.connect(DB_NAME)
    cursor = conn.cursor()
    cursor.execute('''
        CREATE TABLE IF NOT EXISTS sessions (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            date TEXT NOT NULL,
            timestamp TEXT NOT NULL,
            activity TEXT NOT NULL,
            duration_minutes INTEGER NOT NULL,
            comment TEXT
        )
    ''')
    try:
        cursor.execute('ALTER TABLE sessions ADD COLUMN comment TEXT')
    except sqlite3.OperationalError:
        pass
    conn.commit()
    conn.close()

def log_session(activity, duration, comment=""):
    """Logs a completed work session to the database."""
    conn = sqlite3.connect(DB_NAME)
    cursor = conn.cursor()
    
    now = datetime.datetime.now()
    date_str = now.strftime("%Y-%m-%d")
    timestamp_str = now.isoformat()
    
    cursor.execute('''
        INSERT INTO sessions (date, timestamp, activity, duration_minutes, comment)
        VALUES (?, ?, ?, ?, ?)
    ''', (date_str, timestamp_str, activity, duration, comment))
    
    conn.commit()
    conn.close()
    print(f"Logged session for '{activity}' to database.")
