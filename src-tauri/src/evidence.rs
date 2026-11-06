use chrono::DateTime;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{collections::HashMap, fs, io::Read, path::Path};
use uuid::Uuid;

const MAX_TEXT: usize = 128;
const MAX_ACTIONS: usize = 20;
const MAX_DOTENV_BYTES: u64 = 1024 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum EvidenceBindingState {
    Current,
    Stale,
    Mismatch,
    LegacyUnbound,
    InsufficientContext,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EvidenceNextAction {
    pub code: EvidenceActionCode,
    pub owner: EvidenceActionOwner,
    pub priority: EvidenceActionPriority,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum EvidenceActionCode {
    ReviewWarnings,
    RetryOperation,
    VerifyResources,
    VerifyPermissions,
    ProveDelivery,
    CompleteCleanup,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum EvidenceActionOwner {
    Operator,
    SubscriptionOwner,
    IdentityAdmin,
    MessagingAdmin,
    ApplicationOwner,
    TemplateMaintainer,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum EvidenceActionPriority {
    Required,
    Recommended,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct OperationEvidenceContext {
    pub project_id: String,
    pub environment: String,
    pub azure_cloud: Option<String>,
    pub tenant_id: Option<String>,
    pub subscription_id: Option<String>,
    pub resource_group: Option<String>,
    pub template_id: String,
    pub revision: Option<String>,
    pub contract_digest: Option<String>,
}

#[derive(Debug, Clone)]
pub struct PersistedEvidenceOperation {
    pub id: String,
    pub kind: String,
    pub timestamp: String,
    pub context: Option<OperationEvidenceContext>,
}

#[derive(Debug, Clone)]
pub struct ExpectedEvidenceContext {
    pub current: OperationEvidenceContext,
    pub operations: Vec<PersistedEvidenceOperation>,
}

#[derive(Debug, Clone, Default)]
pub struct TargetEvidenceValues {
    pub azure_cloud: Option<String>,
    pub tenant_id: Option<String>,
    pub subscription_id: Option<String>,
    pub location: Option<String>,
    pub resource_group: Option<String>,
}

pub fn read_target_values(project_root: &Path, environment: &str) -> TargetEvidenceValues {
    if environment.is_empty()
        || environment.len() > MAX_TEXT
        || environment.contains(['/', '\\'])
        || environment == "."
        || environment == ".."
    {
        return TargetEvidenceValues::default();
    }
    read_target_values_inner(project_root, environment).unwrap_or_default()
}

pub(crate) fn read_environment_values(
    project_root: &Path,
    environment: &str,
    wanted: &[&str],
) -> Option<HashMap<String, String>> {
    if crate::deployment::validate_environment_name(environment).is_err()
        || matches!(environment, "." | "..")
    {
        return None;
    }
    let root = fs::canonicalize(project_root).ok()?;
    let azure = root.join(".azure");
    if is_link_or_reparse(&azure).ok()? {
        return None;
    }
    let azure = fs::canonicalize(azure).ok()?;
    if !azure.starts_with(&root) || !azure.is_dir() {
        return None;
    }
    let environment_root = azure.join(environment);
    if is_link_or_reparse(&environment_root).ok()? {
        return None;
    }
    let environment_root = fs::canonicalize(environment_root).ok()?;
    if !environment_root.starts_with(&azure) || !environment_root.is_dir() {
        return None;
    }
    let dotenv = environment_root.join(".env");
    if is_link_or_reparse(&dotenv).ok()? {
        return None;
    }
    let metadata = fs::metadata(&dotenv).ok()?;
    if !metadata.is_file() || metadata.len() > MAX_DOTENV_BYTES {
        return None;
    }
    let dotenv = fs::canonicalize(dotenv).ok()?;
    if !dotenv.starts_with(&environment_root) {
        return None;
    }
    let mut bytes = Vec::new();
    fs::File::open(dotenv)
        .ok()?
        .take(MAX_DOTENV_BYTES + 1)
        .read_to_end(&mut bytes)
        .ok()?;
    if bytes.len() as u64 > MAX_DOTENV_BYTES {
        return None;
    }
    let content = String::from_utf8(bytes).ok()?;
    let mut values = HashMap::new();
    for line in content.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let line = line.strip_prefix("export ").unwrap_or(line);
        let Some((key, raw)) = line.split_once('=') else {
            continue;
        };
        let key = key.trim();
        if !wanted.contains(&key) {
            continue;
        }
        let value = dotenv_value(raw.trim())?;
        if value.len() > 32_768 || value.chars().any(|character| character <= '\u{1f}') {
            return None;
        }
        values.insert(key.to_string(), value);
    }
    Some(values)
}

fn read_target_values_inner(
    project_root: &Path,
    environment: &str,
) -> Option<TargetEvidenceValues> {
    let wanted = [
        "AZD_GUI_AZURE_CLOUD",
        "AZURE_TENANT_ID",
        "AZURE_SUBSCRIPTION_ID",
        "AZURE_LOCATION",
        "AZURE_RESOURCE_GROUP",
    ];
    let mut values = read_environment_values(project_root, environment, &wanted)?;
    if values.values().any(|value| value.len() > 256) {
        return None;
    }
    Some(TargetEvidenceValues {
        azure_cloud: values.remove("AZD_GUI_AZURE_CLOUD"),
        tenant_id: values
            .remove("AZURE_TENANT_ID")
            .filter(|value| valid_uuid(value)),
        subscription_id: values
            .remove("AZURE_SUBSCRIPTION_ID")
            .filter(|value| valid_uuid(value)),
        location: values
            .remove("AZURE_LOCATION")
            .filter(|value| bounded(value)),
        resource_group: values
            .remove("AZURE_RESOURCE_GROUP")
            .filter(|value| bounded(value)),
    })
}

fn dotenv_value(raw: &str) -> Option<String> {
    if raw.starts_with('"') || raw.starts_with('\'') {
        let quote = raw.chars().next()?;
        if raw.len() < 2 || !raw.ends_with(quote) {
            return None;
        }
        let value = &raw[1..raw.len() - 1];
        if quote == '\'' {
            return Some(value.into());
        }
        let mut decoded = String::new();
        let mut escaped = false;
        for character in value.chars() {
            if escaped {
                decoded.push(match character {
                    'n' => '\n',
                    'r' => '\r',
                    't' => '\t',
                    other => other,
                });
                escaped = false;
            } else if character == '\\' {
                escaped = true;
            } else {
                decoded.push(character);
            }
        }
        if escaped {
            return None;
        }
        Some(decoded)
    } else {
        Some(raw.split(" #").next().unwrap_or(raw).trim().into())
    }
}

fn is_link_or_reparse(path: &Path) -> std::io::Result<bool> {
    let metadata = fs::symlink_metadata(path)?;
    if metadata.file_type().is_symlink() {
        return Ok(true);
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        Ok(metadata.file_attributes() & 0x400 != 0)
    }
    #[cfg(not(windows))]
    {
        Ok(false)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EvidenceParseError {
    Invalid,
    FutureVersion,
}

#[derive(Debug)]
pub struct ParsedEvidence {
    raw: RawEvidence,
    pub next_actions: Vec<EvidenceNextAction>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct RawEvidence {
    version: String,
    evidence_class: String,
    binding: RawBinding,
    next_actions: Vec<RawNextAction>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct RawBinding {
    project: RawProjectBinding,
    target: RawTargetBinding,
    source: RawSourceBinding,
    operation: RawOperationBinding,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct RawProjectBinding {
    id: String,
    environment: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct RawTargetBinding {
    azure_cloud: String,
    tenant_id: String,
    subscription_id: String,
    resource_group: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct RawSourceBinding {
    template_id: String,
    revision: String,
    contract_digest: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct RawOperationBinding {
    id: String,
    kind: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct RawNextAction {
    code: String,
    owner: String,
    priority: String,
}

pub fn parse_evidence(
    value: &Value,
    evidence_classes: &[&str],
    operation_kinds: &[&str],
) -> Result<ParsedEvidence, EvidenceParseError> {
    let raw: RawEvidence =
        serde_json::from_value(value.clone()).map_err(|_| EvidenceParseError::Invalid)?;
    if raw.version != "1" {
        return Err(
            if raw.version.parse::<u32>().is_ok_and(|version| version > 1) {
                EvidenceParseError::FutureVersion
            } else {
                EvidenceParseError::Invalid
            },
        );
    }
    let class_operation_valid = match raw.evidence_class.as_str() {
        "validation" => raw.binding.operation.kind == "validate",
        "delivery" => raw.binding.operation.kind == "delivery",
        "resourceMutation" => matches!(
            raw.binding.operation.kind.as_str(),
            "provision" | "deploy" | "up"
        ),
        "cleanup" => matches!(raw.binding.operation.kind.as_str(), "down" | "cleanup"),
        _ => false,
    };
    if !evidence_classes.contains(&raw.evidence_class.as_str())
        || !operation_kinds.contains(&raw.binding.operation.kind.as_str())
        || !class_operation_valid
        || raw.next_actions.len() > MAX_ACTIONS
        || !bounded(&raw.binding.project.id)
        || !bounded(&raw.binding.project.environment)
        || !bounded(&raw.binding.target.azure_cloud)
        || !valid_uuid(&raw.binding.target.tenant_id)
        || !valid_uuid(&raw.binding.target.subscription_id)
        || raw
            .binding
            .target
            .resource_group
            .as_deref()
            .is_some_and(|value| !bounded(value))
        || !bounded(&raw.binding.source.template_id)
        || !valid_revision(&raw.binding.source.revision)
        || raw
            .binding
            .source
            .contract_digest
            .as_deref()
            .is_some_and(|value| !valid_digest(value))
        || !valid_uuid(&raw.binding.operation.id)
    {
        return Err(EvidenceParseError::Invalid);
    }
    let next_actions = raw
        .next_actions
        .iter()
        .map(project_action)
        .collect::<Option<Vec<_>>>()
        .ok_or(EvidenceParseError::Invalid)?;
    Ok(ParsedEvidence { raw, next_actions })
}

pub fn correlate(
    evidence: &ParsedEvidence,
    expected: Option<&ExpectedEvidenceContext>,
) -> EvidenceBindingState {
    let Some(expected) = expected else {
        return EvidenceBindingState::InsufficientContext;
    };
    let binding = &evidence.raw.binding;
    let Some(operation) = expected
        .operations
        .iter()
        .find(|operation| operation.id == binding.operation.id)
    else {
        return if expected.operations.is_empty() {
            EvidenceBindingState::InsufficientContext
        } else {
            EvidenceBindingState::Mismatch
        };
    };
    if operation.kind != binding.operation.kind {
        return EvidenceBindingState::Mismatch;
    }
    let Some(operation_context) = operation.context.as_ref() else {
        return EvidenceBindingState::InsufficientContext;
    };

    let comparisons = [
        compare_required(&binding.project.id, Some(&expected.current.project_id)),
        compare_required(
            &binding.project.environment,
            Some(&expected.current.environment),
        ),
        compare_required(
            &binding.target.azure_cloud,
            expected.current.azure_cloud.as_ref(),
        ),
        compare_required(
            &binding.target.tenant_id,
            expected.current.tenant_id.as_ref(),
        ),
        compare_required(
            &binding.target.subscription_id,
            expected.current.subscription_id.as_ref(),
        ),
        compare_optional(
            binding.target.resource_group.as_ref(),
            expected.current.resource_group.as_ref(),
        ),
        compare_required(
            &binding.source.template_id,
            Some(&expected.current.template_id),
        ),
        compare_revision(&binding.source.revision, expected.current.revision.as_ref()),
        compare_optional(
            binding.source.contract_digest.as_ref(),
            expected.current.contract_digest.as_ref(),
        ),
        compare_required(&binding.project.id, Some(&operation_context.project_id)),
        compare_required(
            &binding.project.environment,
            Some(&operation_context.environment),
        ),
        compare_required(
            &binding.target.azure_cloud,
            operation_context.azure_cloud.as_ref(),
        ),
        compare_required(
            &binding.target.tenant_id,
            operation_context.tenant_id.as_ref(),
        ),
        compare_required(
            &binding.target.subscription_id,
            operation_context.subscription_id.as_ref(),
        ),
        compare_optional(
            binding.target.resource_group.as_ref(),
            operation_context.resource_group.as_ref(),
        ),
        compare_required(
            &binding.source.template_id,
            Some(&operation_context.template_id),
        ),
        compare_revision(
            &binding.source.revision,
            operation_context.revision.as_ref(),
        ),
        compare_optional(
            binding.source.contract_digest.as_ref(),
            operation_context.contract_digest.as_ref(),
        ),
    ];
    if comparisons.contains(&Comparison::Mismatch) {
        return EvidenceBindingState::Mismatch;
    }
    if comparisons.contains(&Comparison::Unverifiable) {
        return EvidenceBindingState::InsufficientContext;
    }

    let Ok(matched_at) = DateTime::parse_from_rfc3339(&operation.timestamp) else {
        return EvidenceBindingState::InsufficientContext;
    };
    let stale = expected.operations.iter().any(|candidate| {
        matches!(
            candidate.kind.as_str(),
            "up" | "provision" | "deploy" | "down"
        ) && candidate.id != operation.id
            && DateTime::parse_from_rfc3339(&candidate.timestamp)
                .is_ok_and(|timestamp| timestamp > matched_at)
    });
    if stale {
        EvidenceBindingState::Stale
    } else {
        EvidenceBindingState::Current
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Comparison {
    Match,
    Mismatch,
    Unverifiable,
}

fn compare_required(actual: &str, expected: Option<&String>) -> Comparison {
    match expected {
        Some(expected) if actual == expected => Comparison::Match,
        Some(_) => Comparison::Mismatch,
        None => Comparison::Unverifiable,
    }
}

fn compare_optional(actual: Option<&String>, expected: Option<&String>) -> Comparison {
    match (actual, expected) {
        (Some(actual), Some(expected)) if actual == expected => Comparison::Match,
        (Some(_), Some(_)) => Comparison::Mismatch,
        (None, None) => Comparison::Match,
        _ => Comparison::Unverifiable,
    }
}

fn compare_revision(actual: &str, expected: Option<&String>) -> Comparison {
    if actual == "local" {
        Comparison::Unverifiable
    } else {
        compare_required(actual, expected)
    }
}

fn project_action(raw: &RawNextAction) -> Option<EvidenceNextAction> {
    let code = match raw.code.as_str() {
        "reviewWarnings" => EvidenceActionCode::ReviewWarnings,
        "retryOperation" => EvidenceActionCode::RetryOperation,
        "verifyResources" => EvidenceActionCode::VerifyResources,
        "verifyPermissions" => EvidenceActionCode::VerifyPermissions,
        "proveDelivery" => EvidenceActionCode::ProveDelivery,
        "completeCleanup" => EvidenceActionCode::CompleteCleanup,
        _ => return None,
    };
    let owner = match raw.owner.as_str() {
        "operator" => EvidenceActionOwner::Operator,
        "subscriptionOwner" => EvidenceActionOwner::SubscriptionOwner,
        "identityAdmin" => EvidenceActionOwner::IdentityAdmin,
        "messagingAdmin" => EvidenceActionOwner::MessagingAdmin,
        "applicationOwner" => EvidenceActionOwner::ApplicationOwner,
        "templateMaintainer" => EvidenceActionOwner::TemplateMaintainer,
        _ => return None,
    };
    let priority = match raw.priority.as_str() {
        "required" => EvidenceActionPriority::Required,
        "recommended" => EvidenceActionPriority::Recommended,
        _ => return None,
    };
    Some(EvidenceNextAction {
        code,
        owner,
        priority,
    })
}

fn bounded(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= MAX_TEXT
        && !value.chars().any(|character| character <= '\u{1f}')
}

fn valid_uuid(value: &str) -> bool {
    Uuid::parse_str(value).is_ok()
}

fn valid_revision(value: &str) -> bool {
    value == "local"
        || value.len() == 40
            && value
                .bytes()
                .all(|character| character.is_ascii_digit() || (b'a'..=b'f').contains(&character))
}

fn valid_digest(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|character| character.is_ascii_digit() || (b'a'..=b'f').contains(&character))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use tempfile::tempdir;

    fn validation_value() -> Value {
        let report: Value = serde_json::from_str(include_str!(
            "../tests/fixtures/deployment-validation-bound-v1.1.json"
        ))
        .unwrap();
        report["environment"]["metadata"]["azdManagementEvidence"].clone()
    }

    fn context(_kind: &str) -> OperationEvidenceContext {
        OperationEvidenceContext {
            project_id: "project-example".into(),
            environment: "dev".into(),
            azure_cloud: Some("AzureCloud".into()),
            tenant_id: Some("00000000-0000-0000-0000-000000000000".into()),
            subscription_id: Some("11111111-1111-1111-1111-111111111111".into()),
            resource_group: Some("rg-example-dev".into()),
            template_id: "example".into(),
            revision: Some("0123456789abcdef0123456789abcdef01234567".into()),
            contract_digest: Some("a".repeat(64)),
        }
    }

    fn expected(kind: &str) -> ExpectedEvidenceContext {
        let current = context(kind);
        ExpectedEvidenceContext {
            current: current.clone(),
            operations: vec![PersistedEvidenceOperation {
                id: "22222222-2222-2222-2222-222222222222".into(),
                kind: kind.into(),
                timestamp: "2026-09-21T04:53:46Z".into(),
                context: Some(current),
            }],
        }
    }

    #[test]
    fn correlates_exact_context_and_projects_only_allowlisted_actions() {
        let parsed = parse_evidence(&validation_value(), &["validation"], &["validate"]).unwrap();
        assert_eq!(
            correlate(&parsed, Some(&expected("validate"))),
            EvidenceBindingState::Current
        );
        assert_eq!(parsed.next_actions.len(), 1);
        assert_eq!(
            parsed.next_actions[0].code,
            EvidenceActionCode::ReviewWarnings
        );
    }

    #[test]
    fn mismatch_precedes_missing_or_stale_context() {
        let parsed = parse_evidence(&validation_value(), &["validation"], &["validate"]).unwrap();
        let mut expected = expected("validate");
        expected.current.project_id = "other-project".into();
        expected.operations.push(PersistedEvidenceOperation {
            id: "33333333-3333-3333-3333-333333333333".into(),
            kind: "up".into(),
            timestamp: "2026-09-22T04:53:46Z".into(),
            context: None,
        });
        assert_eq!(
            correlate(&parsed, Some(&expected)),
            EvidenceBindingState::Mismatch
        );
    }

    #[test]
    fn later_mutation_makes_an_exact_operation_stale() {
        let parsed = parse_evidence(&validation_value(), &["validation"], &["validate"]).unwrap();
        let mut expected = expected("validate");
        expected.operations.push(PersistedEvidenceOperation {
            id: "33333333-3333-3333-3333-333333333333".into(),
            kind: "up".into(),
            timestamp: "2026-09-22T04:53:46Z".into(),
            context: Some(context("up")),
        });
        assert_eq!(
            correlate(&parsed, Some(&expected)),
            EvidenceBindingState::Stale
        );
    }

    #[test]
    fn missing_operation_context_and_local_revision_are_never_current() {
        let parsed = parse_evidence(&validation_value(), &["validation"], &["validate"]).unwrap();
        assert_eq!(
            correlate(
                &parsed,
                Some(&ExpectedEvidenceContext {
                    current: context("validate"),
                    operations: Vec::new(),
                })
            ),
            EvidenceBindingState::InsufficientContext
        );
        let mut value = validation_value();
        value["binding"]["source"]["revision"] = json!("local");
        let parsed = parse_evidence(&value, &["validation"], &["validate"]).unwrap();
        assert_eq!(
            correlate(&parsed, Some(&expected("validate"))),
            EvidenceBindingState::InsufficientContext
        );
    }

    #[test]
    fn rejects_unknown_actions_and_reports_future_namespace_versions() {
        let mut value = validation_value();
        value["nextActions"][0]["code"] = json!("openRawUrl");
        assert!(matches!(
            parse_evidence(&value, &["validation"], &["validate"]),
            Err(EvidenceParseError::Invalid)
        ));
        let mut value = validation_value();
        value["version"] = json!("2");
        assert!(matches!(
            parse_evidence(&value, &["validation"], &["validate"]),
            Err(EvidenceParseError::FutureVersion)
        ));
    }

    #[test]
    fn reads_only_allowlisted_target_values_from_the_selected_environment() {
        let root = tempdir().unwrap();
        let environment = root.path().join(".azure/dev");
        fs::create_dir_all(&environment).unwrap();
        fs::write(
            environment.join(".env"),
            "AZD_GUI_AZURE_CLOUD=AzureCloud\nAZURE_TENANT_ID=00000000-0000-0000-0000-000000000000\nAZURE_SUBSCRIPTION_ID=11111111-1111-1111-1111-111111111111\nAZURE_LOCATION=westus2\nAZURE_RESOURCE_GROUP='rg-example-dev'\nSECRET=do-not-read\n",
        )
        .unwrap();
        let values = read_target_values(root.path(), "dev");
        assert_eq!(values.azure_cloud.as_deref(), Some("AzureCloud"));
        assert_eq!(values.location.as_deref(), Some("westus2"));
        assert_eq!(values.resource_group.as_deref(), Some("rg-example-dev"));
        assert_eq!(
            values.subscription_id.as_deref(),
            Some("11111111-1111-1111-1111-111111111111")
        );
    }
}
