use crate::{
    command_runner::{capture, CommandSpec, Executable},
    deployment::validate_optional_guid,
    error::{AppError, AppResult},
    models::{SecurityGroup, SecurityGroupSearch},
};
use serde::Deserialize;
use std::time::Duration;

const GRAPH_TIMEOUT: Duration = Duration::from_secs(20);
const QUERY_PAGE_SIZE: usize = 26;
const RETURNED_GROUP_LIMIT: usize = 25;
const MAX_DISPLAY_NAME_LENGTH: usize = 256;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct AzureCliAccount {
    id: String,
    tenant_id: String,
}

#[derive(Deserialize)]
struct OrganizationEnvelope {
    value: Vec<Organization>,
}

#[derive(Deserialize)]
struct Organization {
    id: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct GroupsEnvelope {
    value: Vec<GraphGroup>,
    #[serde(rename = "@odata.nextLink")]
    next_link: Option<serde_json::Value>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct GraphGroup {
    id: String,
    display_name: String,
    security_enabled: Option<bool>,
}

fn validate_required_guid(label: &str, value: &str) -> AppResult<()> {
    if value.is_empty() {
        return Err(AppError::Invalid(format!("{label} is required.")));
    }
    validate_optional_guid(label, Some(value))
}

fn validate_query(query: &str) -> AppResult<&str> {
    let query = query.trim();
    if !(2..=80).contains(&query.chars().count()) || query.chars().any(char::is_control) {
        return Err(AppError::Invalid(
            "Group search must contain 2 to 80 non-control characters.".into(),
        ));
    }
    Ok(query)
}

fn account_context_spec(subscription_id: &str) -> CommandSpec {
    CommandSpec::new(
        Executable::AzureCli,
        [
            "account",
            "show",
            "--subscription",
            subscription_id,
            "--output",
            "json",
            "--only-show-errors",
        ],
    )
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

fn graph_groups_spec(subscription_id: &str, query: &str) -> CommandSpec {
    let escaped_query = query.replace('\'', "''");
    let filter =
        format!("$filter=securityEnabled eq true and startswith(displayName,'{escaped_query}')");
    CommandSpec::new(
        Executable::AzureCli,
        [
            "rest".into(),
            "--method".into(),
            "GET".into(),
            "--url".into(),
            "https://graph.microsoft.com/v1.0/groups".into(),
            "--uri-parameters".into(),
            filter,
            "$select=id,displayName,securityEnabled".into(),
            format!("$top={QUERY_PAGE_SIZE}"),
            "$count=true".into(),
            "--headers".into(),
            "ConsistencyLevel=eventual".into(),
            "--subscription".into(),
            subscription_id.into(),
            "--output".into(),
            "json".into(),
            "--only-show-errors".into(),
        ],
    )
}

async fn read_json<T: for<'de> Deserialize<'de>>(spec: CommandSpec) -> AppResult<T> {
    let output = capture(spec, GRAPH_TIMEOUT)
        .await
        .map_err(|_| AppError::Command("Could not query the selected directory.".into()))?;
    if !output.success {
        return Err(AppError::Command(
            "Could not query the selected directory.".into(),
        ));
    }
    serde_json::from_str(&output.stdout)
        .map_err(|_| AppError::Invalid("The directory returned an invalid response.".into()))
}

async fn prove_target(tenant_id: &str, subscription_id: &str) -> AppResult<()> {
    let account: AzureCliAccount = read_json(account_context_spec(subscription_id)).await?;
    if !account.id.eq_ignore_ascii_case(subscription_id)
        || !account.tenant_id.eq_ignore_ascii_case(tenant_id)
    {
        return Err(AppError::Invalid(
            "Azure CLI did not confirm the selected tenant and subscription.".into(),
        ));
    }

    let organization: OrganizationEnvelope =
        read_json(graph_organization_spec(subscription_id)).await?;
    if !organization
        .value
        .iter()
        .any(|organization| organization.id.eq_ignore_ascii_case(tenant_id))
    {
        return Err(AppError::Invalid(
            "Microsoft Graph did not confirm the selected tenant.".into(),
        ));
    }
    Ok(())
}

fn normalize_groups(envelope: GroupsEnvelope) -> AppResult<(Vec<SecurityGroup>, bool)> {
    if envelope.value.len() > QUERY_PAGE_SIZE {
        return Err(AppError::Invalid(
            "The directory returned an invalid response.".into(),
        ));
    }
    let response_count = envelope.value.len();
    let mut groups = Vec::with_capacity(response_count.min(RETURNED_GROUP_LIMIT));
    for group in envelope.value {
        validate_required_guid("Directory group ID", &group.id)
            .map_err(|_| AppError::Invalid("The directory returned an invalid response.".into()))?;
        if group.security_enabled != Some(true)
            || group.display_name.is_empty()
            || group.display_name.chars().count() > MAX_DISPLAY_NAME_LENGTH
            || group.display_name.chars().any(char::is_control)
        {
            return Err(AppError::Invalid(
                "The directory returned an invalid response.".into(),
            ));
        }
        if groups.len() < RETURNED_GROUP_LIMIT {
            groups.push(SecurityGroup {
                id: group.id,
                display_name: group.display_name,
            });
        }
    }
    Ok((
        groups,
        envelope.next_link.is_some() || response_count > RETURNED_GROUP_LIMIT,
    ))
}

pub async fn search(
    tenant_id: &str,
    subscription_id: &str,
    query: &str,
) -> AppResult<SecurityGroupSearch> {
    validate_required_guid("Tenant ID", tenant_id)?;
    validate_required_guid("Subscription ID", subscription_id)?;
    let query = validate_query(query)?;

    // Each Azure CLI command pins --subscription, so the Graph token is
    // acquired for the same selected subscription as the two sequential proofs.
    prove_target(tenant_id, subscription_id).await?;
    let envelope: GroupsEnvelope = read_json(graph_groups_spec(subscription_id, query)).await?;
    let (groups, has_more) = normalize_groups(envelope)?;
    Ok(SecurityGroupSearch {
        tenant_id: tenant_id.into(),
        subscription_id: subscription_id.into(),
        groups,
        has_more,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn graph_query_is_fixed_encoded_and_subscription_bound() {
        let subscription = "22222222-2222-2222-2222-222222222222";
        let spec = graph_groups_spec(subscription, "O'Brien");

        assert!(spec
            .args
            .windows(2)
            .any(|pair| pair == ["--url", "https://graph.microsoft.com/v1.0/groups"]));
        assert!(spec
            .args
            .windows(2)
            .any(|pair| pair == ["--subscription", subscription]));
        assert!(spec
            .args
            .windows(2)
            .any(|pair| pair == ["--headers", "ConsistencyLevel=eventual"]));
        assert!(spec.args.iter().any(|arg| arg == "$count=true"));
        assert!(spec.args.iter().any(|arg| arg == "$top=26"));
        let filter = spec
            .args
            .iter()
            .find(|arg| arg.starts_with("$filter="))
            .unwrap();
        assert!(filter.contains("securityEnabled eq true"));
        assert!(filter.contains("startswith(displayName,'O''Brien')"));
        assert!(!filter.contains('&'));
    }

    #[test]
    fn validation_rejects_invalid_targets_and_queries() {
        assert!(search_validation(
            "00000000-0000-0000-0000-000000000000",
            "22222222-2222-2222-2222-222222222222",
            "Fi"
        )
        .is_err());
        assert!(validate_query(" x").is_err());
        assert!(validate_query("two\nlines").is_err());
        assert_eq!(validate_query("  Finance  ").unwrap(), "Finance");
    }

    fn search_validation(tenant_id: &str, subscription_id: &str, query: &str) -> AppResult<()> {
        validate_required_guid("Tenant ID", tenant_id)?;
        validate_required_guid("Subscription ID", subscription_id)?;
        validate_query(query)?;
        Ok(())
    }

    #[test]
    fn response_keeps_only_valid_security_groups_and_never_follows_links() {
        let envelope: GroupsEnvelope = serde_json::from_value(json!({
            "value": (0..26).map(|index| json!({
                "id": format!("00000000-0000-0000-0000-{:012x}", index + 1),
                "displayName": format!("Group {index}"),
                "securityEnabled": true
            })).collect::<Vec<_>>(),
            "@odata.nextLink": "https://graph.microsoft.com/v1.0/groups?$skiptoken=opaque"
        }))
        .unwrap();

        let (groups, has_more) = normalize_groups(envelope).unwrap();
        assert_eq!(groups.len(), RETURNED_GROUP_LIMIT);
        assert!(has_more);
        assert!(groups
            .iter()
            .all(|group| !group.display_name.contains('\n')));
    }

    #[test]
    fn response_rejects_non_security_or_unsafe_groups() {
        for group in [
            json!({"id":"11111111-1111-1111-1111-111111111111","displayName":"Microsoft 365","securityEnabled":false}),
            json!({"id":"not-a-guid","displayName":"Security","securityEnabled":true}),
            json!({"id":"11111111-1111-1111-1111-111111111111","displayName":"Security\nGroup","securityEnabled":true}),
        ] {
            let envelope: GroupsEnvelope =
                serde_json::from_value(json!({"value":[group]})).unwrap();
            assert!(normalize_groups(envelope).is_err());
        }
    }
}
