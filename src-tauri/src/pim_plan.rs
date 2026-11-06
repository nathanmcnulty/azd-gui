//! Registered read-only operation for the pinned azd-pim planner.
use crate::{
    error::{AppError, AppResult},
    graph_session::GraphBinding,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use uuid::Uuid;

pub const SOURCE: &str = "48bdbadb6a02c06fb4144e616ed9562b905b36ec";
pub const SCOPES: [&str; 3] = [
    "AuthenticationContext.Read.All",
    "Policy.Read.All",
    "RoleManagement.Read.Directory",
];

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Input {
    pub review_id: String,
    pub account: String,
    pub tenant_id: String,
    pub client_id: String,
    pub configuration: Configuration,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Configuration {
    #[serde(default)]
    pub preview_existing_objects: bool,
    pub privileged_role_ids: Vec<String>,
    pub less_privileged_role_ids: Vec<String>,
    pub privileged_authentication_profile: AuthenticationProfile,
    pub less_privileged_authentication_profile: AuthenticationProfile,
    pub privileged_device_requirement: DeviceRequirement,
    pub less_privileged_device_requirement: DeviceRequirement,
}

impl Configuration {
    pub fn existing_objects_notice(&self) -> &'static str {
        if self.preview_existing_objects {
            "Existing-object preview: ON. Proposed updates may include existing contexts, policies and role-context links. This does not establish ownership, adopt objects or apply changes. Proposed context updates also enable availability and replace the description with azd-pim's managed description. Review ownership and all current/desired settings before any enforcement elsewhere."
        } else {
            "Existing-object preview: OFF. Existing ownership or role-context conflicts stop this plan."
        }
    }

    pub fn validate_preview_result(&self, plan: &serde_json::Value) -> AppResult<()> {
        let expected = Some(self.preview_existing_objects);
        if plan
            .get("previewExistingObjects")
            .and_then(|value| value.as_bool())
            != expected
            || plan
                .get("ownershipReviewRequired")
                .and_then(|value| value.as_bool())
                != Some(true)
            || plan
                .get("ownershipEstablished")
                .and_then(|value| value.as_bool())
                != Some(false)
            || plan
                .get("executionAuthorized")
                .and_then(|value| value.as_bool())
                != Some(false)
            || plan
                .pointer("/configuration/AdoptExisting")
                .and_then(|value| value.as_bool())
                != expected
            || plan
                .pointer("/configuration/AdoptRoleContexts")
                .and_then(|value| value.as_bool())
                != expected
            || plan
                .pointer("/configuration/Mode")
                .and_then(|value| value.as_str())
                != Some("plan")
        {
            return Err(AppError::Invalid(
                "PIM plan preview choice or mode does not match this review.".into(),
            ));
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum AuthenticationProfile {
    Mfa,
    PhishingResistant,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum DeviceRequirement {
    None,
    Compliant,
    HybridJoined,
    CompliantOrHybrid,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Review {
    pub source_commit: &'static str,
    pub binding: GraphBinding,
    pub configuration: Configuration,
}

impl Input {
    pub fn review(self) -> AppResult<Review> {
        self.review_internal(false)
    }
    pub fn review_catalog(self) -> AppResult<Review> {
        self.review_internal(true)
    }
    fn review_internal(mut self, allow_empty: bool) -> AppResult<Review> {
        if Uuid::parse_str(&self.review_id).map_or(true, |id| id.is_nil()) {
            return Err(AppError::Invalid(
                "A unique PIM review identity is required.".into(),
            ));
        }
        self.account = self.account.trim().to_ascii_lowercase();
        self.tenant_id = self.tenant_id.to_ascii_lowercase();
        self.client_id = self.client_id.to_ascii_lowercase();
        let configuration = &mut self.configuration;
        if configuration.privileged_role_ids.len() + configuration.less_privileged_role_ids.len()
            > 200
        {
            return Err(AppError::Invalid(
                "Select at most 200 role definitions.".into(),
            ));
        }
        for ids in [
            &mut configuration.privileged_role_ids,
            &mut configuration.less_privileged_role_ids,
        ] {
            for id in ids.iter_mut() {
                *id = Uuid::parse_str(id)
                    .ok()
                    .filter(|id| !id.is_nil())
                    .ok_or_else(|| {
                        AppError::Invalid("Role definition IDs must be non-empty GUIDs.".into())
                    })?
                    .to_string();
            }
            ids.sort();
            ids.dedup();
        }
        if !allow_empty
            && configuration.privileged_role_ids.is_empty()
            && configuration.less_privileged_role_ids.is_empty()
        {
            return Err(AppError::Invalid(
                "Select at least one role definition to plan.".into(),
            ));
        }
        if configuration
            .privileged_role_ids
            .iter()
            .any(|id| configuration.less_privileged_role_ids.contains(id))
        {
            return Err(AppError::Invalid(
                "A role definition cannot be selected in both tiers.".into(),
            ));
        }
        let digest = format!(
            "{:x}",
            Sha256::digest(serde_json::to_vec(&(SOURCE, SCOPES, &self))?)
        );
        let binding = GraphBinding {
            requirements_digest: digest,
            account: self.account,
            tenant_id: self.tenant_id,
            client_id: self.client_id,
            resource: "https://graph.microsoft.com".into(),
            purpose: Some("pimPlan".into()),
            scopes: SCOPES.iter().map(|s| (*s).into()).collect(),
            probe_uri: "https://graph.microsoft.com/v1.0/identity/conditionalAccess/policies?$top=1&$select=id".into(),
        };
        binding.validate()?;
        Ok(Review {
            source_commit: SOURCE,
            binding,
            configuration: self.configuration,
        })
    }
}

impl Review {
    pub fn expect_digest(&self, expected: &str) -> AppResult<()> {
        if self.binding.requirements_digest != expected {
            return Err(AppError::Invalid(
                "PIM identity or configuration changed. Review the plan again.".into(),
            ));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{json, Value};
    fn input() -> Value {
        json!({"reviewId":"99999999-9999-4999-8999-999999999999", "account":"operator@example.test", "tenantId":"11111111-1111-4111-8111-111111111111",
            "clientId":"22222222-2222-4222-8222-222222222222", "configuration": {
                "privilegedRoleIds":["33333333-3333-4333-8333-333333333333"], "lessPrivilegedRoleIds":[],
                "privilegedAuthenticationProfile":"mfa", "lessPrivilegedAuthenticationProfile":"mfa",
                "privilegedDeviceRequirement":"none", "lessPrivilegedDeviceRequirement":"none"}})
    }
    fn review(value: Value) -> Review {
        serde_json::from_value::<Input>(value)
            .unwrap()
            .review()
            .unwrap()
    }

    #[test]
    fn every_identity_and_configuration_field_changes_approval_digest() {
        let original = review(input());
        for (path, value) in [
            ("/reviewId", json!("88888888-8888-4888-8888-888888888888")),
            ("/account", json!("other@example.test")),
            ("/tenantId", json!("aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa")),
            ("/clientId", json!("bbbbbbbb-bbbb-4bbb-8bbb-bbbbbbbbbbbb")),
            (
                "/configuration/privilegedRoleIds",
                json!(["44444444-4444-4444-8444-444444444444"]),
            ),
            (
                "/configuration/lessPrivilegedRoleIds",
                json!(["44444444-4444-4444-8444-444444444444"]),
            ),
            (
                "/configuration/privilegedAuthenticationProfile",
                json!("phishingResistant"),
            ),
            (
                "/configuration/lessPrivilegedAuthenticationProfile",
                json!("phishingResistant"),
            ),
            (
                "/configuration/privilegedDeviceRequirement",
                json!("compliant"),
            ),
            (
                "/configuration/lessPrivilegedDeviceRequirement",
                json!("hybridJoined"),
            ),
        ] {
            let mut changed = input();
            *changed.pointer_mut(path).unwrap() = value;
            assert!(
                review(changed)
                    .expect_digest(&original.binding.requirements_digest)
                    .is_err(),
                "{path}"
            );
        }
        assert_eq!(original.binding.scopes, SCOPES);
        assert!(original.binding.scopes.iter().all(|s| !s.contains("Write")));
        let mut preview = input();
        preview["configuration"]["previewExistingObjects"] = json!(true);
        assert!(review(preview)
            .expect_digest(&original.binding.requirements_digest)
            .is_err());
        assert!(!original.configuration.preview_existing_objects);
    }

    #[test]
    fn preview_is_typed_and_result_flags_must_match_the_reviewed_choice() {
        for invalid in [json!(null), json!("true"), json!(1), json!({})] {
            let mut value = input();
            value["configuration"]["previewExistingObjects"] = invalid;
            assert!(serde_json::from_value::<Input>(value).is_err());
        }
        for enabled in [false, true] {
            let mut value = input();
            value["configuration"]["previewExistingObjects"] = json!(enabled);
            let selected = review(value);
            let valid = json!({"mode":"plan","previewExistingObjects":enabled,"ownershipReviewRequired":true,"ownershipEstablished":false,"executionAuthorized":false,
                "configuration":{"Mode":"plan","AdoptExisting":enabled,"AdoptRoleContexts":enabled}});
            assert!(selected
                .configuration
                .validate_preview_result(&valid)
                .is_ok());
            for pointer in [
                "/previewExistingObjects",
                "/configuration/AdoptExisting",
                "/configuration/AdoptRoleContexts",
            ] {
                let mut changed = valid.clone();
                *changed.pointer_mut(pointer).unwrap() = json!(!enabled);
                assert!(selected
                    .configuration
                    .validate_preview_result(&changed)
                    .is_err());
            }
            for (pointer, invalid) in [
                ("/ownershipReviewRequired", false),
                ("/ownershipEstablished", true),
                ("/executionAuthorized", true),
            ] {
                let mut changed = valid.clone();
                *changed.pointer_mut(pointer).unwrap() = json!(invalid);
                assert!(selected
                    .configuration
                    .validate_preview_result(&changed)
                    .is_err());
            }
            let mut enforced = valid;
            enforced["configuration"]["Mode"] = json!("enforced");
            assert!(selected
                .configuration
                .validate_preview_result(&enforced)
                .is_err());
        }
    }

    #[test]
    fn rejects_empty_invalid_duplicate_tiers_and_extension_or_write_inputs() {
        for (path, value) in [
            ("/configuration/privilegedRoleIds", json!([])),
            ("/configuration/privilegedRoleIds", json!(["not-a-guid"])),
            (
                "/configuration/lessPrivilegedRoleIds",
                json!(["33333333-3333-4333-8333-333333333333"]),
            ),
        ] {
            let mut invalid = input();
            *invalid.pointer_mut(path).unwrap() = value;
            assert!(serde_json::from_value::<Input>(invalid)
                .unwrap()
                .review()
                .is_err());
        }
        for field in [
            "mode",
            "notificationMode",
            "enableSessionRevocation",
            "script",
            "uri",
        ] {
            let mut invalid = input();
            invalid["configuration"][field] = json!("enforced");
            assert!(serde_json::from_value::<Input>(invalid).is_err());
        }
    }

    #[test]
    fn bundled_planner_bytes_match_pinned_template_manifest() {
        let manifest: Value =
            serde_json::from_str(include_str!("../pim-template/source.json")).unwrap();
        assert_eq!(manifest["commit"], SOURCE);
        for (name, bytes) in [
            (
                "AzdPim.Common.psm1",
                include_bytes!("../pim-template/AzdPim.Common.psm1").as_slice(),
            ),
            (
                "AzdPim.Graph.psm1",
                include_bytes!("../pim-template/AzdPim.Graph.psm1").as_slice(),
            ),
            (
                "LICENSE",
                include_bytes!("../pim-template/LICENSE").as_slice(),
            ),
        ] {
            assert_eq!(
                manifest["files"][name]["sha256"],
                format!("{:x}", Sha256::digest(bytes))
            );
        }
    }
}
