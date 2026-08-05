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
mod tools;

use crate::{
    azd::{AzdClient, SystemAzdClient},
    command_runner::{spawn_operation, OperationManager},
    error::{AppError, AppResult},
    models::{
        AzureDiscovery, BootstrapData, Catalog, CatalogSource, ConnectionStatus, OperationEvent,
        PermissionCheckRequest, PermissionCheckResult, PrepareDeploymentRequest, ProjectSummary,
        TemplateInspection, ToolStatus,
    },
    persistence::Database,
};
use std::path::PathBuf;
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
async fn list_project_environments(project_path: String) -> AppResult<serde_json::Value> {
    let canonical = std::fs::canonicalize(project_path)?;
    SystemAzdClient.environment_list(&canonical).await
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
            preview_external_catalog,
            save_catalog_source,
            prepare_deployment,
            get_project_state,
            list_project_environments,
            record_operation,
            get_operation_history,
            save_wizard_draft,
            load_wizard_draft
        ])
        .run(tauri::generate_context!())
        .expect("error while running Azure Deployment Studio");
}
