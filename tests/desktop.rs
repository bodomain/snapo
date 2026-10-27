use aftermath::{
    history::{self, Filter, Revision, Session, View},
    store::{SCHEMA, Store},
    sync::{self, Remote},
    timer::{Clock, Phase, Plan, Run},
};
use std::{
    path::Path,
    time::{Duration, Instant},
};
use tempfile::TempDir;

fn session(activity: &str, minutes: f64) -> Session {
    Session {
        date: "2026-01-01".into(),
        timestamp: "2026-01-01T12:00:00+01:00".into(),
        activity: activity.into(),
        duration_minutes: minutes,
        comment: "Notiz".into(),
    }
}
fn store(dir: &TempDir, name: &str) -> Store {
    Store::open(&dir.path().join(name)).unwrap()
}
fn revision(id: &str, parents: &[&str], activity: &str, deleted: bool) -> Revision {
    Revision {
        revision_id: id.into(),
        session_id: "shared".into(),
        parents: parents.iter().map(|s| s.to_string()).collect(),
        device_id: id.into(),
        created_at: "2026-01-01T12:00:00Z".into(),
        session: session(activity, 25.0),
        deleted,
        legacy_id: Some(1),
    }
}

#[test]
fn monotonic_timer_excludes_pauses_and_caps_work() {
    let now = Instant::now();
    let mut clock = Clock::new("Lernen".into(), "".into(), Some(1), now).unwrap();
    clock.toggle_pause(now + Duration::from_secs(10));
    assert_eq!(
        clock.elapsed(now + Duration::from_secs(100)),
        Duration::from_secs(10)
    );
    clock.toggle_pause(now + Duration::from_secs(100));
    assert_eq!(
        clock.elapsed(now + Duration::from_secs(120)),
        Duration::from_secs(30)
    );
    assert_eq!(
        clock.elapsed(now + Duration::from_secs(200)),
        Duration::from_secs(60)
    );
    assert!(clock.complete(now + Duration::from_secs(200)));
}

#[test]
fn partial_timer_save_is_frozen_and_idempotent_even_after_edit() {
    let dir = TempDir::new().unwrap();
    let mut db = store(&dir, "timer.db");
    let now = Instant::now();
    let mut clock = Clock::new("Lernen".into(), "".into(), None, now).unwrap();
    let saved = clock.snapshot(now + Duration::from_secs(90)).unwrap();
    assert_eq!(saved.duration_minutes, 1.5);
    assert_eq!(
        clock.snapshot(now + Duration::from_secs(900)).unwrap(),
        saved
    );
    clock.toggle_pause(now + Duration::from_secs(900));
    assert!(clock.paused);
    db.create(saved.clone(), Some(&clock.id)).unwrap();
    let entry = db.entries().unwrap().remove(0);
    db.change(
        &entry.session_id,
        &entry.head_ids(),
        session("Korrigiert", 10.0),
        false,
        false,
    )
    .unwrap();
    db.create(saved, Some(&clock.id)).unwrap();
    assert_eq!(db.revisions().unwrap().len(), 2);
    assert_eq!(
        db.entries().unwrap()[0].active().unwrap().activity,
        "Korrigiert"
    );
}

#[test]
fn cycles_take_long_break_after_four_and_no_final_break() {
    let now = Instant::now();
    let mut run = Run::new(
        Plan {
            cycles: 5,
            ..Default::default()
        },
        "Fokus".into(),
        "".into(),
        now,
    )
    .unwrap();
    for cycle in 1..=4 {
        assert_eq!(run.cycle, cycle);
        assert_eq!(run.phase, Phase::Work);
        assert!(run.advance(now).unwrap());
        assert_eq!(
            run.phase,
            if cycle == 4 {
                Phase::LongBreak
            } else {
                Phase::ShortBreak
            }
        );
        assert!(run.advance(now).unwrap());
    }
    assert_eq!(run.cycle, 5);
    assert!(!run.advance(now).unwrap());
    let run = Run::new(
        Plan {
            endless: true,
            ..Default::default()
        },
        "Lesen".into(),
        "".into(),
        now,
    )
    .unwrap();
    assert_eq!(run.clock.target, None);
    assert!(Clock::new(" ".into(), "".into(), Some(25), now).is_err());
}

#[test]
fn concurrent_offline_edits_converge_in_both_orders_and_resolve_delete() {
    let versions = vec![
        revision("root", &[], "Ursprung", false),
        revision("a", &["root"], "Änderung", false),
        revision("b", &["root"], "Ursprung", true),
    ];
    let dir = TempDir::new().unwrap();
    let mut left = store(&dir, "left.db");
    let mut right = store(&dir, "right.db");
    left.merge(&versions, false).unwrap();
    right
        .merge(&versions.iter().cloned().rev().collect::<Vec<_>>(), false)
        .unwrap();
    assert_eq!(left.entries().unwrap(), right.entries().unwrap());
    let entry = left.entries().unwrap().remove(0);
    assert!(entry.conflicted());
    assert!(history::totals(std::slice::from_ref(&entry), false).is_empty());
    assert!(
        left.change(
            "shared",
            &entry.head_ids(),
            session("Entscheidung", 30.0),
            false,
            false
        )
        .is_err()
    );
    left.change(
        "shared",
        &entry.head_ids(),
        session("Entscheidung", 30.0),
        true,
        true,
    )
    .unwrap();
    right.merge(&left.revisions().unwrap(), false).unwrap();
    let resolved = right.entries().unwrap().remove(0);
    assert!(!resolved.conflicted());
    assert!(resolved.current().unwrap().deleted);
    right
        .change(
            "shared",
            &resolved.head_ids(),
            resolved.current().unwrap().session.clone(),
            false,
            false,
        )
        .unwrap();
    assert_eq!(
        right.entries().unwrap()[0].active().unwrap().activity,
        "Entscheidung"
    );
}

#[test]
fn concurrent_resolutions_create_a_new_conflict() {
    let dir = TempDir::new().unwrap();
    let mut db = store(&dir, "conflict.db");
    let versions = [
        revision("a", &[], "A", false),
        revision("b", &[], "B", false),
        revision("resolve-a", &["a", "b"], "C", false),
        revision("resolve-b", &["a", "b"], "D", false),
    ];
    db.merge(&versions, false).unwrap();
    assert_eq!(db.entries().unwrap()[0].heads.len(), 2);
}

#[test]
fn stale_editor_cannot_overwrite_a_newer_revision() {
    let dir = TempDir::new().unwrap();
    let mut db = store(&dir, "stale.db");
    db.create(session("A", 25.0), None).unwrap();
    let stale = db.entries().unwrap().remove(0);
    db.change(
        &stale.session_id,
        &stale.head_ids(),
        session("B", 30.0),
        false,
        false,
    )
    .unwrap();
    assert!(
        db.change(
            &stale.session_id,
            &stale.head_ids(),
            session("C", 45.0),
            false,
            false
        )
        .is_err()
    );
    assert_eq!(db.entries().unwrap()[0].active().unwrap().activity, "B");
}

#[test]
fn collisions_roll_back_the_entire_merge() {
    let dir = TempDir::new().unwrap();
    let mut db = store(&dir, "atomic.db");
    db.merge(&[revision("a", &[], "A", false)], false).unwrap();
    assert!(
        db.merge(
            &[
                revision("b", &[], "B", false),
                revision("a", &[], "different", false)
            ],
            true
        )
        .is_err()
    );
    assert_eq!(db.revisions().unwrap().len(), 1);
    assert_eq!(db.pending().unwrap(), 1);
}

#[test]
fn first_cloud_binding_keeps_offline_work_and_refuses_switching_datasets() {
    let dir = TempDir::new().unwrap();
    let mut db = store(&dir, "binding.db");
    db.create(session("Offline", 25.0), None).unwrap();
    db.merge_remote("cloud-a", &[revision("a", &[], "Cloud", false)])
        .unwrap();
    assert_eq!(db.dataset().unwrap(), "cloud-a");
    assert_eq!(db.pending().unwrap(), 1);
    assert!(
        db.merge_remote("cloud-b", &[revision("b", &[], "Other", false)])
            .is_err()
    );
    assert_eq!(db.revisions().unwrap().len(), 2);
}

#[test]
fn reports_filters_recorded_dates_and_csv_are_consistent() {
    let dir = TempDir::new().unwrap();
    let mut db = store(&dir, "reports.db");
    db.create(session("=SUM(A1)", 90.0), None).unwrap();
    db.create(session("Lesen", 15.5), None).unwrap();
    let deleted = db.create(session("Gelöscht", 100.0), None).unwrap();
    db.change(
        &deleted.session_id,
        &[deleted.revision_id],
        deleted.session,
        true,
        false,
    )
    .unwrap();
    db.merge(
        &[
            revision("x", &[], "Konflikt", false),
            revision("y", &[], "Konflikt", false),
        ],
        false,
    )
    .unwrap();
    let entries = db.entries().unwrap();
    let daily = history::totals(&entries, false);
    assert_eq!(daily.values().sum::<f64>(), 105.5);
    assert_eq!(
        history::totals(&entries, true).get(&("2026-W01".into(), "Lesen".into())),
        Some(&15.5)
    );
    let filter = Filter {
        activity: "lesen".into(),
        since: "2026-01-01".into(),
        until: "2026-01-01".into(),
        ..Default::default()
    };
    filter.validate().unwrap();
    assert_eq!(
        entries
            .iter()
            .filter(|e| filter.matches(e, View::History))
            .count(),
        1
    );
    assert!(
        Filter {
            since: "2026-02-02".into(),
            until: "2026-01-01".into(),
            ..Default::default()
        }
        .validate()
        .is_err()
    );
    let csv = dir.path().join("history.csv");
    assert_eq!(history::export_csv(&entries, &csv).unwrap(), 2);
    let content = std::fs::read_to_string(&csv).unwrap();
    assert!(content.contains("'=SUM(A1)"));
    assert!(!content.contains("Gelöscht"));
    assert!(history::export_csv(&entries, &csv).is_err());
    assert_eq!(std::fs::read_to_string(csv).unwrap(), content);
}

#[test]
fn v2_python_import_preserves_ids_conflicts_and_source_bytes() {
    let dir = TempDir::new().unwrap();
    let source = dir.path().join("python.db");
    let python = rusqlite::Connection::open(&source).unwrap();
    python.execute_batch(SCHEMA).unwrap();
    python.execute("INSERT INTO snapo_config VALUES ('schema_version','2'), ('dataset_id','python-dataset')", []).unwrap();
    for r in [
        revision("a", &[], "A", false),
        revision("b", &[], "B", true),
    ] {
        python
            .execute(
                aftermath::store::INSERT_REVISION,
                rusqlite::params![
                    r.revision_id,
                    r.session_id,
                    "[]",
                    r.device_id,
                    r.created_at,
                    serde_json::to_string(&r.session).unwrap(),
                    r.deleted,
                    r.legacy_id
                ],
            )
            .unwrap();
    }
    drop(python);
    let before = std::fs::read(&source).unwrap();
    let mut db = store(&dir, "import.db");
    assert_eq!(db.import(&source).unwrap(), 2);
    assert_eq!(db.import(&source).unwrap(), 0);
    assert!(db.entries().unwrap()[0].conflicted());
    assert_eq!(db.dataset().unwrap(), "python-dataset");
    assert_eq!(std::fs::read(source).unwrap(), before);
    assert!(db.merge_remote("other-dataset", &[]).is_err());
}

#[test]
fn legacy_import_is_read_only_idempotent_and_atomic_on_changed_source() {
    let dir = TempDir::new().unwrap();
    let source = dir.path().join("legacy.db");
    let legacy = rusqlite::Connection::open(&source).unwrap();
    legacy.execute_batch("CREATE TABLE sessions (id INTEGER PRIMARY KEY,date TEXT,timestamp TEXT,activity TEXT,duration_minutes REAL);
        INSERT INTO sessions VALUES (1,'2026-01-01','2026-01-01T12:00:00','Lesen',25);").unwrap();
    let mut db = store(&dir, "desktop.db");
    let before = std::fs::read(&source).unwrap();
    assert_eq!(db.import(&source).unwrap(), 1);
    assert_eq!(db.import(&source).unwrap(), 0);
    assert_eq!(std::fs::read(&source).unwrap(), before);
    legacy
        .execute("UPDATE sessions SET duration_minutes=30", [])
        .unwrap();
    assert!(db.import(&source).is_err());
    assert_eq!(db.revisions().unwrap().len(), 1);
}

#[test]
fn replicas_and_unknown_databases_are_rejected_before_changes() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("replica.db");
    std::fs::write(dir.path().join("replica.db-info"), "engine metadata").unwrap();
    assert!(Store::open(&path).is_err());
    assert!(!path.exists());
    #[cfg(unix)]
    {
        let actual = dir.path().join("engine.db");
        std::fs::write(&actual, "replica bytes").unwrap();
        std::fs::write(dir.path().join("engine.db-info"), "metadata").unwrap();
        let alias = dir.path().join("alias.db");
        std::os::unix::fs::symlink(&actual, &alias).unwrap();
        assert!(Store::open(&alias).is_err());
        assert_eq!(std::fs::read_to_string(actual).unwrap(), "replica bytes");
    }
    let unknown = dir.path().join("unknown.db");
    let conn = rusqlite::Connection::open(&unknown).unwrap();
    conn.execute("CREATE TABLE other (x)", []).unwrap();
    drop(conn);
    let before = std::fs::read(&unknown).unwrap();
    assert!(Store::open(&unknown).is_err());
    assert_eq!(std::fs::read(unknown).unwrap(), before);
}

#[test]
fn cli_checks_exports_and_refuses_network_in_local_mode() {
    use std::process::Command;
    let dir = TempDir::new().unwrap();
    let mut db = store(&dir, "cli.db");
    db.create(session("CLI", 25.0), None).unwrap();
    let binary = env!("CARGO_BIN_EXE_aftermath");
    let checked = Command::new(binary)
        .args(["--db", db.path.to_str().unwrap(), "check"])
        .output()
        .unwrap();
    assert!(checked.status.success());
    assert!(String::from_utf8_lossy(&checked.stdout).contains("OK"));
    let exported = dir.path().join("export.csv");
    assert!(
        Command::new(binary)
            .args([
                "--db",
                db.path.to_str().unwrap(),
                "export",
                exported.to_str().unwrap()
            ])
            .status()
            .unwrap()
            .success()
    );
    assert!(std::fs::read_to_string(exported).unwrap().contains("CLI"));
    let refused = Command::new(binary)
        .args(["--local", "sync"])
        .output()
        .unwrap();
    assert!(!refused.status.success());
    assert!(String::from_utf8_lossy(&refused.stderr).contains("--local"));
}

#[test]
fn invalid_data_is_rejected_and_tokens_are_redacted() {
    for minutes in [0.0, -1.0, f64::NAN, f64::INFINITY] {
        assert!(session("Test", minutes).validated().is_err());
    }
    let remote = Remote {
        url: "https://example.turso.io".into(),
        token: "secret-token".into(),
    };
    assert!(
        !sync::safe_error(&anyhow::anyhow!("bad secret-token"), &remote).contains("secret-token")
    );
    assert!(
        Remote {
            url: "http://example.com".into(),
            token: "x".into()
        }
        .validate()
        .is_err()
    );
    assert!(
        Remote {
            url: "https://user:pass@example.com".into(),
            token: "x".into()
        }
        .validate()
        .is_err()
    );
}

#[test]
fn migrated_legacy_timestamps_are_preserved_and_can_be_trashed_and_corrected() {
    let dir = TempDir::new().unwrap();
    let mut db = store(&dir, "old-data.db");
    let mut legacy = revision("old", &[], "Altdaten", false);
    legacy.session.timestamp = "2026-01-01T:00:00".into();
    db.merge_remote("legacy-dataset", &[legacy.clone()])
        .unwrap();
    assert_eq!(db.revisions().unwrap()[0], legacy);
    db.check().unwrap();
    let entry = db.entries().unwrap().remove(0);
    db.set_deleted(&entry.session_id, &entry.head_ids(), true)
        .unwrap();
    let trashed = db.entries().unwrap().remove(0);
    assert_eq!(
        trashed.current().unwrap().session.timestamp,
        legacy.session.timestamp
    );
    assert!(
        db.change(
            &trashed.session_id,
            &trashed.head_ids(),
            legacy.session.clone(),
            false,
            false
        )
        .is_err()
    );
    db.set_deleted(&trashed.session_id, &trashed.head_ids(), false)
        .unwrap();
    let restored = db.entries().unwrap().remove(0);
    db.change(
        &restored.session_id,
        &restored.head_ids(),
        session("Korrigiert", 25.0),
        false,
        false,
    )
    .unwrap();
    assert_eq!(
        db.entries().unwrap()[0].active().unwrap().activity,
        "Korrigiert"
    );
    let mut invalid_new = legacy;
    invalid_new.revision_id = "new-invalid".into();
    invalid_new.legacy_id = None;
    assert!(db.merge(&[invalid_new], false).is_err());
}

// A real HTTP listener and SQLite engine exercise the wire protocol and the
// complete transport lifecycle without ever loading the user's credentials.
struct Cloud {
    remote: Remote,
    stop: std::sync::Arc<std::sync::atomic::AtomicBool>,
    thread: Option<std::thread::JoinHandle<()>>,
}
impl Cloud {
    fn start(path: &Path) -> Self {
        use serde_json::{Value, json};
        use std::{
            io::{Read, Write},
            net::TcpListener,
            sync::{
                Arc,
                atomic::{AtomicBool, Ordering},
            },
        };
        let path = path.to_owned();
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        let stop = Arc::new(AtomicBool::new(false));
        let signal = stop.clone();
        let thread = std::thread::spawn(move || {
            let conn = rusqlite::Connection::open(path).unwrap();
            conn.execute_batch(SCHEMA).unwrap();
            conn.execute("INSERT INTO snapo_config VALUES ('schema_version','2'),('dataset_id','cloud-test')", []).unwrap();
            while !signal.load(Ordering::Relaxed) {
                let (mut stream, _) = match listener.accept() {
                    Ok(s) => s,
                    Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                        std::thread::sleep(Duration::from_millis(5));
                        continue;
                    }
                    Err(e) => panic!("{e}"),
                };
                stream
                    .set_read_timeout(Some(Duration::from_secs(5)))
                    .unwrap();
                let mut bytes = Vec::new();
                let mut buf = [0; 4096];
                let header_end = loop {
                    let n = stream.read(&mut buf).unwrap();
                    assert!(n > 0);
                    bytes.extend_from_slice(&buf[..n]);
                    if let Some(i) = bytes.windows(4).position(|w| w == b"\r\n\r\n") {
                        break i + 4;
                    }
                };
                let headers = String::from_utf8_lossy(&bytes[..header_end]).to_lowercase();
                assert!(headers.starts_with("post /v2/pipeline "));
                assert!(headers.contains("authorization: bearer disposable-test-token"));
                let length: usize = headers
                    .lines()
                    .find_map(|line| line.strip_prefix("content-length:"))
                    .unwrap()
                    .trim()
                    .parse()
                    .unwrap();
                while bytes.len() < header_end + length {
                    let n = stream.read(&mut buf).unwrap();
                    assert!(n > 0);
                    bytes.extend_from_slice(&buf[..n]);
                }
                let request: Value =
                    serde_json::from_slice(&bytes[header_end..header_end + length]).unwrap();
                let results: Vec<_> = request["requests"].as_array().unwrap().iter().map(|request| {
                    if request["type"] == "close" { return json!({"type":"ok","response":{"type":"close"}}); }
                    let sql = request["stmt"]["sql"].as_str().unwrap();
                    let params: Vec<rusqlite::types::Value> = request["stmt"]["args"].as_array().unwrap().iter().map(|arg| match arg["type"].as_str().unwrap() {
                        "text" => rusqlite::types::Value::Text(arg["value"].as_str().unwrap().into()),
                        "integer" => rusqlite::types::Value::Integer(arg["value"].as_str().unwrap().parse().unwrap()),
                        "null" => rusqlite::types::Value::Null, other => panic!("{other}"),
                    }).collect();
                    let result = (|| -> rusqlite::Result<Value> {
                        let mut stmt = conn.prepare(sql)?; let columns = stmt.column_count();
                        let mut query = stmt.query(rusqlite::params_from_iter(params))?; let mut rows = vec![];
                        while let Some(row) = query.next()? {
                            let mut fields = vec![];
                            for i in 0..columns { fields.push(match row.get_ref(i)? {
                                rusqlite::types::ValueRef::Text(s) => json!({"type":"text","value":std::str::from_utf8(s).unwrap()}),
                                rusqlite::types::ValueRef::Integer(n) => json!({"type":"integer","value":n.to_string()}),
                                rusqlite::types::ValueRef::Null => json!({"type":"null"}), _ => unreachable!(),
                            }); }
                            rows.push(fields);
                        }
                        Ok(json!({"rows":rows}))
                    })();
                    match result { Ok(result) => json!({"type":"ok","response":{"type":"execute","result":result}}), Err(e) => json!({"type":"error","error":{"message":e.to_string()}}) }
                }).collect();
                let body = serde_json::to_vec(&json!({"results":results})).unwrap();
                write!(stream, "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n", body.len()).unwrap();
                stream.write_all(&body).unwrap();
            }
        });
        Self {
            remote: Remote {
                url,
                token: "disposable-test-token".into(),
            },
            stop,
            thread: Some(thread),
        }
    }
}
impl Drop for Cloud {
    fn drop(&mut self) {
        self.stop.store(true, std::sync::atomic::Ordering::Relaxed);
        self.thread.take().unwrap().join().unwrap();
    }
}

#[test]
fn http_transport_round_trip_offline_conflict_resolution_restore_and_reopen() {
    let dir = TempDir::new().unwrap();
    let cloud = Cloud::start(&dir.path().join("cloud.db"));
    let metadata = sync::inspect(&cloud.remote).unwrap();
    assert_eq!(metadata["schema_version"], "2");
    assert_eq!(metadata["revision_count"], 0);
    let mut left = store(&dir, "left.db");
    let mut right = store(&dir, "right.db");
    left.create(session("Offline vor erster Verbindung", 25.0), None)
        .unwrap();
    sync::synchronize(&left.path, &cloud.remote).unwrap();
    sync::synchronize(&right.path, &cloud.remote).unwrap();
    assert_eq!(left.pending().unwrap(), 0);
    assert!(left.sync_status().unwrap().is_some());
    let entry = left.entries().unwrap().remove(0);
    left.change(
        &entry.session_id,
        &entry.head_ids(),
        session("Laptop", 30.0),
        false,
        false,
    )
    .unwrap();
    right
        .change(
            &entry.session_id,
            &entry.head_ids(),
            session("Desktop", 40.0),
            true,
            false,
        )
        .unwrap();
    sync::synchronize(&right.path, &cloud.remote).unwrap();
    sync::synchronize(&left.path, &cloud.remote).unwrap();
    sync::synchronize(&right.path, &cloud.remote).unwrap();
    assert_eq!(left.entries().unwrap(), right.entries().unwrap());
    let conflict = left.entries().unwrap().remove(0);
    assert!(conflict.conflicted());
    left.change(
        &conflict.session_id,
        &conflict.head_ids(),
        session("Aufgelöst", 35.0),
        true,
        true,
    )
    .unwrap();
    sync::synchronize(&left.path, &cloud.remote).unwrap();
    sync::synchronize(&right.path, &cloud.remote).unwrap();
    let deleted = right.entries().unwrap().remove(0);
    right
        .change(
            &deleted.session_id,
            &deleted.head_ids(),
            deleted.current().unwrap().session.clone(),
            false,
            false,
        )
        .unwrap();
    sync::synchronize(&right.path, &cloud.remote).unwrap();
    sync::synchronize(&left.path, &cloud.remote).unwrap();
    assert_eq!(left.entries().unwrap(), right.entries().unwrap());
    let path = left.path.clone();
    drop(left);
    let reopened = Store::open(&path).unwrap();
    assert_eq!(
        reopened.entries().unwrap()[0].active().unwrap().activity,
        "Aufgelöst"
    );
    assert_eq!(reopened.pending().unwrap(), 0);
}

#[test]
fn failed_transport_keeps_local_work_pending() {
    let dir = TempDir::new().unwrap();
    let mut db = store(&dir, "offline.db");
    db.create(session("Offline", 25.0), None).unwrap();
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    drop(listener);
    let remote = Remote {
        url: format!("http://{address}"),
        token: "disposable".into(),
    };
    assert!(sync::synchronize(&db.path, &remote).is_err());
    assert_eq!(db.pending().unwrap(), 1);
    assert!(db.sync_status().unwrap().is_none());
}
