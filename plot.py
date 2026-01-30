import sqlite3
import shutil

DB_NAME = "prodz.db"

# ANSI Colors for terminal output
COLORS = [
    "\033[31m", # Red
    "\033[32m", # Green
    "\033[33m", # Yellow
    "\033[34m", # Blue
    "\033[35m", # Magenta
    "\033[36m", # Cyan
    "\033[91m", # Light Red
    "\033[92m", # Light Green
    "\033[93m", # Light Yellow
    "\033[94m", # Light Blue
    "\033[95m", # Light Magenta
    "\033[96m", # Light Cyan
]
RESET = "\033[0m"

def get_data():
    conn = sqlite3.connect(DB_NAME)
    cursor = conn.cursor()
    cursor.execute('''
        SELECT date, activity, SUM(duration_minutes) 
        FROM sessions 
        GROUP BY date, activity 
        ORDER BY date ASC
    ''')
    data = cursor.fetchall()
    conn.close()
    return data

def draw_chart(data):
    if not data:
        print("No data found in database.")
        return

    # Organize data by date
    daily_stats = {}
    activities = set()
    for date, activity, duration in data:
        if date not in daily_stats:
            daily_stats[date] = {}
        daily_stats[date][activity] = duration
        activities.add(activity)
    
    # Assign colors to activities
    sorted_activities = sorted(list(activities))
    activity_colors = {act: COLORS[i % len(COLORS)] for i, act in enumerate(sorted_activities)}

    # Calculate max daily total for scaling
    max_val = 0
    for date, acts in daily_stats.items():
        total = sum(acts.values())
        if total > max_val:
            max_val = total

    if max_val == 0:
        return

    # Terminal dimensions
    cols, _ = shutil.get_terminal_size((80, 20))
    max_label_len = max(len(d) for d in daily_stats.keys())
    # Available width (date + separator + space + total text + extra buffer)
    bar_width = cols - max_label_len - 15 

    print("\nDaily Activity (Duration in Minutes)\n")

    for date, acts in daily_stats.items():
        total_duration = sum(acts.values())
        
        # Build the stacked bar
        bar_str = ""
        
        # Sort activities by duration for better visualization (largest first) or name? 
        # Keeping them consistent (by name) usually looks cleaner in stacks.
        for activity in sorted_activities:
            duration = acts.get(activity, 0)
            if duration > 0:
                segment_len = int((duration / max_val) * bar_width)
                # Ensure at least one block if there is significant duration but < 1 char
                if segment_len == 0 and duration > 0:
                     segment_len = 1
                
                color = activity_colors[activity]
                bar_str += f"{color}{'█' * segment_len}{RESET}"
        
        print(f"{date} | {bar_str} {total_duration:.2f}m")

    # Legend
    print("\nLegend:")
    for activity in sorted_activities:
        print(f"{activity_colors[activity]}█ {activity}{RESET}", end="  ")
    print("\n")

if __name__ == "__main__":
    try:
        data = get_data()
        draw_chart(data)
    except sqlite3.OperationalError:
        print("Database not found or empty. Run a prodzCLI session first.")