from types import SimpleNamespace

import notifications


def test_failed_audio_backend_falls_through_to_working_backend(monkeypatch):
    called = []
    monkeypatch.setattr(notifications.shutil, 'which', lambda name: '/usr/bin/' + name)
    def run(command, **kwargs):
        called.append(command)
        return SimpleNamespace(returncode=1 if command[0] == '/usr/bin/aplay' else 0)
    monkeypatch.setattr(notifications.subprocess, 'run', run)
    assert notifications.play_sound(terminal_fallback=False)
    assert len(called) == 2
    assert called[1][0].endswith('paplay')
    assert called[1][1] == str(notifications.SOUND_FILE)
    assert notifications.SOUND_FILE.is_absolute()


def test_unavailable_audio_falls_back_to_terminal_when_requested(monkeypatch, capsys):
    monkeypatch.setattr(notifications.shutil, 'which', lambda name: None)
    assert not notifications.play_sound(terminal_fallback=False)
    assert capsys.readouterr().out == ''
    assert not notifications.play_sound()
    assert capsys.readouterr().out == '\a'
