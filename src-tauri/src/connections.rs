use crate::{
    azd,
    command_runner::{capture, CommandSpec, Executable},
    deployment::validate_optional_guid,
    error::{AppError, AppResult},
    models::{ConnectionStatus, ConnectionTarget},
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
        wrong_active_account: false,
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

fn graph_context_spec() -> CommandSpec {
    CommandSpec::new(
        Executable::PowerShell,
        [
            "-NoLogo",
            "-NoProfile",
            "-NonInteractive",
            "-Command",
            "$c=Get-MgContext -ErrorAction SilentlyContinue; if(!$c){$p=Join-Path (Join-Path ([Environment]::GetFolderPath('UserProfile')) '.mg') 'mg.authrecord.json'; if(Test-Path -LiteralPath $p){try{$r=Get-Content -LiteralPath $p -Raw -ErrorAction Stop|ConvertFrom-Json; if($r.username -and $r.tenantId){$c=[pscustomobject]@{Account=$r.username;TenantId=$r.tenantId;Scopes=@()}}}catch{}}}; if($c){[pscustomobject]@{account=$c.Account;tenantId=$c.TenantId;scopes=@($c.Scopes)}|ConvertTo-Json -Compress}",
        ],
    )
}

fn azure_cli_context_spec(target: Option<&ConnectionTarget>) -> CommandSpec {
    let mut args = vec![
        "account".to_string(),
        "show".to_string(),
        "--output".to_string(),
        "json".to_string(),
    ];
    if let Some(target) = target {
        args.extend(["--subscription".into(), target.subscription_id.clone()]);
    }
    CommandSpec::new(Executable::AzureCli, args)
}

fn az_power_shell_context_spec(target: Option<&ConnectionTarget>) -> CommandSpec {
    let (script, target) = if let Some(target) = target {
        (
            "$c=Get-AzContext -ListAvailable -ErrorAction SilentlyContinue|Where-Object {$_.Tenant.Id -eq $env:AZD_GUI_TENANT_ID -and $_.Subscription.Id -eq $env:AZD_GUI_SUBSCRIPTION_ID}|Select-Object -First 1; if($c){[pscustomobject]@{account=$c.Account.Id;tenantId=$c.Tenant.Id;subscriptionId=$c.Subscription.Id;subscriptionName=$c.Subscription.Name}|ConvertTo-Json -Compress}",
            Some(target),
        )
    } else {
        (
            "$c=Get-AzContext -ErrorAction SilentlyContinue; if($c){[pscustomobject]@{account=$c.Account.Id;tenantId=$c.Tenant.Id;subscriptionId=$c.Subscription.Id;subscriptionName=$c.Subscription.Name}|ConvertTo-Json -Compress}",
            None,
        )
    };
    let spec = CommandSpec::new(
        Executable::PowerShell,
        [
            "-NoLogo",
            "-NoProfile",
            "-NonInteractive",
            "-Command",
            script,
        ],
    );
    if let Some(target) = target {
        spec.env("AZD_GUI_TENANT_ID", &target.tenant_id)
            .env("AZD_GUI_SUBSCRIPTION_ID", &target.subscription_id)
    } else {
        spec
    }
}

pub fn validate_target(target: Option<&ConnectionTarget>) -> AppResult<()> {
    let Some(target) = target else {
        return Ok(());
    };
    validate_optional_guid("Tenant ID", Some(&target.tenant_id))?;
    validate_optional_guid("Subscription ID", Some(&target.subscription_id))
}

fn azd_tenant_status_spec(target: &ConnectionTarget) -> AppResult<CommandSpec> {
    validate_target(Some(target))?;
    Ok(CommandSpec::new(
        Executable::Azd,
        [
            "auth",
            "login",
            "--check-status",
            "--tenant-id",
            &target.tenant_id,
            "--output",
            "json",
            "--no-prompt",
        ],
    ))
}

fn azd_connection_status(
    value: Option<&Value>,
    target: Option<&ConnectionTarget>,
    probe: Option<&Value>,
    now: DateTime<Utc>,
) -> ConnectionStatus {
    // auth status deliberately reports no tenant. check-status verifies the requested
    // tenant internally without returning a token or changing the default context.
    let authenticated = value.filter(|value| value["status"] == "authenticated");
    let mut result = status(
        "azd",
        "Azure Developer CLI",
        "Provision and deploy Azure resources.",
        authenticated,
        &["/email", "/account/email", "/user/email"],
        &[],
        (&[], &[]),
    );
    if value.is_none()
        || value.is_some_and(|value| {
            !matches!(
                value["status"].as_str(),
                Some("authenticated" | "unauthenticated")
            )
        })
    {
        result.state = "unknown".into();
    }
    apply_expiration(&mut result, authenticated, now);
    if let Some(target) = target {
        // Managed identity may ignore --tenant-id. Restrict this evidence to user
        // credentials; do not infer a tenant for other or unknown identity types.
        let fresh_probe = probe
            .filter(|probe| probe["status"] == "success")
            .filter(|probe| {
                probe["expiresOn"]
                    .as_str()
                    .and_then(|raw| DateTime::parse_from_rfc3339(raw).ok())
                    .is_some_and(|expires| expires.with_timezone(&Utc) > now)
            });
        let current_expiration = authenticated
            .and_then(|value| value["expiresOn"].as_str())
            .and_then(|raw| DateTime::parse_from_rfc3339(raw).ok())
            .is_some_and(|expires| expires.with_timezone(&Utc) > now);
        if result.state == "connected"
            && current_expiration
            && authenticated.is_some_and(|value| value["type"] == "user")
            && fresh_probe.is_some()
        {
            result.tenant_id = Some(target.tenant_id.clone());
        } else if result.state == "connected" {
            result.detail = Some("The selected tenant has not been verified for this azd credential. Check connections or reconnect with a user account.".into());
        }
    }
    result
}

pub async fn check_all(target: Option<&ConnectionTarget>) -> Vec<ConnectionStatus> {
    let (azd, azd_probe, az_current, az_target, az_ps_current, az_ps_target, graph_ps) = tokio::join!(
        azd::auth_status(),
        async {
            match target.and_then(|target| azd_tenant_status_spec(target).ok()) {
                Some(spec) => json_status(spec).await,
                None => None,
            }
        },
        json_status(azure_cli_context_spec(None)),
        async {
            if target.is_some() {
                json_status(azure_cli_context_spec(target)).await
            } else {
                None
            }
        },
        json_status(az_power_shell_context_spec(None)),
        async {
            if target.is_some() {
                json_status(az_power_shell_context_spec(target)).await
            } else {
                None
            }
        },
        json_status(graph_context_spec())
    );
    let azd_status_value = azd.ok();
    let azd_status = azd_connection_status(
        azd_status_value.as_ref(),
        target,
        azd_probe.as_ref(),
        Utc::now(),
    );
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
    let mut azure_cli_status = status(
        "azureCli",
        "Azure CLI",
        "Template-specific Azure and Graph operations.",
        az_current.as_ref(),
        &["/user/name"],
        &["/tenantId"],
        (&["/id"], &["/name"]),
    );
    let mut az_power_shell_status = status(
        "azPowerShell",
        "Azure PowerShell",
        "Template-specific PowerShell operations.",
        az_ps_current.as_ref(),
        &["/account"],
        &["/tenantId"],
        (&["/subscriptionId"], &["/subscriptionName"]),
    );
    if let Some(target) = target {
        apply_target_context(
            &mut azure_cli_status,
            az_target.as_ref(),
            target,
            &["/user/name"],
            &["/tenantId"],
            &["/id"],
            &["/name"],
        );
        apply_target_context(
            &mut az_power_shell_status,
            az_ps_target.as_ref(),
            target,
            &["/account"],
            &["/tenantId"],
            &["/subscriptionId"],
            &["/subscriptionName"],
        );
    }
    let mut statuses = vec![
        azd_status,
        azure_cli_status,
        az_power_shell_status,
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

fn apply_target_context(
    current: &mut ConnectionStatus,
    target_value: Option<&Value>,
    target: &ConnectionTarget,
    account_keys: &[&str],
    tenant_keys: &[&str],
    subscription_keys: &[&str],
    subscription_name_keys: &[&str],
) {
    if current.state != "connected" {
        return;
    }
    let Some(value) = target_value else {
        current.state = "unknown".into();
        current.detail = Some("The selected target could not be verified for the current account. Check connections or reconnect this provider.".into());
        return;
    };
    let target_status = status(
        &current.id,
        &current.name,
        &current.reason,
        Some(value),
        account_keys,
        tenant_keys,
        (subscription_keys, subscription_name_keys),
    );
    let accounts = current
        .account
        .as_deref()
        .zip(target_status.account.as_deref());
    if accounts.is_none() {
        current.state = "unknown".into();
        current.detail = Some("The selected target account could not be identified. Reconnect this provider, then check again.".into());
    } else if accounts.is_some_and(|(a, b)| !a.eq_ignore_ascii_case(b)) {
        current.state = "disconnected".into();
        current.wrong_active_account = true;
        current.detail = Some("Another account has the selected target. Reconnect this provider with the intended account.".into());
    } else if target_status
        .tenant_id
        .as_deref()
        .is_some_and(|id| id.eq_ignore_ascii_case(&target.tenant_id))
        && target_status
            .subscription_id
            .as_deref()
            .is_some_and(|id| id.eq_ignore_ascii_case(&target.subscription_id))
    {
        // A cached target entry cannot make a different default context current.
        // Template hooks can use the provider default without a target argument.
        if current
            .tenant_id
            .as_deref()
            .is_some_and(|id| id.eq_ignore_ascii_case(&target.tenant_id))
            && current
                .subscription_id
                .as_deref()
                .is_some_and(|id| id.eq_ignore_ascii_case(&target.subscription_id))
        {
            current.subscription_name = target_status.subscription_name;
        }
    } else {
        current.state = "unknown".into();
        current.detail = Some("The selected target could not be verified for the current account. Check connections or reconnect this provider.".into());
    }
}

pub fn login_spec(
    provider_id: &str,
    scopes: &[String],
    target: Option<&ConnectionTarget>,
) -> AppResult<CommandSpec> {
    if scopes.iter().any(|scope| {
        scope.len() > 512
            || !scope
                .chars()
                .all(|character| character.is_ascii_alphanumeric() || ":/._-".contains(character))
    }) {
        return Err(AppError::Invalid("Invalid authentication scope.".into()));
    }
    validate_target(target)?;
    let tenant_id = target.map(|target| target.tenant_id.as_str());
    match provider_id {
        "azd" => {
            let mut args: Vec<String> = vec!["auth".into(), "login".into()];
            if let Some(tenant_id) = tenant_id {
                args.extend(["--tenant-id".into(), tenant_id.into()]);
            }
            Ok(CommandSpec::new(Executable::Azd, args))
        }
        "azureCli" => {
            let mut args: Vec<String> = vec!["login".into()];
            if let Some(tenant_id) = tenant_id {
                args.extend(["--tenant".into(), tenant_id.into()]);
            }
            Ok(CommandSpec::new(Executable::AzureCli, args))
        }
        "azPowerShell" => {
            let mut script = "Connect-AzAccount -Scope CurrentUser".to_string();
            if target.is_some() {
                script.push_str(
                    " -Tenant $env:AZD_GUI_TENANT_ID -Subscription $env:AZD_GUI_SUBSCRIPTION_ID",
                );
            }
            let spec = CommandSpec::new(
                Executable::PowerShell,
                ["-NoLogo", "-NoProfile", "-Command", &script],
            );
            Ok(if let Some(target) = target {
                spec.env("AZD_GUI_TENANT_ID", &target.tenant_id)
                    .env("AZD_GUI_SUBSCRIPTION_ID", &target.subscription_id)
            } else {
                spec
            })
        }
        "graphPowerShell" => {
            let mut script = "Connect-MgGraph -ContextScope CurrentUser".to_string();
            if tenant_id.is_some() {
                script.push_str(" -TenantId $env:AZD_GUI_TENANT_ID");
            }
            if !scopes.is_empty() {
                script.push_str(" -Scopes ($env:AZD_GUI_GRAPH_SCOPES -split ',')");
            }
            let mut spec = CommandSpec::new(
                Executable::PowerShell,
                ["-NoLogo", "-NoProfile", "-Command", &script],
            );
            if let Some(tenant_id) = tenant_id {
                spec = spec.env("AZD_GUI_TENANT_ID", tenant_id);
            }
            if !scopes.is_empty() {
                spec = spec.env("AZD_GUI_GRAPH_SCOPES", scopes.join(","));
            }
            Ok(spec)
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

    #[test]
    fn azd_tenant_probe_checks_without_login_or_context_mutation() {
        let target = ConnectionTarget {
            tenant_id: "11111111-1111-1111-1111-111111111111".into(),
            subscription_id: "22222222-2222-2222-2222-222222222222".into(),
        };
        let spec = azd_tenant_status_spec(&target).unwrap();
        assert_eq!(
            spec.args,
            [
                "auth",
                "login",
                "--check-status",
                "--tenant-id",
                &target.tenant_id,
                "--output",
                "json",
                "--no-prompt"
            ]
        );
        assert!(spec.environment.is_empty());
        let invalid = ConnectionTarget {
            tenant_id: "invalid".into(),
            ..target
        };
        assert!(azd_tenant_status_spec(&invalid).is_err());
    }

    #[test]
    fn azd_target_requires_successful_fresh_user_evidence() {
        let now = DateTime::parse_from_rfc3339("2026-09-20T12:00:00Z")
            .unwrap()
            .with_timezone(&Utc);
        let target = ConnectionTarget {
            tenant_id: "11111111-1111-1111-1111-111111111111".into(),
            subscription_id: "22222222-2222-2222-2222-222222222222".into(),
        };
        let value = serde_json::json!({"status":"authenticated", "type":"user", "email":"admin@example.test", "expiresOn":"2026-09-20T13:00:00Z"});
        let probe = serde_json::json!({"status":"success", "expiresOn":"2026-09-20T13:00:00Z"});
        let result = azd_connection_status(Some(&value), Some(&target), Some(&probe), now);
        assert_eq!(result.tenant_id.as_deref(), Some(target.tenant_id.as_str()));
        assert_eq!(result.account.as_deref(), Some("admin@example.test"));
        // The CLI returns exit zero even when check-status reports unauthenticated.
        for probe in [
            serde_json::json!({"status":"unauthenticated"}),
            serde_json::json!({"status":"success"}),
            serde_json::json!({"status":"success","expiresOn":"invalid"}),
            serde_json::json!({"status":"success","expiresOn":"2026-09-20T11:00:00Z"}),
            serde_json::json!({"status":"unexpected","expiresOn":"2026-09-20T13:00:00Z"}),
        ] {
            assert!(
                azd_connection_status(Some(&value), Some(&target), Some(&probe), now)
                    .tenant_id
                    .is_none()
            );
        }
        for status in ["unauthenticated", "unexpected"] {
            let mut rejected = value.clone();
            rejected["status"] = status.into();
            let result = azd_connection_status(Some(&rejected), Some(&target), Some(&probe), now);
            assert_ne!(result.state, "connected");
            assert!(result.tenant_id.is_none());
        }
        for kind in ["servicePrincipal", "unknown"] {
            let mut rejected = value.clone();
            rejected["type"] = kind.into();
            assert!(
                azd_connection_status(Some(&rejected), Some(&target), Some(&probe), now)
                    .tenant_id
                    .is_none()
            );
        }
        let mut expired = value.clone();
        expired["expiresOn"] = "2026-09-20T11:00:00Z".into();
        assert_eq!(
            azd_connection_status(Some(&expired), Some(&target), Some(&probe), now).state,
            "expired"
        );
        assert!(
            azd_connection_status(Some(&value), Some(&target), None, now)
                .tenant_id
                .is_none()
        );
        assert!(azd_connection_status(Some(&value), None, None, now)
            .tenant_id
            .is_none());
    }

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
            wrong_active_account: false,
        }
    }

    #[test]
    fn target_probe_cannot_replace_the_effective_account() {
        let target = ConnectionTarget {
            tenant_id: "11111111-1111-1111-1111-111111111111".into(),
            subscription_id: "22222222-2222-2222-2222-222222222222".into(),
        };
        let mut current = status(
            "azureCli",
            "Azure CLI",
            "",
            Some(&serde_json::json!({
                "user": {"name": "current@example.test"}, "tenantId": "other", "id": "other"
            })),
            &["/user/name"],
            &["/tenantId"],
            (&["/id"], &["/name"]),
        );
        let mut scoped = serde_json::json!({
            "user": {"name": "cached@example.test"}, "tenantId": target.tenant_id, "id": target.subscription_id
        });
        apply_target_context(
            &mut current,
            Some(&scoped),
            &target,
            &["/user/name"],
            &["/tenantId"],
            &["/id"],
            &["/name"],
        );
        assert_eq!(current.account.as_deref(), Some("current@example.test"));
        assert_eq!(current.state, "disconnected");
        assert!(current.wrong_active_account);

        scoped["user"]["name"] = "current@example.test".into();
        apply_target_context(
            &mut current,
            Some(&scoped),
            &target,
            &["/user/name"],
            &["/tenantId"],
            &["/id"],
            &["/name"],
        );
        assert_eq!(current.state, "disconnected");

        let mut fresh = status(
            "azureCli",
            "Azure CLI",
            "",
            Some(&serde_json::json!({
                "user": {"name": "current@example.test"}, "tenantId": target.tenant_id, "id": target.subscription_id
            })),
            &["/user/name"],
            &["/tenantId"],
            (&["/id"], &["/name"]),
        );
        apply_target_context(
            &mut fresh,
            Some(&scoped),
            &target,
            &["/user/name"],
            &["/tenantId"],
            &["/id"],
            &["/name"],
        );
        assert_eq!(fresh.state, "connected");
        assert_eq!(
            fresh.subscription_id.as_deref(),
            Some(target.subscription_id.as_str())
        );
        apply_target_context(
            &mut fresh,
            None,
            &target,
            &["/user/name"],
            &["/tenantId"],
            &["/id"],
            &["/name"],
        );
        assert_eq!(fresh.state, "unknown");

        let mut wrong_target = status(
            "azureCli",
            "Azure CLI",
            "",
            Some(&serde_json::json!({
                "user": {"name": "current@example.test"}, "tenantId": "other", "id": "other"
            })),
            &["/user/name"],
            &["/tenantId"],
            (&["/id"], &["/name"]),
        );
        apply_target_context(
            &mut wrong_target,
            Some(&scoped),
            &target,
            &["/user/name"],
            &["/tenantId"],
            &["/id"],
            &["/name"],
        );
        assert_eq!(wrong_target.state, "connected");
        assert_eq!(wrong_target.subscription_id.as_deref(), Some("other"));

        let mut no_account = status(
            "azureCli",
            "Azure CLI",
            "",
            Some(&serde_json::json!({
                "tenantId": "other", "id": "other"
            })),
            &["/user/name"],
            &["/tenantId"],
            (&["/id"], &["/name"]),
        );
        apply_target_context(
            &mut no_account,
            Some(&scoped),
            &target,
            &["/user/name"],
            &["/tenantId"],
            &["/id"],
            &["/name"],
        );
        assert_eq!(no_account.state, "unknown");
        assert!(!no_account.wrong_active_account);
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
    fn graph_status_falls_back_to_the_sdk_current_user_auth_record() {
        let spec = graph_context_spec();
        let script = spec.args.last().unwrap();

        assert!(script.contains("Get-MgContext"));
        assert!(script.contains("mg.authrecord.json"));
        assert!(script.contains("GetFolderPath('UserProfile')"));
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
        let interactive = login_spec("graphPowerShell", &[], None).unwrap();

        assert_eq!(
            interactive.args.last().unwrap(),
            "Connect-MgGraph -ContextScope CurrentUser"
        );
        assert!(interactive.environment.is_empty());
    }

    #[test]
    fn graph_login_passes_manifest_scopes_through_a_private_environment_value() {
        let scopes = vec![
            "Application.ReadWrite.All".into(),
            "RoleManagement.ReadWrite.Directory".into(),
        ];
        let spec = login_spec("graphPowerShell", &scopes, None).unwrap();

        assert_eq!(
            spec.args.last().unwrap(),
            "Connect-MgGraph -ContextScope CurrentUser -Scopes ($env:AZD_GUI_GRAPH_SCOPES -split ',')"
        );
        assert_eq!(
            spec.environment,
            vec![(
                "AZD_GUI_GRAPH_SCOPES".into(),
                "Application.ReadWrite.All,RoleManagement.ReadWrite.Directory".into()
            )]
        );
    }

    #[test]
    fn targets_a_validated_tenant_for_each_provider() {
        let tenant = "11111111-1111-1111-1111-111111111111";
        let subscription = "22222222-2222-2222-2222-222222222222";
        let target = ConnectionTarget {
            tenant_id: tenant.into(),
            subscription_id: subscription.into(),
        };

        let azd = login_spec("azd", &[], Some(&target)).unwrap();
        assert_eq!(azd.args, ["auth", "login", "--tenant-id", tenant]);

        let azure_cli = login_spec("azureCli", &[], Some(&target)).unwrap();
        assert_eq!(azure_cli.args, ["login", "--tenant", tenant]);

        let az_power_shell = login_spec("azPowerShell", &[], Some(&target)).unwrap();
        assert_eq!(
            az_power_shell.args.last().unwrap(),
            "Connect-AzAccount -Scope CurrentUser -Tenant $env:AZD_GUI_TENANT_ID -Subscription $env:AZD_GUI_SUBSCRIPTION_ID"
        );
        assert_eq!(
            az_power_shell.environment,
            vec![
                ("AZD_GUI_TENANT_ID".into(), tenant.into()),
                ("AZD_GUI_SUBSCRIPTION_ID".into(), subscription.into())
            ]
        );
        assert!(!az_power_shell.args.last().unwrap().contains(tenant));

        let graph = login_spec("graphPowerShell", &[], Some(&target)).unwrap();
        assert_eq!(
            graph.args.last().unwrap(),
            "Connect-MgGraph -ContextScope CurrentUser -TenantId $env:AZD_GUI_TENANT_ID"
        );
        assert_eq!(
            graph.environment,
            vec![("AZD_GUI_TENANT_ID".into(), tenant.into())]
        );
        assert!(!graph.args.last().unwrap().contains(tenant));

        let azure_context = azure_cli_context_spec(Some(&target));
        assert_eq!(
            azure_context.args,
            [
                "account",
                "show",
                "--output",
                "json",
                "--subscription",
                subscription
            ]
        );
        let az_context = az_power_shell_context_spec(Some(&target));
        assert!(az_context
            .args
            .last()
            .unwrap()
            .contains("Get-AzContext -ListAvailable"));
        assert!(!az_context.args.last().unwrap().contains(subscription));
    }

    #[test]
    fn rejects_an_invalid_or_nil_target_tenant() {
        let subscription = "22222222-2222-2222-2222-222222222222";
        for tenant in ["not-a-tenant", "00000000-0000-0000-0000-000000000000"] {
            let target = ConnectionTarget {
                tenant_id: tenant.into(),
                subscription_id: subscription.into(),
            };
            assert!(login_spec("azd", &[], Some(&target)).is_err());
        }
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
