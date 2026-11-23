use super::*;
use crate::{
    command_runner::{spawn_native_operation, OperationEvents, OperationLaunch, OperationManager},
    models::{CatalogEntry, ProjectSummary, ProjectTemplateSnapshot},
};
use serde_json::{json, Value};
use std::{fs, path::PathBuf, sync::Mutex, time::Duration};

const REVISION: &str = "97054a7c8bb67c4a4de52caccb68442c1e1ce769";
const SAVED_TARGET: &str = "AZD_GUI_AZURE_CLOUD=AzureCloud\nAZURE_TENANT_ID=11111111-1111-4111-8111-111111111111\nAZURE_SUBSCRIPTION_ID=22222222-2222-4222-8222-222222222222\nAZURE_RESOURCE_GROUP=rg-fixture\n";
const OUTPUT: &str = include_str!("../../../tests/fixtures/azd-pim-workflow-host-observation.json");
const EMIT: &str = concat!(
    "[Console]::Out.WriteLine('",
    include_str!("../../../tests/fixtures/azd-pim-workflow-host-observation.json"),
    "'); [Console]::Error.WriteLine('private fixture diagnostics'); exit 0"
);
const PARTIAL: &str = concat!(
    "[Console]::Out.WriteLine('",
    include_str!("../../../tests/fixtures/azd-pim-workflow-host-observation.json"),
    "'); [Console]::Error.WriteLine('synthetic failure after role linking'); exit 9"
);

struct Fixture {
    _root: tempfile::TempDir,
    database: Arc<Database>,
    project: ProjectSummary,
    snapshot: ProjectTemplateSnapshot,
    operations: OperationManager,
}

impl Fixture {
    fn new() -> Self {
        let root = tempfile::tempdir().unwrap();
        let database = Arc::new(Database::open(&root.path().join("state.sqlite")).unwrap());
        let project_path = root.path().join("project");
        let saved = project_path.join(".azure/dev");
        fs::create_dir_all(&saved).unwrap();
        fs::write(saved.join(".env"), SAVED_TARGET).unwrap();
        let path = fs::canonicalize(project_path)
            .unwrap()
            .display()
            .to_string();
        let project = ProjectSummary {
            id: "project-fixture".into(),
            name: "Fixture".into(),
            path,
            environment: Some("dev".into()),
            template_id: Some("pim-fixture".into()),
            revision: Some(REVISION.into()),
            last_operation: None,
            state: "ready".into(),
        };
        let entry: CatalogEntry = serde_json::from_value(json!({
            "id":"pim-fixture","title":"Fixture","description":"","summary":"","repository":REPOSITORY,
            "commit":REVISION,"tags":[],"platforms":["windows"],"trusted":true,
            "manifest":{
                "schemaVersion":"1.0","prerequisites":[],"connections":[],
                "applicationOwnership":{"kind":"verifier","provider":"entra-application-registration","objectIdField":"object"},
                "configuration":{"groups":[{"id":"ownership","title":"Ownership","fields":[
                    {"id":"object","environmentVariable":"AZD_PIM_REVOCATION_APPLICATION_OBJECT_ID","type":"entraObject","label":"Application object ID"}
                ]}]}
            }
        })).unwrap();
        let snapshot = ProjectTemplateSnapshot::new(entry).unwrap();
        database.upsert_project(&project).unwrap();
        database.save_project_template(&project, &snapshot).unwrap();
        Self {
            _root: root,
            database,
            project,
            snapshot,
            operations: OperationManager::default(),
        }
    }

    fn saved(&self) -> PathBuf {
        Path::new(&self.project.path).join(".azure/dev/.env")
    }

    fn persistence(&self) -> OperationPersistence {
        OperationPersistence::with_context(
            self.database.clone(),
            self.project.path.clone(),
            crate::operation_evidence_context(&self.project, &self.snapshot, "dev"),
        )
    }

    fn launch(&self, capture: Capture, events: OperationEvents) -> AppResult<String> {
        spawn_native_operation(
            events,
            self.operations.clone(),
            "up".into(),
            OperationLaunch::ReviewedWorkflow(capture),
            Some(self.persistence()),
        )
    }

    fn assert_operation(&self, id: &str, state: &str) {
        let history = self.database.operation_history(&self.project.path).unwrap();
        assert!(history
            .iter()
            .any(|entry| entry.operation_id == id && entry.state == state));
    }
}

fn capture(script: &'static str) -> (Capture, oneshot::Receiver<Receipt>) {
    let output: Observation = serde_json::from_str(OUTPUT).unwrap();
    let (completion, receiver) = oneshot::channel();
    (
        Capture {
            producer_revision: REVISION,
            producer_sha256: hex::encode(Sha256::digest(script.as_bytes())),
            script,
            workflow_resource_id: output.workflow_resource_id,
            completion,
        },
        receiver,
    )
}

async fn result(receiver: oneshot::Receiver<Receipt>) -> Option<Receipt> {
    tokio::time::timeout(Duration::from_secs(15), receiver)
        .await
        .expect("synthetic workflow operation did not finish")
        .ok()
}

fn record_events() -> (OperationEvents, Arc<Mutex<Vec<OperationEvent>>>) {
    let events = Arc::new(Mutex::new(Vec::new()));
    let recorded = events.clone();
    (
        Arc::new(move |event| recorded.lock().unwrap().push(event)),
        events,
    )
}

#[tokio::test]
async fn captures_only_after_the_exact_deployment_is_persisted() {
    let fixture = Fixture::new();
    let (capture, receiver) = capture(EMIT);
    let producer_sha256 = capture.producer_sha256.clone();
    let (sink, events) = record_events();
    let id = fixture.launch(capture, sink).unwrap();
    let mut receipt = result(receiver).await.unwrap();
    fixture.assert_operation(&id, "succeeded");
    assert_eq!(receipt.operation_id, id);
    assert_eq!(receipt.project_path, fixture.project.path);
    assert_eq!(receipt.context, fixture.persistence().context.unwrap());
    assert_eq!(receipt.context.revision.as_deref(), Some(REVISION));
    assert_eq!(receipt.producer_sha256, producer_sha256);
    let fixture_output: Observation = serde_json::from_str(OUTPUT).unwrap();
    assert_eq!(
        receipt.observation.workflow_resource_id,
        fixture_output.workflow_resource_id
    );
    assert_eq!(
        receipt.current_hostname(&fixture.database),
        Some(fixture_output.hostname.as_str())
    );
    let emitted = serde_json::to_string(&*events.lock().unwrap()).unwrap();
    assert!(!emitted.contains(&fixture_output.hostname));
    assert!(!emitted.contains("private fixture diagnostics"));

    fixture
        .database
        .begin_operation(
            "99999999-9999-4999-8999-999999999999",
            "down",
            &fixture.project.path,
            None,
        )
        .unwrap();
    assert!(receipt.current_hostname(&fixture.database).is_none());
}

#[tokio::test]
async fn pair_observed_before_later_optional_failure_yields_no_receipt() {
    let fixture = Fixture::new();
    let reports = Path::new(&fixture.project.path).join("reports");
    fs::create_dir(&reports).unwrap();
    let partial = reports.join("azd-pim-applied.json");
    fs::write(&partial, br#"{"deploymentStatus":"partial","optionalFailurePhase":"sessionRevocation","coreTenantConfigurationApplied":true}"#).unwrap();
    let (capture, receiver) = capture(PARTIAL);
    let (sink, events) = record_events();
    let id = fixture.launch(capture, sink).unwrap();
    assert!(result(receiver).await.is_none());
    fixture.assert_operation(&id, "failed");
    assert!(partial.exists());
    let emitted = serde_json::to_string(&*events.lock().unwrap()).unwrap();
    assert!(!emitted.contains("example.logic.azure.com"));
    assert!(!emitted.contains("synthetic failure after role linking"));
}

#[tokio::test]
async fn rejects_another_resource_in_the_same_group_and_changed_environment() {
    let fixture = Fixture::new();
    let (mut candidate, receiver) = capture(EMIT);
    candidate.workflow_resource_id = candidate.workflow_resource_id.replace("-example", "-other");
    let id = fixture.launch(candidate, Arc::new(|_| {})).unwrap();
    assert!(result(receiver).await.is_none());
    fixture.assert_operation(&id, "succeeded");

    let (candidate, receiver) = capture(EMIT);
    let saved = fixture.saved();
    let id = fixture
        .launch(
            candidate,
            Arc::new(move |event| {
                if event.state == "running" && event.stream == "system" {
                    fs::write(&saved, SAVED_TARGET.replace("rg-fixture", "rg-drift")).unwrap();
                }
            }),
        )
        .unwrap();
    assert!(result(receiver).await.is_none());
    fixture.assert_operation(&id, "succeeded");
}

#[tokio::test]
async fn captured_hostname_is_revoked_on_observed_source_or_environment_change() {
    let fixture = Fixture::new();
    let (candidate, receiver) = capture(EMIT);
    fixture.launch(candidate, Arc::new(|_| {})).unwrap();
    let mut receipt = result(receiver).await.unwrap();
    fixture
        .database
        .set_project_environment(&fixture.project.path, "other")
        .unwrap();
    assert!(receipt.current_hostname(&fixture.database).is_none());
    fixture
        .database
        .set_project_environment(&fixture.project.path, "dev")
        .unwrap();
    assert!(receipt.current_hostname(&fixture.database).is_none());

    let (candidate, receiver) = capture(EMIT);
    fixture.launch(candidate, Arc::new(|_| {})).unwrap();
    let mut receipt = result(receiver).await.unwrap();
    let mut entry = fixture.snapshot.entry.clone();
    entry.summary = "Changed source contract".into();
    let changed = ProjectTemplateSnapshot::new(entry).unwrap();
    fixture
        .database
        .save_project_template(&fixture.project, &changed)
        .unwrap();
    assert!(receipt.current_hostname(&fixture.database).is_none());
    fixture
        .database
        .save_project_template(&fixture.project, &fixture.snapshot)
        .unwrap();
    assert!(receipt.current_hostname(&fixture.database).is_none());
}

#[tokio::test]
async fn rejects_unpinned_or_foreign_target_before_launch_and_cancellation_drops_capture() {
    let fixture = Fixture::new();
    let (mut candidate, receiver) = capture(EMIT);
    candidate.producer_sha256 = "f".repeat(64);
    assert!(fixture
        .launch(candidate, Arc::new(|_| panic!("unexpected launch")))
        .is_err());
    assert!(result(receiver).await.is_none());

    let (mut candidate, receiver) = capture(EMIT);
    candidate.workflow_resource_id = candidate
        .workflow_resource_id
        .replace("rg-fixture", "other");
    assert!(fixture
        .launch(candidate, Arc::new(|_| panic!("unexpected launch")))
        .is_err());
    assert!(result(receiver).await.is_none());

    let (candidate, receiver) = capture("Start-Sleep -Seconds 5; exit 0");
    let id = fixture.launch(candidate, Arc::new(|_| {})).unwrap();
    assert!(fixture
        .operations
        .reserve("another-operation", false)
        .is_err());
    assert!(fixture.operations.cancel(&id).await);
    assert!(result(receiver).await.is_none());
    fixture.assert_operation(&id, "cancelled");
}

#[tokio::test]
async fn edited_project_outputs_cannot_supply_a_missing_producer_observation() {
    let fixture = Fixture::new();
    let reports = Path::new(&fixture.project.path).join("reports");
    fs::create_dir(&reports).unwrap();
    fs::write(reports.join("azd-pim-applied.json"), OUTPUT).unwrap();
    let output: Observation = serde_json::from_str(OUTPUT).unwrap();
    fs::write(fixture.saved(), format!("{SAVED_TARGET}AZD_PIM_REVOCATION_LOGIC_APP_RESOURCE_ID={}\nAZD_PIM_REVOCATION_WORKFLOW_HOST={}\n", output.workflow_resource_id, output.hostname)).unwrap();
    let (candidate, receiver) = capture("exit 0");
    let id = fixture.launch(candidate, Arc::new(|_| {})).unwrap();
    assert!(result(receiver).await.is_none());
    fixture.assert_operation(&id, "succeeded");
}

#[tokio::test]
async fn observation_requires_one_bounded_record_without_urls_or_bindings() {
    let good: Value = serde_json::from_str(OUTPUT).unwrap();
    assert!(read_observation(OUTPUT.as_bytes()).await.is_some());
    let mut extra = good.clone();
    extra["callbackUrl"] = json!("https://example.logic.azure.com/path?sig=fixture-only");
    let mut url = good;
    url["hostname"] = json!("https://example.logic.azure.com/path");
    for invalid in [
        extra.to_string(),
        url.to_string(),
        format!("{OUTPUT}{OUTPUT}"),
        format!("{OUTPUT}{}", " ".repeat(LIMIT)),
    ] {
        assert!(read_observation(invalid.as_bytes()).await.is_none());
    }
}
