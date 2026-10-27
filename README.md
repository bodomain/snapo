# Aftermath

Aftermath ist eine native Rust-Desktop-App für Linux. Fokus-Timer, bearbeitbarer
Verlauf, Tages- und Wochenberichte, CSV-Export, Papierkorb und Konfliktauflösung
übernehmen die Funktionen der bisherigen Python-Version.

## Rust-App starten

Rust 1.88 oder neuer, eine grafische Linux-Sitzung und die Entwicklungsbibliotheken
für ALSA sind erforderlich. Unter Debian/Ubuntu: `sudo apt install build-essential
pkg-config libasound2-dev`. Unter Arch Linux: `sudo pacman -S base-devel alsa-lib`.

```bash
cargo run --release --locked
# Mit einer separaten Datenbank ausprobieren:
cargo run --locked -- --local --db /tmp/aftermath-desktop-demo.db
```

Die App arbeitet standardmäßig lokal. Ihre Datenbank liegt unter
`~/.local/share/snapo/snapo-desktop.db` (bzw. unter `$XDG_DATA_HOME`). Mit `--db`
oder `AFTERMATH_DESKTOP_DB` lässt sich der Pfad ändern. `SNAPO_DB_PATH` gehört zum
Python-Client und wird vom Desktop bewusst nicht als Datenbankpfad verwendet.
Der bisherige Standardpfad und `SNAPO_DESKTOP_DB` bleiben für bestehende
Installationen kompatibel. Auch Timer-Einstellungen bleiben beim Umbenennen erhalten.

Die Oberfläche bietet:

- **Fokus:** Aktivität und Notiz, Fokus- und Pausendauern, Zyklen, lange Pause
  nach jedem vierten Zyklus, Zeitmessung ohne Limit, Pause/Fortsetzen und Ton.
  Aktivität und Rhythmus lassen sich auch über eine Command-Line im Panel
  definieren – wie in der Python-CLI, z. B. `Lernen -w 30 -c 2 -t "Vokabeln"`
  (Optionen `-a`, `-t`, `-w`, `-b`, `-lb`, `-c`, `-e`).
  Nur Fokuszeit wird gespeichert. Fehlgeschlagene Speicherversuche lassen sich
  ohne doppelte Sitzungen wiederholen. Beim Schließen eines aktiven Timers
  kannst du speichern, verwerfen oder abbrechen. Ein Prozessabbruch erhält den
  aktiven Timer nicht.
- **Verlauf:** Sitzungen ergänzen und bearbeiten, inklusive Datum und
  Zeitstempel. Suche und inklusive Datums-/Aktivitätsfilter gelten auch für
  Stats und CSV-Export. Doppelklick auf einen Zeitstempel öffnet den Editor.
- **Stats:** Kennzahlen (Gesamtzeit, Sitzungen, Aktivitäten, Ø pro Tag, bester
  Tag, Tage in Folge), ein Kalenderjahr-Chart nach Monaten oder ISO-Wochen,
  gestapelte Tagesbalken mit einzelnen Sitzungssegmenten und Details beim
  Überfahren, Wochentag- und Tageszeit-Verteilung, eine Aktivitäten-Rangliste
  sowie Summen nach Tag oder ISO-Woche. Papierkorb und ungelöste Konflikte
  werden ausgeschlossen.
- **Papierkorb und Konflikte:** reversible Löschung/Wiederherstellung sowie
  Vergleich und Zusammenführung paralleler Versionen. Zwischenzeitliche
  Änderungen können einen geöffneten Editor nicht unbemerkt überschreiben.
- **Einstellungen:** Datenbankprüfung, Import, Verbindungsstatus, Hell/Dunkel-
  Darstellung und Tastenkürzel. Timer-Einstellungen bleiben zwischen Starts
  erhalten.

`F2`: Pause/Fortsetzen · `Strg+N`: ergänzen · `Strg+E`: bearbeiten · `Strg+D`:
Papierkorb · `Strg+R`: wiederherstellen · `Strg+S`: synchronisieren · `F5`:
neu laden · `Strg+Q`: beenden · `Escape`: Dialog abbrechen. Tab/Shift+Tab
funktionieren in Formularen.

## Rust-App mit Turso verbinden

Setze `TURSO_DATABASE_URL` und `TURSO_AUTH_TOKEN` in `.env` im Projektordner
oder als Umgebungsvariablen. Umgebungsvariablen haben Vorrang. Die alternative
Option `--env-file /pfad/zu/.env` funktioniert auch für eine installierte App.
Die Zugangsdaten werden nicht in den Oberflächeneinstellungen gespeichert.

```bash
cargo run --release --locked -- --sync
# Expliziter Pfad pro Gerät:
cargo run --release --locked -- --db /pfad/aftermath-desktop.db --sync
```

Alternativ lässt sich die konfigurierte Verbindung in den Einstellungen für die
aktuelle Sitzung einschalten. Für automatischen Sync bei jedem Start verwende
`--sync`. Synchronisiert wird nach Änderungen und alle 15 Sekunden im Hintergrund;
Timer und Oberfläche bleiben bedienbar. Die Statusleiste zeigt ausstehende
Revisionen, den letzten Erfolg und Fehler. Offline-Arbeit bleibt lokal gespeichert
und wird beim nächsten erfolgreichen Abgleich hochgeladen.

Die Rust-App verwendet eine eigene SQLite-Datei und überträgt unveränderliche
Revisionen über [Tursos Hrana-HTTP-Protokoll](https://github.com/tursodatabase/libsql/blob/main/docs/HTTP_V2_SPEC.md).
Sie benötigt eine vorhandene Cloud-Datenbank mit kompatiblem Schema **v2**. Zur
Migration einer alten Cloud-Datenbank gilt weiterhin der unten beschriebene
Python-Migrationsablauf. Die Rust-App migriert eine Cloud-Datenbank nicht beim
Start. **Turso-Replikatdateien des Python-Clients werden niemals mit dem
SQLite-Treiber geöffnet.** Noch nicht hochgeladene Änderungen solcher Replikate
zuerst mit dem Python-Client synchronisieren. Danach lädt die Rust-App den
Cloud-Verlauf einschließlich IDs, Papierkorb und Konflikten.

Migrierte Alt-Datensätze werden wie im Python-Client unverändert übernommen,
auch wenn alte Zeitstempel fehlerhaft sind. Der Verlauf markiert sie als
„Altdaten prüfen“. Sie lassen sich löschen und wiederherstellen; beim Bearbeiten
müssen die Eingaben den aktuellen Validierungsregeln entsprechen.

Beim ersten expliziten Abgleich wird die Desktop-Datenbank an den Cloud-Datensatz
gebunden; vorher erfasste lokale Sitzungen werden mit übertragen. Ein späterer
Wechsel zu einem anderen Datensatz wird abgelehnt. Dafür eine neue Desktop-Datei
wählen. Jeder Rechner benötigt seine eigene Datei und Geräteidentität.

## Bestehende lokale Python-Daten übernehmen

Unter Einstellungen → „Python-Datenbank importieren“ kann eine **normale
SQLite-Datei** ausgewählt werden. Alternativ:

```bash
cargo run --locked -- --db /pfad/neue-desktop.db import /pfad/python-local.db
```

Der Import liest die Quelle in einer Transaktion und verändert sie nicht.
V2-Dateien behalten ihre Datensatz-, Sitzungs- und Revisions-IDs, einschließlich
aller Konfliktversionen. Sie dürfen nur in eine neue Desktop-Datenbank oder
denselben Datensatz importiert werden. Alte `sessions`-Tabellen werden ebenfalls
übernommen; deren IDs hängen vom absoluten Quellpfad und der ursprünglichen
Zeilen-ID ab. Wiederholte Importe derselben unveränderten Quelle sind idempotent.
Wurde eine bereits importierte alte Zeile anschließend in Python geändert, bricht
der Import ab, statt die Änderung unbemerkt zu überschreiben. Zum gemeinsamen
Cloud-Betrieb zuerst die Python-v2-Migration verwenden, damit beide Clients
dieselben IDs erhalten.

## Installation und Befehle

```bash
# Baut die Release-App und legt einen Eintrag im Linux-App-Menü an:
./scripts/install-desktop.sh
# Danach:
~/.local/bin/aftermath
~/.local/bin/aftermath --sync --env-file /pfad/zu/.env

# Weitere Befehle ohne Fenster:
cargo run --locked -- --db /tmp/aftermath-desktop-demo.db check
cargo run --locked -- --db /tmp/aftermath-desktop-demo.db history
cargo run --locked -- --db /tmp/aftermath-desktop-demo.db export /tmp/aftermath-export.csv
cargo run --locked -- --db /pfad/aftermath-desktop.db sync
cargo run --locked -- --db /pfad/aftermath-desktop.db sync --interval 15
# Cloud-Verbindung/Schema prüfen, ohne die Cloud zu verändern:
cargo run --locked -- remote-check
```

Ein CSV-Export überschreibt keine vorhandene Datei und schützt Freitext vor
Tabellenkalkulationsformeln. Er enthält aktive, konfliktfreie Sitzungen und ist
keine vollständige Datensicherung.

Für Synchronisierung bei geschlossenem Fenster kann der Rust-Prozess als eigener
Benutzerdienst installiert werden. Das Skript zeigt standardmäßig nur die Unit:

```bash
./scripts/install-sync-service.sh /pfad/aftermath-desktop.db /pfad/zu/.env
./scripts/install-sync-service.sh /pfad/aftermath-desktop.db /pfad/zu/.env --install
systemctl --user status aftermath-desktop-sync.service
```

## Rust-Entwicklung und Prüfung

```bash
cargo fmt --all -- --check
cargo clippy --locked --all-targets -- -D warnings
cargo test --locked --all-targets
cargo build --release --locked
# Das echte Desktop-Fenster aufnehmen und anschließend schließen:
cargo run --locked -- --local --db /tmp/aftermath-screen.db --screenshot /tmp/aftermath.png
# Bestimmte Ansicht öffnen oder aufnehmen:
cargo run --locked -- --view stats --screenshot /tmp/aftermath-stats.png
```

Rust-Tests verwenden ausschließlich temporäre lokale Daten und einen lokalen
HTTP-Testserver mit SQLite. Sie prüfen Timer und Pausen, Speicherversuche,
Zyklen, Revisionskonvergenz, Konflikte, veraltete Formulare, Papierkorb,
Wiederherstellung, Filter, Stats, CSV, Import sowie den HTTP-Abgleich zweier
unabhängiger Desktop-Dateien. Oberflächentests bedienen echte egui-Schaltflächen
und Tastenkürzel und prüfen kleine sowie große Fenster. Diese Tests verwenden
keine Projekt-Zugangsdaten; ein erfolgreicher lokaler Test ist kein Nachweis
eines Abgleichs mit einem echten Turso-Konto.

Die Desktop-Implementierung liegt in `src/`: `timer.rs`, `history.rs`, `store.rs`,
`sync.rs`, `config.rs`, `ui.rs` und `main.rs`. Schriften sind eingebettet; ihre
Lizenz steht in `assets/FONT-LICENSE`.

## Bisherige Python-Version

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
- **Stats:** key figures (total, sessions, activities, daily average, best day,
  streak), a calendar-year chart by month or ISO week, daily stacked activity
  charts with colored session segments and a legend, weekday and time-of-day
  distribution, an activity ranking, plus daily and ISO-week totals by
  activity. Charts use the History filters. Days follow each session's recorded
  date, including when viewed from another timezone. Deleted sessions and
  unresolved conflicts do not contribute to totals.
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
The native Rust desktop app described above uses compatible revision history.
Tray integration, active-timer handoff, mobile access, and multi-user sharing are deferred.
