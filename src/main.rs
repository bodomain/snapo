use aftermath::{config, history, store::Store, sync, ui::Aftermath};
use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use std::{path::PathBuf, time::Duration};

#[derive(Parser)]
#[command(
    version,
    about = "Aftermath – nativer Fokus-Timer mit Verlauf und Statistiken"
)]
struct Args {
    /// Separate Desktop-Datenbank (niemals eine Turso-Replikatdatei).
    #[arg(long, global = true)]
    db: Option<PathBuf>,
    /// Cloud-Synchronisierung beim Start aktivieren.
    #[arg(long, conflicts_with = "local")]
    sync: bool,
    /// Ausschließlich lokal arbeiten (Standard).
    #[arg(long)]
    local: bool,
    /// Datei mit TURSO_DATABASE_URL und TURSO_AUTH_TOKEN.
    #[arg(long, global = true)]
    env_file: Option<PathBuf>,
    /// Fenster einmal als PNG aufnehmen und schließen.
    #[arg(long)]
    screenshot: Option<PathBuf>,
    /// Beim Start diese Ansicht öffnen.
    #[arg(long, value_enum, default_value = "focus")]
    view: aftermath::ui::Page,
    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(Subcommand)]
enum Command {
    /// Cloud-Verbindung und Schema prüfen, ohne Cloud-Daten zu verändern.
    RemoteCheck,
    /// Lokale Datenbank öffnen und Integrität prüfen, ohne ein Fenster zu öffnen.
    Check,
    /// Verlauf als JSON ausgeben.
    History,
    /// Aktive Sitzungen als CSV exportieren (überschreibt keine Datei).
    Export { path: PathBuf },
    /// Eine Python-SQLite-Datenbank importieren; die Quelle bleibt unverändert.
    Import { path: PathBuf },
    /// Mit Turso synchronisieren; optional als Hintergrundprozess wiederholen.
    Sync {
        #[arg(long, value_parser = clap::value_parser!(u64).range(1..))]
        interval: Option<u64>,
    },
}

fn main() -> Result<()> {
    let args = Args::parse();
    if args.local
        && matches!(
            args.command,
            Some(Command::Sync { .. } | Command::RemoteCheck)
        )
    {
        anyhow::bail!("--local kann nicht mit einem Cloud-Befehl kombiniert werden.");
    }
    let path = args
        .db
        .or_else(|| std::env::var_os("AFTERMATH_DESKTOP_DB").map(PathBuf::from))
        .or_else(|| std::env::var_os("SNAPO_DESKTOP_DB").map(PathBuf::from))
        .map(Ok)
        .unwrap_or_else(config::default_database)?;
    let mut store = Store::open(&path)?;
    match args.command {
        Some(Command::RemoteCheck) => {
            let remote = config::remote(args.env_file.as_deref())?;
            let report = sync::inspect(&remote)
                .map_err(|e| anyhow::anyhow!(sync::safe_error(&e, &remote)))?;
            println!("{}", serde_json::to_string_pretty(&report)?);
        }
        Some(Command::Check) => {
            store.check()?;
            println!(
                "OK · {} Sitzungen · {}",
                store.entries()?.len(),
                path.display()
            );
        }
        Some(Command::History) => {
            println!("{}", serde_json::to_string_pretty(&store.revisions()?)?)
        }
        Some(Command::Export { path }) => {
            println!(
                "{} Sitzungen exportiert: {}",
                history::export_csv(&store.entries()?, &path)?,
                path.display()
            );
        }
        Some(Command::Import { path }) => {
            println!("{} Revisionen importiert.", store.import(&path)?);
        }
        Some(Command::Sync { interval }) => {
            let remote = config::remote(args.env_file.as_deref())?;
            loop {
                match sync::synchronize(&path, &remote) {
                    Ok(()) => println!("Synchronisiert · {}", chrono::Local::now().to_rfc3339()),
                    Err(error) => {
                        if interval.is_none() {
                            anyhow::bail!("{}", sync::safe_error(&error, &remote));
                        }
                        eprintln!("{}", sync::safe_error(&error, &remote));
                    }
                }
                let Some(seconds) = interval else { break };
                std::thread::sleep(Duration::from_secs(seconds));
            }
        }
        None => {
            let configured = config::remote(args.env_file.as_deref());
            let remote = if args.sync {
                Some(
                    configured
                        .as_ref()
                        .map_err(|e| anyhow::anyhow!(e.to_string()))?
                        .clone(),
                )
            } else {
                None
            };
            let options = eframe::NativeOptions {
                viewport: eframe::egui::ViewportBuilder::default()
                    .with_app_id("aftermath")
                    .with_title("Aftermath – Zeit für Fokus")
                    .with_inner_size([1000.0, 680.0])
                    .with_min_inner_size([626.0, 520.0]),
                renderer: eframe::Renderer::Glow,
                // Keep the existing timer preferences through the rename.
                persistence_path: eframe::storage_dir("snapo").map(|dir| dir.join("app.ron")),
                ..Default::default()
            };
            eframe::run_native(
                "Aftermath",
                options,
                Box::new(move |cc| {
                    Ok(Box::new(Aftermath::new(
                        cc,
                        store,
                        remote,
                        configured,
                        args.screenshot,
                        args.view,
                    )))
                }),
            )
            .map_err(|e| anyhow::anyhow!(e.to_string()))
            .context("Das Desktop-Fenster konnte nicht gestartet werden.")?;
        }
    }
    Ok(())
}
