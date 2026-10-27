# Aftermath aus Entwicklersicht

Ein Lehrbuch für Rust-Einsteiger darüber, wie die native Desktop-App
**Aftermath** aufgebaut ist und welche Bibliotheken („Frameworks“) sie nutzt.

Dieses Dokument setzt nur grundlegende Programmierkenntnisse voraus. Rust-
Kenntnisse werden nicht erwartet; die wichtigsten Sprachkonzepte werden dort
erklärt, wo sie im Code auftauchen. Alles, was hier steht, lässt sich mit den
Dateien in `src/` und `tests/` nachvollziehen.

---

## Inhalt

1. [Was ist Aftermath?](#1-was-ist-aftermath)
2. [Wird ein Framework verwendet?](#2-wird-ein-framework-verwendet)
3. [Der Technologie-Stack im Detail](#3-der-technologie-stack-im-detail)
4. [Wie ein Rust-Projekt aufgebaut ist](#4-wie-ein-rust-projekt-aufgebaut-ist)
5. [Die Module im Einzelnen](#5-die-module-im-einzelnen)
6. [Lebenszyklus einer Sitzung](#6-lebenszyklus-einer-sitzung)
7. [Das Datenmodell: Revisionen statt Zeilen](#7-das-datenmodell-revisionen-statt-zeilen)
8. [Nebenläufigkeit: Timer, UI und Sync](#8-nebenläufigkeit-timer-ui-und-sync)
9. [Rust-Konzepte, die du hier siehst](#9-rust-konzepte-die-du-hier-siehst)
10. [Fehlerbehandlung](#10-fehlerbehandlung)
11. [Tests, Build und Werkzeuge](#11-tests-build-und-werkzeuge)
12. [Übungsaufgaben](#12-übungsaufgaben)
13. [Glossar](#13-glossar)

---

## 1. Was ist Aftermath?

Aftermath ist eine **native Linux-Desktop-Anwendung** für Fokus-Zeiten
(Pomodoro-Stil). Sie kann:

- einen Fokus-Timer mit Pausen und Zyklen laufen lassen,
- Sitzungen (Aktivität, Dauer, Notiz, Zeitstempel) speichern und bearbeiten,
- Tages- und Wochenberichte anzeigen und als CSV exportieren,
- Sitzungen in einen Papierkorb verschieben und wiederherstellen,
- parallele Änderungen (Konflikte) auflösen,
- die Daten über die Cloud (Turso) zwischen mehreren Geräten abgleichen.

Im selben Repository liegt außerdem die **ältere Python-Version** (`app.py`,
`sessions.py`, `database.py`, …). Dieses Dokument beschäftigt sich **nur mit der
Rust-Anwendung in `src/`**.

Die Rust-App ist ein direkter Nachbau der Python-Funktionen. Wichtig für das
Verständnis: Das Datenformat ist **kompatibel** zur Python-Version (die
Kommentare im Code sagen „wire-compatible“). Die Rust-App ist also nicht nur
ein Programm, sondern muss dieselben Daten lesen und schreiben können wie ein
zweites Programm in einer anderen Sprache.

---

## 2. Wird ein Framework verwendet?

Ja – genau **ein** großes Framework: **egui/eframe** für die grafische
Oberfläche.

Alles andere sind **Bibliotheken** („Crates“), die einzelne Aufgaben lösen. Der
Unterschied ist wichtig:

| Begriff | Bedeutung |
| --- | --- |
| **Framework** | Gibt den Kontrollfluss vor. *Dein* Code füllt vorher definierte Lücken. Bei egui ruft das Framework deine `update`-Funktion in einer Endlosschleife auf. |
| **Bibliothek** | Wird von deinem Code aufgerufen, wenn du sie brauchst. SQLite, HTTP, CSV usw. sind Bibliotheken: Du entscheidest, wann sie laufen. |

Es gibt **kein** Web-Framework (kein Axum, Rocket, Actix), **keinen**
Async-Runtime (kein Tokio), **kein** ORM und **keine** Web-UI. Die App ist ein
reines Desktop-Programm. Die Cloud-Kommunikation läuft synchron über
`reqwest::blocking`.

> **Merksatz:** egui ist das einzige Framework. Die restlichen Crates sind
> Werkzeuge, die die App bei Bedarf benutzt.

---

## 3. Der Technologie-Stack im Detail

Die Abhängigkeiten stehen in [`Cargo.toml`](../Cargo.toml). So hängt alles
zusammen:

| Crate | Aufgabe | Wo verwendet |
| --- | --- | --- |
| `eframe` / `egui` | GUI-Framework. Fenster, Widgets, Layout, Zeichnen. Renderer: `glow` (OpenGL). | `ui.rs`, `main.rs` |
| `rusqlite` | SQLite-Treiber. Die App speichert lokal in einer SQLite-Datei. Mit Feature `bundled` bringt sie SQLite selbst mit. | `store.rs` |
| `reqwest` | HTTP-Client (hier im `blocking`-Modus) für die Turso-Cloud. `rustls-tls` = TLS ohne System-OpenSSL. | `sync.rs` |
| `serde` / `serde_json` | Serialisierung: Rust-Structs ⇄ JSON. Wird für die Datenbank-Payloads, `.env`-Werte und die Cloud-API gebraucht. | überall |
| `chrono` | Datum und Uhrzeit (Zeitstempel, Tages-/Wochenberechnung). | `history.rs`, `timer.rs`, `store.rs` |
| `clap` | Parser für Kommandozeilen-Argumente und Unterbefehle (`derive`-Stil). | `main.rs`, `ui.rs` |
| `anyhow` | Bequeme Fehlerbehandlung (`Result`, `Context`, `bail!`, `ensure!`). | überall |
| `uuid` | Erzeugt IDs (Version 4 zufällig, Version 5 deterministisch). | `store.rs`, `timer.rs` |
| `rodio` | Audio-Wiedergabe für den Pausenton. | `timer.rs` |
| `image` | Schreibt Bilder (hier: PNG). Wird für den `--screenshot`-Modus gebraucht. | `ui.rs` |
| `csv` | CSV-Export des Verlaufs. | `history.rs` |
| `dotenvy` | Liest `.env`-Dateien. | `config.rs` |
| `directories` | Findet Standard-Ordner (`~/.local/share`, Home-Verzeichnis). | `config.rs`, `ui.rs` |
| `tempfile` (dev) | Temporäre Verzeichnisse in Tests. | `tests/`, `ui.rs`-Tests |

Drei Design-Entscheidungen fallen auf:

1. **Kein Async.** Statt einer Async-Runtime wird ein normaler Thread
   gestartet und das Ergebnis über einen Kanal (`mpsc`) zurückgegeben
   (siehe Kapitel 8). Das ist für Einsteiger deutlich leichter zu lesen.
2. **Kein ORM.** SQL wird direkt als Text geschrieben (`store.rs`). Du siehst
   genau, welche Abfrage läuft.
3. **`bundled` SQLite und eingebettete Schriften.** Die App lädt Schriften und
   SQLite in die Binary, damit sie ohne Zusatzinstallation läuft.

---

## 4. Wie ein Rust-Projekt aufgebaut ist

Ein Rust-Projekt heißt **Crate**. Diese App ist eine Crate mit **zwei
Einstiegspunkten**:

```
Cargo.toml          – Manifest: Name, Version, Abhängigkeiten, Profile
Cargo.lock          – exakte Versionen aller Abhängigkeiten (reproduzierbare Builds)
src/
  main.rs           – Binärprogramm: der Startpunkt (`fn main`)
  lib.rs            – Bibliothek: listet die Module auf
  command.rs        – Command-Line-Syntax für das Fokus-Panel (prodzCLI-Stil)
  config.rs         – DB-Pfad und Cloud-Zugangsdaten
  history.rs        – Datenmodell + CSV
  stats.rs          – Aggregationen für die Stats-Seite
  store.rs          – SQLite-Persistenz
  sync.rs           – Cloud-Abgleich über HTTP
  timer.rs          – Timer-Logik
  ui.rs             – grafische Oberfläche (das größte Modul)
tests/
  desktop.rs        – Integrationstests (Rust)
  test_*.py         – Tests der alten Python-Version
assets/             – Schriften und Icon (werden in die Binary eingebettet)
scripts/            – Installations-Skripte
```

**Warum `main.rs` *und* `lib.rs`?** Das ist ein übliches Muster:

- `lib.rs` definiert eine **Bibliothek** (`aftermath`), die von außen nutzbar
  ist. `tests/desktop.rs` beginnt mit `use aftermath::…` – die Tests greifen
  also auf dieselbe Bibliothek zu wie das Programm.
- `main.rs` ist das **ausführbare Programm**. Es benutzt die Bibliothek nur.

`lib.rs` ist winzig – es macht die Module öffentlich:

```rust
pub mod command;
pub mod config;
pub mod history;
pub mod store;
pub mod sync;
pub mod timer;
pub mod ui;
```

Der Ausdruck `pub mod` bedeutet: „Dieses Modul ist auch von außerhalb der
Crate sichtbar.“ Fehlt `pub`, ist das Modul privat.

### Edition und Rust-Version

`Cargo.toml` legt `edition = "2024"` und `rust-version = "1.88"` fest. Die
*Edition* ist die Sprachversion (vergleichbar mit C++-Standards); sie
aktiviert modernere Syntax. Im Code siehst du zum Beispiel „let-chains“:

```rust
if let Some(suffix) = path.strip_prefix("~/")
    && let Some(dirs) = directories::UserDirs::new()
{
    // ...
}
```

Die `[profile.release]`-Einstellungen (`lto = "thin"`, `strip = true`) machen
das fertige Programm kleiner und schneller – relevant für die Auslieferung,
nicht für den Quellcode.

---

## 5. Die Module im Einzelnen

Die Module sind nach **Verantwortlichkeit** geschnitten. Jedes Modul hat eine
klare Aufgabe und möglichst wenige Abhängigkeiten. Die folgende Tabelle zeigt
die Abhängigkeitsrichtung – jeder Pfeil heißt „benutzt“:

```
main.rs ──▶ ui.rs ──▶ store.rs ──▶ history.rs
                │          │
                │          └──▶ sync.rs ──▶ history.rs, store.rs
                └──▶ config.rs ──▶ sync.rs
                └──▶ timer.rs ──▶ history.rs
```

`history.rs` steht ganz unten und hängt von keinem eigenen Modul ab – es ist
das Fundament (Datenmodell). `ui.rs` steht oben und darf alles benutzen.

### 5.1 `timer.rs` – die Zeitlogik

Hier lebt die reine Fachlogik des Timers, **ohne Bezug zur Oberfläche**.

- `Plan` – die Einstellungen: Arbeitsdauer, kurze/lange Pause, Zyklen,
  Endlosmodus, Ton an/aus.
- `Phase` – eine Aufzählung (`enum`) mit `Work`, `ShortBreak`, `LongBreak`.
- `Clock` – eine laufende Zeitmessung. Sie nutzt `std::time::Instant`, also
  eine **monotone** Uhr, die nicht von der Systemzeit verfälscht wird.
- `Run` – verbindet `Plan`, `Phase` und `Clock` zu einer Zustandsmaschine über
  mehrere Zyklen hinweg. `Run::advance` wechselt von Arbeit zu Pause und wieder
  zurück.
- `bell()` – spielt `bell.wav` in einem eigenen Thread ab; wenn kein
  Audiogerät da ist, sendet sie stattdessen das Terminal-Glockzeichen (`\x07`).
- `greeting()` – „Guten Morgen.“ / „Guten Tag.“ / „Guten Abend.“

Eine Besonderheit: `Clock` **friert** eine Sitzung ein, bevor sie gespeichert
wird (`snapshot`). Das bedeutet: Sollte das Speichern fehlschlagen und der
Benutzer es erneut versuchen, entstehen keine doppelten Sitzungen und die ID
bleibt gleich. Solche Nebenbedingungen sind typisch für Zeitmessung und
zeigen, warum man Logik von der Oberfläche trennt.

### 5.2 `history.rs` – das Datenmodell

Das wichtigste Modul für das Verständnis. Es definiert:

- `Session` – eine einzelne Sitzung: `date`, `timestamp`, `activity`,
  `duration_minutes`, `comment`.
- `Revision` – eine **unveränderliche Version** einer Sitzung (mehr dazu in
  Kapitel 7).
- `Entry` – die Sicht auf *eine* Sitzung, die aus einer oder mehreren
  Revisionen besteht (`heads`).
- `reconstruct()` – baut aus einer flachen Liste von Revisionen die
  `Entry`s.
- `Filter` – Suche und Datums-/Aktivitätsfilter, inklusive Prüfung der
  Eingaben (`validate`).
- `totals()` – rechnet Summen pro Tag oder ISO-Woche.
- `export_csv()` – schreibt aktive, konfliktfreie Sitzungen in eine CSV-Datei.

Zwei Details sind lehrreich:

- **Tabellenkalkulations-Schutz:** Freitext, der mit `=`, `+`, `-`, `@`, Tab
  oder `\r` beginnt, bekommt ein Apostroph vorangestellt, damit Excel & Co. ihn
  nicht als Formel ausführen.
- **Datei-Schutz:** `export_csv` öffnet die Datei mit `create_new(true)`, weigert
  sich also, eine vorhandene Datei zu überschreiben.

### 5.3 `store.rs` – die Persistenz

`Store` kapselt die SQLite-Datenbank. Er besitzt eine `Connection` (die
Verbindung), den `path` (Dateipfad) und die `device_id` des Geräts.

Die Tabelle `session_revisions` speichert Revisionen; `snapo_config` speichert
Metadaten (Schema-Version, Datensatz-ID, Geräte-ID). Die Struktur wird über
Konstanten wie `SCHEMA` beschrieben, damit sie an einer Stelle steht.

Wichtige Methoden:

| Methode | Zweck |
| --- | --- |
| `open` | Datenbank öffnen/erzeugen, Schema prüfen, WAL-Modus aktivieren |
| `check` | `PRAGMA integrity_check` + Prüfung aller Revisionen |
| `create` | neue Sitzung anlegen (erzeugt eine Wurzel-Revision) |
| `change` / `set_deleted` | Sitzung ändern bzw. in den Papierkorb legen |
| `merge` | fremde Revisionen aufnehmen (z. B. nach Import) |
| `merge_remote` | Cloud-Revisionen aufnehmen und Datensatz binden |
| `import` | eine Python-SQLite-Datei einlesen |
| `pending` | Anzahl noch nicht hochgeladener Revisionen |
| `acknowledge` | Revisionen als hochgeladen markieren |

Drei Sicherheitsnetze, die man in einem Einsteigerprojekt selten sieht,
hier aber bewusst eingebaut sind:

1. **`refuse_replica`** verweigert das Öffnen von Turso-Replikatdateien
   (erkannt anhand der Sidecar-Dateien). Die Cloud wird nie mit dem
   SQLite-Treiber „direkt“ beschrieben; nur die eigene Desktop-Datei.
2. **`busy_timeout`** und **`TransactionBehavior::Immediate`** verhindern
   Datenverlust bei gleichzeitigen Zugriffen.
3. Beim Import wird die Quelle **read-only** geöffnet und in einer Transaktion
   gelesen. Die Quelldatei wird nie verändert.

### 5.4 `sync.rs` – der Cloud-Abgleich

`sync.rs` nutzt das dokumentierte **Hrana-HTTP-Protokoll** von Turso, nicht
das SQLite-Dateiformat. Es baut JSON-„Pipelines“ aus SQL-Anweisungen:

```
{ "requests": [ {"type":"execute", "stmt": {...}}, ..., {"type":"close"} ] }
```

- `Remote` hält URL und Token der Cloud-Datenbank.
- `inspect()` ist **rein lesend** und prüft die Verbindung und das Schema.
- `synchronize()` lädt zuerst alle Cloud-Revisionen, führt sie lokal zusammen
  (`merge_remote`), ermittelt dann die lokal noch unbekannten Revisionen,
  lädt sie in Blöcken von 100 hoch, **verifiziert** sie und markiert sie erst
  danach als bestätigt.
- `safe_error()` entfernt das Token aus Fehlermeldungen, damit es nie in der
  Oberfläche oder in Logs landet.

Das schrittweise Vorgehen (laden → zusammenführen → senden → verifizieren →
bestätigen) ist der Kern der Zuverlässigkeit: Ein Abbruch mittendrin kann
höchstens dazu führen, dass etwas erneut gesendet wird – eine reine
Wiederholung, kein Datenverlust.

### 5.5 `config.rs` – Konfiguration

Zwei Aufgaben:

- `default_database()` bestimmt den Standardpfad der Datenbank über
  `directories::ProjectDirs` (unter Linux typischerweise
  `~/.local/share/snapo/snapo-desktop.db`).
- `remote()` liest `TURSO_DATABASE_URL` und `TURSO_AUTH_TOKEN`. Reihenfolge:
  Umgebungsvariablen **vor** `.env`-Datei. Der Kommentar im Code betont, dass
  die Prozessumgebung dabei nicht verändert wird.

### 5.6 `ui.rs` – die Oberfläche

Das größte Modul (≈1800 Zeilen) und das einzige, das `eframe`/`egui` benutzt.

- `Page` – die sechs Seiten der App (Fokus, Verlauf, Stats, Papierkorb,
  Konflikte, Einstellungen).
- `Preferences` – gespeicherte Einstellungen (Timer-Plan, letzte Aktivität,
  wöchentliche Auswertung, Stats-Jahr, Farbschema). Sie werden über eframe
  persistiert (als RON-Datei).
- `Editor` – der Zustand des Bearbeiten-Formulars samt Validierung.
- `Dialog` – eine Aufzählung der möglichen Dialoge (Editor, Export, Löschen,
  Verwerfen, Beenden, Import).
- `Aftermath` – **die App selbst**. Diese Struktur ist das Herz der
  Oberfläche und wird gleich genauer erklärt.
- `impl eframe::App for Aftermath` – hier findet egui den Einstiegspunkt.

egui ist ein **Immediate-Mode**-GUI: Es gibt keine langlebigen Widget-Objekte.
Stattdessen wird das Fenster 60-mal pro Sekunde komplett neu „gemalt“, indem
deine Zeichenfunktion erneut durchläuft. Das ist anfangs ungewohnt, aber
einfach: Man liest den Zustand, zeichnet, und Änderungen am Zustand sind sofort
im nächsten Frame sichtbar.

Die `Aftermath`-Struktur ist der gesamte Anwendungszustand:

```rust
pub struct Aftermath {
    store: Store,                 // die Datenbank
    preferences: Preferences,     // gespeicherte Einstellungen
    entries: Vec<Entry>,          // aktuell geladene Sitzungen
    page: Page,                   // welche Seite sichtbar ist
    filter: Filter,               // aktive Filter
    selected: Option<String>,     // ausgewählte Sitzung
    run: Option<Run>,             // laufender Timer (oder keiner)
    watcher: DeadlineWatcher,      // klingelt auch ohne Frame-Callbacks
    dialog: Option<Dialog>,       // offener Dialog (oder keiner)
    // … weitere Felder: Sync-Status, Benachrichtigungen, Screenshot-Modus
}
```

Die `Option<…>`-Felder sind typisch für Rust: `Option<Run>` heißt „entweder
ein laufender Timer oder keiner“. Es gibt keinen „Null“-Zeiger; man muss den
Fall „kein Timer“ immer mitdenken.

Die eigentliche Zeichenfunktion ist `render` – sie ist bewusst als kurze
Übersicht aufgebaut:

```rust
fn render(&mut self, ctx: &egui::Context) {
    ctx.request_repaint_after(Duration::from_millis(200));  // regelmäßig neu zeichnen
    self.tick(ctx);          // Timer + Sync fortschreiben
    self.shortcuts(ctx);     // Tastenkürzel
    self.footer(ctx);        // Statusleiste unten
    self.sidebar(ctx);       // Navigation links
    egui::CentralPanel::default().show(ctx, |ui| {
        // Kopfbereich + Benachrichtigung + je nach `page` eine der Ansichten
    });
    self.dialogs(ctx);       // modale Dialoge
    self.resync_deadline();  // Glocken-Termin an den Hintergrund-Watcher
    self.capture(ctx);       // optionaler Screenshot + Beenden
}
```

`impl eframe::App` ist dann nur noch eine dünne Hülle:

```rust
impl eframe::App for Aftermath {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.render(ctx);
    }
    fn save(&mut self, storage: &mut dyn eframe::Storage) {
        eframe::set_value(storage, "snapo-preferences", &self.preferences);
    }
}
```

**Glocke ohne Frame-Callbacks:** Unter Wayland/Sway erhält ein Fenster auf
einer unsichtbaren Arbeitsfläche keine Frame-Callbacks; winit drosselt dann
die Redraws, und `render`/`tick` pausieren, bis das Fenster wieder sichtbar
wird. Die gemessene Zeit bleibt dabei korrekt, weil `Clock` elapsed aus
`Instant`-Differenzen berechnet und nie durch Ticks aufsummiert – aber eine
Glocke, die vom Tick-Loop ausgelöst wird, käme zu spät. Deshalb hält
`Aftermath` einen `DeadlineWatcher` (`timer.rs`): ein Hintergrund-Thread, dem
`render` einmal pro Frame den aktuellen Zieltermin published
(`resync_deadline`). Ist der Loop gestoppt, kann sich der Lauf nicht mehr
ändern, der letzte Termin bleibt also gültig. Der Watcher klingelt pünktlich
und merkt sich die Generation (`take_rung`), damit `tick` beim Aufholen
nach der versteckten Phase nicht ein zweites Mal klingelt.

Am Ende von `ui.rs` steht ein `#[cfg(test)] mod tests`, das **echte**
egui-Schaltflächen anklickt und Tastenkürzel sendet (mehr in Kapitel 11).

**Farbschema:** `ui.rs` definiert ein `Theme` (`Light`/`Dark`) und eine
`Palette` (Text, Akzent, Flächen, Linien, Aktivitätsfarben …). Die aktive
Palette liegt in einem `thread_local!`, das `render` zu Beginn setzt – so
greifen die freien Chart- und Style-Helfer ohne Parameter auf die Farben zu.
`configure_style` wählt je nach Theme `Visuals::dark()`/`light()` und übersteuert
die Markenfarben. Das Theme wird in `Preferences` gespeichert und in den
Einstellungen umgeschaltet.

### 5.7 `command.rs` – die Command-Line im Fokus-Panel

Ein kleines Parser-Modul ohne egui-Abhängigkeit. Im Fokus-Panel gibt es ein
Command-Line-Feld im Stil der ursprünglichen Python-CLI (`prodz`):

```
Lernen -w 30 -c 2 -t "Vokabeln"
```

- Optionen mirroring die Python-Flags: `-a`/`--activity`, `-t`/`--text`,
  `-w`/`--work`, `-b`/`--break`, `-lb`/`--long-break`, `-c`/`--cycles`,
  `-e`/`--endless`. Werte mit Leerzeichen stehen in Anführungszeichen.
- Wörter **vor** den Optionen (oder ein einzelnes direkt danach) werden zur
  Aktivität: `Lernen -w 30` definiert dieselbe Aktivität wie `-a Lernen -w 30`.
- `parse` prüft dieselben Grenzen wie die Ziehfelder im Panel (Fokus
  1–240 min, kurze Pause 1–120, lange Pause 1–180, Zyklen 1–24) und liefert
  deutsche Fehlermeldungen.
- `ui.rs::execute_command` wendet das Ergebnis auf `preferences` an, startet
  den Timer und leert das Feld; die Werte bleiben also nach dem Start erhalten.

Ein leerer Befehl ändert nichts und startet mit den zuletzt definierten Werten.

### 5.8 `stats.rs` – die Auswertungen

Ein Modul mit reinen Aggregationsfunktionen (kein egui, gut testbar). Es rechnet
über die bereits gefilterten, aktiven Sitzungen:

- `summarize` – die Kennzahlen: Gesamtzeit, Sitzungszahl, Anzahl Aktivitäten,
  aktive Tage, Durchschnitt pro aktivem Tag, bester Tag und die aktuelle Serie
  aufeinanderfolgender Tage.
- `by_month` / `by_week` – aggregierte Minuten je Kalendermonat bzw. ISO-Woche
  eines Jahres, jeweils aufgeteilt nach Aktivität (Grundlage des Jahres-Charts).
- `by_weekday` – Minuten je Wochentag (Montag = Index 0).
- `by_hour` – Minuten je Stunde (0–23), aus dem Sitzungs-Zeitstempel.
- `by_activity` – Aktivitäten absteigend nach Fokuszeit (für die Rangliste).

`ui.rs` zeichnet diese Daten als Balkendiagramme (`stacked_bar_chart`,
`simple_bar_chart`) und Kacheln (`kpi`). Die Stats-Seite übernimmt die
Verlaufsfilter; Papierkorb und ungelöste Konflikte zählen nicht mit.

### 5.9 `main.rs` – der Start

`main` ist die Naht zwischen Kommandozeile und GUI. Zuerst parst `clap` die
Argumente (`Args` + `Command`), dann wird der Datenbankpfad bestimmt:

```
--db  →  $AFTERMATH_DESKTOP_DB  →  $SNAPO_DESKTOP_DB  →  Standardpfad
```

Danach öffnet die App den `Store` und verzweigt:

- `remote-check`, `check`, `history`, `export`, `import`, `sync` → **ohne
  Fenster** auf der Konsole ausführen (nützlich für Skripte und Server).
- kein Unterbefehl → `eframe::run_native` öffnet das Fenster und übergibt eine
  `Aftermath`-Instanz.

Das Muster „ein Programm, zwei Betriebsarten“ (GUI und CLI) ist praktisch und
in Rust dank `clap`-Unterbefehlen (`Subcommand`) sehr übersichtlich.

---

## 6. Lebenszyklus einer Sitzung

Dieser Ablauf verbindet alle Module und beantwortet die Frage „was passiert
eigentlich, wenn ich auf Start drücke?“.

1. **Benutzer drückt Start** → `ui.rs::start_timer` liest `preferences.plan`
   und ruft `Run::new(...)` auf.
2. `timer.rs::Run::new` prüft die Einstellungen (z. B. `1..=240` Minuten) und
   erzeugt einen `Clock`. Ist eine Aktivität leer, kommt ein Fehler zurück.
3. Die Oberfläche zeichnet in jedem Frame die verbleibende Zeit. `Clock`
   rechnet mit `Instant`, Pausen zählen dabei nicht mit.
4. **Arbeitsphase endet** → `ui.rs::finish_timer` ruft `clock.snapshot(...)`
   auf. Das liefert eine `Session` mit eingefrorener Dauer und ID.
5. `store.create(session, …)` legt eine neue **Wurzel-Revision** an. Die
   Sitzung ist damit in der lokalen SQLite-Datei.
6. `Run::advance` wechselt zur Pause; ist der Zyklusplan fertig, endet der
   Timer.
7. War Cloud-Sync aktiv, startet `ui.rs::start_sync` einen **Thread**, der
   `sync::synchronize` ausführt. Nach Erfolg wird die Revision als „bestätigt“
   markiert (`acknowledge`) und der Zeitpunkt gespeichert (`mark_synced`).

Als Diagramm:

```
Start ─▶ Run/Clock ─▶ snapshot ─▶ Store.create ─▶ SQLite-Revision
                                        │
                                        └─(wenn --sync)─▶ Thread ─▶ sync::synchronize ─▶ Turso
```

---

## 7. Das Datenmodell: Revisionen statt Zeilen

Das ist der ungewöhnlichste Teil der App und der spannendste zum Lernen.

Statt eine Sitzung einfach zu überschreiben (wie in einer normalen Tabelle),
wird **jede Änderung als neue, unveränderliche Revision** angehängt. Eine
Revision enthält:

```rust
pub struct Revision {
    pub revision_id: String,          // ID dieser Version
    pub session_id: String,           // ID der Sitzung (bleibt über Versionen gleich)
    pub parents: Vec<String>,         // Vorgänger-Versionen
    pub device_id: String,            // welches Gerät hat das erzeugt
    pub created_at: String,           // wann
    pub session: Session,             // der Inhalt (Aktivität, Dauer, …)
    pub deleted: bool,                // Papierkorb-Markierung
    pub legacy_id: Option<i64>,       // aus der alten Python-Datenbank
}
```

Das Modell ist **append-only** (nur anfügen) und über `parents` zu einem
gerichteten Graphen verbunden. Man kann es sich wie `git` vorstellen:
Revisionen sind Commits, `session_id` gruppiert sie zu einem „Zweig“, und die
aktuelle Sicht ist derjenige Stand, auf den niemand mehr verweist.

### Rekonstruktion

`history::reconstruct` macht aus einer flachen Revisionsliste die sichtbaren
Sitzungen:

1. Gruppiere alle Revisionen nach `session_id`.
2. Sammle alle IDs, die irgendwo als `parent` genannt werden (das sind die
   „verbrauchten“ Vorgänger).
3. Alles, was **nicht** verbraucht ist, sind die **Köpfe** (`heads`) – die
   aktuell gültigen Versionen.
4. Ein `Entry` mit genau einem Kopf ist eindeutig. Ein `Entry` mit **mehreren
   Köpfen ist ein Konflikt** – zwei Geräte haben parallel geändert.

```rust
pub struct Entry {
    pub session_id: String,
    pub heads: Vec<Revision>,   // 1 = eindeutig, >1 = Konflikt
}
```

Das erklärt die Oberfläche: Der Verlauf zeigt eindeutige Sitzungen, der
Papierkorb zeigt die, deren aktueller Kopf `deleted = true` ist, und die
Konfliktseite zeigt die mehrdeutigen. Nichts geht verloren, weil Revisionen nie
gelöscht werden.

Diese Herangehensweise heißt in der Fachsprache **append-only set
replication** – eine einfache Form eines nebenläufigkeitsfreundlichen
Datenmodells (ähnlich einem CRDT). Sie ist der Grund, warum mehrere Geräte
offline arbeiten können und sich später verlustfrei wieder zusammenfinden.

---

## 8. Nebenläufigkeit: Timer, UI und Sync

Rust hat keine automatische Speicherbereinigung. Stattdessen regelt der
**Compiler** die Speicherverwaltung über Besitz („ownership“) und
Ausleihregeln („borrowing“). Das klingt streng – ist aber der Grund, warum die
Nebenläufigkeit hier so sicher ist.

Die App startet genau einen zusätzlichen Thread, und zwar für die
Synchronisierung, damit die Oberfläche nicht einfriert:

```rust
let (tx, rx) = mpsc::channel();
self.syncing = Some(rx);
std::thread::spawn(move || {
    let result = sync::synchronize(&path, &remote)
        .map_err(|e| sync::safe_error(&e, &remote));
    let _ = tx.send(result);          // Ergebnis an die UI senden
    ctx.request_repaint();            // UI aufwecken
});
```

Das Vorgehen ist das klassische **Kanal-Muster** aus der Rust-Standardbibliothek:

- Der Thread kennt die Oberfläche nicht. Er bekommt nur `path` und `remote`
  (beide geklont bzw. verschoben, deshalb `move`).
- Er sendet sein Ergebnis über `tx` (Sender) an `rx` (Receiver).
- Die UI prüft in `poll_sync` mit `try_recv`, ob ein Ergebnis da ist – **ohne
  zu blockieren**.
- `ctx.request_repaint()` weckt die Oberfläche auf, damit sie bald einen neuen
  Frame zeichnet und das Ergebnis anzeigt.

Der Timer läuft **nicht** in einem Thread. Er ist reine Rechnung auf `Instant`
und wird bei jedem Frame neu berechnet. Das ist robuster als ein Hintergrund-
Thread mit Zähler, weil die dargestellte Zeit immer direkt aus zwei
Zeitpunkten abgeleitet wird.

> **Warum ist der Datenaustausch zwischen Threads in Rust so unkompliziert?**
> Der Compiler weiß durch die Ownership-Regeln, dass es genau einen Besitzer
> der Daten gibt. Wer Daten in einen Thread verschiebt (`move`), gibt sie
> endgültig ab. Damit sind Datenrennen (data races) beim Übersetzen
> ausgeschlossen.

---

## 9. Rust-Konzepte, die du hier siehst

Dieser Abschnitt ist eine kleine Sprachreferenz – sortiert nach den Stellen,
an denen dir die Konzepte im Code begegnen.

### 9.1 Strukturen und Methoden (`struct` + `impl`)

Eine Struktur bündelt Daten; ein `impl`-Block fügt Verhalten hinzu:

```rust
pub struct Session { pub activity: String, pub duration_minutes: f64, /* … */ }

impl Session {
    pub fn now(activity: impl Into<String>, minutes: f64, comment: impl Into<String>) -> Self {
        // …
    }
    pub fn validated(mut self) -> Result<Self> { /* … */ }
}
```

Methoden nehmen `self` (übernimmt), `&self` (liest) oder `&mut self`
(verändert). `Session::now` ist eine assoziierte Funktion (wie ein
Konstruktor).

### 9.2 Aufzählungen mit Daten (`enum`)

Rust-Enums sind mächtiger als in vielen Sprachen: Jede Variante kann Daten
tragen.

```rust
enum Dialog {
    Editor(Box<Editor>),
    Export { path: String, entries: Vec<Entry>, error: Option<String> },
    Delete(Entry),
    // …
}
```

Hier stehen in einer Variablen `dialog: Option<Dialog>` alle möglichen
UI-Zustände – und der Compiler erzwingt, dass man beim `match` jeden Fall
behandelt.

### 9.3 `Option` und `Result` statt `null` und Exceptions

- `Option<T>` = „Wert oder nichts“ (`Some(x)` / `None`).
- `Result<T, E>` = „Erfolg oder Fehler“ (`Ok(x)` / `Err(e)`).

Kombinatoren wie `.map`, `.and_then`, `.is_some_and`, `.unwrap_or_else`
verketten solche Fälle ohne tiefe Verschachtelung, z. B.:

```rust
self.target.map_or(elapsed, |target| elapsed.min(target))
```

### 9.4 Der `?`-Operator

Statt Fehler manuell weiterzureichen, schreibt man einfach `?`:

```rust
let source = std::fs::read_to_string(path)
    .context("Die .env-Datei lässt sich nicht lesen.")?;
```

Ist der Wert ein `Err`, wird die Funktion sofort mit dem (angereicherten)
Fehler verlassen; sonst läuft es mit dem Erfolgswert weiter.

### 9.5 Traits

Ein **Trait** ist ein Vertrag (vergleichbar mit einem Interface). `eframe::App`
sagt: „Wer das implementiert, hat eine `update`-Methode.“ Die ganze GUI hängt
an dieser einen Umsetzung:

```rust
impl eframe::App for Aftermath {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) { … }
}
```

Traits wie `Serialize`/`Deserialize` werden per `#[derive(...)]` automatisch
abgeleitet – ein Makro schreibt den Code dafür.

### 9.6 Iteratoren

Fast alle Listenverarbeitung läuft über Iteratoren. Beispiel aus `reconstruct`:

```rust
let mut heads: Vec<_> = versions.into_iter()
    .filter(|r| !consumed.contains(&r.revision_id))
    .collect();
```

`into_iter` → `filter` → `collect` ist idiomatisch und wird vom Compiler
optimiert.

### 9.7 Pattern Matching und Closures

`match` verzweigt über Enums; Closures (`|x| …`) sind anonyme Funktionen, oft
als Argument (z. B. an `filter`, `map`, `unwrap_or_else`).

### 9.8 Module und Sichtbarkeit

`crate::…` bezeichnet den eigenen Crate-Namensraum, `pub` macht sichtbar.
`history.rs` beginnt mit einer Dokumentation für das Modul:

```rust
//! Wire-compatible with the Python application's immutable revision history.
```

`//!` dokumentiert das Modul selbst, `///` dokumentiert das nächste Element.

---

## 10. Fehlerbehandlung

Die App benutzt konsequent `anyhow::Result<T>` (also
`Result<T, anyhow::Error>`) und drei Werkzeuge:

- **`?`** reicht Fehler weiter.
- **`.context("…")`** ergänzt eine erklärende Nachricht für den Benutzer.
- **`bail!(…)`** bricht sofort mit einem Fehler ab.
- **`ensure!(bedingung, "…")`** ist `if !bedingung { bail!(…) }`.

Beispiel aus `store.rs`:

```rust
ensure!(
    version.as_deref() == Some("2"),
    "Unbekannte Datenbankversion. Bitte Aftermath aktualisieren."
);
```

Diese Meldungen sind bewusst auf Deutsch und für Menschen geschrieben – sie
landen teilweise direkt in der Oberfläche.

---

## 11. Tests, Build und Werkzeuge

### Die Standardbefehle

```bash
cargo fmt --all -- --check          # Formatierung prüfen
cargo clippy --locked --all-targets -- -D warnings   # Linter, Warnungen sind Fehler
cargo test --locked --all-targets   # alle Tests
cargo build --release --locked      # Release-Binary
cargo run --locked -- --local --db /tmp/demo.db      # mit Demodaten starten
```

`cargo` ist Werkzeug und Paketmanager zugleich. `--locked` heißt: exakt die
Versionen aus `Cargo.lock` verwenden (reproduzierbar).

### Wo die Tests liegen

- **Unit-Tests** direkt in der Datei unter `#[cfg(test)] mod tests` –
  z. B. in `ui.rs`. Sie dürfen auf private Funktionen zugreifen.
- **Integrationstests** in `tests/desktop.rs`. Sie benutzen die App als
  Bibliothek (`use aftermath::…`) und testen das Verhalten von außen.

Die Tests sind bemerkenswert konsequent:

- Timer-Tests prüfen, dass Pausen nicht mitzählen und die Arbeitszeit gedeckelt
  wird.
- Sync-Tests starten einen **lokalen HTTP-Testserver mit SQLite** und gleichen
  zwei unabhängige Desktop-Dateien ab.
- Die UI-Tests in `ui.rs` bedienen echte egui-Schaltflächen und Tastenkürzel
  (`click(...)`, `key(...)`) und prüfen sowohl kleine als auch große Fenster.
- Alle Tests verwenden **temporäre** Daten (`tempfile::TempDir`), keine echten
  Zugangsdaten.

Für Einsteiger ist das eine gute Vorlage: Fachlogik ist von der Oberfläche
getrennt und deshalb ohne GUI testbar.

---

## 12. Übungsaufgaben

Die folgenden Aufgaben sind nach Schwierigkeit sortiert und sollen das
Gelesene festigen. (Es sind Lernaufgaben, keine Pflichtänderungen.)

1. **Navigation:** Öffne `src/timer.rs`. Suche `fn elapsed`. Erkläre in
   eigenen Worten, warum die Pause die verstrichene Zeit nicht erhöht.
2. **Datenmodell:** Nimm drei Revisionen mit `session_id = "s1"` an:
   `r1 parents=[]`, `r2 parents=[r1]`, `r3 parents=[r1]`. Welche Köpfe liefert
   `reconstruct`? Ist das ein Konflikt?
3. **Fehler:** Ändere testweise in `store.rs` die Meldung in einem
   `ensure!`-Aufruf und führe `cargo test` aus.
4. **UI:** Finde in `ui.rs` die Funktion `focus`. Welche Karte wird auf schmalen
   Fenstern ausgeblendet? Warum ist das sinnvoll?
5. **Sync:** Beschreibe in eigenen Worten, warum eine Revision erst nach der
   Verifikation als „bestätigt“ markiert wird.
6. **Erweitern:** Überlege dir ein neues Feld in `Session` (z. B. `mood`).
   Welche Stellen müssten angepasst werden, damit es in Anzeige, CSV-Export
   und Sync erscheint? Tipp: Beginne bei `history.rs`.
7. **Tooling:** Führe `cargo clippy` aus und versuche zu verstehen, wofür der
   Linter jeweils warnt.

---

## 13. Glossar

| Begriff | Erklärung |
| --- | --- |
| **Crate** | Ein Rust-Paket (Bibliothek oder Programm). |
| **Cargo** | Build-Werkzeug und Paketmanager. `Cargo.toml` beschreibt das Projekt. |
| **egui** | Immediate-Mode-GUI-Bibliothek: Das UI wird jeden Frame neu gezeichnet. |
| **eframe** | Rahmen um egui, der Fenster, Rendering und Persistenz übernimmt. |
| **Revision** | Unveränderliche Version einer Sitzung. |
| **Head** | Eine Revision, auf die keine neuere verweist – die aktuell gültige Sicht. |
| **Konflikt** | Eine Sitzung mit mehr als einem Head (parallele Änderungen). |
| **Append-only** | Daten werden nur angehängt, nie überschrieben oder gelöscht. |
| **WAL** | Write-Ahead-Logging in SQLite: robuster und nebenläufiger. |
| **Hrana** | HTTP-Protokoll, mit dem die App die Turso-Cloud anspricht. |
| **Turso** | Cloud-Datenbank (libSQL/SQLite-kompatibel), die die Geräte abgleicht. |
| **Ownership** | Rusts Regel, dass jeder Wert genau einen Besitzer hat. Grundlage der Speichersicherheit. |
| **Trait** | Vertrag/Interface, den ein Typ erfüllen kann (z. B. `eframe::App`). |
| **`#[derive]`** | Makro, das Standard-Implementierungen wie `Clone` oder `Serialize` erzeugt. |

---

## Weiterlesen

- [`README.md`](../README.md) – Bedienung, Installation, Cloud-Einrichtung.
- [`src/lib.rs`](../src/lib.rs) – die Modulübersicht; hier beginnt das Lesen.
- Der Rust-Buch-Klassiker: <https://doc.rust-lang.org/book/> (offiziell, kostenlos).
- egui-Beispiele: <https://github.com/emilk/egui>.
