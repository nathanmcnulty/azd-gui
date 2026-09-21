use crate::{
    evidence::{
        correlate, parse_evidence, EvidenceBindingState, EvidenceNextAction, EvidenceParseError,
        ExpectedEvidenceContext,
    },
    validation_report::{read_fixed_report_bytes, ValidationReportState},
};
use chrono::DateTime;
use serde::{Deserialize, Serialize};
use serde_json::Map;
use std::path::Path;

const RECEIPT_FILE: &str = "deployment-receipt.json";
const MAX_RECEIPT_BYTES: u64 = 1024 * 1024;
const MAX_ARTIFACTS: usize = 500;
const MAX_ACTIONS: usize = 500;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum DeploymentReceiptState {
    Available,
    NotFound,
    Invalid,
    UnsafePath,
    TooLarge,
    FutureVersion,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DeploymentReceiptRead {
    pub state: DeploymentReceiptState,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub receipt: Option<DeploymentReceipt>,
}

impl DeploymentReceiptRead {
    fn unavailable(state: DeploymentReceiptState) -> Self {
        Self {
            state,
            receipt: None,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DeploymentReceipt {
    pub generated_at: String,
    pub mode: ReceiptMode,
    pub summary: ReceiptSummary,
    pub artifact_count: usize,
    pub outstanding_action_count: usize,
    pub binding_state: EvidenceBindingState,
    pub next_actions: Vec<EvidenceNextAction>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum ReceiptMode {
    Plan,
    Enforce,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ReceiptSummary {
    pub applied: u32,
    pub unchanged: u32,
    pub skipped: u32,
    pub warnings: u32,
    pub failed: u32,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct RawDeploymentReceipt {
    schema_version: String,
    generated_at: String,
    template: String,
    template_version: String,
    mode: ReceiptMode,
    summary: ReceiptSummary,
    artifacts: Vec<String>,
    operational_actions: Vec<String>,
    details: Map<String, serde_json::Value>,
}

#[cfg(test)]
pub fn read(project_root: &Path) -> DeploymentReceiptRead {
    read_with_context(project_root, None)
}

pub fn read_with_context(
    project_root: &Path,
    expected: Option<&ExpectedEvidenceContext>,
) -> DeploymentReceiptRead {
    match read_inner(project_root, expected) {
        Ok(receipt) => DeploymentReceiptRead {
            state: DeploymentReceiptState::Available,
            receipt: Some(receipt),
        },
        Err(state) => DeploymentReceiptRead::unavailable(state),
    }
}

fn read_inner(
    project_root: &Path,
    expected: Option<&ExpectedEvidenceContext>,
) -> Result<DeploymentReceipt, DeploymentReceiptState> {
    let bytes = read_fixed_report_bytes(project_root, RECEIPT_FILE, MAX_RECEIPT_BYTES)
        .map_err(map_report_state)?;
    let value: serde_json::Value =
        serde_json::from_slice(&bytes).map_err(|_| DeploymentReceiptState::Invalid)?;
    let schema = value
        .get("schemaVersion")
        .and_then(serde_json::Value::as_str)
        .ok_or(DeploymentReceiptState::Invalid)?;
    if schema != "1.0" && schema != "1.1" {
        return Err(
            if schema
                .split_once('.')
                .and_then(|(major, minor)| {
                    Some((major.parse::<u32>().ok()?, minor.parse::<u32>().ok()?))
                })
                .is_some_and(|(major, minor)| major > 1 || major == 1 && minor > 1)
            {
                DeploymentReceiptState::FutureVersion
            } else {
                DeploymentReceiptState::Invalid
            },
        );
    }
    let raw: RawDeploymentReceipt =
        serde_json::from_value(value).map_err(|_| DeploymentReceiptState::Invalid)?;
    let generated_at = DateTime::parse_from_rfc3339(&raw.generated_at)
        .map_err(|_| DeploymentReceiptState::Invalid)?;

    if !matches!(raw.schema_version.as_str(), "1.0" | "1.1")
        || raw.template.is_empty()
        || raw.template_version.is_empty()
        || raw.artifacts.len() > MAX_ARTIFACTS
        || raw.operational_actions.len() > MAX_ACTIONS
        || matches!(raw.mode, ReceiptMode::Plan) && raw.summary.applied > 0
        || raw
            .artifacts
            .iter()
            .any(|path| !is_safe_artifact_path(path))
    {
        return Err(DeploymentReceiptState::Invalid);
    }

    let evidence_value = raw.details.get("azdManagementEvidence");
    let (binding_state, next_actions) = if raw.schema_version == "1.0" {
        if evidence_value.is_some() {
            return Err(DeploymentReceiptState::Invalid);
        }
        (EvidenceBindingState::LegacyUnbound, Vec::new())
    } else {
        let value = evidence_value.ok_or(DeploymentReceiptState::Invalid)?;
        let evidence = parse_evidence(
            value,
            &["resourceMutation", "cleanup"],
            &["provision", "deploy", "up", "down", "cleanup"],
        )
        .map_err(|error| match error {
            EvidenceParseError::Invalid => DeploymentReceiptState::Invalid,
            EvidenceParseError::FutureVersion => DeploymentReceiptState::FutureVersion,
        })?;
        let binding_state = correlate(&evidence, expected);
        (binding_state, evidence.next_actions)
    };
    Ok(DeploymentReceipt {
        generated_at: generated_at.to_rfc3339(),
        mode: raw.mode,
        summary: raw.summary,
        artifact_count: raw.artifacts.len(),
        outstanding_action_count: raw.operational_actions.len(),
        binding_state,
        next_actions,
    })
}

fn map_report_state(state: ValidationReportState) -> DeploymentReceiptState {
    match state {
        ValidationReportState::Available => DeploymentReceiptState::Available,
        ValidationReportState::NotFound => DeploymentReceiptState::NotFound,
        ValidationReportState::Invalid => DeploymentReceiptState::Invalid,
        ValidationReportState::UnsafePath => DeploymentReceiptState::UnsafePath,
        ValidationReportState::TooLarge => DeploymentReceiptState::TooLarge,
        ValidationReportState::FutureVersion => DeploymentReceiptState::FutureVersion,
    }
}

fn is_safe_artifact_path(path: &str) -> bool {
    let bytes = path.as_bytes();
    let windows_drive = bytes.len() >= 2 && bytes[1] == b':' && bytes[0].is_ascii_alphabetic();
    !path.is_empty()
        && !path.starts_with('/')
        && !windows_drive
        && !path.contains('\\')
        && !path.chars().any(|character| character <= '\u{1f}')
        && !path
            .split('/')
            .any(|segment| segment == "." || segment == "..")
}

#[cfg(test)]
mod tests {
    use super::{read, read_with_context, DeploymentReceiptState, MAX_RECEIPT_BYTES};
    use crate::evidence::{
        ExpectedEvidenceContext, OperationEvidenceContext, PersistedEvidenceOperation,
    };
    use serde_json::json;
    use std::fs;
    use tempfile::tempdir;

    fn fixture() -> serde_json::Value {
        serde_json::from_slice(include_bytes!("../tests/fixtures/deployment-receipt.json")).unwrap()
    }

    fn write_receipt(root: &std::path::Path, value: serde_json::Value) {
        let reports = root.join("reports");
        fs::create_dir_all(&reports).unwrap();
        fs::write(
            reports.join("deployment-receipt.json"),
            serde_json::to_vec(&value).unwrap(),
        )
        .unwrap();
    }

    fn expected() -> ExpectedEvidenceContext {
        let context = OperationEvidenceContext {
            project_id: "project-example".into(),
            environment: "dev".into(),
            azure_cloud: Some("AzureCloud".into()),
            tenant_id: Some("00000000-0000-0000-0000-000000000000".into()),
            subscription_id: Some("11111111-1111-1111-1111-111111111111".into()),
            resource_group: Some("rg-example-dev".into()),
            template_id: "example".into(),
            revision: Some("0123456789abcdef0123456789abcdef01234567".into()),
            contract_digest: Some("a".repeat(64)),
        };
        ExpectedEvidenceContext {
            current: context.clone(),
            operations: vec![PersistedEvidenceOperation {
                id: "22222222-2222-2222-2222-222222222222".into(),
                kind: "up".into(),
                timestamp: "2026-09-20T12:00:00Z".into(),
                context: Some(context),
            }],
        }
    }

    #[test]
    fn round_trips_the_pinned_bound_producer_fixture() {
        let root = tempdir().unwrap();
        let value = serde_json::from_str(include_str!(
            "../tests/fixtures/deployment-receipt-bound-v1.1.json"
        ))
        .unwrap();
        write_receipt(root.path(), value);
        let result = read_with_context(root.path(), Some(&expected()));
        assert_eq!(result.state, DeploymentReceiptState::Available);
        let receipt = result.receipt.unwrap();
        assert_eq!(
            receipt.binding_state,
            crate::evidence::EvidenceBindingState::Current
        );
        assert_eq!(receipt.next_actions.len(), 1);
        let serialized = serde_json::to_string(&receipt).unwrap();
        assert!(!serialized.contains("11111111-1111"));
        assert!(!serialized.contains("rg-example-dev"));
    }

    #[test]
    fn distinguishes_future_outer_contracts() {
        let root = tempdir().unwrap();
        let mut value: serde_json::Value = serde_json::from_str(include_str!(
            "../tests/fixtures/deployment-receipt-bound-v1.1.json"
        ))
        .unwrap();
        value["schemaVersion"] = json!("2.0");
        write_receipt(root.path(), value);
        assert_eq!(
            read(root.path()).state,
            DeploymentReceiptState::FutureVersion
        );
    }

    #[test]
    fn projects_the_reference_receipt_without_disclosing_details() {
        let root = tempdir().unwrap();
        write_receipt(root.path(), fixture());

        let result = read(root.path());
        assert_eq!(result.state, DeploymentReceiptState::Available);
        let receipt = result.receipt.unwrap();
        assert_eq!(receipt.generated_at, "2026-08-22T12:00:00+00:00");
        assert!(matches!(receipt.mode, super::ReceiptMode::Enforce));
        assert_eq!(receipt.summary.applied, 1);
        assert_eq!(receipt.artifact_count, 1);
        assert_eq!(receipt.outstanding_action_count, 1);
        let serialized = serde_json::to_string(&receipt).unwrap();
        assert!(!serialized.contains("azd-example"));
        assert!(!serialized.contains("deployment-validation.json"));
        assert!(!serialized.contains("Prove live notification delivery"));
        assert!(!serialized.contains("feature"));
    }

    #[test]
    fn strips_secrets_and_paths_from_the_public_projection() {
        let root = tempdir().unwrap();
        let mut value = fixture();
        value["artifacts"] = json!(["reports/secret-token.json"]);
        value["operationalActions"] = json!(["Use secret-token only with tenant-secret."]);
        value["details"] = json!({ "clientSecret": "tenant-secret" });
        write_receipt(root.path(), value);

        let serialized = serde_json::to_string(&read(root.path()).receipt.unwrap()).unwrap();
        assert!(!serialized.contains("secret-token"));
        assert!(!serialized.contains("tenant-secret"));
        assert!(!serialized.contains("reports"));
    }

    #[test]
    fn rejects_invalid_contract_and_plan_application() {
        let root = tempdir().unwrap();
        let mut value = fixture();
        value["unexpected"] = json!(true);
        write_receipt(root.path(), value);
        assert_eq!(read(root.path()).state, DeploymentReceiptState::Invalid);

        let mut value = fixture();
        value["mode"] = json!("plan");
        write_receipt(root.path(), value);
        assert_eq!(read(root.path()).state, DeploymentReceiptState::Invalid);
    }

    #[test]
    fn accepts_a_plan_with_no_applied_actions() {
        let root = tempdir().unwrap();
        let mut value = fixture();
        value["mode"] = json!("plan");
        value["summary"]["applied"] = json!(0);
        write_receipt(root.path(), value);
        let receipt = read(root.path()).receipt.unwrap();
        assert!(matches!(receipt.mode, super::ReceiptMode::Plan));
        assert_eq!(receipt.summary.applied, 0);
    }

    #[test]
    fn rejects_absolute_traversal_and_excessive_artifact_paths() {
        let root = tempdir().unwrap();
        for path in [
            "/etc/passwd",
            "C:\\temp\\report.json",
            "reports/../secret.json",
        ] {
            let mut value = fixture();
            value["artifacts"] = json!([path]);
            write_receipt(root.path(), value);
            assert_eq!(read(root.path()).state, DeploymentReceiptState::Invalid);
        }

        let mut value = fixture();
        value["artifacts"] = json!(vec!["reports/item.json"; 501]);
        write_receipt(root.path(), value);
        assert_eq!(read(root.path()).state, DeploymentReceiptState::Invalid);

        let mut value = fixture();
        value["operationalActions"] = json!(vec!["action"; 501]);
        write_receipt(root.path(), value);
        assert_eq!(read(root.path()).state, DeploymentReceiptState::Invalid);
    }

    #[test]
    fn distinguishes_missing_and_oversized_receipts() {
        let root = tempdir().unwrap();
        assert_eq!(read(root.path()).state, DeploymentReceiptState::NotFound);
        fs::create_dir_all(root.path().join("reports")).unwrap();
        fs::write(
            root.path().join("reports/deployment-receipt.json"),
            vec![b'x'; MAX_RECEIPT_BYTES as usize + 1],
        )
        .unwrap();
        assert_eq!(read(root.path()).state, DeploymentReceiptState::TooLarge);
    }

    #[cfg(unix)]
    #[test]
    fn rejects_a_symlinked_report_directory() {
        use std::os::unix::fs::symlink;

        let root = tempdir().unwrap();
        let target = tempdir().unwrap();
        fs::write(
            target.path().join("deployment-receipt.json"),
            fixture().to_string(),
        )
        .unwrap();
        symlink(target.path(), root.path().join("reports")).unwrap();
        assert_eq!(read(root.path()).state, DeploymentReceiptState::UnsafePath);
    }

    #[cfg(windows)]
    #[test]
    fn rejects_a_windows_junction_report_directory() {
        use std::os::windows::process::CommandExt;
        let root = tempdir().unwrap();
        let target = tempdir().unwrap();
        fs::write(
            target.path().join("deployment-receipt.json"),
            fixture().to_string(),
        )
        .unwrap();
        let status = std::process::Command::new("powershell.exe")
            .creation_flags(0x08000000)
            .args(["-NoProfile", "-NonInteractive", "-Command",
                "$ErrorActionPreference='Stop'; New-Item -ItemType Junction -Path $env:AZD_GUI_RECEIPT_TEST_LINK -Target $env:AZD_GUI_RECEIPT_TEST_TARGET | Out-Null"])
            .env("AZD_GUI_RECEIPT_TEST_LINK", root.path().join("reports"))
            .env("AZD_GUI_RECEIPT_TEST_TARGET", target.path())
            .status()
            .unwrap();
        assert!(status.success());
        assert_eq!(read(root.path()).state, DeploymentReceiptState::UnsafePath);
    }
}
