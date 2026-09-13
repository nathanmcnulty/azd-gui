use crate::{
    error::{AppError, AppResult},
    models::{
        CatalogEntry, ConfigurationField, FieldCondition, ProjectTemplateSnapshot, TemplateManifest,
    },
    permissions,
};
use sha2::{Digest, Sha256};
use std::collections::{HashMap, HashSet};

const MAX_SNAPSHOT_BYTES: usize = 600_000;

pub(crate) fn is_standard_environment_variable(name: &str) -> bool {
    matches!(
        name,
        "AZURE_ENV_NAME"
            | "AZURE_TENANT_ID"
            | "AZURE_SUBSCRIPTION_ID"
            | "AZURE_LOCATION"
            | "AZURE_RESOURCE_GROUP"
    )
}

pub(crate) fn validate(manifest: &TemplateManifest) -> AppResult<()> {
    if manifest.schema_version != "1.0" {
        return Err(AppError::Invalid(
            "Unsupported template manifest version.".into(),
        ));
    }
    permissions::validate(&manifest.permission_checks)?;
    let mut groups = HashSet::new();
    let mut fields = HashMap::new();
    let mut variables = HashSet::new();
    for group in &manifest.configuration.groups {
        if group.id.is_empty() || !groups.insert(group.id.as_str()) {
            return Err(AppError::Invalid(
                "Template group IDs must be nonempty and unique.".into(),
            ));
        }
        for field in &group.fields {
            let reserved_id = [
                "sourceMode",
                "sourcePath",
                "projectPath",
                "environmentName",
                "tenantId",
                "subscriptionId",
                "location",
                "resourceGroup",
            ]
            .iter()
            .any(|id| field.id.eq_ignore_ascii_case(id));
            if reserved_id
                || field.id.len() > 128
                || !field.id.starts_with(|c: char| c.is_ascii_alphabetic())
                || !field
                    .id
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
            {
                return Err(AppError::Invalid(
                    "Template field IDs cannot replace wizard target or source fields.".into(),
                ));
            }
            if field.id.is_empty()
                || fields.insert(field.id.as_str(), field).is_some()
                || !variables.insert(field.environment_variable.as_str())
            {
                return Err(AppError::Invalid(
                    "Template field IDs and environment variables must be unique.".into(),
                ));
            }
            if is_standard_environment_variable(&field.environment_variable)
                || field.environment_variable.len() > 128
                || !field
                    .environment_variable
                    .starts_with(|c: char| c.is_ascii_uppercase())
                || !field
                    .environment_variable
                    .chars()
                    .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit() || c == '_')
            {
                return Err(AppError::Invalid(
                    "Template fields must declare valid non-reserved environment variables.".into(),
                ));
            }
            if !matches!(
                field.field_type.as_str(),
                "text"
                    | "secret"
                    | "boolean"
                    | "select"
                    | "multiselect"
                    | "tenant"
                    | "subscription"
                    | "location"
                    | "resourceGroup"
                    | "entraObject"
            ) {
                return Err(AppError::Invalid("Unsupported template field type.".into()));
            }
            let mut options = HashSet::new();
            if field
                .options
                .iter()
                .any(|option| !options.insert(option.value.as_str()))
            {
                return Err(AppError::Invalid(
                    "Template option values must be unique.".into(),
                ));
            }
            if field.field_type == "multiselect"
                && field
                    .options
                    .iter()
                    .any(|option| option.value.contains(','))
            {
                return Err(AppError::Invalid(
                    "Multiselect values cannot contain commas.".into(),
                ));
            }
        }
    }
    for group in &manifest.configuration.groups {
        validate_condition(group.condition.as_ref(), &fields)?;
        for field in &group.fields {
            validate_condition(field.condition.as_ref(), &fields)?;
        }
    }
    let mut profiles = HashSet::new();
    for profile in &manifest.recommendation_profiles {
        if profile.id.is_empty()
            || profile.id.len() > 128
            || !profile.id.starts_with(|c: char| c.is_ascii_alphabetic())
            || !profile
                .id
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
            || profile.title.trim().is_empty()
            || profile.description.trim().is_empty()
            || !profiles.insert(profile.id.as_str())
        {
            return Err(AppError::Invalid(
                "Recommendation profile IDs and descriptions must be nonempty and unique.".into(),
            ));
        }
        for (field_id, value) in &profile.values {
            let field = fields.get(field_id.as_str()).ok_or_else(|| {
                AppError::Invalid("Recommendation profiles must reference declared fields.".into())
            })?;
            validate_recommendation_value(field, value)?;
        }
    }
    Ok(())
}

fn validate_recommendation_value(
    field: &ConfigurationField,
    value: &serde_json::Value,
) -> AppResult<()> {
    if field.sensitive || field.field_type == "secret" {
        return Err(AppError::Invalid(
            "Recommendation profiles cannot set sensitive fields.".into(),
        ));
    }
    let valid = match field.field_type.as_str() {
        "boolean" => value.is_boolean(),
        "multiselect" => value.as_array().is_some_and(|values| {
            let options = field
                .options
                .iter()
                .map(|option| option.value.as_str())
                .collect::<HashSet<_>>();
            values
                .iter()
                .all(|item| item.as_str().is_some_and(|item| options.contains(item)))
        }),
        "select" => value
            .as_str()
            .is_some_and(|item| field.options.iter().any(|option| option.value == item)),
        "tenant" | "subscription" | "entraObject" => value.as_str().is_some_and(is_uuid),
        "location" => value.as_str().is_some_and(|item| {
            !item.is_empty() && item.len() <= 64 && item.chars().all(|c| c.is_ascii_alphanumeric())
        }),
        "resourceGroup" => value.as_str().is_some_and(|item| {
            !item.is_empty()
                && item.len() <= 90
                && !item.ends_with('.')
                && item
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '(' | ')' | '-'))
        }),
        "text" => value.is_string(),
        _ => false,
    };
    if valid {
        Ok(())
    } else {
        Err(AppError::Invalid(format!(
            "Recommendation profile value for '{}' does not match its field type.",
            field.id
        )))
    }
}

fn is_uuid(value: &str) -> bool {
    let groups = [8, 4, 4, 4, 12];
    let mut parts = value.split('-');
    groups.iter().all(|length| {
        parts.next().is_some_and(|part| {
            part.len() == *length && part.chars().all(|c| c.is_ascii_hexdigit())
        })
    }) && parts.next().is_none()
        && value
            .chars()
            .any(|character| character != '-' && character != '0')
}

fn validate_condition(
    condition: Option<&FieldCondition>,
    fields: &HashMap<&str, &ConfigurationField>,
) -> AppResult<()> {
    let Some(condition) = condition else {
        return Ok(());
    };
    let field = fields
        .get(condition.field.as_str())
        .ok_or_else(|| AppError::Invalid("A condition references an undeclared field.".into()))?;
    if field.sensitive || field.field_type == "secret" {
        return Err(AppError::Invalid(
            "Conditions cannot depend on sensitive fields.".into(),
        ));
    }
    if usize::from(condition.equals.is_some())
        + usize::from(condition.not_equals.is_some())
        + usize::from(condition.includes.is_some())
        != 1
    {
        return Err(AppError::Invalid(
            "Conditions must specify exactly one comparison.".into(),
        ));
    }
    if condition.includes.is_some() && field.field_type != "multiselect" {
        return Err(AppError::Invalid(
            "Includes conditions require a multiselect field.".into(),
        ));
    }
    Ok(())
}

fn digest(entry: &CatalogEntry) -> AppResult<String> {
    let bytes = serde_json::to_vec(entry)?;
    if bytes.len() > MAX_SNAPSHOT_BYTES {
        return Err(AppError::Invalid(
            "Template snapshot exceeds its size limit.".into(),
        ));
    }
    Ok(format!("{:x}", Sha256::digest(bytes)))
}

impl ProjectTemplateSnapshot {
    pub(crate) fn new(mut entry: CatalogEntry) -> AppResult<Self> {
        if let Some(manifest) = &mut entry.manifest {
            validate(manifest)?;
            // Even template-authored sensitive defaults do not belong in project state.
            for field in manifest
                .configuration
                .groups
                .iter_mut()
                .flat_map(|group| &mut group.fields)
            {
                if field.sensitive || field.field_type == "secret" {
                    field.default = None;
                }
            }
        }
        Ok(Self {
            digest: digest(&entry)?,
            entry,
        })
    }

    pub(crate) fn validate_binding(
        &self,
        template_id: Option<&str>,
        revision: Option<&str>,
    ) -> AppResult<()> {
        let expected_revision =
            (self.entry.commit != "local").then_some(self.entry.commit.as_str());
        if template_id != Some(self.entry.id.as_str())
            || revision != expected_revision
            || self.digest != digest(&self.entry)?
        {
            return Err(AppError::Invalid("The saved template contract does not match this project. Inspect the source again.".into()));
        }
        if let Some(manifest) = &self.entry.manifest {
            validate(manifest)?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn condition_roundtrip_preserves_only_the_declared_comparison() {
        let value = json!({"field":"components", "includes":"worker"});
        let condition: FieldCondition = serde_json::from_value(value.clone()).unwrap();
        assert_eq!(serde_json::to_value(condition).unwrap(), value);
    }

    #[test]
    fn validates_all_bundled_contracts() {
        for entry in crate::catalog::bundled_catalog().unwrap().entries {
            validate(entry.manifest.as_ref().unwrap()).unwrap();
        }
    }

    #[test]
    fn rejects_ambiguous_fields_and_sensitive_conditions() {
        let mut manifest = crate::catalog::entry_by_id("maester-automation")
            .unwrap()
            .manifest
            .unwrap();
        manifest.configuration.groups[0].fields[0].sensitive = true;
        assert!(validate(&manifest).is_err());
        manifest.configuration.groups[0].fields[0].sensitive = false;
        let duplicate = manifest.configuration.groups[0].fields[0].clone();
        manifest.configuration.groups[0].fields.push(duplicate);
        assert!(validate(&manifest).is_err());
    }

    #[test]
    fn snapshot_strips_sensitive_defaults_and_rejects_changed_contract_or_revision() {
        let mut entry = crate::catalog::entry_by_id("myworkid").unwrap();
        let field = &mut entry.manifest.as_mut().unwrap().configuration.groups[2].fields[0];
        field.sensitive = true;
        field.default = Some(json!("never-persist-this"));
        let snapshot = ProjectTemplateSnapshot::new(entry).unwrap();
        assert!(!serde_json::to_string(&snapshot)
            .unwrap()
            .contains("never-persist-this"));
        snapshot
            .validate_binding(Some(&snapshot.entry.id), Some(&snapshot.entry.commit))
            .unwrap();
        assert!(snapshot
            .validate_binding(Some(&snapshot.entry.id), Some("changed"))
            .is_err());
        let mut altered = snapshot.clone();
        altered
            .entry
            .manifest
            .as_mut()
            .unwrap()
            .configuration
            .groups[0]
            .fields[0]
            .required = false;
        assert!(altered
            .validate_binding(Some(&snapshot.entry.id), Some(&snapshot.entry.commit))
            .is_err());
    }

    #[test]
    fn recommendation_profiles_only_set_declared_non_sensitive_values() {
        let mut manifest = crate::catalog::entry_by_id("maester-automation")
            .unwrap()
            .manifest
            .unwrap();
        validate(&manifest).unwrap();

        manifest.recommendation_profiles[0]
            .values
            .insert("unknownField".into(), serde_json::json!(true));
        assert!(validate(&manifest).is_err());

        manifest.recommendation_profiles[0]
            .values
            .remove("unknownField");
        manifest.recommendation_profiles[0]
            .values
            .insert("permissionProfile".into(), serde_json::json!(true));
        assert!(validate(&manifest).is_err());

        manifest.recommendation_profiles[0]
            .values
            .insert("permissionProfile".into(), serde_json::json!("Unsupported"));
        assert!(validate(&manifest).is_err());

        manifest.recommendation_profiles[0]
            .values
            .insert("permissionProfile".into(), serde_json::json!("Minimal"));
        manifest.configuration.groups[2].fields[0].sensitive = true;
        assert!(validate(&manifest).is_err());
    }

    #[test]
    fn profiles_preserve_snapshot_digests_across_reopen_and_legacy_absence() {
        let entry = crate::catalog::entry_by_id("maester-automation").unwrap();
        let snapshot = ProjectTemplateSnapshot::new(entry).unwrap();
        for _ in 0..8 {
            let reopened: ProjectTemplateSnapshot =
                serde_json::from_str(&serde_json::to_string(&snapshot).unwrap()).unwrap();
            reopened
                .validate_binding(Some(&snapshot.entry.id), Some(&snapshot.entry.commit))
                .unwrap();
        }
        let mut legacy = snapshot.entry;
        legacy
            .manifest
            .as_mut()
            .unwrap()
            .recommendation_profiles
            .clear();
        assert!(!serde_json::to_string(&legacy)
            .unwrap()
            .contains("recommendationProfiles"));
        let legacy = ProjectTemplateSnapshot::new(legacy).unwrap();
        let reopened: ProjectTemplateSnapshot =
            serde_json::from_str(&serde_json::to_string(&legacy).unwrap()).unwrap();
        reopened
            .validate_binding(Some(&legacy.entry.id), Some(&legacy.entry.commit))
            .unwrap();
    }
}
