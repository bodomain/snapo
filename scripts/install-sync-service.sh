#!/usr/bin/env bash
set -euo pipefail
if [[ $# -lt 2 || $# -gt 3 || ( $# -eq 3 && "$3" != --install ) ]]; then
    printf 'Aufruf: %s /absoluter/pfad/desktop.db /absoluter/pfad/.env [--install]\n' "$0" >&2
    exit 2
fi
database_path="$1"
env_path="$2"
binary_path="${XDG_BIN_HOME:-$HOME/.local/bin}/aftermath"
[[ "$database_path" == /* && "$env_path" == /* ]] || { printf 'Absolute Pfade erforderlich.\n' >&2; exit 2; }
[[ -x "$binary_path" ]] || { printf 'Zuerst install-desktop.sh ausführen.\n' >&2; exit 1; }
[[ -f "$env_path" ]] || { printf 'Die .env-Datei fehlt.\n' >&2; exit 1; }
systemd_quote() {
    local value="$1"
    value="${value//\\/\\\\}"
    value="${value//\"/\\\"}"
    value="${value//%/%%}"
    value="${value//\$/\$\$}"
    printf '"%s"' "$value"
}
unit() {
    printf '%s\n' '[Unit]' 'Description=Aftermath Desktop – Synchronisierung' 'After=network.target' '' '[Service]' 'Type=simple'
    printf 'ExecStart='
    systemd_quote "$binary_path"
    printf ' --db '
    systemd_quote "$database_path"
    printf ' --env-file '
    systemd_quote "$env_path"
    printf ' sync --interval 15\n'
    printf '%s\n' 'Restart=on-failure' 'RestartSec=15' 'UMask=0077' '' '[Install]' 'WantedBy=default.target'
}
if [[ "${3:-}" == --install ]]; then
    unit_dir="${XDG_CONFIG_HOME:-$HOME/.config}/systemd/user"
    mkdir -p "$unit_dir"
    unit > "$unit_dir/aftermath-desktop-sync.service"
    systemctl --user daemon-reload
    systemctl --user enable --now aftermath-desktop-sync.service
else
    unit
fi
