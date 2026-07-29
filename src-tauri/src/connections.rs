use crate::{
    azd::{AzdClient, SystemAzdClient},
    command_runner::{capture, CommandSpec, Executable},
    error::{AppError, AppResult},
    models::ConnectionStatus,
};
use serde_json::Value;
use std::time::Duration;

fn status(
    id: &str,
    name: &str,
    reason: &str,
    value: Option<Value>,
    account_keys: &[&str],
    tenant_keys: &[&str],
) -> ConnectionStatus {
    let account = value.as_ref().and_then(|value| {
        account_keys
            .iter()
            .find_map(|key| value.pointer(key).and_then(Value::as_str).map(Into::into))
    });
    let tenant_id = value.as_ref().and_then(|value| {
        tenant_keys
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
        account,
        tenant_id,
        expires_on: None,
        detail: None,
    }
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
                "$c=Get-AzContext -ErrorAction SilentlyContinue; if($c){[pscustomobject]@{account=$c.Account.Id;tenantId=$c.Tenant.Id}|ConvertTo-Json -Compress}"
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
    let mut statuses = vec![
        status(
            "azd",
            "Azure Developer CLI",
            "Provision and deploy Azure resources.",
            azd.ok(),
            &["/email", "/account/email", "/user/email"],
            &["/tenantId", "/tenant/id"],
        ),
        status(
            "azureCli",
            "Azure CLI",
            "Template-specific Azure and Graph operations.",
            az,
            &["/user/name"],
            &["/tenantId"],
        ),
        status(
            "azPowerShell",
            "Azure PowerShell",
            "Template-specific PowerShell operations.",
            az_ps,
            &["/account"],
            &["/tenantId"],
        ),
        status(
            "graphPowerShell",
            "Microsoft Graph PowerShell",
            "Template-specific Microsoft Graph operations.",
            graph_ps,
            &["/account"],
            &["/tenantId"],
        ),
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
            let script = if device_code {
                "Connect-MgGraph -Scopes ($env:AZD_GUI_GRAPH_SCOPES -split ',') -UseDeviceCode"
            } else {
                "Connect-MgGraph -Scopes ($env:AZD_GUI_GRAPH_SCOPES -split ',')"
            };
            Ok(CommandSpec::new(
                Executable::PowerShell,
                ["-NoLogo", "-NoProfile", "-Command", script],
            )
            .env("AZD_GUI_GRAPH_SCOPES", scopes.join(",")))
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
        .any(|(_, tenant)| tenant != first)
        .then_some(tenants)
}
