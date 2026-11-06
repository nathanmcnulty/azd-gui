use crate::{
    deployment::{
        validate_environment_name, validate_location, validate_optional_guid,
        validate_resource_group,
    },
    error::{AppError, AppResult},
    local_source,
    models::{ExecutionApprovalGrant, ExecutionApprovalRequest, ExecutionTarget, TemplateManifest},
};
use std::{
    collections::HashMap,
    path::PathBuf,
    sync::Mutex,
    time::{Duration, Instant},
};
use uuid::Uuid;

const APPROVAL_LIFETIME: Duration = Duration::from_secs(5 * 60);

#[derive(Clone)]
struct PendingApproval {
    request: ExecutionApprovalRequest,
    expires_at: Instant,
}

#[derive(Default)]
pub struct ExecutionApprovals {
    pending: Mutex<HashMap<String, PendingApproval>>,
}

impl ExecutionApprovals {
    pub fn clear(&self) -> AppResult<()> {
        self.pending
            .lock()
            .map_err(|_| AppError::Invalid("Execution approval cache is unavailable.".into()))?
            .clear();
        Ok(())
    }

    pub fn validate_request(
        mut request: ExecutionApprovalRequest,
    ) -> AppResult<ExecutionApprovalRequest> {
        if !matches!(
            request.action.as_str(),
            "prepare" | "preview" | "up" | "provision" | "deploy" | "down"
        ) {
            return Err(AppError::Invalid(format!(
                "Unsupported approval action: {}",
                request.action
            )));
        }
        validate_environment_name(&request.environment)?;
        request.target = Some(validate_target(request.target.ok_or_else(|| {
            AppError::Invalid("Execution approval requires an Azure target review.".into())
        })?)?);
        if request.template_id.len() > 128 {
            return Err(AppError::Invalid("Template ID is too long.".into()));
        }
        if request.revision.as_ref().is_some_and(|revision| {
            revision != "local"
                && (revision.len() != 40
                    || !revision
                        .chars()
                        .all(|character| character.is_ascii_hexdigit()))
        }) {
            return Err(AppError::Invalid("Invalid template revision.".into()));
        }
        if request.snapshot_digest.as_ref().is_some_and(|digest| {
            digest.len() != 64
                || !digest
                    .chars()
                    .all(|character| character.is_ascii_hexdigit())
        }) {
            return Err(AppError::Invalid(
                "Invalid saved template contract digest.".into(),
            ));
        }
        if let Some(binding) = &request.source_binding {
            if request.action != "prepare" {
                return Err(AppError::Invalid(
                    "Local source binding is valid only for template preparation.".into(),
                ));
            }
            let source_path = PathBuf::from(&binding.source_path);
            local_source::verify_for_path(binding, &source_path)?;
        }

        let path = PathBuf::from(&request.project_path);
        if !path.is_absolute() {
            return Err(AppError::Invalid(
                "Execution approval requires an absolute project path.".into(),
            ));
        }
        if request.action != "prepare" {
            let canonical = std::fs::canonicalize(path)?;
            if !canonical.join("azure.yaml").is_file() {
                return Err(AppError::Invalid(format!(
                    "{} does not contain azure.yaml.",
                    canonical.display()
                )));
            }
            request.project_path = canonical.display().to_string();
        }
        Ok(request)
    }

    pub fn issue(&self, request: ExecutionApprovalRequest) -> AppResult<ExecutionApprovalGrant> {
        let request = Self::validate_request(request)?;
        let token = Uuid::new_v4().to_string();
        let mut pending = self
            .pending
            .lock()
            .map_err(|_| AppError::Invalid("Execution approval cache is unavailable.".into()))?;
        pending.retain(|_, approval| approval.expires_at > Instant::now());
        pending.insert(
            token.clone(),
            PendingApproval {
                request: request.clone(),
                expires_at: Instant::now() + APPROVAL_LIFETIME,
            },
        );
        Ok(ExecutionApprovalGrant {
            token,
            project_path: request.project_path,
        })
    }

    fn take(&self, token: &str) -> AppResult<ExecutionApprovalRequest> {
        let approval = self.pending.lock()
            .map_err(|_| AppError::Invalid("Execution approval cache is unavailable.".into()))?
            .remove(token)
            .filter(|approval| approval.expires_at > Instant::now())
            .ok_or_else(|| AppError::Invalid("Execution approval is missing, expired, or already used. Confirm the action again.".into()))?;
        Ok(approval.request)
    }

    pub fn consume(&self, token: &str, expected: ExecutionApprovalRequest) -> AppResult<()> {
        let expected = Self::validate_request(expected)?;
        let approved = self.take(token)?;
        Self::compare(&approved, &expected)
    }

    /// The caller supplies newly read environment values, never renderer state.
    /// Proofs stay inside the one-time native approval and are revalidated before
    /// any lifecycle process can start.
    pub fn consume_lifecycle(
        &self,
        token: &str,
        mut expected: ExecutionApprovalRequest,
        manifest: Option<&TemplateManifest>,
    ) -> AppResult<()> {
        let approved = self.take(token)?;
        if expected.action != "down" {
            if let Some(manifest) = manifest {
                crate::acknowledgements::validate(
                    manifest,
                    &expected.acknowledgement_values,
                    expected
                        .target
                        .as_ref()
                        .and_then(|target| target.tenant_id.as_deref()),
                    &approved.acknowledgement_proofs,
                )?;
            }
            expected.acknowledgement_proofs = approved.acknowledgement_proofs.clone();
        }
        let expected = Self::validate_request(expected)?;
        Self::compare(&approved, &expected)
    }

    fn compare(
        approved: &ExecutionApprovalRequest,
        expected: &ExecutionApprovalRequest,
    ) -> AppResult<()> {
        if approved != expected {
            return Err(AppError::Invalid(
                "Execution approval does not match this action. Confirm the action again.".into(),
            ));
        }
        Ok(())
    }
}

pub fn validate_target(mut target: ExecutionTarget) -> AppResult<ExecutionTarget> {
    for (value, label) in [
        (&mut target.tenant_id, "Tenant ID"),
        (&mut target.subscription_id, "Subscription ID"),
    ] {
        if value.as_deref().is_some_and(str::is_empty) {
            *value = None;
        }
        validate_optional_guid(label, value.as_deref())?;
        if let Some(raw) = value.as_deref() {
            *value = Some(Uuid::parse_str(raw).expect("validated UUID").to_string());
        }
    }
    if target.location.as_deref().is_some_and(str::is_empty) {
        target.location = None;
    }
    if let Some(location) = target.location.as_deref() {
        validate_location(location)?;
        target.location = Some(location.to_ascii_lowercase());
    }
    if target.resource_group.as_deref().is_some_and(str::is_empty) {
        target.resource_group = None;
    }
    validate_resource_group(target.resource_group.as_deref())?;
    Ok(target)
}

#[cfg(test)]
mod tests {
    use super::ExecutionApprovals;
    use crate::models::{ExecutionApprovalRequest, ExecutionTarget};
    use tempfile::tempdir;

    fn request(project_path: String, action: &str) -> ExecutionApprovalRequest {
        ExecutionApprovalRequest {
            action: action.into(),
            project_path,
            environment: "dev".into(),
            template_id: "sample".into(),
            revision: Some("0123456789abcdef0123456789abcdef01234567".into()),
            snapshot_digest: Some("a".repeat(64)),
            acknowledgement_proofs: Vec::new(),
            acknowledgement_values: Default::default(),
            source_binding: None,
            target: Some(ExecutionTarget {
                tenant_id: Some("11111111-1111-1111-1111-111111111111".into()),
                subscription_id: Some("22222222-2222-2222-2222-222222222222".into()),
                location: Some("westus2".into()),
                resource_group: Some("rg-sample-dev".into()),
            }),
        }
    }

    #[test]
    fn approval_is_bound_to_the_action_and_consumed_once() {
        let directory = tempdir().unwrap();
        std::fs::write(directory.path().join("azure.yaml"), "name: sample").unwrap();
        let approvals = ExecutionApprovals::default();
        let preview = request(directory.path().display().to_string(), "preview");
        let grant = approvals.issue(preview.clone()).unwrap();

        let mut deploy = preview.clone();
        deploy.action = "up".into();
        assert!(approvals.consume(&grant.token, deploy).is_err());

        let grant = approvals.issue(preview.clone()).unwrap();
        approvals.consume(&grant.token, preview.clone()).unwrap();
        assert!(approvals.consume(&grant.token, preview).is_err());
    }

    #[test]
    fn local_source_changes_invalidate_a_pending_preparation_approval() {
        let source = tempdir().unwrap();
        std::fs::write(source.path().join("azure.yaml"), "name: sample").unwrap();
        let binding = crate::local_source::inspect(source.path()).unwrap();
        let approvals = ExecutionApprovals::default();
        let request = ExecutionApprovalRequest {
            action: "prepare".into(),
            project_path: source.path().display().to_string(),
            environment: "dev".into(),
            template_id: "local-sample".into(),
            revision: None,
            snapshot_digest: None,
            acknowledgement_proofs: Vec::new(),
            acknowledgement_values: Default::default(),
            source_binding: Some(binding),
            target: Some(ExecutionTarget {
                tenant_id: None,
                subscription_id: None,
                location: Some("westus2".into()),
                resource_group: None,
            }),
        };
        let grant = approvals.issue(request.clone()).unwrap();
        std::fs::write(source.path().join("azure.yaml"), "name: changed").unwrap();

        assert!(approvals.consume(&grant.token, request).is_err());
    }

    #[test]
    fn saved_contract_digest_changes_invalidate_a_pending_lifecycle_approval() {
        let directory = tempdir().unwrap();
        std::fs::write(directory.path().join("azure.yaml"), "name: sample").unwrap();
        let approvals = ExecutionApprovals::default();
        let request = request(directory.path().display().to_string(), "preview");
        let grant = approvals.issue(request.clone()).unwrap();
        let mut replaced = request;
        replaced.snapshot_digest = Some("b".repeat(64));

        assert!(approvals.consume(&grant.token, replaced).is_err());
    }

    #[test]
    fn changed_azure_target_invalidates_a_pending_lifecycle_approval() {
        let directory = tempdir().unwrap();
        std::fs::write(directory.path().join("azure.yaml"), "name: sample").unwrap();
        let approvals = ExecutionApprovals::default();
        let request = request(directory.path().display().to_string(), "down");
        let grant = approvals.issue(request.clone()).unwrap();
        let mut retargeted = request;
        retargeted.target.as_mut().unwrap().subscription_id =
            Some("33333333-3333-3333-3333-333333333333".into());

        assert!(approvals.consume(&grant.token, retargeted).is_err());
    }
}
