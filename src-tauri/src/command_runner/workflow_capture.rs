//! Native-only evidence for the registered azd-pim workflow producer.
//! Generic azd output and saved artifacts never enter this channel.
use super::{read_bounded_output, CommandSpec, Executable, OperationPersistence};
use crate::{
    application_ownership::valid_endpoint_host,
    error::{AppError, AppResult},
    evidence::{read_complete_environment_snapshot, OperationEvidenceContext},
    models::OperationEvent,
    persistence::Database,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{collections::BTreeMap, path::Path, sync::Arc, time::SystemTime};
use tokio::{io::AsyncRead, sync::oneshot};
use uuid::Uuid;

use super::workflow_producer::REPOSITORY;

const LIMIT: usize = 1024;
const WORKFLOW_ID: &str = "AZD_PIM_REVOCATION_LOGIC_APP_RESOURCE_ID";
const APPLICATION_OBJECT_ID: &str = "AZD_PIM_REVOCATION_APPLICATION_OBJECT_ID";
const APPLICATION_CLIENT_ID: &str = "AZD_PIM_REVOCATION_APPLICATION_CLIENT_ID";
// Outputs of the reviewed PIM ARM deployment and post-provision producer.
// Keep AZD_PIM_NOTIFICATION_MODE bound because it is also an input.
const OUTPUTS: &[&str] = &[
    "AZD_PIM_POLLING_FUNCTION_NAME",
    "AZD_PIM_POLLING_FUNCTION_RESOURCE_ID",
    "AZD_PIM_POLLING_FUNCTION_PRINCIPAL_ID",
    "AZD_PIM_SENTINEL_NOTIFICATION_LOGIC_APP_RESOURCE_ID",
    "AZD_PIM_SENTINEL_NOTIFICATION_ALERT_RESOURCE_ID",
    "AZD_PIM_SENTINEL_NOTIFICATION_ACTION_GROUP_RESOURCE_ID",
    "AZD_PIM_REVOCATION_LOGIC_APP_NAME",
    WORKFLOW_ID,
    "AZD_PIM_REVOCATION_LOGIC_APP_PRINCIPAL_ID",
    "AZD_PIM_REVOCATION_ALERT_RESOURCE_ID",
    "AZD_PIM_REVOCATION_CUSTOM_EXTENSION_ID",
    "AZD_PIM_REVOCATION_PRE_APPROVAL_CUSTOM_EXTENSION_ID",
    "AZD_PIM_REVOCATION_POST_APPROVAL_CUSTOM_EXTENSION_ID",
];

#[derive(PartialEq, Eq, Serialize)]
pub(super) struct CaptureInputs {
    selection_revision: i64,
    digest: [u8; 32],
    #[serde(skip)]
    application_object_id: Option<Uuid>,
    #[serde(skip)]
    application_client_id: Option<Uuid>,
}

impl CaptureInputs {
    fn matches_final(&self, final_inputs: &Self) -> bool {
        // Both IDs are written by the successful hook, but a nonempty
        // admitted value was also a legacy selector and must survive.
        let preserves = |admitted: Option<Uuid>, actual: Option<Uuid>| {
            actual.is_some() && (admitted.is_none() || admitted == actual)
        };
        self.selection_revision == final_inputs.selection_revision
            && self.digest == final_inputs.digest
            && preserves(
                self.application_object_id,
                final_inputs.application_object_id,
            )
            && preserves(
                self.application_client_id,
                final_inputs.application_client_id,
            )
    }
}

#[derive(PartialEq, Eq, Serialize)]
struct SavedEnvironment {
    inputs: CaptureInputs,
    modified: SystemTime,
    digest: [u8; 32],
    workflow_resource_id: Option<String>,
}

pub(super) struct Capture {
    // These values belong to a native registration, not template metadata or IPC.
    producer_revision: &'static str,
    producer_sha256: String,
    script: String,
    // From the same operation's independent deployment output, never .env.
    workflow_resource_id: String,
    completion: oneshot::Sender<Receipt>,
    #[cfg(all(test, windows))]
    offline_powershell: Option<std::path::PathBuf>,
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
    saved_environment: SavedEnvironment,
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

fn saved_environment(
    database: &Database,
    path: &str,
    expected: &OperationEvidenceContext,
) -> Option<SavedEnvironment> {
    let selection_revision = database.project_selection_revision(path).ok()??;
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
    let saved = read_complete_environment_snapshot(Path::new(path), &expected.environment)?;
    let saved_value = |name: &str| saved.values.get(name).map(String::as_str);
    let group = saved_value("AZURE_RESOURCE_GROUP")?;
    let valid_uuid = |id: Option<&str>| {
        id.and_then(|value| Uuid::parse_str(value).ok())
            .is_some_and(|value| !value.is_nil())
    };
    if saved_value("AZD_GUI_AZURE_CLOUD") != Some("AzureCloud")
        || saved_value("AZD_GUI_AZURE_CLOUD") != expected.azure_cloud.as_deref()
        || saved_value("AZURE_TENANT_ID") != expected.tenant_id.as_deref()
        || saved_value("AZURE_SUBSCRIPTION_ID") != expected.subscription_id.as_deref()
        || Some(group) != expected.resource_group.as_deref()
        || !valid_uuid(expected.tenant_id.as_deref())
        || !valid_uuid(expected.subscription_id.as_deref())
        || crate::deployment::validate_resource_group(Some(group)).is_err()
        || group.is_empty()
        || database.project_selection_revision(path).ok()?? != selection_revision
    {
        return None;
    }

    let entries: BTreeMap<_, _> = saved.values.iter().collect();
    let inputs: BTreeMap<_, _> = entries
        .iter()
        .filter(|(key, _)| {
            !OUTPUTS.contains(&key.as_str())
                && !matches!(key.as_str(), APPLICATION_OBJECT_ID | APPLICATION_CLIENT_ID)
        })
        .collect();
    let application_id = |name| {
        let value = saved_value(name)
            .map(str::trim)
            .filter(|value| !value.is_empty());
        match value {
            Some(value) => {
                let id = Uuid::parse_str(value).ok().filter(|id| !id.is_nil())?;
                Some(Some(id))
            }
            None => Some(None),
        }
    };
    Some(SavedEnvironment {
        inputs: CaptureInputs {
            selection_revision,
            digest: Sha256::digest(serde_json::to_vec(&inputs).ok()?).into(),
            application_object_id: application_id(APPLICATION_OBJECT_ID)?,
            application_client_id: application_id(APPLICATION_CLIENT_ID)?,
        },
        modified: saved.modified?,
        digest: Sha256::digest(serde_json::to_vec(&entries).ok()?).into(),
        workflow_resource_id: saved_value(WORKFLOW_ID).map(str::to_ascii_lowercase),
    })
}

impl Capture {
    pub(super) fn prepare(
        &self,
        kind: &str,
        persistence: Option<&OperationPersistence>,
    ) -> AppResult<(CommandSpec, CaptureInputs)> {
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
        let saved = saved_environment(&persistence.database, &persistence.project_path, context)
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
                self.script.as_str(),
            ],
        )
        .cwd(assets.path());
        spec.assets = Some(assets);
        #[cfg(all(test, windows))]
        {
            spec.offline_workflow_powershell = self.offline_powershell.clone();
        }
        Ok((spec, saved.inputs))
    }

    pub(super) fn complete(
        self,
        event: &OperationEvent,
        persistence: &OperationPersistence,
        prepared_inputs: CaptureInputs,
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
        let Some(saved_environment) =
            saved_environment(&persistence.database, &persistence.project_path, &context)
        else {
            return;
        };
        // Saved output is only a consistency check against the independently
        // supplied ID and the reviewed child's observation.
        if !prepared_inputs.matches_final(&saved_environment.inputs)
            || saved_environment.workflow_resource_id.as_deref()
                != Some(self.workflow_resource_id.as_str())
        {
            return;
        }
        let mut receipt = Receipt {
            operation_id: event.operation_id.clone(),
            project_path: persistence.project_path.clone(),
            context,
            producer_sha256: self.producer_sha256,
            saved_environment,
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
            && saved_environment(database, &self.project_path, &self.context).as_ref()
                == Some(&self.saved_environment)
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
