mod azd;
mod azure;
mod catalog;
mod command_runner;
mod connections;
mod deployment;
mod error;
mod models;
mod permissions;
mod persistence;
mod redaction;
mod terminal;
mod tools;

use crate::{
    azd::{AzdClient, SystemAzdClient},
    command_runner::{spawn_operation, OperationManager},
    error::{AppError, AppResult},
    models::{
        AzureDiscovery, BootstrapData, Catalog, CatalogSource, ConnectionStatus, OperationEvent,
        PermissionCheckRequest, PermissionCheckResult, PrepareDeploymentRequest,
        ProjectEnvironment, ProjectEnvironmentValue, ProjectEnvironmentValueInput, ProjectSummary,
        TemplateInspection, ToolStatus,
    },
    persistence::Database,
};
use std::{
    collections::HashSet,
    io::BufRead,
    path::{Path, PathBuf},
};
use tauri::{AppHandle, Manager, State};

pub struct AppState {
    database: Database,
    operations: OperationManager,
    template_cache: PathBuf,
}

fn platform_name() -> String {
    match std::env::consts::OS {
        "macos" => "macos",
        "linux" => "linux",
        _ => "windows",
    }
    .into()
}

#[tauri::command]
async fn bootstrap(state: State<'_, AppState>) -> AppResult<BootstrapData> {
    let (tools, connections) = tokio::join!(tools::check_all(), connections::check_all());
    let mut catalog = catalog::bundled_catalog()?;
    for mut external in state.database.accepted_catalogs()? {
        for entry in &mut external.entries {
            entry.source = Some("external".into());
            entry.trusted = true;
        }
        catalog.entries.extend(external.entries);
    }
    Ok(BootstrapData {
        catalog,
        tools,
        connections,
        projects: state.database.projects()?,
        platform: platform_name(),
        app_version: env!("CARGO_PKG_VERSION").into(),
        is_native: true,
    })
}

#[tauri::command]
fn inspect_local_template(path: String) -> AppResult<TemplateInspection> {
    catalog::inspect_local(&path)
}

#[tauri::command]
async fn inspect_github_template(url: String) -> AppResult<TemplateInspection> {
    catalog::inspect_github(&url).await
}

#[tauri::command]
async fn check_tools() -> AppResult<Vec<ToolStatus>> {
    Ok(tools::check_all().await)
}

#[tauri::command]
fn install_tool(app: AppHandle, state: State<'_, AppState>, id: String) -> AppResult<String> {
    let spec = tools::install_spec(&id).ok_or_else(|| {
        AppError::Invalid(format!(
            "{id} has no vetted non-elevated installation strategy."
        ))
    })?;
    spawn_operation(app, state.operations.clone(), format!("install:{id}"), spec)
}

#[tauri::command]
async fn check_connections() -> AppResult<Vec<ConnectionStatus>> {
    Ok(connections::check_all().await)
}

#[tauri::command]
async fn discover_azure_scope(
    subscription_id: Option<String>,
    refresh: bool,
) -> AppResult<AzureDiscovery> {
    azure::discover(subscription_id.as_deref(), refresh).await
}

#[tauri::command]
async fn run_permission_checks(
    request: PermissionCheckRequest,
) -> AppResult<Vec<PermissionCheckResult>> {
    permissions::run(request).await
}

#[tauri::command]
fn start_login(
    app: AppHandle,
    state: State<'_, AppState>,
    provider_id: String,
    scopes: Vec<String>,
    device_code: bool,
) -> AppResult<String> {
    let spec = connections::login_spec(&provider_id, &scopes, device_code)?;
    spawn_operation(
        app,
        state.operations.clone(),
        format!("login:{provider_id}"),
        spec,
    )
}

#[tauri::command]
fn start_azd_operation(
    app: AppHandle,
    state: State<'_, AppState>,
    action: String,
    project_path: String,
    environment: Option<String>,
) -> AppResult<String> {
    let canonical = std::fs::canonicalize(&project_path)?;
    if !canonical.join("azure.yaml").is_file() {
        return Err(AppError::Invalid(format!(
            "{} does not contain azure.yaml.",
            canonical.display()
        )));
    }
    let client = SystemAzdClient;
    let spec = client.lifecycle_command(&action, &canonical, environment.as_deref())?;
    spawn_operation(app, state.operations.clone(), action, spec)
}

#[tauri::command]
async fn cancel_operation(state: State<'_, AppState>, operation_id: String) -> AppResult<()> {
    if state.operations.cancel(&operation_id).await {
        Ok(())
    } else {
        Err(AppError::Invalid(format!(
            "Operation is not running: {operation_id}"
        )))
    }
}

#[tauri::command]
fn open_external_terminal(
    project_path: String,
    environment: Option<String>,
) -> AppResult<terminal::TerminalLaunch> {
    terminal::open(&project_path, environment.as_deref())
}

#[tauri::command]
async fn preview_external_catalog(url: String) -> AppResult<Catalog> {
    catalog::preview_external_catalog(&url).await
}

#[tauri::command]
fn save_catalog_source(state: State<'_, AppState>, source: CatalogSource) -> AppResult<()> {
    if !source.url.starts_with("https://") {
        return Err(AppError::Url(
            "Only HTTPS catalog sources are allowed.".into(),
        ));
    }
    if !source.trusted {
        return Err(AppError::Invalid(
            "A reviewed catalog snapshot is required before trusting a source.".into(),
        ));
    }
    let accepted_catalog = source.accepted_catalog.as_ref().ok_or_else(|| {
        AppError::Invalid(
            "A reviewed catalog snapshot is required before trusting a source.".into(),
        )
    })?;
    catalog::validate_external_snapshot(accepted_catalog)?;
    state.database.save_catalog_source(&source)
}

#[tauri::command]
async fn prepare_deployment(
    state: State<'_, AppState>,
    request: PrepareDeploymentRequest,
) -> AppResult<ProjectSummary> {
    deployment::prepare(request, &state.template_cache, &state.database).await
}

#[tauri::command]
async fn get_project_state(
    project_path: String,
    environment: Option<String>,
) -> AppResult<serde_json::Value> {
    let canonical = std::fs::canonicalize(project_path)?;
    SystemAzdClient
        .show(&canonical, environment.as_deref())
        .await
}

#[tauri::command]
async fn list_project_environments(project_path: String) -> AppResult<Vec<ProjectEnvironment>> {
    let canonical = std::fs::canonicalize(project_path)?;
    SystemAzdClient.environment_list(&canonical).await
}

#[tauri::command]
async fn create_project_environment(
    state: State<'_, AppState>,
    project_path: String,
    environment: String,
    subscription: Option<String>,
    location: Option<String>,
) -> AppResult<Vec<ProjectEnvironment>> {
    let canonical = std::fs::canonicalize(project_path)?;
    if !canonical.join("azure.yaml").is_file() {
        return Err(AppError::Invalid(format!(
            "{} does not contain azure.yaml.",
            canonical.display()
        )));
    }
    let client = SystemAzdClient;
    client
        .environment_new(
            &canonical,
            &environment,
            subscription.as_deref(),
            location.as_deref(),
        )
        .await?;
    let environments = client.environment_list(&canonical).await?;
    let selected = environments
        .iter()
        .find(|item| item.is_default)
        .ok_or_else(|| AppError::Invalid("azd did not select the new environment.".into()))?;
    state
        .database
        .set_project_environment(&canonical.display().to_string(), &selected.name)?;
    Ok(environments)
}

#[tauri::command]
async fn select_project_environment(
    state: State<'_, AppState>,
    project_path: String,
    environment: String,
) -> AppResult<Vec<ProjectEnvironment>> {
    let canonical = std::fs::canonicalize(project_path)?;
    if !canonical.join("azure.yaml").is_file() {
        return Err(AppError::Invalid(format!(
            "{} does not contain azure.yaml.",
            canonical.display()
        )));
    }
    let client = SystemAzdClient;
    client.environment_select(&canonical, &environment).await?;
    let environments = client.environment_list(&canonical).await?;
    let selected = environments
        .iter()
        .find(|item| item.is_default)
        .ok_or_else(|| AppError::Invalid("azd did not select an environment.".into()))?;
    state
        .database
        .set_project_environment(&canonical.display().to_string(), &selected.name)?;
    Ok(environments)
}

fn dotenv_keys(path: &Path) -> AppResult<HashSet<String>> {
    if !path.is_file() {
        return Ok(HashSet::new());
    }
    let file = std::fs::File::open(path)?;
    let mut keys = HashSet::new();
    for line in std::io::BufReader::new(file).lines() {
        let line = line?;
        let candidate = line
            .trim()
            .strip_prefix("export ")
            .unwrap_or(line.trim())
            .split_once('=')
            .map(|(key, _)| key.trim());
        if let Some(key) = candidate.filter(|key| !key.is_empty()) {
            keys.insert(key.to_string());
        }
    }
    Ok(keys)
}

fn project_configuration_fields(
    state: &AppState,
    template_id: Option<&str>,
) -> AppResult<Vec<crate::models::ConfigurationField>> {
    let Some(template_id) = template_id.filter(|value| !value.is_empty()) else {
        return Ok(Vec::new());
    };
    let entry = match catalog::entry_by_id(template_id) {
        Ok(entry) => Some(entry),
        Err(_) => state.database.accepted_catalog_entry(template_id)?,
    };
    Ok(entry
        .and_then(|entry| entry.manifest)
        .map(|manifest| {
            manifest
                .configuration
                .groups
                .into_iter()
                .flat_map(|group| group.fields)
                .collect()
        })
        .unwrap_or_default())
}

#[tauri::command]
async fn get_project_environment_values(
    state: State<'_, AppState>,
    project_path: String,
    environment: String,
) -> AppResult<Vec<ProjectEnvironmentValue>> {
    deployment::validate_environment_name(&environment)?;
    let canonical = std::fs::canonicalize(project_path)?;
    if !canonical.join("azure.yaml").is_file() {
        return Err(AppError::Invalid(format!(
            "{} does not contain azure.yaml.",
            canonical.display()
        )));
    }
    let client = SystemAzdClient;
    let description = client
        .environment_list(&canonical)
        .await?
        .into_iter()
        .find(|item| item.name == environment)
        .ok_or_else(|| AppError::Invalid(format!("Environment does not exist: {environment}")))?;
    let keys = if description.dot_env_path.is_empty() {
        HashSet::new()
    } else {
        let dotenv = PathBuf::from(&description.dot_env_path);
        let azure_root = std::fs::canonicalize(canonical.join(".azure"))?;
        let dotenv = std::fs::canonicalize(dotenv)?;
        if !dotenv.starts_with(&azure_root) {
            return Err(AppError::Invalid(
                "azd returned an environment path outside .azure.".into(),
            ));
        }
        dotenv_keys(&dotenv)?
    };
    let canonical_path = canonical.display().to_string();
    let template_id = state
        .database
        .projects()?
        .into_iter()
        .find(|project| project.path == canonical_path)
        .and_then(|project| project.template_id);
    let fields = project_configuration_fields(&state, template_id.as_deref())?;
    let declared_names = fields
        .iter()
        .map(|field| field.environment_variable.clone())
        .collect::<HashSet<_>>();
    let mut values = Vec::new();
    for field in fields {
        let is_set = keys.contains(&field.environment_variable);
        let sensitive = field.sensitive || field.field_type == "secret";
        let value = if is_set && !sensitive {
            client
                .environment_get_value(&canonical, &environment, &field.environment_variable)
                .await
                .ok()
        } else {
            None
        };
        values.push(ProjectEnvironmentValue {
            name: field.environment_variable,
            value,
            is_set,
            sensitive,
            declared: true,
            required: field.required,
        });
    }
    let mut raw = keys
        .into_iter()
        .filter(|name| !declared_names.contains(name))
        .collect::<Vec<_>>();
    raw.sort_unstable();
    values.extend(raw.into_iter().map(|name| ProjectEnvironmentValue {
        name,
        value: None,
        is_set: true,
        sensitive: true,
        declared: false,
        required: false,
    }));
    Ok(values)
}

#[tauri::command]
async fn set_project_environment_values(
    project_path: String,
    environment: String,
    values: Vec<ProjectEnvironmentValueInput>,
) -> AppResult<()> {
    let canonical = std::fs::canonicalize(project_path)?;
    if !canonical.join("azure.yaml").is_file() {
        return Err(AppError::Invalid(format!(
            "{} does not contain azure.yaml.",
            canonical.display()
        )));
    }
    let values = values
        .into_iter()
        .map(|value| (value.name, value.value))
        .collect::<Vec<_>>();
    SystemAzdClient
        .environment_set_values(&canonical, &environment, &values)
        .await
}

#[tauri::command]
fn record_operation(
    state: State<'_, AppState>,
    event: OperationEvent,
    project_path: String,
) -> AppResult<()> {
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
    {
        return Err(AppError::Invalid(
            "Only azd lifecycle operations are persisted.".into(),
        ));
    }
    let canonical = std::fs::canonicalize(project_path)?;
    state
        .database
        .record_operation(&event, &canonical.display().to_string())
}

#[tauri::command]
fn get_operation_history(
    state: State<'_, AppState>,
    project_path: String,
) -> AppResult<Vec<OperationEvent>> {
    let canonical = std::fs::canonicalize(project_path)?;
    state
        .database
        .operation_history(&canonical.display().to_string())
}

fn sanitized_draft(mut values: serde_json::Value) -> AppResult<serde_json::Value> {
    let object = values
        .as_object_mut()
        .ok_or_else(|| AppError::Invalid("Wizard draft must be an object.".into()))?;
    object.retain(|key, _| {
        let key = key.to_ascii_lowercase();
        !["secret", "password", "token", "webhook", "credential"]
            .iter()
            .any(|marker| key.contains(marker))
    });
    Ok(values)
}

#[tauri::command]
fn save_wizard_draft(
    state: State<'_, AppState>,
    template_id: String,
    values: serde_json::Value,
) -> AppResult<()> {
    if template_id.len() > 128 {
        return Err(AppError::Invalid("Template ID is too long.".into()));
    }
    state
        .database
        .save_draft(&template_id, &sanitized_draft(values)?)
}

#[tauri::command]
fn load_wizard_draft(
    state: State<'_, AppState>,
    template_id: String,
) -> AppResult<Option<serde_json::Value>> {
    state.database.load_draft(&template_id)
}

pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .setup(|app| {
            let app_data = app.path().app_data_dir()?;
            std::fs::create_dir_all(&app_data)?;
            let database = Database::open(&app_data.join("azd-gui.sqlite"))
                .map_err(|error| std::io::Error::other(error.to_string()))?;
            let template_cache = app_data.join("cache").join("templates");
            std::fs::create_dir_all(&template_cache)?;
            app.manage(AppState {
                database,
                operations: OperationManager::default(),
                template_cache,
            });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            bootstrap,
            inspect_local_template,
            inspect_github_template,
            check_tools,
            install_tool,
            check_connections,
            discover_azure_scope,
            run_permission_checks,
            start_login,
            start_azd_operation,
            cancel_operation,
            open_external_terminal,
            preview_external_catalog,
            save_catalog_source,
            prepare_deployment,
            get_project_state,
            list_project_environments,
            create_project_environment,
            select_project_environment,
            get_project_environment_values,
            set_project_environment_values,
            record_operation,
            get_operation_history,
            save_wizard_draft,
            load_wizard_draft
        ])
        .run(tauri::generate_context!())
        .expect("error while running Azure Deployment Studio");
}

#[cfg(test)]
mod environment_value_tests {
    use super::dotenv_keys;
    use std::io::Write;

    #[test]
    fn reads_only_dotenv_key_names() {
        let mut file = tempfile::NamedTempFile::new().unwrap();
        writeln!(file, "# comment\nexport FIRST=value\nSECOND=secret-value\n").unwrap();
        let keys = dotenv_keys(file.path()).unwrap();
        assert_eq!(keys.len(), 2);
        assert!(keys.contains("FIRST"));
        assert!(keys.contains("SECOND"));
    }
}
