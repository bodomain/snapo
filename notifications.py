"""Audio notifications shared by the CLI and TUI."""
from pathlib import Path
import shutil
import subprocess


SOUND_FILE = Path(__file__).with_name("bell.wav")


def play_sound(*, terminal_fallback=True):
    """Play the completion sound; return whether an audio player succeeded.

    Call from a background thread in an interactive UI. Failed or unavailable
    players fall through to the next backend; no backend can wait indefinitely.
    """
    commands = (
        ("aplay", ["-q", str(SOUND_FILE)]),
        ("paplay", [str(SOUND_FILE)]),
        ("ffplay", ["-nodisp", "-autoexit", str(SOUND_FILE)]),
    )
    for player, arguments in commands:
        executable = shutil.which(player)
        if not executable:
            continue
        try:
            result = subprocess.run([executable, *arguments], stdout=subprocess.DEVNULL,
                                    stderr=subprocess.DEVNULL, timeout=10, check=False)
            if result.returncode == 0:
                return True
        except (OSError, subprocess.TimeoutExpired):
            continue
    if terminal_fallback:
        print("\a", end="", flush=True)
    return False
