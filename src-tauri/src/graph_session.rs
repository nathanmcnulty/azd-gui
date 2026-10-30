//! One owned MSAL credential and its registered reads live in one pwsh process.
//! Evidence from this adapter is deliberately NOT a grant for azd child hooks.
use crate::{
    command_runner::{attach_process_tree, resolve_system_executable, ProcessTree},
    error::{AppError, AppResult},
    requirements::RequirementSnapshot,
};
use serde::{Deserialize, Serialize};
use std::{
    process::Stdio,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};
use tokio::{
    io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader},
    process::{Child, ChildStdin, ChildStdout, Command},
    sync::{watch, Mutex as AsyncMutex},
};
use uuid::Uuid;

const LIMIT: u64 = 65_536;
const TIMEOUT: Duration = Duration::from_secs(30);
const SIGN_IN_TIMEOUT: Duration = Duration::from_secs(180);
const SESSION_LIFETIME: Duration = Duration::from_secs(15 * 60);
include!(concat!(env!("OUT_DIR"), "/graph_adapter_assets.rs"));

fn invalid(message: &str) -> AppError {
    AppError::Invalid(message.into())
}

/// azd launches independent hook processes. No worker proof can authorize them.
pub fn reject_unadapted_execution(
    contract: &crate::models::ProjectTemplateSnapshot,
) -> AppResult<()> {
    if contract.entry.manifest.as_ref().is_some_and(|manifest| {
        manifest
            .connections
            .iter()
            .any(|connection| connection.id == "graphPowerShell")
            || manifest
                .permission_checks
                .iter()
                .any(|check| check.id == "graph-delegated-scopes")
    }) {
        return Err(invalid("This template needs process-bound Graph execution. Separate azd hooks and terminals cannot use the owned Graph check session. A reviewed execution adapter is required before initialization or lifecycle execution."));
    }
    Ok(())
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct GraphBinding {
    pub requirements_digest: String,
    pub account: String,
    pub tenant_id: String,
    pub client_id: String,
    pub resource: String,
    pub scopes: Vec<String>,
    pub probe_uri: String,
}

impl GraphBinding {
    pub fn from_requirements(
        snapshot: &RequirementSnapshot,
        account: String,
        client_id: String,
    ) -> AppResult<Self> {
        let mut scopes = snapshot
            .requirements
            .connections
            .iter()
            .filter(|c| c.id == "graphPowerShell")
            .flat_map(|c| c.scopes.iter().cloned())
            .collect::<Vec<_>>();
        scopes.sort_by_key(|s| s.to_ascii_lowercase());
        scopes.dedup_by(|a, b| a.eq_ignore_ascii_case(b));
        let has = |scope: &str| scopes.iter().any(|s| s.eq_ignore_ascii_case(scope));
        let probe = if has("Directory.Read.All") || has("Directory.ReadWrite.All") {
            "https://graph.microsoft.com/v1.0/organization?$select=id"
        } else if has("Policy.Read.All") || has("Policy.ReadWrite.ConditionalAccess") {
            "https://graph.microsoft.com/v1.0/identity/conditionalAccess/policies?$top=1&$select=id"
        } else if has("User.Read") {
            "https://graph.microsoft.com/v1.0/me?$select=id"
        } else {
            return Err(invalid("No registered Graph read is covered by these requested scopes. No extra scopes will be requested."));
        };
        let binding = Self {
            requirements_digest: snapshot.digest.clone(),
            account: account.trim().to_ascii_lowercase(),
            tenant_id: snapshot
                .requirements
                .target
                .tenant_id
                .clone()
                .unwrap_or_default(),
            client_id: client_id.to_ascii_lowercase(),
            resource: "https://graph.microsoft.com".into(),
            scopes,
            probe_uri: probe.into(),
        };
        binding.validate()?;
        Ok(binding)
    }

    fn validate(&self) -> AppResult<()> {
        if self.requirements_digest.len() != 64
            || !self
                .requirements_digest
                .bytes()
                .all(|c| c.is_ascii_hexdigit())
            || self.account.len() > 320
            || !self.account.contains('@')
            || !self.account.is_ascii()
            || self
                .account
                .chars()
                .any(|c| c.is_control() || c.is_whitespace())
            || [&self.tenant_id, &self.client_id]
                .iter()
                .any(|v| Uuid::parse_str(v).map_or(true, |id| id.is_nil()))
            || self.resource != "https://graph.microsoft.com"
            || self.scopes.is_empty()
            || self.scopes.len() > 64
            || self.scopes.iter().any(|s| {
                s.is_empty()
                    || s.len() > 512
                    || !s
                        .bytes()
                        .all(|c| c.is_ascii_alphanumeric() || b"._-".contains(&c))
                    || s == ".default"
            })
        {
            return Err(invalid("Graph session needs an exact account, tenant, client, resource and bounded delegated scope names."));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct ContextEvidence {
    account: String,
    tenant_id: String,
    client_id: String,
    resource: String,
    pub scopes: Vec<String>,
    auth_type: String,
    context_scope: String,
    environment: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Reply {
    sequence: u64,
    session_id: String,
    requirements_digest: String,
    process_id: u32,
    action: String,
    ok: bool,
    evidence: Option<ContextEvidence>,
}

struct Transport {
    child: Child,
    input: ChildStdin,
    output: BufReader<ChildStdout>,
    sequence: u64,
    _tree: Option<ProcessTree>,
    _files: tempfile::TempDir,
}

pub struct GraphWorker {
    created: Instant,
    session_id: String,
    pid: u32,
    binding: GraphBinding,
    transport: AsyncMutex<Option<Transport>>,
    cancel: watch::Sender<bool>,
}

impl GraphWorker {
    pub async fn spawn(binding: GraphBinding) -> AppResult<Arc<Self>> {
        Self::spawn_with_module(binding, None).await
    }

    async fn spawn_with_module(
        binding: GraphBinding,
        module: Option<&std::path::Path>,
    ) -> AppResult<Arc<Self>> {
        binding.validate()?;
        let files = tempfile::tempdir()?;
        for (name, bytes) in GRAPH_ADAPTER_ASSETS {
            std::fs::write(files.path().join(name), bytes)?;
        }
        std::fs::write(
            files.path().join("GraphAdapter.psm1"),
            include_str!("GraphAdapter.psm1"),
        )?;
        let script = files.path().join("graph-worker.ps1");
        std::fs::write(&script, include_str!("graph_worker.ps1"))?;
        let executable = resolve_system_executable("pwsh")?;
        let mut command = Command::new(executable);
        command
            .args(["-NoLogo", "-NoProfile", "-NonInteractive", "-File"])
            .arg(script)
            .kill_on_drop(true)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null());
        if let Some(module) = module {
            command.arg("-AdapterModule").arg(module);
        }
        #[cfg(windows)]
        command.creation_flags(0x0800_0000);
        #[cfg(unix)]
        command.process_group(0);
        let mut child = command.spawn()?;
        let tree = attach_process_tree(&mut child)?;
        let pid = child
            .id()
            .ok_or_else(|| invalid("Graph worker has no process identity."))?;
        let input = child
            .stdin
            .take()
            .ok_or_else(|| invalid("Graph worker stdin unavailable."))?;
        let output = BufReader::new(
            child
                .stdout
                .take()
                .ok_or_else(|| invalid("Graph worker stdout unavailable."))?,
        );
        let (cancel, _) = watch::channel(false);
        let worker = Arc::new(Self {
            created: Instant::now(),
            session_id: Uuid::new_v4().to_string(),
            pid,
            binding,
            transport: AsyncMutex::new(Some(Transport {
                child,
                input,
                output,
                sequence: 0,
                _tree: tree,
                _files: files,
            })),
            cancel,
        });
        worker.request("initialize", false).await?;
        Ok(worker)
    }

    /// Call only after an exact native sign-in approval. Probes never authenticate.
    pub async fn authenticate(&self) -> AppResult<()> {
        self.request("authenticate", true).await.map(|_| ())
    }

    pub async fn probe(&self) -> AppResult<ContextEvidence> {
        self.request("probe", false)
            .await?
            .evidence
            .ok_or_else(|| invalid("Fresh Graph read evidence absent."))
    }

    async fn request(&self, action: &str, allow_interactive: bool) -> AppResult<Reply> {
        self.request_with_timeout(
            action,
            allow_interactive,
            if allow_interactive {
                SIGN_IN_TIMEOUT
            } else {
                TIMEOUT
            },
        )
        .await
    }

    async fn request_with_timeout(
        &self,
        action: &str,
        allow_interactive: bool,
        timeout: Duration,
    ) -> AppResult<Reply> {
        let mut guard = self.transport.lock().await;
        let mut transport = guard
            .take()
            .ok_or_else(|| invalid("Graph session is closed or cancelled."))?;
        let mut cancelled = self.cancel.subscribe();
        if *cancelled.borrow() || self.created.elapsed() >= SESSION_LIFETIME {
            return Err(invalid("Graph session cancelled."));
        }
        transport.sequence += 1;
        let sequence = transport.sequence;
        let request = serde_json::json!({"action":action,"sequence":sequence,"sessionId":self.session_id,
            "requirementsDigest":self.binding.requirements_digest,"binding":self.binding,"allowInteractive":allow_interactive});
        let mut bytes = serde_json::to_vec(&request)?;
        bytes.push(b'\n');
        let io = async {
            transport.input.write_all(&bytes).await?;
            transport.input.flush().await?;
            for _ in 0..64 {
                let mut line = Vec::new();
                let count = (&mut transport.output)
                    .take(LIMIT)
                    .read_until(b'\n', &mut line)
                    .await?;
                if count == 0 || count as u64 >= LIMIT {
                    return Err(invalid("Graph worker closed or returned oversized output."));
                }
                if let Some(data) = line.strip_prefix(b"AZD_GRAPH_RPC:") {
                    // Do not expose unexpected SDK output or raw parse errors.
                    let reply: Reply = serde_json::from_slice(data)
                        .map_err(|_| invalid("Invalid Graph worker protocol response."))?;
                    return self.validate_reply(reply, action, sequence);
                }
            }
            Err(invalid(
                "Graph worker did not return bounded protocol evidence.",
            ))
        };
        let result = tokio::select! {
            result=tokio::time::timeout(timeout,io)=>result.unwrap_or_else(|_|Err(invalid("Graph worker timed out; session discarded."))),
            _=cancelled.changed()=>Err(invalid("Graph session cancelled; response discarded.")),
        };
        match result {
            Ok(reply) => {
                if self.created.elapsed() >= SESSION_LIFETIME || *cancelled.borrow() {
                    self.cancel();
                    let _ = transport.child.kill().await;
                    let _ = transport.child.wait().await;
                    return Err(invalid(
                        "Graph session expired or changed during operation.",
                    ));
                }
                *guard = Some(transport);
                Ok(reply)
            }
            Err(error) => {
                self.cancel.send_replace(true);
                let _ = transport.child.kill().await;
                let _ = transport.child.wait().await;
                Err(error)
            }
        }
        // Dropping an interrupted request drops Transport, its process tree and
        // kill_on_drop child. A cancelled caller can never leave reusable proof.
    }

    fn validate_reply(&self, reply: Reply, action: &str, sequence: u64) -> AppResult<Reply> {
        if !reply.ok
            || reply.sequence != sequence
            || reply.session_id != self.session_id
            || reply.process_id != self.pid
            || reply.requirements_digest != self.binding.requirements_digest
            || reply.action != action
        {
            return Err(invalid(
                "Graph worker evidence did not match this operation/session.",
            ));
        }
        if action != "initialize" {
            let evidence = reply
                .evidence
                .as_ref()
                .ok_or_else(|| invalid("Graph context evidence absent."))?;
            if evidence.account != self.binding.account
                || evidence.auth_type != "Delegated"
                || evidence.context_scope != "Process"
                || evidence.environment != "Global"
                || !evidence
                    .tenant_id
                    .eq_ignore_ascii_case(&self.binding.tenant_id)
                || !evidence
                    .client_id
                    .eq_ignore_ascii_case(&self.binding.client_id)
                || evidence.resource != self.binding.resource
                || self
                    .binding
                    .scopes
                    .iter()
                    .any(|s| !evidence.scopes.iter().any(|a| s.eq_ignore_ascii_case(a)))
            {
                return Err(invalid(
                    "Graph identity/resource/scopes changed; session discarded.",
                ));
            }
        }
        Ok(reply)
    }

    pub fn cancel(&self) {
        self.cancel.send_replace(true);
    }

    pub async fn close(&self) {
        self.cancel();
        if let Some(mut transport) = self.transport.lock().await.take() {
            // Closing stdin asks the loop to exit; forcibly terminate if needed.
            let _ = transport.input.shutdown().await;
            if tokio::time::timeout(Duration::from_secs(2), transport.child.wait())
                .await
                .is_err()
            {
                let _ = transport.child.kill().await;
                let _ = transport.child.wait().await;
            }
        }
    }
}

#[derive(Default)]
struct Registry {
    generation: Uuid,
    requirements_digest: Option<String>,
    worker: Option<Arc<GraphWorker>>,
}

impl Registry {
    fn revoke(&mut self) -> Option<Arc<GraphWorker>> {
        self.generation = Uuid::new_v4();
        self.requirements_digest = None;
        let old = self.worker.take();
        if let Some(worker) = &old {
            worker.cancel();
        }
        old
    }
}

#[derive(Default)]
pub struct GraphSessions {
    current: Mutex<Registry>,
}

impl GraphSessions {
    // Reserve intent BEFORE a dialog or asynchronous process startup. A cancel
    // with no installed worker still revokes this pending authentication ticket.
    pub fn begin(&self, binding: &GraphBinding) -> AppResult<Uuid> {
        let mut registry = self
            .current
            .lock()
            .map_err(|_| invalid("Graph sessions unavailable."))?;
        registry.revoke();
        registry.requirements_digest = Some(binding.requirements_digest.clone());
        Ok(registry.generation)
    }

    pub async fn install(&self, ticket: Uuid, worker: Arc<GraphWorker>) -> AppResult<()> {
        let old = {
            let mut registry = self
                .current
                .lock()
                .map_err(|_| invalid("Graph sessions unavailable."))?;
            if registry.generation != ticket
                || registry.requirements_digest.as_deref()
                    != Some(&worker.binding.requirements_digest)
            {
                worker.cancel();
                return Err(invalid(
                    "Graph sign-in was cancelled or replaced before installation.",
                ));
            }
            registry.worker.replace(worker)
        };
        if let Some(old) = old {
            old.close().await;
        }
        Ok(())
    }

    pub fn ensure_current(&self, ticket: Uuid, worker: &Arc<GraphWorker>) -> AppResult<()> {
        let registry = self
            .current
            .lock()
            .map_err(|_| invalid("Graph sessions unavailable."))?;
        if registry.generation != ticket
            || !registry
                .worker
                .as_ref()
                .is_some_and(|active| Arc::ptr_eq(active, worker))
        {
            return Err(invalid(
                "Graph sign-in/session changed; late completion discarded.",
            ));
        }
        Ok(())
    }

    pub fn invalidate_ticket(&self, ticket: Uuid) {
        if let Ok(mut registry) = self.current.lock() {
            if registry.generation == ticket {
                registry.revoke();
            }
        }
    }

    pub async fn verified_binding(&self) -> AppResult<Option<(GraphBinding, ContextEvidence)>> {
        let worker = self
            .current
            .lock()
            .map_err(|_| invalid("Graph sessions unavailable."))?
            .worker
            .clone();
        let Some(worker) = worker else {
            return Ok(None);
        };
        let binding = worker.binding.clone();
        let observation = self.probe(&binding).await?;
        Ok(Some((binding, observation)))
    }

    pub fn invalidate_worker(&self, worker: &Arc<GraphWorker>) {
        if let Ok(mut registry) = self.current.lock() {
            if registry
                .worker
                .as_ref()
                .is_some_and(|active| Arc::ptr_eq(active, worker))
            {
                registry.revoke();
            }
        }
    }

    pub fn requirements_changed(&self, digest: &str) -> bool {
        if let Ok(mut registry) = self.current.lock() {
            if registry
                .requirements_digest
                .as_deref()
                .is_some_and(|current| current != digest)
            {
                registry.revoke();
                return true;
            }
        }
        false
    }

    pub fn binding_for(&self, snapshot: &RequirementSnapshot) -> AppResult<GraphBinding> {
        let current = self
            .current
            .lock()
            .map_err(|_| invalid("Graph sessions unavailable."))?
            .worker
            .clone()
            .ok_or_else(|| {
                invalid("Approve an owned Graph session for these requirements first.")
            })?;
        let expected = match GraphBinding::from_requirements(
            snapshot,
            current.binding.account.clone(),
            current.binding.client_id.clone(),
        ) {
            Ok(expected) => expected,
            Err(error) => {
                self.invalidate_worker(&current);
                return Err(error);
            }
        };
        if expected != current.binding {
            self.invalidate_worker(&current);
            return Err(invalid(
                "Graph requirements changed; sign-in session discarded.",
            ));
        }
        Ok(expected)
    }

    pub async fn dispose(&self) {
        let worker = self
            .current
            .lock()
            .ok()
            .and_then(|mut registry| registry.revoke());
        if let Some(worker) = worker {
            worker.close().await;
        }
    }

    #[cfg(test)]
    pub async fn replace(&self, worker: Arc<GraphWorker>) -> AppResult<()> {
        let ticket = self.begin(&worker.binding)?;
        self.install(ticket, worker).await
    }

    pub fn invalidate(&self) {
        if let Ok(mut registry) = self.current.lock() {
            registry.revoke();
        }
    }

    pub async fn probe(&self, binding: &GraphBinding) -> AppResult<ContextEvidence> {
        let (worker, ticket) = {
            let mut registry = self
                .current
                .lock()
                .map_err(|_| invalid("Graph sessions unavailable."))?;
            if registry
                .worker
                .as_ref()
                .is_some_and(|worker| &worker.binding != binding)
            {
                registry.revoke();
            }
            (
                registry.worker.clone().ok_or_else(|| {
                    invalid("No owned Graph session for these exact requirements and identity.")
                })?,
                registry.generation,
            )
        };
        let observation = match worker.probe().await {
            Ok(observation) => observation,
            Err(error) => {
                self.invalidate_worker(&worker);
                return Err(error);
            }
        };
        self.ensure_current(ticket, &worker)?;
        Ok(observation)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{json, Value};

    fn binding() -> GraphBinding {
        let mut entry = crate::catalog::entry_by_id("myworkid").unwrap();
        entry.manifest = Some(
            serde_json::from_str(include_str!(
                "../../src/lib/fixtures/conditional-requirements-v1.1.json"
            ))
            .unwrap(),
        );
        let contract = crate::models::ProjectTemplateSnapshot::new(entry).unwrap();
        let snapshot = crate::requirements::resolve(
            &contract,
            &Default::default(),
            "dev",
            crate::models::ExecutionTarget {
                tenant_id: Some("11111111-1111-4111-8111-111111111111".into()),
                subscription_id: None,
                location: None,
                resource_group: None,
            },
        )
        .unwrap();
        GraphBinding::from_requirements(
            &snapshot,
            "operator@example.test".into(),
            "22222222-2222-4222-8222-222222222222".into(),
        )
        .unwrap()
    }

    struct Fixture {
        root: tempfile::TempDir,
    }
    impl Fixture {
        fn new() -> Self {
            let root = tempfile::tempdir().unwrap();
            let managed=std::env::var_os("AZD_GUI_GRAPH_TEST_ADAPTER_DIR")
                .expect("Run tools/Test-GraphProcessAdapter.ps1 to build the managed offline substitutes first.");
            for entry in std::fs::read_dir(managed).unwrap() {
                let path = entry.unwrap().path();
                if path.extension().is_some_and(|extension| extension == "dll") {
                    std::fs::copy(&path, root.path().join(path.file_name().unwrap())).unwrap();
                }
            }
            std::fs::write(
                root.path().join("Fake.Graph.psm1"),
                include_str!("../tests/fixtures/graph/Fake.Graph.psm1"),
            )
            .unwrap();
            let fixture = Self { root };
            fixture.control("good", 0);
            fixture
        }
        fn control(&self, mode: &str, delay_ms: u64) {
            std::fs::write(
                self.root.path().join("control.json"),
                json!({"account":"operator@example.test","mode":mode,"delayMs":delay_ms})
                    .to_string(),
            )
            .unwrap();
        }
        async fn worker(&self) -> Arc<GraphWorker> {
            GraphWorker::spawn_with_module(
                binding(),
                Some(&self.root.path().join("Fake.Graph.psm1")),
            )
            .await
            .unwrap()
        }
        fn calls(&self) -> Vec<Value> {
            std::fs::read_to_string(self.root.path().join("calls.jsonl"))
                .unwrap_or_default()
                .lines()
                .filter_map(|s| serde_json::from_str(s).ok())
                .collect()
        }
        async fn wait_for_read(&self) {
            tokio::time::timeout(Duration::from_secs(8), async {
                while !self
                    .calls()
                    .iter()
                    .any(|v| v["kind"] == "protected-read-start")
                {
                    tokio::time::sleep(Duration::from_millis(30)).await;
                }
            })
            .await
            .expect("fake protected operation did not start");
        }
    }

    #[test]
    fn exact_binding_never_adds_probe_permissions_or_accepts_default_scope() {
        let expected = binding();
        assert_eq!(expected.scopes, ["Directory.Read.All"]);
        assert!(!expected.scopes.iter().any(|s| s == "User.Read"));
        let mut invalid_binding = expected;
        invalid_binding.scopes = vec![".default".into()];
        assert!(invalid_binding.validate().is_err());
    }

    #[tokio::test]
    #[ignore = "offline integration requires installed pwsh; substitutes MSAL/network and never authenticates"]
    async fn actual_adapter_keeps_authentication_and_protected_read_in_same_process() {
        let fixture = Fixture::new();
        let worker = fixture.worker().await;
        worker.authenticate().await.unwrap();
        worker.probe().await.unwrap();
        let calls = fixture.calls();
        assert_eq!(calls.len(), 3);
        assert!(calls.iter().all(|call| call["pid"] == worker.pid));
        assert!(calls.iter().all(|call| call["owner"] == calls[0]["owner"]));
        assert_eq!(calls[0]["scopes"], json!(["Directory.Read.All"]));
        assert_eq!(
            calls[0]["requestedScopes"],
            json!(["https://graph.microsoft.com/Directory.Read.All"])
        );
        assert_eq!(calls[1]["uri"], binding().probe_uri);
        let separate = Fixture::new();
        let other = separate.worker().await;
        assert_ne!(worker.pid, other.pid);
        assert!(other.probe().await.is_err());
        assert!(separate.calls().is_empty());
        worker.close().await;
        assert!(worker.probe().await.is_err());
    }

    #[tokio::test]
    #[ignore = "offline integration requires installed pwsh; substitutes MSAL/network and never authenticates"]
    async fn actual_grants_are_reported_and_any_later_addition_or_reduction_revokes() {
        let fixture = Fixture::new();
        fixture.control("initialExtra", 0);
        let worker = fixture.worker().await;
        worker.authenticate().await.unwrap();
        let evidence = worker.probe().await.unwrap();
        assert_eq!(evidence.scopes, ["Directory.Read.All", "Policy.Read.All"]);
        // Reduction still covers the required permission, but changes approved identity evidence.
        fixture.control("good", 0);
        assert!(worker.probe().await.is_err());
        assert_eq!(fixture.calls().len(), 3);
        fixture.control("initialExtra", 0);
        assert!(worker.probe().await.is_err());
        // Addition is covered independently by the mismatch table's extraScopes case.
    }

    #[tokio::test]
    #[ignore = "offline integration requires installed pwsh; substitutes MSAL/network and never authenticates"]
    async fn independent_session_and_foreign_record_cannot_replace_retained_credential() {
        let fixture = Fixture::new();
        let worker = fixture.worker().await;
        worker.authenticate().await.unwrap();
        // A foreign record is deliberately confined to the fixture. The adapter has no
        // shared-record reader or cache registration; a second owned process is separate.
        let foreign = fixture.root.path().join("mg.authrecord.json");
        std::fs::write(
            &foreign,
            r#"{"username":"other@example.test","tenantId":"foreign"}"#,
        )
        .unwrap();
        let separate = Fixture::new();
        let other = separate.worker().await;
        other.authenticate().await.unwrap();
        std::fs::write(
            &foreign,
            "replaced between authentication and first request",
        )
        .unwrap();
        let reply = worker.request("probe", false).await.unwrap();
        let frame = format!("{reply:?}");
        assert!(!frame.contains("offline-only-token"));
        assert!(!frame.contains("accessToken"));
        worker.probe().await.unwrap(); // Fake rotates its token; HTTP must use each latest result.
        let calls = fixture.calls();
        let owner = &calls[0]["owner"];
        assert!(calls.iter().all(|call| &call["owner"] == owner));
        assert_ne!(owner, &separate.calls()[0]["owner"]);
        assert_eq!(
            std::fs::read_to_string(foreign).unwrap(),
            "replaced between authentication and first request"
        );
        worker.close().await;
        other.close().await;
    }

    #[tokio::test]
    #[ignore = "offline integration requires installed pwsh; substitutes MSAL/network and never authenticates"]
    async fn cancellation_during_authentication_never_installs_evidence_or_replays() {
        let fixture = Fixture::new();
        let worker = fixture.worker().await;
        std::fs::write(
            fixture.root.path().join("control.json"),
            json!({
                "account":"operator@example.test", "mode":"good", "delayMs":0, "authDelayMs":10000
            })
            .to_string(),
        )
        .unwrap();
        let owned = worker.clone();
        let task = tokio::spawn(async move { owned.authenticate().await });
        tokio::time::timeout(Duration::from_secs(8), async {
            while !fixture
                .calls()
                .iter()
                .any(|call| call["kind"] == "authenticate-start")
            {
                tokio::time::sleep(Duration::from_millis(30)).await;
            }
        })
        .await
        .unwrap();
        worker.cancel();
        assert!(task.await.unwrap().is_err());
        worker.close().await;
        assert!(worker.authenticate().await.is_err());
        assert!(worker.probe().await.is_err());
        assert_eq!(fixture.calls().len(), 1);
    }

    #[tokio::test]
    #[ignore = "offline integration requires installed pwsh; substitutes MSAL/network and never authenticates"]
    async fn foreign_protocol_requests_are_rejected_before_authentication() {
        for field in [
            "sequence",
            "sessionId",
            "requirementsDigest",
            "allowInteractive",
            "action",
        ] {
            let fixture = Fixture::new();
            let worker = fixture.worker().await;
            let mut request = json!({"sequence":2,"sessionId":worker.session_id,
                "requirementsDigest":worker.binding.requirements_digest,
                "action":"authenticate","allowInteractive":true});
            request[field] = match field {
                "sequence" => json!(1),
                "allowInteractive" => json!(false),
                _ => json!("foreign"),
            };
            let mut guard = worker.transport.lock().await;
            let transport = guard.as_mut().unwrap();
            let mut bytes = serde_json::to_vec(&request).unwrap();
            bytes.push(b'\n');
            transport.input.write_all(&bytes).await.unwrap();
            transport.input.flush().await.unwrap();
            let mut line = String::new();
            tokio::time::timeout(
                Duration::from_secs(8),
                transport.output.read_line(&mut line),
            )
            .await
            .unwrap()
            .unwrap();
            let reply: Value =
                serde_json::from_str(line.strip_prefix("AZD_GRAPH_RPC:").unwrap()).unwrap();
            assert_eq!(reply["ok"], false, "{field}");
            assert!(fixture.calls().is_empty(), "{field}");
            drop(guard);
            worker.close().await;
        }
    }

    #[tokio::test]
    #[ignore = "offline production dependency initialization only; no authentication or network request"]
    async fn embedded_production_dependencies_initialize_without_authentication() {
        let worker = GraphWorker::spawn(binding()).await.unwrap();
        // Probe refuses before calling MSAL refresh or HTTP without an explicitly authenticated account.
        assert!(worker.probe().await.is_err());
        assert!(worker.authenticate().await.is_err()); // Failure closed the process; cannot trigger sign-in.
        worker.close().await;
        for (uri, scopes) in [
            (
                "https://other.example.test/",
                vec!["Directory.Read.All".into()],
            ),
            (
                "https://graph.microsoft.com/v1.0/organization?$select=id",
                vec!["User.Read".into()],
            ),
        ] {
            let mut invalid = binding();
            invalid.probe_uri = uri.into();
            invalid.scopes = scopes;
            assert!(GraphWorker::spawn(invalid).await.is_err());
        }
    }

    #[tokio::test]
    #[ignore = "offline integration requires installed pwsh; substitutes MSAL/network and never authenticates"]
    async fn actual_adapter_blocks_missing_stale_wrong_identity_resource_and_scopes_before_operation(
    ) {
        for mode in [
            "missing",
            "account",
            "principal",
            "object",
            "authority",
            "silentRequired",
            "tenant",
            "client",
            "resource",
            "scopes",
            "extraScopes",
            "appOnly",
            "cached",
        ] {
            let fixture = Fixture::new();
            let worker = fixture.worker().await;
            worker.authenticate().await.unwrap();
            fixture.control(mode, 0);
            assert!(worker.probe().await.is_err(), "{mode}");
            assert_eq!(
                fixture.calls().len(),
                1,
                "{mode} must not execute protected read"
            );
            fixture.control("good", 0);
            assert!(
                worker.probe().await.is_err(),
                "failed session must stay revoked"
            );
        }
    }

    #[tokio::test]
    #[ignore = "offline integration requires installed pwsh; substitutes MSAL/network and never authenticates"]
    async fn actual_adapter_rejects_mid_operation_identity_change_and_foreign_protocol_reply() {
        for mode in [
            "changeDuringRead",
            "spoof",
            "oversized",
            "malformed",
            "denied",
            "redirect",
        ] {
            let fixture = Fixture::new();
            let worker = fixture.worker().await;
            worker.authenticate().await.unwrap();
            fixture.control(mode, 0);
            assert!(worker.probe().await.is_err(), "{mode}");
            assert!(worker.probe().await.is_err());
        }
    }

    #[tokio::test]
    #[ignore = "offline integration requires installed pwsh; substitutes MSAL/network and never authenticates"]
    async fn actual_adapter_cancel_and_dropped_caller_dispose_worker_without_replay() {
        for abort in [false, true] {
            let fixture = Fixture::new();
            let worker = fixture.worker().await;
            worker.authenticate().await.unwrap();
            fixture.control("good", 10_000);
            let owned = worker.clone();
            let task = tokio::spawn(async move { owned.probe().await });
            fixture.wait_for_read().await;
            if abort {
                task.abort();
                assert!(task.await.unwrap_err().is_cancelled());
            } else {
                worker.cancel();
                assert!(task.await.unwrap().is_err());
            }
            worker.close().await;
            assert!(worker.probe().await.is_err());
            assert_eq!(
                fixture
                    .calls()
                    .iter()
                    .filter(|v| v["kind"] == "protected-read-start")
                    .count(),
                1
            );
            assert!(!fixture
                .calls()
                .iter()
                .any(|v| v["kind"] == "protected-read-end"));
        }
    }

    #[tokio::test]
    #[ignore = "offline integration requires installed pwsh; substitutes MSAL/network and never authenticates"]
    async fn actual_adapter_invalidates_expansion_reduction_and_late_session_responses() {
        let manager = Arc::new(GraphSessions::default());
        let fixture = Fixture::new();
        let worker = fixture.worker().await;
        worker.authenticate().await.unwrap();
        manager.replace(worker).await.unwrap();
        let mut changed = binding();
        changed.requirements_digest = "b".repeat(64);
        assert!(manager.probe(&changed).await.is_err());
        assert!(manager.probe(&binding()).await.is_err()); // A -> B -> A never revives session.
        let worker = fixture.worker().await;
        worker.authenticate().await.unwrap();
        manager.replace(worker).await.unwrap();
        fixture.control("good", 10_000);
        let sessions = manager.clone();
        let task = tokio::spawn(async move { sessions.probe(&binding()).await });
        fixture.wait_for_read().await;
        let replacement = Fixture::new();
        manager.replace(replacement.worker().await).await.unwrap();
        assert!(task.await.unwrap().is_err());
        assert!(manager.probe(&binding()).await.is_err()); // replacement has never signed in.
        manager.invalidate();
    }

    #[tokio::test]
    #[ignore = "offline integration requires installed pwsh; substitutes MSAL/network and never authenticates"]
    async fn actual_adapter_cancellation_before_install_and_concurrent_opens_cannot_start_sign_in()
    {
        let manager = GraphSessions::default();
        let first = manager.begin(&binding()).unwrap();
        manager.dispose().await; // Cancel while native approval/startup is pending.
        let fixture = Fixture::new();
        let worker = fixture.worker().await;
        assert!(manager.install(first, worker.clone()).await.is_err());
        assert!(worker.authenticate().await.is_err());
        assert!(fixture.calls().is_empty());
        let old = manager.begin(&binding()).unwrap();
        let newest = manager.begin(&binding()).unwrap();
        let fixture = Fixture::new();
        let worker = fixture.worker().await;
        assert!(manager.install(old, worker.clone()).await.is_err());
        assert!(worker.authenticate().await.is_err());
        assert!(fixture.calls().is_empty());
        let current = Fixture::new();
        let worker = current.worker().await;
        manager.install(newest, worker.clone()).await.unwrap();
        manager.ensure_current(newest, &worker).unwrap();
        worker.authenticate().await.unwrap();
        manager.probe(&binding()).await.unwrap();
        manager.dispose().await;
        let pending = manager.begin(&binding()).unwrap();
        manager.requirements_changed(&"b".repeat(64));
        let cancelled = Fixture::new();
        let worker = cancelled.worker().await;
        assert!(manager.install(pending, worker.clone()).await.is_err());
        assert!(worker.authenticate().await.is_err());
        assert!(cancelled.calls().is_empty());
    }

    #[tokio::test]
    #[ignore = "offline integration requires installed pwsh; substitutes MSAL/network and never authenticates"]
    async fn actual_adapter_timeout_expiry_and_eof_revoke_without_replay() {
        let fixture = Fixture::new();
        let worker = fixture.worker().await;
        worker.authenticate().await.unwrap();
        fixture.control("good", 10_000);
        assert!(worker
            .request_with_timeout("probe", false, Duration::from_millis(250))
            .await
            .is_err());
        assert!(worker.probe().await.is_err());
        assert!(!fixture
            .calls()
            .iter()
            .any(|call| call["kind"] == "protected-read-end"));
        let fixture = Fixture::new();
        let mut worker = fixture.worker().await;
        worker.authenticate().await.unwrap();
        Arc::get_mut(&mut worker).unwrap().created = Instant::now() - SESSION_LIFETIME;
        assert!(worker.probe().await.is_err());
        assert_eq!(fixture.calls().len(), 1);
        let fixture = Fixture::new();
        let worker = fixture.worker().await;
        worker.authenticate().await.unwrap();
        {
            let mut guard = worker.transport.lock().await;
            let transport = guard.as_mut().unwrap();
            transport.child.kill().await.unwrap();
            transport.child.wait().await.unwrap();
        }
        assert!(worker.probe().await.is_err());
        assert_eq!(fixture.calls().len(), 1);
    }
}
