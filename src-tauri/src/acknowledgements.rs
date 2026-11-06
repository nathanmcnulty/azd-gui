//! Required-true acknowledgements are fresh, target-bound execution evidence.
//! A persisted true value is configuration, never sufficient approval on its own.
use crate::{
    error::{AppError, AppResult},
    models::{AcknowledgementProof, ConfigurationField, FieldCondition, TemplateManifest},
};
use serde_json::Value;
use std::collections::{BTreeMap, HashMap, HashSet};

fn invalid(message: impl Into<String>) -> AppError {
    AppError::Invalid(message.into())
}

pub(crate) fn has_acknowledgements(manifest: &TemplateManifest) -> bool {
    manifest
        .configuration
        .groups
        .iter()
        .flat_map(|group| &group.fields)
        .any(|field| field.field_type == "acknowledgement")
}

fn controller<'a>(
    manifest: &'a TemplateManifest,
    condition: &FieldCondition,
) -> AppResult<&'a ConfigurationField> {
    for group in &manifest.configuration.groups {
        if let Some(field) = group
            .fields
            .iter()
            .find(|field| field.id == condition.field)
        {
            if group.condition.is_some()
                || field.condition.is_some()
                || field.sensitive
                || matches!(field.field_type.as_str(), "secret" | "acknowledgement")
            {
                return Err(invalid("Acknowledgements must depend on unconditional, non-sensitive controller fields."));
            }
            return Ok(field);
        }
    }
    Err(invalid(
        "Acknowledgements must depend on declared controller fields.",
    ))
}

fn typed_value(field: &ConfigurationField, value: &Value) -> AppResult<()> {
    let valid = match field.field_type.as_str() {
        "boolean" => value.is_boolean(),
        "multiselect" => value.as_array().is_some_and(|values| {
            values.iter().all(|value| {
                value
                    .as_str()
                    .is_some_and(|value| field.options.iter().any(|option| option.value == value))
            })
        }),
        "select" => value.as_str().is_some_and(|value| {
            if value.is_empty() {
                !field.required
            } else {
                field.options.iter().any(|option| option.value == value)
            }
        }),
        "text" | "tenant" | "subscription" | "location" | "resourceGroup" | "entraObject" => {
            value.is_string()
        }
        _ => false,
    };
    if valid {
        Ok(())
    } else {
        Err(invalid(format!(
            "Acknowledgement controller '{}' has an invalid value type or option.",
            field.id
        )))
    }
}

pub(crate) fn validate_manifest(manifest: &TemplateManifest) -> AppResult<()> {
    for group in &manifest.configuration.groups {
        for field in &group.fields {
            if field.field_type != "acknowledgement" {
                if field.validation_message.is_some() {
                    return Err(invalid(
                        "Only acknowledgement fields may declare validationMessage.",
                    ));
                }
                continue;
            }
            if field.label.is_empty()
                || field
                    .validation_message
                    .as_deref()
                    .is_some_and(str::is_empty)
            {
                return Err(invalid(
                    "Acknowledgement labels and validation messages must be nonempty.",
                ));
            }
            if !field.required
                || field.sensitive
                || field.default.is_some()
                || !field.options.is_empty()
                || field.pattern.is_some()
                || field.placeholder.is_some()
                || field.lookup.is_some()
            {
                return Err(invalid("Acknowledgements must be required, non-sensitive, and have no default, options, pattern, placeholder, or lookup."));
            }
            let condition = field
                .condition
                .as_ref()
                .ok_or_else(|| invalid("Acknowledgement fields must declare a condition."))?;
            for condition in std::iter::once(condition).chain(group.condition.iter()) {
                let control = controller(manifest, condition)?;
                if control.field_type == "multiselect"
                    && control.options.iter().any(|option| option.value.is_empty())
                {
                    return Err(invalid("Multiselect acknowledgement controllers cannot declare empty option values."));
                }
                if condition.includes.as_ref().is_some_and(|value| {
                    !control.options.iter().any(|option| &option.value == value)
                }) {
                    return Err(invalid("Acknowledgement includes conditions must reference a declared multiselect option."));
                }
                if let Some(default) = &control.default {
                    typed_value(control, default)?;
                }
                if let Some(expected) = condition.equals.as_ref().or(condition.not_equals.as_ref())
                {
                    typed_value(control, expected)?;
                }
            }
        }
    }
    for profile in &manifest.recommendation_profiles {
        for field in manifest
            .configuration
            .groups
            .iter()
            .flat_map(|group| &group.fields)
        {
            if field.field_type == "acknowledgement" && profile.values.contains_key(&field.id) {
                return Err(invalid(
                    "Recommendation profiles cannot set acknowledgements.",
                ));
            }
        }
    }
    Ok(())
}

/// Project just the non-sensitive acknowledgement context. Native preparation and
/// lifecycle execution use this same representation in their one-time token.
pub(crate) fn binding_values(
    manifest: &TemplateManifest,
    supplied: &HashMap<String, Value>,
) -> AppResult<HashMap<String, Value>> {
    crate::manifest::validate(manifest)?;
    let mut values = HashMap::new();
    for group in &manifest.configuration.groups {
        for field in group
            .fields
            .iter()
            .filter(|field| field.field_type == "acknowledgement")
        {
            values.insert(
                field.environment_variable.clone(),
                supplied
                    .get(&field.environment_variable)
                    .cloned()
                    .unwrap_or(Value::Null),
            );
            for condition in field.condition.iter().chain(group.condition.iter()) {
                let control = controller(manifest, condition)?;
                let value = supplied
                    .get(&control.environment_variable)
                    .filter(|value| !value.is_null())
                    .or(control.default.as_ref())
                    .cloned()
                    .unwrap_or(Value::Null);
                if !value.is_null() {
                    typed_value(control, &value)?;
                }
                values.insert(control.environment_variable.clone(), value);
            }
        }
    }
    // Hidden acknowledgements are irrelevant to both approval and comparison.
    // Normalize false, stale true, and omitted values to false, also clearing
    // previously saved true values when preparation writes configuration.
    for group in &manifest.configuration.groups {
        for field in group
            .fields
            .iter()
            .filter(|field| field.field_type == "acknowledgement")
        {
            let mut active = true;
            for condition in field.condition.iter().chain(group.condition.iter()) {
                active &= condition_matches(manifest, condition, &values)?;
            }
            if !active {
                values.insert(field.environment_variable.clone(), Value::Bool(false));
            }
        }
    }
    Ok(values)
}

pub(crate) fn saved_binding_values(
    manifest: &TemplateManifest,
    project_path: &std::path::Path,
    environment: &str,
) -> AppResult<HashMap<String, Value>> {
    if !has_acknowledgements(manifest) {
        return Ok(HashMap::new());
    }
    let fields = binding_values(manifest, &HashMap::new())?;
    if fields.is_empty() {
        return Ok(fields);
    }
    let wanted = fields.keys().map(String::as_str).collect::<Vec<_>>();
    let saved = crate::evidence::read_environment_values(project_path, environment, &wanted)
        .ok_or_else(|| {
            invalid(
                "The exact saved environment could not be read safely for acknowledgement review.",
            )
        })?;
    let mut supplied = HashMap::new();
    for field in manifest
        .configuration
        .groups
        .iter()
        .flat_map(|group| &group.fields)
    {
        if !fields.contains_key(&field.environment_variable) {
            continue;
        }
        let Some(raw) = saved.get(&field.environment_variable) else {
            continue;
        };
        let value = match field.field_type.as_str() {
            "boolean" | "acknowledgement" => match raw.as_str() {
                "true" => Value::Bool(true),
                "false" => Value::Bool(false),
                // Keep malformed acknowledgement values as strings: inactive
                // fields are ignored, active fields must be exactly boolean true.
                _ if field.field_type == "acknowledgement" => Value::String(raw.clone()),
                _ => {
                    return Err(invalid(format!(
                        "Saved acknowledgement controller '{}' must be true or false.",
                        field.id
                    )))
                }
            },
            "multiselect" => Value::Array(if raw.is_empty() {
                Vec::new()
            } else {
                raw.split(',')
                    .map(|item| Value::String(item.into()))
                    .collect()
            }),
            _ => Value::String(raw.clone()),
        };
        supplied.insert(field.environment_variable.clone(), value);
    }
    binding_values(manifest, &supplied)
}

fn condition_matches(
    manifest: &TemplateManifest,
    condition: &FieldCondition,
    values: &HashMap<String, Value>,
) -> AppResult<bool> {
    let field = controller(manifest, condition)?;
    let current = values
        .get(&field.environment_variable)
        .unwrap_or(&Value::Null);
    Ok(if let Some(expected) = &condition.equals {
        current == expected
    } else if let Some(expected) = &condition.not_equals {
        current != expected
    } else if let Some(expected) = &condition.includes {
        current
            .as_array()
            .is_some_and(|values| values.iter().any(|value| value.as_str() == Some(expected)))
    } else {
        false
    })
}

pub(crate) fn validate_preparation(
    manifest: &TemplateManifest,
    supplied: &HashMap<String, Value>,
    tenant_id: Option<&str>,
    proofs: &[AcknowledgementProof],
) -> AppResult<()> {
    crate::manifest::validate(manifest)?;
    // Preparation may reuse an existing environment. Missing/null controllers
    // must never omit a write and silently retain older, broader saved scope.
    for group in &manifest.configuration.groups {
        for field in group
            .fields
            .iter()
            .filter(|field| field.field_type == "acknowledgement")
        {
            for condition in field.condition.iter().chain(group.condition.iter()) {
                let control = controller(manifest, condition)?;
                let value = supplied
                    .get(&control.environment_variable)
                    .filter(|value| !value.is_null())
                    .ok_or_else(|| {
                        invalid(format!(
                            "Preparation requires an explicit acknowledgement controller value: {}",
                            control.environment_variable
                        ))
                    })?;
                typed_value(control, value)?;
            }
        }
    }
    validate(manifest, supplied, tenant_id, proofs)
}

pub(crate) fn validate(
    manifest: &TemplateManifest,
    values: &HashMap<String, Value>,
    tenant_id: Option<&str>,
    proofs: &[AcknowledgementProof],
) -> AppResult<()> {
    let values = binding_values(manifest, values)?;
    // A missing saved controller with no declared default is unknown scope, not
    // evidence that an acknowledgement is inactive. Require an explicit value.
    for group in &manifest.configuration.groups {
        for field in group
            .fields
            .iter()
            .filter(|field| field.field_type == "acknowledgement")
        {
            for condition in field.condition.iter().chain(group.condition.iter()) {
                let control = controller(manifest, condition)?;
                if values
                    .get(&control.environment_variable)
                    .map_or(true, Value::is_null)
                {
                    return Err(invalid(format!("Acknowledgement controller value is missing: {}. Save an explicit configuration value before approval.", control.environment_variable)));
                }
            }
        }
    }
    let ack_fields = manifest
        .configuration
        .groups
        .iter()
        .flat_map(|group| &group.fields)
        .filter(|field| field.field_type == "acknowledgement")
        .map(|field| field.id.as_str())
        .collect::<HashSet<_>>();
    let mut seen = HashSet::new();
    for proof in proofs {
        if !ack_fields.contains(proof.field_id.as_str()) || !seen.insert(proof.field_id.as_str()) {
            return Err(invalid(
                "Acknowledgement proofs must reference unique declared acknowledgement fields.",
            ));
        }
    }
    for group in &manifest.configuration.groups {
        for field in group
            .fields
            .iter()
            .filter(|field| field.field_type == "acknowledgement")
        {
            let conditions = field
                .condition
                .iter()
                .chain(group.condition.iter())
                .collect::<Vec<_>>();
            let mut active = true;
            for condition in &conditions {
                active &= condition_matches(manifest, condition, &values)?;
            }
            if !active {
                continue;
            }
            let error = || {
                invalid(field.validation_message.clone().unwrap_or_else(|| {
                    format!("A fresh acknowledgement is required: {}", field.label)
                }))
            };
            if values.get(&field.environment_variable) != Some(&Value::Bool(true)) {
                return Err(error());
            }
            let proof = proofs
                .iter()
                .find(|proof| proof.field_id == field.id)
                .ok_or_else(error)?;
            let tenant = tenant_id
                .and_then(|value| uuid::Uuid::parse_str(value).ok())
                .filter(|value| !value.is_nil())
                .ok_or_else(error)?;
            if uuid::Uuid::parse_str(&proof.tenant_id).ok() != Some(tenant) {
                return Err(error());
            }
            let expected = conditions
                .into_iter()
                .map(|condition| {
                    let control = controller(manifest, condition)?;
                    Ok((
                        control.id.clone(),
                        values
                            .get(&control.environment_variable)
                            .cloned()
                            .unwrap_or(Value::Null),
                    ))
                })
                .collect::<AppResult<BTreeMap<_, _>>>()?;
            if proof.controlling_values != expected {
                return Err(error());
            }
        }
    }
    Ok(())
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use serde_json::json;

    pub(crate) const TENANT: &str = "11111111-1111-1111-1111-111111111111";

    pub(crate) fn fixture_value() -> Value {
        json!({"schemaVersion":"1.0", "configuration":{"groups":[
            {"id":"controllers","title":"Scope", "fields":[
                {"id":"mode","environmentVariable":"MODE","label":"Mode","type":"select","default":"all","options":[{"label":"Limited","value":"limited"},{"label":"All","value":"all"}]},
                {"id":"enabled","environmentVariable":"ENABLED","label":"Enabled","type":"boolean","default":true}
            ]},
            {"id":"review","title":"Review", "condition":{"field":"enabled","equals":true}, "fields":[
                {"id":"confirm","environmentVariable":"CONFIRM_ALL","label":"I understand all users are included","type":"acknowledgement","required":true,"condition":{"field":"mode","equals":"all"},"validationMessage":"Confirm all users first."}
            ]}
        ]}})
    }

    pub(crate) fn fixture_manifest() -> TemplateManifest {
        serde_json::from_value(fixture_value()).unwrap()
    }
    pub(crate) fn values() -> HashMap<String, Value> {
        HashMap::from([
            ("MODE".into(), json!("all")),
            ("ENABLED".into(), json!(true)),
            ("CONFIRM_ALL".into(), json!(true)),
        ])
    }
    pub(crate) fn proof() -> AcknowledgementProof {
        AcknowledgementProof {
            field_id: "confirm".into(),
            tenant_id: TENANT.into(),
            controlling_values: BTreeMap::from([
                ("mode".into(), json!("all")),
                ("enabled".into(), json!(true)),
            ]),
        }
    }
    fn reject_manifest(value: Value) {
        assert!(serde_json::from_value::<TemplateManifest>(value)
            .map(|manifest| crate::manifest::validate(&manifest).is_err())
            .unwrap_or(true));
    }

    #[test]
    fn validates_fresh_active_acknowledgement_and_rejects_missing_false_or_saved_true_without_proof(
    ) {
        let manifest = fixture_manifest();
        assert!(validate(&manifest, &values(), Some(TENANT), &[proof()]).is_ok());
        assert!(validate(&manifest, &values(), Some(TENANT), &[]).is_err());
        for value in [Value::Null, json!(false), json!("true"), json!(1)] {
            let mut values = values();
            values.insert("CONFIRM_ALL".into(), value);
            assert!(validate(&manifest, &values, Some(TENANT), &[proof()]).is_err());
        }
    }

    #[test]
    fn rejects_stale_tenant_and_field_or_group_controller_proofs() {
        let manifest = fixture_manifest();
        for stale in ["tenant", "mode", "enabled", "missing", "extra"] {
            let mut proof = proof();
            match stale {
                "tenant" => proof.tenant_id = "22222222-2222-2222-2222-222222222222".into(),
                "missing" => {
                    proof.controlling_values.remove("enabled");
                }
                "extra" => {
                    proof
                        .controlling_values
                        .insert("unrelated".into(), json!(true));
                }
                key => {
                    proof.controlling_values.insert(key.into(), json!(false));
                }
            }
            assert!(
                validate(&manifest, &values(), Some(TENANT), &[proof]).is_err(),
                "{stale}"
            );
        }
        assert!(validate(&manifest, &values(), None, &[proof()]).is_err());
        assert!(validate(&manifest, &values(), Some("not-a-tenant"), &[proof()]).is_err());
    }

    #[test]
    fn ignores_inactive_acknowledgements_in_field_or_group_conditions() {
        for (key, value) in [("MODE", json!("limited")), ("ENABLED", json!(false))] {
            let mut values = values();
            values.insert(key.into(), value);
            values.remove("CONFIRM_ALL");
            assert!(validate(&fixture_manifest(), &values, None, &[]).is_ok());
        }
    }

    #[test]
    fn missing_and_null_controller_values_use_declared_defaults() {
        for values in [
            HashMap::new(),
            HashMap::from([("MODE".into(), Value::Null)]),
        ] {
            assert!(validate(&fixture_manifest(), &values, Some(TENANT), &[]).is_err());
        }
    }

    #[test]
    fn rejects_duplicate_unknown_and_forged_proof_keys() {
        assert!(validate(
            &fixture_manifest(),
            &values(),
            Some(TENANT),
            &[proof(), proof()]
        )
        .is_err());
        let mut unknown = proof();
        unknown.field_id = "mode".into();
        assert!(validate(&fixture_manifest(), &values(), Some(TENANT), &[unknown]).is_err());
    }

    #[test]
    fn rejects_acknowledgement_defaults_and_other_forbidden_property_presence() {
        for (key, value) in [
            ("default", json!(true)),
            ("default", json!(false)),
            ("default", Value::Null),
            ("options", json!([])),
            ("pattern", Value::Null),
            ("placeholder", json!("")),
            ("lookup", Value::Null),
            ("sensitive", json!(true)),
            ("required", json!(false)),
            ("condition", Value::Null),
            ("label", json!("")),
            ("validationMessage", json!("")),
            ("validationMessage", Value::Null),
        ] {
            let mut manifest = fixture_value();
            manifest["configuration"]["groups"][1]["fields"][0][key] = value;
            reject_manifest(manifest);
        }
        let mut manifest = fixture_value();
        manifest["configuration"]["groups"][0]["fields"][0]["validationMessage"] = Value::Null;
        reject_manifest(manifest);
    }

    #[test]
    fn rejects_sensitive_conditional_unknown_and_acknowledgement_controllers() {
        for variant in ["sensitive", "field", "group", "unknown", "self"] {
            let mut manifest = fixture_value();
            match variant {
                "sensitive" => {
                    manifest["configuration"]["groups"][0]["fields"][0]["sensitive"] = json!(true)
                }
                "field" => {
                    manifest["configuration"]["groups"][0]["fields"][0]["condition"] =
                        json!({"field":"enabled","equals":true})
                }
                "group" => {
                    manifest["configuration"]["groups"][0]["condition"] =
                        json!({"field":"enabled","equals":true})
                }
                "unknown" => {
                    manifest["configuration"]["groups"][1]["fields"][0]["condition"]["field"] =
                        json!("missing")
                }
                _ => {
                    manifest["configuration"]["groups"][1]["fields"][0]["condition"]["field"] =
                        json!("confirm")
                }
            }
            reject_manifest(manifest);
        }
    }

    #[test]
    fn rejects_acknowledgements_from_recommendation_profiles() {
        for value in [json!(true), json!(false)] {
            let mut manifest = fixture_value();
            manifest["recommendationProfiles"] = json!([{"id":"profile","title":"Profile","description":"Description","values":{"confirm":value}}]);
            reject_manifest(manifest);
        }
    }

    #[test]
    fn snapshot_roundtrip_omits_forbidden_ack_properties_and_preserves_digest() {
        let mut entry = crate::catalog::entry_by_id("myworkid").unwrap();
        entry.manifest = Some(fixture_manifest());
        let snapshot = crate::models::ProjectTemplateSnapshot::new(entry).unwrap();
        let json = serde_json::to_value(&snapshot).unwrap();
        let field = &json["entry"]["manifest"]["configuration"]["groups"][1]["fields"][0];
        for key in ["default", "options", "pattern", "placeholder", "lookup"] {
            assert!(field.get(key).is_none());
        }
        let reopened: crate::models::ProjectTemplateSnapshot =
            serde_json::from_value(json).unwrap();
        reopened
            .validate_binding(Some(&snapshot.entry.id), Some(&snapshot.entry.commit))
            .unwrap();
        assert_eq!(reopened.digest, snapshot.digest);
    }

    #[test]
    fn saved_environment_uses_exact_selection_and_typed_controller_values() {
        let root = tempfile::tempdir().unwrap();
        for (name, mode, confirmed) in [("dev", "limited", "false"), ("prod", "all", "true")] {
            let path = root.path().join(".azure").join(name);
            std::fs::create_dir_all(&path).unwrap();
            std::fs::write(path.join(".env"), format!("MODE=\"{mode}\"\nENABLED=\"true\"\nCONFIRM_ALL=\"{confirmed}\"\nUNRELATED_SECRET=private\n")).unwrap();
        }
        let manifest = fixture_manifest();
        let dev = saved_binding_values(&manifest, root.path(), "dev").unwrap();
        assert!(validate(&manifest, &dev, None, &[]).is_ok());
        let prod = saved_binding_values(&manifest, root.path(), "prod").unwrap();
        assert_eq!(prod, values());
        assert!(!prod.contains_key("UNRELATED_SECRET"));
        assert!(validate(&manifest, &prod, Some(TENANT), &[]).is_err());
        assert!(validate(&manifest, &prod, Some(TENANT), &[proof()]).is_ok());
        assert!(saved_binding_values(&manifest, root.path(), "../prod").is_err());
        assert!(saved_binding_values(&manifest, root.path(), "missing").is_err());
    }

    #[test]
    fn supports_typed_multiselect_includes_and_not_equals_conditions() {
        let mut value = fixture_value();
        value["configuration"]["groups"][0]["fields"][0]["type"] = json!("multiselect");
        value["configuration"]["groups"][0]["fields"][0]["default"] = json!(["all"]);
        value["configuration"]["groups"][1]["fields"][0]["condition"] =
            json!({"field":"mode","includes":"all"});
        let manifest: TemplateManifest = serde_json::from_value(value).unwrap();
        let mut values = values();
        values.insert("MODE".into(), json!(["all"]));
        let mut proof = proof();
        proof
            .controlling_values
            .insert("mode".into(), json!(["all"]));
        assert!(validate(&manifest, &values, Some(TENANT), &[proof]).is_ok());
        assert!(validate(&manifest, &values, Some(TENANT), &[]).is_err());
        let mut manifest = fixture_manifest();
        let condition = manifest.configuration.groups[1].fields[0]
            .condition
            .as_mut()
            .unwrap();
        condition.equals = None;
        condition.not_equals = Some(json!("limited"));
        assert!(validate(&manifest, &HashMap::new(), Some(TENANT), &[]).is_err());
    }
    #[test]
    fn native_wire_rejects_unknown_ack_properties_and_malformed_condition_presence() {
        for (key, value) in [
            ("command", json!("run")),
            ("defaultValue", json!(true)),
            ("help", Value::Null),
        ] {
            let mut manifest = fixture_value();
            manifest["configuration"]["groups"][1]["fields"][0][key] = value;
            reject_manifest(manifest);
        }
        for condition in [
            json!({"field":"enabled","equals":null,"notEquals":false}),
            json!({"field":"enabled","equals":[true]}),
            json!({"field":"enabled","equals":{}}),
            json!({"field":"mode","includes":null}),
        ] {
            let mut manifest = fixture_value();
            manifest["configuration"]["groups"][1]["fields"][0]["condition"] = condition;
            reject_manifest(manifest);
        }
        let mut manifest = fixture_value();
        manifest["configuration"]["groups"][0]["fields"][0]["type"] = json!("multiselect");
        manifest["configuration"]["groups"][0]["fields"][0]["options"] =
            json!([{"label":"Empty","value":""}]);
        manifest["configuration"]["groups"][0]["fields"][0]["default"] = json!([""]);
        manifest["configuration"]["groups"][1]["fields"][0]["condition"] =
            json!({"field":"mode","includes":""});
        reject_manifest(manifest);
    }
    #[test]
    fn optional_blank_select_controller_is_explicit_but_required_blank_and_unknown_choices_fail() {
        let mut manifest = fixture_manifest();
        manifest.configuration.groups[0].fields[0].default = None;
        let mut supplied = values();
        supplied.insert("MODE".into(), json!(""));
        assert!(validate_preparation(&manifest, &supplied, None, &[]).is_ok());
        assert_eq!(
            binding_values(&manifest, &supplied).unwrap()["MODE"],
            json!("")
        );
        supplied.insert("MODE".into(), json!("unknown"));
        assert!(validate_preparation(&manifest, &supplied, None, &[]).is_err());
        manifest.configuration.groups[0].fields[0].required = true;
        supplied.insert("MODE".into(), json!(""));
        assert!(validate_preparation(&manifest, &supplied, None, &[]).is_err());
    }
    #[test]
    fn saved_missing_controller_without_default_is_unknown_not_inactive() {
        let root = tempfile::tempdir().unwrap();
        let directory = root.path().join(".azure/dev");
        std::fs::create_dir_all(&directory).unwrap();
        std::fs::write(directory.join(".env"), "MODE=limited\nCONFIRM_ALL=false\n").unwrap();
        let mut manifest = fixture_manifest();
        manifest.configuration.groups[0].fields[1].default = None;
        let saved = saved_binding_values(&manifest, root.path(), "dev").unwrap();
        assert_eq!(saved["ENABLED"], Value::Null);
        assert!(validate(&manifest, &saved, None, &[]).is_err());
        manifest.configuration.groups[0].fields[1].default = Some(json!(true));
        let saved = saved_binding_values(&manifest, root.path(), "dev").unwrap();
        assert_eq!(saved["ENABLED"], json!(true));
        assert!(validate(&manifest, &saved, None, &[]).is_ok());
    }
}
