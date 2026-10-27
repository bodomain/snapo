use crate::history::{Entry, Revision, Session, reconstruct};
use anyhow::{Context, Result, bail, ensure};
use chrono::Local;
use rusqlite::{Connection, OpenFlags, OptionalExtension, TransactionBehavior, params};
use std::{
    collections::HashSet,
    path::{Path, PathBuf},
    time::Duration,
};
use uuid::Uuid;

pub const SCHEMA: &str = "CREATE TABLE IF NOT EXISTS session_revisions (
    revision_id TEXT PRIMARY KEY, session_id TEXT NOT NULL, parents TEXT NOT NULL,
    device_id TEXT NOT NULL, created_at TEXT NOT NULL, payload TEXT NOT NULL,
    deleted INTEGER NOT NULL, legacy_id INTEGER);
    CREATE INDEX IF NOT EXISTS session_revisions_session ON session_revisions(session_id);
    CREATE TABLE IF NOT EXISTS snapo_config (name TEXT PRIMARY KEY, value TEXT NOT NULL);";
pub const SELECT_REVISIONS: &str = "SELECT revision_id,session_id,parents,device_id,created_at,payload,deleted,legacy_id FROM session_revisions";
pub const INSERT_REVISION: &str = "INSERT INTO session_revisions (revision_id,session_id,parents,device_id,created_at,payload,deleted,legacy_id) VALUES (?,?,?,?,?,?,?,?)";

pub struct Store {
    conn: Connection,
    pub path: PathBuf,
    device_id: String,
}

impl Store {
    pub fn open(path: &Path) -> Result<Self> {
        // Turso's replica format must only be opened by its own engine.
        refuse_replica(path)?;
        if let Some(parent) = path.parent().filter(|p| !p.as_os_str().is_empty()) {
            std::fs::create_dir_all(parent)?;
        }
        let mut conn = Connection::open(path).context("Datenbank lässt sich nicht öffnen")?;
        conn.busy_timeout(Duration::from_secs(5))?;
        // Refuse legacy and unknown schemas before changing journal mode or tables.
        let tables: Vec<String> = conn
            .prepare("SELECT name FROM sqlite_master WHERE type='table'")?
            .query_map([], |r| r.get(0))?
            .collect::<rusqlite::Result<_>>()?;
        if tables.iter().any(|s| s == "snapo_config") {
            let version: Option<String> = conn
                .query_row(
                    "SELECT value FROM snapo_config WHERE name='schema_version'",
                    [],
                    |r| r.get(0),
                )
                .optional()?;
            ensure!(
                version.as_deref() == Some("2"),
                "Unbekannte Datenbankversion. Bitte Aftermath aktualisieren."
            );
            ensure!(
                tables.iter().any(|s| s == "session_revisions"),
                "Die Revisionstabelle fehlt. Datenbank bitte wiederherstellen."
            );
            let dataset: Option<String> = conn
                .query_row(
                    "SELECT value FROM snapo_config WHERE name='dataset_id'",
                    [],
                    |r| r.get(0),
                )
                .optional()?;
            ensure!(
                dataset.is_some_and(|s| !s.is_empty()),
                "Die Datensatz-ID fehlt. Datenbank bitte wiederherstellen."
            );
        } else if !tables.is_empty() {
            bail!(
                "Bestehende Datenbank benötigt eine Migration. Bitte zuerst migrate_db.py verwenden."
            );
        }
        conn.pragma_update(None, "journal_mode", "WAL")?;
        let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
        tx.execute_batch(SCHEMA)?;
        for (key, value) in [
            ("schema_version", "2".to_owned()),
            ("dataset_id", Uuid::new_v4().to_string()),
            ("desktop_device_id", Uuid::new_v4().to_string()),
        ] {
            tx.execute(
                "INSERT OR IGNORE INTO snapo_config VALUES (?,?)",
                params![key, value],
            )?;
        }
        tx.execute_batch(
            "CREATE TABLE IF NOT EXISTS desktop_sync_ack (revision_id TEXT PRIMARY KEY);",
        )?;
        tx.commit()?;
        let device_id = conn.query_row(
            "SELECT value FROM snapo_config WHERE name='desktop_device_id'",
            [],
            |r| r.get(0),
        )?;
        Ok(Self {
            conn,
            path: path.to_owned(),
            device_id,
        })
    }

    pub fn revisions(&self) -> Result<Vec<Revision>> {
        read_revisions(&self.conn)
    }
    pub fn check(&self) -> Result<()> {
        let status: String = self
            .conn
            .query_row("PRAGMA integrity_check", [], |r| r.get(0))?;
        ensure!(status == "ok", "Datenbankprüfung: {status}");
        for revision in self.revisions()? {
            revision.validate_replication()?;
        }
        Ok(())
    }
    pub fn entries(&self) -> Result<Vec<Entry>> {
        Ok(reconstruct(self.revisions()?))
    }
    pub fn dataset(&self) -> Result<String> {
        Ok(self.conn.query_row(
            "SELECT value FROM snapo_config WHERE name='dataset_id'",
            [],
            |r| r.get(0),
        )?)
    }
    pub fn set_dataset(&mut self, dataset: &str) -> Result<()> {
        let tx = self
            .conn
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let count: i64 =
            tx.query_row("SELECT COUNT(*) FROM session_revisions", [], |r| r.get(0))?;
        ensure!(
            count == 0,
            "Diese lokale Datenbank gehört zu einem anderen Datensatz. Bitte eine neue Desktop-Datenbank verwenden."
        );
        tx.execute(
            "UPDATE snapo_config SET value=? WHERE name='dataset_id'",
            [dataset],
        )?;
        tx.commit()?;
        Ok(())
    }
    pub fn create(&mut self, session: Session, session_id: Option<&str>) -> Result<Revision> {
        let session = session.validated()?;
        let sid = session_id
            .map(str::to_owned)
            .unwrap_or_else(|| Uuid::new_v4().to_string());
        let tx = self
            .conn
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let revisions = read_revisions(&tx)?;
        let existing: Vec<_> = revisions.iter().filter(|r| r.session_id == sid).collect();
        if !existing.is_empty() {
            let roots: Vec<_> = existing.iter().filter(|r| r.parents.is_empty()).collect();
            if roots.len() == 1 && roots[0].session == session {
                return Ok((*roots[0]).clone());
            }
            bail!("Diese Sitzung wurde bereits mit anderem Inhalt gespeichert.");
        }
        let revision = Revision {
            revision_id: Uuid::new_v4().to_string(),
            session_id: sid,
            parents: vec![],
            device_id: self.device_id.clone(),
            created_at: Local::now().to_rfc3339(),
            session,
            deleted: false,
            legacy_id: None,
        };
        append(&tx, &revision)?;
        tx.commit()?;
        Ok(revision)
    }
    pub fn change(
        &mut self,
        sid: &str,
        base: &[String],
        session: Session,
        deleted: bool,
        resolve: bool,
    ) -> Result<()> {
        self.change_inner(sid, base, Some(session.validated()?), deleted, resolve)
    }

    pub fn set_deleted(&mut self, sid: &str, base: &[String], deleted: bool) -> Result<()> {
        self.change_inner(sid, base, None, deleted, false)
    }

    fn change_inner(
        &mut self,
        sid: &str,
        base: &[String],
        session: Option<Session>,
        deleted: bool,
        resolve: bool,
    ) -> Result<()> {
        let tx = self
            .conn
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let entry = reconstruct(read_revisions(&tx)?)
            .into_iter()
            .find(|e| e.session_id == sid)
            .context("Sitzung nicht gefunden.")?;
        ensure!(
            base.iter().collect::<HashSet<_>>() == entry.head_ids().iter().collect::<HashSet<_>>(),
            "Die Sitzung wurde inzwischen geändert. Bitte den Editor neu öffnen."
        );
        ensure!(
            !entry.conflicted() || resolve,
            "Bitte zuerst den Konflikt auflösen."
        );
        let revision = Revision {
            revision_id: Uuid::new_v4().to_string(),
            session_id: sid.to_owned(),
            parents: entry.head_ids(),
            device_id: self.device_id.clone(),
            created_at: Local::now().to_rfc3339(),
            session: session.unwrap_or_else(|| entry.heads[0].session.clone()),
            deleted,
            legacy_id: entry.heads[0].legacy_id,
        };
        append(&tx, &revision)?;
        tx.commit()?;
        Ok(())
    }
    pub fn merge(&mut self, revisions: &[Revision], acknowledged: bool) -> Result<()> {
        let tx = self
            .conn
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        for revision in revisions {
            revision.validate_replication()?;
            append(&tx, revision)?;
            if acknowledged {
                tx.execute(
                    "INSERT OR IGNORE INTO desktop_sync_ack VALUES (?)",
                    [&revision.revision_id],
                )?;
            }
        }
        tx.commit()?;
        Ok(())
    }
    /// Bind and receive in one transaction. A first explicit sync also uploads
    /// offline work, but can never switch an already bound/imported dataset.
    pub fn merge_remote(&mut self, dataset: &str, revisions: &[Revision]) -> Result<()> {
        ensure!(!dataset.is_empty(), "Datensatz-ID fehlt.");
        let tx = self
            .conn
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let binding: Option<String> = tx
            .query_row(
                "SELECT value FROM snapo_config WHERE name='desktop_remote_dataset'",
                [],
                |r| r.get(0),
            )
            .optional()?;
        if let Some(binding) = binding {
            ensure!(
                binding == dataset,
                "Diese Desktop-Datenbank ist bereits mit einem anderen Datensatz verbunden. Bitte --db mit einem neuen Pfad verwenden."
            );
        } else {
            tx.execute(
                "UPDATE snapo_config SET value=? WHERE name='dataset_id'",
                [dataset],
            )?;
            tx.execute(
                "INSERT INTO snapo_config VALUES ('desktop_remote_dataset',?)",
                [dataset],
            )?;
        }
        for revision in revisions {
            revision.validate_replication()?;
            append(&tx, revision)?;
            tx.execute(
                "INSERT OR IGNORE INTO desktop_sync_ack VALUES (?)",
                [&revision.revision_id],
            )?;
        }
        tx.commit()?;
        Ok(())
    }

    /// Read-only snapshot import. V2 IDs and all competing versions survive;
    /// legacy IDs are deterministic so repeating an import is idempotent.
    pub fn import(&mut self, source: &Path) -> Result<usize> {
        refuse_replica(source)?;
        ensure!(source.exists(), "Die Quelldatenbank existiert nicht.");
        ensure!(
            source.canonicalize()? != self.path.canonicalize()?,
            "Quelle und Ziel müssen verschieden sein."
        );
        let mut conn = Connection::open_with_flags(source, OpenFlags::SQLITE_OPEN_READ_ONLY)?;
        let snapshot = conn.transaction()?;
        let tables: Vec<String> = snapshot
            .prepare("SELECT name FROM sqlite_master WHERE type='table'")?
            .query_map([], |r| r.get(0))?
            .collect::<rusqlite::Result<_>>()?;
        let (revisions, dataset) = if tables.iter().any(|s| s == "snapo_config") {
            let version: String = snapshot.query_row(
                "SELECT value FROM snapo_config WHERE name='schema_version'",
                [],
                |r| r.get(0),
            )?;
            ensure!(version == "2", "Unbekannte Version der Quelldatenbank.");
            let dataset: String = snapshot.query_row(
                "SELECT value FROM snapo_config WHERE name='dataset_id'",
                [],
                |r| r.get(0),
            )?;
            (read_revisions(&snapshot)?, Some(dataset))
        } else {
            ensure!(
                tables.iter().any(|s| s == "sessions"),
                "Keine unterstützten Sitzungen in der Quelldatenbank gefunden."
            );
            let columns: Vec<String> = snapshot
                .prepare("PRAGMA table_info(sessions)")?
                .query_map([], |r| r.get(1))?
                .collect::<rusqlite::Result<_>>()?;
            let comment = if columns.iter().any(|c| c == "comment") {
                "COALESCE(comment,'')"
            } else {
                "''"
            };
            let namespace = Uuid::new_v5(
                &Uuid::NAMESPACE_URL,
                source.canonicalize()?.to_string_lossy().as_bytes(),
            );
            let revisions = snapshot.prepare(&format!("SELECT id,date,timestamp,activity,duration_minutes,{comment} FROM sessions ORDER BY id"))?
                .query_map([], |r| {
                    let id: i64 = r.get(0)?;
                    let session = Session { date: r.get(1)?, timestamp: r.get(2)?, activity: r.get(3)?, duration_minutes: r.get(4)?, comment: r.get(5)? };
                    Ok(Revision { revision_id: Uuid::new_v5(&namespace, format!("revision:{id}").as_bytes()).to_string(),
                        session_id: Uuid::new_v5(&namespace, format!("session:{id}").as_bytes()).to_string(),
                        parents: vec![], device_id: "desktop-import".into(), created_at: session.timestamp.clone(),
                        session, deleted: false, legacy_id: Some(id) })
                })?.collect::<rusqlite::Result<Vec<_>>>()?;
            (revisions, None)
        };
        let tx = self
            .conn
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        if let Some(dataset) = dataset {
            ensure!(!dataset.is_empty(), "Datensatz-ID fehlt in der Quelle.");
            let current: String = tx.query_row(
                "SELECT value FROM snapo_config WHERE name='dataset_id'",
                [],
                |r| r.get(0),
            )?;
            let count: i64 =
                tx.query_row("SELECT COUNT(*) FROM session_revisions", [], |r| r.get(0))?;
            let binding: Option<String> = tx
                .query_row(
                    "SELECT value FROM snapo_config WHERE name='desktop_remote_dataset'",
                    [],
                    |r| r.get(0),
                )
                .optional()?;
            ensure!(
                (count == 0 && binding.is_none()) || current == dataset,
                "Die Quelle gehört zu einem anderen Datensatz. Bitte eine neue Desktop-Datenbank als Ziel wählen."
            );
            tx.execute(
                "UPDATE snapo_config SET value=? WHERE name='dataset_id'",
                [&dataset],
            )?;
            tx.execute(
                "INSERT OR IGNORE INTO snapo_config VALUES ('desktop_remote_dataset',?)",
                [&dataset],
            )?;
        }
        let before: i64 =
            tx.query_row("SELECT COUNT(*) FROM session_revisions", [], |r| r.get(0))?;
        for revision in &revisions {
            revision.validate_replication()?;
            append(&tx, revision)?;
        }
        let after: i64 =
            tx.query_row("SELECT COUNT(*) FROM session_revisions", [], |r| r.get(0))?;
        tx.commit()?;
        Ok((after - before) as usize)
    }
    pub fn acknowledge(&mut self, ids: &[String]) -> Result<()> {
        let tx = self.conn.transaction()?;
        for id in ids {
            tx.execute("INSERT OR IGNORE INTO desktop_sync_ack VALUES (?)", [id])?;
        }
        tx.commit()?;
        Ok(())
    }
    pub fn pending(&self) -> Result<usize> {
        Ok(self.conn.query_row("SELECT COUNT(*) FROM session_revisions WHERE revision_id NOT IN (SELECT revision_id FROM desktop_sync_ack)", [], |r| r.get(0))?)
    }
    pub fn sync_status(&self) -> Result<Option<String>> {
        Ok(self
            .conn
            .query_row(
                "SELECT value FROM snapo_config WHERE name='desktop_last_sync'",
                [],
                |r| r.get(0),
            )
            .optional()?)
    }
    pub fn mark_synced(&self) -> Result<()> {
        self.conn.execute("INSERT INTO snapo_config VALUES ('desktop_last_sync',?) ON CONFLICT(name) DO UPDATE SET value=excluded.value", [Local::now().to_rfc3339()])?;
        Ok(())
    }
}

fn refuse_replica(path: &Path) -> Result<()> {
    let canonical = path.canonicalize().ok();
    for path in std::iter::once(path).chain(canonical.as_deref()) {
        for suffix in ["-info", "-changes", "-wal-revert"] {
            let mut sidecar = path.as_os_str().to_owned();
            sidecar.push(suffix);
            ensure!(
                !PathBuf::from(sidecar).exists(),
                "Turso-Replikat erkannt. Bitte eine eigene Desktop-Datenbank wählen; bestehende Cloud-Daten werden über --sync geladen. Lokale Replikatänderungen zuerst mit dem Python-Client synchronisieren."
            );
        }
    }
    Ok(())
}

fn read_revisions(conn: &Connection) -> Result<Vec<Revision>> {
    let mut stmt = conn.prepare(SELECT_REVISIONS)?;
    let rows = stmt.query_map([], |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, String>(2)?,
            row.get::<_, String>(3)?,
            row.get::<_, String>(4)?,
            row.get::<_, String>(5)?,
            row.get::<_, bool>(6)?,
            row.get::<_, Option<i64>>(7)?,
        ))
    })?;
    let mut revisions = Vec::new();
    for row in rows {
        let (revision_id, session_id, parents, device_id, created_at, payload, deleted, legacy_id) =
            row?;
        revisions.push(Revision {
            revision_id,
            session_id,
            parents: serde_json::from_str(&parents)?,
            device_id,
            created_at,
            session: serde_json::from_str(&payload)?,
            deleted,
            legacy_id,
        });
    }
    Ok(revisions)
}

fn append(conn: &Connection, r: &Revision) -> Result<()> {
    // Compare decoded contents: Python and Rust JSON whitespace can differ.
    let existing = conn
        .query_row(
            &format!("{SELECT_REVISIONS} WHERE revision_id=?"),
            [&r.revision_id],
            |row| {
                Ok((
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, String>(4)?,
                    row.get::<_, String>(5)?,
                    row.get::<_, bool>(6)?,
                    row.get::<_, Option<i64>>(7)?,
                ))
            },
        )
        .optional()?;
    if let Some((sid, parents, device, created, payload, deleted, legacy)) = existing {
        ensure!(
            sid == r.session_id
                && serde_json::from_str::<Vec<String>>(&parents)? == r.parents
                && device == r.device_id
                && created == r.created_at
                && serde_json::from_str::<Session>(&payload)? == r.session
                && deleted == r.deleted
                && legacy == r.legacy_id,
            "Revisions-ID mit abweichendem Inhalt gefunden."
        );
        return Ok(());
    }
    conn.execute(
        INSERT_REVISION,
        params![
            r.revision_id,
            r.session_id,
            serde_json::to_string(&r.parents)?,
            r.device_id,
            r.created_at,
            serde_json::to_string(&r.session)?,
            r.deleted,
            r.legacy_id
        ],
    )?;
    Ok(())
}
