import shutil
import database
from rich.text import Text


CHART_COLORS = ("red", "green", "yellow", "blue", "magenta", "cyan",
                "bright_red", "bright_green", "bright_yellow", "bright_blue",
                "bright_magenta", "bright_cyan")


def activity_chart(data, width):
    """Render session-level daily bars within the available terminal width."""
    chart = Text("DAILY ACTIVITY · minutes\n\n", style="bold")
    days = {}
    for day, activity, duration in data:
        if duration > 0:
            days.setdefault(day, []).append((activity, duration))
    if not days:
        chart.append("No activity for these filters.\n", style="dim")
        return chart
    activities = sorted({activity for sessions in days.values() for activity, _ in sessions})
    colors = {activity: CHART_COLORS[i % len(CHART_COLORS)]
              for i, activity in enumerate(activities)}
    chart.append("Legend: ", style="default")
    for activity in activities:
        chart.append("█ " + activity + "  ", style=colors[activity])
    chart.append("\n\n", style="default")
    maximum = max(sum(duration for _, duration in sessions) for sessions in days.values())
    label_width = max(len(day) for day in days)
    total_width = max(len(f"{sum(d for _, d in sessions):.2f}m") for sessions in days.values())
    available = max(0, width - label_width - total_width - 4)
    for day, sessions in sorted(days.items(), reverse=True):
        sessions = sorted(sessions, key=lambda item: item[0])
        total = sum(duration for _, duration in sessions)
        target = min(available, max(1, round(total / maximum * available)))
        separators = min(len(sessions) - 1, max(0, target - len(sessions)))
        budget = target - separators
        minimum = 1 if budget >= len(sessions) else 0
        remaining = budget - minimum * len(sessions)
        shares = [duration / total * remaining for _, duration in sessions]
        lengths = [minimum + int(share) for share in shares]
        for i in sorted(range(len(shares)), key=lambda i: shares[i] - int(shares[i]), reverse=True)[:budget - sum(lengths)]:
            lengths[i] += 1
        chart.append(f"{day:<{label_width}} | ", style="default")
        drawn = False
        for (activity, _), length in zip(sessions, lengths):
            if not length:
                continue
            if drawn and separators:
                chart.append("│", style="dim")
                separators -= 1
            chart.append("█" * length, style=colors[activity])
            drawn = True
        chart.append(f" {total:.2f}m\n", style="default")
    chart.append("\n", style="default")
    return chart

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
DIM = "\033[2m"

def get_data():
    return database.get_sessions()

def draw_chart(data):
    if not data:
        print("No data found in database.")
        return

    # Organize data: date -> activity -> list of durations
    daily_stats = {}
    activities = set()
    
    for date, activity, duration in data:
        if date not in daily_stats:
            daily_stats[date] = {}
        if activity not in daily_stats[date]:
            daily_stats[date][activity] = []
        
        daily_stats[date][activity].append(duration)
        activities.add(activity)
    
    # Assign colors to activities
    sorted_activities = sorted(list(activities))
    activity_colors = {act: COLORS[i % len(COLORS)] for i, act in enumerate(sorted_activities)}

    # Calculate max daily total for scaling
    max_val = 0
    for date, acts in daily_stats.items():
        total = sum(sum(durations) for durations in acts.values())
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
        total_duration = sum(sum(durations) for durations in acts.values())
        
        # Build the stacked bar
        bar_str = ""
        
        for activity in sorted_activities:
            if activity in acts:
                durations = acts[activity]
                color = activity_colors[activity]
                
                # Build segments for this activity
                segments = []
                for dur in durations:
                    if dur > 0:
                        # Calculate length for this specific session
                        segment_len = int((dur / max_val) * bar_width)
                        # Ensure visibility for small sessions
                        if segment_len == 0 and dur > 0:
                            segment_len = 1
                        segments.append("█" * segment_len)
                
                # Join sessions with a thin separator
                # We apply the color to the blocks, and use a dim separator
                activity_bar = f"{RESET}{DIM}|{RESET}{color}".join(segments)
                
                bar_str += f"{color}{activity_bar}{RESET}"
        
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
    except Exception as error:
        print(f"Could not read the database: {error}")
