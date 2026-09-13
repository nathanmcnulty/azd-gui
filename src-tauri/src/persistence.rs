use crate::{
    catalog,
    error::{AppError, AppResult},
    models::{
        AppSettings, CatalogSource, CatalogSourceSummary, OperationEvent, ProjectSummary,
        ProjectTemplateSnapshot,
    },
    redaction::redact,
};
use rusqlite::{params, Connection, OptionalExtension};
use std::{path::Path, sync::Mutex};

const SCHEMA_VERSION: i64 = 2;

fn column_exists(connection: &Connection, table: &str, column: &str) -> AppResult<bool> {
    let mut statement = connection.prepare(&format!("PRAGMA table_info({table})"))?;
    let columns = statement.query_map([], |row| row.get::<_, String>(1))?;
    for name in columns {
        if name? == column {
            return Ok(true);
        }
    }
    Ok(false)
}

fn migrate(connection: &mut Connection) -> AppResult<()> {
    let version =
        connection.pragma_query_value(None, "user_version", |row| row.get::<_, i64>(0))?;
    if version > SCHEMA_VERSION {
        return Err(AppError::Invalid(format!(
            "Database schema {version} is newer than this app supports ({SCHEMA_VERSION})."
        )));
    }
    if version == SCHEMA_VERSION {
        return Ok(());
    }

    let transaction = connection.transaction()?;
    transaction.execute_batch(
        "
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
        CREATE TABLE IF NOT EXISTS project_templates (
            project_path TEXT PRIMARY KEY REFERENCES projects(path) ON DELETE CASCADE,
            snapshot_json TEXT NOT NULL
        );
        CREATE TABLE IF NOT EXISTS operation_history (
            id TEXT PRIMARY KEY,
            project_path TEXT NOT NULL,
            kind TEXT NOT NULL,
            state TEXT NOT NULL,
            started_at TEXT NOT NULL,
            completed_at TEXT,
            summary TEXT,
            outcome_json TEXT
        );
        CREATE TABLE IF NOT EXISTS wizard_drafts (
            project_key TEXT PRIMARY KEY,
            values_json TEXT NOT NULL,
            updated_at TEXT NOT NULL
        );
        CREATE TABLE IF NOT EXISTS app_settings (
            id INTEGER PRIMARY KEY CHECK (id = 1),
            values_json TEXT NOT NULL,
            updated_at TEXT NOT NULL
        );
        ",
    )?;
    if !column_exists(&transaction, "catalog_sources", "accepted_catalog_json")? {
        transaction.execute(
            "ALTER TABLE catalog_sources ADD COLUMN accepted_catalog_json TEXT",
            [],
        )?;
    }
    if !column_exists(&transaction, "operation_history", "outcome_json")? {
        transaction.execute(
            "ALTER TABLE operation_history ADD COLUMN outcome_json TEXT",
            [],
        )?;
    }
    transaction.pragma_update(None, "user_version", SCHEMA_VERSION)?;
    transaction.commit()?;
    Ok(())
}

pub struct Database {
    connection: Mutex<Connection>,
}

impl Database {
    pub fn open(path: &Path) -> AppResult<Self> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let mut connection = Connection::open(path)?;
        connection.execute_batch("PRAGMA journal_mode = WAL; PRAGMA foreign_keys = ON;")?;
        migrate(&mut connection)?;
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

    pub fn project_by_path(&self, path: &str) -> AppResult<Option<ProjectSummary>> {
        let canonical = std::fs::canonicalize(path).ok();
        let matches = self
            .projects()?
            .into_iter()
            .filter(|project| {
                project.path == path
                    || canonical.as_ref().is_some_and(|canonical| {
                        std::fs::canonicalize(&project.path).ok().as_ref() == Some(canonical)
                    })
            })
            .collect::<Vec<_>>();
        if matches.len() > 1 {
            return Err(AppError::Invalid("Multiple registered projects resolve to this folder. Resolve duplicate registrations before continuing.".into()));
        }
        Ok(matches.into_iter().next())
    }

    fn stored_project_path(&self, path: &str) -> AppResult<String> {
        Ok(self
            .project_by_path(path)?
            .map(|project| project.path)
            .unwrap_or_else(|| path.into()))
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

    pub fn set_project_environment(&self, project_path: &str, environment: &str) -> AppResult<()> {
        let project_path = self.stored_project_path(project_path)?;
        self.lock()?.execute(
            "UPDATE projects SET environment = ?1 WHERE path = ?2",
            params![environment, project_path],
        )?;
        Ok(())
    }

    pub fn save_project_template(
        &self,
        project: &ProjectSummary,
        snapshot: &ProjectTemplateSnapshot,
    ) -> AppResult<()> {
        snapshot.validate_binding(project.template_id.as_deref(), project.revision.as_deref())?;
        self.lock()?.execute(
            "INSERT INTO project_templates (project_path, snapshot_json) VALUES (?1, ?2)
             ON CONFLICT(project_path) DO UPDATE SET snapshot_json=excluded.snapshot_json",
            params![project.path, serde_json::to_string(snapshot)?],
        )?;
        Ok(())
    }

    pub fn project_template(
        &self,
        project_path: &str,
    ) -> AppResult<Option<ProjectTemplateSnapshot>> {
        let project_path = self.stored_project_path(project_path)?;
        let row: Option<(Option<String>, Option<String>, String)> = self
            .lock()?
            .query_row(
                "SELECT p.template_id, p.revision, t.snapshot_json FROM project_templates t
             JOIN projects p ON p.path=t.project_path WHERE p.path=?1",
                [project_path],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .optional()?;
        let Some((template_id, revision, json)) = row else {
            return Ok(None);
        };
        if json.len() > 650_000 {
            return Err(AppError::Invalid(
                "Saved template contract exceeds its size limit.".into(),
            ));
        }
        let snapshot: ProjectTemplateSnapshot = serde_json::from_str(&json).map_err(|_| {
            AppError::Invalid(
                "Saved template contract is invalid. Inspect the source again.".into(),
            )
        })?;
        snapshot.validate_binding(template_id.as_deref(), revision.as_deref())?;
        Ok(Some(snapshot))
    }

    pub fn save_catalog_source(&self, source: &CatalogSource) -> AppResult<()> {
        let accepted_catalog = source
            .accepted_catalog
            .as_ref()
            .map(serde_json::to_string)
            .transpose()?;
        let mut connection = self.lock()?;
        let transaction = connection.transaction()?;
        transaction.execute(
            "DELETE FROM catalog_sources WHERE url = ?1 AND id != ?2",
            params![source.url, source.id],
        )?;
        transaction.execute(
            "INSERT INTO catalog_sources (id, label, url, trusted, pinned_revision, accepted_catalog_json)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)
             ON CONFLICT(id) DO UPDATE SET label=excluded.label, url=excluded.url,
             trusted=excluded.trusted,
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
        transaction.commit()?;
        Ok(())
    }

    pub fn catalog_sources(&self) -> AppResult<Vec<CatalogSourceSummary>> {
        let connection = self.lock()?;
        let mut statement = connection.prepare(
            "SELECT id, label, url FROM catalog_sources
             WHERE trusted = 1 AND accepted_catalog_json IS NOT NULL
             ORDER BY label COLLATE NOCASE, id",
        )?;
        let rows = statement.query_map([], |row| {
            Ok(CatalogSourceSummary {
                id: row.get(0)?,
                label: row.get(1)?,
                url: row.get(2)?,
            })
        })?;
        rows.collect::<Result<Vec<_>, _>>().map_err(AppError::from)
    }

    pub fn remove_catalog_source(&self, id: &str) -> AppResult<()> {
        let removed = self
            .lock()?
            .execute("DELETE FROM catalog_sources WHERE id = ?1", [id])?;
        if removed == 0 {
            return Err(AppError::Invalid(format!(
                "Catalog source is not registered: {id}"
            )));
        }
        Ok(())
    }

    pub fn accepted_catalogs(&self) -> AppResult<Vec<crate::models::Catalog>> {
        let connection = self.lock()?;
        let mut statement = connection.prepare(
            "SELECT id, url, accepted_catalog_json FROM catalog_sources
             WHERE trusted = 1 AND accepted_catalog_json IS NOT NULL",
        )?;
        let rows = statement
            .query_map([], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                ))
            })?
            .collect::<Result<Vec<_>, _>>()?;
        drop(statement);

        let mut catalogs = Vec::new();
        for (id, url, value) in rows {
            if catalog::normalize_catalog_url(&url).is_err() {
                connection.execute("DELETE FROM catalog_sources WHERE id = ?1", [&id])?;
                continue;
            }
            let catalog = serde_json::from_str(&value)
                .map_err(AppError::from)
                .and_then(|catalog| {
                    catalog::validate_external_snapshot(&catalog)?;
                    Ok(catalog)
                });
            match catalog {
                Ok(catalog) => catalogs.push(catalog),
                Err(_) => {
                    connection
                        .execute("UPDATE catalog_sources SET trusted = 0 WHERE id = ?1", [id])?;
                }
            }
        }
        Ok(catalogs)
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
        let value = connection
            .query_row(
                "SELECT values_json FROM wizard_drafts WHERE project_key = ?1",
                [project_key],
                |row| row.get::<_, String>(0),
            )
            .optional()?;
        let Some(value) = value else {
            return Ok(None);
        };
        match serde_json::from_str(&value) {
            Ok(value) => Ok(Some(value)),
            Err(_) => {
                connection.execute(
                    "DELETE FROM wizard_drafts WHERE project_key = ?1",
                    [project_key],
                )?;
                Ok(None)
            }
        }
    }

    pub fn app_settings(&self) -> AppResult<AppSettings> {
        let connection = self.lock()?;
        let value = connection
            .query_row(
                "SELECT values_json FROM app_settings WHERE id = 1",
                [],
                |row| row.get::<_, String>(0),
            )
            .optional()?;
        let Some(value) = value else {
            return Ok(AppSettings::default());
        };
        match serde_json::from_str(&value) {
            Ok(settings) => Ok(settings),
            Err(_) => {
                connection.execute("DELETE FROM app_settings WHERE id = 1", [])?;
                Ok(AppSettings::default())
            }
        }
    }

    pub fn save_app_settings(&self, settings: &AppSettings) -> AppResult<()> {
        self.lock()?.execute(
            "INSERT INTO app_settings (id, values_json, updated_at)
             VALUES (1, ?1, ?2)
             ON CONFLICT(id) DO UPDATE SET
             values_json=excluded.values_json, updated_at=excluded.updated_at",
            params![
                serde_json::to_string(settings)?,
                chrono::Utc::now().to_rfc3339()
            ],
        )?;
        Ok(())
    }

    pub fn clear_local_data(&self) -> AppResult<()> {
        let mut connection = self.lock()?;
        connection.pragma_update(None, "secure_delete", "ON")?;
        let transaction = connection.transaction()?;
        transaction.execute_batch(
            "
            DELETE FROM operation_history;
            DELETE FROM projects;
            DELETE FROM catalog_sources;
            DELETE FROM wizard_drafts;
            DELETE FROM app_settings;
            ",
        )?;
        transaction.commit()?;
        connection.execute_batch("PRAGMA wal_checkpoint(TRUNCATE); VACUUM;")?;
        Ok(())
    }

    pub fn record_operation(&self, event: &OperationEvent, project_path: &str) -> AppResult<()> {
        let project_path = self.stored_project_path(project_path)?;
        if ![
            "up",
            "provision",
            "preview",
            "deploy",
            "refresh",
            "show",
            "down",
        ]
        .contains(&event.kind.as_str())
            || !["succeeded", "failed", "cancelled"].contains(&event.state.as_str())
        {
            return Err(AppError::Invalid(
                "Only terminal azd lifecycle operations are persisted.".into(),
            ));
        }
        let message = redact(&event.message);
        let outcome_json = event
            .outcome
            .clone()
            .map(|mut outcome| {
                outcome.resource_state = redact(&outcome.resource_state);
                outcome.summary = redact(&outcome.summary);
                outcome.guidance = redact(&outcome.guidance);
                serde_json::to_string(&outcome)
            })
            .transpose()?;
        let mut connection = self.lock()?;
        let transaction = connection.transaction()?;
        transaction.execute(
            "INSERT INTO operation_history
             (id, project_path, kind, state, started_at, completed_at, summary, outcome_json)
             VALUES (?1, ?2, ?3, ?4, ?5, ?5, ?6, ?7)
             ON CONFLICT(id) DO UPDATE SET
             state=excluded.state, completed_at=excluded.completed_at,
             summary=excluded.summary, outcome_json=excluded.outcome_json",
            params![
                event.operation_id,
                project_path,
                event.kind,
                event.state,
                event.timestamp,
                message,
                outcome_json
            ],
        )?;
        transaction.execute(
            "UPDATE projects SET last_operation = ?1, state = ?2 WHERE path = ?3",
            params![
                event.timestamp,
                if event.state == "succeeded" {
                    "ready"
                } else {
                    "attention"
                },
                project_path
            ],
        )?;
        transaction.commit()?;
        Ok(())
    }

    pub fn operation_history(&self, project_path: &str) -> AppResult<Vec<OperationEvent>> {
        let project_path = self.stored_project_path(project_path)?;
        let connection = self.lock()?;
        let mut statement = connection.prepare(
            "SELECT id, kind, state, COALESCE(completed_at, started_at), summary, outcome_json
             FROM operation_history
             WHERE project_path = ?1
             ORDER BY COALESCE(completed_at, started_at) DESC
             LIMIT 100",
        )?;
        let rows = statement.query_map([project_path], |row| {
            let state: String = row.get(2)?;
            let outcome_json: Option<String> = row.get(5)?;
            let outcome = outcome_json
                .map(|value| {
                    serde_json::from_str(&value).map_err(|error| {
                        rusqlite::Error::FromSqlConversionFailure(
                            5,
                            rusqlite::types::Type::Text,
                            Box::new(error),
                        )
                    })
                })
                .transpose()?;
            Ok(OperationEvent {
                operation_id: row.get(0)?,
                kind: row.get(1)?,
                level: match state.as_str() {
                    "succeeded" => "success",
                    "failed" => "error",
                    "cancelled" => "warning",
                    _ => "info",
                }
                .into(),
                state,
                timestamp: row.get(3)?,
                stream: "system".into(),
                message: row.get(4)?,
                progress: None,
                outcome,
            })
        })?;
        rows.collect::<Result<Vec<_>, _>>().map_err(AppError::from)
    }
}

#[cfg(test)]
mod tests {
    use super::{column_exists, Database, SCHEMA_VERSION};
    use crate::models::{
        AppSettings, CatalogSource, OperationEvent, OperationOutcome, ProjectSummary,
        ThemePreference,
    };
    use rusqlite::Connection;
    use tempfile::tempdir;

    #[test]
    fn migrates_unversioned_databases_atomically() {
        let directory = tempdir().unwrap();
        let path = directory.path().join("legacy.sqlite");
        let connection = Connection::open(&path).unwrap();
        connection
            .execute_batch(
                "
                CREATE TABLE catalog_sources (
                    id TEXT PRIMARY KEY,
                    label TEXT NOT NULL,
                    url TEXT NOT NULL UNIQUE,
                    trusted INTEGER NOT NULL DEFAULT 0,
                    pinned_revision TEXT
                );
                CREATE TABLE operation_history (
                    id TEXT PRIMARY KEY,
                    project_path TEXT NOT NULL,
                    kind TEXT NOT NULL,
                    state TEXT NOT NULL,
                    started_at TEXT NOT NULL,
                    completed_at TEXT,
                    summary TEXT
                );
                ",
            )
            .unwrap();
        drop(connection);

        let database = Database::open(&path).unwrap();
        let connection = database.lock().unwrap();
        let version = connection
            .pragma_query_value(None, "user_version", |row| row.get::<_, i64>(0))
            .unwrap();
        assert_eq!(version, SCHEMA_VERSION);
        assert!(column_exists(&connection, "catalog_sources", "accepted_catalog_json").unwrap());
        assert!(column_exists(&connection, "operation_history", "outcome_json").unwrap());
    }

    #[test]
    fn application_settings_default_and_persist_across_reopen() {
        let directory = tempdir().unwrap();
        let path = directory.path().join("state.sqlite");
        {
            let database = Database::open(&path).unwrap();
            assert_eq!(database.app_settings().unwrap(), AppSettings::default());
            database
                .save_app_settings(&AppSettings {
                    theme: ThemePreference::Dark,
                    navigation_expanded: false,
                })
                .unwrap();
        }

        let database = Database::open(&path).unwrap();
        assert_eq!(
            database.app_settings().unwrap(),
            AppSettings {
                theme: ThemePreference::Dark,
                navigation_expanded: false,
            }
        );
    }

    #[test]
    fn migrates_version_one_and_preserves_local_project_contract_across_reopen() {
        let directory = tempdir().unwrap();
        let path = directory.path().join("state.sqlite");
        let mut entry = crate::catalog::entry_by_id("myworkid").unwrap();
        entry.id = "local-sample".into();
        entry.commit = "local".into();
        entry.source = Some("local".into());
        let project = ProjectSummary {
            id: "sample-dev".into(),
            name: "sample".into(),
            path: format!("{}/./", directory.path().display()),
            environment: Some("dev".into()),
            template_id: Some(entry.id.clone()),
            revision: None,
            last_operation: None,
            state: "ready".into(),
        };
        {
            let database = Database::open(&path).unwrap();
            database.upsert_project(&project).unwrap();
            database
                .lock()
                .unwrap()
                .execute_batch("DROP TABLE project_templates; PRAGMA user_version=1;")
                .unwrap();
        }
        {
            let database = Database::open(&path).unwrap();
            assert!(database.project_template(&project.path).unwrap().is_none());
            let snapshot = crate::models::ProjectTemplateSnapshot::new(entry).unwrap();
            database.save_project_template(&project, &snapshot).unwrap();
            database
                .set_project_environment(
                    &std::fs::canonicalize(directory.path())
                        .unwrap()
                        .display()
                        .to_string(),
                    "prod",
                )
                .unwrap();
        }
        let database = Database::open(&path).unwrap();
        let canonical = std::fs::canonicalize(directory.path())
            .unwrap()
            .display()
            .to_string();
        let saved = database.project_template(&canonical).unwrap().unwrap();
        assert_eq!(
            database.project_by_path(&canonical).unwrap().unwrap().path,
            project.path
        );
        assert_eq!(saved.entry.id, "local-sample");
        assert!(!saved
            .entry
            .manifest
            .unwrap()
            .configuration
            .groups
            .is_empty());
        assert_eq!(
            database.projects().unwrap()[0].environment.as_deref(),
            Some("prod")
        );
        database.clear_local_data().unwrap();
        assert!(database.project_template(&project.path).unwrap().is_none());
    }

    #[test]
    fn rejects_a_snapshot_copied_to_a_different_revision_without_breaking_project_listing() {
        let directory = tempdir().unwrap();
        let database = Database::open(&directory.path().join("state.sqlite")).unwrap();
        let mut entry = crate::catalog::entry_by_id("myworkid").unwrap();
        entry.id = "github-custom".into();
        let project = ProjectSummary {
            id: "sample-dev".into(),
            name: "sample".into(),
            path: directory.path().display().to_string(),
            environment: Some("dev".into()),
            template_id: Some(entry.id.clone()),
            revision: Some(entry.commit.clone()),
            last_operation: None,
            state: "ready".into(),
        };
        database.upsert_project(&project).unwrap();
        let snapshot = crate::models::ProjectTemplateSnapshot::new(entry).unwrap();
        database.save_project_template(&project, &snapshot).unwrap();
        database
            .lock()
            .unwrap()
            .execute(
                "UPDATE projects SET revision='another-revision' WHERE path=?1",
                [&project.path],
            )
            .unwrap();
        assert!(database.project_template(&project.path).is_err());
        assert_eq!(database.projects().unwrap().len(), 1);
    }

    #[test]
    fn catalog_sources_refresh_identity_and_can_be_removed() {
        let directory = tempdir().unwrap();
        let database = Database::open(&directory.path().join("state.sqlite")).unwrap();
        let url = "https://example.com/azd-gui.catalog.json";
        let first = CatalogSource {
            id: "first".into(),
            label: "First catalog".into(),
            url: url.into(),
            trusted: true,
            pinned_revision: None,
            accepted_catalog: None,
        };
        database.save_catalog_source(&first).unwrap();

        let second = CatalogSource {
            id: "second".into(),
            label: "Second catalog".into(),
            url: url.into(),
            trusted: true,
            pinned_revision: None,
            accepted_catalog: Some(crate::catalog::bundled_catalog().unwrap()),
        };
        database.save_catalog_source(&second).unwrap();

        let sources = database.catalog_sources().unwrap();
        assert_eq!(sources.len(), 1);
        assert_eq!(sources[0].id, second.id);
        assert_eq!(sources[0].label, second.label);
        database.remove_catalog_source(&second.id).unwrap();
        assert!(database.catalog_sources().unwrap().is_empty());
        assert!(database.remove_catalog_source(&second.id).is_err());
    }

    #[test]
    fn terminal_operation_updates_project_attention_and_persists_outcome() {
        let directory = tempdir().unwrap();
        let database = Database::open(&directory.path().join("state.sqlite")).unwrap();
        let project = ProjectSummary {
            id: "sample-dev".into(),
            name: "sample".into(),
            path: directory.path().display().to_string(),
            environment: Some("dev".into()),
            template_id: Some("sample".into()),
            revision: None,
            last_operation: None,
            state: "ready".into(),
        };
        database.upsert_project(&project).unwrap();
        database
            .set_project_environment(&project.path, "staging")
            .unwrap();
        assert_eq!(
            database.projects().unwrap()[0].environment.as_deref(),
            Some("staging")
        );
        let event = OperationEvent {
            operation_id: "operation-1".into(),
            kind: "up".into(),
            state: "failed".into(),
            timestamp: "2026-08-05T12:00:00Z".into(),
            stream: "system".into(),
            level: "error".into(),
            message: "Deployment did not complete.".into(),
            progress: Some(55),
            outcome: Some(OperationOutcome {
                resource_state: "possiblyModified".into(),
                summary: "Deployment did not complete.".into(),
                guidance: "Review output before retrying.".into(),
                retryable: true,
            }),
        };

        database.record_operation(&event, &project.path).unwrap();

        let projects = database.projects().unwrap();
        assert_eq!(projects[0].state, "attention");
        assert_eq!(
            projects[0].last_operation.as_deref(),
            Some(event.timestamp.as_str())
        );
        let connection = database.lock().unwrap();
        let outcome: String = connection
            .query_row(
                "SELECT outcome_json FROM operation_history WHERE id = ?1",
                [&event.operation_id],
                |row| row.get(0),
            )
            .unwrap();
        assert!(outcome.contains("possiblyModified"));
        drop(connection);

        let history = database.operation_history(&project.path).unwrap();
        assert_eq!(history.len(), 1);
        assert_eq!(history[0].operation_id, event.operation_id);
        assert_eq!(history[0].level, "error");
        let outcome = history[0].outcome.as_ref().unwrap();
        assert_eq!(outcome.resource_state, "possiblyModified");
        assert!(outcome.retryable);
    }

    #[test]
    fn operation_history_redacts_messages_at_the_storage_boundary() {
        let directory = tempdir().unwrap();
        let database = Database::open(&directory.path().join("state.sqlite")).unwrap();
        let event = OperationEvent {
            operation_id: "operation-secret".into(),
            kind: "preview".into(),
            state: "failed".into(),
            timestamp: "2026-08-05T12:00:00Z".into(),
            stream: "system".into(),
            level: "error".into(),
            message: "token=super-secret-value".into(),
            progress: None,
            outcome: Some(OperationOutcome {
                resource_state: "unchanged".into(),
                summary: "password=super-secret-value".into(),
                guidance: "Retry without Authorization: Bearer abcdefghijklmnopqrstuvwxyz".into(),
                retryable: true,
            }),
        };

        database
            .record_operation(&event, directory.path().to_str().unwrap())
            .unwrap();
        let history = database
            .operation_history(directory.path().to_str().unwrap())
            .unwrap();
        let serialized = serde_json::to_string(&history).unwrap();
        assert!(!serialized.contains("super-secret-value"));
        assert!(!serialized.contains("abcdefghijklmnopqrstuvwxyz"));
        assert!(serialized.contains("[REDACTED]"));
    }

    #[test]
    fn malformed_json_is_quarantined_instead_of_blocking_startup() {
        let directory = tempdir().unwrap();
        let database = Database::open(&directory.path().join("state.sqlite")).unwrap();
        {
            let connection = database.lock().unwrap();
            connection
                .execute(
                    "INSERT INTO catalog_sources
                     (id, label, url, trusted, accepted_catalog_json)
                     VALUES ('broken', 'Broken', 'https://example.com/catalog.json', 1, '{')",
                    [],
                )
                .unwrap();
            connection
                .execute(
                    "INSERT INTO app_settings (id, values_json, updated_at)
                     VALUES (1, '{', '2026-08-05T12:00:00Z')",
                    [],
                )
                .unwrap();
            connection
                .execute(
                    "INSERT INTO wizard_drafts (project_key, values_json, updated_at)
                     VALUES ('broken', '{', '2026-08-05T12:00:00Z')",
                    [],
                )
                .unwrap();
        }

        assert!(database.accepted_catalogs().unwrap().is_empty());
        assert_eq!(database.app_settings().unwrap(), AppSettings::default());
        assert!(database.load_draft("broken").unwrap().is_none());
        let connection = database.lock().unwrap();
        let trusted: bool = connection
            .query_row(
                "SELECT trusted FROM catalog_sources WHERE id = 'broken'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert!(!trusted);
    }

    #[test]
    fn legacy_catalog_urls_with_credentials_are_deleted() {
        let directory = tempdir().unwrap();
        let database = Database::open(&directory.path().join("state.sqlite")).unwrap();
        database
            .lock()
            .unwrap()
            .execute(
                "INSERT INTO catalog_sources
                 (id, label, url, trusted, accepted_catalog_json)
                 VALUES ('private-url', 'Private URL', 'https://example.com/catalog.json?sig=secret', 1, '{}')",
                [],
            )
            .unwrap();

        assert!(database.accepted_catalogs().unwrap().is_empty());
        let count: i64 = database
            .lock()
            .unwrap()
            .query_row(
                "SELECT COUNT(*) FROM catalog_sources WHERE id = 'private-url'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(count, 0);
    }

    #[test]
    fn clearing_local_data_preserves_user_project_folders() {
        let directory = tempdir().unwrap();
        let project_path = directory.path().join("project");
        std::fs::create_dir(&project_path).unwrap();
        std::fs::write(project_path.join("azure.yaml"), "name: sample").unwrap();
        let database = Database::open(&directory.path().join("state.sqlite")).unwrap();
        database
            .upsert_project(&ProjectSummary {
                id: "sample".into(),
                name: "Sample".into(),
                path: project_path.display().to_string(),
                environment: Some("dev".into()),
                template_id: Some("sample".into()),
                revision: None,
                last_operation: None,
                state: "ready".into(),
            })
            .unwrap();
        database
            .save_draft("sample", &serde_json::json!({"location": "westus2"}))
            .unwrap();
        database
            .save_app_settings(&AppSettings {
                theme: ThemePreference::Dark,
                navigation_expanded: false,
            })
            .unwrap();

        database.clear_local_data().unwrap();

        assert!(database.projects().unwrap().is_empty());
        assert!(database.load_draft("sample").unwrap().is_none());
        assert_eq!(database.app_settings().unwrap(), AppSettings::default());
        assert!(project_path.join("azure.yaml").is_file());
    }
}
