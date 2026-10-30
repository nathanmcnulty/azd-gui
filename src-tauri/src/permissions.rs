use crate::{
    command_runner::{capture, CommandSpec, Executable},
    error::{AppError, AppResult},
    models::{PermissionCheck, PermissionCheckRequest, PermissionCheckResult},
};
use serde::Deserialize;
use std::{collections::HashSet, time::Duration};
use uuid::Uuid;

const TIMEOUT: Duration = Duration::from_secs(20);

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct AzureSubscription {
    subscription_id: String,
    tenant_id: String,
    state: String,
}

#[derive(Debug, Deserialize)]
struct GraphOrganizationEnvelope {
    value: Vec<GraphOrganization>,
}

#[derive(Debug, Deserialize)]
struct GraphOrganization {
    id: String,
}

fn registered_provider(id: &str) -> Option<&'static str> {
    match id {
        "azure-subscription-access" => Some("azure"),
        "graph-directory-read" | "graph-delegated-scopes" | "entra-directory-access" => {
            Some("graph")
        }
        _ => None,
    }
}

pub fn validate(checks: &[PermissionCheck]) -> AppResult<()> {
    if checks.len() > 32 {
        return Err(AppError::Invalid(
            "A template cannot request more than 32 permission checks.".into(),
        ));
    }
    let mut ids = HashSet::new();
    for check in checks {
        let provider = registered_provider(&check.id).ok_or_else(|| {
            AppError::Invalid(format!(
                "Unsupported permission-check provider ID: {}",
                check.id
            ))
        })?;
        if check.provider != provider {
            return Err(AppError::Invalid(format!(
                "Permission check {} must use the {provider} provider.",
                check.id
            )));
        }
        if !ids.insert(check.id.as_str()) {
            return Err(AppError::Invalid(format!(
                "Permission check {} is duplicated.",
                check.id
            )));
        }
    }
    Ok(())
}

fn result(
    check: &PermissionCheck,
    state: &str,
    detail: impl Into<String>,
) -> PermissionCheckResult {
    PermissionCheckResult {
        id: check.id.clone(),
        name: check.name.clone(),
        provider: check.provider.clone(),
        required: check.required,
        description: check.description.clone(),
        state: state.into(),
        detail: detail.into(),
        blocking: check.required && state == "failed",
    }
}

async fn json<T: for<'de> Deserialize<'de>>(spec: CommandSpec) -> Result<T, String> {
    let output = capture(spec, TIMEOUT)
        .await
        .map_err(|error| error.to_string())?;
    if !output.success {
        return Err(if output.stderr.trim().is_empty() {
            "The provider command did not succeed.".into()
        } else {
            output.stderr.trim().into()
        });
    }
    serde_json::from_str(&output.stdout)
        .map_err(|_| "The provider returned an unexpected response.".into())
}

fn azure_subscription_spec(subscription_id: &str) -> CommandSpec {
    let url = format!(
        "https://management.azure.com/subscriptions/{subscription_id}?api-version=2022-12-01"
    );
    CommandSpec::new(
        Executable::AzureCli,
        [
            "rest",
            "--method",
            "GET",
            "--url",
            &url,
            "--subscription",
            subscription_id,
            "--output",
            "json",
            "--only-show-errors",
        ],
    )
}

async fn azure_subscription(subscription_id: &str) -> Result<AzureSubscription, String> {
    json(azure_subscription_spec(subscription_id)).await
}

fn graph_organization_spec(subscription_id: &str) -> CommandSpec {
    CommandSpec::new(
        Executable::AzureCli,
        [
            "rest",
            "--method",
            "GET",
            "--url",
            "https://graph.microsoft.com/v1.0/organization?$select=id",
            "--subscription",
            subscription_id,
            "--output",
            "json",
            "--only-show-errors",
        ],
    )
}

async fn graph_organization(subscription_id: &str) -> Result<GraphOrganizationEnvelope, String> {
    json(graph_organization_spec(subscription_id)).await
}

fn evaluate_azure(
    check: &PermissionCheck,
    evidence: &Result<AzureSubscription, String>,
    tenant_id: &str,
    subscription_id: &str,
) -> PermissionCheckResult {
    match evidence {
        Ok(account)
            if account.subscription_id.eq_ignore_ascii_case(subscription_id)
                && account.tenant_id.eq_ignore_ascii_case(tenant_id)
                && account.state.eq_ignore_ascii_case("enabled") =>
        {
            result(
                check,
                "passed",
                "Azure CLI completed a live ARM read of the selected enabled subscription in the selected tenant.",
            )
        }
        Ok(account) if !account.tenant_id.eq_ignore_ascii_case(tenant_id) => result(
            check,
            "failed",
            "Azure CLI resolved the subscription in a different tenant.",
        ),
        Ok(_) => result(
            check,
            "failed",
            "Azure CLI did not resolve the selected subscription as enabled.",
        ),
        Err(error) => result(check, "failed", format!("Azure CLI check failed: {error}")),
    }
}

fn evaluate_directory(
    check: &PermissionCheck,
    evidence: &Result<GraphOrganizationEnvelope, String>,
    tenant_id: &str,
) -> PermissionCheckResult {
    match evidence {
        Ok(organization)
            if organization
                .value
                .iter()
                .any(|value| value.id.eq_ignore_ascii_case(tenant_id)) =>
        {
            if check.id == "entra-directory-access" {
                result(
                    check,
                    "warning",
                    "Baseline directory access is available. Write privileges cannot be verified safely without changing the directory.",
                )
            } else {
                result(
                    check,
                    "passed",
                    "Azure CLI can query Microsoft Graph for the selected tenant.",
                )
            }
        }
        Ok(_) => result(
            check,
            "failed",
            "Microsoft Graph did not return the selected tenant.",
        ),
        Err(error) => result(
            check,
            "failed",
            format!("Microsoft Graph directory check failed: {error}"),
        ),
    }
}

pub async fn run(request: PermissionCheckRequest) -> AppResult<Vec<PermissionCheckResult>> {
    validate(&request.checks)?;
    if request
        .checks
        .iter()
        .any(|check| check.id == "graph-delegated-scopes")
    {
        return Err(AppError::Invalid(
            "Graph scope checks require the owned process adapter and native requirement snapshot."
                .into(),
        ));
    }
    Uuid::parse_str(&request.tenant_id)
        .map_err(|_| AppError::Invalid("Tenant ID must be a GUID.".into()))?;
    Uuid::parse_str(&request.subscription_id)
        .map_err(|_| AppError::Invalid("Subscription ID must be a GUID.".into()))?;
    if request.required_graph_scopes.len() > 64
        || request.required_graph_scopes.iter().any(|scope| {
            scope.len() > 512
                || !scope.chars().all(|character| {
                    character.is_ascii_alphanumeric() || ":/._-".contains(character)
                })
        })
    {
        return Err(AppError::Invalid(
            "Manifest-requested Microsoft Graph scopes are invalid.".into(),
        ));
    }

    let needs_azure = request
        .checks
        .iter()
        .any(|check| check.id == "azure-subscription-access");
    let needs_directory = request
        .checks
        .iter()
        .any(|check| check.id == "graph-directory-read" || check.id == "entra-directory-access");
    let (azure, directory) = tokio::join!(
        async {
            if needs_azure {
                Some(azure_subscription(&request.subscription_id).await)
            } else {
                None
            }
        },
        async {
            if needs_directory {
                Some(graph_organization(&request.subscription_id).await)
            } else {
                None
            }
        }
    );

    Ok(request
        .checks
        .iter()
        .map(|check| match check.id.as_str() {
            "azure-subscription-access" => evaluate_azure(
                check,
                azure.as_ref().expect("required Azure evidence"),
                &request.tenant_id,
                &request.subscription_id,
            ),
            "graph-directory-read" | "entra-directory-access" => evaluate_directory(
                check,
                directory.as_ref().expect("required directory evidence"),
                &request.tenant_id,
            ),
            _ => unreachable!("validated permission check"),
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn check(id: &str, provider: &str) -> PermissionCheck {
        PermissionCheck {
            id: id.into(),
            name: id.into(),
            provider: provider.into(),
            required: true,
            description: String::new(),
            condition: None,
        }
    }

    #[test]
    fn rejects_unregistered_or_mismatched_providers() {
        assert!(validate(&[check("shell-command", "azure")]).is_err());
        assert!(validate(&[check("azure-subscription-access", "graph")]).is_err());
    }

    #[test]
    fn azure_tenant_mismatch_blocks_required_check() {
        let output = evaluate_azure(
            &check("azure-subscription-access", "azure"),
            &Ok(AzureSubscription {
                subscription_id: "11111111-1111-1111-1111-111111111111".into(),
                tenant_id: "22222222-2222-2222-2222-222222222222".into(),
                state: "Enabled".into(),
            }),
            "33333333-3333-3333-3333-333333333333",
            "11111111-1111-1111-1111-111111111111",
        );
        assert_eq!(output.state, "failed");
        assert!(output.blocking);
    }

    #[tokio::test]
    async fn legacy_scope_route_cannot_probe_an_unrelated_process() {
        let result = run(PermissionCheckRequest {
            tenant_id: "11111111-1111-4111-8111-111111111111".into(),
            subscription_id: "33333333-3333-4333-8333-333333333333".into(),
            checks: vec![check("graph-delegated-scopes", "graph")],
            required_graph_scopes: vec!["Directory.Read.All".into()],
        })
        .await;
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("owned process adapter"));
    }

    #[test]
    fn legacy_directory_write_check_is_an_honest_warning() {
        let output = evaluate_directory(
            &check("entra-directory-access", "graph"),
            &Ok(GraphOrganizationEnvelope {
                value: vec![GraphOrganization {
                    id: "22222222-2222-2222-2222-222222222222".into(),
                }],
            }),
            "22222222-2222-2222-2222-222222222222",
        );
        assert_eq!(output.state, "warning");
        assert!(!output.blocking);
    }

    #[test]
    fn azure_cli_permission_checks_target_the_selected_subscription() {
        let subscription = "11111111-1111-1111-1111-111111111111";
        for spec in [
            azure_subscription_spec(subscription),
            graph_organization_spec(subscription),
        ] {
            assert!(spec
                .args
                .windows(2)
                .any(|pair| pair == ["--subscription", subscription]));
        }
    }
}
