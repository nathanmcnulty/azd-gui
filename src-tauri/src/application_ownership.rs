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
    time::{Duration, Instant},
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
    let saved = crate::evidence::read_environment_values(path, environment, &keys)
        .ok_or_else(|| invalid("The saved environment is unavailable."))?;
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

#[derive(Debug, Clone)]
struct Observation {
    identity: Identity,
    object_id: Uuid,
    application_id: Uuid,
    single_tenant: bool,
    identifier_uri_matches: bool,
    access_token_version: Option<u8>,
}

#[derive(Debug, Clone, Copy, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) enum Unknown {
    Unavailable,
    AccessDenied,
    Incomplete,
    Cancelled,
}

trait Transport {
    fn identity(&self) -> Option<Identity>;
    fn read(&self, tenant: Uuid, object: Uuid) -> Result<Observation, Unknown>;
}

struct OfflineTransport;
impl Transport for OfflineTransport {
    fn identity(&self) -> Option<Identity> {
        None
    }
    fn read(&self, _: Uuid, _: Uuid) -> Result<Observation, Unknown> {
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
    properties: Properties,
    checked_at: String,
    digest: String,
    issued: Instant,
}

#[derive(Default)]
pub struct Reviews(Mutex<Option<Pending>>);

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
    fn verify(&self, target: Target, transport: &impl Transport) -> AppResult<Verification> {
        let mut pending = self
            .0
            .lock()
            .map_err(|_| invalid("Application review is unavailable."))?;
        *pending = None;
        let Some(identity) = transport.identity() else {
            return Ok(Verification::Unknown {
                reason: Unknown::Unavailable,
            });
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
        let observed = match transport.read(target.tenant_id, target.object_id) {
            Ok(observed) => observed,
            Err(reason) => return Ok(Verification::Unknown { reason }),
        };
        if observed.identity.tenant != identity.tenant {
            return Ok(Verification::Foreign {
                reason: "differentTenant",
            });
        }
        if observed.identity != identity {
            return Ok(Verification::Foreign {
                reason: "differentOperator",
            });
        }
        if observed.object_id != target.object_id {
            return Ok(Verification::Mismatch {
                reason: "differentObject",
            });
        }
        if observed.application_id.is_nil() || observed.access_token_version.is_none() {
            return Ok(Verification::Unknown {
                reason: Unknown::Incomplete,
            });
        }
        if target
            .expected_application_id
            .is_some_and(|expected| expected != observed.application_id)
        {
            return Ok(Verification::Mismatch {
                reason: "differentApplication",
            });
        }
        if !observed.single_tenant
            || !observed.identifier_uri_matches
            || observed.access_token_version != Some(2)
        {
            return Ok(Verification::Mismatch {
                reason: "unexpectedProperties",
            });
        }
        let properties = Properties {
            object_id: observed.object_id,
            application_id: observed.application_id,
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
        *pending = Some(Pending {
            target,
            identity,
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
        target: Target,
        transport: &impl Transport,
        expected_digest: &str,
    ) -> AppResult<ReviewReceipt> {
        let mut slot = self
            .0
            .lock()
            .map_err(|_| invalid("Application review is unavailable."))?;
        let pending = slot.take().ok_or_else(|| {
            invalid("Verify this exact application again before recording the review.")
        })?;
        if pending.issued.elapsed() >= REVIEW_LIFETIME
            || pending.target != target
            || expected_digest != pending.digest
            || transport.identity().as_ref() != Some(&pending.identity)
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
pub(crate) fn verify_project_application_ownership(
    state: tauri::State<'_, crate::AppState>,
    reviews: tauri::State<'_, Reviews>,
    project_path: String,
    environment: String,
) -> AppResult<Verification> {
    reviews.verify(
        registered_target(&state.database, &project_path, &environment)?,
        &OfflineTransport,
    )
}

#[tauri::command]
pub fn confirm_project_application_review(
    state: tauri::State<'_, crate::AppState>,
    reviews: tauri::State<'_, Reviews>,
    project_path: String,
    environment: String,
    verification_digest: String,
) -> AppResult<ReviewReceipt> {
    reviews.confirm(
        registered_target(&state.database, &project_path, &environment)?,
        &OfflineTransport,
        &verification_digest,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{CatalogEntry, ProjectSummary, ProjectTemplateSnapshot};
    use std::{cell::Cell, fs};

    const TENANT: &str = "11111111-1111-4111-8111-111111111111";
    const SUBSCRIPTION: &str = "22222222-2222-4222-8222-222222222222";
    const OBJECT: &str = "33333333-3333-4333-8333-333333333333";
    const APPLICATION: &str = "44444444-4444-4444-8444-444444444444";
    const OTHER: &str = "55555555-5555-4555-8555-555555555555";
    const FIELD: &str = "AZD_PIM_REVOCATION_APPLICATION_OBJECT_ID";

    struct Fixture {
        _root: tempfile::TempDir,
        database: Database,
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
                "id":"fixture","title":"Fixture","description":"","summary":"","repository":"https://example.invalid/fixture",
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
                database,
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
            registered_target(&self.database, &self.project, "dev").unwrap()
        }
    }

    struct Scripted {
        identity: Option<Identity>,
        response: Result<Observation, Unknown>,
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
                response: Ok(Observation {
                    identity,
                    object_id: uuid(OBJECT).unwrap(),
                    application_id: uuid(APPLICATION).unwrap(),
                    single_tenant: true,
                    identifier_uri_matches: true,
                    access_token_version: Some(2),
                }),
                read_count: Cell::new(0),
            }
        }
    }

    impl Transport for Scripted {
        fn identity(&self) -> Option<Identity> {
            self.identity.clone()
        }
        fn read(&self, tenant: Uuid, object: Uuid) -> Result<Observation, Unknown> {
            assert_eq!(tenant, uuid(TENANT).unwrap());
            assert_eq!(object, uuid(OBJECT).unwrap());
            self.read_count.set(self.read_count.get() + 1);
            self.response.clone()
        }
    }

    fn verify(reviews: &Reviews, target: Target, transport: &Scripted) -> String {
        let result = serde_json::to_value(reviews.verify(target, transport).unwrap()).unwrap();
        assert_eq!(result["kind"], "verification");
        assert_eq!(result["state"], "verified");
        assert_eq!(result["binding"]["objectId"], OBJECT);
        assert_eq!(result["properties"]["applicationId"], APPLICATION);
        assert!(result.get("reason").is_none());
        result["verificationDigest"].as_str().unwrap().to_owned()
    }

    #[test]
    fn actual_registered_caller_rechecks_object_and_tenant_before_one_use_review_receipt() {
        let fixture = Fixture::new();
        let transport = Scripted::matching();
        let reviews = Reviews::default();
        let target = fixture.target();
        let before = fs::read(Path::new(&fixture.project).join(".azure/dev/.env")).unwrap();
        assert!(reviews
            .confirm(target.clone(), &transport, "unverified")
            .is_err());
        assert!(registered_target(&fixture.database, &fixture.project, "foreign").is_err());
        assert!(
            registered_target(&fixture.database, r"\\not-registered.invalid\share", "dev").is_err()
        );
        let digest = verify(&reviews, target.clone(), &transport);
        fixture.set_saved(OTHER, OBJECT);
        assert!(reviews
            .confirm(fixture.target(), &transport, &digest)
            .is_err());
        fixture.set_saved(TENANT, OBJECT);
        let digest = verify(&reviews, target.clone(), &transport);
        fixture.set_saved(TENANT, OTHER);
        assert!(reviews
            .confirm(fixture.target(), &transport, &digest)
            .is_err());
        fixture.set_saved(TENANT, OBJECT);
        let digest = verify(&reviews, target.clone(), &transport);
        let path = Path::new(&fixture.project).join(".azure/dev/.env");
        let previous = String::from_utf8(before.clone()).unwrap();
        fs::write(&path, previous.replace("rg-fixture", "rg-changed")).unwrap();
        assert!(reviews
            .confirm(fixture.target(), &transport, &digest)
            .is_err());
        fs::write(&path, &before).unwrap();

        let digest = verify(&reviews, target.clone(), &transport);
        let mut different_operator = Scripted::matching();
        different_operator.identity.as_mut().unwrap().operator = "e".repeat(64);
        assert!(reviews
            .confirm(fixture.target(), &different_operator, &digest)
            .is_err());

        let digest = verify(&reviews, target.clone(), &transport);
        reviews.0.lock().unwrap().as_mut().unwrap().issued -= REVIEW_LIFETIME;
        assert!(reviews
            .confirm(fixture.target(), &transport, &digest)
            .is_err());

        let digest = verify(&reviews, target.clone(), &transport);
        let receipt = serde_json::to_value(
            reviews
                .confirm(fixture.target(), &transport, &digest)
                .unwrap(),
        )
        .unwrap();
        assert_eq!(receipt["kind"], "ownershipReviewDecision");
        assert_eq!(receipt["binding"]["tenantId"], TENANT);
        assert_eq!(receipt["binding"]["objectId"], OBJECT);
        assert!(receipt.get("ownershipSource").is_none());
        assert!(receipt.get("accessToken").is_none());
        assert!(reviews.confirm(target, &transport, &digest).is_err());
        assert_eq!(transport.read_count.get(), 6);
        assert_eq!(
            before,
            fs::read(Path::new(&fixture.project).join(".azure/dev/.env")).unwrap()
        );
    }

    #[test]
    fn wrong_identity_denied_incomplete_and_mismatched_results_cannot_be_reviewed() {
        let fixture = Fixture::new();
        let target = fixture.target();
        let reviews = Reviews::default();
        let mut transport = Scripted::matching();
        transport.identity.as_mut().unwrap().tenant = uuid(OTHER).unwrap();
        assert!(matches!(
            reviews.verify(target.clone(), &transport).unwrap(),
            Verification::Foreign {
                reason: "differentTenant"
            }
        ));
        assert_eq!(transport.read_count.get(), 0);

        let mut transport = Scripted::matching();
        let token = verify(&reviews, target.clone(), &transport);
        transport.response = Err(Unknown::AccessDenied);
        assert!(matches!(
            reviews.verify(target.clone(), &transport).unwrap(),
            Verification::Unknown {
                reason: Unknown::AccessDenied
            }
        ));
        assert!(reviews.confirm(target.clone(), &transport, &token).is_err());

        let mut transport = Scripted::matching();
        transport.response.as_mut().unwrap().object_id = uuid(OTHER).unwrap();
        assert!(matches!(
            reviews.verify(target.clone(), &transport).unwrap(),
            Verification::Mismatch {
                reason: "differentObject"
            }
        ));
        transport.response.as_mut().unwrap().object_id = target.object_id;
        transport.response.as_mut().unwrap().access_token_version = None;
        assert!(matches!(
            reviews.verify(target.clone(), &transport).unwrap(),
            Verification::Unknown {
                reason: Unknown::Incomplete
            }
        ));
        assert!(matches!(
            reviews.verify(target, &OfflineTransport).unwrap(),
            Verification::Unknown {
                reason: Unknown::Unavailable
            }
        ));
    }
}
