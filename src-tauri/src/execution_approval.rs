use crate::{
    deployment::validate_environment_name,
    error::{AppError, AppResult},
    models::{ExecutionApprovalGrant, ExecutionApprovalRequest},
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

    pub fn consume(&self, token: &str, expected: ExecutionApprovalRequest) -> AppResult<()> {
        let expected = Self::validate_request(expected)?;
        let approval = self
            .pending
            .lock()
            .map_err(|_| AppError::Invalid("Execution approval cache is unavailable.".into()))?
            .remove(token)
            .ok_or_else(|| {
                AppError::Invalid(
                    "Execution approval is missing, expired, or already used. Confirm the action again."
                        .into(),
                )
            })?;
        if approval.expires_at <= Instant::now() || approval.request != expected {
            return Err(AppError::Invalid(
                "Execution approval does not match this action. Confirm the action again.".into(),
            ));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::ExecutionApprovals;
    use crate::models::ExecutionApprovalRequest;
    use tempfile::tempdir;

    fn request(project_path: String, action: &str) -> ExecutionApprovalRequest {
        ExecutionApprovalRequest {
            action: action.into(),
            project_path,
            environment: "dev".into(),
            template_id: "sample".into(),
            revision: Some("0123456789abcdef0123456789abcdef01234567".into()),
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
}
