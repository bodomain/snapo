# Snapo

A personal Linux productivity timer and session workspace. The Textual TUI
combines a timer, editable history, daily/weekly reports, CSV export, Trash, and
conflict review. Each computer has its own local Turso replica; session history
syncs through your Turso database. Active timers remain local.

## Install and try it

Requires Python 3.10+ and a Linux terminal. A terminal at least 100×30 works best.

```bash
uv venv .venv
uv pip install --python .venv/bin/python -r requirements.txt
# Try the complete interface against separate local-only demo data:
.venv/bin/python app.py --local --db /tmp/snapo-demo.db
```

For synced use, copy `.env.example` to `.env` and set your database URL, token,
and absolute local database path. Environment variables override `.env` values.
The old one-line token file is still supported. `.env` is loaded from the project
directory, so launching from another directory works.

```bash
.venv/bin/python app.py
# Or choose this computer's local replica explicitly:
.venv/bin/python app.py --db /absolute/path/to/prodz.db
```

The first connection needs internet access to bootstrap a replica. Later local
reads and saves work offline; the TUI attempts sync after saves and every 15
seconds. Missing credentials never silently switch a Turso replica to plain
SQLite. `--local` requires an explicit database path and refuses replica files.

## Workspace

- **Timer:** activity/comment, work and break durations, cycle count, endless
  mode, pause/resume, save/stop, and discard. Pauses do not count toward work.
  Completed work saves once; breaks produce no session. A long break follows
  every fourth work cycle if more cycles remain. The final cycle ends without
  an extra break. Closing with Ctrl+Q or Ctrl+C prompts to save, discard, or
  cancel. Work and break completion play `bell.wav` in the background,
  with terminal bell fallback. A failed save retains the timer for retry. Terminal/process crashes
  do not preserve an active timer.
- **History:** browse timestamps through the editor, search activity/comments/
  timestamps, filter activity and an inclusive date range, add/edit sessions,
  or move them to Trash. History and report exports use the History filters.
- **Reports:** daily and ISO-week totals by activity. Days follow each session's
  recorded date, including when viewed from another timezone. Deleted sessions
  and unresolved conflicts do not contribute to totals.
- **Trash:** deletion syncs as a reversible revision. Restore a selected session
  to return it to history on all computers. There is no permanent-delete action.
- **Conflicts:** concurrent offline edits, including delete-versus-edit, retain
  all versions. Choose a version as a starting point or correct its fields,
  choose active/Trash state, then save to resolve the displayed versions.
  A newer edit arriving while a form is open requires reopening the form.
- **Sync status:** last successful sync, queued operations, and connectivity
  errors. “Saved locally” does not mean “uploaded.” Pending operations count
  driver operations, not sessions. Local-only mode is labeled explicitly.

Shortcuts: **Ctrl+N** add, **Ctrl+E** edit selected history row, **Ctrl+D** move to
Trash, **Ctrl+R** restore selected Trash row, **Ctrl+S** sync, **F5** reload,
**F2** pause/resume, **Ctrl+Q** quit. Buttons provide the same actions. Standard
Tab/Shift+Tab navigation works in forms. Escape cancels a dialog.

CSV exports contain active, non-conflicted sessions, include IDs and timestamps,
and refuse to overwrite existing files. Free-text spreadsheet formula prefixes
are escaped with an apostrophe. Exports are not a database backup.

## Upgrade an existing database

Revision history is a schema change. Old clients cannot safely write alongside
new ones. Perform the migration once from one designated computer, then
bootstrap fresh replicas on your other computers.

1. Stop the old timer and sync worker **on every computer**:

   ```bash
   systemctl --user stop prodz-sync.service
   ```

2. Configure the designated computer to use the existing database. Check the
   migration; the default command does not apply schema changes or upload data:

   ```bash
   .venv/bin/python migrate_db.py
   ```

3. Resolve any reported legacy record or metadata differences before proceeding.
   The check compares local records with an independently bootstrapped remote
   replica. Matching records are preserved; local-only and remote-only records
   are included in the merge. A conflicting ID stops migration instead of
   choosing a winner. A damaged replica must be recovered separately.

4. Apply after all old clients are stopped:

   ```bash
   .venv/bin/python migrate_db.py --apply --old-clients-stopped
   ```

   Before applying, it backs up the database and its sidecars to
   `backups/migration-<timestamp>/`. It preserves the original `sessions` table
   and other metadata, imports deterministic revision/session IDs from one
   shared dataset UUID, uploads the revisions, and verifies remote contents.
   Repeating a completed migration does not duplicate sessions. If upload or
   verification fails, keep the backup, fix connectivity, rerun the same migration command to retry upload and independent remote
   verification before cutover. Never restore an old
   replica over an active revised database.

5. Start the TUI and replace the background service with one generated for your
   current checkout and Python environment:

   ```bash
   .venv/bin/python install_service.py          # preview the unit
   .venv/bin/python install_service.py --install
   .venv/bin/python app.py
   ```

For local-only legacy files use `--local --db /path/to/file.db` on migration
commands. Never use plain SQLite to modify a Turso replica.

## Additional computers

Install the same version, configure the same Turso URL with an appropriate
credential, and choose a **new local database path**. Start `app.py` online;
it bootstraps the shared revision history. Install the generated background
service if you want sync while the TUI is closed.

Do not copy a running replica, its sidecar files, or its `.device.json` identity
between computers. Do not migrate independently from a second old replica.
Reconcile any remaining old local-only data before bootstrap. Runtime databases,
sidecars, identities, status, backups, and credentials must stay out of Git.
Previously tracked database files need to be untracked explicitly during rollout;
ignore rules alone do not remove files from Git history.

## Background synchronization

```bash
.venv/bin/python sync_db.py --once
.venv/bin/python sync_db.py --interval 15
systemctl --user status prodz-sync.service
journalctl --user -u prodz-sync.service
systemctl --user disable --now prodz-sync.service
```

The service starts at login and runs during your user session. It serializes
access with other clients on that computer, retries network failures, and writes
status beside the replica. It does not replace or reset damaged replicas.
Regenerate/reinstall the unit if the project or environment moves. The TUI
also syncs when open; cross-process locking serializes both callers.

The checked-in `prodz-sync.service` retains compatibility with the original
checkout location. Use `install_service.py` for a portable installation.

## Existing CLI

```bash
.venv/bin/python prodz_cli.py -w 25 -b 5 -lb 15 -c 4 -a 'Python study' -t 'Chapter 2'
.venv/bin/python prodz_cli.py --endless -a 'Reading'
.venv/bin/python plot.py
```

The CLI now uses the same monotonic timer and revision-backed session store.
Press `p` to pause/resume, `l` to save and exit, or `q` to discard and exit.
It queues local writes; use the background worker for uploads while it is closed.
Set `SNAPO_LOCAL=1` and `SNAPO_DB_PATH` for a separate local-only CLI database.
Sound notifications use `aplay`, `paplay`, or `ffplay`, with terminal bell fallback.

## Development and validation

```bash
uv pip install --python .venv/bin/python -r requirements-dev.txt
.venv/bin/python -m pytest
```

Tests cover monotonic timing, partial and idempotent saves, actual Turso engine
SQL compatibility, immutable revision convergence in both delivery orders,
concurrent resolutions, Trash/restore, filters, CSV, migration/backup, and TUI
keyboard/mouse workflows. Every default test uses temporary local data.

Cloud transport acceptance tests are opt-in and never load the project's token.
Provide credentials for a **separate disposable database**, then run:

```bash
# Set these securely in your environment, not in committed files:
# SNAPO_TEST_TURSO_URL, SNAPO_TEST_TURSO_TOKEN
SNAPO_TEST_REMOTE_OK=1 .venv/bin/python -m pytest -m remote
```

These tests create real history in the disposable database. They exercise two
independent replicas, offline edits, delete conflicts, resolution, restoration,
reopening, and both synchronization orders. Delete the disposable database
through your Turso account afterward. Without credentials they skip, so local
passes alone do not certify the cloud transport.

The shared modules are `timer.py` (timekeeping), `sessions.py` (revision history,
conflicts, reports, exports), and `database.py` (local operations and transport).
A future desktop app can use the same operations. Desktop/tray integration,
active-timer handoff, mobile access, and multi-user sharing are deferred.
