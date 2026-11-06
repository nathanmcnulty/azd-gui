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
    sync::{
        atomic::{AtomicU8, Ordering},
        Arc, Mutex,
    },
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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub purpose: Option<String>,
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
            purpose: None,
        };
        binding.validate()?;
        Ok(binding)
    }

    pub(crate) fn validate(&self) -> AppResult<()> {
        if self
            .purpose
            .as_deref()
            .is_some_and(|purpose| purpose != "pimPlan")
        {
            return Err(invalid("Unknown Graph session purpose."));
        }
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

#[derive(Debug, Clone, Serialize, Deserialize)]
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
    #[serde(default)]
    principal_fingerprint: Option<String>,
    #[serde(default)]
    verification_reads: u32,
    #[serde(default)]
    pim_reads_attempted: u32,
    #[serde(default)]
    pim_reads_completed: u32,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ProcessEvidence {
    active: bool,
    process_id: u32,
    session_id: String,
    context: ContextEvidence,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
enum CancellationSnapshotSource {
    NativeRequestRevocationV1,
    AlreadyRevokedNativeWorkerV1,
    HistoricalProcessEvidenceV1,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct PimCancellationReceipt {
    requirements_digest: String,
    snapshot_source: CancellationSnapshotSource,
    // This is native RPC state, never proof of an interrupted HTTP transfer.
    // None means this action did not capture a fresh owned revocation.
    pim_plan_request_in_flight_at_revocation: Option<bool>,
    process_evidence: ProcessEvidence,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ProgressFrame {
    sequence: u64,
    session_id: String,
    requirements_digest: String,
    process_id: u32,
    kind: String,
    evidence: ContextEvidence,
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
    #[serde(default)]
    pim_plan: Option<serde_json::Value>,
    #[serde(default)]
    error_code: Option<String>,
}

struct Transport {
    child: Child,
    input: ChildStdin,
    output: BufReader<ChildStdout>,
    sequence: u64,
    _tree: Option<ProcessTree>,
    _files: tempfile::TempDir,
}

// An aborted request also revokes cached availability before Transport drops.
const PIM_REQUEST_IN_FLIGHT: u8 = 1;
const REQUESTS_REVOKED: u8 = 2;

struct PimRequestGuard<'a> {
    state: &'a AtomicU8,
}

impl<'a> PimRequestGuard<'a> {
    fn begin(state: &'a AtomicU8) -> AppResult<Self> {
        state
            .compare_exchange(
                0,
                PIM_REQUEST_IN_FLIGHT,
                Ordering::AcqRel,
                Ordering::Acquire,
            )
            .map_err(|_| invalid("PIM request is cancelled or unavailable."))?;
        Ok(Self { state })
    }
}

impl Drop for PimRequestGuard<'_> {
    fn drop(&mut self) {
        // Preserve permanent revocation when completion/abort races with cancel.
        self.state
            .fetch_and(!PIM_REQUEST_IN_FLIGHT, Ordering::AcqRel);
    }
}

struct RequestRevocation<'a> {
    cancel: watch::Sender<bool>,
    request_state: &'a AtomicU8,
    armed: bool,
}
impl Drop for RequestRevocation<'_> {
    fn drop(&mut self) {
        if self.armed {
            self.request_state
                .fetch_or(REQUESTS_REVOKED, Ordering::AcqRel);
            self.cancel.send_replace(true);
        }
    }
}

pub struct GraphWorker {
    created: Instant,
    session_id: String,
    pid: u32,
    binding: GraphBinding,
    transport: AsyncMutex<Option<Transport>>,
    cancel: watch::Sender<bool>,
    observed: Mutex<Option<ContextEvidence>>,
    request_state: AtomicU8,
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
        for (name, content) in [
            (
                "AzdPim.Common.psm1",
                include_str!("../pim-template/AzdPim.Common.psm1"),
            ),
            (
                "AzdPim.Graph.psm1",
                include_str!("../pim-template/AzdPim.Graph.psm1"),
            ),
        ] {
            std::fs::write(files.path().join(name), content)?;
        }
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
            observed: Mutex::new(None),
            request_state: AtomicU8::new(0),
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
        self.request_parameters(action, allow_interactive, timeout, None)
            .await
    }

    pub async fn pim_plan(
        &self,
        configuration: &crate::pim_plan::Configuration,
    ) -> AppResult<serde_json::Value> {
        self.request_parameters(
            "pimPlan",
            false,
            Duration::from_secs(180),
            Some(serde_json::to_value(configuration)?),
        )
        .await?
        .pim_plan
        .ok_or_else(|| invalid("PIM plan artifact absent."))
    }

    async fn request_parameters(
        &self,
        action: &str,
        allow_interactive: bool,
        timeout: Duration,
        parameters: Option<serde_json::Value>,
    ) -> AppResult<Reply> {
        let mut guard = self.transport.lock().await;
        let mut transport = guard
            .take()
            .ok_or_else(|| invalid("Graph session is closed or cancelled."))?;
        let mut revocation = RequestRevocation {
            cancel: self.cancel.clone(),
            request_state: &self.request_state,
            armed: true,
        };
        let mut cancelled = self.cancel.subscribe();
        if *cancelled.borrow() || self.created.elapsed() >= SESSION_LIFETIME {
            return Err(invalid("Graph session cancelled."));
        }
        transport.sequence += 1;
        let sequence = transport.sequence;
        let request = serde_json::json!({"action":action,"sequence":sequence,"sessionId":self.session_id,
            "requirementsDigest":self.binding.requirements_digest,"binding":self.binding,"allowInteractive":allow_interactive,"parameters":parameters});
        let mut bytes = serde_json::to_vec(&request)?;
        bytes.push(b'\n');
        // Only an admitted pimPlan RPC counts. Waiting on the transport mutex,
        // authentication, probing and a completed reply never create this marker.
        let _pim_request = if action == "pimPlan" {
            Some(PimRequestGuard::begin(&self.request_state)?)
        } else {
            None
        };
        let io = async {
            let limit = if action == "pimPlan" {
                16 * 1024 * 1024
            } else {
                LIMIT
            };
            transport.input.write_all(&bytes).await?;
            transport.input.flush().await?;
            for _ in 0..if action == "pimPlan" { 512 } else { 64 } {
                let mut line = Vec::new();
                let count = (&mut transport.output)
                    .take(limit)
                    .read_until(b'\n', &mut line)
                    .await?;
                if count == 0 || count as u64 >= limit {
                    return Err(invalid("Graph worker closed or returned oversized output."));
                }
                if let Some(data) = line.strip_prefix(b"AZD_GRAPH_PROGRESS:") {
                    if self.binding.purpose.as_deref() != Some("pimPlan")
                        || !matches!(action, "pimPlan" | "probe")
                        || data.len() > LIMIT as usize
                    {
                        return Err(invalid("Unexpected Graph progress frame."));
                    }
                    let progress: ProgressFrame = serde_json::from_slice(data)
                        .map_err(|_| invalid("Invalid Graph progress frame."))?;
                    self.validate_progress(progress, action, sequence)?;
                    continue;
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
                revocation.armed = false;
                Ok(reply)
            }
            Err(error) => {
                self.cancel();
                let _ = transport.child.kill().await;
                let _ = transport.child.wait().await;
                Err(error)
            }
        }
        // Dropping an interrupted request drops Transport, its process tree and
        // kill_on_drop child. A cancelled caller can never leave reusable proof.
    }

    fn validate_reply(&self, reply: Reply, action: &str, sequence: u64) -> AppResult<Reply> {
        if reply.sequence != sequence
            || reply.session_id != self.session_id
            || reply.process_id != self.pid
            || reply.requirements_digest != self.binding.requirements_digest
            || reply.action != action
        {
            return Err(invalid(
                "Graph worker evidence did not match this operation/session.",
            ));
        }
        if !reply.ok {
            let message = match reply.error_code.as_deref() {
                Some("GraphReadDenied") => "Graph read denied; stop and review the existing account's access. No permissions were changed.",
                Some("GraphDelegatedScopeMissing") => "The returned delegated grant does not include the required read permission. Do not grant consent automatically.",
                Some("GraphIdentityMismatch" | "GraphIdentityOrGrantsChanged") => "Graph identity or delegated grants did not match the approved session.",
                Some("PimVerificationBudgetExceeded" | "PimReadBudgetOrSessionRejected" | "PimResponseLimitExceeded") => "PIM read budget or response limit rejected the operation.",
                Some("GraphReadBadRequest") => "The protected Graph request was rejected as invalid (HTTP 400). Review the registered query contract; no retry was made.",
                Some("GraphReadNotFound") => "The registered Graph read returned not found (HTTP 404); no retry was made.",
                Some("GraphReadThrottled") => "Graph throttled the protected read (HTTP 429); no retry was made.",
                Some("GraphRedirectRejected") => "The protected Graph read returned a redirect. It was rejected without following it or retrying.",
                Some("GraphReadFailed") => "The protected Graph read failed; no retry was made.",
                Some("PimPlannerRejectedConfiguration") => "PIM planner could not build a plan from the selected configuration and returned policy data. No plan was accepted; the session was discarded.",
                Some("PimContextAlreadyExists") => "An authentication context with the planned name already exists. Ownership/adoption review is required; nothing was adopted or changed.",
                Some("PimContextDuplicateNames") => "Multiple authentication contexts have the planned name. Resolve the naming conflict before another plan; nothing was changed.",
                Some("PimContextSlotsFull") => "No unused authentication context slot is available to this planner. Review existing context usage; nothing was changed.",
                Some("PimConditionalAccessAlreadyExists") => "A Conditional Access policy with the planned name already exists. Ownership/adoption review is required; nothing was adopted or changed.",
                Some("PimConditionalAccessDuplicateNames") => "Multiple Conditional Access policies have the planned name. Resolve the naming conflict before another plan; nothing was changed.",
                Some("PimRoleContextConflict") => "A selected role already requires a different authentication context. Review that conflict before another plan; nothing was adopted or changed.",
                Some("PimPlannerPolicyDataInvalid") => "Returned planner inventory is incomplete or has an unsupported data shape. No plan was accepted; the session was discarded.",
                Some("PimRoleReferenceDataInvalid") => "Returned role-policy reference data is incomplete or has an unsupported shape. No plan was accepted; the session was discarded.",
                Some("PimContextIdentityDataInvalid") => "Returned authentication context identity data is incomplete or has an unsupported shape. No plan was accepted; the session was discarded.",
                Some("PimContextSlotDataInvalid") => "Returned authentication context availability or ownership metadata is incomplete or has an unsupported shape. No plan was accepted; the session was discarded.",
                Some("PimConditionalAccessReferenceDataInvalid") => "Returned Conditional Access reference data has an unsupported shape. No plan was accepted; the session was discarded.",
                Some("PimRolePolicyDataInvalid") => "Returned role-policy assignment or rule data is incomplete or has an unsupported shape. No plan was accepted; the session was discarded.",
                Some("PimReadFailed") => "PIM read failed validation or exceeded its time limit; the session was discarded.",
                Some("GraphAuthenticationFailed") => "Graph sign-in failed. Stop on consent, unexpected identity or an alternate authentication flow.",
                Some("GraphInteractionRequired" | "GraphTokenRefreshFailed") => "Silent Graph refresh could not complete. No interactive fallback was attempted; review the account before another approved sign-in.",
                _ => "Graph worker rejected the operation; session discarded.",
            };
            return Err(invalid(message));
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
            self.observe(evidence)?;
        }
        Ok(reply)
    }

    fn observe(&self, evidence: &ContextEvidence) -> AppResult<()> {
        let mut observed = self
            .observed
            .lock()
            .map_err(|_| invalid("Graph evidence unavailable."))?;
        if self.binding.purpose.as_deref() == Some("pimPlan")
            && (evidence
                .principal_fingerprint
                .as_ref()
                .is_none_or(|s| s.len() != 64 || !s.bytes().all(|b| b.is_ascii_hexdigit()))
                || evidence.verification_reads > 1
                || evidence.pim_reads_attempted > 100
                || evidence.pim_reads_completed > evidence.pim_reads_attempted
                || evidence.pim_reads_attempted - evidence.pim_reads_completed > 1)
        {
            return Err(invalid("Invalid PIM execution evidence."));
        }
        if let Some(previous) = observed.as_ref() {
            if evidence.principal_fingerprint != previous.principal_fingerprint
                || evidence.scopes.len() != previous.scopes.len()
                || evidence
                    .scopes
                    .iter()
                    .zip(&previous.scopes)
                    .any(|(actual, prior)| !actual.eq_ignore_ascii_case(prior))
                || evidence.verification_reads < previous.verification_reads
                || evidence.pim_reads_attempted < previous.pim_reads_attempted
                || evidence.pim_reads_completed < previous.pim_reads_completed
            {
                return Err(invalid(
                    "Observed Graph identity, grants or counters changed unexpectedly.",
                ));
            }
        }
        *observed = Some(evidence.clone());
        Ok(())
    }

    fn validate_progress(
        &self,
        progress: ProgressFrame,
        action: &str,
        sequence: u64,
    ) -> AppResult<()> {
        let previous = self
            .observed
            .lock()
            .map_err(|_| invalid("Graph evidence unavailable."))?
            .clone()
            .ok_or_else(|| invalid("No prior verified Graph evidence."))?;
        let evidence = &progress.evidence;
        let valid = evidence.verification_reads == 1
            && match progress.kind.as_str() {
                "verificationStarted" => {
                    action == "probe"
                        && previous.verification_reads == 0
                        && evidence.pim_reads_attempted == 0
                        && evidence.pim_reads_completed == 0
                }
                "verificationCompleted" => {
                    action == "probe"
                        && previous.verification_reads == 1
                        && evidence.pim_reads_attempted == 0
                        && evidence.pim_reads_completed == 0
                }
                "readStarted" if action == "pimPlan" => {
                    evidence.pim_reads_attempted == previous.pim_reads_attempted + 1
                        && evidence.pim_reads_completed == previous.pim_reads_completed
                }
                "readCompleted" if action == "pimPlan" => {
                    evidence.pim_reads_completed == previous.pim_reads_completed + 1
                        && evidence.pim_reads_attempted == previous.pim_reads_attempted
                }
                _ => false,
            };
        if !valid {
            return Err(invalid("Invalid PIM read progress sequence."));
        }
        self.validate_reply(
            Reply {
                sequence: progress.sequence,
                session_id: progress.session_id,
                requirements_digest: progress.requirements_digest,
                process_id: progress.process_id,
                action: action.into(),
                ok: true,
                evidence: Some(progress.evidence),
                pim_plan: None,
                error_code: None,
            },
            action,
            sequence,
        )?;
        Ok(())
    }

    pub(crate) fn process_evidence(&self) -> AppResult<ProcessEvidence> {
        let context = self
            .observed
            .lock()
            .map_err(|_| invalid("Graph evidence unavailable."))?
            .clone()
            .ok_or_else(|| invalid("Verified Graph evidence absent."))?;
        let exited = self.transport.try_lock().ok().is_some_and(|mut transport| {
            transport.as_mut().is_none_or(|transport| {
                transport
                    .child
                    .try_wait()
                    .map_or(true, |status| status.is_some())
            })
        });
        if exited || self.created.elapsed() >= SESSION_LIFETIME {
            self.cancel();
        }
        Ok(ProcessEvidence {
            active: self.request_state.load(Ordering::Acquire) & REQUESTS_REVOKED == 0
                && !*self.cancel.borrow(),
            process_id: self.pid,
            session_id: self.session_id.clone(),
            context,
        })
    }

    pub fn cancel(&self) {
        let _ = self.cancel_with_snapshot();
    }

    fn cancel_with_snapshot(&self) -> Option<bool> {
        let previous = self
            .request_state
            .fetch_or(REQUESTS_REVOKED, Ordering::AcqRel);
        self.cancel.send_replace(true);
        (previous & REQUESTS_REVOKED == 0).then_some(previous & PIM_REQUEST_IN_FLIGHT != 0)
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
    last_pim_failure: Mutex<Option<(String, ProcessEvidence)>>,
}

impl GraphSessions {
    pub async fn dispose_pim_with_receipt(
        &self,
        digest: &str,
    ) -> AppResult<Option<PimCancellationReceipt>> {
        let (worker, mut receipt) = {
            let mut registry = self
                .current
                .lock()
                .map_err(|_| invalid("Graph sessions unavailable."))?;
            if registry.requirements_digest.as_deref() == Some(digest) {
                let receipt = registry
                    .worker
                    .as_ref()
                    .filter(|worker| {
                        worker.binding.purpose.as_deref() == Some("pimPlan")
                            && worker.binding.requirements_digest == digest
                    })
                    .and_then(|worker| {
                        // Linearize the marker with revocation before any await.
                        // Later counters are metadata observations, not wire proof.
                        let snapshot = worker.cancel_with_snapshot();
                        worker.process_evidence().ok().map(|mut evidence| {
                            evidence.active = false;
                            PimCancellationReceipt {
                                requirements_digest: digest.into(),
                                snapshot_source: if snapshot.is_some() {
                                    CancellationSnapshotSource::NativeRequestRevocationV1
                                } else {
                                    CancellationSnapshotSource::AlreadyRevokedNativeWorkerV1
                                },
                                pim_plan_request_in_flight_at_revocation: snapshot,
                                process_evidence: evidence,
                            }
                        })
                    });
                // Also revokes a pending installation with no worker/evidence.
                (registry.revoke(), receipt)
            } else {
                (None, None)
            }
        };
        if let Ok(mut cache) = self.last_pim_failure.lock() {
            if cache.as_ref().is_some_and(|(owner, _)| owner == digest) {
                if worker.is_none() && receipt.is_none() {
                    receipt = cache.take().map(|(_, mut evidence)| {
                        evidence.active = false;
                        PimCancellationReceipt {
                            requirements_digest: digest.into(),
                            snapshot_source:
                                CancellationSnapshotSource::HistoricalProcessEvidenceV1,
                            pim_plan_request_in_flight_at_revocation: None,
                            process_evidence: evidence,
                        }
                    });
                } else {
                    *cache = None;
                }
            }
        }
        // Missing metadata must never prevent pending/auth/request cancellation.
        if let Some(worker) = worker {
            worker.close().await;
        }
        Ok(receipt)
    }
    pub fn pim_evidence(&self, digest: &str) -> AppResult<Option<ProcessEvidence>> {
        let registry = self
            .current
            .lock()
            .map_err(|_| invalid("Graph sessions unavailable."))?;
        let active = registry
            .worker
            .as_ref()
            .filter(|w| {
                w.binding.purpose.as_deref() == Some("pimPlan")
                    && w.binding.requirements_digest == digest
            })
            .map(|w| w.process_evidence())
            .transpose()?;
        if active.is_some() {
            return Ok(active);
        }
        Ok(self
            .last_pim_failure
            .lock()
            .map_err(|_| invalid("PIM receipt unavailable."))?
            .as_ref()
            .filter(|(owner, _)| owner == digest)
            .map(|(_, evidence)| evidence.clone()))
    }

    pub fn retain_pim_failure(&self, worker: &GraphWorker) {
        if worker.binding.purpose.as_deref() != Some("pimPlan") {
            return;
        }
        let Ok(registry) = self.current.lock() else {
            return;
        };
        if !registry
            .worker
            .as_ref()
            .is_some_and(|current| std::ptr::eq(current.as_ref(), worker))
        {
            return;
        }
        if let (Ok(mut cache), Ok(mut evidence)) =
            (self.last_pim_failure.lock(), worker.process_evidence())
        {
            evidence.active = false;
            *cache = Some((worker.binding.requirements_digest.clone(), evidence));
        }
    }
    pub async fn dispose_matching(&self, digest: &str) {
        let worker = self.current.lock().ok().and_then(|mut registry| {
            if registry.requirements_digest.as_deref() == Some(digest) {
                registry.revoke()
            } else {
                None
            }
        });
        if let Ok(mut cache) = self.last_pim_failure.lock() {
            if cache.as_ref().is_some_and(|(owner, _)| owner == digest) {
                *cache = None;
            }
        }
        if let Some(worker) = worker {
            worker.close().await;
        }
    }
    pub async fn pim_plan(&self, review: &crate::pim_plan::Review) -> AppResult<serde_json::Value> {
        let (worker, ticket) = {
            let mut registry = self
                .current
                .lock()
                .map_err(|_| invalid("Graph sessions unavailable."))?;
            if registry
                .worker
                .as_ref()
                .is_some_and(|w| w.binding != review.binding)
            {
                registry.revoke();
            }
            let worker = registry
                .worker
                .clone()
                .filter(|w| w.binding == review.binding)
                .ok_or_else(|| {
                    invalid("Approve an owned session for this exact PIM configuration first.")
                })?;
            (worker, registry.generation)
        };
        let result = worker.pim_plan(&review.configuration).await;
        if result.is_err() {
            self.retain_pim_failure(&worker);
            self.invalidate_worker(&worker);
        }
        let plan = result?;
        self.ensure_current(ticket, &worker)?;
        if plan
            .get("tenantId")
            .and_then(|v| v.as_str())
            .is_none_or(|tenant| !tenant.eq_ignore_ascii_case(&review.binding.tenant_id))
            || plan.get("mode").and_then(|v| v.as_str()) != Some("plan")
        {
            self.invalidate_worker(&worker);
            return Err(invalid("PIM plan identity or mode mismatch."));
        }
        if let Err(error) = review.configuration.validate_preview_result(&plan) {
            self.invalidate_worker(&worker);
            return Err(error);
        }
        let execution_evidence = worker.process_evidence()?;
        if !execution_evidence.active {
            return Err(invalid("PIM session was cancelled; artifact discarded."));
        }
        Ok(
            serde_json::json!({ "schemaVersion": "1.0", "sourceCommit": review.source_commit,
            "requirementsDigest": review.binding.requirements_digest, "identity": review.binding,
            "configuration": review.configuration, "previewExistingObjects": review.configuration.preview_existing_objects,
            "ownershipReviewRequired": true, "ownershipEstablished": false, "executionAuthorized": false,
            "ownershipReviewNotice": review.configuration.existing_objects_notice(),
            "executionEvidence": execution_evidence, "plan": plan }),
        )
    }
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
        if let Ok(mut cache) = self.last_pim_failure.lock() {
            *cache = None;
        }
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

    fn pim_review() -> crate::pim_plan::Review {
        pim_review_with_preview(false)
    }

    fn pim_review_with_preview(preview: bool) -> crate::pim_plan::Review {
        serde_json::from_value::<crate::pim_plan::Input>(json!({
            "reviewId":"99999999-9999-4999-8999-999999999999", "account":"operator@example.test", "tenantId":"11111111-1111-4111-8111-111111111111",
            "clientId":"22222222-2222-4222-8222-222222222222", "configuration": {
                "previewExistingObjects":preview,
                "privilegedRoleIds":["33333333-3333-4333-8333-333333333333"], "lessPrivilegedRoleIds":[],
                "privilegedAuthenticationProfile":"mfa", "lessPrivilegedAuthenticationProfile":"mfa",
                "privilegedDeviceRequirement":"none", "lessPrivilegedDeviceRequirement":"none"
            }
        })).unwrap().review().unwrap()
    }

    #[test]
    fn pim_diagnostic_locations_match_the_exact_pinned_planner() {
        use sha2::{Digest, Sha256};
        let planner = include_str!("../pim-template/AzdPim.Graph.psm1");
        let worker = include_str!("graph_worker.ps1");
        assert!(worker.contains(&format!("{:x}", Sha256::digest(planner.as_bytes()))));
        for (line, text) in [
            (283, "$policy.rules"),
            (284, "$rule.isEnabled"),
            (452, "$_.displayName"),
            (454, "Multiple authentication contexts"),
            (458, "already exists as"),
            (468, "$unused ="),
            (479, "No unused authentication context slots"),
            (639, "Multiple Conditional Access policies"),
            (655, "Conditional Access policy"),
            (768, "already requires authentication context"),
        ] {
            assert!(planner.lines().nth(line - 1).unwrap().contains(text));
        }
    }

    #[tokio::test]
    #[ignore = "offline real PowerShell PIM diagnostics; substitutes MSAL and HTTP"]
    async fn pinned_planner_failures_have_closed_safe_diagnostics_and_revoke_the_worker() {
        const CONTEXTS: &str = "https://graph.microsoft.com/v1.0/identity/conditionalAccess/authenticationContextClassReferences";
        const POLICIES: &str =
            "https://graph.microsoft.com/beta/identity/conditionalAccess/policies?$top=999";
        const ROLE_POLICIES: &str = "https://graph.microsoft.com/v1.0/policies/roleManagementPolicies?$filter=scopeId%20eq%20%27%2F%27%20and%20scopeType%20eq%20%27DirectoryRole%27&$expand=rules&$top=999";
        const INVENTORY: &str = "https://graph.microsoft.com/beta/roleManagement/directory/roleDefinitions?$filter=isPrivileged%20eq%20true&$skiptoken=offline-fixture";
        const SECRET: &str = "fixture-sensitive-role-or-policy-value";
        for (case, expected) in [
            (
                "context_exists",
                "An authentication context with the planned name already exists.",
            ),
            (
                "context_duplicate",
                "Multiple authentication contexts have the planned name.",
            ),
            (
                "slots_full",
                "No unused authentication context slot is available",
            ),
            (
                "ca_exists",
                "A Conditional Access policy with the planned name already exists.",
            ),
            (
                "ca_duplicate",
                "Multiple Conditional Access policies have the planned name.",
            ),
            (
                "role_conflict",
                "A selected role already requires a different authentication context.",
            ),
            (
                "context_identity",
                "Returned authentication context identity data is incomplete",
            ),
            (
                "context_slot",
                "Returned authentication context availability or ownership metadata is incomplete",
            ),
            (
                "role_references",
                "Returned role-policy reference data is incomplete",
            ),
            (
                "ca_references",
                "Returned Conditional Access reference data has an unsupported shape.",
            ),
            (
                "role_policy",
                "Returned role-policy assignment or rule data is incomplete",
            ),
            ("changed_pin", "PIM planner could not build a plan"),
        ] {
            let fixture = Fixture::new();
            let review = pim_review();
            let worker = fixture.pim_worker(&review).await;
            let mut responses: Value =
                serde_json::from_str(include_str!("../graph-adapter-tests/pim-responses.json"))
                    .unwrap();
            responses[INVENTORY]["value"][0]["displayName"] = json!(SECRET);
            match case {
                "context_exists" | "changed_pin" => responses[CONTEXTS]["value"] = json!([{ "id":SECRET, "displayName":"PIM Privileged Roles" }]),
                "context_duplicate" => responses[CONTEXTS]["value"] = json!([{ "id":SECRET, "displayName":"PIM Privileged Roles" }, {"id":"c2", "displayName":"PIM Privileged Roles"}]),
                "slots_full" => responses[CONTEXTS]["value"] = json!((1..=25).map(|n| json!({"id":format!("c{n}"),"displayName":format!("{SECRET}-{n}"),"isAvailable":true,"description":SECRET})).collect::<Vec<_>>()),
                "ca_exists" => responses[POLICIES]["value"] = json!([{ "id":SECRET, "displayName":"PIM - Privileged Roles - Require reauthentication", "conditions":{} }]),
                "ca_duplicate" => responses[POLICIES]["value"] = json!([{ "id":SECRET, "displayName":"PIM - Privileged Roles - Require reauthentication", "conditions":{} }, {"id":"second-policy","displayName":"PIM - Privileged Roles - Require reauthentication","conditions":{}}]),
                "role_conflict" => { responses[ROLE_POLICIES]["value"][0]["rules"][0]["isEnabled"] = json!(true); responses[ROLE_POLICIES]["value"][0]["rules"][0]["claimValue"] = json!("c2"); },
                "context_identity" => responses[CONTEXTS]["value"] = json!([{ "id":SECRET }]),
                "context_slot" => responses[CONTEXTS]["value"] = json!([{ "id":"c1", "displayName":"", "description":SECRET }]),
                "role_references" => { responses[ROLE_POLICIES]["value"][0].as_object_mut().unwrap().remove("rules"); },
                "ca_references" => responses[POLICIES]["value"] = json!([{ "id":SECRET, "conditions":{"applications":{"includeAuthenticationContextClassReferences":SECRET}} }]),
                "role_policy" => responses[ROLE_POLICIES]["value"][0]["rules"] = json!([]),
                _ => unreachable!(),
            }
            std::fs::write(
                fixture.root.path().join("responses.json"),
                responses.to_string(),
            )
            .unwrap();
            if case == "changed_pin" {
                let transport = worker.transport.lock().await;
                let planner = transport
                    .as_ref()
                    .unwrap()
                    ._files
                    .path()
                    .join("AzdPim.Graph.psm1");
                let mut source = std::fs::read_to_string(&planner).unwrap();
                source.push_str("\n# offline changed-pin diagnostic guard\n");
                std::fs::write(planner, source).unwrap();
            }
            worker.authenticate().await.unwrap();
            worker.probe().await.unwrap();
            let error = worker
                .pim_plan(&review.configuration)
                .await
                .unwrap_err()
                .to_string();
            assert!(error.contains(expected), "{case}: {error}");
            assert!(
                !error.contains(SECRET),
                "{case}: private fixture value leaked"
            );
            assert!(!error.contains("AzdPim.Graph.psm1") && !error.contains("PropertyNotFound"));
            assert!(*worker.cancel.borrow());
            let calls = fixture.calls().len();
            assert!(worker.pim_plan(&review.configuration).await.is_err());
            assert_eq!(fixture.calls().len(), calls, "{case}: retry was dispatched");
            assert!(worker.transport.lock().await.is_none());
        }
    }

    #[tokio::test]
    #[ignore = "offline real PowerShell HTTP diagnostics; substitutes MSAL and HTTP"]
    async fn http_failure_categories_are_fixed_and_revoke_without_retry() {
        for (status, expected) in [
            (400, "rejected as invalid (HTTP 400)"),
            (401, "Graph read denied"),
            (403, "Graph read denied"),
            (404, "returned not found (HTTP 404)"),
            (429, "throttled the protected read (HTTP 429)"),
            (300, "returned a redirect"),
            (302, "returned a redirect"),
            (307, "returned a redirect"),
            (399, "returned a redirect"),
            (409, "protected Graph read failed; no retry"),
            (500, "protected Graph read failed; no retry"),
            (503, "protected Graph read failed; no retry"),
        ] {
            let fixture = Fixture::new();
            let review = pim_review();
            let worker = fixture.pim_worker(&review).await;
            worker.authenticate().await.unwrap();
            worker.probe().await.unwrap();
            std::fs::write(fixture.root.path().join("control.json"), json!({
                "account":"operator@example.test", "mode":"pim", "delayMs":0, "responseStatus":status
            }).to_string()).unwrap();
            let error = worker
                .pim_plan(&review.configuration)
                .await
                .unwrap_err()
                .to_string();
            assert!(error.contains(expected), "HTTP {status}: {error}");
            assert!(
                !error.contains("offline-only-token")
                    && !error.contains("https://")
                    && !error.contains("operator@example.test")
            );
            assert!(*worker.cancel.borrow());
            let attempts = fixture
                .calls()
                .iter()
                .filter(|call| call["kind"] == "protected-read-start")
                .count();
            assert_eq!(attempts, 2, "HTTP {status}: unexpected redirect or retry");
            assert!(worker.pim_plan(&review.configuration).await.is_err());
            assert_eq!(
                fixture
                    .calls()
                    .iter()
                    .filter(|call| call["kind"] == "protected-read-start")
                    .count(),
                attempts
            );
            assert!(worker.transport.lock().await.is_none());
        }
    }

    #[tokio::test]
    #[ignore = "offline real PowerShell PIM evidence; substitutes MSAL and HTTP"]
    async fn pim_observed_receipt_and_polling_never_add_reads_or_authorize_after_failure() {
        let fixture = Fixture::new();
        let review = pim_review();
        let worker = fixture.pim_worker(&review).await;
        let control = fixture.root.path().join("control.json");
        std::fs::write(
            &control,
            json!({"account":"operator@example.test","mode":"pim","delayMs":0,"extraGrants":true})
                .to_string(),
        )
        .unwrap();
        worker.authenticate().await.unwrap();
        worker.probe().await.unwrap();
        let sessions = GraphSessions::default();
        sessions.replace(worker.clone()).await.unwrap();
        let calls = fixture.calls().len();
        for _ in 0..20 {
            let receipt = sessions
                .pim_evidence(&review.binding.requirements_digest)
                .unwrap()
                .unwrap();
            assert!(receipt.active);
            assert_eq!(receipt.context.verification_reads, 1);
            assert!(receipt.context.scopes.contains(&"User.Read".into()));
            assert!(!review.binding.scopes.contains(&"User.Read".into()));
            assert_eq!(
                receipt
                    .context
                    .principal_fingerprint
                    .as_ref()
                    .unwrap()
                    .len(),
                64
            );
        }
        assert_eq!(fixture.calls().len(), calls);
        std::fs::write(&control, json!({"account":"operator@example.test","mode":"pim","delayMs":0,"extraGrants":true,"responseStatus":403}).to_string()).unwrap();
        let error = sessions.pim_plan(&review).await.unwrap_err().to_string();
        assert!(error.contains("Graph read denied"), "{error}");
        let receipt = sessions
            .pim_evidence(&review.binding.requirements_digest)
            .unwrap()
            .unwrap();
        assert!(!receipt.active);
        assert_eq!(receipt.context.pim_reads_attempted, 1);
        assert_eq!(receipt.context.pim_reads_completed, 0);
        assert!(sessions.pim_plan(&review).await.is_err());
        assert_eq!(
            fixture
                .calls()
                .iter()
                .filter(|v| v["kind"] == "protected-read-start")
                .count(),
            2
        );
        sessions
            .dispose_matching(&review.binding.requirements_digest)
            .await;
        assert!(sessions
            .pim_evidence(&review.binding.requirements_digest)
            .unwrap()
            .is_none());
    }

    #[tokio::test]
    #[ignore = "offline real PowerShell PIM evidence; substitutes MSAL and HTTP"]
    async fn pim_purpose_allows_only_one_verification_and_rejects_foreign_progress() {
        let fixture = Fixture::new();
        let review = pim_review();
        let worker = fixture.pim_worker(&review).await;
        worker.authenticate().await.unwrap();
        worker.probe().await.unwrap();
        assert!(worker
            .probe()
            .await
            .unwrap_err()
            .to_string()
            .contains("PIM read budget"));
        assert_eq!(
            fixture
                .calls()
                .iter()
                .filter(|v| v["kind"] == "protected-read-start")
                .count(),
            1
        );

        let fixture = Fixture::new();
        let worker = fixture.pim_worker(&review).await;
        worker.authenticate().await.unwrap();
        worker.probe().await.unwrap();
        std::fs::write(fixture.root.path().join("control.json"), json!({"account":"operator@example.test","mode":"pim","delayMs":0,"spoofPimProgress":true}).to_string()).unwrap();
        assert!(worker.pim_plan(&review.configuration).await.is_err());
        assert!(!worker.process_evidence().unwrap().active);
        assert!(worker.pim_plan(&review.configuration).await.is_err());
    }

    #[tokio::test]
    #[ignore = "offline real PowerShell PIM diagnostics; substitutes MSAL and HTTP"]
    async fn silent_pim_refresh_failure_is_distinguished_without_interactive_fallback_or_dispatch()
    {
        let fixture = Fixture::new();
        let review = pim_review();
        let worker = fixture.pim_worker(&review).await;
        worker.authenticate().await.unwrap();
        worker.probe().await.unwrap();
        let sessions = GraphSessions::default();
        sessions.replace(worker).await.unwrap();
        fixture.control("silentRequired", 0);
        let error = sessions.pim_plan(&review).await.unwrap_err().to_string();
        assert!(error.contains("Silent Graph refresh"), "{error}");
        let receipt = sessions
            .pim_evidence(&review.binding.requirements_digest)
            .unwrap()
            .unwrap();
        assert!(!receipt.active);
        assert_eq!(receipt.context.pim_reads_attempted, 0);
        assert_eq!(
            fixture
                .calls()
                .iter()
                .filter(|v| v["kind"] == "connect")
                .count(),
            1
        );
        assert_eq!(
            fixture
                .calls()
                .iter()
                .filter(|v| v["kind"] == "protected-read-start")
                .count(),
            1
        );
        sessions.dispose().await;
    }

    #[tokio::test]
    #[ignore = "offline real PowerShell PIM evidence; substitutes MSAL and HTTP"]
    async fn aborted_pim_read_revokes_cached_availability_and_global_disposal_clears_failure_receipt(
    ) {
        let fixture = Fixture::new();
        let review = pim_review();
        let worker = fixture.pim_worker(&review).await;
        worker.authenticate().await.unwrap();
        worker.probe().await.unwrap();
        let sessions = GraphSessions::default();
        sessions.replace(worker.clone()).await.unwrap();
        fixture.control("pim", 10000);
        let task_worker = worker.clone();
        let configuration = review.configuration.clone();
        let task = tokio::spawn(async move { task_worker.pim_plan(&configuration).await });
        tokio::time::timeout(Duration::from_secs(8), async {
            while worker
                .process_evidence()
                .unwrap()
                .context
                .pim_reads_attempted
                < 1
            {
                tokio::time::sleep(Duration::from_millis(30)).await;
            }
        })
        .await
        .unwrap();
        task.abort();
        assert!(task.await.is_err());
        assert_eq!(
            worker.request_state.load(Ordering::Acquire),
            REQUESTS_REVOKED
        );
        assert!(PimRequestGuard::begin(&worker.request_state).is_err());
        assert!(
            !sessions
                .pim_evidence(&review.binding.requirements_digest)
                .unwrap()
                .unwrap()
                .active
        );
        assert!(worker.pim_plan(&review.configuration).await.is_err());
        sessions.retain_pim_failure(&worker);
        sessions.invalidate_worker(&worker);
        assert!(sessions
            .pim_evidence(&review.binding.requirements_digest)
            .unwrap()
            .is_some());
        sessions.dispose().await;
        assert!(sessions
            .pim_evidence(&review.binding.requirements_digest)
            .unwrap()
            .is_none());
        sessions.retain_pim_failure(&worker);
        assert!(sessions
            .pim_evidence(&review.binding.requirements_digest)
            .unwrap()
            .is_none());
    }

    impl Fixture {
        async fn pim_worker(&self, review: &crate::pim_plan::Review) -> Arc<GraphWorker> {
            self.control("pim", 0);
            std::fs::write(
                self.root.path().join("responses.json"),
                include_str!("../graph-adapter-tests/pim-responses.json"),
            )
            .unwrap();
            GraphWorker::spawn_with_module(
                review.binding.clone(),
                Some(&self.root.path().join("Fake.Graph.psm1")),
            )
            .await
            .unwrap()
        }
    }

    #[tokio::test]
    #[ignore = "offline real PowerShell cancellation snapshots; substitutes MSAL and HTTP"]
    async fn cancellation_receipt_snapshots_native_pim_request_state_and_digest() {
        // Delayed planner: record the native request state, revoke, and discard
        // every late result. No UI timing or transport-lock heuristic is used.
        let fixture = Fixture::new();
        let review = pim_review();
        let digest = review.binding.requirements_digest.clone();
        let worker = fixture.pim_worker(&review).await;
        worker.authenticate().await.unwrap();
        worker.probe().await.unwrap();
        let sessions = Arc::new(GraphSessions::default());
        sessions.replace(worker.clone()).await.unwrap();
        fixture.control("pim", 10000);
        let planning = sessions.clone();
        let task = tokio::spawn(async move { planning.pim_plan(&review).await });
        tokio::time::timeout(Duration::from_secs(8), async {
            while worker
                .process_evidence()
                .unwrap()
                .context
                .pim_reads_attempted
                < 1
            {
                tokio::time::sleep(Duration::from_millis(20)).await;
            }
        })
        .await
        .unwrap();
        assert!(sessions
            .dispose_pim_with_receipt(&"f".repeat(64))
            .await
            .unwrap()
            .is_none());
        assert_eq!(
            worker.request_state.load(Ordering::Acquire),
            PIM_REQUEST_IN_FLIGHT
        );
        let receipt = sessions
            .dispose_pim_with_receipt(&digest)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(receipt.requirements_digest, digest);
        assert!(matches!(
            receipt.snapshot_source,
            CancellationSnapshotSource::NativeRequestRevocationV1
        ));
        assert_eq!(receipt.pim_plan_request_in_flight_at_revocation, Some(true));
        assert!(!receipt.process_evidence.active);
        assert_eq!(receipt.process_evidence.session_id, worker.session_id);
        assert!(task.await.unwrap().is_err());
        assert_eq!(
            worker.request_state.load(Ordering::Acquire),
            REQUESTS_REVOKED
        );
        assert!(sessions.pim_evidence(&digest).unwrap().is_none());
        assert!(worker.pim_plan(&pim_review().configuration).await.is_err());

        // Completed planner plus artificial mutex contention is NOT in flight.
        let fixture = Fixture::new();
        let review = pim_review();
        let digest = review.binding.requirements_digest.clone();
        let worker = fixture.pim_worker(&review).await;
        worker.authenticate().await.unwrap();
        worker.probe().await.unwrap();
        let sessions = Arc::new(GraphSessions::default());
        sessions.replace(worker.clone()).await.unwrap();
        sessions.pim_plan(&review).await.unwrap();
        assert_eq!(worker.request_state.load(Ordering::Acquire), 0);
        let held_transport = worker.transport.lock().await;
        let queued = worker.pim_plan(&review.configuration);
        tokio::pin!(queued);
        tokio::select! {
            _ = &mut queued => panic!("queued request unexpectedly acquired held transport"),
            _ = tokio::time::sleep(Duration::from_millis(25)) => {}
        }
        assert_eq!(worker.request_state.load(Ordering::Acquire), 0);
        let calls_before = fixture.calls().len();
        let cancelling = sessions.clone();
        let expected_digest = digest.clone();
        let cancel =
            tokio::spawn(
                async move { cancelling.dispose_pim_with_receipt(&expected_digest).await },
            );
        tokio::time::timeout(Duration::from_secs(8), async {
            while worker.request_state.load(Ordering::Acquire) & REQUESTS_REVOKED == 0 {
                tokio::time::sleep(Duration::from_millis(20)).await;
            }
        })
        .await
        .unwrap();
        drop(held_transport);
        assert!(queued.await.is_err());
        let receipt = cancel.await.unwrap().unwrap().unwrap();
        assert_eq!(receipt.requirements_digest, digest);
        assert_eq!(
            receipt.pim_plan_request_in_flight_at_revocation,
            Some(false)
        );
        assert_eq!(receipt.process_evidence.context.pim_reads_completed, 7);
        assert!(matches!(
            receipt.snapshot_source,
            CancellationSnapshotSource::NativeRequestRevocationV1
        ));
        assert_eq!(fixture.calls().len(), calls_before);

        // A live probe is a different action and must not count as a PIM RPC.
        let fixture = Fixture::new();
        let review = pim_review();
        let worker = fixture.pim_worker(&review).await;
        worker.authenticate().await.unwrap();
        let sessions = GraphSessions::default();
        sessions.replace(worker.clone()).await.unwrap();
        fixture.control("pim", 10000);
        let probing = worker.clone();
        let probe = tokio::spawn(async move { probing.probe().await });
        fixture.wait_for_read().await;
        let receipt = sessions
            .dispose_pim_with_receipt(&review.binding.requirements_digest)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(
            receipt.pim_plan_request_in_flight_at_revocation,
            Some(false)
        );
        assert!(probe.await.unwrap().is_err());
        assert_eq!(
            worker.request_state.load(Ordering::Acquire),
            REQUESTS_REVOKED
        );

        // An earlier failed/revoked request supplies historical metadata only.
        let fixture = Fixture::new();
        let review = pim_review();
        let digest = review.binding.requirements_digest.clone();
        let worker = fixture.pim_worker(&review).await;
        worker.authenticate().await.unwrap();
        worker.probe().await.unwrap();
        let sessions = GraphSessions::default();
        sessions.replace(worker.clone()).await.unwrap();
        std::fs::write(fixture.root.path().join("control.json"), json!({"account":"operator@example.test","mode":"pim","delayMs":0,"responseStatus":403}).to_string()).unwrap();
        assert!(sessions.pim_plan(&review).await.is_err());
        assert_eq!(
            worker.request_state.load(Ordering::Acquire),
            REQUESTS_REVOKED
        );
        let receipt = sessions
            .dispose_pim_with_receipt(&digest)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(receipt.requirements_digest, digest);
        assert_eq!(receipt.pim_plan_request_in_flight_at_revocation, None);
        assert!(matches!(
            receipt.snapshot_source,
            CancellationSnapshotSource::HistoricalProcessEvidenceV1
        ));
        assert!(!receipt.process_evidence.active);
    }

    #[tokio::test]
    #[ignore = "offline real PowerShell PIM cancellation; substitutes MSAL and HTTP"]
    async fn scoped_pim_cancellation_without_evidence_revokes_pending_and_inflight_authentication()
    {
        let fixture = Fixture::new();
        let review = pim_review();
        let sessions = GraphSessions::default();
        let ticket = sessions.begin(&review.binding).unwrap();
        assert!(sessions
            .dispose_pim_with_receipt(&review.binding.requirements_digest)
            .await
            .unwrap()
            .is_none());
        let worker = fixture.pim_worker(&review).await;
        assert!(sessions.install(ticket, worker.clone()).await.is_err());
        assert!(worker.authenticate().await.is_err());
        assert!(fixture.calls().is_empty());
        worker.close().await;

        let fixture = Fixture::new();
        let worker = fixture.pim_worker(&review).await;
        std::fs::write(
            fixture.root.path().join("control.json"),
            json!({
                "account":"operator@example.test", "mode":"pim", "delayMs":0, "authDelayMs":10000
            })
            .to_string(),
        )
        .unwrap();
        let sessions = GraphSessions::default();
        sessions.replace(worker.clone()).await.unwrap();
        let authenticating = worker.clone();
        let task = tokio::spawn(async move { authenticating.authenticate().await });
        tokio::time::timeout(Duration::from_secs(8), async {
            while !fixture
                .calls()
                .iter()
                .any(|v| v["kind"] == "authenticate-start")
            {
                tokio::time::sleep(Duration::from_millis(30)).await;
            }
        })
        .await
        .unwrap();
        assert!(sessions
            .dispose_pim_with_receipt(&review.binding.requirements_digest)
            .await
            .unwrap()
            .is_none());
        assert!(task.await.unwrap().is_err());
        assert!(!fixture
            .calls()
            .iter()
            .any(|v| v["kind"] == "connect" || v["kind"] == "protected-read-start"));
        assert!(sessions
            .pim_evidence(&review.binding.requirements_digest)
            .unwrap()
            .is_none());
        assert!(worker.probe().await.is_err());
    }

    #[tokio::test]
    #[ignore = "offline real PowerShell PIM planner; substitutes MSAL and HTTP"]
    async fn pinned_pim_planner_returns_real_artifact_from_owned_reads() {
        let fixture = Fixture::new();
        let review = pim_review();
        let worker = fixture.pim_worker(&review).await;
        worker.authenticate().await.unwrap();
        worker.probe().await.unwrap();
        let sessions = GraphSessions::default();
        sessions.replace(worker.clone()).await.unwrap();
        let artifact = sessions.pim_plan(&review).await.unwrap();
        assert_eq!(
            artifact["requirementsDigest"],
            review.binding.requirements_digest
        );
        assert_eq!(artifact["sourceCommit"], crate::pim_plan::SOURCE);
        assert_eq!(artifact["executionEvidence"]["processId"], worker.pid);
        assert_eq!(
            artifact["executionEvidence"]["context"]["verificationReads"],
            1
        );
        assert_eq!(
            artifact["executionEvidence"]["context"]["pimReadsAttempted"],
            7
        );
        assert_eq!(
            artifact["executionEvidence"]["context"]["pimReadsCompleted"],
            7
        );
        assert_eq!(
            artifact["plan"]["tiers"]["privileged"]["context"]["action"],
            "create"
        );
        assert_eq!(
            artifact["plan"]["tiers"]["privileged"]["roles"][0]["displayName"],
            "Offline privileged role"
        );
        assert!(!artifact.to_string().contains("offline-only-token"));
        let calls = fixture.calls();
        assert_eq!(
            calls
                .iter()
                .filter(|v| v["kind"] == "protected-read-start")
                .count(),
            8
        );
        assert!(calls.iter().all(|v| v["pid"] == worker.pid));
        assert_eq!(calls[0]["scopes"], json!(crate::pim_plan::SCOPES));
        sessions.dispose().await;
    }

    #[tokio::test]
    #[ignore = "offline real PowerShell PIM planner; substitutes MSAL and HTTP"]
    async fn phishing_resistant_plan_reuses_strength_reader_without_extra_permissions() {
        let fixture = Fixture::new();
        let mut review = pim_review();
        review.configuration.privileged_authentication_profile =
            crate::pim_plan::AuthenticationProfile::PhishingResistant;
        review.configuration.privileged_device_requirement =
            crate::pim_plan::DeviceRequirement::CompliantOrHybrid;
        let worker = fixture.pim_worker(&review).await;
        worker.authenticate().await.unwrap();
        worker.probe().await.unwrap();
        let plan = worker.pim_plan(&review.configuration).await.unwrap();
        assert_eq!(
            plan["tiers"]["privileged"]["authenticationStrength"]["policyType"],
            "builtIn"
        );
        assert_eq!(
            plan["tiers"]["privileged"]["conditionalAccessPolicies"]
                .as_array()
                .unwrap()
                .len(),
            2
        );
        assert!(fixture.calls().iter().any(|v| v["uri"]
            == "https://graph.microsoft.com/v1.0/policies/authenticationStrengthPolicies"));
        assert_eq!(fixture.calls()[0]["scopes"], json!(crate::pim_plan::SCOPES));
        worker.close().await;
    }

    #[tokio::test]
    #[ignore = "offline real PowerShell PIM preview; substitutes MSAL and HTTP"]
    async fn existing_object_preview_is_explicit_bound_and_cannot_apply_or_write_state() {
        const CONTEXTS: &str = "https://graph.microsoft.com/v1.0/identity/conditionalAccess/authenticationContextClassReferences";
        const POLICIES: &str =
            "https://graph.microsoft.com/beta/identity/conditionalAccess/policies?$top=999";
        const ROLE_POLICIES: &str = "https://graph.microsoft.com/v1.0/policies/roleManagementPolicies?$filter=scopeId%20eq%20%27%2F%27%20and%20scopeType%20eq%20%27DirectoryRole%27&$expand=rules&$top=999";
        for preview in [false, true] {
            let fixture = Fixture::new();
            let review = pim_review_with_preview(preview);
            let worker = fixture.pim_worker(&review).await;
            let mut responses: Value =
                serde_json::from_str(include_str!("../graph-adapter-tests/pim-responses.json"))
                    .unwrap();
            responses[CONTEXTS]["value"] = json!([{"id":"c1","displayName":"PIM Privileged Roles","description":"Existing unmanaged description","isAvailable":false}]);
            responses[POLICIES]["value"] = json!([{"id":"existing-ca-policy","displayName":"PIM - Privileged Roles - Require reauthentication","state":"disabled","conditions":{"applications":{"includeAuthenticationContextClassReferences":["c1"]}},"grantControls":{"builtInControls":["mfa"]}}]);
            responses[ROLE_POLICIES]["value"][0]["rules"][0]["isEnabled"] = json!(true);
            responses[ROLE_POLICIES]["value"][0]["rules"][0]["claimValue"] = json!("c2");
            std::fs::write(
                fixture.root.path().join("responses.json"),
                responses.to_string(),
            )
            .unwrap();
            worker.authenticate().await.unwrap();
            worker.probe().await.unwrap();
            let worker_directory = worker
                .transport
                .lock()
                .await
                .as_ref()
                .unwrap()
                ._files
                .path()
                .to_path_buf();
            let files_before: std::collections::BTreeSet<_> = std::fs::read_dir(&worker_directory)
                .unwrap()
                .map(|entry| entry.unwrap().file_name())
                .collect();
            let sessions = GraphSessions::default();
            sessions.replace(worker.clone()).await.unwrap();
            if !preview {
                let error = sessions.pim_plan(&review).await.unwrap_err().to_string();
                assert!(error
                    .contains("An authentication context with the planned name already exists"));
                assert!(*worker.cancel.borrow());
                continue;
            }
            let artifact = sessions.pim_plan(&review).await.unwrap();
            assert_eq!(artifact["configuration"]["previewExistingObjects"], true);
            assert_eq!(artifact["previewExistingObjects"], true);
            assert_eq!(artifact["ownershipReviewRequired"], true);
            assert_eq!(artifact["ownershipEstablished"], false);
            assert_eq!(artifact["executionAuthorized"], false);
            assert_eq!(artifact["plan"]["mode"], "plan");
            assert_eq!(artifact["plan"]["configuration"]["Mode"], "plan");
            assert_eq!(artifact["plan"]["configuration"]["AdoptExisting"], true);
            assert_eq!(artifact["plan"]["configuration"]["AdoptRoleContexts"], true);
            let tier = &artifact["plan"]["tiers"]["privileged"];
            assert_eq!(tier["context"]["action"], "adoptAndUpdate");
            assert_eq!(
                tier["context"]["existing"]["description"],
                "Existing unmanaged description"
            );
            assert_eq!(
                tier["conditionalAccessPolicies"][0]["action"],
                "adoptAndUpdate"
            );
            assert_eq!(
                tier["conditionalAccessPolicies"][0]["existing"]["state"],
                "disabled"
            );
            assert!(tier["conditionalAccessPolicies"][0]["body"].is_object());
            assert_eq!(tier["roleRules"][0]["existing"]["claimValue"], "c2");
            assert_eq!(tier["roleRules"][0]["desiredContextId"], "c1");
            assert_eq!(
                fixture
                    .calls()
                    .iter()
                    .filter(|call| call["kind"] == "protected-read-start")
                    .count(),
                8
            );
            assert_eq!(fixture.calls()[0]["scopes"], json!(crate::pim_plan::SCOPES));
            let files_after: std::collections::BTreeSet<_> = std::fs::read_dir(&worker_directory)
                .unwrap()
                .map(|entry| entry.unwrap().file_name())
                .collect();
            assert_eq!(
                files_before, files_after,
                "read-only preview wrote worker state"
            );

            // Invoke the actual pinned apply entry with this real preview plan.
            // Both Graph and persistence tripwires prevent any fallback/network
            // or durable write even if its enforced-mode guard regresses.
            let apply_check = fixture.root.path().join("reject-preview-apply.ps1");
            let plan_path = fixture.root.path().join("preview-plan.json");
            std::fs::write(&plan_path, artifact["plan"].to_string()).unwrap();
            std::fs::write(&apply_check, r#"param([string]$ModulePath,[string]$PlanPath)
$ErrorActionPreference='Stop'
Import-Module $ModulePath -Force
$plan=Get-Content -LiteralPath $PlanPath -Raw | ConvertFrom-Json -Depth 100
& (Get-Module AzdPim.Graph) {
    param($Plan)
    $script:tripwire=$false
    function script:Invoke-AzdPimGraphRequest { $script:tripwire=$true; throw 'OFFLINE_GRAPH_WRITE_TRIPWIRE' }
    try { Invoke-AzdPimApply -Plan $Plan -StateChanged { $script:tripwire=$true; throw 'OFFLINE_STATE_WRITE_TRIPWIRE' }; throw 'Preview was accepted for enforcement.' }
    catch { if ($_.Exception.Message -cne 'Invoke-AzdPimApply requires an enforced plan.') { throw } }
    if ($script:tripwire) { throw 'Preview attempted a protected action.' }
} $plan
[Console]::Out.WriteLine('PREVIEW_APPLY_REJECTED_BEFORE_GRAPH_OR_STATE')
"#).unwrap();
            let mut command = Command::new(resolve_system_executable("pwsh").unwrap());
            command
                .args(["-NoLogo", "-NoProfile", "-NonInteractive", "-File"])
                .arg(&apply_check)
                .arg(worker_directory.join("AzdPim.Graph.psm1"))
                .arg(&plan_path);
            #[cfg(windows)]
            command.creation_flags(0x0800_0000);
            let output = command.output().await.unwrap();
            assert!(
                output.status.success(),
                "{}",
                String::from_utf8_lossy(&output.stderr)
            );
            assert!(String::from_utf8_lossy(&output.stdout)
                .contains("PREVIEW_APPLY_REJECTED_BEFORE_GRAPH_OR_STATE"));
            sessions.dispose().await;
        }
    }

    #[tokio::test]
    #[ignore = "offline real PowerShell PIM planner; substitutes MSAL and HTTP"]
    async fn late_pim_disposal_cannot_revoke_a_new_review_session() {
        let fixture = Fixture::new();
        let old = pim_review();
        let mut new = pim_review();
        new.binding.requirements_digest = "a".repeat(64);
        let worker = fixture.pim_worker(&new).await;
        worker.authenticate().await.unwrap();
        worker.probe().await.unwrap();
        let sessions = GraphSessions::default();
        sessions.replace(worker).await.unwrap();
        sessions
            .dispose_matching(&old.binding.requirements_digest)
            .await;
        assert!(sessions.pim_plan(&new).await.is_ok());
        sessions
            .dispose_matching(&new.binding.requirements_digest)
            .await;
        assert!(sessions.pim_plan(&new).await.is_err());
    }

    #[tokio::test]
    #[ignore = "offline real PowerShell PIM planner; substitutes MSAL and HTTP"]
    async fn cancelling_pim_plan_discards_late_artifact_and_revokes_worker() {
        let fixture = Fixture::new();
        let review = pim_review();
        let worker = fixture.pim_worker(&review).await;
        worker.authenticate().await.unwrap();
        worker.probe().await.unwrap();
        fixture.control("pim", 10000);
        let task_worker = worker.clone();
        let configuration = review.configuration.clone();
        let task = tokio::spawn(async move { task_worker.pim_plan(&configuration).await });
        tokio::time::timeout(Duration::from_secs(8), async {
            while fixture
                .calls()
                .iter()
                .filter(|v| v["kind"] == "protected-read-start")
                .count()
                < 2
                || worker
                    .process_evidence()
                    .unwrap()
                    .context
                    .pim_reads_attempted
                    < 1
            {
                tokio::time::sleep(Duration::from_millis(30)).await;
            }
        })
        .await
        .unwrap();
        assert_eq!(
            worker
                .process_evidence()
                .unwrap()
                .context
                .pim_reads_attempted,
            1
        );
        worker.cancel();
        assert!(task.await.unwrap().is_err());
        assert!(worker.pim_plan(&review.configuration).await.is_err());
        assert_eq!(
            fixture
                .calls()
                .iter()
                .filter(|v| v["kind"] == "protected-read-end")
                .count(),
            1
        );
    }

    #[tokio::test]
    #[ignore = "offline real PowerShell PIM planner; substitutes MSAL and HTTP"]
    async fn changed_pim_configuration_revokes_existing_session_before_dispatch() {
        let fixture = Fixture::new();
        let review = pim_review();
        let worker = fixture.pim_worker(&review).await;
        worker.authenticate().await.unwrap();
        worker.probe().await.unwrap();
        let sessions = GraphSessions::default();
        sessions.replace(worker.clone()).await.unwrap();
        let changed = pim_review_with_preview(true);
        assert!(sessions.pim_plan(&changed).await.is_err());
        assert!(sessions.pim_plan(&review).await.is_err());
        assert_eq!(fixture.calls().len(), 3);
        worker.close().await;
    }

    #[tokio::test]
    #[ignore = "offline real PowerShell PIM planner; substitutes MSAL and HTTP"]
    async fn pim_pagination_cannot_change_host_collection_or_query_contract() {
        const INITIAL: &str = "https://graph.microsoft.com/beta/roleManagement/directory/roleDefinitions?$filter=isPrivileged%20eq%20true";
        for next in [
            "https://example.invalid/beta/roleManagement/directory/roleDefinitions?$skiptoken=x",
            "https://graph.microsoft.com/v1.0/users?$skiptoken=x",
            "https://graph.microsoft.com/beta/roleManagement/directory/roleDefinitions?$filter=isPrivileged%20eq%20false&$skiptoken=x",
            "https://graph.microsoft.com/beta/roleManagement/directory/roleDefinitions?$filter=isPrivileged%20eq%20true&$expand=roleAssignments&$skiptoken=x",
        ] {
            let fixture = Fixture::new();
            let review = pim_review();
            let worker = fixture.pim_worker(&review).await;
            let mut responses: Value = serde_json::from_str(include_str!("../graph-adapter-tests/pim-responses.json")).unwrap();
            responses[INITIAL]["@odata.nextLink"] = json!(next);
            std::fs::write(fixture.root.path().join("responses.json"), responses.to_string()).unwrap();
            worker.authenticate().await.unwrap();
        worker.probe().await.unwrap();
            assert!(worker.pim_plan(&review.configuration).await.is_err(), "{next}");
            assert_eq!(fixture.calls().iter().filter(|v| v["kind"] == "protected-read-start").count(), 2);
            assert!(worker.probe().await.is_err());
        }
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
        assert!(!worker.process_evidence().unwrap().active);
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
        assert!(!worker.process_evidence().unwrap().active);
        assert!(worker.probe().await.is_err());
        assert_eq!(fixture.calls().len(), 1);
    }
}
