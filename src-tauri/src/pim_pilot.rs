//! Project-bound selected-role pilot host. The pinned template owns deployment.
use crate::{
    error::{AppError, AppResult},
    graph_session::{GraphBinding, GraphWorker},
    pim_plan, AppState,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    io::Read,
    path::{Path, PathBuf},
};
use tauri::{AppHandle, State};
use tauri_plugin_dialog::{DialogExt, MessageDialogButtons, MessageDialogKind};

#[derive(Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Input {
    project_path: String,
    environment: String,
    input: pim_plan::Input,
}

impl Input {
    fn binding(&self, state: &AppState) -> AppResult<GraphBinding> {
        if self.environment.is_empty()
            || self.environment.len() > 64
            || !self
                .environment
                .bytes()
                .all(|c| c.is_ascii_alphanumeric() || b"_-".contains(&c))
        {
            return Err(AppError::Invalid(
                "Choose a named project environment (letters, digits, _ or -).".into(),
            ));
        }
        let mut binding = self.input.clone().review()?.binding;
        binding.purpose = Some("pimPilot".into());
        binding
            .scopes
            .retain(|scope| scope != "AuthenticationContext.Read.All");
        binding
            .scopes
            .push("AuthenticationContext.ReadWrite.All".into());
        binding.scopes.extend([
            "Policy.ReadWrite.ConditionalAccess".into(),
            "RoleManagementPolicy.ReadWrite.Directory".into(),
        ]);
        binding.scopes.sort();
        let project = state
            .database
            .project_by_path(&self.project_path)?
            .ok_or_else(|| AppError::Invalid("Register a pilot workspace first.".into()))?;
        let snapshot = state.database.project_template_exact(&project.path)?;
        binding.requirements_digest = format!(
            "{:x}",
            Sha256::digest(serde_json::to_vec(&(
                pim_plan::SOURCE,
                &binding,
                self,
                &project.id,
                &project.template_id,
                &project.revision,
                snapshot.map(|s| s.digest)
            ))?)
        );
        binding.validate()?;
        Ok(binding)
    }
    fn directory(&self, state: &AppState) -> AppResult<PathBuf> {
        let project = state
            .database
            .project_by_path(&self.project_path)?
            .ok_or_else(|| AppError::Invalid("Select a registered PIM project.".into()))?;
        let snapshot = state.database.project_template_exact(&project.path)?;
        if project.template_id.as_deref() != Some("pim-pilot")
            && !snapshot.is_some_and(|s| {
                s.entry
                    .repository
                    .trim_end_matches(".git")
                    .trim_end_matches('/')
                    == "https://github.com/nathanmcnulty/azd-pim"
            })
        {
            return Err(AppError::Invalid(
                "Select a PIM project or register a dedicated pilot workspace.".into(),
            ));
        }
        let root = Path::new(&project.path);
        let target = root.join(".azure").join(&self.environment);
        reject_aliases(&target)?;
        Ok(target)
    }
}

#[tauri::command]
pub fn register_pim_pilot(
    state: State<'_, AppState>,
    project_path: String,
) -> AppResult<crate::models::ProjectSummary> {
    let root = Path::new(&project_path);
    reject_aliases(root)?;
    let root = std::fs::canonicalize(root)?;
    if !root.is_dir() {
        return Err(AppError::Invalid(
            "Choose an existing pilot workspace directory.".into(),
        ));
    }
    let path = root.display().to_string();
    if let Some(project) = state.database.project_by_path(&path)? {
        return Ok(project);
    }
    let project = crate::models::ProjectSummary {
        id: uuid::Uuid::new_v4().to_string(),
        name: "Selected-role PIM pilot".into(),
        path,
        environment: Some("pilot".into()),
        template_id: Some("pim-pilot".into()),
        revision: Some(pim_plan::SOURCE.into()),
        last_operation: None,
        state: "attention".into(),
    };
    state.database.upsert_project(&project)?;
    Ok(project)
}

fn reject_aliases(target: &Path) -> AppResult<()> {
    for path in target.ancestors() {
        if let Ok(metadata) = std::fs::symlink_metadata(path) {
            #[cfg(windows)]
            {
                use std::os::windows::fs::MetadataExt;
                if metadata.file_attributes() & 0x400 != 0 {
                    return Err(AppError::Invalid(
                        "Pilot paths cannot contain reparse points.".into(),
                    ));
                }
            }
            if metadata.file_type().is_symlink() {
                return Err(AppError::Invalid(
                    "Pilot paths cannot contain aliases.".into(),
                ));
            }
        }
    }
    Ok(())
}

#[tauri::command]
pub fn load_pim_pilot(
    state: State<'_, AppState>,
    project_path: String,
    environment: String,
) -> AppResult<Option<serde_json::Value>> {
    if environment.is_empty()
        || environment.len() > 64
        || !environment
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || b"_-".contains(&c))
    {
        return Err(AppError::Invalid("Choose a valid environment.".into()));
    }
    let project = state
        .database
        .project_by_path(&project_path)?
        .ok_or_else(|| AppError::Invalid("Select a registered project.".into()))?;
    let path = Path::new(&project.path)
        .join(".azure")
        .join(environment)
        .join("azd-pim-gui-draft.json");
    reject_aliases(&path)?;
    if !path.exists() {
        return Ok(None);
    }
    if std::fs::metadata(&path)?.len() > 16 * 1024 * 1024 {
        return Err(AppError::Invalid("Saved pilot draft exceeds limit.".into()));
    }
    let mut bytes = Vec::new();
    std::fs::File::open(path)?
        .take(16 * 1024 * 1024 + 1)
        .read_to_end(&mut bytes)?;
    if bytes.len() > 16 * 1024 * 1024 {
        return Err(AppError::Invalid("Saved pilot draft exceeds limit.".into()));
    }
    let saved: serde_json::Value = serde_json::from_slice(&bytes)?;
    // Draft data is never returned as approval or live evidence.
    Ok(Some(
        serde_json::json!({"identity":saved.get("identity"),"configuration":saved.get("configuration")}),
    ))
}

#[tauri::command]
pub fn review_pim_pilot(state: State<'_, AppState>, input: Input) -> AppResult<GraphBinding> {
    let binding = input.binding(&state)?;
    input.directory(&state)?;
    Ok(binding)
}

#[tauri::command]
pub async fn connect_pim_pilot(
    app: AppHandle,
    state: State<'_, AppState>,
    input: Input,
    expected_digest: String,
) -> AppResult<serde_json::Value> {
    let binding = input.binding(&state)?;
    if binding.requirements_digest != expected_digest {
        return Err(AppError::Invalid("Pilot configuration changed.".into()));
    }
    let directory = input.directory(&state)?;
    let ticket = state.graph_sessions.begin(&binding)?;
    let approved = app.dialog().message(format!("Sign in for the selected-role pilot.\nAccount: {}\nTenant: {}\nProject: {}\nEnvironment: {}\nPermissions: {}\n\nThis session can write core PIM controls only after a separate application approval. Stop if new consent is requested; consent is not authorized by this workflow.", binding.account, binding.tenant_id, input.project_path, input.environment, binding.scopes.join(", ")))
        .title("Review pilot sign-in").kind(MessageDialogKind::Warning)
        .buttons(MessageDialogButtons::OkCancelCustom("Sign in and recheck".into(), "Cancel".into())).blocking_show();
    if !approved {
        state.graph_sessions.invalidate_ticket(ticket);
        return Err(AppError::Invalid("Pilot sign-in cancelled.".into()));
    }
    let _guard = state.operations.begin_preparation()?;
    let worker = GraphWorker::spawn(binding.clone()).await?;
    state.graph_sessions.install(ticket, worker.clone()).await?;
    let result = async {
        state.graph_sessions.ensure_current(ticket, &worker)?;
        worker.authenticate().await?;
        worker.probe().await?;
        let result = worker.pilot(false, serde_json::json!({"directory": directory, "configuration":input.input.configuration})).await?;
        state.graph_sessions.ensure_current(ticket, &worker)?;
        Ok(result)
    }.await;
    if result.is_err() {
        state.graph_sessions.invalidate_worker(&worker);
        worker.close().await;
    }
    result
}

#[tauri::command]
pub async fn run_pim_pilot(
    app: AppHandle,
    state: State<'_, AppState>,
    input: Input,
    expected_digest: String,
    approved_digest: Option<String>,
) -> AppResult<serde_json::Value> {
    let binding = input.binding(&state)?;
    if binding.requirements_digest != expected_digest {
        return Err(AppError::Invalid(
            "Pilot identity or configuration changed.".into(),
        ));
    }
    let directory = input.directory(&state)?;
    let worker = state.graph_sessions.pilot_worker(&expected_digest)?;
    let _guard = state.operations.begin_preparation()?;
    let parameters = serde_json::json!({"directory":directory,"configuration":input.input.configuration,"approvedDigest":approved_digest});
    if let Some(digest) = approved_digest {
        // The native host retrieves the exact worker review before prompting.
        let current = worker.pilot(false, parameters.clone()).await?;
        if current.get("digest").and_then(|d| d.as_str()) != Some(&digest) {
            return Err(AppError::Invalid(
                "Live state or ownership changed. Recheck and review the new proposal.".into(),
            ));
        }
        let approved = app.dialog().message(format!("Apply the reviewed selected-role PIM pilot?\nTenant: {}\nAccount: {}\nProject: {}\nEnvironment: {}\nReview: {}\n\nThis can create/adopt authentication contexts and enabled Conditional Access policies, replace selected role-context links and remove legacy activation MFA only where shown in the review. Existing objects are authorized only by this exact review. Cancellation can leave partial or uncertain changes; recheck before another approval.", binding.tenant_id, binding.account, input.project_path, input.environment, digest))
            .title("Approve PIM pilot application").kind(MessageDialogKind::Warning)
            .buttons(MessageDialogButtons::OkCancelCustom("Apply reviewed pilot".into(), "Cancel".into())).blocking_show();
        if !approved {
            state.graph_sessions.invalidate_worker(&worker);
            worker.close().await;
            return Err(AppError::Invalid(
                "Pilot application cancelled. Reconnect and review before applying.".into(),
            ));
        }
        worker.pilot(true, parameters).await
    } else {
        worker.pilot(false, parameters).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{models::ProjectSummary, persistence::Database};
    use std::{
        collections::HashMap,
        sync::{Arc, Mutex},
    };

    fn fixture() -> (tempfile::TempDir, AppState, Input) {
        let root = tempfile::tempdir().unwrap();
        let state = AppState {
            database: Arc::new(Database::open(&root.path().join("state.sqlite")).unwrap()),
            operations: Default::default(),
            environment_writes: tokio::sync::Mutex::new(()),
            template_cache: root.path().join("cache"),
            inspected_manifests: Mutex::new(HashMap::new()),
            reviewed_contracts: Mutex::new(HashMap::new()),
            catalog_reviews: Default::default(),
            execution_approvals: Default::default(),
            tool_install_approvals: Default::default(),
            graph_sessions: Default::default(),
        };
        let path = root.path().display().to_string();
        state
            .database
            .upsert_project(&ProjectSummary {
                id: "pilot".into(),
                name: "Pilot".into(),
                path: path.clone(),
                environment: Some("pilot".into()),
                template_id: Some("pim-pilot".into()),
                revision: Some(pim_plan::SOURCE.into()),
                last_operation: None,
                state: "attention".into(),
            })
            .unwrap();
        let input = serde_json::from_value(serde_json::json!({"projectPath":path,"environment":"pilot","input":{
            "reviewId":"99999999-9999-4999-8999-999999999999","account":"operator@example.test","tenantId":"11111111-1111-4111-8111-111111111111",
            "clientId":"22222222-2222-4222-8222-222222222222","configuration":{"privilegedRoleIds":["33333333-3333-4333-8333-333333333333"],"lessPrivilegedRoleIds":[],
            "privilegedAuthenticationProfile":"mfa","lessPrivilegedAuthenticationProfile":"mfa","privilegedDeviceRequirement":"none","lessPrivilegedDeviceRequirement":"none"}}})).unwrap();
        (root, state, input)
    }

    #[test]
    fn pilot_host_binds_project_environment_identity_configuration_and_registration() {
        let (_root, state, input) = fixture();
        let original = input.binding(&state).unwrap();
        assert_eq!(original.purpose.as_deref(), Some("pimPilot"));
        assert_eq!(original.scopes.len(), 5);
        assert!(input.directory(&state).is_ok());
        for change in ["environment", "tenant", "config"] {
            let mut changed = input.clone();
            match change {
                "environment" => changed.environment = "another".into(),
                "tenant" => changed.input.tenant_id = "aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa".into(),
                _ => changed.input.configuration.preview_existing_objects = true,
            }
            assert_ne!(
                original.requirements_digest,
                changed.binding(&state).unwrap().requirements_digest
            );
        }
        let mut project = state
            .database
            .project_by_path(&input.project_path)
            .unwrap()
            .unwrap();
        project.revision = Some("changed".into());
        state.database.upsert_project(&project).unwrap();
        assert_ne!(
            original.requirements_digest,
            input.binding(&state).unwrap().requirements_digest
        );
        project.template_id = Some("unrelated".into());
        state.database.upsert_project(&project).unwrap();
        assert!(input.directory(&state).is_err());
    }

    #[test]
    fn pilot_host_rejects_unregistered_alias_paths_and_environment_traversal() {
        let (root, state, input) = fixture();
        for environment in ["", "../pilot", "a/b", "a\\b", "pilot.", "C:", "a b"] {
            let mut changed = input.clone();
            changed.environment = environment.into();
            assert!(changed.binding(&state).is_err());
        }
        let mut changed = input.clone();
        changed.project_path = root.path().join("other").display().to_string();
        assert!(changed.binding(&state).is_err());
        assert!(changed.directory(&state).is_err());
        #[cfg(unix)]
        {
            std::os::unix::fs::symlink(root.path(), root.path().join("alias")).unwrap();
            assert!(reject_aliases(&root.path().join("alias/.azure/pilot")).is_err());
        }
    }
}
