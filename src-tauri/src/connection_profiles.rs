use crate::{
    command_runner::{capture, CapturedOutput, CommandSpec, Executable},
    error::{AppError, AppResult},
};
use serde::Serialize;
use serde_json::Value;
use std::{collections::BTreeMap, time::Duration};
use uuid::Uuid;

const MAX_PROFILES: usize = 100;

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VisibleProfile {
    account: String,
    tenant_id: String,
    subscription_id: String,
    subscription_name: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VisibleProfiles {
    provider_id: String,
    state: &'static str,
    profiles: Vec<VisibleProfile>,
    has_more: bool,
}

fn inventory_spec(provider_id: &str) -> AppResult<CommandSpec> {
    match provider_id {
        // No refresh: enumerate provider-owned metadata without acquiring tokens.
        "azureCli" => Ok(CommandSpec::new(Executable::AzureCli, [
            "account", "list", "--all", "--output", "json", "--only-show-errors",
            "--query", "[:101].{account:user.name,tenantId:tenantId,subscriptionId:id,subscriptionName:name}",
        ])),
        "azPowerShell" => Ok(CommandSpec::new(Executable::PowerShell, [
            "-NoLogo", "-NoProfile", "-NonInteractive", "-Command",
            "$ErrorActionPreference='Stop'; $c=@(Get-AzContext -ListAvailable -ErrorAction Stop | Select-Object -First 101 | ForEach-Object {[pscustomobject]@{account=$_.Account.Id;tenantId=$_.Tenant.Id;subscriptionId=$_.Subscription.Id;subscriptionName=$_.Subscription.Name}}); ConvertTo-Json -InputObject $c -Compress",
        ])),
        _ => Err(AppError::Invalid("Visible profiles are supported only for Azure CLI and Azure PowerShell.".into())),
    }
}

fn label(value: &Value, key: &str, limit: usize) -> Option<String> {
    let text = value.get(key)?.as_str()?.trim();
    (!text.is_empty() && text.len() <= limit && !text.chars().any(char::is_control))
        .then(|| text.to_string())
}

fn guid(value: &Value, key: &str) -> Option<String> {
    let id = Uuid::parse_str(value.get(key)?.as_str()?).ok()?;
    (!id.is_nil()).then(|| id.to_string())
}

fn normalize(provider_id: &str, value: Option<&Value>) -> VisibleProfiles {
    let mut result = VisibleProfiles {
        provider_id: provider_id.into(),
        state: "unknown",
        profiles: vec![],
        has_more: false,
    };
    let Some(entries) = value.and_then(Value::as_array) else {
        return result;
    };
    result.state = "available";
    result.has_more = entries.len() > MAX_PROFILES;
    let mut profiles = BTreeMap::new();
    for entry in entries.iter().take(MAX_PROFILES + 1) {
        let (Some(account), Some(tenant_id), Some(subscription_id)) = (
            label(entry, "account", 320),
            guid(entry, "tenantId"),
            guid(entry, "subscriptionId"),
        ) else {
            continue;
        };
        let key = (
            account.to_lowercase(),
            tenant_id.clone(),
            subscription_id.clone(),
        );
        profiles.entry(key).or_insert_with(|| VisibleProfile {
            account,
            tenant_id,
            subscription_id,
            subscription_name: label(entry, "subscriptionName", 256),
        });
    }
    result.profiles = profiles.into_values().take(MAX_PROFILES).collect();
    if !entries.is_empty() && result.profiles.is_empty() {
        result.state = "unknown";
    }
    result
}

fn inventory_output(provider_id: &str, output: AppResult<CapturedOutput>) -> VisibleProfiles {
    let value = match output {
        Ok(output) if output.success => serde_json::from_str::<Value>(&output.stdout).ok(),
        _ => None,
    };
    // Never return raw provider output or errors to the renderer or history.
    normalize(provider_id, value.as_ref())
}

pub async fn list(provider_id: &str) -> AppResult<VisibleProfiles> {
    let spec = inventory_spec(provider_id)?;
    Ok(inventory_output(
        provider_id,
        capture(spec, Duration::from_secs(15)).await,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn profile(account: &str, tenant: &str, subscription: &str) -> Value {
        json!({"account": account, "tenantId": tenant, "subscriptionId": subscription,
            "subscriptionName": "Lab", "accessToken": "must-not-leave-native-boundary"})
    }
    const TENANT: &str = "11111111-1111-4111-8111-111111111111";
    const SUB: &str = "22222222-2222-4222-8222-222222222222";

    #[test]
    fn inventory_is_allowlisted_deduplicated_and_not_readiness() {
        let input = json!([
            profile("Nora@example.test", TENANT, SUB),
            profile("nora@example.test", TENANT, SUB),
            profile("other@example.test", TENANT, SUB),
            profile(
                "Nora@example.test",
                "33333333-3333-4333-8333-333333333333",
                SUB
            ),
        ]);
        let result = normalize("azureCli", Some(&input));
        assert_eq!(result.state, "available");
        assert_eq!(result.profiles.len(), 3);
        let serialized = serde_json::to_value(result).unwrap();
        let fields = serialized["profiles"][0].as_object().unwrap();
        assert_eq!(fields.len(), 4);
        assert!(fields.contains_key("account"));
        assert!(!serialized.to_string().contains("must-not-leave"));
        assert!(!serialized.to_string().contains("connected"));
    }

    #[test]
    fn malformed_empty_and_oversized_metadata_are_suppressed() {
        let input = json!([
            profile("", TENANT, SUB), profile("a\nb", TENANT, SUB),
            profile(&"x".repeat(321), TENANT, SUB), profile("valid", "bad", SUB),
            profile("valid", TENANT, "00000000-0000-0000-0000-000000000000"),
            {"account": 7}, null, profile(" valid ", TENANT, SUB),
        ]);
        let result = normalize("azPowerShell", Some(&input));
        assert_eq!(result.profiles.len(), 1);
        assert_eq!(result.profiles[0].account, "valid");
        assert_eq!(normalize("azureCli", Some(&json!([]))).state, "available");
        for value in [
            None,
            Some(json!({})),
            Some(json!("unexpected")),
            Some(json!([null])),
        ] {
            let result = normalize("azureCli", value.as_ref());
            assert_eq!(result.state, "unknown");
            assert!(result.profiles.is_empty());
        }
    }

    #[test]
    fn inventory_has_a_fixed_bound() {
        let input = Value::Array(
            (0..120)
                .map(|i| profile(&format!("user{i}"), TENANT, SUB))
                .collect(),
        );
        let result = normalize("azureCli", Some(&input));
        assert_eq!(result.profiles.len(), MAX_PROFILES);
        assert!(result.has_more);
    }

    #[test]
    fn failed_or_missing_providers_cannot_export_metadata_or_errors() {
        for output in [
            Err(AppError::MissingExecutable("private-path".into())),
            Err(AppError::Command("Bearer private-error".into())),
            Ok(CapturedOutput {
                stdout: json!([profile("cached", TENANT, SUB)]).to_string(),
                stderr: "private-error".into(),
                success: false,
                executable_path: "private-path".into(),
            }),
            Ok(CapturedOutput {
                stdout: "invalid-json".into(),
                stderr: String::new(),
                success: true,
                executable_path: "private-path".into(),
            }),
        ] {
            let result = inventory_output("azureCli", output);
            assert_eq!(result.state, "unknown");
            assert!(result.profiles.is_empty());
            let serialized = serde_json::to_string(&result).unwrap();
            assert!(!serialized.contains("private"));
            assert!(!serialized.contains("cached"));
        }
    }

    #[test]
    fn only_metadata_commands_are_offered() {
        let cli = inventory_spec("azureCli").unwrap();
        assert_eq!(&cli.args[..3], ["account", "list", "--all"]);
        assert!(!cli.args.iter().any(|arg| arg == "--refresh"));
        let ps = inventory_spec("azPowerShell").unwrap();
        let script = ps.args.last().unwrap();
        assert!(script.contains("Get-AzContext -ListAvailable"));
        assert!(!script.contains("Set-AzContext"));
        assert!(!script.contains("Connect-AzAccount"));
        assert!(inventory_spec("azd").is_err());
        assert!(inventory_spec("graphPowerShell").is_err());
    }
}
