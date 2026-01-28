import sqlite3
import shutil

DB_NAME = "pomodoro.db"

def get_daily_stats():
    conn = sqlite3.connect(DB_NAME)
    cursor = conn.cursor()
    cursor.execute('''
        SELECT date, SUM(duration_minutes) 
        FROM sessions 
        GROUP BY date 
        ORDER BY date ASC
    ''')
    data = cursor.fetchall()
    conn.close()
    return data

def draw_chart(data):
    if not data:
        print("No data found in database.")
        return

    # Terminal dimensions
    cols, _ = shutil.get_terminal_size((80, 20))
    max_label_len = max(len(d[0]) for d in data)
    max_val = max(d[1] for d in data)
    
    # Available width for the bar (leaving space for label and value)
    bar_width = cols - max_label_len - 15 

    print("\nDaily Activity (Duration in Minutes)\n")

    for date, duration in data:
        # Calculate bar length
        scaled_len = int((duration / max_val) * bar_width) if max_val > 0 else 0
        bar = "█" * scaled_len
        print(f"{date} | {bar} {duration:.2f}m")

if __name__ == "__main__":
    try:
        stats = get_daily_stats()
        draw_chart(stats)
    except sqlite3.OperationalError:
        print("Database not found or empty. Run a pomodoro session first.")
