import asyncio
import sqlite3
import time

import pytest
from textual.widgets import Button, Checkbox, DataTable, Input, Static, TabbedContent

from app import Editor, Question, Snapo
from database import Config, Store
from sessions import Session, SessionHistory


@pytest.fixture
def store(tmp_path):
    result = Store(Config(tmp_path / 'ui.db', local=True))
    result.initialize()
    return result


async def settle(app, pilot):
    await app.workers.wait_for_complete()
    await pilot.pause()


async def test_create_edit_filter_trash_restore_and_export(store, tmp_path):
    app = Snapo(store, auto_sync=False)
    async with app.run_test(size=(120, 45)) as pilot:
        await settle(app, pilot)
        assert await pilot.click(app.query_one('#tabs', TabbedContent).get_tab('history-tab'))
        await pilot.pause()
        await pilot.press('ctrl+n')
        assert isinstance(app.screen, Editor)
        app.screen.query_one('#edit-activity', Input).value = 'Python study'
        assert await pilot.click('#editor-save')
        await settle(app, pilot)
        assert len(store.entries()) == 1
        assert app.query_one('#history', DataTable).row_count == 1
        await pilot.press('ctrl+e')
        app.screen.query_one('#edit-duration', Input).value = '12.5'
        assert await pilot.click('#editor-save')
        await settle(app, pilot)
        assert store.entries()[0].current.session.duration_minutes == 12.5
        app.query_one('#search', Input).value = 'absent'
        await pilot.pause()
        assert app.query_one('#history', DataTable).row_count == 0
        app.query_one('#search', Input).value = ''
        await pilot.pause()
        await pilot.click('#delete')
        assert isinstance(app.screen, Question)
        await pilot.click('#yes')
        await settle(app, pilot)
        assert store.entries()[0].current.deleted
        assert app.query_one('#history', DataTable).row_count == 0
        assert await pilot.click(app.query_one('#tabs', TabbedContent).get_tab('trash-tab'))
        await pilot.pause()
        assert app.query_one('#tabs', TabbedContent).active == 'trash-tab'
        assert await pilot.click('#restore')
        await settle(app, pilot)
        assert not store.entries()[0].current.deleted
        assert await pilot.click(app.query_one('#tabs', TabbedContent).get_tab('history-tab'))
        await pilot.pause()
        await pilot.click('#export')
        app.screen.query_one('#export-path', Input).value = str(tmp_path / 'ui-export.csv')
        await pilot.click('#export-save')
        await settle(app, pilot)
        assert 'Python study' in (tmp_path / 'ui-export.csv').read_text()


async def test_stale_form_shows_error_without_overwriting(store):
    store.create(Session.now('Original', 25))
    app = Snapo(store, auto_sync=False)
    async with app.run_test(size=(120, 45)) as pilot:
        await settle(app, pilot)
        app.action_edit()
        await pilot.pause()
        entry = store.entries()[0]
        await asyncio.to_thread(store.change, entry.session_id, entry.head_ids, session=Session.now('Other editor', 30))
        app.screen.query_one('#edit-activity', Input).value = 'Stale'
        assert await pilot.click('#editor-save')
        assert isinstance(app.screen, Editor)
        assert 'changed' in str(app.screen.query_one('#editor-error', Static).content)
        assert store.entries()[0].current.session.activity == 'Other editor'
        await pilot.press('escape')


async def test_timer_save_is_once_and_close_can_cancel(store):
    app = Snapo(store, auto_sync=False)
    async with app.run_test(size=(120, 45)) as pilot:
        await settle(app, pilot)
        app.query_one('#activity', Input).value = 'Focus'
        app.query_one('#endless', Checkbox).value = True
        await pilot.click('#start')
        assert app.timer is not None
        app.timer._started -= 75
        await pilot.press('f2')
        assert app.timer.paused
        await pilot.press('ctrl+q')
        assert isinstance(app.screen, Question)
        await pilot.click('#cancel')
        assert app.timer is not None
        await pilot.click('#save')
        await settle(app, pilot)
        assert app.timer is None
        assert len(store.entries()) == 1
        assert store.entries()[0].current.session.duration_minutes >= 1.25


async def test_sync_does_not_freeze_timer_controls(store, monkeypatch):
    def slow_sync():
        time.sleep(.4)
        raise RuntimeError('offline')
    monkeypatch.setattr(store, 'sync', slow_sync)
    app = Snapo(store, auto_sync=False)
    async with app.run_test(size=(120, 45)) as pilot:
        await settle(app, pilot)
        app.query_one('#activity', Input).value = 'Focus'
        app.start_timer()
        app.sync_remote()
        await asyncio.sleep(.02)
        app.action_pause_timer()
        assert app.timer.paused
        await settle(app, pilot)
        assert 'offline' in str(app.query_one('#sync-status', Static).content)


async def test_conflict_review_can_choose_delete_or_corrected_contents(store):
    root = store.create(Session.now('Original', 25))
    with sqlite3.connect(store.config.path) as conn:
        h = SessionHistory(conn, 'other')
        a = h.change(root.session_id, (root.revision_id,), session=Session.now('Version A', 30))
        # An independently created branch from the root.
        from dataclasses import replace
        b = replace(a, revision_id='other-revision', device_id='device-b',
                    session=Session.now('Version B', 15), deleted=True)
        h.append(b)
    app = Snapo(store, auto_sync=False)
    async with app.run_test(size=(120, 45)) as pilot:
        await settle(app, pilot)
        assert await pilot.click(app.query_one('#tabs', TabbedContent).get_tab('conflicts-tab'))
        await pilot.pause()
        await pilot.click('#resolve')
        assert isinstance(app.screen, Editor)
        app.screen.query_one('#edit-activity', Input).value = 'Resolved'
        app.screen.query_one('#edit-deleted', Checkbox).value = False
        assert await pilot.click('#editor-save')
        await settle(app, pilot)
        entry = store.entries()[0]
        assert not entry.conflicted
        assert entry.current.session.activity == 'Resolved'
        assert not entry.current.deleted


@pytest.mark.parametrize('size', [(80, 24), (100, 30)])
async def test_editor_actions_remain_visible_in_small_terminals(store, size):
    app = Snapo(store, auto_sync=False)
    async with app.run_test(size=size) as pilot:
        await settle(app, pilot)
        await pilot.press('ctrl+n')
        await pilot.pause()
        save = app.screen.query_one('#editor-save', Button)
        assert save.region.bottom <= size[1]
        app.screen.query_one('#edit-activity', Input).value = 'Small screen'
        assert await pilot.click('#editor-save')
        await settle(app, pilot)
        assert len(store.entries()) == 1


async def test_ctrl_c_prompts_and_discard_does_not_save(store):
    app = Snapo(store, auto_sync=False)
    async with app.run_test(size=(100, 30)) as pilot:
        await settle(app, pilot)
        app.query_one('#activity', Input).value = 'Discarded'
        app.start_timer()
        await pilot.press('ctrl+c')
        assert isinstance(app.screen, Question)
        await pilot.click('#cancel')
        assert app.timer is not None
        app.discard_timer('yes')
        assert not store.entries()
        assert app.timer is None


async def test_sync_status_is_visible_above_footer(store):
    from textual.widgets import Footer
    app = Snapo(store, auto_sync=False)
    async with app.run_test(size=(100, 30)) as pilot:
        await settle(app, pilot)
        status = app.query_one('#sync-status', Static)
        footer = app.query_one(Footer)
        assert status.region.height >= 1
        assert status.region.bottom <= footer.region.y, (status.region, footer.region)


async def test_completed_work_plays_audio_without_blocking_ui(store, monkeypatch):
    import app as app_module
    called = []
    def sound(**kwargs):
        called.append(kwargs)
        return True
    monkeypatch.setattr(app_module, 'play_sound', sound)
    app = Snapo(store, auto_sync=False)
    async with app.run_test(size=(120, 45)) as pilot:
        await settle(app, pilot)
        app.query_one('#activity', Input).value = 'Audible completion'
        app.query_one('#timer-cycles', Input).value = '1'
        app.start_timer()
        app.timer._started -= app.timer.seconds
        await app.finish_timer(advance=True).wait()
        await settle(app, pilot)
        assert called == [{'terminal_fallback': False}]
        assert len(store.entries()) == 1
        assert app.timer is None
