//! Native-only evidence for the registered azd-pim workflow producer.
//! Generic azd output and saved artifacts never enter this channel.
use super::{read_bounded_output, CommandSpec, Executable, OperationPersistence};
use crate::{
    application_ownership::valid_endpoint_host,
    error::{AppError, AppResult},
    evidence::{read_target_snapshot, OperationEvidenceContext},
    models::OperationEvent,
    persistence::Database,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{path::Path, sync::Arc, time::SystemTime};
use tokio::{io::AsyncRead, sync::oneshot};
use uuid::Uuid;

const REPOSITORY: &str = "https://github.com/nathanmcnulty/azd-pim";
const LIMIT: usize = 1024;

pub(super) struct Capture {
    // These values belong to a native registration, not template metadata or IPC.
    producer_revision: &'static str,
    producer_sha256: String,
    script: &'static str,
    // From the same operation's independent deployment output, never .env.
    workflow_resource_id: String,
    completion: oneshot::Sender<Receipt>,
}

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct Observation {
    workflow_resource_id: String,
    hostname: String,
}

#[derive(Serialize)]
pub(super) struct Receipt {
    operation_id: String,
    project_path: String,
    context: OperationEvidenceContext,
    producer_sha256: String,
    environment_generation: SystemTime,
    observation: Observation,
    #[serde(skip)]
    valid: bool,
}

fn invalid() -> AppError {
    AppError::Invalid("The reviewed workflow capture does not match this deployment.".into())
}

fn scoped_workflow_id(resource_id: &str, context: &OperationEvidenceContext) -> bool {
    let (Some(subscription), Some(group)) = (&context.subscription_id, &context.resource_group)
    else {
        return false;
    };
    let prefix = format!(
        "/subscriptions/{}/resourcegroups/{}/providers/microsoft.logic/workflows/",
        subscription.to_ascii_lowercase(),
        group.to_ascii_lowercase()
    );
    resource_id.strip_prefix(&prefix).is_some_and(|name| {
        name.len() <= 80
            && name
                .as_bytes()
                .first()
                .is_some_and(u8::is_ascii_alphanumeric)
            && name.bytes().all(|byte| {
                byte.is_ascii_lowercase() || byte.is_ascii_digit() || b"-_".contains(&byte)
            })
    })
}

fn generation(
    database: &Database,
    path: &str,
    expected: &OperationEvidenceContext,
) -> Option<SystemTime> {
    let project = database
        .projects()
        .ok()?
        .into_iter()
        .find(|p| p.path == path)?;
    if project.id != expected.project_id || project.environment.as_deref()? != expected.environment
    {
        return None;
    }
    let snapshot = database.project_template_exact(path).ok()??;
    if snapshot.entry.repository != REPOSITORY
        || snapshot.entry.id != expected.template_id
        || Some(snapshot.entry.commit.as_str()) != expected.revision.as_deref()
        || Some(snapshot.digest.as_str()) != expected.contract_digest.as_deref()
        || snapshot
            .entry
            .manifest
            .as_ref()?
            .application_ownership
            .is_none()
    {
        return None;
    }
    let (target, modified) = read_target_snapshot(Path::new(path), &expected.environment)?;
    let group = target.resource_group.as_deref()?;
    let valid_uuid = |id: Option<&str>| {
        id.and_then(|value| Uuid::parse_str(value).ok())
            .is_some_and(|value| !value.is_nil())
    };
    if target.azure_cloud.as_deref() != Some("AzureCloud")
        || target.azure_cloud != expected.azure_cloud
        || target.tenant_id != expected.tenant_id
        || target.subscription_id != expected.subscription_id
        || target.resource_group != expected.resource_group
        || !valid_uuid(target.tenant_id.as_deref())
        || !valid_uuid(target.subscription_id.as_deref())
        || crate::deployment::validate_resource_group(Some(group)).is_err()
        || group.is_empty()
    {
        return None;
    }
    modified
}

impl Capture {
    pub(super) fn prepare(
        &self,
        kind: &str,
        persistence: Option<&OperationPersistence>,
    ) -> AppResult<(CommandSpec, SystemTime)> {
        let persistence = persistence.ok_or_else(invalid)?;
        let context = persistence.context.as_ref().ok_or_else(invalid)?;
        if !matches!(kind, "up" | "provision")
            || self.producer_revision.len() != 40
            || !self
                .producer_revision
                .bytes()
                .all(|byte| byte.is_ascii_hexdigit())
            || context.revision.as_deref() != Some(self.producer_revision)
            || hex::encode(Sha256::digest(self.script.as_bytes())) != self.producer_sha256
            || !scoped_workflow_id(&self.workflow_resource_id, context)
        {
            return Err(invalid());
        }
        let generation = generation(&persistence.database, &persistence.project_path, context)
            .ok_or_else(invalid)?;
        // Execute the verified bytes passed by the host, away from project files.
        let assets = Arc::new(tempfile::tempdir()?);
        let mut spec = CommandSpec::new(
            Executable::PowerShell,
            [
                "-NoLogo",
                "-NoProfile",
                "-NonInteractive",
                "-Command",
                self.script,
            ],
        )
        .cwd(assets.path());
        spec.assets = Some(assets);
        Ok((spec, generation))
    }

    pub(super) fn complete(
        self,
        event: &OperationEvent,
        persistence: &OperationPersistence,
        environment_generation: SystemTime,
        observation: Option<Observation>,
    ) {
        let Some(observation) = observation else {
            return;
        };
        if event.state != "succeeded"
            || observation.workflow_resource_id != self.workflow_resource_id
        {
            return;
        }
        let Some(context) = persistence.context.clone() else {
            return;
        };
        let mut receipt = Receipt {
            operation_id: event.operation_id.clone(),
            project_path: persistence.project_path.clone(),
            context,
            producer_sha256: self.producer_sha256,
            environment_generation,
            observation,
            valid: true,
        };
        if receipt.current_hostname(&persistence.database).is_some() {
            let _ = self.completion.send(receipt);
        }
    }
}

impl Receipt {
    // Recheck before a native consumer uses the hostname. No I/O to the cloud.
    pub(super) fn current_hostname(&mut self, database: &Database) -> Option<&str> {
        self.valid = self.valid
            && generation(database, &self.project_path, &self.context)
                == Some(self.environment_generation)
            && matches!(
                database.is_current_workflow_operation(&self.project_path, &self.operation_id),
                Ok(true)
            );
        self.valid.then_some(&self.observation.hostname)
    }
}

pub(super) async fn read_observation(reader: impl AsyncRead + Unpin) -> Option<Observation> {
    // Drain the pipe even after overflow, then reject the entire result.
    let bytes = read_bounded_output(reader, LIMIT + 1).await.ok()?;
    if bytes.len() > LIMIT {
        return None;
    }
    let observation: Observation = serde_json::from_slice(&bytes).ok()?;
    valid_endpoint_host(&observation.hostname).then_some(observation)
}

#[cfg(all(test, windows))]
mod tests;
