use crate::{
    command_runner::{capture, CommandSpec, Executable},
    error::{AppError, AppResult},
    models::{AzureDiscovery, AzureLocation, AzureResourceGroup, AzureSubscription, AzureTenant},
};
use serde::Deserialize;
use std::{collections::BTreeMap, time::Duration};
use uuid::Uuid;

const DISCOVERY_TIMEOUT: Duration = Duration::from_secs(30);

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct AccountRecord {
    id: String,
    name: String,
    tenant_id: String,
    #[serde(default)]
    tenant_display_name: Option<String>,
    #[serde(default)]
    tenant_default_domain: Option<String>,
    #[serde(default)]
    is_default: bool,
    #[serde(default)]
    state: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct LocationRecord {
    name: String,
    display_name: String,
    #[serde(default)]
    regional_display_name: Option<String>,
    #[serde(default)]
    metadata: Option<LocationMetadata>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct LocationMetadata {
    #[serde(default)]
    region_type: Option<String>,
}

#[derive(Debug, Deserialize)]
struct LocationListResult {
    value: Vec<LocationRecord>,
}

#[derive(Debug, Deserialize)]
struct ResourceGroupRecord {
    name: String,
    location: String,
}

fn parse_accounts(json: &str) -> AppResult<(Vec<AzureSubscription>, Vec<AzureTenant>)> {
    let records: Vec<AccountRecord> = serde_json::from_str(json)?;
    let mut subscriptions = records
        .into_iter()
        .filter(|record| match record.state.as_deref() {
            Some(state) => state.eq_ignore_ascii_case("enabled"),
            None => true,
        })
        .map(|record| AzureSubscription {
            id: record.id,
            name: record.name,
            tenant_id: record.tenant_id,
            tenant_display_name: record.tenant_display_name,
            tenant_default_domain: record.tenant_default_domain,
            is_default: record.is_default,
        })
        .collect::<Vec<_>>();
    subscriptions.sort_by(|left, right| {
        right
            .is_default
            .cmp(&left.is_default)
            .then_with(|| left.name.to_lowercase().cmp(&right.name.to_lowercase()))
            .then_with(|| left.id.cmp(&right.id))
    });
    subscriptions.dedup_by(|left, right| left.id.eq_ignore_ascii_case(&right.id));

    let mut tenants = BTreeMap::<String, AzureTenant>::new();
    for subscription in &subscriptions {
        let key = subscription.tenant_id.to_lowercase();
        let tenant = tenants.entry(key).or_insert_with(|| AzureTenant {
            tenant_id: subscription.tenant_id.clone(),
            display_name: None,
            default_domain: None,
        });
        if tenant.display_name.is_none() {
            tenant.display_name = subscription.tenant_display_name.clone();
        }
        if tenant.default_domain.is_none() {
            tenant.default_domain = subscription.tenant_default_domain.clone();
        }
    }
    let mut tenants = tenants.into_values().collect::<Vec<_>>();
    tenants.sort_by(|left, right| {
        left.display_name
            .as_deref()
            .or(left.default_domain.as_deref())
            .unwrap_or(&left.tenant_id)
            .to_lowercase()
            .cmp(
                &right
                    .display_name
                    .as_deref()
                    .or(right.default_domain.as_deref())
                    .unwrap_or(&right.tenant_id)
                    .to_lowercase(),
            )
    });
    Ok((subscriptions, tenants))
}

fn parse_locations(json: &str) -> AppResult<Vec<AzureLocation>> {
    let result: LocationListResult = serde_json::from_str(json)?;
    let mut locations = result
        .value
        .into_iter()
        .filter(|record| {
            match record
                .metadata
                .as_ref()
                .and_then(|metadata| metadata.region_type.as_deref())
            {
                Some(kind) => kind.eq_ignore_ascii_case("physical"),
                None => true,
            }
        })
        .map(|record| AzureLocation {
            name: record.name,
            display_name: record.display_name,
            regional_display_name: record.regional_display_name,
        })
        .collect::<Vec<_>>();
    locations.sort_by(|left, right| {
        left.display_name
            .to_lowercase()
            .cmp(&right.display_name.to_lowercase())
            .then_with(|| left.name.cmp(&right.name))
    });
    locations.dedup_by(|left, right| left.name.eq_ignore_ascii_case(&right.name));
    Ok(locations)
}

fn parse_resource_groups(json: &str) -> AppResult<Vec<AzureResourceGroup>> {
    let records: Vec<ResourceGroupRecord> = serde_json::from_str(json)?;
    let mut groups = records
        .into_iter()
        .map(|record| AzureResourceGroup {
            name: record.name,
            location: record.location,
        })
        .collect::<Vec<_>>();
    groups.sort_by_key(|group| group.name.to_lowercase());
    groups.dedup_by(|left, right| left.name.eq_ignore_ascii_case(&right.name));
    Ok(groups)
}

fn command_failure(label: &str, stderr: &str) -> AppError {
    let detail = stderr.trim();
    if detail.is_empty() {
        AppError::Command(format!("Azure CLI could not {label}."))
    } else {
        AppError::Command(format!("Azure CLI could not {label}: {detail}"))
    }
}

pub async fn discover(subscription_id: Option<&str>, refresh: bool) -> AppResult<AzureDiscovery> {
    let mut account_args = vec![
        "account".to_string(),
        "list".to_string(),
        "--output".to_string(),
        "json".to_string(),
        "--only-show-errors".to_string(),
    ];
    if refresh {
        account_args.push("--refresh".into());
    }
    let account_output = capture(
        CommandSpec::new(Executable::AzureCli, account_args),
        DISCOVERY_TIMEOUT,
    )
    .await?;
    if !account_output.success {
        return Err(command_failure(
            "list subscriptions",
            &account_output.stderr,
        ));
    }
    let (subscriptions, tenants) = parse_accounts(&account_output.stdout)?;
    let mut discovery = AzureDiscovery {
        provider: "azureCli".into(),
        subscriptions,
        tenants,
        locations: vec![],
        resource_groups: vec![],
        selected_subscription_id: None,
        warnings: vec![],
    };

    let Some(requested_id) = subscription_id else {
        return Ok(discovery);
    };
    Uuid::parse_str(requested_id)
        .map_err(|_| AppError::Invalid("Subscription ID must be a UUID.".into()))?;
    let selected = discovery
        .subscriptions
        .iter()
        .find(|subscription| subscription.id.eq_ignore_ascii_case(requested_id))
        .ok_or_else(|| {
            AppError::Invalid(
                "The selected subscription is not available to the signed-in Azure CLI account."
                    .into(),
            )
        })?;
    discovery.selected_subscription_id = Some(selected.id.clone());

    let locations_url = format!(
        "https://management.azure.com/subscriptions/{}/locations?api-version=2022-12-01",
        selected.id
    );
    let location_spec = CommandSpec::new(
        Executable::AzureCli,
        [
            "rest",
            "--method",
            "get",
            "--url",
            locations_url.as_str(),
            "--output",
            "json",
            "--only-show-errors",
        ],
    );
    let group_spec = CommandSpec::new(
        Executable::AzureCli,
        [
            "group",
            "list",
            "--subscription",
            selected.id.as_str(),
            "--output",
            "json",
            "--only-show-errors",
        ],
    );
    let (location_result, group_result) = tokio::join!(
        capture(location_spec, DISCOVERY_TIMEOUT),
        capture(group_spec, DISCOVERY_TIMEOUT)
    );

    match location_result {
        Ok(output) if output.success => match parse_locations(&output.stdout) {
            Ok(locations) => discovery.locations = locations,
            Err(error) => discovery
                .warnings
                .push(format!("Azure regions could not be read: {error}")),
        },
        Ok(output) => discovery
            .warnings
            .push(command_failure("list regions", &output.stderr).to_string()),
        Err(error) => discovery
            .warnings
            .push(format!("Azure regions could not be loaded: {error}")),
    }
    match group_result {
        Ok(output) if output.success => match parse_resource_groups(&output.stdout) {
            Ok(groups) => discovery.resource_groups = groups,
            Err(error) => discovery
                .warnings
                .push(format!("Resource groups could not be read: {error}")),
        },
        Ok(output) => discovery
            .warnings
            .push(command_failure("list resource groups", &output.stderr).to_string()),
        Err(error) => discovery
            .warnings
            .push(format!("Resource groups could not be loaded: {error}")),
    }
    Ok(discovery)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accounts_are_filtered_sorted_and_grouped_by_tenant() {
        let json = r#"[
          {"id":"22222222-2222-2222-2222-222222222222","name":"Zulu","tenantId":"TENANT-A","tenantDisplayName":"Contoso","isDefault":false,"state":"Enabled"},
          {"id":"11111111-1111-1111-1111-111111111111","name":"Alpha","tenantId":"tenant-a","tenantDefaultDomain":"contoso.onmicrosoft.com","isDefault":true,"state":"Enabled"},
          {"id":"33333333-3333-3333-3333-333333333333","name":"Disabled","tenantId":"tenant-b","isDefault":false,"state":"Disabled"}
        ]"#;
        let (subscriptions, tenants) = parse_accounts(json).unwrap();
        assert_eq!(subscriptions.len(), 2);
        assert_eq!(subscriptions[0].name, "Alpha");
        assert_eq!(tenants.len(), 1);
        assert_eq!(tenants[0].display_name.as_deref(), Some("Contoso"));
        assert_eq!(
            tenants[0].default_domain.as_deref(),
            Some("contoso.onmicrosoft.com")
        );
    }

    #[test]
    fn locations_keep_physical_regions_and_sort_by_display_name() {
        let json = r#"{"value":[
          {"name":"westus2","displayName":"West US 2","metadata":{"regionType":"Physical"}},
          {"name":"global","displayName":"Global","metadata":{"regionType":"Logical"}},
          {"name":"eastus","displayName":"East US","metadata":{"regionType":"Physical"}}
        ]}"#;
        let locations = parse_locations(json).unwrap();
        assert_eq!(locations.len(), 2);
        assert_eq!(locations[0].name, "eastus");
    }

    #[test]
    fn resource_groups_are_sorted_case_insensitively() {
        let json = r#"[
          {"name":"zeta-rg","location":"westus2"},
          {"name":"Alpha-rg","location":"eastus"}
        ]"#;
        let groups = parse_resource_groups(json).unwrap();
        assert_eq!(groups[0].name, "Alpha-rg");
    }
}
