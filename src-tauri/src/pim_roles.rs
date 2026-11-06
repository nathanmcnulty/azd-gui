//! Normalized, tenant-scoped complete role inventory. Never stores Graph bodies.
use crate::error::{AppError, AppResult};
use serde::{Deserialize, Serialize};
use std::{
    collections::HashSet,
    io::Write,
    path::{Path, PathBuf},
};
use uuid::Uuid;
fn invalid(message: &str) -> AppError {
    AppError::Invalid(message.into())
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Role {
    pub id: String,
    pub display_name: String,
    pub is_built_in: Option<bool>,
    pub is_enabled: Option<bool>,
    pub is_privileged: Option<bool>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Catalog {
    pub tenant_id: String,
    pub retrieved_at: String,
    pub source: String,
    pub roles: Vec<Role>,
}
fn tenant_id(tenant: &str) -> AppResult<String> {
    Uuid::parse_str(tenant)
        .ok()
        .filter(|id| !id.is_nil())
        .map(|id| id.to_string())
        .ok_or_else(|| invalid("Enter a valid tenant ID to load roles."))
}
impl Catalog {
    fn validate(&self, tenant: &str) -> AppResult<()> {
        if self.tenant_id != tenant_id(tenant)?
            || self.roles.len() > 5000
            || !matches!(self.source.as_str(), "live" | "cached")
            || chrono::DateTime::parse_from_rfc3339(&self.retrieved_at).is_err()
        {
            return Err(invalid("Role catalog identity or metadata is invalid."));
        }
        let mut ids = HashSet::new();
        for role in &self.roles {
            if tenant_id(&role.id)? != role.id
                || !ids.insert(&role.id)
                || role.display_name.trim().is_empty()
                || role.display_name.len() > 4096
            {
                return Err(invalid("Role catalog contains invalid or duplicate roles."));
            }
        }
        Ok(())
    }
    pub fn live(tenant: &str, roles: serde_json::Value) -> AppResult<Self> {
        let catalog = Self {
            tenant_id: tenant_id(tenant)?,
            retrieved_at: chrono::Utc::now().to_rfc3339(),
            source: "live".into(),
            roles: serde_json::from_value(roles)
                .map_err(|_| invalid("Role refresh returned an incomplete or invalid catalog."))?,
        };
        catalog.validate(tenant)?;
        Ok(catalog)
    }
    fn path(root: &Path, tenant: &str) -> AppResult<PathBuf> {
        Ok(root.join(format!("{}.json", tenant_id(tenant)?)))
    }
    pub fn save(&self, root: &Path) -> AppResult<()> {
        self.validate(&self.tenant_id)?;
        std::fs::create_dir_all(root)?;
        let mut temporary = tempfile::NamedTempFile::new_in(root)?;
        temporary.write_all(&serde_json::to_vec(self)?)?;
        temporary.as_file().sync_all()?;
        temporary
            .persist(Self::path(root, &self.tenant_id)?)
            .map_err(|_| {
                invalid("Live roles loaded but the last complete cache could not be updated.")
            })?;
        Ok(())
    }
    pub fn load(root: &Path, tenant: &str) -> AppResult<Option<Self>> {
        let path = Self::path(root, tenant)?;
        if !path.exists() {
            return Ok(None);
        }
        if std::fs::metadata(&path)?.len() > 8 * 1024 * 1024 {
            return Err(invalid("Role cache exceeds limit. Refresh tenant roles."));
        }
        let mut catalog: Self = serde_json::from_slice(&std::fs::read(path)?)
            .map_err(|_| invalid("Role cache is invalid. Refresh tenant roles."))?;
        catalog.validate(tenant)?;
        catalog.source = "cached".into();
        Ok(Some(catalog))
    }
    pub fn validate_selection(
        &self,
        configuration: &crate::pim_plan::Configuration,
    ) -> AppResult<()> {
        for (ids, privileged) in [
            (&configuration.privileged_role_ids, true),
            (&configuration.less_privileged_role_ids, false),
        ] {
            for id in ids {
                if !self.roles.iter().any(|role| {
                    &role.id == id
                        && role.is_enabled == Some(true)
                        && role.is_privileged == Some(privileged)
                }) {
                    return Err(invalid("A selected role is missing, disabled, reclassified or has unknown classification. Refresh and review the selected roles."));
                }
            }
        }
        Ok(())
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    const TENANT: &str = "11111111-1111-4111-8111-111111111111";
    fn rows() -> serde_json::Value {
        serde_json::json!([{"id":"33333333-3333-4333-8333-333333333333", "displayName":"Custom role", "isBuiltIn":false,"isEnabled":true,"isPrivileged":null}])
    }
    #[test]
    fn cache_is_tenant_scoped_and_preserves_timestamp_and_unknown() {
        let root = tempfile::tempdir().unwrap();
        let first = Catalog::live(TENANT, rows()).unwrap();
        first.save(root.path()).unwrap();
        let cached = Catalog::load(root.path(), TENANT).unwrap().unwrap();
        assert_eq!(cached.source, "cached");
        assert_eq!(cached.retrieved_at, first.retrieved_at);
        assert_eq!(cached.roles[0].is_privileged, None);
        assert!(
            Catalog::load(root.path(), "22222222-2222-4222-8222-222222222222")
                .unwrap()
                .is_none()
        );
        assert!(Catalog::live(TENANT, serde_json::json!({"value":[]})).is_err());
        assert_eq!(
            Catalog::load(root.path(), TENANT).unwrap().unwrap().roles,
            first.roles
        );
        let mut newer = first.clone();
        newer.roles[0].display_name = "Renamed".into();
        newer.save(root.path()).unwrap();
        assert_eq!(
            Catalog::load(root.path(), TENANT).unwrap().unwrap().roles[0].display_name,
            "Renamed"
        );
        assert!(Catalog::load(root.path(), "../escape").is_err());
    }
    #[test]
    fn selected_roles_must_exist_be_enabled_and_have_explicit_matching_classification() {
        let mut catalog = Catalog::live(TENANT, rows()).unwrap();
        let configuration: crate::pim_plan::Configuration = serde_json::from_value(serde_json::json!({
            "privilegedRoleIds":["33333333-3333-4333-8333-333333333333"],"lessPrivilegedRoleIds":[],
            "privilegedAuthenticationProfile":"mfa","lessPrivilegedAuthenticationProfile":"mfa",
            "privilegedDeviceRequirement":"none","lessPrivilegedDeviceRequirement":"none"
        })).unwrap();
        assert!(catalog.validate_selection(&configuration).is_err());
        catalog.roles[0].is_privileged = Some(false);
        assert!(catalog.validate_selection(&configuration).is_err());
        catalog.roles[0].is_privileged = Some(true);
        assert!(catalog.validate_selection(&configuration).is_ok());
        catalog.roles[0].is_enabled = Some(false);
        assert!(catalog.validate_selection(&configuration).is_err());
        catalog.roles.clear();
        assert!(catalog.validate_selection(&configuration).is_err());
    }

    #[test]
    fn duplicate_or_invalid_catalog_is_never_accepted() {
        let mut data = rows();
        let duplicate = data[0].clone();
        data.as_array_mut().unwrap().push(duplicate);
        assert!(Catalog::live(TENANT, data).is_err());
        assert!(Catalog::live(TENANT, serde_json::json!([{"id":"not-a-guid"}])).is_err());
        assert!(Catalog::live(TENANT, serde_json::json!([])).is_ok());
    }
}
