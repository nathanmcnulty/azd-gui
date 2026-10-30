//! Native-owned effective requirement data. This is a requested-permission
//! snapshot, never evidence of authentication, consent or provider access.
use crate::{
    error::AppResult,
    models::{
        ConnectionRequirement, ExecutionTarget, PermissionCheck, Prerequisite,
        ProjectTemplateSnapshot,
    },
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, HashMap};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RequirementSet {
    pub schema_version: String,
    pub contract_digest: String,
    pub environment: String,
    pub target: ExecutionTarget,
    pub controller_values: BTreeMap<String, String>,
    pub minimum_azd_version: Option<String>,
    pub prerequisites: Vec<Prerequisite>,
    pub connections: Vec<ConnectionRequirement>,
    pub permission_checks: Vec<PermissionCheck>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RequirementSnapshot {
    pub digest: String,
    pub requirements: RequirementSet,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RequirementSelection {
    pub reviewed_contract_digest: Option<String>,
    pub project_path: Option<String>,
    pub environment: String,
    pub environment_values: Option<HashMap<String, String>>,
    pub target: Option<ExecutionTarget>,
}

impl RequirementSnapshot {
    pub async fn ensure_ready(&self) -> AppResult<()> {
        crate::tools::ensure_requirements(&self.requirements.prerequisites).await?;
        if self
            .requirements
            .connections
            .iter()
            .any(|connection| connection.required)
        {
            let request = self.permission_request()?;
            let target = crate::models::ConnectionTarget {
                tenant_id: request.tenant_id,
                subscription_id: request.subscription_id,
            };
            let evidence = crate::connections::check_all(Some(&target)).await;
            self.validate_connections(&evidence)?;
        }
        if !self.requirements.permission_checks.is_empty() {
            let results = crate::permissions::run(self.permission_request()?).await?;
            let blocking = results
                .iter()
                .filter(|result| result.blocking)
                .map(|result| result.name.as_str())
                .collect::<Vec<_>>();
            if !blocking.is_empty() {
                return Err(crate::error::AppError::Invalid(format!("Required permission checks failed: {}. Run preflight again for the current target and choices.",blocking.join(", "))));
            }
        }
        Ok(())
    }

    fn validate_connections(&self, evidence: &[crate::models::ConnectionStatus]) -> AppResult<()> {
        for required in self
            .requirements
            .connections
            .iter()
            .filter(|connection| connection.required)
        {
            let status = evidence
                .iter()
                .find(|status| status.id == required.id)
                .ok_or_else(|| {
                    crate::error::AppError::Invalid(format!(
                        "Required connection {} has no fresh evidence.",
                        required.name
                    ))
                })?;
            let target = &self.requirements.target;
            let same = |left: Option<&str>, right: Option<&str>| {
                left.zip(right)
                    .is_some_and(|(a, b)| a.eq_ignore_ascii_case(b))
            };
            if status.state != "connected"
                || status.wrong_active_account
                || !same(status.tenant_id.as_deref(), target.tenant_id.as_deref())
                || (matches!(required.id.as_str(), "azureCli" | "azPowerShell")
                    && !same(
                        status.subscription_id.as_deref(),
                        target.subscription_id.as_deref(),
                    ))
                || (required.id == "graphPowerShell"
                    && required.scopes.iter().any(|scope| {
                        !status
                            .granted_scopes
                            .iter()
                            .any(|granted| scope.eq_ignore_ascii_case(granted))
                    }))
            {
                return Err(crate::error::AppError::Invalid(format!("Required connection {} could not be freshly verified for the selected target and requested scopes. Review provider evidence before retrying; a cached login or reconnect alone may not provide fresh scope evidence.",required.name)));
            }
        }
        Ok(())
    }

    pub fn permission_request(&self) -> AppResult<crate::models::PermissionCheckRequest> {
        let target = &self.requirements.target;
        let tenant_id = target.tenant_id.clone().ok_or_else(|| {
            crate::error::AppError::Invalid("Select a tenant before checking permissions.".into())
        })?;
        let subscription_id = target.subscription_id.clone().ok_or_else(|| {
            crate::error::AppError::Invalid(
                "Select a subscription before checking permissions.".into(),
            )
        })?;
        Ok(crate::models::PermissionCheckRequest {
            checks: self.requirements.permission_checks.clone(),
            tenant_id,
            subscription_id,
            required_graph_scopes: self
                .requirements
                .connections
                .iter()
                .filter(|connection| connection.id == "graphPowerShell")
                .flat_map(|connection| connection.scopes.clone())
                .collect(),
        })
    }
}

pub fn resolve(
    contract: &ProjectTemplateSnapshot,
    supplied: &HashMap<String, String>,
    environment: &str,
    target: ExecutionTarget,
) -> AppResult<RequirementSnapshot> {
    contract.validate_binding(
        Some(&contract.entry.id),
        (contract.entry.commit != "local").then_some(contract.entry.commit.as_str()),
    )?;
    crate::deployment::validate_environment_name(environment)?;
    let mut data = RequirementSet {
        schema_version: "1.0".into(),
        contract_digest: contract.digest.clone(),
        environment: environment.into(),
        target: crate::execution_approval::validate_target(target)?,
        controller_values: BTreeMap::new(),
        minimum_azd_version: None,
        prerequisites: vec![],
        connections: vec![],
        permission_checks: vec![],
    };
    if let Some(manifest) = &contract.entry.manifest {
        data.controller_values =
            crate::manifest::normalized_requirement_values(manifest, supplied)?;
        data.minimum_azd_version = manifest.minimum_azd_version.clone();
        data.prerequisites = crate::tools::effective_prerequisites(
            crate::manifest::effective_prerequisites(manifest, supplied)?,
            manifest.minimum_azd_version.as_deref(),
        );
        data.connections = crate::manifest::effective_connections(manifest, supplied)?;
        data.permission_checks = crate::manifest::effective_permission_checks(manifest, supplied)?;
    } else if !supplied.is_empty() {
        return Err(crate::error::AppError::Invalid(
            "This contract has no requirement controllers.".into(),
        ));
    }
    data.prerequisites.sort_by(|a, b| a.id.cmp(&b.id));
    data.connections.sort_by(|a, b| a.id.cmp(&b.id));
    data.permission_checks.sort_by(|a, b| a.id.cmp(&b.id));
    let graph_scopes = data
        .connections
        .iter()
        .filter(|connection| connection.id == "graphPowerShell")
        .flat_map(|connection| &connection.scopes)
        .map(|scope| scope.to_ascii_lowercase())
        .collect::<std::collections::HashSet<_>>();
    if graph_scopes.len() > 64 {
        return Err(crate::error::AppError::Invalid(
            "Effective Graph scope union exceeds the supported 64-scope limit.".into(),
        ));
    }
    let digest = format!("{:x}", Sha256::digest(serde_json::to_vec(&data)?));
    Ok(RequirementSnapshot {
        digest,
        requirements: data,
    })
}

#[cfg(test)]
pub fn saved_values(
    manifest: &crate::models::TemplateManifest,
    project: &std::path::Path,
    environment: &str,
) -> AppResult<HashMap<String, String>> {
    let variables = crate::manifest::requirement_controller_variables(manifest)?;
    if variables.is_empty() {
        return Ok(HashMap::new());
    }
    let wanted = variables.iter().map(String::as_str).collect::<Vec<_>>();
    crate::evidence::read_environment_values(project, environment, &wanted).ok_or_else(|| {
        crate::error::AppError::Invalid(
            "The exact saved environment could not be read safely for requirement review.".into(),
        )
    })
}

/// Read target and declared controllers from the same bounded .env contents.
/// Reading them separately could bind a combination that was never saved.
pub fn saved_context(
    manifest: Option<&crate::models::TemplateManifest>,
    project: &std::path::Path,
    environment: &str,
) -> AppResult<(HashMap<String, String>, ExecutionTarget)> {
    let controllers = manifest
        .map(crate::manifest::requirement_controller_variables)
        .transpose()?
        .unwrap_or_default();
    let mut wanted = controllers.iter().map(String::as_str).collect::<Vec<_>>();
    wanted.extend([
        "AZD_GUI_AZURE_CLOUD",
        "AZURE_TENANT_ID",
        "AZURE_SUBSCRIPTION_ID",
        "AZURE_LOCATION",
        "AZURE_RESOURCE_GROUP",
    ]);
    let mut values = crate::evidence::read_environment_values(project, environment, &wanted)
        .ok_or_else(|| {
            crate::error::AppError::Invalid(
                "The exact saved environment could not be read safely for requirement review."
                    .into(),
            )
        })?;
    if values
        .get("AZD_GUI_AZURE_CLOUD")
        .is_some_and(|cloud| cloud != "AzureCloud")
    {
        return Err(crate::error::AppError::Invalid(
            "The selected environment is not configured for AzureCloud.".into(),
        ));
    }
    let target = crate::execution_approval::validate_target(ExecutionTarget {
        tenant_id: values.get("AZURE_TENANT_ID").cloned(),
        subscription_id: values.get("AZURE_SUBSCRIPTION_ID").cloned(),
        location: values.get("AZURE_LOCATION").cloned(),
        resource_group: values.get("AZURE_RESOURCE_GROUP").cloned(),
    })?;
    if target.tenant_id.is_none() || target.subscription_id.is_none() {
        return Err(crate::error::AppError::Invalid("The selected environment needs explicit AZURE_TENANT_ID and AZURE_SUBSCRIPTION_ID before requirement review.".into()));
    }
    values.retain(|key, _| controllers.contains(key));
    Ok((values, target))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn entry() -> crate::models::CatalogEntry {
        let mut entry = crate::catalog::entry_by_id("myworkid").unwrap();
        entry.manifest = Some(
            serde_json::from_str(include_str!(
                "../../src/lib/fixtures/conditional-requirements-v1.1.json"
            ))
            .unwrap(),
        );
        entry
    }

    fn target() -> ExecutionTarget {
        ExecutionTarget {
            tenant_id: Some("11111111-1111-4111-8111-111111111111".into()),
            subscription_id: Some("22222222-2222-4222-8222-222222222222".into()),
            location: Some("westus2".into()),
            resource_group: None,
        }
    }

    #[test]
    fn one_snapshot_selects_all_requirement_surfaces_and_omits_inactive_scopes() {
        let contract = ProjectTemplateSnapshot::new(entry()).unwrap();
        let plan = resolve(&contract, &HashMap::new(), "dev", target()).unwrap();
        assert_eq!(plan.requirements.prerequisites.len(), 1);
        assert_eq!(plan.requirements.connections.len(), 1);
        assert_eq!(plan.requirements.permission_checks.len(), 1);
        assert_eq!(
            plan.requirements.connections[0].scopes,
            ["Directory.Read.All"]
        );
        assert!(plan.requirements.connections[0].scope_groups.is_none());
        assert_eq!(plan.requirements.controller_values["MODE"], "plan");
        assert!(!serde_json::to_string(&plan)
            .unwrap()
            .contains("Policy.ReadWrite"));

        let values = HashMap::from([
            ("INCLUDE_AZURE".into(), "true".into()),
            ("MODE".into(), "enforced".into()),
        ]);
        let enforced = resolve(&contract, &values, "dev", target()).unwrap();
        assert_ne!(plan.digest, enforced.digest);
        assert_eq!(enforced.requirements.prerequisites.len(), 2);
        assert_eq!(enforced.requirements.connections.len(), 2);
        assert_eq!(enforced.requirements.permission_checks.len(), 2);
        assert_eq!(
            enforced
                .requirements
                .connections
                .iter()
                .find(|c| c.id == "graphPowerShell")
                .unwrap()
                .scopes,
            ["Directory.Read.All", "Policy.ReadWrite.ConditionalAccess"]
        );
    }

    #[test]
    fn canonical_defaults_explicit_values_and_map_order_have_the_same_digest() {
        let contract = ProjectTemplateSnapshot::new(entry()).unwrap();
        let implicit = resolve(&contract, &HashMap::new(), "dev", target()).unwrap();
        let values = HashMap::from([
            ("MODE".into(), "plan".into()),
            ("INCLUDE_AZURE".into(), "false".into()),
        ]);
        assert_eq!(
            implicit,
            resolve(&contract, &values, "dev", target()).unwrap()
        );
        let roundtrip: RequirementSnapshot =
            serde_json::from_str(&serde_json::to_string(&implicit).unwrap()).unwrap();
        assert_eq!(implicit, roundtrip);
    }

    #[test]
    fn snapshot_includes_the_implicit_minimum_azd_tool_once() {
        let mut entry = entry();
        let manifest = entry.manifest.as_mut().unwrap();
        manifest.prerequisites.clear();
        manifest.minimum_azd_version = Some("1.20.0".into());
        let contract = ProjectTemplateSnapshot::new(entry).unwrap();
        let snapshot = resolve(&contract, &HashMap::new(), "dev", target()).unwrap();
        assert_eq!(snapshot.requirements.prerequisites.len(), 1);
        assert_eq!(snapshot.requirements.prerequisites[0].id, "azd");
        assert_eq!(
            snapshot.requirements.prerequisites[0]
                .minimum_version
                .as_deref(),
            Some(">=1.20.0")
        );
        assert!(snapshot.requirements.prerequisites[0].required);
    }

    #[test]
    fn requested_graph_scopes_cannot_substitute_for_fresh_provider_evidence() {
        let contract = ProjectTemplateSnapshot::new(entry()).unwrap();
        let snapshot = resolve(&contract, &HashMap::new(), "dev", target()).unwrap();
        let mut status:crate::models::ConnectionStatus=serde_json::from_value(json!({
            "id":"graphPowerShell","name":"Graph","required":true,"reason":"Test","state":"connected",
            "tenantId":target().tenant_id,"scopes":["Directory.Read.All"],"grantedScopes":[]
        })).unwrap();
        assert!(snapshot.validate_connections(&[status.clone()]).is_err());
        status.granted_scopes = vec!["directory.read.all".into()];
        snapshot.validate_connections(&[status.clone()]).unwrap();
        status.tenant_id = Some("33333333-3333-4333-8333-333333333333".into());
        assert!(snapshot.validate_connections(&[status]).is_err());
        assert!(snapshot.validate_connections(&[]).is_err());
    }

    #[test]
    fn saved_context_rejects_missing_or_invalid_target_cloud_and_environment() {
        let root = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(root.path().join(".azure/dev")).unwrap();
        let fixture = entry();
        let manifest = fixture.manifest.as_ref();
        let file = root.path().join(".azure/dev/.env");
        let good="MODE=plan\nAZURE_TENANT_ID=11111111-1111-4111-8111-111111111111\nAZURE_SUBSCRIPTION_ID=22222222-2222-4222-8222-222222222222\nSECRET=must-not-return\n";
        std::fs::write(&file, good).unwrap();
        let (values, target) = saved_context(manifest, root.path(), "dev").unwrap();
        assert_eq!(values.len(), 1);
        assert_eq!(values["MODE"], "plan");
        assert!(target.tenant_id.is_some());
        for content in [
            "MODE=plan".to_string(),
            good.replace("11111111-1111-4111-8111-111111111111", "bad"),
            format!("{good}AZD_GUI_AZURE_CLOUD=AzureUSGovernment\n"),
        ] {
            std::fs::write(&file, content).unwrap();
            assert!(saved_context(manifest, root.path(), "dev").is_err());
        }
        assert!(saved_context(manifest, root.path(), "../dev").is_err());
        assert!(saved_context(manifest, root.path(), "missing").is_err());
    }

    #[test]
    fn contract_target_environment_and_both_input_directions_change_the_digest() {
        let contract = ProjectTemplateSnapshot::new(entry()).unwrap();
        let initial = resolve(&contract, &HashMap::new(), "dev", target()).unwrap();
        let enabled = HashMap::from([("INCLUDE_AZURE".into(), "true".into())]);
        assert_ne!(
            initial.digest,
            resolve(&contract, &enabled, "dev", target())
                .unwrap()
                .digest
        );
        assert_ne!(
            initial.digest,
            resolve(&contract, &HashMap::new(), "other", target())
                .unwrap()
                .digest
        );
        let mut other_target = target();
        other_target.tenant_id = Some("33333333-3333-4333-8333-333333333333".into());
        assert_ne!(
            initial.digest,
            resolve(&contract, &HashMap::new(), "dev", other_target)
                .unwrap()
                .digest
        );
        let mut changed = entry();
        changed.description.push_str(" changed");
        let changed = ProjectTemplateSnapshot::new(changed).unwrap();
        assert_ne!(
            initial.digest,
            resolve(&changed, &HashMap::new(), "dev", target())
                .unwrap()
                .digest
        );
        let mut forged = contract;
        forged.entry.description.push_str(" forged");
        assert!(resolve(&forged, &HashMap::new(), "dev", target()).is_err());
    }

    #[test]
    fn scope_group_only_controllers_are_whitelisted_and_invalid_values_fail_closed() {
        let entry = entry();
        let manifest = entry.manifest.as_ref().unwrap();
        assert_eq!(
            crate::manifest::requirement_controller_variables(manifest).unwrap(),
            ["INCLUDE_AZURE", "MODE"]
        );
        let contract = ProjectTemplateSnapshot::new(entry).unwrap();
        for (key, value) in [
            ("MODE", "unknown"),
            ("MODE", ""),
            ("INCLUDE_AZURE", "FALSE"),
            ("UNDECLARED", "true"),
        ] {
            assert!(resolve(
                &contract,
                &HashMap::from([(key.into(), value.into())]),
                "dev",
                target()
            )
            .is_err());
        }
    }

    #[test]
    fn new_scope_and_select_contracts_require_a_version_old_clients_reject() {
        let mut entry = entry();
        entry.manifest.as_mut().unwrap().schema_version = "1.0".into();
        assert!(ProjectTemplateSnapshot::new(entry.clone()).is_err());
        // A select controller remains new even after its scope group is removed.
        let manifest = entry.manifest.as_mut().unwrap();
        manifest.connections[0].scope_groups = None;
        manifest.prerequisites[1].condition.as_mut().unwrap().field = "mode".into();
        manifest.prerequisites[1].condition.as_mut().unwrap().equals = Some(json!("plan"));
        assert!(ProjectTemplateSnapshot::new(entry).is_err());
    }

    #[test]
    fn legacy_snapshots_do_not_serialize_new_empty_scope_fields() {
        let old = crate::catalog::entry_by_id("myworkid").unwrap();
        let original = ProjectTemplateSnapshot::new(old).unwrap();
        let bytes = serde_json::to_string(&original).unwrap();
        assert!(!bytes.contains("scopeGroups"));
        let restored: ProjectTemplateSnapshot = serde_json::from_str(&bytes).unwrap();
        restored
            .validate_binding(Some(&original.entry.id), Some(&original.entry.commit))
            .unwrap();
        assert_eq!(original.digest, restored.digest);
    }

    #[test]
    fn frozen_legacy_snapshot_keeps_its_pre_scope_group_digest() {
        let restored: ProjectTemplateSnapshot = serde_json::from_str(include_str!(
            "../../src/lib/fixtures/legacy-requirement-snapshot-v1.0.json"
        ))
        .unwrap();
        assert_eq!(
            restored.digest,
            "14fbf83bfc4dd6f9bf507f272760987bb0081ce1058f8d6e478f585be948ad0d"
        );
        restored
            .validate_binding(Some("legacy-requirement-fixture"), None)
            .unwrap();
        assert!(restored.entry.manifest.as_ref().unwrap().connections[0]
            .scope_groups
            .is_none());
    }

    #[test]
    fn rejects_sensitive_conditional_and_ambiguous_select_controllers() {
        for kind in ["sensitive", "conditional", "duplicate"] {
            let mut entry = entry();
            let field = &mut entry.manifest.as_mut().unwrap().configuration.groups[0].fields[1];
            match kind {
                "sensitive" => field.sensitive = true,
                "conditional" => {
                    field.condition = Some(
                        serde_json::from_value(json!({"field":"includeAzure","equals":true}))
                            .unwrap(),
                    )
                }
                _ => field.options.push(field.options[0].clone()),
            }
            assert!(ProjectTemplateSnapshot::new(entry).is_err());
        }
    }

    #[test]
    fn saved_environment_reads_only_declared_controller_values() {
        let root = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(root.path().join(".azure/dev")).unwrap();
        std::fs::write(
            root.path().join(".azure/dev/.env"),
            "MODE=\"enforced\"\nINCLUDE_AZURE=\"false\"\nSECRET=\"do-not-read\"\n",
        )
        .unwrap();
        let entry = entry();
        let manifest = entry.manifest.as_ref().unwrap();
        let values = saved_values(manifest, root.path(), "dev").unwrap();
        assert_eq!(values.len(), 2);
        assert!(!values.contains_key("SECRET"));
        assert_eq!(values["MODE"], "enforced");
        assert!(saved_values(manifest, root.path(), "missing").is_err());
    }

    #[test]
    fn rejects_ambiguous_provider_rows_and_bounds_the_active_scope_union() {
        let mut duplicate = entry();
        let manifest = duplicate.manifest.as_mut().unwrap();
        manifest.connections.push(manifest.connections[0].clone());
        assert!(ProjectTemplateSnapshot::new(duplicate).is_err());
        let mut large = entry();
        let graph = &mut large.manifest.as_mut().unwrap().connections[0];
        graph.scopes = (0..64).map(|index| format!("Scope{index}.Read")).collect();
        let contract = ProjectTemplateSnapshot::new(large).unwrap();
        assert_eq!(
            resolve(&contract, &HashMap::new(), "dev", target())
                .unwrap()
                .requirements
                .connections[0]
                .scopes
                .len(),
            64
        );
        assert!(resolve(
            &contract,
            &HashMap::from([("MODE".into(), "enforced".into())]),
            "dev",
            target()
        )
        .is_err());
    }

    #[test]
    fn graph_scope_union_deduplicates_case_variants_and_legacy_rejects_empty_groups() {
        let mut entry = entry();
        entry.manifest.as_mut().unwrap().connections[0]
            .scope_groups
            .as_mut()
            .unwrap()[0]
            .scopes = vec!["directory.read.all".into()];
        let contract = ProjectTemplateSnapshot::new(entry).unwrap();
        let enabled = resolve(
            &contract,
            &HashMap::from([("MODE".into(), "enforced".into())]),
            "dev",
            target(),
        )
        .unwrap();
        assert_eq!(enabled.requirements.connections[0].scopes.len(), 1);
        let mut legacy = crate::catalog::entry_by_id("myworkid").unwrap();
        legacy.manifest.as_mut().unwrap().connections[0].scope_groups = Some(vec![]);
        assert!(ProjectTemplateSnapshot::new(legacy).is_err());
    }
}
