"""Snapo's Linux terminal workspace. Run: python app.py --help."""
import argparse
import asyncio
from datetime import datetime
from pathlib import Path

from textual import work
from rich.text import Text
from textual.app import App, ComposeResult
from textual.binding import Binding
from textual.containers import Horizontal, Vertical, VerticalScroll
from textual.screen import ModalScreen
from textual.widgets import (Button, Checkbox, DataTable, Footer, Header, Input, Label,
                             Select, Static, TabbedContent, TabPane)

from database import Config, Store
from sessions import Session, export_csv, filtered, totals
from timer import Timer
from notifications import play_sound


class Question(ModalScreen[str]):
    BINDINGS = [("escape", "cancel", "Cancel")]

    def __init__(self, text, *, exiting=False):
        super().__init__()
        self.text = text
        self.exiting = exiting

    def compose(self):
        with Vertical(id="dialog"):
            yield Static(self.text, markup=False)
            with Horizontal(classes="buttons"):
                if self.exiting:
                    yield Button("Save & exit", id="yes", variant="primary")
                    yield Button("Discard & exit", id="discard", variant="error")
                else:
                    yield Button("Confirm", id="yes", variant="primary")
                yield Button("Cancel", id="cancel")

    def on_button_pressed(self, event):
        event.stop()
        self.dismiss(event.button.id)

    def action_cancel(self):
        self.dismiss("cancel")


class Editor(ModalScreen[bool]):
    BINDINGS = [("escape", "cancel", "Cancel")]

    def __init__(self, entry=None, *, resolving=False):
        super().__init__()
        self.entry = entry
        self.resolving = resolving
        self.saving = False

    def compose(self):
        session = self.entry.heads[0].session if self.entry else Session.now("", 25)
        with Vertical(id="editor-dialog"):
            with VerticalScroll(id="editor-fields"):
                yield Label("Resolve conflict" if self.resolving else "Edit session" if self.entry else "Add session")
                if self.resolving:
                    yield Static("Choose a version as a starting point, or enter corrected values.\n"
                                 "Saving resolves all versions shown here; every original is retained.")
                    yield Select([(f"{r.device_id[:8]} · {r.created_at} · "
                                   f"{'DELETED' if r.deleted else r.session.activity}", r.revision_id)
                                  for r in self.entry.heads], allow_blank=False,
                                 value=self.entry.heads[0].revision_id, id="version")
                    yield Static("", id="version-detail", markup=False)
                for label, identifier, value in (
                    ("Activity", "edit-activity", session.activity),
                    ("Date (YYYY-MM-DD)", "edit-date", session.date),
                    ("Timestamp (ISO format; offset recommended)", "edit-timestamp", session.timestamp),
                    ("Duration in minutes", "edit-duration", str(session.duration_minutes)),
                    ("Comment", "edit-comment", session.comment),
                ):
                    yield Label(label)
                    yield Input(value, id=identifier)
                if self.resolving:
                    yield Checkbox("Keep this session in Trash", value=self.entry.heads[0].deleted, id="edit-deleted")

            yield Static("", id="editor-error", markup=False)
            with Horizontal(classes="buttons"):
                yield Button("Save", id="editor-save", variant="primary")
                yield Button("Cancel", id="editor-cancel")

    def on_mount(self):
        if self.resolving:
            self.show_version(self.entry.heads[0].revision_id)

    def show_version(self, revision_id):
        r = next(r for r in self.entry.heads if r.revision_id == revision_id)
        s = r.session
        for identifier, value in (("activity", s.activity), ("date", s.date), ("timestamp", s.timestamp),
                                  ("duration", str(s.duration_minutes)), ("comment", s.comment)):
            self.query_one(f"#edit-{identifier}", Input).value = value
        self.query_one("#edit-deleted", Checkbox).value = r.deleted
        self.query_one("#version-detail", Static).update(
            f"Device: {r.device_id}\nChanged: {r.created_at}\n"
            f"Activity: {s.activity} · {s.duration_minutes:g} min\nComment: {s.comment}\n"
            f"State: {'Trash' if r.deleted else 'Active'}")

    def on_select_changed(self, event):
        if event.select.id == "version" and isinstance(event.value, str):
            self.show_version(event.value)

    async def on_button_pressed(self, event):
        event.stop()
        if event.button.id == "editor-cancel":
            self.action_cancel()
        elif event.button.id == "editor-save" and not self.saving:
            self.saving = True
            event.button.disabled = True
            try:
                value = lambda key: self.query_one(f"#edit-{key}", Input).value
                session = Session(value("date"), value("timestamp"), value("activity"),
                                  float(value("duration")), value("comment")).validated()
                if self.entry:
                    changes = {"session": session}
                    if self.resolving:
                        changes.update(resolve=True, deleted=self.query_one("#edit-deleted", Checkbox).value)
                    await asyncio.to_thread(self.app.store.change, self.entry.session_id,
                                            self.entry.head_ids, **changes)
                else:
                    await asyncio.to_thread(self.app.store.create, session)
            except Exception as error:
                self.query_one("#editor-error", Static).update(str(error))
                event.button.disabled = False
                self.saving = False
            else:
                self.dismiss(True)

    def action_cancel(self):
        if not self.saving:
            self.dismiss(False)


class ExportDialog(ModalScreen[bool]):
    BINDINGS = [("escape", "cancel", "Cancel")]

    def __init__(self, entries):
        super().__init__()
        self.entries = entries

    def compose(self):
        with Vertical(id="dialog"):
            yield Label("Export filtered active sessions (conflicts excluded)")
            yield Input(str(Path.home() / f"snapo-{datetime.now():%Y%m%d-%H%M%S}.csv"), id="export-path")
            yield Static("Existing files will not be overwritten.")
            yield Static("", id="export-error", markup=False)
            with Horizontal(classes="buttons"):
                yield Button("Export", id="export-save", variant="primary")
                yield Button("Cancel", id="export-cancel")

    async def on_button_pressed(self, event):
        event.stop()
        if event.button.id == "export-cancel":
            self.dismiss(False)
        elif event.button.id == "export-save":
            event.button.disabled = True
            try:
                await asyncio.to_thread(export_csv, self.entries,
                                        Path(self.query_one("#export-path", Input).value).expanduser())
            except Exception as error:
                self.query_one("#export-error", Static).update(str(error))
                event.button.disabled = False
            else:
                self.dismiss(True)

    def action_cancel(self):
        self.dismiss(False)


class Snapo(App):
    TITLE = "Snapo"
    SUB_TITLE = "Focus · history · sync"
    CSS = """
    Screen { background: $surface; }
    #bottom-bar { dock: bottom; height: auto; }
    #bottom-bar Footer { dock: none; }
    #sync-status { height: auto; max-height: 4; padding: 0 1; background: $panel; }
    TabPane { padding: 1 2; }
    .buttons { height: auto; min-height: 3; margin-top: 1; }
    .buttons Button { margin-right: 1; }
    .filter-row { height: 3; }
    .filter-row Input { width: 1fr; }
    #timer-display { height: 5; content-align: center middle; text-style: bold; border: round $accent; }
    #timer-form { height: auto; }
    .setting { width: 1fr; height: auto; padding-right: 1; }
    #reports-body { height: 1fr; }
    DataTable { height: 1fr; }
    ModalScreen { align: center middle; background: $background 70%; }
    #dialog { width: 80; max-width: 95%; height: auto; padding: 1 2; border: thick $accent; background: $surface; }
    #editor-dialog { width: 90; max-width: 95%; height: 90%; padding: 1 2; border: thick $accent; background: $surface; }
    #editor-fields { height: 1fr; }
    #editor-error, #export-error { color: $error; height: auto; }
    #version-detail { height: auto; margin: 1 0; }
    """
    BINDINGS = [Binding("ctrl+q", "request_quit", "Quit", priority=True),
                Binding("ctrl+c", "request_quit", show=False, priority=True),
                Binding("ctrl+n", "add", "Add"), Binding("ctrl+e", "edit", "Edit"),
                Binding("ctrl+d", "delete", "Trash"), Binding("ctrl+r", "restore", "Restore"),
                Binding("ctrl+s", "sync_now", "Sync"), Binding("f5", "reload", "Refresh"),
                Binding("f2", "pause_timer", "Pause/resume")]

    def __init__(self, store=None, *, auto_sync=True):
        super().__init__()
        self.store = store or Store()
        self.auto_sync = auto_sync
        self.entries = []
        self.timer = None
        self.phase = "Work"
        self.cycle = 0
        self.settings = None
        self.saving_timer = False
        self.timer_save_failed = False
        self.ready = False
        self.loading = False
        self.syncing = False
        self.error = None
        self.sync_error = None
        self.history_lock = asyncio.Lock()

    def compose(self) -> ComposeResult:
        yield Header()
        with TabbedContent(initial="timer-tab", id="tabs"):
            with TabPane("Timer", id="timer-tab"):
                with VerticalScroll():
                    yield Static("Ready to focus", id="timer-display", markup=False)
                    with Vertical(id="timer-form"):
                        yield Label("Activity")
                        yield Input(placeholder="What are you working on?", id="activity")
                        yield Label("Comment")
                        yield Input(id="comment")
                        with Horizontal(classes="buttons"):
                            for name, label, value in (("work", "Work minutes", "25"), ("short", "Short break", "5"),
                                                        ("long", "Long break", "15"), ("cycles", "Cycles", "4")):
                                with Vertical(classes="setting"):
                                    yield Label(label)
                                    yield Input(value, id=f"timer-{name}")
                        yield Checkbox("Endless session", id="endless")
                    with Horizontal(classes="buttons"):
                        yield Button("Start", id="start", variant="primary")
                        yield Button("Pause / resume", id="pause")
                        yield Button("Save & stop", id="save")
                        yield Button("Discard", id="discard", variant="error")
            with TabPane("History", id="history-tab"):
                with Horizontal(classes="filter-row"):
                    yield Input(placeholder="Search activity, comment, timestamp", id="search")
                    yield Input(placeholder="Activity contains…", id="filter-activity")
                with Horizontal(classes="filter-row"):
                    yield Input(placeholder="From YYYY-MM-DD", id="since")
                    yield Input(placeholder="Through YYYY-MM-DD", id="until")
                yield Static("", id="filter-error", markup=False)
                yield DataTable(id="history", cursor_type="row")
                with Horizontal(classes="buttons"):
                    yield Button("Add", id="add")
                    yield Button("Edit", id="edit")
                    yield Button("Move to Trash", id="delete")
                    yield Button("Export CSV", id="export")
            with TabPane("Reports", id="reports-tab"):
                yield Static("Uses the History filters. Deleted and conflicted sessions are excluded.")
                with VerticalScroll(id="reports-body"):
                    yield Static("", id="reports", markup=False)
                with Horizontal(classes="buttons"):
                    yield Button("Export CSV", id="report-export")
            with TabPane("Trash", id="trash-tab"):
                yield DataTable(id="trash", cursor_type="row")
                with Horizontal(classes="buttons"):
                    yield Button("Restore selected", id="restore")
            with TabPane("Conflicts", id="conflicts-tab"):
                yield Static("Competing edits are retained and excluded from reports until resolved.")
                yield DataTable(id="conflicts", cursor_type="row")
                with Horizontal(classes="buttons"):
                    yield Button("Review & resolve", id="resolve", variant="primary")
        with Vertical(id="bottom-bar"):
            yield Static("Opening local database…", id="sync-status", markup=False)
            yield Footer()

    async def on_mount(self):
        for name in ("history", "trash", "conflicts"):
            self.query_one(f"#{name}", DataTable).add_columns("Date", "Activity", "Minutes", "Comment", "State")
        self.set_interval(0.2, self.tick)
        self.set_interval(2, self.poll_history)
        if self.auto_sync:
            self.set_interval(15, self.sync_remote)
        try:
            await asyncio.to_thread(self.store.initialize)
            self.ready = True
            await self.load_history()
            if self.auto_sync:
                self.sync_remote()
        except Exception as error:
            self.error = str(error)
            self.notify(str(error), severity="error", timeout=15)
            self.render_status()

    def render_status(self):
        status = self.store.status()
        pending = status.get("pending_operations")
        text = "Local only" if status.get("mode") == "local" else (
            f"Last sync: {status.get('last_success') or 'never'} · "
            f"Pending operations: {pending if pending is not None else 'unknown'}")
        if self.syncing:
            text += " · Syncing…"
        error = self.error or self.sync_error or status.get("error")
        if error:
            text += f"\n{error}"
        self.query_one("#sync-status", Static).update(text)

    def poll_history(self):
        if self.ready and not self.loading:
            self.reload_history()

    @work(group="refresh", exit_on_error=False)
    async def reload_history(self):
        if self.ready:
            await self.load_history()

    async def load_history(self):
        async with self.history_lock:
            self.loading = True
            try:
                self.entries = await asyncio.to_thread(self.store.entries)
                self.error = None
                self.render_history()
            except Exception as error:
                self.error = str(error)
            finally:
                self.loading = False
                self.render_status()

    def filters(self):
        return dict(search=self.query_one("#search", Input).value,
                    activity=self.query_one("#filter-activity", Input).value,
                    since=self.query_one("#since", Input).value,
                    until=self.query_one("#until", Input).value)

    def active_entries(self):
        return filtered(self.entries, **self.filters())

    def render_history(self):
        try:
            active = self.active_entries()
        except ValueError as error:
            self.query_one("#filter-error", Static).update(str(error))
            return
        self.query_one("#filter-error", Static).update("")
        for name, entries in (("history", active), ("trash", filtered(self.entries, view="trash")),
                              ("conflicts", filtered(self.entries, view="conflicts"))):
            table = self.query_one(f"#{name}", DataTable)
            selected = self.selected_id(name)
            table.clear()
            for entry in entries:
                r = entry.heads[0]
                s = r.session
                state = f"CONFLICT ({len(entry.heads)})" if entry.conflicted else "Trash" if r.deleted else "Active"
                table.add_row(s.date, Text(s.activity), f"{s.duration_minutes:.2f}", Text(s.comment), state, key=entry.session_id)
            if selected and selected in table.rows:
                table.move_cursor(row=table.get_row_index(selected))
        daily, weekly = totals(active)
        lines = ["DAILY TOTALS", *[f"{day}  {activity}: {minutes:.2f} min" for (day, activity), minutes in sorted(daily.items(), reverse=True)],
                 "", "WEEKLY TOTALS", *[f"{week}  {activity}: {minutes:.2f} min" for (week, activity), minutes in sorted(weekly.items(), reverse=True)]]
        self.query_one("#reports", Static).update("\n".join(lines))

    def on_input_changed(self, event):
        if event.input.id in {"search", "filter-activity", "since", "until"} and self.ready:
            self.render_history()

    def selected_id(self, name):
        table = self.query_one(f"#{name}", DataTable)
        if not table.row_count:
            return None
        return table.coordinate_to_cell_key(table.cursor_coordinate).row_key.value

    def selected(self, name="history"):
        sid = self.selected_id(name)
        return next((e for e in self.entries if e.session_id == sid), None)

    def editor_done(self, saved):
        if saved:
            self.notify("Saved locally")
            self.reload_history()
            if self.auto_sync:
                self.sync_remote()

    def action_add(self):
        if self.ready:
            self.push_screen(Editor(), self.editor_done)

    def action_edit(self):
        entry = self.selected()
        if entry:
            self.push_screen(Editor(entry, resolving=entry.conflicted), self.editor_done)

    def action_delete(self):
        entry = self.selected()
        if not entry:
            return
        if entry.conflicted:
            self.notify("Resolve competing versions before deleting", severity="warning")
            return
        self.push_screen(Question("Move this session to Trash on all synced devices?"),
                         lambda result: self.change_deleted(entry, True) if result == "yes" else None)

    def action_restore(self):
        entry = self.selected("trash")
        if entry:
            self.change_deleted(entry, False)

    @work(exit_on_error=False)
    async def change_deleted(self, entry, deleted):
        try:
            await asyncio.to_thread(self.store.change, entry.session_id, entry.head_ids, deleted=deleted)
            self.editor_done(True)
        except Exception as error:
            self.notify(str(error), severity="error")

    def action_reload(self):
        self.reload_history()

    def action_sync_now(self):
        self.sync_remote()

    @work(group="sync", exit_on_error=False)
    async def sync_remote(self):
        if self.syncing or not self.ready:
            return
        self.syncing = True
        self.render_status()
        try:
            await asyncio.to_thread(self.store.sync)
            self.sync_error = None
        except Exception as error:
            self.sync_error = str(error)
        finally:
            self.syncing = False
            await self.load_history()

    def start_timer(self):
        if not self.ready or self.timer is not None:
            return
        try:
            number = lambda name: float(self.query_one(f"#timer-{name}", Input).value)
            work_minutes, short, long = number("work"), number("short"), number("long")
            cycles = int(self.query_one("#timer-cycles", Input).value)
            # Reuse Timer's finite/positive validation for every duration.
            for minutes in (work_minutes, short, long):
                Timer(minutes * 60)
            if cycles <= 0:
                raise ValueError("Cycles must be positive")
            activity = self.query_one("#activity", Input).value
            comment = self.query_one("#comment", Input).value
            endless = self.query_one("#endless", Checkbox).value
            self.settings = (work_minutes, short, long, cycles, activity, comment, endless)
            self.cycle = 1
            self.phase = "Work"
            self.timer = Timer(None if endless else work_minutes * 60, activity=activity, comment=comment)
            self.timer_save_failed = False
            self.query_one("#start", Button).disabled = True
        except ValueError as error:
            self.notify(str(error), severity="error")

    def action_pause_timer(self):
        if self.timer and not self.timer.finished and not self.saving_timer:
            self.timer.resume() if self.timer.paused else self.timer.pause()

    def tick(self):
        if not self.timer:
            return
        timer = self.timer
        elapsed = int(timer.elapsed)
        remaining = "endless" if timer.remaining is None else f"{int(timer.remaining) // 60:02d}:{int(timer.remaining) % 60:02d}"
        state = "Saving…" if self.saving_timer else "Save failed — retry Save" if self.timer_save_failed else "Paused" if timer.paused else "Running"
        self.query_one("#timer-display", Static).update(
            f"{self.phase} · cycle {self.cycle} · {state}\n"
            f"Elapsed {elapsed // 60:02d}:{elapsed % 60:02d} · Remaining {remaining}")
        if timer.complete and not self.saving_timer and not self.timer_save_failed:
            self.finish_timer(advance=True)

    @work(group="timer-save", exit_on_error=False)
    async def finish_timer(self, *, advance=False, exiting=False):
        if not self.timer or self.saving_timer:
            return
        self.saving_timer = True
        self.timer_save_failed = False
        try:
            if self.phase == "Work":
                save = self.timer.save()
                if save:
                    await asyncio.to_thread(self.store.create, save.session, session_id=save.session_id)
                    self.notify("Work saved locally")
                    self.reload_history()
                    if self.auto_sync:
                        self.sync_remote()
            else:
                self.timer.discard()
            if exiting:
                self.exit()
                return
            if advance:
                self.advance_timer()
                self.play_completion_sound()
            else:
                self.reset_timer()
        except Exception as error:
            self.timer_save_failed = True
            self.notify(f"Session retained for retry: {error}", severity="error", timeout=10)
        finally:
            self.saving_timer = False

    @work(group="sound", exit_on_error=False)
    async def play_completion_sound(self):
        if not await asyncio.to_thread(play_sound, terminal_fallback=False):
            self.bell()

    def advance_timer(self):
        work_minutes, short, long, cycles, activity, comment, endless = self.settings
        if self.phase == "Work":
            if self.cycle >= cycles:
                self.reset_timer()
                return
            self.phase = "Long break" if self.cycle % 4 == 0 else "Break"
            self.timer = Timer((long if self.cycle % 4 == 0 else short) * 60)
        else:
            self.cycle += 1
            self.phase = "Work"
            self.timer = Timer(work_minutes * 60, activity=activity, comment=comment)

    def reset_timer(self):
        self.timer = None
        self.query_one("#start", Button).disabled = False
        self.query_one("#timer-display", Static).update("Ready to focus")

    def discard_timer(self, result):
        if result == "yes" and self.timer and not self.saving_timer:
            self.timer.discard()
            self.reset_timer()

    def action_quit(self):
        self.action_request_quit()

    def action_request_quit(self):
        if self.saving_timer or (isinstance(self.screen, Editor) and self.screen.saving):
            self.notify("Wait for the local save to finish")
        elif self.timer:
            self.push_screen(Question("An active timer is local to this computer. What should happen?", exiting=True), self.quit_choice)
        else:
            self.exit()

    def quit_choice(self, result):
        if result == "yes":
            self.finish_timer(exiting=True)
        elif result == "discard":
            self.timer.discard()
            self.exit()

    def on_button_pressed(self, event):
        identifier = event.button.id
        if identifier == "start":
            self.start_timer()
        elif identifier == "pause":
            self.action_pause_timer()
        elif identifier == "save":
            self.finish_timer()
        elif identifier == "discard" and self.timer and not self.saving_timer:
            self.push_screen(Question("Discard this timer without saving elapsed work?"), self.discard_timer)
        elif identifier == "add":
            self.action_add()
        elif identifier == "edit":
            self.action_edit()
        elif identifier == "delete":
            self.action_delete()
        elif identifier == "restore":
            self.action_restore()
        elif identifier == "resolve":
            entry = self.selected("conflicts")
            if entry:
                self.push_screen(Editor(entry, resolving=True), self.editor_done)
        elif identifier in {"export", "report-export"}:
            try:
                self.push_screen(ExportDialog(self.active_entries()),
                                 lambda done: self.notify("CSV exported") if done else None)
            except ValueError as error:
                self.notify(str(error), severity="error")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--db", type=Path, help="Local replica path (unique to this computer)")
    parser.add_argument("--local", action="store_true", help="Use a separate, unsynced SQLite database")
    args = parser.parse_args()
    if args.local and args.db is None:
        parser.error("--local requires --db so the Turso replica is not edited accidentally")
    Snapo(Store(Config.load(args.db, args.local))).run()


if __name__ == "__main__":
    main()
