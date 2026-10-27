use crate::sync::Remote;
use anyhow::{Context, Result};
use directories::ProjectDirs;
use std::{
    collections::HashMap,
    path::{Path, PathBuf},
};

pub fn default_database() -> Result<PathBuf> {
    // Retain the original storage location so the rename keeps existing data.
    Ok(ProjectDirs::from("de", "snapo", "Snapo")
        .context("Kein Benutzerverzeichnis gefunden. Bitte --db angeben.")?
        .data_local_dir()
        .join("snapo-desktop.db"))
}

/// Read configuration without mutating the process environment (Rust 2024).
/// Environment values always win. Credentials never enter persisted UI state.
pub fn remote(env_file: Option<&Path>) -> Result<Remote> {
    let default = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(".env");
    let path = env_file.unwrap_or(&default);
    let mut values = HashMap::new();
    if path.exists() {
        let source =
            std::fs::read_to_string(path).context("Die .env-Datei lässt sich nicht lesen.")?;
        // Preserve the Python client's old single-line token-file convention.
        if !source.contains('=') && !source.trim().is_empty() {
            values.insert("TURSO_AUTH_TOKEN".to_owned(), source.trim().to_owned());
        } else {
            for pair in dotenvy::from_read_iter(source.as_bytes()) {
                let (key, value) = pair.map_err(|_| {
                    anyhow::anyhow!("Ungültige .env-Datei. Erwartet wird NAME=WERT.")
                })?;
                values.insert(key, value);
            }
        }
    } else if env_file.is_some() {
        anyhow::bail!("Die angegebene .env-Datei existiert nicht.");
    }
    let setting = |key| {
        std::env::var(key)
            .ok()
            .or_else(|| values.get(key).cloned())
            .with_context(|| format!("{key} fehlt. In .env oder als Umgebungsvariable setzen."))
    };
    let remote = Remote {
        url: setting("TURSO_DATABASE_URL")?,
        token: setting("TURSO_AUTH_TOKEN")?,
    };
    remote.validate()?;
    Ok(remote)
}
