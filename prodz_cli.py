import time
import argparse
import subprocess
import shutil
import database
import sys
import select
import termios
import tty
import plot

def play_sound():
    """Plays a notification sound using system tools."""
    sound_file = "bell.wav"
    try:
        # Check for available players
        if shutil.which("aplay"):
            subprocess.run(["aplay", "-q", sound_file], check=False)
        elif shutil.which("paplay"):
            subprocess.run(["paplay", sound_file], check=False)
        elif shutil.which("ffplay"):
            subprocess.run(["ffplay", "-nodisp", "-autoexit", sound_file], stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL, check=False)
        else:
             # Fallback to terminal bell
            print("\a", end="", flush=True)
    except Exception:
        # Fallback to terminal bell
        print("\a", end="", flush=True)

def countdown(t, label, activity_name=None):
    """Displays a countdown timer with pause and exit functionality."""
    print(f"{label}: Starting... (p:pause, l:log&exit, q:quit)", end="\r")
    
    initial_t = t
    # Save original terminal settings
    old_settings = termios.tcgetattr(sys.stdin)
    try:
        tty.setcbreak(sys.stdin.fileno())
        
        while t > 0:
            mins, secs = divmod(t, 60)
            timer = '{:02d}:{:02d}'.format(mins, secs)
            print(f"{label}: {timer}   ", end="\r")
            
            # Wait for input or timeout (1 second)
            rlist, _, _ = select.select([sys.stdin], [], [], 1)
            
            if rlist:
                key = sys.stdin.read(1)
                if key.lower() == 'p':
                    print(f"\r{label}: PAUSED (p:resume, q:quit)   ", end="")
                    while True:
                        # Wait indefinitely for input when paused
                        rlist_paused, _, _ = select.select([sys.stdin], [], [])
                        if rlist_paused:
                            key_paused = sys.stdin.read(1)
                            if key_paused.lower() == 'p':
                                print(f"\r{label}: Resuming...                  ", end="")
                                break
                            elif key_paused.lower() == 'q':
                                key = 'q'
                                break
                            elif key_paused.lower() == 'l':
                                # Handle exit from pause state
                                key = 'l' 
                                break
                
                if key.lower() == 'l':
                    elapsed_seconds = initial_t - t
                    elapsed_minutes = elapsed_seconds / 60
                    print(f"\nExiting... Session logged: {elapsed_minutes:.2f} min")
                    if activity_name:
                         database.log_session(activity_name, elapsed_minutes)
                    sys.exit(0)
                
                if key.lower() == 'q':
                    print("\nQuitting without logging...")
                    sys.exit(0)

            else:
                # Timeout reached, decrement timer
                t -= 1
    finally:
        # Restore terminal settings
        termios.tcsetattr(sys.stdin, termios.TCSADRAIN, old_settings)
        print() # Ensure newline after loop

def start_prodz(work_minutes, break_minutes, long_break_minutes, cycles, activity):
    """Starts the prodzCLI timer."""
    database.init_db()
    for i in range(cycles):
        print(f"--- Cycle {i+1}/{cycles} ---")
        print(f"Starting work session: {activity}")
        countdown(work_minutes * 60, "Work", activity)
        print("\nWork session complete!")
        play_sound()
        database.log_session(activity, work_minutes)

        if (i + 1) % 4 == 0:
            print("Starting long break...")
            countdown(long_break_minutes * 60, "Long Break")
            print("\nLong break complete!")
            play_sound()
        else:
            print("Starting short break...")
            countdown(break_minutes * 60, "Break")
            print("\nBreak complete!")
            play_sound()

    print("prodzCLI session finished!")

def input_safe_int(prompt, default):
    try:
        val = input(f"{prompt} [{default}]: ").strip()
        return int(val) if val else default
    except ValueError:
        return default

def show_menu():
    while True:
        print("\n--- Prodz CLI Menu ---")
        print("1. Start Default Session (25/5)")
        print("2. Start Custom Session")
        print("3. View Statistics")
        print("4. Quit")
        
        choice = input("Enter choice: ").strip()
        
        if choice == '1':
            activity = input("Activity name [default]: ").strip() or "default"
            start_prodz(25, 5, 15, 4, activity)
        elif choice == '2':
            activity = input("Activity name [custom]: ").strip() or "custom"
            w = input_safe_int("Work minutes", 25)
            b = input_safe_int("Short break minutes", 5)
            lb = input_safe_int("Long break minutes", 15)
            c = input_safe_int("Cycles", 4)
            start_prodz(w, b, lb, c, activity)
        elif choice == '3':
            try:
                data = plot.get_data()
                plot.draw_chart(data)
            except Exception as e:
                print(f"Error showing stats: {e}")
        elif choice == '4':
            print("Goodbye!")
            sys.exit(0)
        else:
            print("Invalid choice, try again.")

if __name__ == "__main__":
    if len(sys.argv) > 1:
        parser = argparse.ArgumentParser(description="prodzCLI - A simple productivity timer.")
        parser.add_argument("-w", "--work", type=int, default=25,
                            help="Work session duration in minutes (default: 25)")
        parser.add_argument("-b", "--break", type=int, default=5,
                            help="Short break duration in minutes (default: 5)")
        parser.add_argument("-lb", "--long-break", type=int, default=15,
                            help="Long break duration in minutes (default: 15)")
        parser.add_argument("-c", "--cycles", type=int, default=4,
                            help="Number of work cycles (default: 4)")
        parser.add_argument("-a", "--activity", type=str, default="whatever",
                            help="Name of the activity (default: 'whatever')")
        args = parser.parse_args()

        start_prodz(args.work, getattr(args, 'break'), args.long_break, args.cycles, args.activity)
    else:
        show_menu()