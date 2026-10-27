#!/usr/bin/env bash
set -euo pipefail
project_dir="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"
bin_dir="${XDG_BIN_HOME:-$HOME/.local/bin}"
data_dir="${XDG_DATA_HOME:-$HOME/.local/share}"

# Escape according to the Desktop Entry Exec grammar, including field codes.
desktop_quote() {
    local value="$1"
    value="${value//\\/\\\\}"
    value="${value//\"/\\\"}"
    value="${value//\$/\\\$}"
    value="${value//\`/\\\`}"
    value="${value//%/%%}"
    printf '"%s"' "$value"
}

cargo build --manifest-path "$project_dir/Cargo.toml" --target-dir "$project_dir/target" --release --locked
install -Dm755 "$project_dir/target/release/aftermath" "$bin_dir/aftermath"
install -Dm644 "$project_dir/assets/aftermath.svg" "$data_dir/icons/hicolor/scalable/apps/aftermath.svg"
mkdir -p "$data_dir/applications"
{
    printf '%s\n' '[Desktop Entry]' 'Type=Application' 'Name=Aftermath' 'Comment=Zeit für Fokus – Timer, Verlauf und Stats' 'Icon=aftermath' 'StartupWMClass=Aftermath' 'Terminal=false' 'Categories=Office;'
    printf 'Exec='
    desktop_quote "$bin_dir/aftermath"
    if [[ -f "$project_dir/.env" ]]; then
        printf ' --env-file '
        desktop_quote "$project_dir/.env"
    fi
    printf '\n'
} > "$data_dir/applications/aftermath.desktop"
if command -v update-desktop-database >/dev/null; then update-desktop-database "$data_dir/applications"; fi
printf 'Installiert: %s\nApp-Menü: Aftermath\n' "$bin_dir/aftermath"
