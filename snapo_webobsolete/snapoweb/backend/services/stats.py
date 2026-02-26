import sqlite3
import os

DB_NAME = os.path.join(os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__)))), "prodz.db")

def get_stats_data():
    if not os.path.exists(DB_NAME):
        return []

    conn = sqlite3.connect(DB_NAME)
    cursor = conn.cursor()
    # Fetch individual sessions
    try:
        cursor.execute('''
            SELECT date, activity, duration_minutes 
            FROM sessions 
            ORDER BY date ASC
        ''')
        data = cursor.fetchall()
    except sqlite3.OperationalError:
        data = []
    finally:
        conn.close()

    # Format data for the frontend (JSON friendly)
    # Return a list of records: [{"date": "2023-10-27", "activity": "coding", "duration_minutes": 25}, ...]
    formatted_data = []
    for date, activity, duration in data:
        formatted_data.append({
            "date": date,
            "activity": activity,
            "duration_minutes": duration
        })
    
    return formatted_data
