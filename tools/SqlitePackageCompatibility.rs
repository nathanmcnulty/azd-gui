#![cfg(test)]

use rusqlite::{Connection, ErrorCode, OpenFlags};
use std::{fmt, path::Path};

enum AppError {
    Invalid(String),
    Database(rusqlite::Error),
}

impl fmt::Debug for AppError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Invalid(message) => f.write_str(message),
            Self::Database(error) => fmt::Debug::fmt(error, f),
        }
    }
}

impl From<rusqlite::Error> for AppError {
    fn from(error: rusqlite::Error) -> Self {
        Self::Database(error)
    }
}

type AppResult<T> = Result<T, AppError>;
include!("app_migration.rs");

const FILE: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/state.sqlite");
const TIMESTAMP: &str = "2026-11-23T00:04:00.123+00:00";

fn assert_integrity(db: &Connection) {
    assert_eq!(
        db.query_row("PRAGMA integrity_check", [], |row| row.get::<_, String>(0))
            .unwrap(),
        "ok"
    );
    assert!(!db
        .prepare("PRAGMA foreign_key_check")
        .unwrap()
        .exists([])
        .unwrap());
}

fn schema_version(db: &Connection) -> i64 {
    db.pragma_query_value(None, "user_version", |row| row.get(0))
        .unwrap()
}

fn set_app_pragmas(db: &Connection) {
    db.execute_batch("PRAGMA journal_mode = WAL; PRAGMA foreign_keys = ON;")
        .unwrap();
}

fn engine_version(db: &Connection) -> String {
    db.query_row("SELECT sqlite_version()", [], |row| row.get(0))
        .unwrap()
}

#[test]
fn seed_base() {
    assert!(!Path::new(FILE).exists());
    let db = Connection::open(FILE).unwrap();
    set_app_pragmas(&db);
    // Minimal v5 schema with the persisted project, template snapshot and operation data.
    db.execute_batch(
        "CREATE TABLE projects (
             id TEXT PRIMARY KEY, name TEXT NOT NULL, path TEXT NOT NULL UNIQUE,
             environment TEXT, template_id TEXT, revision TEXT, last_operation TEXT,
             state TEXT NOT NULL DEFAULT 'ready'
         );
         CREATE TABLE project_templates (
             project_path TEXT PRIMARY KEY REFERENCES projects(path) ON DELETE CASCADE,
             snapshot_json TEXT NOT NULL
         );
         CREATE TABLE operation_history (
             id TEXT PRIMARY KEY, project_path TEXT NOT NULL, kind TEXT NOT NULL,
             state TEXT NOT NULL, started_at TEXT NOT NULL, completed_at TEXT, summary TEXT
         );
         INSERT INTO projects (id, name, path, environment)
             VALUES ('fixture', 'Synthetic project', 'fixture-project', 'dev');
         INSERT INTO project_templates VALUES ('fixture-project', '{}');
         PRAGMA user_version = 5;",
    )
    .unwrap();
    db.execute(
        "UPDATE projects SET last_operation=?1 WHERE id='fixture'",
        [TIMESTAMP],
    )
    .unwrap();
    db.execute(
        "INSERT INTO operation_history (id, project_path, kind, state, started_at)
         VALUES ('fixture-op', 'fixture-project', 'preview', 'succeeded', ?1)",
        [TIMESTAMP],
    )
    .unwrap();
    assert_integrity(&db);
    assert_eq!(schema_version(&db), 5);

    let checkpoint = db
        .query_row("PRAGMA wal_checkpoint(TRUNCATE)", [], |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, i64>(1)?,
                row.get::<_, i64>(2)?,
            ))
        })
        .unwrap();
    assert_eq!(checkpoint, (0, 0, 0));
    println!(
        "Base bundled SQLite {} wrote and checkpointed schema 5",
        engine_version(&db)
    );
}

#[test]
fn upgrade_copy() {
    let mut db = Connection::open_with_flags(FILE, OpenFlags::SQLITE_OPEN_READ_WRITE).unwrap();
    set_app_pragmas(&db);
    assert_eq!(schema_version(&db), 5);
    migrate(&mut db).unwrap();
    assert_eq!(schema_version(&db), 6);
    assert_eq!(schema_version(&db), SCHEMA_VERSION);

    let read_project = |db: &Connection| -> (String, String, i64) {
        db.query_row(
            "SELECT environment, last_operation, selection_revision FROM projects WHERE id='fixture'",
            [],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .unwrap()
    };
    assert_eq!(read_project(&db), ("dev".into(), TIMESTAMP.into(), 0));
    assert_eq!(
        db.query_row(
            "SELECT started_at, context_json FROM operation_history WHERE id='fixture-op'",
            [],
            |row| Ok((row.get::<_, String>(0)?, row.get::<_, Option<String>>(1)?)),
        )
        .unwrap(),
        (TIMESTAMP.into(), None)
    );
    assert_eq!(
        db.query_row(
            "SELECT snapshot_json FROM project_templates WHERE project_path='fixture-project'",
            [],
            |row| row.get::<_, String>(0),
        )
        .unwrap(),
        "{}"
    );
    db.execute(
        "UPDATE projects SET environment='staging' WHERE id='fixture'",
        [],
    )
    .unwrap();
    let err = db
        .execute(
            "INSERT INTO project_templates VALUES ('missing-project', '{}')",
            [],
        )
        .unwrap_err();
    assert!(matches!(
        err,
        rusqlite::Error::SqliteFailure(error, _)
            if error.code == ErrorCode::ConstraintViolation && error.extended_code == rusqlite::ffi::SQLITE_CONSTRAINT_FOREIGNKEY
    ));
    assert_integrity(&db);
    drop(db);

    let mut reopened =
        Connection::open_with_flags(FILE, OpenFlags::SQLITE_OPEN_READ_WRITE).unwrap();
    set_app_pragmas(&reopened);
    migrate(&mut reopened).unwrap();
    assert_eq!(schema_version(&reopened), SCHEMA_VERSION);
    assert_eq!(
        read_project(&reopened),
        ("staging".into(), TIMESTAMP.into(), 1)
    );
    assert_integrity(&reopened);
    println!(
        "Candidate bundled SQLite {} migrated the copy, preserved rows, and reopened the committed update",
        engine_version(&reopened)
    );
}
