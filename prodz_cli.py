import argparse
import database
import json
import os
import signal
import sys
import select
import termios
import threading
import tty
from pathlib import Path
import plot
from timer import Timer
from notifications import play_sound

STATE_DIR = Path(os.environ.get("XDG_STATE_HOME", Path.home() / ".local/state")) / "prodz"

_active_timer = None
_session_saved = False


def _sync_after_save():
    """Best-effort background sync; never blocks the timer or Waybar."""
    try:
        database.sync_once()
    except Exception:
        pass


def _state_file():
    """Location of the live session snapshot; PRODZ_STATE_FILE overrides it."""
    override = os.environ.get("PRODZ_STATE_FILE")
    return Path(override) if override else STATE_DIR / "session.json"


def _on_pause_signal(signum, frame):
    """SIGUSR1 toggles pause/resume, e.g. from a Waybar module click."""
    if _active_timer is not None and not _active_timer.finished:
        _active_timer.resume() if _active_timer.paused else _active_timer.pause()


def _write_state(phase, timer, cycle, cycles):
    """Exports a live session snapshot for external tools such as Waybar."""
    state = {
        "phase": phase,
        "state": "paused" if timer.paused else "running",
        "activity": timer.activity,
        "comment": timer.comment,
        "cycle": cycle,
        "cycles": cycles,
        "elapsed": round(timer.elapsed, 1),
        "remaining": None if timer.remaining is None else round(timer.remaining, 1),
        "endless": timer.seconds is None,
        "started_at": timer.started_at.isoformat(),
        "pid": os.getpid(),
    }
    path = _state_file()
    path.parent.mkdir(parents=True, exist_ok=True)
    temp = path.with_suffix(path.suffix + ".tmp")
    temp.write_text(json.dumps(state))
    temp.replace(path)


def _clear_state():
    _state_file().unlink(missing_ok=True)


def countdown(t, label, activity_name=None, comment="", endless=False, cycle=1, cycles=1):
    """Terminal adapter for the shared monotonic timer."""
    global _active_timer
    timer = Timer(None if endless else t, activity=activity_name or label, comment=comment)
    _active_timer = timer
    signal.signal(signal.SIGUSR1, _on_pause_signal)
    old_settings = termios.tcgetattr(sys.stdin)
    outcome = "complete"
    try:
        tty.setcbreak(sys.stdin.fileno())
        while endless or not timer.complete:
            _write_state(label, timer, cycle, cycles)
            elapsed = int(timer.elapsed)
            remaining = "Endless" if timer.remaining is None else f"{int(timer.remaining) // 60:02d}:{int(timer.remaining) % 60:02d}"
            state = "PAUSED" if timer.paused else label
            print(f"\r{state}: {elapsed // 60:02d}:{elapsed % 60:02d} remaining {remaining}  p:pause l:save q:discard    ", end="", flush=True)
            ready, _, _ = select.select([sys.stdin], [], [], 0.2)
            if not ready:
                continue
            key = sys.stdin.read(1).lower()
            if key == "p":
                timer.resume() if timer.paused else timer.pause()
            elif key in {"l", "q"}:
                outcome = key
                break
    finally:
        _active_timer = None
        termios.tcsetattr(sys.stdin, termios.TCSADRAIN, old_settings)
        print()
    if outcome == "q":
        timer.discard()
        raise SystemExit(0)
    if activity_name:
        save = timer.save()
        if save:
            database.Store().create(save.session, session_id=save.session_id)
            print(f"Saved {save.session.duration_minutes:.2f} minutes locally.")
            global _session_saved
            _session_saved = True
            threading.Thread(target=_sync_after_save, daemon=True).start()
    if outcome == "l":
        raise SystemExit(0)


def start_prodz(work_minutes, break_minutes, long_break_minutes, cycles, activity, comment=""):
    """Starts the prodzCLI timer."""
    if min(work_minutes, break_minutes, long_break_minutes, cycles) <= 0:
        raise ValueError("Durations and cycles must be positive")
    database.init_db()
    try:
        for i in range(cycles):
            print(f"--- Cycle {i + 1}/{cycles} ---")
            print(f"Starting work session: {activity}")
            countdown(work_minutes * 60, "Work", activity, comment,
                      cycle=i + 1, cycles=cycles)
            print("\nWork session complete!")
            play_sound()

            if i + 1 == cycles:
                break
            if (i + 1) % 4 == 0:
                print("Starting long break...")
                countdown(long_break_minutes * 60, "Long Break",
                          cycle=i + 1, cycles=cycles)
                print("\nLong break complete!")
                play_sound()
            else:
                print("Starting short break...")
                countdown(break_minutes * 60, "Break",
                          cycle=i + 1, cycles=cycles)
                print("\nBreak complete!")
                play_sound()

        print("prodzCLI session finished!")
    finally:
        if _session_saved:
            worker = threading.Thread(target=_sync_after_save, daemon=True)
            worker.start()
            worker.join(timeout=5)
        _clear_state()


def start_endless(activity, comment=""):
    """Starts an endless prodzCLI session."""
    database.init_db()
    print(f"--- Endless Session ---")
    print(f"Starting work session: {activity}")
    try:
        countdown(0, "Work", activity, comment, endless=True)
    finally:
        if _session_saved:
            worker = threading.Thread(target=_sync_after_save, daemon=True)
            worker.start()
            worker.join(timeout=5)
        _clear_state()


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
        print("3. Start Endless Session")
        print("4. View Statistics")
        print("5. Quit")

        choice = input("Enter choice: ").strip()

        if choice == "1":
            activity = input("Activity name [default]: ").strip() or "default"
            comment = input("Comment [optional]: ").strip()
            start_prodz(25, 5, 15, 4, activity, comment)
        elif choice == "2":
            activity = input("Activity name [custom]: ").strip() or "custom"
            comment = input("Comment [optional]: ").strip()
            w = input_safe_int("Work minutes", 25)
            b = input_safe_int("Short break minutes", 5)
            lb = input_safe_int("Long break minutes", 15)
            c = input_safe_int("Cycles", 4)
            start_prodz(w, b, lb, c, activity, comment)
        elif choice == "3":
            activity = input("Activity name [endless]: ").strip() or "endless"
            comment = input("Comment [optional]: ").strip()
            start_endless(activity, comment)
        elif choice == "4":
            try:
                data = plot.get_data()
                plot.draw_chart(data)
            except Exception as e:
                print(f"Error showing stats: {e}")
        elif choice == "5":
            print("Goodbye!")
            sys.exit(0)
        else:
            print("Invalid choice, try again.")


if __name__ == "__main__":
    if len(sys.argv) > 1:
        parser = argparse.ArgumentParser(
            description="prodzCLI - A simple productivity timer."
        )
        parser.add_argument(
            "-w",
            "--work",
            type=int,
            default=25,
            help="Work session duration in minutes (default: 25)",
        )
        parser.add_argument(
            "-b",
            "--break",
            type=int,
            default=5,
            help="Short break duration in minutes (default: 5)",
        )
        parser.add_argument(
            "-lb",
            "--long-break",
            type=int,
            default=15,
            help="Long break duration in minutes (default: 15)",
        )
        parser.add_argument(
            "-c",
            "--cycles",
            type=int,
            default=4,
            help="Number of work cycles (default: 4)",
        )
        parser.add_argument(
            "-a",
            "--activity",
            type=str,
            default="whatever",
            help="Name of the activity (default: 'whatever')",
        )
        parser.add_argument(
            "-t",
            "--text",
            type=str,
            default="",
            help="Short comment about the activity",
        )
        parser.add_argument(
            "-e",
            "--endless",
            action="store_true",
            help="Start an endless session",
        )
        args = parser.parse_args()

        if args.endless:
            start_endless(args.activity, args.text)
        else:
            start_prodz(
                args.work,
                getattr(args, "break"),
                args.long_break,
                args.cycles,
                args.activity,
                args.text,
            )
    else:
        show_menu()
