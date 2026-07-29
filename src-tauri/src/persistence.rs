use crate::{
    error::{AppError, AppResult},
    models::{CatalogSource, OperationEvent, ProjectSummary},
};
use rusqlite::{params, Connection};
use std::{path::Path, sync::Mutex};

pub struct Database {
    connection: Mutex<Connection>,
}

impl Database {
    pub fn open(path: &Path) -> AppResult<Self> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let connection = Connection::open(path)?;
        connection.execute_batch(
            "
            PRAGMA journal_mode = WAL;
            PRAGMA foreign_keys = ON;
            CREATE TABLE IF NOT EXISTS projects (
                id TEXT PRIMARY KEY,
                name TEXT NOT NULL,
                path TEXT NOT NULL UNIQUE,
                environment TEXT,
                template_id TEXT,
                revision TEXT,
                last_operation TEXT,
                state TEXT NOT NULL DEFAULT 'ready'
            );
            CREATE TABLE IF NOT EXISTS catalog_sources (
                id TEXT PRIMARY KEY,
                label TEXT NOT NULL,
                url TEXT NOT NULL UNIQUE,
                trusted INTEGER NOT NULL DEFAULT 0,
                pinned_revision TEXT,
                accepted_catalog_json TEXT
            );
            CREATE TABLE IF NOT EXISTS operation_history (
                id TEXT PRIMARY KEY,
                project_path TEXT NOT NULL,
                kind TEXT NOT NULL,
                state TEXT NOT NULL,
                started_at TEXT NOT NULL,
                completed_at TEXT,
                summary TEXT
            );
            CREATE TABLE IF NOT EXISTS wizard_drafts (
                project_key TEXT PRIMARY KEY,
                values_json TEXT NOT NULL,
                updated_at TEXT NOT NULL
            );
            ",
        )?;
        let _ = connection.execute(
            "ALTER TABLE catalog_sources ADD COLUMN accepted_catalog_json TEXT",
            [],
        );
        Ok(Self {
            connection: Mutex::new(connection),
        })
    }

    fn lock(&self) -> AppResult<std::sync::MutexGuard<'_, Connection>> {
        self.connection
            .lock()
            .map_err(|_| AppError::Database(rusqlite::Error::InvalidQuery))
    }

    pub fn projects(&self) -> AppResult<Vec<ProjectSummary>> {
        let connection = self.lock()?;
        let mut statement = connection.prepare(
            "SELECT id, name, path, environment, template_id, revision, last_operation, state
             FROM projects ORDER BY COALESCE(last_operation, '') DESC",
        )?;
        let rows = statement.query_map([], |row| {
            Ok(ProjectSummary {
                id: row.get(0)?,
                name: row.get(1)?,
                path: row.get(2)?,
                environment: row.get(3)?,
                template_id: row.get(4)?,
                revision: row.get(5)?,
                last_operation: row.get(6)?,
                state: row.get(7)?,
            })
        })?;
        rows.collect::<Result<Vec<_>, _>>().map_err(AppError::from)
    }

    pub fn upsert_project(&self, project: &ProjectSummary) -> AppResult<()> {
        self.lock()?.execute(
            "INSERT INTO projects (id, name, path, environment, template_id, revision, last_operation, state)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
             ON CONFLICT(path) DO UPDATE SET
                name=excluded.name, environment=excluded.environment,
                template_id=excluded.template_id, revision=excluded.revision,
                last_operation=excluded.last_operation, state=excluded.state",
            params![
                project.id,
                project.name,
                project.path,
                project.environment,
                project.template_id,
                project.revision,
                project.last_operation,
                project.state
            ],
        )?;
        Ok(())
    }

    pub fn save_catalog_source(&self, source: &CatalogSource) -> AppResult<()> {
        let accepted_catalog = source
            .accepted_catalog
            .as_ref()
            .map(serde_json::to_string)
            .transpose()?;
        self.lock()?.execute(
            "INSERT INTO catalog_sources (id, label, url, trusted, pinned_revision, accepted_catalog_json)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)
             ON CONFLICT(url) DO UPDATE SET label=excluded.label, trusted=excluded.trusted,
             pinned_revision=excluded.pinned_revision, accepted_catalog_json=excluded.accepted_catalog_json",
            params![
                source.id,
                source.label,
                source.url,
                source.trusted,
                source.pinned_revision,
                accepted_catalog
            ],
        )?;
        Ok(())
    }

    pub fn accepted_catalogs(&self) -> AppResult<Vec<crate::models::Catalog>> {
        let connection = self.lock()?;
        let mut statement = connection.prepare(
            "SELECT accepted_catalog_json FROM catalog_sources
             WHERE trusted = 1 AND accepted_catalog_json IS NOT NULL",
        )?;
        let values = statement.query_map([], |row| row.get::<_, String>(0))?;
        values
            .map(|value| {
                let value = value?;
                serde_json::from_str(&value).map_err(|error| {
                    rusqlite::Error::FromSqlConversionFailure(
                        0,
                        rusqlite::types::Type::Text,
                        Box::new(error),
                    )
                })
            })
            .collect::<Result<Vec<_>, _>>()
            .map_err(AppError::from)
    }

    pub fn accepted_catalog_entry(
        &self,
        template_id: &str,
    ) -> AppResult<Option<crate::models::CatalogEntry>> {
        Ok(self
            .accepted_catalogs()?
            .into_iter()
            .flat_map(|catalog| catalog.entries)
            .find(|entry| entry.id == template_id))
    }

    pub fn save_draft(&self, project_key: &str, values: &serde_json::Value) -> AppResult<()> {
        self.lock()?.execute(
            "INSERT INTO wizard_drafts (project_key, values_json, updated_at)
             VALUES (?1, ?2, ?3)
             ON CONFLICT(project_key) DO UPDATE SET
             values_json=excluded.values_json, updated_at=excluded.updated_at",
            params![
                project_key,
                serde_json::to_string(values)?,
                chrono::Utc::now().to_rfc3339()
            ],
        )?;
        Ok(())
    }

    pub fn load_draft(&self, project_key: &str) -> AppResult<Option<serde_json::Value>> {
        let connection = self.lock()?;
        let mut statement =
            connection.prepare("SELECT values_json FROM wizard_drafts WHERE project_key = ?1")?;
        let mut rows = statement.query([project_key])?;
        let Some(row) = rows.next()? else {
            return Ok(None);
        };
        let value: String = row.get(0)?;
        Ok(Some(serde_json::from_str(&value)?))
    }

    pub fn record_operation(&self, event: &OperationEvent, project_path: &str) -> AppResult<()> {
        self.lock()?.execute(
            "INSERT INTO operation_history
             (id, project_path, kind, state, started_at, completed_at, summary)
             VALUES (?1, ?2, ?3, ?4, ?5, ?5, ?6)
             ON CONFLICT(id) DO UPDATE SET
             state=excluded.state, completed_at=excluded.completed_at, summary=excluded.summary",
            params![
                event.operation_id,
                project_path,
                event.kind,
                event.state,
                event.timestamp,
                event.message
            ],
        )?;
        Ok(())
    }
}
