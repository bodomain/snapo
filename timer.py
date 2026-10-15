"""A local, monotonic timer independent of terminal input and persistence."""
import math
import time
from dataclasses import dataclass
from datetime import datetime
import uuid

from sessions import Session


@dataclass(frozen=True)
class TimerSave:
    session_id: str
    session: Session


class Timer:
    def __init__(self, seconds=None, *, activity="Work", comment="", clock=time.monotonic,
                 started_at=None):
        if seconds is not None and (not math.isfinite(seconds) or seconds <= 0):
            raise ValueError("Timer duration must be positive")
        if not activity.strip():
            raise ValueError("Activity is required")
        self.seconds = seconds
        self.activity = activity
        self.comment = comment
        self.clock = clock
        self.started_at = started_at or datetime.now().astimezone()
        self.session_id = str(uuid.uuid4())
        self._started = clock()
        self._elapsed = 0.0
        self.paused = False
        self.finished = False
        self._save = None

    @property
    def elapsed(self):
        elapsed = self._elapsed + (0 if self.paused or self.finished else self.clock() - self._started)
        return elapsed if self.seconds is None else min(elapsed, self.seconds)

    @property
    def remaining(self):
        return None if self.seconds is None else max(0, self.seconds - self.elapsed)

    @property
    def complete(self):
        return self.seconds is not None and self.elapsed >= self.seconds

    def pause(self):
        if not self.finished and not self.paused:
            self._elapsed = self.elapsed
            self.paused = True

    def resume(self):
        if not self.finished and self.paused:
            self._started = self.clock()
            self.paused = False

    def save(self):
        if self._save is not None:
            return self._save
        if self.finished:
            return None
        elapsed = self.elapsed
        if elapsed <= 0:
            raise ValueError("No elapsed work to save")
        self._elapsed = elapsed
        self.finished = True
        self._save = TimerSave(self.session_id, Session(
            self.started_at.date().isoformat(), self.started_at.isoformat(),
            self.activity, elapsed / 60, self.comment))
        return self._save

    def discard(self):
        if not self.finished:
            self._elapsed = self.elapsed
            self.finished = True
