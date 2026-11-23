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
const WORKFLOW: &str = "/subscriptions/22222222-2222-4222-8222-222222222222/resourceGroups/rg-fixture/providers/Microsoft.Logic/workflows/pim-revoke-sessions-example";
const APP_OBJECT: &str = "aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa";
const APP_CLIENT: &str = "bbbbbbbb-bbbb-4bbb-8bbb-bbbbbbbbbbbb";

fn saved_outputs() -> String {
    format!("{WORKFLOW_ID}={WORKFLOW}\n{APPLICATION_OBJECT_ID}={APP_OBJECT}\n{APPLICATION_CLIENT_ID}={APP_CLIENT}\n")
}
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
        Self::with_revision(REVISION)
    }

    fn with_revision(revision: &str) -> Self {
        let root = tempfile::tempdir().unwrap();
        let database = Arc::new(Database::open(&root.path().join("state.sqlite")).unwrap());
        let project_path = root.path().join("project");
        let saved = project_path.join(".azure/dev");
        fs::create_dir_all(&saved).unwrap();
        fs::create_dir_all(project_path.join(".azure/other")).unwrap();
        fs::write(
            saved.join(".env"),
            format!("{SAVED_TARGET}{}", saved_outputs()),
        )
        .unwrap();
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
            revision: Some(revision.into()),
            last_operation: None,
            state: "ready".into(),
        };
        let entry: CatalogEntry = serde_json::from_value(json!({
            "id":"pim-fixture","title":"Fixture","description":"","summary":"","repository":REPOSITORY,
            "commit":revision,"tags":[],"platforms":["windows"],"trusted":true,
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

    fn app_state(&self) -> crate::AppState {
        crate::AppState {
            database: self.database.clone(),
            operations: self.operations.clone(),
            environment_writes: Default::default(),
            application_reviews: Default::default(),
            template_cache: self._root.path().join("cache"),
            inspected_manifests: Default::default(),
            reviewed_contracts: Default::default(),
            catalog_reviews: Default::default(),
            execution_approvals: Default::default(),
            tool_install_approvals: Default::default(),
            graph_sessions: Default::default(),
        }
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

fn capture(script: impl Into<String>) -> (Capture, oneshot::Receiver<Receipt>) {
    let script = script.into();
    let output: Observation = serde_json::from_str(OUTPUT).unwrap();
    let (completion, receiver) = oneshot::channel();
    (
        Capture {
            producer_revision: REVISION,
            producer_sha256: hex::encode(Sha256::digest(script.as_bytes())),
            script,
            workflow_resource_id: output.workflow_resource_id,
            completion,
            offline_powershell: None,
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
                    let changed_target = SAVED_TARGET.replace("rg-fixture", "rg-drift");
                    fs::write(&saved, format!("{changed_target}{}", saved_outputs())).unwrap();
                }
            }),
        )
        .unwrap();
    assert!(result(receiver).await.is_none());
    fixture.assert_operation(&id, "succeeded");
}

fn writes_output(path: &Path, body: &str) -> String {
    let quote = |value: &str| value.replace('\'', "''");
    format!(
        "[IO.File]::WriteAllText('{}', '{}'); {EMIT}",
        quote(&path.display().to_string()),
        quote(body)
    )
}

#[tokio::test]
async fn reviewed_output_writes_are_allowed_and_unrelated_saved_edits_are_rejected() {
    let fixture = Fixture::new();
    let path = fixture.saved();
    fs::write(&path, SAVED_TARGET).unwrap();
    let final_body = format!(
        "{SAVED_TARGET}{}AZD_PIM_REVOCATION_LOGIC_APP_NAME=fixture\n",
        saved_outputs()
    );
    let (candidate, receiver) = capture(writes_output(&path, &final_body));
    let id = fixture.launch(candidate, Arc::new(|_| {})).unwrap();
    let mut receipt = result(receiver).await.unwrap();
    fixture.assert_operation(&id, "succeeded");
    assert_eq!(
        receipt.current_hostname(&fixture.database),
        Some("example.logic.azure.com")
    );
    assert_eq!(fs::read_to_string(&path).unwrap(), final_body);

    // Even a valid record and expected saved workflow ID cannot cover another input edit.
    fs::write(&path, SAVED_TARGET).unwrap();
    let (candidate, receiver) = capture(writes_output(
        &path,
        &format!("{final_body}AZURE_LOCATION=unknown-drift\n"),
    ));
    let id = fixture.launch(candidate, Arc::new(|_| {})).unwrap();
    assert!(result(receiver).await.is_none());
    fixture.assert_operation(&id, "succeeded");

    fs::write(&path, SAVED_TARGET).unwrap();
    let wrong_id = final_body.replace("pim-revoke-sessions-example", "pim-revoke-sessions-other");
    let (candidate, receiver) = capture(writes_output(&path, &wrong_id));
    let id = fixture.launch(candidate, Arc::new(|_| {})).unwrap();
    assert!(result(receiver).await.is_none());
    fixture.assert_operation(&id, "succeeded");
}

#[tokio::test]
async fn existing_application_selectors_cannot_be_replaced_when_the_hook_writes_ids() {
    let fixture = Fixture::new();
    let path = fixture.saved();
    let complete = format!("{SAVED_TARGET}{}", saved_outputs());
    let replacement = "cccccccc-cccc-4ccc-8ccc-cccccccccccc";

    for (key, id) in [
        (APPLICATION_OBJECT_ID, APP_OBJECT),
        (APPLICATION_CLIENT_ID, APP_CLIENT),
    ] {
        // The pinned producer supports either legacy selector on its own.
        // Filling the other ID is allowed only while preserving this selection.
        let admitted = format!("{SAVED_TARGET}{key}={id}\n");
        fs::write(&path, &admitted).unwrap();
        let (candidate, receiver) = capture(writes_output(&path, &complete));
        let operation = fixture.launch(candidate, Arc::new(|_| {})).unwrap();
        assert!(result(receiver).await.is_some());
        fixture.assert_operation(&operation, "succeeded");

        fs::write(&path, &admitted).unwrap();
        let (candidate, receiver) =
            capture(writes_output(&path, &complete.replace(id, replacement)));
        let operation = fixture.launch(candidate, Arc::new(|_| {})).unwrap();
        assert!(result(receiver).await.is_none(), "{key} was replaced");
        fixture.assert_operation(&operation, "succeeded");
    }

    // A selected app is also bound when both selectors were preexisting.
    for id in [APP_OBJECT, APP_CLIENT] {
        fs::write(&path, &complete).unwrap();
        let (candidate, receiver) =
            capture(writes_output(&path, &complete.replace(id, replacement)));
        let operation = fixture.launch(candidate, Arc::new(|_| {})).unwrap();
        assert!(result(receiver).await.is_none());
        fixture.assert_operation(&operation, "succeeded");
    }
}

#[tokio::test]
async fn freshly_written_application_outputs_must_be_a_complete_pair_of_valid_ids() {
    let fixture = Fixture::new();
    let path = fixture.saved();
    for final_body in [
        format!("{SAVED_TARGET}{WORKFLOW_ID}={WORKFLOW}\n{APPLICATION_OBJECT_ID}={APP_OBJECT}\n"),
        format!(
            "{SAVED_TARGET}{}",
            saved_outputs().replace(APP_CLIENT, "not-an-app-id")
        ),
    ] {
        fs::write(&path, SAVED_TARGET).unwrap();
        let (candidate, receiver) = capture(writes_output(&path, &final_body));
        let operation = fixture.launch(candidate, Arc::new(|_| {})).unwrap();
        assert!(result(receiver).await.is_none());
        fixture.assert_operation(&operation, "succeeded");
    }
}

#[tokio::test]
async fn real_pim_picker_is_excluded_during_capture_and_an_unobserved_round_trip_revokes() {
    let fixture = Fixture::new();
    let state = fixture.app_state();
    let select = |environment: &str| {
        crate::pim_pilot::select_saved_pim_environment(
            &state,
            fixture.project.path.clone(),
            environment.into(),
        )
    };
    let (candidate, receiver) = capture(format!("Start-Sleep -Seconds 2; {EMIT}"));
    fixture.launch(candidate, Arc::new(|_| {})).unwrap();
    assert!(select("other")
        .await
        .unwrap_err()
        .to_string()
        .contains("active operation"));
    let mut receipt = result(receiver).await.unwrap();
    assert_eq!(
        receipt.current_hostname(&fixture.database),
        Some("example.logic.azure.com")
    );
    select("other").await.unwrap();
    select("dev").await.unwrap();
    assert!(receipt.current_hostname(&fixture.database).is_none());
}

#[tokio::test]
async fn saved_source_round_trip_revokes_even_before_any_mismatch_is_read() {
    let fixture = Fixture::new();
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
    fs::write(
        fixture.saved(),
        format!(
            "{SAVED_TARGET}{}AZD_PIM_REVOCATION_WORKFLOW_HOST={}\n",
            saved_outputs(),
            output.hostname
        ),
    )
    .unwrap();
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

#[tokio::test]
async fn offline_producer_pin_preserves_ordinary_azd_dispatch() {
    use crate::{
        bind_approval_requirements, command_runner::workflow_producer, lifecycle_execution_target,
        models::ExecutionApprovalRequest, start_azd_operation_with_probe,
    };

    for (scenario, revision, action, expected_error) in [
        ("explicit", workflow_producer::REVISION, "up", None),
        ("implicit", workflow_producer::REVISION, "provision", None),
        ("different-revision", REVISION, "up", None),
        (
            "missing",
            workflow_producer::REVISION,
            "up",
            Some("exact saved environment"),
        ),
        (
            "unreadable",
            workflow_producer::REVISION,
            "provision",
            Some("exact saved environment"),
        ),
        (
            "late-target-drift",
            workflow_producer::REVISION,
            "up",
            Some("saved execution context changed"),
        ),
    ] {
        let fixture = Fixture::with_revision(revision);
        let state = fixture.app_state();
        let path = &fixture.project.path;
        let root = Path::new(path);
        fs::write(root.join("azure.yaml"), "name: synthetic-target\n").unwrap();
        if scenario != "implicit" {
            fs::write(
                fixture.saved(),
                format!(
                    "{SAVED_TARGET}AZD_PIM_MODE=enforced\nAZD_PIM_ENABLE_SESSION_REVOCATION=true\n"
                ),
            )
            .unwrap();
        }
        let mut request = ExecutionApprovalRequest {
            requirements: None,
            action: action.into(),
            project_path: path.clone(),
            environment: "dev".into(),
            template_id: fixture.snapshot.entry.id.clone(),
            revision: Some(revision.into()),
            snapshot_digest: Some(fixture.snapshot.digest.clone()),
            source_binding: None,
            target: Some(lifecycle_execution_target(root, "dev").unwrap()),
            acknowledgement_proofs: Vec::new(),
            acknowledgement_values: Default::default(),
        };
        bind_approval_requirements(&state, &mut request).unwrap();
        let grant = state.execution_approvals.issue(request).unwrap();
        let saved = fixture.saved();
        if matches!(scenario, "missing" | "unreadable") {
            fs::remove_file(&saved).unwrap();
        }
        if scenario == "unreadable" {
            fs::create_dir(&saved).unwrap();
        }
        let dispatch = start_azd_operation_with_probe(
            &state,
            action.into(),
            path.clone(),
            Some("dev".into()),
            Some(grant.token),
            |_| async {
                assert!(!matches!(scenario, "missing" | "unreadable"));
                if scenario == "late-target-drift" {
                    fs::write(&saved, SAVED_TARGET.replace("rg-fixture", "rg-other")).unwrap();
                }
                Ok(())
            },
            |kind, spec, _| {
                assert!(expected_error.is_none(), "{scenario}");
                assert_eq!(kind, action);
                assert!(matches!(spec.executable, Executable::Azd));
                assert_eq!(spec.args, [action, "--no-prompt", "--environment", "dev"]);
                assert_eq!(spec.cwd.as_deref(), Some(root));
                assert!(spec.environment.is_empty() && spec.assets.is_none());
                Ok("ordinary-command".into())
            },
        )
        .await;
        if let Some(expected) = expected_error {
            let error = dispatch.unwrap_err().to_string();
            assert!(error.contains(expected), "{scenario}: {error}");
        } else {
            assert_eq!(dispatch.unwrap(), "ordinary-command", "{scenario}");
        }
    }
}

#[tokio::test]
#[ignore = "requires offline raw Git assets and a reviewed Windows PowerShell executable"]
async fn exact_pinned_wrapper_uses_the_native_capture_with_only_a_synthetic_nested_hook() {
    use crate::command_runner::{display_executable, resolve_system_executable, workflow_producer};

    assert_eq!(std::env::var("AZD_PIM_MODE").as_deref(), Ok("enforced"));
    assert_eq!(
        std::env::var("AZD_PIM_ENABLE_SESSION_REVOCATION").as_deref(),
        Ok("true")
    );
    let source = PathBuf::from(std::env::var_os("AZD_GUI_WORKFLOW_PIN_FIXTURE").unwrap());
    let entry = workflow_producer::verify_source(&source).unwrap();
    let altered = tempfile::tempdir().unwrap();
    for &(path, _, _) in workflow_producer::ASSETS {
        let destination = altered.path().join(path);
        fs::create_dir_all(destination.parent().unwrap()).unwrap();
        fs::copy(source.join(path), destination).unwrap();
    }
    let vendored = altered
        .path()
        .join("scripts/vendor/Azd.GraphAuthentication/Azd.GraphAuthentication.psm1");
    let mut bytes = fs::read(&vendored).unwrap();
    bytes[0] ^= 1;
    fs::write(&vendored, bytes).unwrap();
    assert!(workflow_producer::verify_source(altered.path()).is_err());

    // Measure the exact file returned by the same resolver used at launch.
    let executable = resolve_system_executable("pwsh").unwrap();
    let approved =
        fs::canonicalize(std::env::var_os("AZD_GUI_WORKFLOW_TEST_PWSH").unwrap()).unwrap();
    let expected_digest = std::env::var("AZD_GUI_WORKFLOW_TEST_PWSH_SHA256").unwrap();
    assert_eq!(executable, approved);
    assert_eq!(
        executable
            .file_name()
            .unwrap()
            .to_string_lossy()
            .to_ascii_lowercase(),
        "pwsh.exe"
    );
    assert_eq!(
        hex::encode(Sha256::digest(fs::read(&executable).unwrap())),
        expected_digest
    );
    let module_hashes = std::env::var("AZD_GUI_WORKFLOW_TEST_PWSH_MODULES_SHA256").unwrap();
    let expected: Vec<_> = module_hashes.split(',').collect();
    let assemblies = [
        "System.Management.Automation.dll",
        "Microsoft.PowerShell.Commands.Utility.dll",
        "Microsoft.PowerShell.Commands.Management.dll",
    ];
    assert_eq!(expected.len(), assemblies.len());
    for (name, expected) in assemblies.into_iter().zip(expected) {
        let path = executable.with_file_name(name);
        assert_eq!(
            hex::encode(Sha256::digest(fs::read(path).unwrap())),
            expected
        );
    }
    let executable_path = display_executable(&executable);
    let quote = |value: &str| value.replace('\'', "''");

    for (scenario, expected_state, accepts) in [
        ("success", "succeeded", true),
        ("partial", "failed", false),
        ("source-drift", "succeeded", false),
        ("target-drift", "succeeded", false),
        ("foreign-workflow", "succeeded", false),
        ("callback-field", "succeeded", false),
    ] {
        let fixture = Fixture::with_revision(workflow_producer::REVISION);
        let approved_env = format!(
            "{SAVED_TARGET}{}AZD_PIM_MODE=enforced\nAZD_PIM_ENABLE_SESSION_REVOCATION=true\n",
            saved_outputs()
        );
        fs::write(fixture.saved(), &approved_env).unwrap();
        fs::write(
            Path::new(&fixture.project.path).join("azure.yaml"),
            "name: synthetic-target\n",
        )
        .unwrap();
        let stage = tempfile::tempdir().unwrap();
        let wrapper = stage.path().join(workflow_producer::ENTRYPOINT);
        let scripts = wrapper.parent().unwrap();
        fs::create_dir(scripts).unwrap();
        // The only upstream script executed is the exact native entry point.
        // None of its real deployment or authentication dependencies are staged.
        fs::write(&wrapper, &entry).unwrap();
        let tail = if scenario == "partial" {
            "throw 'synthetic optional failure'"
        } else {
            ""
        };
        let mut output: Value = serde_json::from_str(OUTPUT).unwrap();
        if scenario == "callback-field" {
            output["callbackUrl"] = json!("https://example.invalid/fixture?sig=synthetic-only");
        }
        let output = output.to_string();
        let revision = workflow_producer::REVISION;
        let native_hook = format!(
            r#"
param([ref] $NativeWorkflowHostObservation)
$ErrorActionPreference = 'Stop'
if ([Environment]::ProcessPath -ine '{}' -or
    -not [Environment]::Is64BitProcess -or $PSVersionTable.PSEdition -cne 'Core' -or
    $PSVersionTable.PSVersion -lt [version]'7.6' -or
    (Test-Path -LiteralPath 'azure.yaml') -or $env:PATH -or $env:AZURE_CONFIG_DIR -or
    $env:AZURE_TENANT_ID -or $env:AZD_GUI_WORKFLOW_TEST_AMBIENT -or
    $env:AZD_PIM_MODE -or $env:AZD_PIM_ENABLE_SESSION_REVOCATION -or
    (Get-Module Microsoft.Graph.Authentication) -or
    $env:PSModulePath -ine [IO.Path]::Combine($PSHOME, 'Modules') -or
    (Get-Command ConvertTo-Json).ImplementingType.Assembly.Location -ine [IO.Path]::Combine($PSHOME, 'Microsoft.PowerShell.Commands.Utility.dll') -or
    (Get-Command Join-Path).ImplementingType.Assembly.Location -ine [IO.Path]::Combine($PSHOME, 'Microsoft.PowerShell.Commands.Management.dll') -or
    [System.Management.Automation.PSObject].Assembly.Location -ine [IO.Path]::Combine($PSHOME, 'System.Management.Automation.dll') -or
    $env:AZD_GUI_PROJECT_ID -cne 'project-fixture' -or $env:AZD_GUI_ENVIRONMENT -cne 'dev' -or
    $env:AZD_GUI_OPERATION_KIND -cne 'up' -or $env:AZD_GUI_SOURCE_REVISION -cne '{revision}' -or
    $env:AZD_GUI_OPERATION_ID -notmatch '^[0-9a-f-]{{36}}$' -or
    $env:AZD_GUI_CONTRACT_DIGEST -cne '{}') {{ throw 'offline launch assumptions changed' }}
$NativeWorkflowHostObservation.Value = ConvertFrom-Json -InputObject '{}'
Write-Host 'synthetic ordinary output'
{tail}
"#,
            quote(&executable_path),
            fixture.snapshot.digest,
            quote(&output)
        );
        fs::write(scripts.join("Post-Provision.ps1"), native_hook).unwrap();

        // PowerShell can repopulate its default module path at startup. Reset
        // it before calling any script and load only the verified built-ins.
        let command = format!(
            r#"
$ErrorActionPreference = 'Stop'
$env:PATH = $null
$env:PSModulePath = [IO.Path]::Combine($PSHOME, 'Modules')
$PSModuleAutoLoadingPreference = 'None'
Import-Module ([IO.Path]::Combine($PSHOME, 'Microsoft.PowerShell.Commands.Utility.dll')) -ErrorAction Stop
Import-Module ([IO.Path]::Combine($PSHOME, 'Microsoft.PowerShell.Commands.Management.dll')) -ErrorAction Stop
& '{}'
"#,
            quote(&wrapper.display().to_string())
        );
        let (mut candidate, receiver) = capture(command);
        candidate.producer_revision = revision;
        candidate.offline_powershell = Some(executable.clone());
        if scenario == "foreign-workflow" {
            candidate.workflow_resource_id =
                candidate.workflow_resource_id.replace("-example", "-other");
        }
        let (sink, emitted) = record_events();
        let database = fixture.database.clone();
        let project = fixture.project.clone();
        let mut entry = fixture.snapshot.entry.clone();
        entry.summary = "Changed source during the native operation".into();
        let changed = ProjectTemplateSnapshot::new(entry).unwrap();
        let target = fixture.saved();
        let drifted_env = approved_env.replace("rg-fixture", "rg-other");
        let operation = fixture
            .launch(
                candidate,
                Arc::new(move |event| {
                    if scenario == "source-drift" && event.state == "running" {
                        database.save_project_template(&project, &changed).unwrap();
                    }
                    if scenario == "target-drift" && event.state == "running" {
                        fs::write(&target, &drifted_env).unwrap();
                    }
                    sink(event);
                }),
            )
            .unwrap();
        let receipt = result(receiver).await;
        assert_eq!(receipt.is_some(), accepts, "{scenario}");
        fixture.assert_operation(&operation, expected_state);
        if let Some(mut receipt) = receipt {
            assert_eq!(receipt.operation_id, operation);
            assert_eq!(
                receipt.current_hostname(&fixture.database),
                Some("example.logic.azure.com")
            );
        }
        let events = serde_json::to_string(&*emitted.lock().unwrap()).unwrap();
        assert!(!events.contains("synthetic ordinary output"));
        assert!(!events.contains("example.logic.azure.com"));
        assert!(!events.contains("synthetic-only"));
        assert!(!fs::read_to_string(fixture.saved())
            .unwrap()
            .contains("synthetic-only"));
        assert!(!Path::new(&fixture.project.path).join("reports").exists());
    }
}
