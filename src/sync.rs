//! Append-only set replication over Turso's documented Hrana HTTP protocol.
//! The desktop owns a SQLite database, never a Python/Turso replica file.
use crate::{
    history::{Revision, Session},
    store::{INSERT_REVISION, SELECT_REVISIONS, Store},
};
use anyhow::{Context, Result, bail, ensure};
use reqwest::blocking::Client;
use serde_json::{Value, json};
use std::{collections::HashMap, path::Path, time::Duration};

#[derive(Clone)]
pub struct Remote {
    pub url: String,
    pub token: String,
}

impl Remote {
    pub fn validate(&self) -> Result<()> {
        ensure!(!self.token.trim().is_empty(), "TURSO_AUTH_TOKEN ist leer.");
        self.endpoint()?;
        Ok(())
    }
    pub fn from_env() -> Result<Self> {
        let remote = Self {
            url: std::env::var("TURSO_DATABASE_URL").context("TURSO_DATABASE_URL fehlt.")?,
            token: std::env::var("TURSO_AUTH_TOKEN").context("TURSO_AUTH_TOKEN fehlt.")?,
        };
        ensure!(
            !remote.token.trim().is_empty(),
            "TURSO_AUTH_TOKEN ist leer."
        );
        remote.endpoint()?;
        Ok(remote)
    }
    fn endpoint(&self) -> Result<String> {
        let url = if let Some(host) = self.url.strip_prefix("libsql://") {
            format!("https://{host}")
        } else {
            self.url.clone()
        };
        let parsed = reqwest::Url::parse(&url)?;
        ensure!(
            parsed.scheme() == "https"
                || (parsed.scheme() == "http"
                    && matches!(parsed.host_str(), Some("localhost" | "127.0.0.1" | "[::1]"))),
            "Cloud-Synchronisierung benötigt HTTPS."
        );
        ensure!(
            parsed.username().is_empty()
                && parsed.password().is_none()
                && parsed.query().is_none()
                && parsed.fragment().is_none(),
            "Ungültige Turso-URL."
        );
        Ok(format!("{}/v2/pipeline", url.trim_end_matches('/')))
    }
}

fn sql_value(value: Value) -> Value {
    match value {
        Value::Null => json!({"type":"null"}),
        Value::Number(n) => json!({"type":"integer","value":n.to_string()}),
        Value::String(s) => json!({"type":"text","value":s}),
        _ => unreachable!("only scalar SQL parameters"),
    }
}
fn statement(sql: &str, args: Vec<Value>) -> Value {
    json!({"type":"execute","stmt":{"sql":sql,"args":args.into_iter().map(sql_value).collect::<Vec<_>>(),"want_rows":true}})
}

fn pipeline(client: &Client, remote: &Remote, mut requests: Vec<Value>) -> Result<Vec<Value>> {
    let expected = requests.len();
    requests.push(json!({"type":"close"}));
    let response: Value = client
        .post(remote.endpoint()?)
        .bearer_auth(&remote.token)
        .json(&json!({"requests":requests}))
        .send()
        .context("Turso ist nicht erreichbar. Lokale Änderungen bleiben gespeichert.")?
        .error_for_status()
        .context("Turso-Anmeldung oder Anfrage fehlgeschlagen.")?
        .json()?;
    let results = response["results"]
        .as_array()
        .context("Ungültige Turso-Antwort.")?;
    ensure!(
        results.len() == expected + 1,
        "Unvollständige Turso-Antwort."
    );
    let mut output = Vec::new();
    for item in results.iter().take(expected) {
        if item["type"] != "ok" {
            let message = item["error"]["message"]
                .as_str()
                .unwrap_or("Unbekannter SQL-Fehler")
                .replace(&remote.token, "[redacted]");
            bail!("Turso: {message}");
        }
        output.push(item["response"]["result"].clone());
    }
    Ok(output)
}

fn rows(result: &Value) -> Result<Vec<Vec<Value>>> {
    result["rows"]
        .as_array()
        .context("SQL-Ergebnis ohne Zeilen.")?
        .iter()
        .map(|row| {
            row.as_array()
                .context("Ungültige SQL-Zeile.")?
                .iter()
                .map(|v| match v["type"].as_str() {
                    Some("text") => Ok(Value::String(
                        v["value"].as_str().context("Text fehlt.")?.to_owned(),
                    )),
                    Some("integer") => Ok(json!(
                        v["value"].as_str().context("Zahl fehlt.")?.parse::<i64>()?
                    )),
                    Some("null") => Ok(Value::Null),
                    _ => bail!("Unerwarteter SQL-Datentyp."),
                })
                .collect()
        })
        .collect()
}

fn decode_revision(row: &[Value]) -> Result<Revision> {
    ensure!(row.len() == 8, "Ungültige Revision.");
    let text = |i: usize| -> Result<String> {
        Ok(row[i].as_str().context("Revisionsfeld fehlt.")?.to_owned())
    };
    Ok(Revision {
        revision_id: text(0)?,
        session_id: text(1)?,
        parents: serde_json::from_str(&text(2)?)?,
        device_id: text(3)?,
        created_at: text(4)?,
        session: serde_json::from_str::<Session>(&text(5)?)?,
        deleted: row[6].as_i64().context("Löschstatus fehlt.")? != 0,
        legacy_id: row[7].as_i64(),
    })
}

/// Connection diagnostics are strictly read-only, including for legacy clouds.
pub fn inspect(remote: &Remote) -> Result<Value> {
    remote.validate()?;
    let client = Client::builder()
        .connect_timeout(Duration::from_secs(5))
        .timeout(Duration::from_secs(25))
        .redirect(reqwest::redirect::Policy::none())
        .build()?;
    let output = pipeline(
        &client,
        remote,
        vec![statement(
            "SELECT name FROM sqlite_master WHERE type='table'",
            vec![],
        )],
    )?;
    let tables: Vec<String> = rows(&output[0])?
        .iter()
        .filter_map(|row| row.first()?.as_str().map(str::to_owned))
        .collect();
    let mut report = json!({"connected":true,"schema_version":null,"revision_count":0});
    if tables.iter().any(|name| name == "snapo_config") {
        let output = pipeline(
            &client,
            remote,
            vec![statement(
                "SELECT name,value FROM snapo_config WHERE name IN ('schema_version','dataset_id')",
                vec![],
            )],
        )?;
        for row in rows(&output[0])? {
            ensure!(row.len() == 2, "Ungültige Datenbank-Metadaten.");
            if let Some(key) = row[0].as_str() {
                report[key] = row[1].clone();
            }
        }
    }
    if tables.iter().any(|name| name == "session_revisions") {
        let output = pipeline(
            &client,
            remote,
            vec![statement("SELECT COUNT(*) FROM session_revisions", vec![])],
        )?;
        report["revision_count"] = rows(&output[0])?
            .first()
            .and_then(|row| row.first())
            .context("Revisionsanzahl fehlt.")?
            .clone();
    }
    if tables.iter().any(|name| name == "sessions") {
        let output = pipeline(
            &client,
            remote,
            vec![statement("SELECT COUNT(*) FROM sessions", vec![])],
        )?;
        report["legacy_session_count"] = rows(&output[0])?
            .first()
            .and_then(|row| row.first())
            .context("Sitzungsanzahl fehlt.")?
            .clone();
    }
    Ok(report)
}

pub fn synchronize(path: &Path, remote: &Remote) -> Result<()> {
    remote.validate()?;
    let client = Client::builder()
        .connect_timeout(Duration::from_secs(5))
        .timeout(Duration::from_secs(25))
        .redirect(reqwest::redirect::Policy::none())
        .build()?;
    let mut store = Store::open(path)?;
    // No remote schema creation/migration: existing Python v2 databases are the contract.
    let config = pipeline(
        &client,
        remote,
        vec![statement(
            "SELECT name,value FROM snapo_config WHERE name IN ('schema_version','dataset_id')",
            vec![],
        )],
    )?;
    let mut settings = HashMap::new();
    for row in rows(&config[0])? {
        ensure!(row.len() == 2, "Ungültige Datenbank-Metadaten.");
        settings.insert(
            row[0].as_str().unwrap_or_default().to_owned(),
            row[1].as_str().unwrap_or_default().to_owned(),
        );
    }
    ensure!(
        settings.get("schema_version").map(String::as_str) == Some("2"),
        "Turso-Datenbank benötigt das v2-Schema für Aftermath. Bitte zuerst migrate_db.py ausführen."
    );
    let dataset = settings
        .get("dataset_id")
        .filter(|s| !s.is_empty())
        .context("Datensatz-ID fehlt.")?;
    let output = pipeline(&client, remote, vec![statement(SELECT_REVISIONS, vec![])])?;
    let remote_revisions: Vec<_> = rows(&output[0])?
        .iter()
        .map(|r| decode_revision(r))
        .collect::<Result<_>>()?;
    store.merge_remote(dataset, &remote_revisions)?;
    let remote_ids: std::collections::HashSet<_> =
        remote_revisions.iter().map(|r| &r.revision_id).collect();
    let local = store.revisions()?;
    let pending: Vec<_> = local
        .iter()
        .filter(|r| !remote_ids.contains(&r.revision_id))
        .collect();
    for chunk in pending.chunks(100) {
        let requests = chunk
            .iter()
            .map(|r| {
                Ok(statement(
                    &INSERT_REVISION.replace("INSERT INTO", "INSERT OR IGNORE INTO"),
                    vec![
                        json!(r.revision_id),
                        json!(r.session_id),
                        json!(serde_json::to_string(&r.parents)?),
                        json!(r.device_id),
                        json!(r.created_at),
                        json!(serde_json::to_string(&r.session)?),
                        json!(i64::from(r.deleted)),
                        json!(r.legacy_id),
                    ],
                ))
            })
            .collect::<Result<Vec<_>>>()?;
        pipeline(&client, remote, requests)?;
        // Verify the rows before acknowledging INSERT OR IGNORE (ID collision or retry).
        let verified = pipeline(
            &client,
            remote,
            chunk
                .iter()
                .map(|r| {
                    statement(
                        &format!("{SELECT_REVISIONS} WHERE revision_id=?"),
                        vec![json!(r.revision_id)],
                    )
                })
                .collect(),
        )?;
        for (result, original) in verified.iter().zip(chunk) {
            let fetched = rows(result)?;
            ensure!(
                fetched.len() == 1 && decode_revision(&fetched[0])? == **original,
                "Turso hat eine Revision mit abweichendem Inhalt. Änderungen bleiben lokal vorgemerkt."
            );
        }
        store.acknowledge(
            &chunk
                .iter()
                .map(|r| r.revision_id.clone())
                .collect::<Vec<_>>(),
        )?;
    }
    store.mark_synced()?;
    Ok(())
}

/// Network errors must never expose authentication tokens in the UI or logs.
pub fn safe_error(error: &anyhow::Error, remote: &Remote) -> String {
    format!("{error:#}").replace(&remote.token, "[redacted]")
}
