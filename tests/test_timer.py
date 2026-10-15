from datetime import datetime
import pytest
from timer import Timer


def test_pause_resume_completion_and_save_once():
    now = [0.0]
    timer = Timer(60, activity='Test', clock=lambda: now[0],
                  started_at=datetime.fromisoformat('2026-10-15T23:59:00+02:00'))
    now[0] = 10
    timer.pause()
    now[0] = 100
    assert timer.elapsed == 10
    timer.resume()
    now[0] = 200
    assert timer.complete
    assert timer.elapsed == 60
    first = timer.save()
    assert first.session.duration_minutes == 1
    assert first.session.date == '2026-10-15'
    assert timer.save() is first
    now[0] = 300
    assert timer.elapsed == 60


def test_partial_endless_save_and_discard():
    now = [0.0]
    timer = Timer(clock=lambda: now[0])
    now[0] = 75
    assert timer.save().session.duration_minutes == 1.25
    other = Timer(clock=lambda: now[0])
    now[0] = 100
    other.discard()
    assert other.save() is None
    assert other.elapsed == 25


def test_empty_timer_can_retry_after_time_elapses():
    now = [0.0]
    timer = Timer(clock=lambda: now[0])
    with pytest.raises(ValueError):
        timer.save()
    assert not timer.finished
    now[0] = 1
    assert timer.save() is not None


@pytest.mark.parametrize('seconds', [0, -1, float('nan'), float('inf')])
def test_invalid_duration(seconds):
    with pytest.raises(ValueError):
        Timer(seconds)
