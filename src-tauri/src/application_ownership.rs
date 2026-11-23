//! Local review boundary for the registered application verifier.
//! Production transport stays unavailable until a separately qualified provider is installed.
use crate::{
    error::{AppError, AppResult},
    models::{ConfigurationField, TemplateManifest},
    persistence::Database,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    path::Path,
    sync::Mutex,
    time::{Duration, Instant, UNIX_EPOCH},
};
use uuid::Uuid;

const PROVIDER: &str = "entra-application-registration";
const REVIEW_LIFETIME: Duration = Duration::from_secs(300);

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Declaration {
    kind: String,
    provider: String,
    object_id_field: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    application_id_field: Option<String>,
}

fn invalid(message: &str) -> AppError {
    AppError::Invalid(message.into())
}

fn lookup<'a>(manifest: &'a TemplateManifest, id: &str) -> AppResult<&'a ConfigurationField> {
    manifest.configuration.groups.iter()
        .filter(|group| group.condition.is_none())
        .flat_map(|group| &group.fields)
        .find(|field| field.id == id && field.condition.is_none() && !field.sensitive
            && matches!(field.field_type.as_str(), "text" | "entraObject"))
        .ok_or_else(|| invalid("Ownership verifier fields must be declared, unconditional, non-sensitive ID inputs."))
}

impl Declaration {
    pub(crate) fn validate(&self, manifest: &TemplateManifest) -> AppResult<()> {
        if self.kind != "verifier"
            || self.provider != PROVIDER
            || self.application_id_field.as_ref() == Some(&self.object_id_field)
        {
            return Err(invalid(
                "Unsupported application ownership verifier declaration.",
            ));
        }
        for field in std::iter::once(&self.object_id_field).chain(self.application_id_field.iter())
        {
            if field.is_empty()
                || field.len() > 64
                || !field.as_bytes()[0].is_ascii_alphabetic()
                || !field
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
            {
                return Err(invalid("Ownership verifier field ID has an invalid shape."));
            }
            lookup(manifest, field)?;
        }
        Ok(())
    }
}

fn uuid(value: &str) -> Option<Uuid> {
    Uuid::parse_str(value.trim())
        .ok()
        .filter(|value| !value.is_nil())
}

fn digest(value: Value) -> String {
    hex::encode(Sha256::digest(value.to_string().as_bytes()))
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
struct Target {
    tenant_id: Uuid,
    subscription_id: Uuid,
    environment: String,
    target_resource_digest: String,
    source_contract_digest: String,
    inputs_digest: String,
    object_id: Uuid,
    #[serde(skip)]
    resource_group: String,
    #[serde(skip)]
    environment_modified: u128,
    #[serde(skip_serializing_if = "Option::is_none")]
    expected_application_id: Option<Uuid>,
}

fn registered_target(
    database: &Database,
    project_path: &str,
    environment: &str,
) -> AppResult<Target> {
    let project = database
        .projects()?
        .into_iter()
        .find(|project| project.path == project_path)
        .ok_or_else(|| invalid("Select an exact registered project."))?;
    if project.environment.as_deref() != Some(environment) {
        return Err(invalid(
            "The project environment changed. Reopen the project.",
        ));
    }
    let snapshot = database
        .project_template_exact(&project.path)?
        .ok_or_else(|| invalid("Review the saved source contract first."))?;
    let manifest = snapshot
        .entry
        .manifest
        .as_ref()
        .ok_or_else(|| invalid("The saved manifest is unavailable."))?;
    let declaration = manifest.application_ownership.as_ref().ok_or_else(|| {
        invalid("The manifest does not declare an application ownership verifier.")
    })?;
    declaration.validate(manifest)?;
    let path = Path::new(&project.path);
    let _local = crate::component_provenance::Root::open(path)
        .map_err(|_| invalid("The registered project is not a safe local folder."))?;
    let object = &lookup(manifest, &declaration.object_id_field)?.environment_variable;
    let client = declaration
        .application_id_field
        .as_deref()
        .map(|id| lookup(manifest, id))
        .transpose()?
        .map(|field| field.environment_variable.as_str());
    let mut keys = vec![
        "AZURE_TENANT_ID",
        "AZURE_SUBSCRIPTION_ID",
        "AZURE_RESOURCE_GROUP",
        "AZD_GUI_AZURE_CLOUD",
        object,
    ];
    keys.extend(client);
    let saved = crate::evidence::read_environment_snapshot(path, environment, &keys)
        .ok_or_else(|| invalid("The saved environment is unavailable."))?;
    let environment_modified = saved
        .modified
        .and_then(|modified| modified.duration_since(UNIX_EPOCH).ok())
        .ok_or_else(|| invalid("The saved environment's file revision is unavailable."))?
        .as_nanos();
    let saved = saved.values;
    let required_uuid = |key| {
        saved.get(key).and_then(|value| uuid(value)).ok_or_else(|| {
            invalid(
                "Save the exact tenant, subscription and application object ID before verifying.",
            )
        })
    };
    let tenant_id = required_uuid("AZURE_TENANT_ID")?;
    let subscription_id = required_uuid("AZURE_SUBSCRIPTION_ID")?;
    let object_id = required_uuid(object.as_str())?;
    let expected_application_id = client
        .and_then(|field| saved.get(field))
        .filter(|value| !value.trim().is_empty())
        .map(|value| {
            uuid(value).ok_or_else(|| invalid("Save a valid application ID before verifying."))
        })
        .transpose()?;
    let group = saved
        .get("AZURE_RESOURCE_GROUP")
        .filter(|value| !value.is_empty())
        .ok_or_else(|| invalid("Save the exact target resource group before verifying."))?;
    crate::deployment::validate_resource_group(Some(group))?;
    if saved.get("AZD_GUI_AZURE_CLOUD").map(String::as_str) != Some("AzureCloud") {
        return Err(invalid("Select public Azure for this verifier."));
    }
    Ok(Target {
        tenant_id,
        subscription_id,
        environment: environment.into(),
        target_resource_digest: digest(json!([subscription_id, group.to_ascii_lowercase()])),
        resource_group: group.clone(),
        environment_modified,
        source_contract_digest: snapshot.digest,
        inputs_digest: digest(json!([project.id, object_id, expected_application_id])),
        object_id,
        expected_application_id,
    })
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct Identity {
    tenant: Uuid,
    operator: String,
}

const MAX_APPLICATION_RESPONSE_BYTES: usize = 64 * 1024;
const APPLICATION_FIELDS: &str = "id,appId,signInAudience,identifierUris,api";

#[derive(Clone)]
struct ApplicationRead {
    // The owned transport attests its identity outside the Graph body.
    identity: Identity,
    status: reqwest::StatusCode,
    body: Vec<u8>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct GraphApplication {
    id: Uuid,
    app_id: Uuid,
    sign_in_audience: String,
    identifier_uris: Vec<String>,
    api: GraphApplicationApi,
    #[serde(rename = "@odata.type")]
    resource_type: Option<String>,
    #[serde(rename = "@odata.context")]
    context: Option<String>,
    #[serde(rename = "@odata.nextLink")]
    continuation: Option<serde::de::IgnoredAny>,
    value: Option<serde::de::IgnoredAny>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct GraphApplicationApi {
    #[serde(default, deserialize_with = "present_token_version")]
    requested_access_token_version: Option<u8>,
}

fn present_token_version<'de, D: serde::Deserializer<'de>>(
    value: D,
) -> Result<Option<u8>, D::Error> {
    // Graph defines explicit null as version 1. A missing projection remains unknown.
    Option::<u8>::deserialize(value).map(|version| Some(version.unwrap_or(1)))
}

impl GraphApplication {
    fn is_application(&self) -> bool {
        let exact_type = "#microsoft.graph.application";
        if self
            .resource_type
            .as_deref()
            .is_some_and(|kind| kind != exact_type)
        {
            return false;
        }
        match self.context.as_deref() {
            Some(context) => context
                .strip_prefix("https://graph.microsoft.com/v1.0/$metadata#applications")
                .and_then(|context| context.strip_suffix("/$entity"))
                .is_some_and(|projection| {
                    projection.is_empty()
                        || projection
                            .strip_prefix('(')
                            .and_then(|projection| projection.strip_suffix(')'))
                            .is_some_and(|fields| {
                                fields.split(',').all(|field| {
                                    APPLICATION_FIELDS.split(',').any(|known| known == field)
                                })
                            })
                }),
            None => self.resource_type.as_deref() == Some(exact_type),
        }
    }
}

fn application_request(object_id: Uuid) -> AppResult<reqwest::Request> {
    let mut url = reqwest::Url::parse(&format!(
        "https://graph.microsoft.com/v1.0/applications/{object_id}"
    ))
    .map_err(|_| invalid("The registered application lookup is unavailable."))?;
    url.query_pairs_mut()
        .append_pair("$select", APPLICATION_FIELDS);
    Ok(reqwest::Request::new(reqwest::Method::GET, url))
}

impl ApplicationRead {
    fn application(&self) -> Result<GraphApplication, Unknown> {
        use reqwest::StatusCode;
        match self.status {
            StatusCode::OK => {}
            StatusCode::UNAUTHORIZED | StatusCode::FORBIDDEN => return Err(Unknown::AccessDenied),
            status if status.is_redirection() || status.is_success() => {
                return Err(Unknown::Incomplete);
            }
            _ => return Err(Unknown::Unavailable),
        }
        if self.body.len() > MAX_APPLICATION_RESPONSE_BYTES {
            return Err(Unknown::Incomplete);
        }
        let application: GraphApplication =
            serde_json::from_slice(&self.body).map_err(|_| Unknown::Incomplete)?;
        if application.id.is_nil()
            || application.app_id.is_nil()
            || application.continuation.is_some()
            || application.value.is_some()
            || !application.is_application()
        {
            return Err(Unknown::Incomplete);
        }
        Ok(application)
    }
}

fn valid_endpoint_host(host: &str) -> bool {
    // The pinned producer uses the canonical DNS host of the workflow. It
    // never uses a path, query, port or the identifier URI from Graph as input.
    host.len() <= 253
        && host.contains('.')
        && host.split('.').all(|label| {
            !label.is_empty()
                && label.len() <= 63
                && !label.starts_with('-')
                && !label.ends_with('-')
                && label
                    .bytes()
                    .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
        })
        && host.parse::<std::net::IpAddr>().is_err()
}

#[derive(Debug, Clone, Copy, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) enum Unknown {
    Unavailable,
    ProviderUnavailable,
    AccessDenied,
    Incomplete,
    Cancelled,
}

trait Transport {
    fn identity(&self) -> Result<Identity, Unknown>;

    // Native evidence for this exact saved target, independent of the application response.
    fn reviewed_endpoint_host(&self, target: &Target) -> Option<&str>;

    // One authorized read. A future transport must bound the body while
    // receiving it and reject redirects, retries and alternate endpoints.
    fn read(&self, tenant: Uuid, request: reqwest::Request) -> Result<ApplicationRead, Unknown>;
}

struct OfflineTransport;
impl Transport for OfflineTransport {
    fn identity(&self) -> Result<Identity, Unknown> {
        // Production has no no-refresh application credential or independently
        // verified workflow hostname for this target.
        Err(Unknown::ProviderUnavailable)
    }
    fn reviewed_endpoint_host(&self, _: &Target) -> Option<&str> {
        None
    }

    fn read(&self, _: Uuid, _: reqwest::Request) -> Result<ApplicationRead, Unknown> {
        Err(Unknown::Unavailable)
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Properties {
    object_id: Uuid,
    application_id: Uuid,
    single_tenant: bool,
    identifier_uri_matches: bool,
    access_token_version: u8,
}

#[derive(Serialize)]
#[serde(
    tag = "state",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub(crate) enum Verification {
    Unknown {
        reason: Unknown,
    },
    Foreign {
        reason: &'static str,
    },
    Mismatch {
        reason: &'static str,
    },
    Verified {
        kind: &'static str,
        binding: Box<Binding>,
        properties: Properties,
        checked_at: String,
        verification_digest: String,
    },
}

struct Pending {
    target: Target,
    identity: Identity,
    endpoint_host: String,
    properties: Properties,
    checked_at: String,
    digest: String,
    issued: Instant,
}

#[derive(Default)]
struct ReviewState {
    pending: Option<Pending>,
    revision: u64,
}

impl ReviewState {
    fn invalidate(&mut self) {
        self.pending = None;
        self.revision = self.revision.wrapping_add(1);
    }
}

#[derive(Default)]
pub(crate) struct Reviews(Mutex<ReviewState>);

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ReviewContext {
    #[serde(flatten)]
    target: Target,
    resource_group: String,
    context_digest: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ReviewRequest {
    project_path: String,
    environment: String,
    context_digest: String,
}

impl ReviewRequest {
    fn resolve(&self, database: &Database, revision: u64) -> AppResult<Target> {
        let target = registered_target(database, &self.project_path, &self.environment)?;
        if self.context_digest != target.context_digest(revision) {
            return Err(invalid(
                "The saved application target or source changed. Review it again.",
            ));
        }
        Ok(target)
    }
}

impl Target {
    fn context_digest(&self, revision: u64) -> String {
        digest(json!([
            "application-registration-review-context",
            self,
            self.environment_modified.to_string(),
            revision
        ]))
    }
}

fn binding_values(target: &Target, identity: &Identity) -> Value {
    json!([
        PROVIDER,
        "AzureCloud",
        target.tenant_id,
        target.subscription_id,
        target.environment,
        identity.operator,
        target.target_resource_digest,
        target.source_contract_digest,
        target.inputs_digest,
        target.object_id,
        target.expected_application_id
    ])
}

impl Reviews {
    pub(crate) fn invalidate(&self) {
        if let Ok(mut state) = self.0.lock() {
            state.invalidate();
        }
    }

    fn context(
        &self,
        database: &Database,
        path: &str,
        environment: &str,
    ) -> AppResult<ReviewContext> {
        let mut state = self
            .0
            .lock()
            .map_err(|_| invalid("Application review is unavailable."))?;
        let target = registered_target(database, path, environment);
        if state
            .pending
            .as_ref()
            .is_some_and(|pending| target.as_ref().ok() != Some(&pending.target))
        {
            state.invalidate();
        }
        let target = target?;
        Ok(ReviewContext {
            context_digest: target.context_digest(state.revision),
            resource_group: target.resource_group.clone(),
            target,
        })
    }

    fn verify(
        &self,
        resolve: impl FnOnce(u64) -> AppResult<Target>,
        transport: &impl Transport,
    ) -> AppResult<Verification> {
        let mut state = self
            .0
            .lock()
            .map_err(|_| invalid("Application review is unavailable."))?;
        state.pending = None;
        let target = resolve(state.revision)?;
        let identity = match transport.identity() {
            Ok(identity) => identity,
            Err(reason) => return Ok(Verification::Unknown { reason }),
        };
        if identity.tenant != target.tenant_id {
            return Ok(Verification::Foreign {
                reason: "differentTenant",
            });
        }
        if identity.operator.len() != 64
            || !identity
                .operator
                .bytes()
                .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
        {
            return Ok(Verification::Unknown {
                reason: Unknown::Incomplete,
            });
        }
        let Some(endpoint_host) = transport
            .reviewed_endpoint_host(&target)
            .filter(|host| valid_endpoint_host(host))
        else {
            return Ok(Verification::Unknown {
                reason: Unknown::Unavailable,
            });
        };
        let response =
            match transport.read(target.tenant_id, application_request(target.object_id)?) {
                Ok(response) => response,
                Err(reason) => return Ok(Verification::Unknown { reason }),
            };
        if response.identity.tenant != identity.tenant {
            return Ok(Verification::Foreign {
                reason: "differentTenant",
            });
        }
        if response.identity != identity {
            return Ok(Verification::Foreign {
                reason: "differentOperator",
            });
        }
        let application = match response.application() {
            Ok(application) => application,
            Err(reason) => return Ok(Verification::Unknown { reason }),
        };
        if application.id != target.object_id {
            return Ok(Verification::Mismatch {
                reason: "differentObject",
            });
        }
        if application.api.requested_access_token_version.is_none() {
            return Ok(Verification::Unknown {
                reason: Unknown::Incomplete,
            });
        }
        if target
            .expected_application_id
            .is_some_and(|expected| expected != application.app_id)
        {
            return Ok(Verification::Mismatch {
                reason: "differentApplication",
            });
        }
        let identifier_uri = format!("api://{endpoint_host}/{}", application.app_id);
        if application.sign_in_audience != "AzureADMyOrg"
            || application.identifier_uris.as_slice() != [identifier_uri]
            || application.api.requested_access_token_version != Some(2)
        {
            return Ok(Verification::Mismatch {
                reason: "unexpectedProperties",
            });
        }
        let properties = Properties {
            object_id: application.id,
            application_id: application.app_id,
            single_tenant: true,
            identifier_uri_matches: true,
            access_token_version: 2,
        };
        let checked_at = crate::timestamp::now();
        let verification_digest = digest(json!([
            "application-registration-verification",
            binding_values(&target, &identity),
            checked_at,
            [
                properties.object_id,
                properties.application_id,
                properties.single_tenant,
                properties.identifier_uri_matches,
                properties.access_token_version
            ]
        ]));
        let binding = Box::new(Binding::new(target.clone(), &identity));
        state.pending = Some(Pending {
            target,
            identity,
            endpoint_host: endpoint_host.into(),
            properties: properties.clone(),
            checked_at: checked_at.clone(),
            digest: verification_digest.clone(),
            issued: Instant::now(),
        });
        Ok(Verification::Verified {
            kind: "verification",
            binding,
            properties,
            checked_at,
            verification_digest,
        })
    }

    fn confirm(
        &self,
        resolve: impl FnOnce(u64) -> AppResult<Target>,
        transport: &impl Transport,
        expected_digest: &str,
    ) -> AppResult<ReviewReceipt> {
        let mut state = self
            .0
            .lock()
            .map_err(|_| invalid("Application review is unavailable."))?;
        let pending = state.pending.take().ok_or_else(|| {
            invalid("Verify this exact application again before recording the review.")
        })?;
        let target = resolve(state.revision)?;
        if pending.issued.elapsed() >= REVIEW_LIFETIME
            || pending.target != target
            || expected_digest != pending.digest
            || transport.identity().ok().as_ref() != Some(&pending.identity)
            || transport.reviewed_endpoint_host(&target) != Some(pending.endpoint_host.as_str())
        {
            return Err(invalid(
                "The application review context changed. Verify again.",
            ));
        }
        let reviewed_at = crate::timestamp::now();
        let binding = binding_values(&target, &pending.identity);
        let receipt_digest = digest(json!([
            "application-registration-review-receipt",
            binding,
            pending.properties.application_id,
            pending.checked_at,
            reviewed_at,
            pending.digest
        ]));
        Ok(ReviewReceipt {
            binding: Binding::new(target, &pending.identity),
            kind: "ownershipReviewDecision",
            application_id: pending.properties.application_id,
            verified_at: pending.checked_at,
            reviewed_at,
            verification_digest: pending.digest,
            receipt_digest,
        })
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReviewReceipt {
    kind: &'static str,
    binding: Binding,
    application_id: Uuid,
    verified_at: String,
    reviewed_at: String,
    verification_digest: String,
    receipt_digest: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Binding {
    provider: &'static str,
    cloud: &'static str,
    operator_digest: String,
    #[serde(flatten)]
    target: Target,
}

impl Binding {
    fn new(target: Target, identity: &Identity) -> Self {
        Self {
            provider: PROVIDER,
            cloud: "AzureCloud",
            operator_digest: identity.operator.clone(),
            target,
        }
    }
}

#[tauri::command]
pub(crate) async fn get_project_application_review_context(
    state: tauri::State<'_, crate::AppState>,
    project_path: String,
    environment: String,
) -> AppResult<ReviewContext> {
    let _lock = state.environment_writes.lock().await;
    state
        .application_reviews
        .context(&state.database, &project_path, &environment)
}

#[tauri::command]
pub(crate) async fn verify_project_application_ownership(
    state: tauri::State<'_, crate::AppState>,
    request: ReviewRequest,
) -> AppResult<Verification> {
    let _lock = state.environment_writes.lock().await;
    verify_saved_project(
        &state.application_reviews,
        &state.database,
        &request,
        &OfflineTransport,
    )
}

#[tauri::command]
pub(crate) async fn confirm_project_application_review(
    state: tauri::State<'_, crate::AppState>,
    request: ReviewRequest,
    verification_digest: String,
) -> AppResult<ReviewReceipt> {
    let _lock = state.environment_writes.lock().await;
    confirm_saved_project(
        &state.application_reviews,
        &state.database,
        &request,
        &OfflineTransport,
        &verification_digest,
    )
}

fn verify_saved_project(
    reviews: &Reviews,
    database: &Database,
    request: &ReviewRequest,
    transport: &impl Transport,
) -> AppResult<Verification> {
    reviews.verify(|revision| request.resolve(database, revision), transport)
}

fn confirm_saved_project(
    reviews: &Reviews,
    database: &Database,
    request: &ReviewRequest,
    transport: &impl Transport,
    verification_digest: &str,
) -> AppResult<ReviewReceipt> {
    reviews.confirm(
        |revision| request.resolve(database, revision),
        transport,
        verification_digest,
    )
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::models::{CatalogEntry, ProjectSummary, ProjectTemplateSnapshot};
    use std::{cell::Cell, collections::HashMap, fs, sync::Arc};

    const TENANT: &str = "11111111-1111-4111-8111-111111111111";
    const SUBSCRIPTION: &str = "22222222-2222-4222-8222-222222222222";
    const OBJECT: &str = "33333333-3333-4333-8333-333333333333";
    const APPLICATION: &str = "44444444-4444-4444-8444-444444444444";
    const OTHER: &str = "55555555-5555-4555-8555-555555555555";
    const FIELD: &str = "AZD_PIM_REVOCATION_APPLICATION_OBJECT_ID";

    struct Fixture {
        _root: tempfile::TempDir,
        state: crate::AppState,
        project: String,
    }

    impl Fixture {
        fn new() -> Self {
            let root = tempfile::tempdir().unwrap();
            let database = Database::open(&root.path().join("test.sqlite")).unwrap();
            let path = root.path().join("project");
            fs::create_dir_all(path.join(".azure/dev")).unwrap();
            let project = fs::canonicalize(&path).unwrap().display().to_string();
            let summary = ProjectSummary {
                id: "fixture".into(),
                name: "Fixture".into(),
                path: project.clone(),
                environment: Some("dev".into()),
                template_id: Some("fixture".into()),
                revision: Some("a".repeat(40)),
                last_operation: None,
                state: "ready".into(),
            };
            let entry: CatalogEntry = serde_json::from_value(json!({
                "id":"fixture","title":"Fixture","description":"","summary":"","repository":"https://github.com/nathanmcnulty/azd-pim",
                "commit":"a".repeat(40),"tags":[],"platforms":["windows"],"trusted":true,
                "manifest":{
                    "schemaVersion":"1.0","prerequisites":[],"connections":[],
                    "applicationOwnership":{"kind":"verifier","provider":PROVIDER,"objectIdField":"object","applicationIdField":"client"},
                    "configuration":{"groups":[{"id":"ownership","title":"Ownership","fields":[
                        {"id":"object","environmentVariable":FIELD,"type":"entraObject","label":"Application object ID"},
                        {"id":"client","environmentVariable":"AZD_PIM_REVOCATION_APPLICATION_CLIENT_ID","type":"entraObject","label":"Application ID"}
                    ]}]}
                }
            })).unwrap();
            let snapshot = ProjectTemplateSnapshot::new(entry).unwrap();
            database.upsert_project(&summary).unwrap();
            database.save_project_template(&summary, &snapshot).unwrap();
            let fixture = Self {
                _root: root,
                state: crate::AppState {
                    database: Arc::new(database),
                    operations: Default::default(),
                    environment_writes: Default::default(),
                    application_reviews: Reviews::default(),
                    template_cache: Path::new(&project).join("cache"),
                    inspected_manifests: Mutex::new(HashMap::new()),
                    reviewed_contracts: Mutex::new(HashMap::new()),
                    catalog_reviews: Default::default(),
                    execution_approvals: Default::default(),
                    tool_install_approvals: Default::default(),
                    graph_sessions: Default::default(),
                },
                project,
            };
            fixture.set_saved(TENANT, OBJECT);
            fixture
        }

        fn set_saved(&self, tenant: &str, object_id: &str) {
            fs::write(Path::new(&self.project).join(".azure/dev/.env"), format!(
                "AZD_GUI_AZURE_CLOUD=AzureCloud\nAZURE_TENANT_ID={tenant}\nAZURE_SUBSCRIPTION_ID={SUBSCRIPTION}\nAZURE_RESOURCE_GROUP=rg-fixture\n{FIELD}={object_id}\nAZD_PIM_REVOCATION_APPLICATION_CLIENT_ID={APPLICATION}\n"
            )).unwrap();
        }

        fn target(&self) -> Target {
            registered_target(&self.state.database, &self.project, "dev").unwrap()
        }
    }

    const WORKFLOW_HOST: &str = "example.logic.azure.com";

    pub(crate) fn application_body() -> Value {
        json!({
            "@odata.context": "https://graph.microsoft.com/v1.0/$metadata#applications(id,appId,signInAudience,identifierUris,api)/$entity",
            "id": OBJECT,
            "appId": APPLICATION,
            "signInAudience": "AzureADMyOrg",
            "identifierUris": [format!("api://{WORKFLOW_HOST}/{APPLICATION}")],
            "api": { "requestedAccessTokenVersion": 2 }
        })
    }

    struct Scripted {
        identity: Option<Identity>,
        reviewed_host: Option<&'static str>,
        response: Result<ApplicationRead, Unknown>,
        read_count: Cell<usize>,
    }

    impl Scripted {
        fn matching() -> Self {
            let identity = Identity {
                tenant: uuid(TENANT).unwrap(),
                operator: "b".repeat(64),
            };
            Self {
                identity: Some(identity.clone()),
                reviewed_host: Some(WORKFLOW_HOST),
                response: Ok(ApplicationRead {
                    identity,
                    status: reqwest::StatusCode::OK,
                    body: serde_json::to_vec(&application_body()).unwrap(),
                }),
                read_count: Cell::new(0),
            }
        }
    }

    impl Transport for Scripted {
        fn identity(&self) -> Result<Identity, Unknown> {
            self.identity.clone().ok_or(Unknown::Unavailable)
        }
        fn reviewed_endpoint_host(&self, target: &Target) -> Option<&str> {
            assert_eq!(target.tenant_id, uuid(TENANT).unwrap());
            assert_eq!(target.subscription_id, uuid(SUBSCRIPTION).unwrap());
            assert_eq!(target.resource_group, "rg-fixture");
            self.reviewed_host
        }

        fn read(
            &self,
            tenant: Uuid,
            request: reqwest::Request,
        ) -> Result<ApplicationRead, Unknown> {
            assert_eq!(tenant, uuid(TENANT).unwrap());
            assert_eq!(request.method(), reqwest::Method::GET);
            let url = request.url();
            assert_eq!(
                url.origin().ascii_serialization(),
                "https://graph.microsoft.com"
            );
            assert_eq!(url.path(), format!("/v1.0/applications/{OBJECT}"));
            assert_eq!(
                url.query_pairs()
                    .map(|(key, value)| (key.into_owned(), value.into_owned()))
                    .collect::<Vec<_>>(),
                [(
                    "$select".to_owned(),
                    "id,appId,signInAudience,identifierUris,api".to_owned()
                )]
            );
            assert!(
                url.username().is_empty() && url.password().is_none() && url.fragment().is_none()
            );
            assert!(request.body().is_none() && request.headers().is_empty());
            self.read_count.set(self.read_count.get() + 1);
            self.response.clone()
        }
    }

    pub(crate) fn verify_worker_response(
        tenant: Uuid,
        operator: String,
        status: u16,
        body: Option<String>,
        expected_unknown: Option<Unknown>,
    ) {
        let fixture = Fixture::new();
        let request = fixture.request();
        let identity = Identity { tenant, operator };
        let transport = Scripted {
            identity: Some(identity.clone()),
            reviewed_host: Some(WORKFLOW_HOST),
            response: Ok(ApplicationRead {
                identity,
                status: reqwest::StatusCode::from_u16(status).unwrap(),
                body: body.unwrap_or_default().into_bytes(),
            }),
            read_count: Cell::new(0),
        };
        let result = fixture.verify_saved(&request, &transport).unwrap();
        match (result, expected_unknown) {
            (
                Verification::Verified {
                    verification_digest,
                    ..
                },
                None,
            ) => {
                let receipt = fixture
                    .confirm_saved(&request, &transport, &verification_digest)
                    .unwrap();
                let receipt = serde_json::to_value(receipt).unwrap();
                assert_eq!(receipt["binding"]["objectId"], OBJECT);
                assert_eq!(receipt["applicationId"], APPLICATION);
                assert!(receipt.get("body").is_none());
                assert!(fixture
                    .confirm_saved(&request, &transport, &verification_digest)
                    .is_err());
            }
            (Verification::Unknown { reason }, Some(expected)) => {
                assert_eq!(
                    serde_json::to_value(reason).unwrap(),
                    serde_json::to_value(expected).unwrap()
                );
                assert!(fixture
                    .confirm_saved(&request, &transport, &"a".repeat(64))
                    .is_err());
            }
            _ => panic!("Unexpected application verification state"),
        }
        assert_eq!(transport.read_count.get(), 1);
    }

    fn verify(reviews: &Reviews, target: Target, transport: &Scripted) -> String {
        let result =
            serde_json::to_value(reviews.verify(|_| Ok(target), transport).unwrap()).unwrap();
        assert_eq!(result["kind"], "verification");
        assert_eq!(result["state"], "verified");
        assert_eq!(result["binding"]["objectId"], OBJECT);
        assert_eq!(result["properties"]["applicationId"], APPLICATION);
        assert!(result.get("reason").is_none());
        result["verificationDigest"].as_str().unwrap().to_owned()
    }

    impl Fixture {
        fn request(&self) -> ReviewRequest {
            let context = self
                .state
                .application_reviews
                .context(&self.state.database, &self.project, "dev")
                .unwrap();
            ReviewRequest {
                project_path: self.project.clone(),
                environment: "dev".into(),
                context_digest: context.context_digest,
            }
        }

        fn verify_saved(
            &self,
            request: &ReviewRequest,
            transport: &impl Transport,
        ) -> AppResult<Verification> {
            verify_saved_project(
                &self.state.application_reviews,
                &self.state.database,
                request,
                transport,
            )
        }

        fn confirm_saved(
            &self,
            request: &ReviewRequest,
            transport: &impl Transport,
            digest: &str,
        ) -> AppResult<ReviewReceipt> {
            confirm_saved_project(
                &self.state.application_reviews,
                &self.state.database,
                request,
                transport,
                digest,
            )
        }

        fn fingerprint(&self, request: &ReviewRequest, transport: &Scripted) -> String {
            let result =
                serde_json::to_value(self.verify_saved(request, transport).unwrap()).unwrap();
            assert_eq!(result["kind"], "verification");
            assert_eq!(result["state"], "verified");
            assert_eq!(result["binding"]["objectId"], OBJECT);
            assert_eq!(result["properties"]["applicationId"], APPLICATION);
            assert!(result.get("reason").is_none());
            result["verificationDigest"].as_str().unwrap().to_owned()
        }

        fn set_file(&self, contents: &str, seconds: u64) {
            let file = fs::File::create(Path::new(&self.project).join(".azure/dev/.env")).unwrap();
            use std::io::Write;
            (&file).write_all(contents.as_bytes()).unwrap();
            file.set_times(
                fs::FileTimes::new().set_modified(UNIX_EPOCH + Duration::from_secs(seconds)),
            )
            .unwrap();
        }
    }

    #[test]
    fn ownership_only_context_uses_narrow_native_values_and_reaches_offline_transport() {
        let fixture = Fixture::new();
        let path = Path::new(&fixture.project).join(".azure/dev/.env");
        let text =
            fs::read_to_string(&path).unwrap() + "UNRELATED_SECRET=sentinel-not-for-renderer\n";
        fixture.set_file(&text, 1_800_000_000);
        let reviews = &fixture.state.application_reviews;
        let database = &fixture.state.database;
        let shown = reviews.context(database, &fixture.project, "dev").unwrap();
        let serialized = serde_json::to_value(&shown).unwrap();
        assert_eq!(serialized["tenantId"], TENANT);
        assert_eq!(serialized["subscriptionId"], SUBSCRIPTION);
        assert_eq!(serialized["resourceGroup"], "rg-fixture");
        assert_eq!(serialized["objectId"], OBJECT);
        assert_eq!(serialized["expectedApplicationId"], APPLICATION);
        assert_eq!(
            serialized["sourceContractDigest"],
            fixture.target().source_contract_digest
        );
        assert_eq!(
            serialized["targetResourceDigest"],
            fixture.target().target_resource_digest
        );
        assert_eq!(serialized.as_object().unwrap().len(), 10);
        assert!(!serialized.to_string().contains("sentinel-not-for-renderer"));
        assert!(reviews
            .context(database, &fixture.project, "foreign")
            .is_err());
        assert!(reviews
            .context(database, r"\\not-registered.invalid\share", "dev")
            .is_err());
        assert!(matches!(
            fixture
                .verify_saved(&fixture.request(), &OfflineTransport)
                .unwrap(),
            Verification::Unknown {
                reason: Unknown::ProviderUnavailable
            }
        ));
        assert_eq!(fs::read_to_string(path).unwrap(), text);
    }

    #[test]
    fn verify_and_confirm_reject_subscription_group_and_source_drift() {
        let fixture = Fixture::new();
        let transport = Scripted::matching();
        let path = Path::new(&fixture.project).join(".azure/dev/.env");
        let before = fs::read_to_string(&path).unwrap();
        for (old, new) in [
            (SUBSCRIPTION, OTHER),
            ("rg-fixture", "rg-changed"),
            (TENANT, OTHER),
            (OBJECT, OTHER),
        ] {
            fixture.set_file(&before, 1_800_000_000);
            let request = fixture.request();
            let digest = fixture.fingerprint(&request, &transport);
            fixture.set_file(&before.replace(old, new), 1_800_000_001);
            let reads = transport.read_count.get();
            assert!(fixture
                .confirm_saved(&request, &transport, &digest)
                .is_err());
            assert!(fixture.verify_saved(&request, &transport).is_err());
            assert_eq!(transport.read_count.get(), reads);
            fixture.set_file(&before, 1_800_000_002);
            assert!(fixture
                .confirm_saved(&fixture.request(), &transport, &digest)
                .is_err());
        }

        let database = &fixture.state.database;
        let old_snapshot = database
            .project_template_exact(&fixture.project)
            .unwrap()
            .unwrap();
        let request = fixture.request();
        let digest = fixture.fingerprint(&request, &transport);
        let mut entry = old_snapshot.entry;
        entry
            .manifest
            .as_mut()
            .unwrap()
            .application_ownership
            .as_mut()
            .unwrap()
            .application_id_field = None;
        let changed = ProjectTemplateSnapshot::new(entry).unwrap();
        assert_ne!(changed.digest, old_snapshot.digest);
        let project = database.projects().unwrap().remove(0);
        database.save_project_template(&project, &changed).unwrap();
        assert!(fixture.verify_saved(&request, &transport).is_err());
        assert!(fixture
            .confirm_saved(&request, &transport, &digest)
            .is_err());
    }

    #[tokio::test]
    async fn failed_requests_and_native_edits_cannot_revive_a_pending_review() {
        let fixture = Fixture::new();
        let transport = Scripted::matching();
        let path = Path::new(&fixture.project).join(".azure/dev/.env");
        let before = fs::read_to_string(&path).unwrap();
        fixture.set_file(&before, 1_800_000_000);

        let request = fixture.request();
        let digest = fixture.fingerprint(&request, &transport);
        fs::remove_file(&path).unwrap();
        assert!(fixture.verify_saved(&request, &transport).is_err());
        fixture.set_file(&before, 1_800_000_000);
        assert!(fixture
            .confirm_saved(&request, &transport, &digest)
            .is_err());

        let digest = fixture.fingerprint(&request, &transport);
        fs::remove_file(&path).unwrap();
        assert!(fixture
            .confirm_saved(&request, &transport, &digest)
            .is_err());
        fixture.set_file(&before, 1_800_000_000);
        assert!(fixture
            .confirm_saved(&request, &transport, &digest)
            .is_err());

        let digest = fixture.fingerprint(&request, &transport);
        fixture.set_file(&before.replace("rg-fixture", "rg-changed"), 1_800_000_001);
        assert!(fixture.verify_saved(&request, &transport).is_err());
        fixture.set_file(&before, 1_800_000_000);
        assert!(fixture
            .confirm_saved(&request, &transport, &digest)
            .is_err());

        let digest = fixture.fingerprint(&request, &transport);
        fixture.set_file(&before.replace("rg-fixture", "rg-changed"), 1_800_000_001);
        fixture.set_file(&before, 1_800_000_002);
        assert!(fixture
            .confirm_saved(&request, &transport, &digest)
            .is_err());

        fixture.set_file(&before, 1_800_000_000);
        let digest = fixture.fingerprint(&request, &transport);
        {
            let _write = fixture.state.lock_environment_for_write().await;
            fixture.set_file(&before.replace("rg-fixture", "rg-changed"), 1_800_000_000);
        }
        {
            let _write = fixture.state.lock_environment_for_write().await;
            fixture.set_file(&before, 1_800_000_000);
        }
        let fresh_request = fixture.request();
        assert_ne!(fresh_request.context_digest, request.context_digest);
        assert!(fixture
            .confirm_saved(&fresh_request, &transport, &digest)
            .is_err());
        assert!(fixture.verify_saved(&request, &transport).is_err());

        let digest = fixture.fingerprint(&fresh_request, &transport);
        let receipt = serde_json::to_value(
            fixture
                .confirm_saved(&fresh_request, &transport, &digest)
                .unwrap(),
        )
        .unwrap();
        assert_eq!(receipt["kind"], "ownershipReviewDecision");
        assert_eq!(receipt["binding"]["objectId"], OBJECT);
        assert!(receipt.get("ownershipSource").is_none());
        assert!(receipt.get("accessToken").is_none());
        assert!(fixture
            .confirm_saved(&fresh_request, &transport, &digest)
            .is_err());
        assert_eq!(fs::read_to_string(path).unwrap(), before);
    }

    #[tokio::test]
    async fn native_pim_environment_selection_cannot_revive_earlier_review() {
        let fixture = Fixture::new();
        fs::create_dir(Path::new(&fixture.project).join(".azure/recovery")).unwrap();
        let transport = Scripted::matching();
        let original = fixture.request();
        let proof = fixture.fingerprint(&original, &transport);

        crate::pim_pilot::select_saved_pim_environment(
            &fixture.state,
            fixture.project.clone(),
            "recovery".into(),
        )
        .await
        .unwrap();
        let selected = fixture.state.database.projects().unwrap().remove(0);
        assert_eq!(selected.environment.as_deref(), Some("recovery"));

        crate::pim_pilot::select_saved_pim_environment(
            &fixture.state,
            fixture.project.clone(),
            "dev".into(),
        )
        .await
        .unwrap();
        let restored = fixture.request();
        assert_ne!(restored.context_digest, original.context_digest);
        assert!(fixture
            .confirm_saved(&restored, &transport, &proof)
            .is_err());
        assert_eq!(transport.read_count.get(), 1);
    }

    #[test]
    fn native_context_observation_revokes_pending_on_failure_or_change() {
        let fixture = Fixture::new();
        let reviews = &fixture.state.application_reviews;
        let database = &fixture.state.database;
        let path = Path::new(&fixture.project).join(".azure/dev/.env");
        let before = fs::read_to_string(&path).unwrap();
        let transport = Scripted::matching();
        let stamp = 1_800_000_000;
        fixture.set_file(&before, stamp);

        let original = fixture.request();
        let proof = fixture.fingerprint(&original, &transport);
        assert_eq!(fixture.request().context_digest, original.context_digest);
        fs::remove_file(&path).unwrap();
        assert!(reviews.context(database, &fixture.project, "dev").is_err());
        fixture.set_file(&before, stamp);
        let restored = fixture.request();
        assert_ne!(restored.context_digest, original.context_digest);
        assert!(fixture
            .confirm_saved(&restored, &transport, &proof)
            .is_err());

        let proof = fixture.fingerprint(&restored, &transport);
        fixture.set_file(&before.replace("rg-fixture", "rg-changed"), stamp);
        let changed = reviews.context(database, &fixture.project, "dev").unwrap();
        assert_eq!(changed.resource_group, "rg-changed");
        fixture.set_file(&before, stamp);
        let restored_again = fixture.request();
        assert_ne!(restored_again.context_digest, restored.context_digest);
        assert!(fixture
            .confirm_saved(&restored_again, &transport, &proof)
            .is_err());
        assert_eq!(transport.read_count.get(), 2);
    }

    #[test]
    fn changed_operator_and_expired_proof_cannot_issue_a_receipt() {
        let fixture = Fixture::new();
        let transport = Scripted::matching();
        let request = fixture.request();
        let digest = fixture.fingerprint(&request, &transport);
        let mut different_operator = Scripted::matching();
        different_operator.identity.as_mut().unwrap().operator = "e".repeat(64);
        assert!(fixture
            .confirm_saved(&request, &different_operator, &digest)
            .is_err());
        let digest = fixture.fingerprint(&request, &transport);
        fixture
            .state
            .application_reviews
            .0
            .lock()
            .unwrap()
            .pending
            .as_mut()
            .unwrap()
            .issued -= REVIEW_LIFETIME;
        assert!(fixture
            .confirm_saved(&request, &transport, &digest)
            .is_err());
    }

    fn result(fixture: &Fixture, request: &ReviewRequest, transport: &Scripted) -> Value {
        serde_json::to_value(fixture.verify_saved(request, transport).unwrap()).unwrap()
    }

    #[test]
    fn exact_read_projects_safe_evidence_without_changing_saved_project() {
        let fixture = Fixture::new();
        let env = Path::new(&fixture.project).join(".azure/dev/.env");
        let before = fs::read(&env).unwrap();
        let database = &fixture.state.database;
        let source = database
            .project_template_exact(&fixture.project)
            .unwrap()
            .unwrap()
            .digest;
        let project = serde_json::to_value(database.projects().unwrap()).unwrap();

        let mut transport = Scripted::matching();
        let mut payload = application_body();
        payload["web"] = json!({"redirectUris":["https://synthetic.invalid/callback?sig=opaque"]});
        payload["displayName"] = json!("Untrusted display metadata");
        payload["accessToken"] = json!("synthetic-secret");
        payload["api"]["oauth2PermissionScopes"] = json!([{"value":"synthetic-grant"}]);
        transport.response.as_mut().unwrap().body = serde_json::to_vec(&payload).unwrap();

        let request = fixture.request();
        let evidence = result(&fixture, &request, &transport);
        assert_eq!(evidence["state"], "verified");
        assert_eq!(evidence["binding"]["tenantId"], TENANT);
        assert_eq!(
            evidence["properties"],
            json!({
                "objectId": OBJECT, "applicationId": APPLICATION,
                "singleTenant": true, "identifierUriMatches": true, "accessTokenVersion": 2
            })
        );
        let serialized = evidence.to_string();
        for private in [
            WORKFLOW_HOST,
            "synthetic.invalid",
            "Untrusted display metadata",
            "synthetic-secret",
            "synthetic-grant",
        ] {
            assert!(!serialized.contains(private));
        }
        assert!(evidence.get("ownershipSource").is_none());
        assert_eq!(transport.read_count.get(), 1);
        assert_eq!(fs::read(&env).unwrap(), before);
        assert_eq!(
            database
                .project_template_exact(&fixture.project)
                .unwrap()
                .unwrap()
                .digest,
            source
        );
        assert_eq!(
            serde_json::to_value(database.projects().unwrap()).unwrap(),
            project
        );

        // Older state can use the returned client ID after an exact object-ID read.
        let only_object = String::from_utf8(before).unwrap().replace(
            &format!("AZD_PIM_REVOCATION_APPLICATION_CLIENT_ID={APPLICATION}\n"),
            "",
        );
        fs::write(&env, only_object).unwrap();
        let evidence = result(&fixture, &fixture.request(), &transport);
        assert_eq!(evidence["state"], "verified");
        assert_eq!(evidence["properties"]["applicationId"], APPLICATION);
        assert!(evidence["binding"].get("expectedApplicationId").is_none());
        assert_eq!(transport.read_count.get(), 2);
    }

    #[test]
    fn changed_properties_remain_distinct_from_missing_or_wrong_resource_type() {
        let fixture = Fixture::new();
        let request = fixture.request();
        let mut transport = Scripted::matching();
        let mut changed = application_body();
        changed["api"]["requestedAccessTokenVersion"] = Value::Null;
        let mut missing_version = application_body();
        missing_version["api"] = json!({});
        let mut missing_api = application_body();
        missing_api.as_object_mut().unwrap().remove("api");
        let mut missing_type = application_body();
        missing_type
            .as_object_mut()
            .unwrap()
            .remove("@odata.context");
        let mut explicit_type = missing_type.clone();
        explicit_type["@odata.type"] = json!("#microsoft.graph.application");
        transport.response.as_mut().unwrap().body = serde_json::to_vec(&explicit_type).unwrap();
        assert_eq!(result(&fixture, &request, &transport)["state"], "verified");
        let baseline = fixture.fingerprint(&request, &Scripted::matching());

        let cases = [
            (changed, "mismatch", "unexpectedProperties"),
            (missing_version, "unknown", "incomplete"),
            (missing_api, "unknown", "incomplete"),
            (missing_type, "unknown", "incomplete"),
            (
                application_body_for("@odata.type", json!("#microsoft.graph.servicePrincipal")),
                "unknown",
                "incomplete",
            ),
            (
                application_body_for(
                    "@odata.type",
                    json!("#microsoft.graph.agentIdentityBlueprint"),
                ),
                "unknown",
                "incomplete",
            ),
            (
                application_body_for(
                    "@odata.context",
                    json!("https://graph.microsoft.com/v1.0/$metadata#servicePrincipals/$entity"),
                ),
                "unknown",
                "incomplete",
            ),
            (
                application_body_for("appId", json!(OTHER)),
                "mismatch",
                "differentApplication",
            ),
            (
                application_body_for("signInAudience", json!("AzureADMultipleOrgs")),
                "mismatch",
                "unexpectedProperties",
            ),
            (
                application_body_for(
                    "identifierUris",
                    json!([format!("api://foreign.logic.azure.com/{APPLICATION}")]),
                ),
                "mismatch",
                "unexpectedProperties",
            ),
        ];
        for (body, state, reason) in cases {
            transport.response.as_mut().unwrap().body = serde_json::to_vec(&body).unwrap();
            assert_eq!(
                result(&fixture, &request, &transport),
                json!({"state":state,"reason":reason})
            );
            assert!(fixture
                .confirm_saved(&request, &transport, &baseline)
                .is_err());
        }
    }

    fn application_body_for(key: &str, value: Value) -> Value {
        let mut body = application_body();
        body[key] = value;
        body
    }

    #[test]
    fn unavailable_and_unsupported_reads_are_single_attempts_without_proof() {
        use reqwest::StatusCode;
        let fixture = Fixture::new();
        let request = fixture.request();
        let mut transport = Scripted::matching();
        for (status, reason) in [
            (StatusCode::UNAUTHORIZED, "accessDenied"),
            (StatusCode::FORBIDDEN, "accessDenied"),
            (StatusCode::NOT_FOUND, "unavailable"),
            (StatusCode::TOO_MANY_REQUESTS, "unavailable"),
            (StatusCode::SERVICE_UNAVAILABLE, "unavailable"),
            (StatusCode::FOUND, "incomplete"),
            (StatusCode::PARTIAL_CONTENT, "incomplete"),
        ] {
            let response = transport.response.as_mut().unwrap();
            response.status = status;
            response.body = br#"{"error":{"message":"synthetic-provider-details"}}"#.to_vec();
            let before = transport.read_count.get();
            assert_eq!(
                result(&fixture, &request, &transport),
                json!({"state":"unknown","reason":reason})
            );
            assert_eq!(transport.read_count.get(), before + 1);
        }
        transport.response.as_mut().unwrap().status = StatusCode::OK;
        for body in [
            b"{".to_vec(),
            serde_json::to_vec(&json!({"value":[application_body()]})).unwrap(),
            serde_json::to_vec(&application_body_for(
                "@odata.nextLink",
                json!("https://synthetic.invalid/next"),
            ))
            .unwrap(),
            vec![b' '; MAX_APPLICATION_RESPONSE_BYTES + 1],
        ] {
            transport.response.as_mut().unwrap().body = body;
            assert_eq!(
                result(&fixture, &request, &transport),
                json!({"state":"unknown","reason":"incomplete"})
            );
        }
        transport.response = Err(Unknown::Cancelled);
        assert_eq!(
            result(&fixture, &request, &transport),
            json!({"state":"unknown","reason":"cancelled"})
        );
        assert!(fixture
            .state
            .application_reviews
            .0
            .lock()
            .unwrap()
            .pending
            .is_none());
    }

    #[test]
    fn only_transport_identity_and_independent_endpoint_evidence_can_bind_the_read() {
        let fixture = Fixture::new();
        let request = fixture.request();
        let mut transport = Scripted::matching();
        transport.reviewed_host = None;
        assert_eq!(
            result(&fixture, &request, &transport),
            json!({"state":"unknown","reason":"unavailable"})
        );
        transport.reviewed_host = Some("example.logic.azure.com/path?sig=secret");
        assert_eq!(
            result(&fixture, &request, &transport),
            json!({"state":"unknown","reason":"unavailable"})
        );
        assert_eq!(transport.read_count.get(), 0);

        transport.reviewed_host = Some(WORKFLOW_HOST);
        let response = transport.response.as_mut().unwrap();
        response.identity.tenant = uuid(OTHER).unwrap();
        response.body =
            serde_json::to_vec(&application_body_for("tenantId", json!(TENANT))).unwrap();
        assert_eq!(
            result(&fixture, &request, &transport),
            json!({"state":"foreign","reason":"differentTenant"})
        );
        let response = transport.response.as_mut().unwrap();
        response.identity.tenant = uuid(TENANT).unwrap();
        response.identity.operator = "e".repeat(64);
        assert_eq!(
            result(&fixture, &request, &transport),
            json!({"state":"foreign","reason":"differentOperator"})
        );
        transport.response.as_mut().unwrap().identity.operator = "b".repeat(64);

        let digest = fixture.fingerprint(&request, &transport);
        transport.reviewed_host = Some("changed.logic.azure.com");
        assert!(fixture
            .confirm_saved(&request, &transport, &digest)
            .is_err());
        transport.reviewed_host = Some(WORKFLOW_HOST);
        assert!(fixture
            .confirm_saved(&request, &transport, &digest)
            .is_err());
        assert_eq!(transport.read_count.get(), 3);
    }

    #[test]
    fn wrong_identity_denied_incomplete_and_mismatched_results_cannot_be_reviewed() {
        let fixture = Fixture::new();
        let target = fixture.target();
        let reviews = Reviews::default();
        let mut transport = Scripted::matching();
        transport.identity.as_mut().unwrap().tenant = uuid(OTHER).unwrap();
        assert!(matches!(
            reviews.verify(|_| Ok(target.clone()), &transport).unwrap(),
            Verification::Foreign {
                reason: "differentTenant"
            }
        ));
        assert_eq!(transport.read_count.get(), 0);

        let mut transport = Scripted::matching();
        let token = verify(&reviews, target.clone(), &transport);
        transport.response = Err(Unknown::AccessDenied);
        assert!(matches!(
            reviews.verify(|_| Ok(target.clone()), &transport).unwrap(),
            Verification::Unknown {
                reason: Unknown::AccessDenied
            }
        ));
        assert!(reviews
            .confirm(|_| Ok(target.clone()), &transport, &token)
            .is_err());

        let mut transport = Scripted::matching();
        let mut foreign_object = application_body();
        foreign_object["id"] = json!(OTHER);
        transport.response.as_mut().unwrap().body = serde_json::to_vec(&foreign_object).unwrap();
        assert!(matches!(
            reviews.verify(|_| Ok(target.clone()), &transport).unwrap(),
            Verification::Mismatch {
                reason: "differentObject"
            }
        ));
        let mut missing_version = application_body();
        missing_version["api"] = json!({});
        transport.response.as_mut().unwrap().body = serde_json::to_vec(&missing_version).unwrap();
        assert!(matches!(
            reviews.verify(|_| Ok(target.clone()), &transport).unwrap(),
            Verification::Unknown {
                reason: Unknown::Incomplete
            }
        ));
        assert!(matches!(
            reviews.verify(|_| Ok(target), &OfflineTransport).unwrap(),
            Verification::Unknown {
                reason: Unknown::ProviderUnavailable
            }
        ));
    }
}
