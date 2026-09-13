use chrono::{DateTime, FixedOffset};
use once_cell::sync::Lazy;
use regex::Regex;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use std::{fs, path::Path};

const REPORT_DIRECTORY: &str = "reports";
const REPORT_FILE: &str = "deployment-validation.json";
const MAX_REPORT_BYTES: u64 = 1024 * 1024;
const MAX_CHECKS: usize = 500;
const MAX_CHECK_ID_LENGTH: usize = 96;

static CHECK_ID: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"^[a-z][a-z0-9.-]+$").expect("valid check identifier pattern"));

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum ValidationReportState {
    Available,
    NotFound,
    Invalid,
    UnsafePath,
    TooLarge,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ValidationReportRead {
    pub state: ValidationReportState,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub report: Option<ValidationReport>,
}

impl ValidationReportRead {
    fn unavailable(state: ValidationReportState) -> Self {
        Self {
            state,
            report: None,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ValidationReport {
    pub started_at: String,
    pub completed_at: String,
    pub duration_ms: u32,
    pub mode: ValidationMode,
    pub outcome: ValidationOutcome,
    pub summary: ValidationSummary,
    pub environment_binding: EnvironmentBinding,
    pub checks: Vec<ValidationCheck>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ValidationMode {
    Plan,
    Verify,
    Delivery,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ValidationOutcome {
    Planned,
    Passed,
    PassedWithWarnings,
    Failed,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ValidationPhase {
    Context,
    Infrastructure,
    Identity,
    Configuration,
    Security,
    Runtime,
    Delivery,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ValidationStatus {
    Pass,
    Fail,
    Warning,
    Info,
    Skipped,
    Planned,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ValidationSideEffect {
    None,
    ReadOnly,
    NegativeProbe,
    SyntheticDelivery,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum EnvironmentBinding {
    #[serde(rename = "environmentMatch")]
    Match,
    #[serde(rename = "environmentMismatch")]
    Mismatch,
    #[serde(rename = "environmentUnbound")]
    Unbound,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ValidationSummary {
    pub pass: u32,
    pub fail: u32,
    pub warning: u32,
    pub info: u32,
    pub skipped: u32,
    pub planned: u32,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ValidationCheck {
    pub id: String,
    pub phase: ValidationPhase,
    pub status: ValidationStatus,
    pub side_effect: ValidationSideEffect,
    pub duration_ms: u32,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct RawValidationReport {
    schema_version: String,
    report_type: String,
    started_at: String,
    completed_at: String,
    duration_ms: u32,
    template: RawTemplate,
    environment: Option<RawEnvironment>,
    mode: ValidationMode,
    requirements: RawRequirements,
    outcome: ValidationOutcome,
    summary: ValidationSummary,
    checks: Vec<RawValidationCheck>,
    next_steps: Vec<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct RawTemplate {
    name: String,
    version: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct RawEnvironment {
    name: Option<String>,
    tenant_id: Option<String>,
    subscription_id: Option<String>,
    resource_group: Option<String>,
    azure_cloud: Option<String>,
    graph_environment: Option<String>,
    metadata: Option<Map<String, Value>>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct RawRequirements {
    tools: Vec<String>,
    modules: Vec<String>,
    permissions: Vec<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct RawValidationCheck {
    id: String,
    phase: ValidationPhase,
    title: String,
    status: ValidationStatus,
    summary: String,
    evidence: Map<String, Value>,
    remediation: Option<String>,
    expected: Option<Value>,
    actual: Option<Value>,
    metadata: Option<Map<String, Value>>,
    duration_ms: u32,
    side_effect: ValidationSideEffect,
}

pub fn read(project_root: &Path, selected_environment: Option<&str>) -> ValidationReportRead {
    match read_inner(project_root, selected_environment) {
        Ok(report) => ValidationReportRead {
            state: ValidationReportState::Available,
            report: Some(report),
        },
        Err(state) => ValidationReportRead::unavailable(state),
    }
}

fn read_inner(
    project_root: &Path,
    selected_environment: Option<&str>,
) -> Result<ValidationReport, ValidationReportState> {
    if is_link_or_reparse(project_root).map_err(|_| ValidationReportState::Invalid)? {
        return Err(ValidationReportState::UnsafePath);
    }
    let root = fs::canonicalize(project_root).map_err(|_| ValidationReportState::Invalid)?;
    if !root.is_dir() {
        return Err(ValidationReportState::Invalid);
    }

    let reports = root.join(REPORT_DIRECTORY);
    let report_path = reports.join(REPORT_FILE);
    let report_metadata = safe_report_metadata(&reports, &report_path)?;
    if report_metadata.len() > MAX_REPORT_BYTES {
        return Err(ValidationReportState::TooLarge);
    }

    let bytes = fs::read(&report_path).map_err(|_| ValidationReportState::Invalid)?;
    if bytes.len() as u64 > MAX_REPORT_BYTES {
        return Err(ValidationReportState::TooLarge);
    }

    // Recheck after reading so a replacement cannot turn this fixed child path into a link.
    let _ = safe_report_metadata(&reports, &report_path)?;
    let canonical_report =
        fs::canonicalize(&report_path).map_err(|_| ValidationReportState::Invalid)?;
    if !canonical_report.starts_with(&root) {
        return Err(ValidationReportState::UnsafePath);
    }

    let raw: RawValidationReport =
        serde_json::from_slice(&bytes).map_err(|_| ValidationReportState::Invalid)?;
    project_report(raw, selected_environment).ok_or(ValidationReportState::Invalid)
}

fn safe_report_metadata(
    reports: &Path,
    report_path: &Path,
) -> Result<fs::Metadata, ValidationReportState> {
    match fs::symlink_metadata(reports) {
        Ok(metadata) => {
            if is_link_or_reparse(reports).map_err(|_| ValidationReportState::Invalid)? {
                return Err(ValidationReportState::UnsafePath);
            }
            if !metadata.is_dir() {
                return Err(ValidationReportState::Invalid);
            }
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Err(ValidationReportState::NotFound);
        }
        Err(_) => return Err(ValidationReportState::Invalid),
    }

    let metadata = match fs::symlink_metadata(report_path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Err(ValidationReportState::NotFound);
        }
        Err(_) => return Err(ValidationReportState::Invalid),
    };
    if is_link_or_reparse(report_path).map_err(|_| ValidationReportState::Invalid)? {
        return Err(ValidationReportState::UnsafePath);
    }
    if !metadata.is_file() {
        return Err(ValidationReportState::Invalid);
    }
    Ok(metadata)
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

fn project_report(
    raw: RawValidationReport,
    selected_environment: Option<&str>,
) -> Option<ValidationReport> {
    if raw.schema_version != "1.0"
        || raw.report_type != "azdDeploymentValidation"
        || raw.template.name.is_empty()
        || raw.template.version.is_empty()
        || raw.checks.len() > MAX_CHECKS
    {
        return None;
    }
    let started_at = DateTime::<FixedOffset>::parse_from_rfc3339(&raw.started_at)
        .ok()?
        .to_rfc3339();
    let completed_at = DateTime::<FixedOffset>::parse_from_rfc3339(&raw.completed_at)
        .ok()?
        .to_rfc3339();

    let checks = raw
        .checks
        .into_iter()
        .map(|check| {
            if check.id.len() > MAX_CHECK_ID_LENGTH
                || !CHECK_ID.is_match(&check.id)
                || check.title.is_empty()
                || check.summary.is_empty()
            {
                return None;
            }
            let _ = (
                check.evidence,
                check.remediation,
                check.expected,
                check.actual,
                check.metadata,
            );
            Some(ValidationCheck {
                id: check.id,
                phase: check.phase,
                status: check.status,
                side_effect: check.side_effect,
                duration_ms: check.duration_ms,
            })
        })
        .collect::<Option<Vec<_>>>()?;

    // Parsing these fields validates the envelope but deliberately does not disclose them.
    let _ = (
        raw.requirements.tools,
        raw.requirements.modules,
        raw.requirements.permissions,
        raw.next_steps,
    );
    let environment_binding = match (
        selected_environment.filter(|value| !value.is_empty()),
        raw.environment.and_then(|environment| {
            let _ = (
                environment.tenant_id,
                environment.subscription_id,
                environment.resource_group,
                environment.azure_cloud,
                environment.graph_environment,
                environment.metadata,
            );
            environment.name.filter(|value| !value.is_empty())
        }),
    ) {
        (Some(selected), Some(report_environment)) if selected == report_environment => {
            EnvironmentBinding::Match
        }
        (Some(_), Some(_)) => EnvironmentBinding::Mismatch,
        _ => EnvironmentBinding::Unbound,
    };

    Some(ValidationReport {
        started_at,
        completed_at,
        duration_ms: raw.duration_ms,
        mode: raw.mode,
        outcome: raw.outcome,
        summary: raw.summary,
        environment_binding,
        checks,
    })
}

#[cfg(test)]
mod tests {
    use super::{read, ValidationReportState, MAX_REPORT_BYTES};
    use serde_json::json;
    use std::fs;
    use tempfile::tempdir;

    fn report(environment: &str) -> serde_json::Value {
        json!({
            "schemaVersion": "1.0",
            "reportType": "azdDeploymentValidation",
            "startedAt": "2026-09-12T12:00:00Z",
            "completedAt": "2026-09-12T12:00:01Z",
            "durationMs": 1000,
            "template": { "name": "sample", "version": "0.1.0" },
            "environment": { "name": environment, "tenantId": "tenant-secret" },
            "mode": "verify",
            "requirements": { "tools": ["az"], "modules": [], "permissions": [] },
            "outcome": "passed",
            "summary": { "pass": 1, "fail": 0, "warning": 0, "info": 0, "skipped": 0, "planned": 0 },
            "checks": [{
                "id": "infrastructure.resource-group",
                "phase": "infrastructure",
                "title": "Bearer secret-value",
                "status": "pass",
                "summary": "CLIENT_SECRET=secret-value",
                "evidence": { "token": "secret-value" },
                "expected": "secret-value",
                "actual": "secret-value",
                "metadata": { "connectionString": "secret-value" },
                "durationMs": 5,
                "sideEffect": "readOnly"
            }],
            "nextSteps": ["https://example.invalid/?sig=secret-value"]
        })
    }

    fn write_report(root: &std::path::Path, value: serde_json::Value) {
        let reports = root.join("reports");
        fs::create_dir_all(&reports).unwrap();
        fs::write(
            reports.join("deployment-validation.json"),
            serde_json::to_vec(&value).unwrap(),
        )
        .unwrap();
    }

    #[test]
    fn projects_only_safe_contract_fields() {
        let root = tempdir().unwrap();
        write_report(root.path(), report("dev"));

        let result = read(root.path(), Some("dev"));
        assert!(matches!(result.state, ValidationReportState::Available));
        let serialized = serde_json::to_string(&result).unwrap();
        assert!(serialized.contains("infrastructure.resource-group"));
        assert!(!serialized.contains("secret-value"));
        assert!(!serialized.contains("tenant-secret"));
        assert_eq!(
            result.report.unwrap().environment_binding,
            super::EnvironmentBinding::Match
        );
    }

    #[test]
    fn distinguishes_missing_invalid_and_oversized_reports() {
        let root = tempdir().unwrap();
        assert!(matches!(
            read(root.path(), Some("dev")).state,
            ValidationReportState::NotFound
        ));

        write_report(root.path(), json!({ "schemaVersion": "0.9" }));
        assert!(matches!(
            read(root.path(), Some("dev")).state,
            ValidationReportState::Invalid
        ));

        fs::write(
            root.path().join("reports/deployment-validation.json"),
            vec![b'x'; MAX_REPORT_BYTES as usize + 1],
        )
        .unwrap();
        assert!(matches!(
            read(root.path(), Some("dev")).state,
            ValidationReportState::TooLarge
        ));
    }

    #[test]
    fn reports_environment_mismatches_without_disclosing_names() {
        let root = tempdir().unwrap();
        write_report(root.path(), report("other"));

        let result = read(root.path(), Some("dev"));
        assert!(matches!(result.state, ValidationReportState::Available));
        let serialized = serde_json::to_string(&result).unwrap();
        assert!(serialized.contains("environmentMismatch"));
        assert!(!serialized.contains("other"));
    }

    #[test]
    fn projects_the_reference_plan_fixture_without_exposing_report_text() {
        let root = tempdir().unwrap();
        let reports = root.path().join("reports");
        fs::create_dir_all(&reports).unwrap();
        fs::write(
            reports.join("deployment-validation.json"),
            include_bytes!("../tests/fixtures/reference-plan.json"),
        )
        .unwrap();

        let result = read(root.path(), Some("gui-plan-fixture"));
        assert!(matches!(result.state, ValidationReportState::Available));
        let report = result.report.unwrap();
        assert!(matches!(report.mode, super::ValidationMode::Plan));
        assert!(matches!(report.outcome, super::ValidationOutcome::Planned));
        assert_eq!(report.summary.planned, 3);
        assert_eq!(report.summary.skipped, 1);
        assert_eq!(report.checks.len(), 4);
        assert_eq!(report.environment_binding, super::EnvironmentBinding::Match);
        assert!(!serde_json::to_string(&report)
            .unwrap()
            .contains("gui-plan-fixture"));
    }

    #[cfg(unix)]
    #[test]
    fn rejects_a_symlinked_report_directory() {
        use std::os::unix::fs::symlink;

        let root = tempdir().unwrap();
        let target = tempdir().unwrap();
        fs::write(target.path().join("deployment-validation.json"), b"{}").unwrap();
        symlink(target.path(), root.path().join("reports")).unwrap();

        assert!(matches!(
            read(root.path(), Some("dev")).state,
            ValidationReportState::UnsafePath
        ));
    }
}
