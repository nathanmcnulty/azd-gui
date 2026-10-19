mod acknowledgements;
mod azd;
mod azure;
mod catalog;
mod command_runner;
mod connection_profiles;
mod connections;
mod deployment;
mod deployment_receipt;
mod error;
mod evidence;
mod execution_approval;
mod local_source;
mod manifest;
mod models;
mod permissions;
mod persistence;
mod redaction;
mod security_groups;
mod terminal;
mod tools;
mod validation_report;

use crate::{
    command_runner::{spawn_operation, OperationManager, OperationPersistence},
    error::{AppError, AppResult},
    evidence::{read_target_values, ExpectedEvidenceContext, OperationEvidenceContext},
    models::{
        AppSettings, AzureDiscovery, BootstrapData, Catalog, CatalogEntry, CatalogSource,
        ConnectionStatus, ConnectionTarget, ExecutionApprovalGrant, ExecutionApprovalRequest,
        ExecutionTarget, ExternalCatalogPreview, OperationEvent, PermissionCheckRequest,
        PermissionCheckResult, PrepareDeploymentRequest, ProjectEnvironment,
        ProjectEnvironmentValue, ProjectEnvironmentValueInput, ProjectSummary,
        ProjectTemplateSnapshot, SecurityGroupSearch, TemplateInspection, TemplateManifest,
        ToolInstallApprovalGrant, ToolInstallPlan, ToolStatus,
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
use tokio::sync::Mutex as AsyncMutex;

const MAX_INSPECTED_MANIFESTS: usize = 128;
const MAX_DRAFT_IDENTITY_BYTES: usize = 1024 * 1024;

pub struct AppState {
    database: Arc<Database>,
    operations: OperationManager,
    environment_writes: AsyncMutex<()>,
    template_cache: PathBuf,
    inspected_manifests: Mutex<HashMap<String, TemplateManifest>>,
    reviewed_contracts: Mutex<HashMap<String, ProjectTemplateSnapshot>>,
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
    let _environment_lock = state.environment_writes.lock().await;
    let _clear_guard = state.operations.begin_clear()?;

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
    state
        .reviewed_contracts
        .lock()
        .map_err(|_| AppError::Invalid("Reviewed contract cache is unavailable.".into()))?
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
async fn check_template_prerequisites(
    manifest: TemplateManifest,
    environment_values: Option<HashMap<String, String>>,
) -> AppResult<Vec<ToolStatus>> {
    manifest::validate(&manifest)?;
    tools::evaluate_manifest_prerequisites(&manifest, &environment_values.unwrap_or_default()).await
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
async fn list_visible_profiles(
    provider_id: String,
) -> AppResult<connection_profiles::VisibleProfiles> {
    connection_profiles::list(&provider_id).await
}

#[tauri::command]
async fn discover_azure_scope(
    subscription_id: Option<String>,
    refresh: bool,
) -> AppResult<AzureDiscovery> {
    azure::discover(subscription_id.as_deref(), refresh).await
}

#[tauri::command]
async fn search_security_groups(
    tenant_id: String,
    subscription_id: String,
    query: String,
) -> AppResult<SecurityGroupSearch> {
    security_groups::search(&tenant_id, &subscription_id, &query).await
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
    start_azd_operation_checked(
        state.inner(),
        action,
        project_path,
        environment,
        execution_approval_token,
        |action, spec, persistence| {
            spawn_operation(
                app,
                state.operations.clone(),
                action,
                spec,
                Some(persistence),
            )
        },
    )
    .await
}

// Keep the entire production validation-to-launch path testable without a GUI
// runtime. The Tauri command above has no independent process-launch route.
async fn start_azd_operation_checked<F>(
    state: &AppState,
    action: String,
    project_path: String,
    environment: Option<String>,
    execution_approval_token: Option<String>,
    launch: F,
) -> AppResult<String>
where
    F: FnOnce(String, command_runner::CommandSpec, OperationPersistence) -> AppResult<String>,
{
    // Keep in-app environment writes outside the target read -> approval consume
    // -> operation reservation window. Once reserved, edits fail until completion.
    let _environment_lock = state.environment_writes.lock().await;
    let canonical = std::fs::canonicalize(&project_path)?;
    if !canonical.join("azure.yaml").is_file() {
        return Err(AppError::Invalid(format!(
            "{} does not contain azure.yaml.",
            canonical.display()
        )));
    }
    let spec = azd::lifecycle_command(&action, &canonical, environment.as_deref())?;
    let mut persistence_context = None;
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
        let snapshot = if let Some(project) = &project {
            let snapshot = resolve_project_template(&state.database, project)?;
            ensure_registered_project_lifecycle_contract(&action, snapshot.as_ref())?;
            if let Some(snapshot) = &snapshot {
                ensure_project_lifecycle_route(&action, snapshot)?;
            }
            snapshot
        } else {
            None
        };
        if let (Some(project), Some(snapshot)) = (project.as_ref(), snapshot.as_ref()) {
            persistence_context = Some(operation_evidence_context(project, snapshot, environment));
        }
        let manifest = snapshot
            .as_ref()
            .and_then(|snapshot| snapshot.entry.manifest.as_ref());
        let acknowledgement_values = if action == "down" {
            HashMap::new()
        } else {
            manifest
                .map(|manifest| {
                    acknowledgements::saved_binding_values(manifest, &canonical, environment)
                })
                .transpose()?
                .unwrap_or_default()
        };
        let expected = ExecutionApprovalRequest {
            action: action.clone(),
            project_path: canonical.display().to_string(),
            environment: environment.to_string(),
            template_id: project
                .as_ref()
                .and_then(|project| project.template_id.clone())
                .unwrap_or_default(),
            revision: project
                .as_ref()
                .and_then(|project| project.revision.clone()),
            snapshot_digest: snapshot.as_ref().map(|snapshot| snapshot.digest.clone()),
            source_binding: None,
            acknowledgement_proofs: Vec::new(),
            acknowledgement_values,
            target: Some(lifecycle_execution_target(&canonical, environment)?),
        };
        state
            .execution_approvals
            .consume_lifecycle(token, expected.clone(), manifest)?;
        if let Some(manifest) = manifest {
            let requirement_values =
                registered_requirement_environment_values(manifest, &canonical, environment)
                    .await?;
            tools::ensure_manifest_requirements(manifest, &requirement_values).await?;
        }
        // Tool checks can take time. Re-read immediately before the final launch,
        // in addition to the pre-process gate, to catch external environment edits.
        let current_values = if action == "down" {
            HashMap::new()
        } else {
            manifest
                .map(|manifest| {
                    acknowledgements::saved_binding_values(manifest, &canonical, environment)
                })
                .transpose()?
                .unwrap_or_default()
        };
        if current_values != expected.acknowledgement_values
            || Some(lifecycle_execution_target(&canonical, environment)?) != expected.target
        {
            return Err(AppError::Invalid(
                "The saved execution context changed before launch. Review and approve again."
                    .into(),
            ));
        }
    }
    launch(
        action,
        spec,
        match persistence_context {
            Some(context) => OperationPersistence::with_context(
                state.database.clone(),
                canonical.display().to_string(),
                context,
            ),
            None => {
                OperationPersistence::new(state.database.clone(), canonical.display().to_string())
            }
        },
    )
}

fn ensure_registered_project_lifecycle_contract(
    action: &str,
    snapshot: Option<&ProjectTemplateSnapshot>,
) -> AppResult<()> {
    if matches!(action, "preview" | "up" | "provision" | "deploy" | "down") && snapshot.is_none() {
        return Err(AppError::Invalid(
            "This registered project needs a saved reviewed template contract before lifecycle actions. Refresh and inspection remain available; for an external catalog, review and add the exact recorded revision first."
                .into(),
        ));
    }
    Ok(())
}

fn validate_registered_project_approval(
    database: &Database,
    request: ExecutionApprovalRequest,
) -> AppResult<ExecutionApprovalRequest> {
    if request.action == "prepare" {
        return Ok(request);
    }
    let Some(project) = database.project_by_path(&request.project_path)? else {
        return Ok(request);
    };
    let snapshot = resolve_project_template(database, &project)?;
    ensure_registered_project_lifecycle_contract(&request.action, snapshot.as_ref())?;
    let snapshot = snapshot.expect("registered lifecycle contract was required");
    if request.snapshot_digest.as_deref() != Some(snapshot.digest.as_str()) {
        return Err(AppError::Invalid(
            "The saved template contract changed. Review and approve the lifecycle action again."
                .into(),
        ));
    }
    Ok(request)
}

fn ensure_project_lifecycle_route(
    action: &str,
    snapshot: &ProjectTemplateSnapshot,
) -> AppResult<()> {
    if matches!(action, "preview" | "up" | "provision" | "deploy" | "down")
        && snapshot
            .entry
            .manifest
            .as_ref()
            .is_some_and(|manifest| manifest.requires_interactive_terminal)
    {
        return Err(AppError::Invalid(
            "This project's saved template contract requires an interactive terminal. Open the project terminal to run azd.".into(),
        ));
    }
    Ok(())
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
    let _environment_lock = state.environment_writes.lock().await;
    let _preparation_guard = state.operations.begin_preparation()?;
    let requires_local_binding = matches!(
        request.source_mode.as_str(),
        "localTemplate" | "existingProject"
    );
    if requires_local_binding != request.source_binding.is_some() {
        return Err(AppError::Invalid(
            "Local sources require the exact binding returned by native inspection.".into(),
        ));
    }
    let mut expected = ExecutionApprovalRequest {
        action: "prepare".into(),
        project_path: request.destination.clone(),
        environment: request.environment.clone(),
        template_id: request.template_id.clone(),
        revision: request.source_revision.clone(),
        snapshot_digest: Some(request.reviewed_contract_digest.clone()),
        acknowledgement_proofs: request.acknowledgement_proofs.clone(),
        acknowledgement_values: request.environment_values.clone(),
        source_binding: request.source_binding.clone(),
        target: Some(execution_approval::validate_target(ExecutionTarget {
            tenant_id: request.tenant.clone(),
            subscription_id: request.subscription.clone(),
            location: Some(request.location.clone()),
            resource_group: request.resource_group.clone(),
        })?),
    };
    prepare_approval_acknowledgements(state.inner(), &mut expected)?;
    state
        .execution_approvals
        .consume(&request.execution_approval_token, expected)?;
    // Preparation can also rewrite an existing environment. Revoke any other
    // pending grants before those writes, including an A -> B -> A sequence.
    state.execution_approvals.clear()?;
    deployment::prepare(request, &state.template_cache, &state.database).await
}

#[tauri::command]
fn reviewed_contract_digest(state: State<'_, AppState>, entry: CatalogEntry) -> AppResult<String> {
    let snapshot = reviewed_contract_snapshot(entry)?;
    let digest = snapshot.digest.clone();
    let mut contracts = state
        .reviewed_contracts
        .lock()
        .map_err(|_| AppError::Invalid("Reviewed contract cache is unavailable.".into()))?;
    if contracts.len() >= MAX_INSPECTED_MANIFESTS {
        contracts.clear();
    }
    contracts.insert(digest.clone(), snapshot);
    Ok(digest)
}

fn reviewed_contract_snapshot(mut entry: CatalogEntry) -> AppResult<ProjectTemplateSnapshot> {
    // Bootstrap marks accepted external entries as reviewed for presentation. The
    // persisted contract retains the source document's false value, so exclude
    // that UI-only state from the digest supplied for native preparation.
    if entry.source.as_deref() == Some("external") {
        entry.trusted = false;
    }
    ProjectTemplateSnapshot::new(entry)
}

fn prepare_approval_acknowledgements(
    state: &AppState,
    request: &mut ExecutionApprovalRequest,
) -> AppResult<()> {
    let contracts = state
        .reviewed_contracts
        .lock()
        .map_err(|_| AppError::Invalid("Reviewed contract cache is unavailable.".into()))?;
    let snapshot = request
        .snapshot_digest
        .as_ref()
        .and_then(|digest| contracts.get(digest))
        .ok_or_else(|| {
            AppError::Invalid(
                "Review the exact template contract before requesting preparation approval.".into(),
            )
        })?;
    let revision = (snapshot.entry.commit != "local").then_some(snapshot.entry.commit.as_str());
    if snapshot.entry.id != request.template_id || request.revision.as_deref() != revision {
        return Err(AppError::Invalid(
            "The reviewed contract does not match this template and revision.".into(),
        ));
    }
    if let Some(manifest) = &snapshot.entry.manifest {
        acknowledgements::validate_preparation(
            manifest,
            &request.acknowledgement_values,
            request
                .target
                .as_ref()
                .and_then(|target| target.tenant_id.as_deref()),
            &request.acknowledgement_proofs,
        )?;
        request.acknowledgement_values =
            acknowledgements::binding_values(manifest, &request.acknowledgement_values)?;
    } else {
        request.acknowledgement_values.clear();
        if !request.acknowledgement_proofs.is_empty() {
            return Err(AppError::Invalid(
                "This template does not declare acknowledgements.".into(),
            ));
        }
    }
    Ok(())
}

fn lifecycle_approval_acknowledgements(
    database: &Database,
    request: &mut ExecutionApprovalRequest,
) -> AppResult<()> {
    if request.action == "down" {
        request.acknowledgement_values.clear();
        request.acknowledgement_proofs.clear();
        return Ok(());
    }
    let manifest = database
        .project_by_path(&request.project_path)?
        .map(|project| resolve_project_template(database, &project))
        .transpose()?
        .flatten()
        .and_then(|snapshot| snapshot.entry.manifest);
    if let Some(manifest) = manifest {
        request.acknowledgement_values = acknowledgements::saved_binding_values(
            &manifest,
            Path::new(&request.project_path),
            &request.environment,
        )?;
        acknowledgements::validate(
            &manifest,
            &request.acknowledgement_values,
            request
                .target
                .as_ref()
                .and_then(|target| target.tenant_id.as_deref()),
            &request.acknowledgement_proofs,
        )?;
    } else if !request.acknowledgement_proofs.is_empty()
        || !request.acknowledgement_values.is_empty()
    {
        return Err(AppError::Invalid(
            "This project does not declare acknowledgements.".into(),
        ));
    }
    Ok(())
}

#[tauri::command]
fn request_execution_approval(
    app: AppHandle,
    state: State<'_, AppState>,
    mut request: ExecutionApprovalRequest,
) -> AppResult<ExecutionApprovalGrant> {
    use tauri_plugin_dialog::{DialogExt, MessageDialogButtons, MessageDialogKind};

    if matches!(
        request.action.as_str(),
        "preview" | "up" | "provision" | "deploy" | "down"
    ) {
        request.target = Some(lifecycle_execution_target(
            Path::new(&request.project_path),
            &request.environment,
        )?);
    }
    let request = execution_approval::ExecutionApprovals::validate_request(request)?;
    let mut request = validate_registered_project_approval(&state.database, request)?;
    if request.action == "prepare" {
        prepare_approval_acknowledgements(state.inner(), &mut request)?;
    } else {
        lifecycle_approval_acknowledgements(&state.database, &mut request)?;
    }
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
    let target = request.target.as_ref().expect("validated approval target");
    let target_summary =
        format!(
        "\nAzure cloud: AzureCloud\nTenant: {}\nSubscription: {}\nLocation: {}\nResource group: {}",
        target.tenant_id.as_deref().unwrap_or("not selected"),
        target.subscription_id.as_deref().unwrap_or("not selected"),
        target.location.as_deref().unwrap_or("not selected"),
        target.resource_group.as_deref().unwrap_or("azd/template default")
    );
    let approved = app
        .dialog()
        .message(format!(
            "Approve once to {action}.\n\nProject: {}\nEnvironment: {}\nTemplate: {}{revision}{source_binding}{target_summary}\n\nTemplate hooks can change local files, Azure resources, or directory objects.",
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
    if request.action != "prepare"
        && request.target.as_ref()
            != Some(&lifecycle_execution_target(
                Path::new(&request.project_path),
                &request.environment,
            )?)
    {
        return Err(AppError::Invalid(
            "The saved Azure target changed during approval. Review the target and approve again."
                .into(),
        ));
    }
    if request.action != "prepare" {
        let mut fresh = request.clone();
        lifecycle_approval_acknowledgements(&state.database, &mut fresh)?;
        if fresh != request {
            return Err(AppError::Invalid("The saved acknowledgement context changed during approval. Review and acknowledge again.".into()));
        }
    }
    state.execution_approvals.issue(request)
}

fn lifecycle_execution_target(
    project_path: &Path,
    environment: &str,
) -> AppResult<ExecutionTarget> {
    let values = read_target_values(project_path, environment);
    if values
        .azure_cloud
        .as_deref()
        .is_some_and(|cloud| cloud != "AzureCloud")
    {
        return Err(AppError::Invalid(
            "The selected environment is not configured for AzureCloud.".into(),
        ));
    }
    let target = execution_approval::validate_target(ExecutionTarget {
        tenant_id: values.tenant_id,
        subscription_id: values.subscription_id,
        location: values.location,
        resource_group: values.resource_group,
    })?;
    if target.tenant_id.is_none() || target.subscription_id.is_none() {
        return Err(AppError::Invalid(
            "The selected environment needs explicit AZURE_TENANT_ID and AZURE_SUBSCRIPTION_ID before lifecycle approval."
                .into(),
        ));
    }
    Ok(target)
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
fn get_project_validation_report(
    state: State<'_, AppState>,
    project_path: String,
) -> AppResult<validation_report::ValidationReportRead> {
    let root = PathBuf::from(project_path);
    let canonical = std::fs::canonicalize(&root)?.display().to_string();
    let project = state
        .database
        .project_by_path(&canonical)?
        .ok_or_else(|| AppError::Invalid("The project is not registered.".into()))?;
    let expected = expected_evidence_context(&state.database, &project)?;
    Ok(validation_report::read_with_context(
        &root,
        project.environment.as_deref(),
        Some(&expected),
    ))
}

#[tauri::command]
fn get_project_deployment_receipt(
    state: State<'_, AppState>,
    project_path: String,
) -> AppResult<deployment_receipt::DeploymentReceiptRead> {
    let root = PathBuf::from(project_path);
    let canonical = std::fs::canonicalize(&root)?.display().to_string();
    let project = state
        .database
        .project_by_path(&canonical)?
        .ok_or_else(|| AppError::Invalid("The project is not registered.".into()))?;
    let expected = expected_evidence_context(&state.database, &project)?;
    Ok(deployment_receipt::read_with_context(
        Path::new(&project.path),
        Some(&expected),
    ))
}

fn operation_evidence_context(
    project: &ProjectSummary,
    snapshot: &ProjectTemplateSnapshot,
    environment: &str,
) -> OperationEvidenceContext {
    let target = read_target_values(Path::new(&project.path), environment);
    OperationEvidenceContext {
        project_id: project.id.clone(),
        environment: environment.to_string(),
        azure_cloud: target.azure_cloud,
        tenant_id: target.tenant_id,
        subscription_id: target.subscription_id,
        resource_group: target.resource_group,
        template_id: snapshot.entry.id.clone(),
        revision: (snapshot.entry.commit != "local").then(|| snapshot.entry.commit.clone()),
        contract_digest: Some(snapshot.digest.clone()),
    }
}

fn expected_evidence_context(
    database: &Database,
    project: &ProjectSummary,
) -> AppResult<ExpectedEvidenceContext> {
    let snapshot = database.project_template(&project.path)?;
    let environment = project.environment.clone().unwrap_or_default();
    let current = if let Some(snapshot) = snapshot {
        operation_evidence_context(project, &snapshot, &environment)
    } else {
        let target = read_target_values(Path::new(&project.path), &environment);
        OperationEvidenceContext {
            project_id: project.id.clone(),
            environment,
            azure_cloud: target.azure_cloud,
            tenant_id: target.tenant_id,
            subscription_id: target.subscription_id,
            resource_group: target.resource_group,
            template_id: project.template_id.clone().unwrap_or_default(),
            revision: project.revision.clone(),
            contract_digest: None,
        }
    };
    Ok(ExpectedEvidenceContext {
        current,
        operations: database.evidence_operations(&project.path)?,
    })
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
    let _environment_lock = state.environment_writes.lock().await;
    let _environment_change = state.operations.begin_environment_change()?;
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
    let _environment_lock = state.environment_writes.lock().await;
    let _environment_change = state.operations.begin_environment_change()?;
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
    Ok(None)
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

#[tauri::command]
async fn refresh_project_template(
    state: State<'_, AppState>,
    project_path: String,
) -> AppResult<ProjectTemplateSnapshot> {
    if !state.operations.is_idle().await {
        return Err(AppError::Invalid(
            "Wait for active operations before replacing a project contract.".into(),
        ));
    }
    let canonical = std::fs::canonicalize(project_path)?;
    state
        .database
        .refresh_project_template(&canonical.display().to_string())
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

async fn registered_requirement_environment_values(
    manifest: &TemplateManifest,
    project_path: &Path,
    environment: &str,
) -> AppResult<HashMap<String, String>> {
    let controllers = manifest::requirement_controller_variables(manifest)?;
    if controllers.is_empty() {
        return Ok(HashMap::new());
    }
    // Read once immediately before spawning. This preserves the distinction
    // between a missing key (manifest default) and an explicitly empty value
    // (invalid), without returning other environment values to the renderer.
    let persisted = azd::environment_get_values(project_path, environment).await?;
    Ok(controllers
        .into_iter()
        .filter_map(|environment_variable| {
            persisted
                .get(&environment_variable)
                .map(|value| (environment_variable, value.clone()))
        })
        .collect())
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
    let mut declared_names = fields
        .iter()
        .map(|field| field.environment_variable.clone())
        .collect::<HashSet<_>>();
    let mut values = Vec::new();
    if fields
        .iter()
        .any(|field| field.field_type == "acknowledgement")
    {
        let tenant = read_target_values(&canonical, &environment).tenant_id;
        declared_names.insert("AZURE_TENANT_ID".into());
        values.push(ProjectEnvironmentValue {
            name: "AZURE_TENANT_ID".into(),
            is_set: tenant.is_some(),
            value: tenant,
            sensitive: false,
            declared: true,
            required: false,
        });
    }
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
    state: State<'_, AppState>,
    project_path: String,
    environment: String,
    values: Vec<ProjectEnvironmentValueInput>,
) -> AppResult<()> {
    let _environment_lock = state.environment_writes.lock().await;
    let _environment_change = state.operations.begin_environment_change()?;
    let canonical = std::fs::canonicalize(project_path)?;
    if !canonical.join("azure.yaml").is_file() {
        return Err(AppError::Invalid(format!(
            "{} does not contain azure.yaml.",
            canonical.display()
        )));
    }
    // A GUI edit is a new intent boundary even if the user later restores the
    // previous values. Never revive a native grant across an A -> B -> A edit.
    state.execution_approvals.clear()?;
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
    environment: Option<String>,
) -> AppResult<Vec<OperationEvent>> {
    let canonical = std::fs::canonicalize(project_path)?;
    state
        .database
        .operation_history_for_environment(&canonical.display().to_string(), environment.as_deref())
}

fn sanitized_draft(
    mut values: serde_json::Value,
    allowed_keys: &HashSet<String>,
    sensitive_keys: &HashSet<String>,
) -> AppResult<serde_json::Value> {
    let object = values
        .as_object_mut()
        .ok_or_else(|| AppError::Invalid("Wizard draft must be an object.".into()))?;
    object.retain(|key, value| {
        let key = key.to_ascii_lowercase();
        !value.is_null()
            && allowed_keys.contains(&key)
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
        if field.sensitive || matches!(field.field_type.as_str(), "secret" | "acknowledgement") {
            sensitive_keys.insert(field_id);
        }
    }
    Ok((allowed_keys, sensitive_keys))
}

fn draft_storage_key(template_id: &str, identity: &CatalogEntry) -> AppResult<String> {
    if template_id.len() > 128 {
        return Err(AppError::Invalid("Template ID is too long.".into()));
    }
    if identity.id != template_id {
        return Err(AppError::Invalid(
            "Draft identity does not match the template ID.".into(),
        ));
    }
    if serde_json::to_vec(identity)?.len() > MAX_DRAFT_IDENTITY_BYTES {
        return Err(AppError::Invalid("Draft identity is too large.".into()));
    }
    let snapshot = ProjectTemplateSnapshot::new(identity.clone())?;
    Ok(format!("v2:{template_id}:{}", snapshot.digest))
}

#[tauri::command]
fn save_wizard_draft(
    state: State<'_, AppState>,
    template_id: String,
    identity: CatalogEntry,
    values: serde_json::Value,
) -> AppResult<()> {
    let storage_key = draft_storage_key(&template_id, &identity)?;
    let (allowed_keys, sensitive_keys) = draft_keys(state.inner(), &template_id)?;
    state.database.save_draft(
        &storage_key,
        &sanitized_draft(values, &allowed_keys, &sensitive_keys)?,
    )
}

#[tauri::command]
fn load_wizard_draft(
    state: State<'_, AppState>,
    template_id: String,
    identity: CatalogEntry,
) -> AppResult<Option<serde_json::Value>> {
    let storage_key = draft_storage_key(&template_id, &identity)?;
    let Some(values) = state.database.load_draft(&storage_key)? else {
        return Ok(None);
    };
    let (allowed_keys, sensitive_keys) = draft_keys(state.inner(), &template_id)?;
    let sanitized = sanitized_draft(values.clone(), &allowed_keys, &sensitive_keys)?;
    if sanitized != values {
        state.database.save_draft(&storage_key, &sanitized)?;
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
                environment_writes: AsyncMutex::new(()),
                template_cache,
                inspected_manifests: Mutex::new(HashMap::new()),
                reviewed_contracts: Mutex::new(HashMap::new()),
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
            refresh_project_template,
            inspect_local_template,
            inspect_github_template,
            check_tools,
            check_template_prerequisites,
            preview_tool_install,
            request_tool_install_approval,
            install_tool,
            check_connections,
            list_visible_profiles,
            discover_azure_scope,
            search_security_groups,
            run_permission_checks,
            start_login,
            start_azd_operation,
            cancel_operation,
            open_external_terminal,
            preview_external_catalog,
            save_catalog_source,
            remove_catalog_source,
            reviewed_contract_digest,
            prepare_deployment,
            request_execution_approval,
            get_project_state,
            get_project_validation_report,
            get_project_deployment_receipt,
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
    use super::{catalog, reviewed_contract_snapshot, CatalogReviews, ProjectTemplateSnapshot};

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

    #[test]
    fn reviewed_external_display_state_keeps_the_persisted_contract_digest() {
        let mut catalog = external_catalog();
        let mut persisted = catalog.entries.remove(0);
        let expected = ProjectTemplateSnapshot::new(persisted.clone())
            .unwrap()
            .digest;
        persisted.trusted = true;

        assert_eq!(
            reviewed_contract_snapshot(persisted).unwrap().digest,
            expected
        );
    }
}

#[cfg(test)]
mod interactive_lifecycle_tests {
    use super::{
        catalog, ensure_project_lifecycle_route, ensure_registered_project_lifecycle_contract,
        lifecycle_execution_target, operation_evidence_context,
        validate_registered_project_approval, Database, ExecutionApprovalRequest, ExecutionTarget,
        ProjectSummary, ProjectTemplateSnapshot,
    };
    use crate::execution_approval::ExecutionApprovals;
    use tempfile::tempdir;

    #[test]
    fn registered_project_without_saved_contract_blocks_all_native_lifecycle_actions() {
        for action in ["preview", "up", "provision", "deploy", "down"] {
            assert!(ensure_registered_project_lifecycle_contract(action, None).is_err());
        }
        assert!(ensure_registered_project_lifecycle_contract("refresh", None).is_ok());
        let snapshot =
            ProjectTemplateSnapshot::new(catalog::entry_by_id("myworkid").unwrap()).unwrap();
        assert!(ensure_registered_project_lifecycle_contract("up", Some(&snapshot)).is_ok());
        assert!(ensure_project_lifecycle_route("refresh", &snapshot).is_ok());
    }

    #[test]
    fn registered_project_approval_requires_the_current_saved_contract_digest() {
        let directory = tempdir().unwrap();
        std::fs::write(directory.path().join("azure.yaml"), "name: sample").unwrap();
        let database = Database::open(&directory.path().join("state.sqlite")).unwrap();
        let entry = catalog::entry_by_id("myworkid").unwrap();
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
        let snapshot = ProjectTemplateSnapshot::new(entry).unwrap();
        database.save_project_template(&project, &snapshot).unwrap();
        let request = ExecutionApprovalRequest {
            action: "preview".into(),
            project_path: project.path.clone(),
            environment: "dev".into(),
            template_id: project.template_id.clone().unwrap(),
            revision: project.revision.clone(),
            snapshot_digest: Some(snapshot.digest.clone()),
            source_binding: None,
            acknowledgement_proofs: Vec::new(),
            acknowledgement_values: Default::default(),
            target: Some(ExecutionTarget {
                tenant_id: Some("11111111-1111-1111-1111-111111111111".into()),
                subscription_id: Some("22222222-2222-2222-2222-222222222222".into()),
                location: Some("westus2".into()),
                resource_group: None,
            }),
        };
        assert!(validate_registered_project_approval(&database, request.clone()).is_ok());
        let mut stale = request;
        stale.snapshot_digest = Some("a".repeat(64));
        assert!(validate_registered_project_approval(&database, stale).is_err());
    }

    #[test]
    fn operation_evidence_uses_the_approved_environment_not_the_saved_selection() {
        let directory = tempdir().unwrap();
        for (environment, subscription) in [
            ("dev", "11111111-1111-1111-1111-111111111111"),
            ("staging", "22222222-2222-2222-2222-222222222222"),
        ] {
            let environment_dir = directory.path().join(".azure").join(environment);
            std::fs::create_dir_all(&environment_dir).unwrap();
            std::fs::write(
                environment_dir.join(".env"),
                format!("AZURE_SUBSCRIPTION_ID={subscription}\n"),
            )
            .unwrap();
        }
        let entry = catalog::entry_by_id("myworkid").unwrap();
        let snapshot = ProjectTemplateSnapshot::new(entry.clone()).unwrap();
        let project = ProjectSummary {
            id: "sample-dev".into(),
            name: "sample".into(),
            path: directory.path().display().to_string(),
            environment: Some("dev".into()),
            template_id: Some(entry.id),
            revision: Some(entry.commit),
            last_operation: None,
            state: "ready".into(),
        };

        let context = operation_evidence_context(&project, &snapshot, "staging");

        assert_eq!(context.environment, "staging");
        assert_eq!(
            context.subscription_id.as_deref(),
            Some("22222222-2222-2222-2222-222222222222")
        );
    }

    #[test]
    fn lifecycle_approval_target_tracks_saved_environment_changes() {
        let directory = tempdir().unwrap();
        std::fs::write(directory.path().join("azure.yaml"), "name: sample").unwrap();
        let environment = directory.path().join(".azure/dev");
        std::fs::create_dir_all(&environment).unwrap();
        let dotenv = environment.join(".env");
        std::fs::write(
            &dotenv,
            "AZD_GUI_AZURE_CLOUD=AzureCloud\nAZURE_TENANT_ID=11111111-1111-1111-1111-111111111111\nAZURE_SUBSCRIPTION_ID=22222222-2222-2222-2222-222222222222\nAZURE_LOCATION=westus2\nAZURE_RESOURCE_GROUP=rg-original\n",
        ).unwrap();
        let original = lifecycle_execution_target(directory.path(), "dev").unwrap();
        assert_eq!(original.resource_group.as_deref(), Some("rg-original"));
        let approvals = ExecutionApprovals::default();
        let request = ExecutionApprovalRequest {
            action: "down".into(),
            project_path: directory.path().display().to_string(),
            environment: "dev".into(),
            template_id: "sample".into(),
            revision: None,
            snapshot_digest: None,
            source_binding: None,
            acknowledgement_proofs: Vec::new(),
            acknowledgement_values: Default::default(),
            target: Some(original.clone()),
        };
        let grant = approvals.issue(request.clone()).unwrap();

        std::fs::write(
            &dotenv,
            "AZD_GUI_AZURE_CLOUD=AzureCloud\nAZURE_TENANT_ID=11111111-1111-1111-1111-111111111111\nAZURE_SUBSCRIPTION_ID=33333333-3333-3333-3333-333333333333\nAZURE_LOCATION=eastus2\nAZURE_RESOURCE_GROUP=rg-other\n",
        ).unwrap();
        let changed = lifecycle_execution_target(directory.path(), "dev").unwrap();
        assert_ne!(original, changed);
        assert_eq!(
            changed.subscription_id.as_deref(),
            Some("33333333-3333-3333-3333-333333333333")
        );
        let mut retargeted = request;
        retargeted.target = Some(changed);
        assert!(approvals.consume(&grant.token, retargeted).is_err());

        std::fs::write(&dotenv, "AZURE_LOCATION=westus2\n").unwrap();
        assert!(lifecycle_execution_target(directory.path(), "dev").is_err());
    }

    #[test]
    fn saved_interactive_contract_requires_terminal_for_all_template_actions() {
        let mut entry = catalog::entry_by_id("myworkid").unwrap();
        entry
            .manifest
            .as_mut()
            .unwrap()
            .requires_interactive_terminal = true;
        let snapshot = ProjectTemplateSnapshot::new(entry).unwrap();
        for action in ["preview", "up", "provision", "deploy", "down"] {
            assert!(ensure_project_lifecycle_route(action, &snapshot).is_err());
        }
        assert!(ensure_project_lifecycle_route("refresh", &snapshot).is_ok());
    }

    #[test]
    fn noninteractive_contract_keeps_output_panel_route() {
        let mut entry = catalog::entry_by_id("myworkid").unwrap();
        entry
            .manifest
            .as_mut()
            .unwrap()
            .requires_interactive_terminal = false;
        let snapshot = ProjectTemplateSnapshot::new(entry).unwrap();
        for action in ["preview", "up", "provision", "deploy", "down", "refresh"] {
            assert!(ensure_project_lifecycle_route(action, &snapshot).is_ok());
        }
    }
}

#[cfg(test)]
mod draft_tests {
    use super::{
        catalog, draft_keys, draft_storage_key, sanitized_draft, AppState, CatalogReviews,
        Database, OperationManager, MAX_DRAFT_IDENTITY_BYTES,
    };
    use crate::models::CatalogEntry;
    use std::{
        collections::{HashMap, HashSet},
        sync::{Arc, Mutex},
    };
    use tempfile::tempdir;

    fn keys(values: &[&str]) -> HashSet<String> {
        values.iter().map(|value| value.to_string()).collect()
    }

    fn draft_identity() -> CatalogEntry {
        catalog::entry_by_id("myworkid").unwrap()
    }

    #[test]
    fn draft_storage_keys_bind_the_complete_template_identity() {
        let identity = draft_identity();
        let key = draft_storage_key(&identity.id, &identity).unwrap();
        assert_eq!(key, draft_storage_key(&identity.id, &identity).unwrap());
        assert!(key.starts_with(&format!("v2:{}:", identity.id)));

        let mut changed_revision = identity.clone();
        changed_revision.commit = "different-revision".into();
        assert_ne!(
            key,
            draft_storage_key(&identity.id, &changed_revision).unwrap()
        );

        let mut changed_repository = identity.clone();
        changed_repository.repository = "https://example.invalid/other-template".into();
        assert_ne!(
            key,
            draft_storage_key(&identity.id, &changed_repository).unwrap()
        );

        let mut changed_subdirectory = identity.clone();
        changed_subdirectory.subdirectory = Some("other-subdirectory".into());
        assert_ne!(
            key,
            draft_storage_key(&identity.id, &changed_subdirectory).unwrap()
        );

        let mut changed_manifest = identity.clone();
        let manifest = changed_manifest.manifest.as_mut().unwrap();
        manifest.requires_interactive_terminal = !manifest.requires_interactive_terminal;
        assert_ne!(
            key,
            draft_storage_key(&identity.id, &changed_manifest).unwrap()
        );
    }

    #[test]
    fn draft_storage_key_rejects_mismatched_or_oversized_identity() {
        let identity = draft_identity();
        assert!(draft_storage_key("different-template", &identity).is_err());

        let mut oversized = identity.clone();
        oversized.summary = "x".repeat(MAX_DRAFT_IDENTITY_BYTES + 1);
        assert!(draft_storage_key(&oversized.id, &oversized).is_err());
    }

    #[test]
    fn legacy_template_id_drafts_are_safe_misses_without_deletion() {
        let directory = tempdir().unwrap();
        let database = Database::open(&directory.path().join("state.sqlite")).unwrap();
        let identity = draft_identity();
        let legacy = serde_json::json!({"location": "westus2"});
        database.save_draft(&identity.id, &legacy).unwrap();

        let storage_key = draft_storage_key(&identity.id, &identity).unwrap();
        assert_ne!(storage_key, identity.id);
        assert!(database.load_draft(&storage_key).unwrap().is_none());
        assert_eq!(database.load_draft(&identity.id).unwrap(), Some(legacy));
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
    fn optional_null_defaults_do_not_prevent_saving_other_draft_values() {
        let sanitized = sanitized_draft(
            serde_json::json!({
                "projectPath": "C:\\Deployments\\draft-only",
                "customDomains": null,
                "managedCertificate": false
            }),
            &keys(&["projectpath", "customdomains", "managedcertificate"]),
            &HashSet::new(),
        )
        .unwrap();

        assert_eq!(
            sanitized,
            serde_json::json!({
                "projectPath": "C:\\Deployments\\draft-only",
                "managedCertificate": false
            })
        );
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
            environment_writes: tokio::sync::Mutex::new(()),
            template_cache: directory.path().join("cache"),
            inspected_manifests: Mutex::new(HashMap::new()),
            reviewed_contracts: Mutex::new(HashMap::new()),
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

#[cfg(test)]
mod acknowledgement_caller_tests {
    use super::*;
    use crate::acknowledgements::tests::{fixture_manifest, fixture_value, proof, values, TENANT};
    use serde_json::json;

    fn fixture() -> (
        tempfile::TempDir,
        AppState,
        ProjectTemplateSnapshot,
        ExecutionApprovalRequest,
    ) {
        let root = tempfile::tempdir().unwrap();
        std::fs::write(
            root.path().join("azure.yaml"),
            "name: acknowledgement-test\n",
        )
        .unwrap();
        write_environment(root.path(), "all", "true");
        let state = AppState {
            database: Arc::new(Database::open(&root.path().join("state.sqlite")).unwrap()),
            operations: OperationManager::default(),
            environment_writes: AsyncMutex::new(()),
            template_cache: root.path().join("cache"),
            inspected_manifests: Mutex::new(HashMap::new()),
            reviewed_contracts: Mutex::new(HashMap::new()),
            catalog_reviews: CatalogReviews::default(),
            execution_approvals: execution_approval::ExecutionApprovals::default(),
            tool_install_approvals: tools::ToolInstallApprovals::default(),
        };
        let mut entry = catalog::entry_by_id("myworkid").unwrap();
        entry.manifest = Some(fixture_manifest());
        let snapshot = ProjectTemplateSnapshot::new(entry).unwrap();
        let project = ProjectSummary {
            id: "ack-test".into(),
            name: "Acknowledgement test".into(),
            path: root.path().display().to_string(),
            environment: Some("dev".into()),
            template_id: Some(snapshot.entry.id.clone()),
            revision: Some(snapshot.entry.commit.clone()),
            last_operation: None,
            state: "ready".into(),
        };
        state.database.upsert_project(&project).unwrap();
        state
            .database
            .save_project_template(&project, &snapshot)
            .unwrap();
        state
            .reviewed_contracts
            .lock()
            .unwrap()
            .insert(snapshot.digest.clone(), snapshot.clone());
        let request = ExecutionApprovalRequest {
            action: "preview".into(),
            project_path: project.path,
            environment: "dev".into(),
            template_id: snapshot.entry.id.clone(),
            revision: project.revision,
            snapshot_digest: Some(snapshot.digest.clone()),
            source_binding: None,
            target: Some(lifecycle_execution_target(root.path(), "dev").unwrap()),
            acknowledgement_proofs: vec![proof()],
            acknowledgement_values: values(),
        };
        (root, state, snapshot, request)
    }

    fn write_environment(path: &Path, mode: &str, enabled: &str) {
        let environment = path.join(".azure/dev");
        std::fs::create_dir_all(&environment).unwrap();
        std::fs::write(environment.join(".env"), format!("AZURE_TENANT_ID={TENANT}\nAZURE_SUBSCRIPTION_ID=22222222-2222-2222-2222-222222222222\nAZURE_LOCATION=westus2\nMODE={mode}\nENABLED={enabled}\nCONFIRM_ALL=true\n")).unwrap();
    }

    #[test]
    fn preparation_approval_independently_requires_proof_and_binds_submitted_values() {
        let (_root, state, _snapshot, mut request) = fixture();
        request.action = "prepare".into();
        let mut missing = request.clone();
        missing.acknowledgement_proofs.clear();
        assert!(prepare_approval_acknowledgements(&state, &mut missing).is_err());
        prepare_approval_acknowledgements(&state, &mut request).unwrap();
        let grant = state.execution_approvals.issue(request.clone()).unwrap();
        let mut changed = request.clone();
        changed
            .acknowledgement_values
            .insert("MODE".into(), json!("limited"));
        prepare_approval_acknowledgements(&state, &mut changed).unwrap();
        assert!(state
            .execution_approvals
            .consume(&grant.token, changed)
            .is_err());
        let grant = state.execution_approvals.issue(request.clone()).unwrap();
        state
            .execution_approvals
            .consume(&grant.token, request.clone())
            .unwrap();
        assert!(state
            .execution_approvals
            .consume(&grant.token, request)
            .is_err());
    }

    #[test]
    fn lifecycle_approval_reads_saved_values_and_saved_true_alone_never_approves() {
        let (root, state, snapshot, mut request) = fixture();
        request.acknowledgement_proofs.clear();
        request
            .acknowledgement_values
            .insert("MODE".into(), json!("limited"));
        assert!(lifecycle_approval_acknowledgements(&state.database, &mut request).is_err());
        request.acknowledgement_proofs = vec![proof()];
        lifecycle_approval_acknowledgements(&state.database, &mut request).unwrap();
        assert_eq!(request.acknowledgement_values, values());
        let grant = state.execution_approvals.issue(request.clone()).unwrap();
        write_environment(root.path(), "limited", "true");
        let mut expected = request.clone();
        expected.acknowledgement_values = acknowledgements::saved_binding_values(
            snapshot.entry.manifest.as_ref().unwrap(),
            root.path(),
            "dev",
        )
        .unwrap();
        expected.acknowledgement_proofs.clear();
        assert!(state
            .execution_approvals
            .consume_lifecycle(&grant.token, expected, snapshot.entry.manifest.as_ref())
            .is_err());
    }

    #[test]
    fn actual_lifecycle_consumption_rejects_forged_saved_true_without_proof_for_each_expansion_action(
    ) {
        let (_root, state, snapshot, request) = fixture();
        for action in ["preview", "up", "provision", "deploy"] {
            let mut forged = request.clone();
            forged.action = action.into();
            forged.acknowledgement_proofs.clear();
            // Even a mistakenly issued internal grant cannot bypass the actual
            // consumer used by start_azd_operation.
            let grant = state.execution_approvals.issue(forged.clone()).unwrap();
            assert!(
                state
                    .execution_approvals
                    .consume_lifecycle(&grant.token, forged, snapshot.entry.manifest.as_ref())
                    .is_err(),
                "{action}"
            );
        }
    }

    #[test]
    fn lifecycle_consumption_rejects_stale_tenant_and_group_controller() {
        let (_root, state, snapshot, request) = fixture();
        for change in ["tenant", "group"] {
            let grant = state.execution_approvals.issue(request.clone()).unwrap();
            let mut changed = request.clone();
            if change == "tenant" {
                changed.target.as_mut().unwrap().tenant_id =
                    Some("33333333-3333-3333-3333-333333333333".into());
            } else {
                changed
                    .acknowledgement_values
                    .insert("ENABLED".into(), json!(false));
            }
            assert!(state
                .execution_approvals
                .consume_lifecycle(&grant.token, changed, snapshot.entry.manifest.as_ref())
                .is_err());
        }
    }

    #[test]
    fn down_does_not_require_expansion_acknowledgement_and_gui_writes_invalidate_grants() {
        let (_root, state, snapshot, mut request) = fixture();
        request.action = "down".into();
        lifecycle_approval_acknowledgements(&state.database, &mut request).unwrap();
        assert!(request.acknowledgement_proofs.is_empty());
        assert!(request.acknowledgement_values.is_empty());
        let grant = state.execution_approvals.issue(request.clone()).unwrap();
        state
            .execution_approvals
            .consume_lifecycle(
                &grant.token,
                request.clone(),
                snapshot.entry.manifest.as_ref(),
            )
            .unwrap();
        let grant = state.execution_approvals.issue(request.clone()).unwrap();
        state.execution_approvals.clear().unwrap(); // Same invalidation used before GUI environment writes.
        assert!(state
            .execution_approvals
            .consume_lifecycle(&grant.token, request, snapshot.entry.manifest.as_ref())
            .is_err());
    }

    #[test]
    fn hidden_false_and_omitted_preparation_acknowledgements_have_identical_binding() {
        let (_root, state, _snapshot, mut request) = fixture();
        request.action = "prepare".into();
        request.acknowledgement_proofs.clear();
        request
            .acknowledgement_values
            .insert("MODE".into(), json!("limited"));
        request
            .acknowledgement_values
            .insert("CONFIRM_ALL".into(), json!(false));
        prepare_approval_acknowledgements(&state, &mut request).unwrap();
        let grant = state.execution_approvals.issue(request.clone()).unwrap();
        request.acknowledgement_values.remove("CONFIRM_ALL");
        prepare_approval_acknowledgements(&state, &mut request).unwrap();
        state
            .execution_approvals
            .consume(&grant.token, request)
            .unwrap();
    }

    #[test]
    fn native_drafts_never_save_or_restore_acknowledgements() {
        let (_root, state, snapshot, _request) = fixture();
        state
            .inspected_manifests
            .lock()
            .unwrap()
            .insert(snapshot.entry.id.clone(), fixture_manifest());
        let (allowed, blocked) = draft_keys(&state, &snapshot.entry.id).unwrap();
        let cleaned = sanitized_draft(
            json!({"mode":"all","confirm":true,"acknowledgementProofs":[proof()]}),
            &allowed,
            &blocked,
        )
        .unwrap();
        assert_eq!(cleaned, json!({"mode":"all"}));
    }

    #[tokio::test]
    async fn actual_preparation_revalidates_resolved_manifest_before_azd_or_destination_creation() {
        let root = tempfile::tempdir().unwrap();
        let source = root.path().join("source");
        std::fs::create_dir(&source).unwrap();
        std::fs::write(source.join("azure.yaml"), "name: acknowledgement-test\n").unwrap();
        std::fs::write(
            source.join("azd-gui.json"),
            serde_json::to_vec(&fixture_value()).unwrap(),
        )
        .unwrap();
        let inspected = catalog::inspect_local(source.to_str().unwrap()).unwrap();
        let snapshot = ProjectTemplateSnapshot::new(inspected.entry.clone()).unwrap();
        let destination = root.path().join("not-created");
        let request = PrepareDeploymentRequest {
            template_id: inspected.entry.id,
            source_mode: "localTemplate".into(),
            execution_approval_token: "cannot-bypass".into(),
            reviewed_contract_digest: snapshot.digest,
            source_path: Some(source.display().to_string()),
            source_revision: None,
            source_binding: inspected.source_binding,
            destination: destination.display().to_string(),
            environment: "dev".into(),
            tenant: Some(TENANT.into()),
            subscription: Some("22222222-2222-2222-2222-222222222222".into()),
            location: "westus2".into(),
            resource_group: None,
            environment_values: values(),
            acknowledgement_proofs: Vec::new(),
        };
        let database = Database::open(&root.path().join("state.sqlite")).unwrap();
        let error = deployment::prepare(request, &root.path().join("cache"), &database)
            .await
            .unwrap_err();
        assert!(
            error.to_string().contains("Confirm all users first"),
            "{error}"
        );
        assert!(!destination.exists());
        assert!(database
            .project_by_path(destination.to_str().unwrap())
            .unwrap()
            .is_none());
    }
    #[tokio::test]
    async fn actual_lifecycle_start_blocks_before_launcher_for_missing_or_stale_acknowledgement() {
        use std::sync::atomic::{AtomicUsize, Ordering};
        for action in ["preview", "up", "provision", "deploy"] {
            for scenario in ["missing", "false", "tenant", "controller", "group"] {
                let (root, state, _snapshot, mut request) = fixture();
                request.action = action.into();
                if scenario == "missing" {
                    request.acknowledgement_proofs.clear();
                }
                if scenario == "tenant" {
                    request.acknowledgement_proofs[0].tenant_id =
                        "33333333-3333-3333-3333-333333333333".into();
                }
                let grant = state.execution_approvals.issue(request.clone()).unwrap();
                match scenario {
                    "false" => {
                        let file = root.path().join(".azure/dev/.env");
                        let text = std::fs::read_to_string(&file)
                            .unwrap()
                            .replace("CONFIRM_ALL=true", "CONFIRM_ALL=false");
                        std::fs::write(file, text).unwrap();
                    }
                    "controller" => write_environment(root.path(), "limited", "true"),
                    "group" => write_environment(root.path(), "all", "false"),
                    _ => {}
                }
                let launches = AtomicUsize::new(0);
                let result = start_azd_operation_checked(
                    &state,
                    action.into(),
                    request.project_path,
                    Some("dev".into()),
                    Some(grant.token),
                    |_, _, _| {
                        launches.fetch_add(1, Ordering::Relaxed);
                        Ok("must-not-launch".into())
                    },
                )
                .await;
                assert!(result.is_err(), "{action}: {scenario}");
                assert_eq!(launches.load(Ordering::Relaxed), 0, "{action}: {scenario}");
            }
        }
    }

    #[tokio::test]
    async fn actual_lifecycle_start_accepts_fresh_proof_once_and_hidden_fields_without_proof() {
        use std::sync::atomic::{AtomicUsize, Ordering};
        for hidden in [false, true] {
            let (root, state, _snapshot, mut request) = fixture();
            if hidden {
                write_environment(root.path(), "limited", "true");
                request.acknowledgement_proofs.clear();
            }
            lifecycle_approval_acknowledgements(&state.database, &mut request).unwrap();
            let grant = state.execution_approvals.issue(request.clone()).unwrap();
            let launches = AtomicUsize::new(0);
            let result = start_azd_operation_checked(
                &state,
                "preview".into(),
                request.project_path.clone(),
                Some("dev".into()),
                Some(grant.token.clone()),
                |_, _, _| {
                    launches.fetch_add(1, Ordering::Relaxed);
                    Ok("launched".into())
                },
            )
            .await;
            assert_eq!(result.unwrap(), "launched");
            let result = start_azd_operation_checked(
                &state,
                "preview".into(),
                request.project_path,
                Some("dev".into()),
                Some(grant.token),
                |_, _, _| {
                    launches.fetch_add(1, Ordering::Relaxed);
                    Ok("must-not-repeat".into())
                },
            )
            .await;
            assert!(result.is_err());
            assert_eq!(launches.load(Ordering::Relaxed), 1);
        }
    }

    #[tokio::test]
    async fn actual_existing_project_preparation_rejects_null_and_missing_controller_retention() {
        for has_default in [false, true] {
            for explicit_null in [false, true] {
                let root = tempfile::tempdir().unwrap();
                let source = root.path().join("source");
                std::fs::create_dir(&source).unwrap();
                std::fs::write(source.join("azure.yaml"), "name: acknowledgement-test\n").unwrap();
                let mut manifest = fixture_value();
                if !has_default {
                    manifest["configuration"]["groups"][0]["fields"][1]
                        .as_object_mut()
                        .unwrap()
                        .remove("default");
                }
                std::fs::write(
                    source.join("azd-gui.json"),
                    serde_json::to_vec(&manifest).unwrap(),
                )
                .unwrap();
                write_environment(&source, "all", "true");
                let inspected = catalog::inspect_local(source.to_str().unwrap()).unwrap();
                let snapshot = ProjectTemplateSnapshot::new(inspected.entry.clone()).unwrap();
                let mut submitted = values();
                submitted.remove("ENABLED");
                submitted.remove("CONFIRM_ALL");
                if explicit_null {
                    submitted.insert("ENABLED".into(), serde_json::Value::Null);
                }
                let request = PrepareDeploymentRequest {
                    template_id: inspected.entry.id,
                    source_mode: "existingProject".into(),
                    execution_approval_token: "cannot-bypass".into(),
                    reviewed_contract_digest: snapshot.digest,
                    source_path: Some(source.display().to_string()),
                    source_revision: None,
                    source_binding: inspected.source_binding,
                    destination: source.display().to_string(),
                    environment: "dev".into(),
                    tenant: Some(TENANT.into()),
                    subscription: Some("22222222-2222-2222-2222-222222222222".into()),
                    location: "westus2".into(),
                    resource_group: None,
                    environment_values: submitted,
                    acknowledgement_proofs: Vec::new(),
                };
                let database = Database::open(&root.path().join("state.sqlite")).unwrap();
                let error = deployment::prepare(request, &root.path().join("cache"), &database)
                    .await
                    .unwrap_err();
                assert!(
                    error
                        .to_string()
                        .contains("explicit acknowledgement controller value: ENABLED"),
                    "{error}"
                );
                assert!(std::fs::read_to_string(source.join(".azure/dev/.env"))
                    .unwrap()
                    .contains("ENABLED=true"));
            }
        }
    }
    #[tokio::test]
    async fn actual_lifecycle_start_rejects_unset_controller_without_default_before_launch() {
        use std::sync::atomic::{AtomicUsize, Ordering};
        let (root, state, mut snapshot, mut request) = fixture();
        snapshot
            .entry
            .manifest
            .as_mut()
            .unwrap()
            .configuration
            .groups[0]
            .fields[1]
            .default = None;
        snapshot = ProjectTemplateSnapshot::new(snapshot.entry).unwrap();
        let project = state
            .database
            .project_by_path(&request.project_path)
            .unwrap()
            .unwrap();
        state
            .database
            .save_project_template(&project, &snapshot)
            .unwrap();
        request.snapshot_digest = Some(snapshot.digest.clone());
        let file = root.path().join(".azure/dev/.env");
        let content = std::fs::read_to_string(&file)
            .unwrap()
            .replace("ENABLED=true\n", "");
        std::fs::write(file, content).unwrap();
        assert!(
            lifecycle_approval_acknowledgements(&state.database, &mut request.clone()).is_err()
        );
        let grant = state.execution_approvals.issue(request.clone()).unwrap();
        let launches = AtomicUsize::new(0);
        let error = start_azd_operation_checked(
            &state,
            "preview".into(),
            request.project_path,
            Some("dev".into()),
            Some(grant.token),
            |_, _, _| {
                launches.fetch_add(1, Ordering::Relaxed);
                Ok("must-not-launch".into())
            },
        )
        .await
        .unwrap_err();
        assert!(
            error
                .to_string()
                .contains("controller value is missing: ENABLED"),
            "{error}"
        );
        assert_eq!(launches.load(Ordering::Relaxed), 0);
    }
}
