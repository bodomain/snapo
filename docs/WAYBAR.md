# prodzCLI im Waybar zeigen

Der Timer aus `prodz_cli.py` exportiert seinen Live-Zustand nach
`~/.local/state/prodz/session.json` (bzw. `$XDG_STATE_HOME/prodz/session.json`).
Das Modul `scripts/prodz_waybar.py` liest die Datei für Waybar aus.

## Waybar einrichten

In `~/.config/waybar/config` ein Modul ergänzen:

```jsonc
"custom/prodz": {
    "exec": "python3 /pfad/zum/repo/scripts/prodz_waybar.py",
    "return-type": "json",
    "interval": 1,
    "on-click": "python3 /pfad/zum/repo/scripts/prodz_waybar.py --toggle"
},
```

und `"custom/prodz"` in die `modules`-Liste der gewünschten Leiste (z. B.
`"modules-right"`) aufnehmen.

## Verhalten

- **Anzeige**: Phase + Restzeit, z. B. `Work 12:26`. Endlose Sitzungen zeigen
  die vergangene Zeit (`Work 12:26 elapsed`). Pausierte Sitzungen erhalten ein
  `⏸`-Präfix.
- **Tooltip**: Aktivität, Kommentar, Zyklus x/y, Zustand und Startzeit.
- **Ohne Sitzung**: Das Modul verschwindet. Auch eine tote PID (abgeschossene
  oder abgestürzte Session) wird ignoriert, damit nie ein eingefrorener Timer
  stehen bleibt.
- **Klick**: `--toggle` sendet `SIGUSR1` an die laufende Session und schaltet
  Pause/Fortsetzung um – wie die `p`-Taste, nur aus der Statusleiste.
- **Aufräumen**: Beim Ende der Session (speichern, verwerfen, abgeschlossen)
  löscht `prodz_cli.py` die Zustandsdatei.

## Farben (optional)

`scripts/prodz_waybar.py` setzt die Klassen `prodz-running`, `prodz-paused`
und `prodz-critical` (weniger als eine Minute Restzeit). Beispiel für
`~/.config/waybar/style.css`:

```css
#custom-prodz.prodz-running {
    color: #2ec27e;
}
#custom-prodz.prodz-paused {
    color: #f6d32d;
}
#custom-prodz.prodz-critical {
    color: #e01b24;
    font-weight: bold;
}
```
