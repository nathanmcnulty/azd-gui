mod azd;
mod azure;
mod catalog;
mod command_runner;
mod connections;
mod deployment;
mod error;
mod execution_approval;
mod local_source;
mod manifest;
mod models;
mod permissions;
mod persistence;
mod redaction;
mod terminal;
mod tools;

use crate::{
    command_runner::{spawn_operation, OperationManager, OperationPersistence},
    error::{AppError, AppResult},
    models::{
        AppSettings, AzureDiscovery, BootstrapData, Catalog, CatalogSource, ConnectionStatus,
        ConnectionTarget, ExecutionApprovalGrant, ExecutionApprovalRequest, ExternalCatalogPreview,
        OperationEvent, PermissionCheckRequest, PermissionCheckResult, PrepareDeploymentRequest,
        ProjectEnvironment, ProjectEnvironmentValue, ProjectEnvironmentValueInput, ProjectSummary,
        ProjectTemplateSnapshot, TemplateInspection, TemplateManifest, ToolInstallApprovalGrant,
        ToolInstallPlan, ToolStatus,
    },
    persistence::Database,
};
use std::{
    collections::{HashMap, HashSet},
    io::BufRead,
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
};
use tauri::{AppHandle, Manager, State};

const MAX_INSPECTED_MANIFESTS: usize = 128;

pub struct AppState {
    database: Arc<Database>,
    operations: OperationManager,
    template_cache: PathBuf,
    inspected_manifests: Mutex<HashMap<String, TemplateManifest>>,
    catalog_reviews: CatalogReviews,
    execution_approvals: execution_approval::ExecutionApprovals,
    tool_install_approvals: tools::ToolInstallApprovals,
}

#[derive(Clone)]
struct PendingCatalogReview {
    source_url: String,
    catalog: Catalog,
}

#[derive(Default)]
struct CatalogReviews {
    pending: Mutex<HashMap<String, PendingCatalogReview>>,
}

impl CatalogReviews {
    fn register(&self, source_url: String, catalog: Catalog) -> AppResult<ExternalCatalogPreview> {
        let review_token = uuid::Uuid::new_v4().to_string();
        let mut pending = self
            .pending
            .lock()
            .map_err(|_| AppError::Invalid("Catalog review cache is unavailable.".into()))?;
        pending.clear();
        pending.insert(
            review_token.clone(),
            PendingCatalogReview {
                source_url,
                catalog: catalog.clone(),
            },
        );
        Ok(ExternalCatalogPreview {
            catalog,
            review_token,
        })
    }

    fn accept(&self, review_token: &str, source_url: &str) -> AppResult<Catalog> {
        let review = self
            .pending
            .lock()
            .map_err(|_| AppError::Invalid("Catalog review cache is unavailable.".into()))?
            .remove(review_token)
            .ok_or_else(|| {
                AppError::Invalid("Catalog review expired. Preview the source again.".into())
            })?;
        if review.source_url != source_url {
            return Err(AppError::Invalid(
                "Catalog source changed after review. Preview the source again.".into(),
            ));
        }
        catalog::validate_external_snapshot(&review.catalog)?;
        Ok(review.catalog)
    }
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
    let (tools, connections) = tokio::join!(tools::check_all(false), connections::check_all(None));
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
        catalog_sources: state.database.catalog_sources()?,
        tools,
        connections,
        projects: state.database.projects()?,
        settings: state.database.app_settings()?,
        platform: platform_name(),
        app_version: env!("CARGO_PKG_VERSION").into(),
        is_native: true,
    })
}

#[tauri::command]
fn list_projects(state: State<'_, AppState>) -> AppResult<Vec<ProjectSummary>> {
    state.database.projects()
}

#[tauri::command]
fn save_app_settings(state: State<'_, AppState>, settings: AppSettings) -> AppResult<()> {
    state.database.save_app_settings(&settings)
}

#[tauri::command]
async fn clear_local_app_data(app: AppHandle, state: State<'_, AppState>) -> AppResult<bool> {
    use tauri_plugin_dialog::{DialogExt, MessageDialogButtons, MessageDialogKind};

    if !state.operations.is_idle().await {
        return Err(AppError::Invalid(
            "Wait for the active command to finish or cancel it before clearing local data.".into(),
        ));
    }
    let approved = app
        .dialog()
        .message(
            "This permanently clears Azure Deployment Studio project references, operation history, wizard drafts, catalog trust decisions, and settings.\n\nProject folders and credential caches owned by azd, Azure CLI, and PowerShell are not deleted.",
        )
        .title("Clear local application data?")
        .kind(MessageDialogKind::Warning)
        .buttons(MessageDialogButtons::OkCancelCustom(
            "Clear local data".into(),
            "Cancel".into(),
        ))
        .blocking_show();
    if !approved {
        return Ok(false);
    }
    if !state.operations.is_idle().await {
        return Err(AppError::Invalid(
            "A command started while confirmation was open. Cancel it before clearing local data."
                .into(),
        ));
    }

    if state.template_cache.exists() {
        std::fs::remove_dir_all(&state.template_cache)?;
    }
    std::fs::create_dir_all(&state.template_cache)?;
    state.database.clear_local_data()?;
    state
        .inspected_manifests
        .lock()
        .map_err(|_| AppError::Invalid("Template inspection cache is unavailable.".into()))?
        .clear();
    state
        .catalog_reviews
        .pending
        .lock()
        .map_err(|_| AppError::Invalid("Catalog review cache is unavailable.".into()))?
        .clear();
    state.execution_approvals.clear()?;
    state.tool_install_approvals.clear()?;
    Ok(true)
}

#[tauri::command]
fn inspect_local_template(
    state: State<'_, AppState>,
    path: String,
) -> AppResult<TemplateInspection> {
    let inspection = catalog::inspect_local(&path)?;
    remember_inspection_manifest(state.inner(), &inspection)?;
    Ok(inspection)
}

#[tauri::command]
async fn inspect_github_template(
    state: State<'_, AppState>,
    url: String,
) -> AppResult<TemplateInspection> {
    let inspection = catalog::inspect_github(&url).await?;
    remember_inspection_manifest(state.inner(), &inspection)?;
    Ok(inspection)
}

fn remember_inspection_manifest(
    state: &AppState,
    inspection: &TemplateInspection,
) -> AppResult<()> {
    let Some(manifest) = inspection.entry.manifest.clone() else {
        return Ok(());
    };
    let mut manifests = state
        .inspected_manifests
        .lock()
        .map_err(|_| AppError::Invalid("Template inspection cache is unavailable.".into()))?;
    if manifests.len() >= MAX_INSPECTED_MANIFESTS && !manifests.contains_key(&inspection.entry.id) {
        return Err(AppError::Invalid(
            "Too many templates have been inspected in this session. Restart the app before inspecting another source."
                .into(),
        ));
    }
    manifests.insert(inspection.entry.id.clone(), manifest);
    Ok(())
}

#[tauri::command]
async fn check_tools() -> AppResult<Vec<ToolStatus>> {
    Ok(tools::check_all(true).await)
}

#[tauri::command]
async fn check_template_prerequisites(manifest: TemplateManifest) -> AppResult<Vec<ToolStatus>> {
    Ok(tools::evaluate_manifest_prerequisites(&manifest).await)
}

#[tauri::command]
async fn preview_tool_install(id: String) -> AppResult<ToolInstallPlan> {
    tools::resolve_install_plan(&id).await
}

#[tauri::command]
async fn request_tool_install_approval(
    app: AppHandle,
    state: State<'_, AppState>,
    id: String,
    exact_version: String,
) -> AppResult<ToolInstallApprovalGrant> {
    use tauri_plugin_dialog::{DialogExt, MessageDialogButtons, MessageDialogKind};

    let plan = tools::resolve_install_plan(&id).await?;
    if plan.exact_version != exact_version {
        return Err(AppError::Invalid(
            "The available module version changed. Review the installation again.".into(),
        ));
    }
    let approved = app
        .dialog()
        .message(format!(
            "Approve once to install {} version {} from {}.\n\nScope: {}\nDestination: {}\n\nExact command:\n{}",
            plan.tool_name,
            plan.exact_version,
            plan.source,
            plan.scope,
            plan.destination,
            plan.command
        ))
        .title("Confirm exact PowerShell module installation")
        .kind(MessageDialogKind::Warning)
        .buttons(MessageDialogButtons::OkCancelCustom(
            "Install exact version".into(),
            "Cancel".into(),
        ))
        .blocking_show();
    if !approved {
        return Err(AppError::Invalid(
            "Tool installation approval was cancelled.".into(),
        ));
    }
    state.tool_install_approvals.issue(&plan)
}

#[tauri::command]
fn install_tool(
    app: AppHandle,
    state: State<'_, AppState>,
    id: String,
    exact_version: String,
    approval_token: String,
) -> AppResult<String> {
    state
        .tool_install_approvals
        .consume(&approval_token, &id, &exact_version)?;
    let spec = tools::install_spec(&id, &exact_version)?;
    spawn_operation(
        app,
        state.operations.clone(),
        format!("install:{id}"),
        spec,
        None,
    )
}

#[tauri::command]
async fn check_connections(target: Option<ConnectionTarget>) -> AppResult<Vec<ConnectionStatus>> {
    connections::validate_target(target.as_ref())?;
    Ok(connections::check_all(target.as_ref()).await)
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
    target: Option<ConnectionTarget>,
) -> AppResult<String> {
    let spec = connections::login_spec(&provider_id, &scopes, target.as_ref())?;
    spawn_operation(
        app,
        state.operations.clone(),
        format!("login:{provider_id}"),
        spec,
        None,
    )
}

#[tauri::command]
async fn start_azd_operation(
    app: AppHandle,
    state: State<'_, AppState>,
    action: String,
    project_path: String,
    environment: Option<String>,
    execution_approval_token: Option<String>,
) -> AppResult<String> {
    let canonical = std::fs::canonicalize(&project_path)?;
    if !canonical.join("azure.yaml").is_file() {
        return Err(AppError::Invalid(format!(
            "{} does not contain azure.yaml.",
            canonical.display()
        )));
    }
    let spec = azd::lifecycle_command(&action, &canonical, environment.as_deref())?;
    if matches!(
        action.as_str(),
        "preview" | "up" | "provision" | "deploy" | "down"
    ) {
        let environment = environment.as_deref().ok_or_else(|| {
            AppError::Invalid("A lifecycle action requires an explicit environment.".into())
        })?;
        let token = execution_approval_token
            .as_deref()
            .ok_or_else(|| AppError::Invalid("A native execution approval is required.".into()))?;
        let project = state
            .database
            .project_by_path(&canonical.display().to_string())?;
        if let Some(project) = &project {
            if let Some(snapshot) = resolve_project_template(&state.database, project)? {
                if let Some(manifest) = &snapshot.entry.manifest {
                    tools::ensure_manifest_requirements(manifest).await?;
                }
            }
        }
        state.execution_approvals.consume(
            token,
            ExecutionApprovalRequest {
                action: action.clone(),
                project_path: canonical.display().to_string(),
                environment: environment.to_string(),
                template_id: project
                    .as_ref()
                    .and_then(|project| project.template_id.clone())
                    .unwrap_or_default(),
                revision: project.and_then(|project| project.revision),
                source_binding: None,
            },
        )?;
    }
    spawn_operation(
        app,
        state.operations.clone(),
        action,
        spec,
        Some(OperationPersistence::new(
            state.database.clone(),
            canonical.display().to_string(),
        )),
    )
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
async fn preview_external_catalog(
    state: State<'_, AppState>,
    url: String,
) -> AppResult<ExternalCatalogPreview> {
    let source_url = catalog::normalize_catalog_url(url.trim())?.to_string();
    let catalog = catalog::preview_external_catalog(&source_url).await?;
    state.catalog_reviews.register(source_url, catalog)
}

#[tauri::command]
fn save_catalog_source(
    state: State<'_, AppState>,
    url: String,
    review_token: String,
) -> AppResult<Catalog> {
    let source_url = catalog::normalize_catalog_url(url.trim())?.to_string();
    let accepted_catalog = state.catalog_reviews.accept(&review_token, &source_url)?;
    state.database.save_catalog_source(&CatalogSource {
        id: accepted_catalog.id.clone(),
        label: accepted_catalog.title.clone(),
        url: source_url,
        trusted: true,
        pinned_revision: None,
        accepted_catalog: Some(accepted_catalog.clone()),
    })?;
    Ok(accepted_catalog)
}

#[tauri::command]
fn remove_catalog_source(state: State<'_, AppState>, id: String) -> AppResult<()> {
    state.database.remove_catalog_source(&id)
}

#[tauri::command]
async fn prepare_deployment(
    state: State<'_, AppState>,
    request: PrepareDeploymentRequest,
) -> AppResult<ProjectSummary> {
    let requires_local_binding = matches!(
        request.source_mode.as_str(),
        "localTemplate" | "existingProject"
    );
    if requires_local_binding != request.source_binding.is_some() {
        return Err(AppError::Invalid(
            "Local sources require the exact binding returned by native inspection.".into(),
        ));
    }
    state.execution_approvals.consume(
        &request.execution_approval_token,
        ExecutionApprovalRequest {
            action: "prepare".into(),
            project_path: request.destination.clone(),
            environment: request.environment.clone(),
            template_id: request.template_id.clone(),
            revision: request.source_revision.clone(),
            source_binding: request.source_binding.clone(),
        },
    )?;
    deployment::prepare(request, &state.template_cache, &state.database).await
}

#[tauri::command]
fn request_execution_approval(
    app: AppHandle,
    state: State<'_, AppState>,
    request: ExecutionApprovalRequest,
) -> AppResult<ExecutionApprovalGrant> {
    use tauri_plugin_dialog::{DialogExt, MessageDialogButtons, MessageDialogKind};

    let request = execution_approval::ExecutionApprovals::validate_request(request)?;
    let destructive = request.action == "down";
    let action = if destructive {
        "delete the environment's Azure resources with azd down --force"
    } else if request.action == "prepare" {
        "materialize the template and run azd init"
    } else {
        request.action.as_str()
    };
    let revision = request
        .revision
        .as_deref()
        .map(|revision| format!("\nRevision: {revision}"))
        .unwrap_or_default();
    let source_binding = request
        .source_binding
        .as_ref()
        .map(|binding| {
            format!(
                "\nReviewed local source: {}\nContent digest: {}\n.git and .azure runtime state are excluded. Local templates are staged after verification; existing-project working trees remain mutable after approval.",
                binding.source_path,
                &binding.content_digest[..12]
            )
        })
        .unwrap_or_default();
    let approved = app
        .dialog()
        .message(format!(
            "Approve once to {action}.\n\nProject: {}\nEnvironment: {}\nTemplate: {}{revision}{source_binding}\n\nTemplate hooks can change local files, Azure resources, or directory objects.",
            request.project_path, request.environment, request.template_id
        ))
        .title(if destructive {
            "Confirm Azure resource deletion"
        } else {
            "Confirm template code execution"
        })
        .kind(MessageDialogKind::Warning)
        .buttons(MessageDialogButtons::OkCancelCustom(
            if destructive { "Delete resources" } else { "Approve once" }.into(),
            "Cancel".into(),
        ))
        .blocking_show();
    if !approved {
        return Err(AppError::Invalid(
            "Execution approval was cancelled.".into(),
        ));
    }
    state.execution_approvals.issue(request)
}

#[tauri::command]
async fn get_project_state(
    project_path: String,
    environment: Option<String>,
) -> AppResult<serde_json::Value> {
    let canonical = std::fs::canonicalize(project_path)?;
    azd::show(&canonical, environment.as_deref()).await
}

#[tauri::command]
async fn list_project_environments(project_path: String) -> AppResult<Vec<ProjectEnvironment>> {
    let canonical = std::fs::canonicalize(project_path)?;
    azd::environment_list(&canonical).await
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
    azd::environment_new(
        &canonical,
        &environment,
        subscription.as_deref(),
        location.as_deref(),
    )
    .await?;
    let environments = azd::environment_list(&canonical).await?;
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
    azd::environment_select(&canonical, &environment).await?;
    let environments = azd::environment_list(&canonical).await?;
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

pub(crate) fn resolve_project_template(
    database: &Database,
    project: &ProjectSummary,
) -> AppResult<Option<ProjectTemplateSnapshot>> {
    if let Some(snapshot) = database.project_template(&project.path)? {
        return Ok(Some(snapshot));
    }
    // Legacy catalog projects may use only the exact recorded revision, never a newer catalog contract.
    let Some(template_id) = project.template_id.as_deref() else {
        return Ok(None);
    };
    let entry = match catalog::entry_by_id(template_id) {
        Ok(entry) => Some(entry),
        Err(_) => database.accepted_catalog_entry(template_id)?,
    };
    entry
        .filter(|entry| project.revision.as_deref() == Some(entry.commit.as_str()))
        .map(ProjectTemplateSnapshot::new)
        .transpose()
}

#[tauri::command]
fn get_project_template(
    state: State<'_, AppState>,
    project_path: String,
) -> AppResult<Option<ProjectTemplateSnapshot>> {
    let canonical = std::fs::canonicalize(project_path)?.display().to_string();
    let project = state
        .database
        .project_by_path(&canonical)?
        .ok_or_else(|| AppError::Invalid("The project is not registered.".into()))?;
    resolve_project_template(&state.database, &project)
}

fn project_configuration_fields(
    state: &AppState,
    project_path: &str,
) -> AppResult<Vec<crate::models::ConfigurationField>> {
    let project = state
        .database
        .project_by_path(project_path)?
        .ok_or_else(|| AppError::Invalid("The project is not registered.".into()))?;
    Ok(resolve_project_template(&state.database, &project)?
        .and_then(|snapshot| snapshot.entry.manifest)
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
    let description = azd::environment_list(&canonical)
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
    let fields = project_configuration_fields(&state, &canonical_path)?;
    let declared_names = fields
        .iter()
        .map(|field| field.environment_variable.clone())
        .collect::<HashSet<_>>();
    let mut values = Vec::new();
    for field in fields {
        let is_set = keys.contains(&field.environment_variable);
        let sensitive = field.sensitive || field.field_type == "secret";
        let value = if is_set && !sensitive {
            azd::environment_get_value(&canonical, &environment, &field.environment_variable)
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
    azd::environment_set_values(&canonical, &environment, &values).await
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

fn sanitized_draft(
    mut values: serde_json::Value,
    allowed_keys: &HashSet<String>,
    sensitive_keys: &HashSet<String>,
) -> AppResult<serde_json::Value> {
    let object = values
        .as_object_mut()
        .ok_or_else(|| AppError::Invalid("Wizard draft must be an object.".into()))?;
    object.retain(|key, _| {
        let key = key.to_ascii_lowercase();
        allowed_keys.contains(&key)
            && !sensitive_keys.contains(&key)
            && !["secret", "password", "token", "webhook", "credential"]
                .iter()
                .any(|marker| key.contains(marker))
    });
    if object.values().any(|value| {
        !(value.is_string()
            || value.is_boolean()
            || value
                .as_array()
                .is_some_and(|values| values.iter().all(serde_json::Value::is_string)))
    }) {
        return Err(AppError::Invalid(
            "Wizard draft values must be strings, booleans, or string arrays.".into(),
        ));
    }
    Ok(values)
}

fn draft_keys(
    state: &AppState,
    template_id: &str,
) -> AppResult<(HashSet<String>, HashSet<String>)> {
    let mut allowed_keys = [
        "sourceMode",
        "projectPath",
        "sourcePath",
        "environmentName",
        "subscriptionId",
        "location",
        "tenantId",
        "resourceGroup",
    ]
    .into_iter()
    .map(str::to_ascii_lowercase)
    .collect::<HashSet<_>>();
    let mut sensitive_keys = HashSet::new();
    let mut manifests = Vec::new();
    if let Ok(entry) = catalog::entry_by_id(template_id) {
        manifests.extend(entry.manifest);
    }
    manifests.extend(
        state
            .database
            .accepted_catalogs()?
            .into_iter()
            .flat_map(|catalog| catalog.entries)
            .filter(|entry| entry.id == template_id)
            .filter_map(|entry| entry.manifest),
    );
    manifests.extend(
        state
            .inspected_manifests
            .lock()
            .map_err(|_| AppError::Invalid("Template inspection cache is unavailable.".into()))?
            .get(template_id)
            .cloned(),
    );
    for field in manifests
        .into_iter()
        .flat_map(|manifest| manifest.configuration.groups)
        .flat_map(|group| group.fields)
    {
        let field_id = field.id.to_ascii_lowercase();
        allowed_keys.insert(field_id.clone());
        if field.sensitive || field.field_type == "secret" {
            sensitive_keys.insert(field_id);
        }
    }
    Ok((allowed_keys, sensitive_keys))
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
    let (allowed_keys, sensitive_keys) = draft_keys(state.inner(), &template_id)?;
    state.database.save_draft(
        &template_id,
        &sanitized_draft(values, &allowed_keys, &sensitive_keys)?,
    )
}

#[tauri::command]
fn load_wizard_draft(
    state: State<'_, AppState>,
    template_id: String,
) -> AppResult<Option<serde_json::Value>> {
    if template_id.len() > 128 {
        return Err(AppError::Invalid("Template ID is too long.".into()));
    }
    let Some(values) = state.database.load_draft(&template_id)? else {
        return Ok(None);
    };
    let (allowed_keys, sensitive_keys) = draft_keys(state.inner(), &template_id)?;
    let sanitized = sanitized_draft(values.clone(), &allowed_keys, &sensitive_keys)?;
    if sanitized != values {
        state.database.save_draft(&template_id, &sanitized)?;
    }
    Ok(Some(sanitized))
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
                database: Arc::new(database),
                operations: OperationManager::default(),
                template_cache,
                inspected_manifests: Mutex::new(HashMap::new()),
                catalog_reviews: CatalogReviews::default(),
                execution_approvals: execution_approval::ExecutionApprovals::default(),
                tool_install_approvals: tools::ToolInstallApprovals::default(),
            });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            bootstrap,
            list_projects,
            get_project_template,
            inspect_local_template,
            inspect_github_template,
            check_tools,
            check_template_prerequisites,
            preview_tool_install,
            request_tool_install_approval,
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
            remove_catalog_source,
            prepare_deployment,
            request_execution_approval,
            get_project_state,
            list_project_environments,
            create_project_environment,
            select_project_environment,
            get_project_environment_values,
            set_project_environment_values,
            get_operation_history,
            save_wizard_draft,
            load_wizard_draft,
            save_app_settings,
            clear_local_app_data
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

#[cfg(test)]
mod catalog_review_tests {
    use super::{catalog, CatalogReviews};

    fn external_catalog() -> crate::models::Catalog {
        let mut catalog = catalog::bundled_catalog().unwrap();
        catalog.id = "review-test".into();
        catalog.title = "Review test".into();
        for entry in &mut catalog.entries {
            entry.id = format!("external-review-test.{}", entry.id);
            use sha2::{Digest, Sha256};
            entry.manifest_sha256 = Some(format!(
                "{:x}",
                Sha256::digest(serde_json::to_vec(entry.manifest.as_ref().unwrap()).unwrap())
            ));
            entry.source = Some("external".into());
            entry.trusted = false;
        }
        catalog
    }

    #[test]
    fn accepts_only_the_backend_preview_for_the_same_source_once() {
        let reviews = CatalogReviews::default();
        let source = "https://example.com/azd-gui.catalog.json";
        let preview = reviews.register(source.into(), external_catalog()).unwrap();

        assert!(reviews
            .accept(&preview.review_token, "https://example.com/changed.json")
            .is_err());
        let preview = reviews.register(source.into(), external_catalog()).unwrap();
        let accepted = reviews.accept(&preview.review_token, source).unwrap();
        assert_eq!(accepted.id, preview.catalog.id);
        assert!(reviews.accept(&preview.review_token, source).is_err());
    }
}

#[cfg(test)]
mod draft_tests {
    use super::{
        catalog, draft_keys, sanitized_draft, AppState, CatalogReviews, Database, OperationManager,
    };
    use std::{
        collections::{HashMap, HashSet},
        sync::{Arc, Mutex},
    };
    use tempfile::tempdir;

    fn keys(values: &[&str]) -> HashSet<String> {
        values.iter().map(|value| value.to_string()).collect()
    }

    #[test]
    fn removes_manifest_secrets_even_when_the_name_is_neutral() {
        let draft = serde_json::json!({
            "apiKey": "do-not-store",
            "location": "westus2",
            "unexpected": "do-not-store-either"
        });

        let sanitized =
            sanitized_draft(draft, &keys(&["apikey", "location"]), &keys(&["apikey"])).unwrap();

        assert_eq!(sanitized, serde_json::json!({"location": "westus2"}));
    }

    #[test]
    fn rejects_values_outside_the_typed_wizard_contract() {
        let error = sanitized_draft(
            serde_json::json!({"location": {"token": "nested-secret"}}),
            &keys(&["location"]),
            &HashSet::new(),
        )
        .unwrap_err();

        assert!(error
            .to_string()
            .contains("strings, booleans, or string arrays"));
    }

    #[test]
    fn resolves_sensitive_fields_from_bundled_and_inspected_manifests() {
        let directory = tempdir().unwrap();
        let state = AppState {
            database: Arc::new(Database::open(&directory.path().join("state.sqlite")).unwrap()),
            operations: OperationManager::default(),
            template_cache: directory.path().join("cache"),
            inspected_manifests: Mutex::new(HashMap::new()),
            catalog_reviews: CatalogReviews::default(),
            execution_approvals: crate::execution_approval::ExecutionApprovals::default(),
            tool_install_approvals: crate::tools::ToolInstallApprovals::default(),
        };

        let (allowed, sensitive) = draft_keys(&state, "entra-health-monitoring").unwrap();
        assert!(allowed.contains("teamschannellink"));
        assert!(!sensitive.contains("teamschannellink"));

        let manifest = catalog::entry_by_id("entra-health-monitoring")
            .unwrap()
            .manifest
            .unwrap();
        state
            .inspected_manifests
            .lock()
            .unwrap()
            .insert("local-template".into(), manifest);
        let (allowed, sensitive) = draft_keys(&state, "local-template").unwrap();
        assert!(allowed.contains("teamschannellink"));
        assert!(!sensitive.contains("teamschannellink"));
    }
}
