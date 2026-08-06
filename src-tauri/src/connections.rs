use crate::{
    azd::{AzdClient, SystemAzdClient},
    command_runner::{capture, CommandSpec, Executable},
    error::{AppError, AppResult},
    models::ConnectionStatus,
};
use chrono::{DateTime, Utc};
use serde_json::Value;
use std::time::Duration;

fn status(
    id: &str,
    name: &str,
    reason: &str,
    value: Option<&Value>,
    account_keys: &[&str],
    tenant_keys: &[&str],
    subscription_keys: (&[&str], &[&str]),
) -> ConnectionStatus {
    let account = value.and_then(|value| {
        account_keys
            .iter()
            .find_map(|key| value.pointer(key).and_then(Value::as_str).map(Into::into))
    });
    let tenant_id = value.and_then(|value| {
        tenant_keys
            .iter()
            .find_map(|key| value.pointer(key).and_then(Value::as_str).map(Into::into))
    });
    let subscription_id = value.and_then(|value| {
        subscription_keys
            .0
            .iter()
            .find_map(|key| value.pointer(key).and_then(Value::as_str).map(Into::into))
    });
    let subscription_name = value.and_then(|value| {
        subscription_keys
            .1
            .iter()
            .find_map(|key| value.pointer(key).and_then(Value::as_str).map(Into::into))
    });
    ConnectionStatus {
        id: id.into(),
        name: name.into(),
        required: id == "azd",
        reason: reason.into(),
        state: if value.is_some() {
            "connected"
        } else {
            "disconnected"
        }
        .into(),
        scopes: vec![],
        granted_scopes: vec![],
        account,
        tenant_id,
        subscription_id,
        subscription_name,
        expires_on: None,
        detail: None,
    }
}

fn apply_expiration(status: &mut ConnectionStatus, value: Option<&Value>, now: DateTime<Utc>) {
    let Some(raw) = value
        .and_then(|value| value.pointer("/expiresOn"))
        .and_then(Value::as_str)
    else {
        return;
    };
    let Ok(expires_at) = DateTime::parse_from_rfc3339(raw) else {
        return;
    };
    status.expires_on = Some(raw.into());
    if expires_at.with_timezone(&Utc) <= now {
        status.state = "expired".into();
        status.detail =
            Some("The cached credential has expired. Reconnect before deployment.".into());
    }
}

fn granted_scopes(value: Option<&Value>) -> Vec<String> {
    value
        .and_then(|value| value.pointer("/scopes"))
        .map(|value| match value {
            Value::Array(values) => values
                .iter()
                .filter_map(Value::as_str)
                .map(Into::into)
                .collect(),
            Value::String(value) => vec![value.clone()],
            _ => vec![],
        })
        .unwrap_or_default()
}

async fn json_status(spec: CommandSpec) -> Option<Value> {
    let output = capture(spec, Duration::from_secs(15)).await.ok()?;
    if !output.success || output.stdout.trim().is_empty() {
        return None;
    }
    serde_json::from_str(&output.stdout).ok()
}

pub async fn check_all() -> Vec<ConnectionStatus> {
    let azd_client = SystemAzdClient;
    let (azd, az, az_ps, graph_ps) = tokio::join!(
        azd_client.auth_status(),
        json_status(CommandSpec::new(
            Executable::AzureCli,
            ["account", "show", "--output", "json"]
        )),
        json_status(CommandSpec::new(
            Executable::PowerShell,
            [
                "-NoLogo",
                "-NoProfile",
                "-NonInteractive",
                "-Command",
                "$c=Get-AzContext -ErrorAction SilentlyContinue; if($c){[pscustomobject]@{account=$c.Account.Id;tenantId=$c.Tenant.Id;subscriptionId=$c.Subscription.Id;subscriptionName=$c.Subscription.Name}|ConvertTo-Json -Compress}"
            ]
        )),
        json_status(CommandSpec::new(
            Executable::PowerShell,
            [
                "-NoLogo",
                "-NoProfile",
                "-NonInteractive",
                "-Command",
                "$c=Get-MgContext -ErrorAction SilentlyContinue; if($c){[pscustomobject]@{account=$c.Account;tenantId=$c.TenantId;scopes=$c.Scopes}|ConvertTo-Json -Compress}"
            ]
        ))
    );
    let azd_status_value = azd.ok();
    let mut azd_status = status(
        "azd",
        "Azure Developer CLI",
        "Provision and deploy Azure resources.",
        azd_status_value.as_ref(),
        &["/email", "/account/email", "/user/email"],
        &["/tenantId", "/tenant/id"],
        (&[], &[]),
    );
    apply_expiration(&mut azd_status, azd_status_value.as_ref(), Utc::now());
    let graph_granted_scopes = granted_scopes(graph_ps.as_ref());
    let mut graph_status = status(
        "graphPowerShell",
        "Microsoft Graph PowerShell",
        "Template-specific Microsoft Graph operations.",
        graph_ps.as_ref(),
        &["/account"],
        &["/tenantId"],
        (&[], &[]),
    );
    graph_status.granted_scopes = graph_granted_scopes;
    let mut statuses = vec![
        azd_status,
        status(
            "azureCli",
            "Azure CLI",
            "Template-specific Azure and Graph operations.",
            az.as_ref(),
            &["/user/name"],
            &["/tenantId"],
            (&["/id"], &["/name"]),
        ),
        status(
            "azPowerShell",
            "Azure PowerShell",
            "Template-specific PowerShell operations.",
            az_ps.as_ref(),
            &["/account"],
            &["/tenantId"],
            (&["/subscriptionId"], &["/subscriptionName"]),
        ),
        graph_status,
    ];
    if let Some(mismatches) = tenant_mismatch(&statuses) {
        let detail = format!(
            "Connected tools use different tenants: {}",
            mismatches
                .iter()
                .map(|(name, tenant)| format!("{name} ({tenant})"))
                .collect::<Vec<_>>()
                .join(", ")
        );
        for status in &mut statuses {
            if status.tenant_id.is_some() {
                status.detail = Some(detail.clone());
            }
        }
    }
    statuses
}

pub fn login_spec(
    provider_id: &str,
    scopes: &[String],
    device_code: bool,
) -> AppResult<CommandSpec> {
    if scopes.iter().any(|scope| {
        scope.len() > 512
            || !scope
                .chars()
                .all(|character| character.is_ascii_alphanumeric() || ":/._-".contains(character))
    }) {
        return Err(AppError::Invalid("Invalid authentication scope.".into()));
    }
    match provider_id {
        "azd" => {
            let mut args: Vec<String> = vec!["auth".into(), "login".into()];
            if device_code {
                args.push("--use-device-code".into());
            }
            Ok(CommandSpec::new(Executable::Azd, args))
        }
        "azureCli" => {
            let mut args: Vec<String> = vec!["login".into()];
            if device_code {
                args.push("--use-device-code".into());
            }
            Ok(CommandSpec::new(Executable::AzureCli, args))
        }
        "azPowerShell" => Ok(CommandSpec::new(
            Executable::PowerShell,
            [
                "-NoLogo",
                "-NoProfile",
                "-Command",
                if device_code {
                    "Connect-AzAccount -UseDeviceAuthentication"
                } else {
                    "Connect-AzAccount"
                },
            ],
        )),
        "graphPowerShell" => {
            let script = match (scopes.is_empty(), device_code) {
                (true, true) => "Connect-MgGraph -UseDeviceCode",
                (true, false) => "Connect-MgGraph",
                (false, true) => {
                    "Connect-MgGraph -Scopes ($env:AZD_GUI_GRAPH_SCOPES -split ',') -UseDeviceCode"
                }
                (false, false) => "Connect-MgGraph -Scopes ($env:AZD_GUI_GRAPH_SCOPES -split ',')",
            };
            let spec = CommandSpec::new(
                Executable::PowerShell,
                ["-NoLogo", "-NoProfile", "-Command", script],
            );
            Ok(if scopes.is_empty() {
                spec
            } else {
                spec.env("AZD_GUI_GRAPH_SCOPES", scopes.join(","))
            })
        }
        _ => Err(AppError::Invalid(format!(
            "Unsupported connection provider: {provider_id}"
        ))),
    }
}

pub fn tenant_mismatch(statuses: &[ConnectionStatus]) -> Option<Vec<(String, String)>> {
    let tenants: Vec<_> = statuses
        .iter()
        .filter_map(|status| {
            status
                .tenant_id
                .as_ref()
                .map(|tenant| (status.name.clone(), tenant.clone()))
        })
        .collect();
    let first = tenants.first()?.1.as_str();
    tenants
        .iter()
        .any(|(_, tenant)| !tenant.eq_ignore_ascii_case(first))
        .then_some(tenants)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn connected(name: &str, tenant_id: &str) -> ConnectionStatus {
        ConnectionStatus {
            id: name.into(),
            name: name.into(),
            required: true,
            reason: String::new(),
            state: "connected".into(),
            scopes: vec![],
            granted_scopes: vec![],
            account: None,
            tenant_id: Some(tenant_id.into()),
            subscription_id: None,
            subscription_name: None,
            expires_on: None,
            detail: None,
        }
    }

    #[test]
    fn graph_status_preserves_granted_scopes() {
        let value = serde_json::json!({
            "scopes": ["Application.ReadWrite.All", "RoleManagement.ReadWrite.Directory"]
        });
        let scopes = granted_scopes(Some(&value));

        assert_eq!(scopes.len(), 2);
        assert_eq!(scopes[0], "Application.ReadWrite.All");
    }

    #[test]
    fn azd_expiration_marks_only_stale_credentials_as_expired() {
        let now = DateTime::parse_from_rfc3339("2026-08-06T12:00:00Z")
            .unwrap()
            .with_timezone(&Utc);
        let expired_value = serde_json::json!({"expiresOn": "2026-08-06T11:59:59Z"});
        let active_value = serde_json::json!({"expiresOn": "2026-08-06T12:00:01Z"});
        let mut expired = connected("azd", "tenant-a");
        let mut active = connected("azd", "tenant-a");

        apply_expiration(&mut expired, Some(&expired_value), now);
        apply_expiration(&mut active, Some(&active_value), now);

        assert_eq!(expired.state, "expired");
        assert_eq!(expired.expires_on.as_deref(), Some("2026-08-06T11:59:59Z"));
        assert!(expired.detail.as_deref().unwrap().contains("Reconnect"));
        assert_eq!(active.state, "connected");
        assert_eq!(active.expires_on.as_deref(), Some("2026-08-06T12:00:01Z"));
    }

    #[test]
    fn graph_login_without_template_scopes_omits_the_scopes_argument() {
        let interactive = login_spec("graphPowerShell", &[], false).unwrap();
        let device_code = login_spec("graphPowerShell", &[], true).unwrap();

        assert_eq!(interactive.args.last().unwrap(), "Connect-MgGraph");
        assert!(interactive.environment.is_empty());
        assert_eq!(
            device_code.args.last().unwrap(),
            "Connect-MgGraph -UseDeviceCode"
        );
        assert!(device_code.environment.is_empty());
    }

    #[test]
    fn graph_login_passes_manifest_scopes_through_a_private_environment_value() {
        let scopes = vec![
            "Application.ReadWrite.All".into(),
            "RoleManagement.ReadWrite.Directory".into(),
        ];
        let spec = login_spec("graphPowerShell", &scopes, false).unwrap();
        let device_code = login_spec("graphPowerShell", &scopes, true).unwrap();

        assert_eq!(
            spec.args.last().unwrap(),
            "Connect-MgGraph -Scopes ($env:AZD_GUI_GRAPH_SCOPES -split ',')"
        );
        assert_eq!(
            spec.environment,
            vec![(
                "AZD_GUI_GRAPH_SCOPES".into(),
                "Application.ReadWrite.All,RoleManagement.ReadWrite.Directory".into()
            )]
        );
        assert_eq!(
            device_code.args.last().unwrap(),
            "Connect-MgGraph -Scopes ($env:AZD_GUI_GRAPH_SCOPES -split ',') -UseDeviceCode"
        );
        assert_eq!(device_code.environment, spec.environment);
    }

    #[test]
    fn tenant_alignment_is_case_insensitive() {
        let statuses = vec![connected("azd", "ABC-123"), connected("az", "abc-123")];
        assert!(tenant_mismatch(&statuses).is_none());
    }

    #[test]
    fn tenant_mismatch_returns_each_connected_tenant() {
        let statuses = vec![connected("azd", "tenant-a"), connected("az", "tenant-b")];
        assert_eq!(tenant_mismatch(&statuses).unwrap().len(), 2);
    }
}
