//! Short-lived, native observations for opt-in acknowledgement fields.
use crate::{
    error::{AppError, AppResult},
    pim_roles::Catalog,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    time::{Duration, Instant},
};
use uuid::Uuid;

const LIFETIME: Duration = Duration::from_secs(5 * 60);

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum Provider {
    EntraPimRoleDefinitions,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "camelCase")]
pub enum Classification {
    Privileged,
    LessPrivileged,
    All,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Declaration {
    pub id: Provider,
    pub classification: Classification,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub selected_ids_field: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Proof {
    pub id: Provider,
    pub classification: Classification,
    pub digest: String,
    pub revision: Uuid,
    pub selected_ids: Vec<String>,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct View {
    pub digest: String,
    pub revision: Uuid,
    pub ids: Vec<String>,
}

pub struct Observation {
    tenant: String,
    observed: Instant,
    views: BTreeMap<Classification, View>,
}

impl Observation {
    pub fn from_live_catalog(catalog: &Catalog) -> Option<Self> {
        if catalog.source != "live" || catalog.roles.is_empty() {
            return None;
        }
        let revision = Uuid::new_v4();
        let mut roles: Vec<_> = catalog.roles.iter().collect();
        roles.sort_by(|a, b| a.id.cmp(&b.id));
        let mut views = BTreeMap::new();
        for classification in [
            Classification::Privileged,
            Classification::LessPrivileged,
            Classification::All,
        ] {
            // A role of unknown classification can belong to either tier.
            if classification != Classification::All
                && roles.iter().any(|role| role.is_privileged.is_none())
            {
                continue;
            }
            let matching: Vec<_> = roles
                .iter()
                .copied()
                .filter(|role| match classification {
                    Classification::Privileged => role.is_privileged == Some(true),
                    Classification::LessPrivileged => role.is_privileged == Some(false),
                    Classification::All => true,
                })
                .collect();
            if matching.is_empty() {
                continue;
            }
            let normalized =
                serde_json::to_vec(&(catalog.tenant_id.as_str(), classification, &matching))
                    .ok()?;
            views.insert(
                classification,
                View {
                    digest: hex::encode(Sha256::digest(normalized)),
                    revision,
                    ids: matching.iter().map(|role| role.id.clone()).collect(),
                },
            );
        }
        Some(Self {
            tenant: catalog.tenant_id.clone(),
            observed: Instant::now(),
            views,
        })
    }

    pub fn view(&self, tenant: Uuid, declaration: &Declaration) -> Option<&View> {
        if self.observed.elapsed() >= LIFETIME || self.tenant != tenant.to_string() {
            return None;
        }
        self.views.get(&declaration.classification)
    }
}

pub(super) fn selected_ids(value: Option<&serde_json::Value>) -> AppResult<Vec<String>> {
    fn invalid() -> AppError {
        AppError::Invalid("Select valid unique IDs from the complete provider result.".into())
    }
    let items = match value {
        Some(serde_json::Value::Array(items)) => items
            .iter()
            .map(|id| id.as_str().ok_or_else(invalid))
            .collect::<AppResult<Vec<_>>>()?,
        Some(serde_json::Value::String(items)) if items.len() <= 8 * 1024 => {
            items.split([',', ';']).collect()
        }
        _ => return Err(invalid()),
    };
    if items.is_empty() || items.len() > 200 {
        return Err(invalid());
    }
    let mut ids = items
        .into_iter()
        .map(|id| {
            Uuid::parse_str(id.trim())
                .ok()
                .filter(|id| !id.is_nil())
                .map(|id| id.to_string())
                .ok_or_else(invalid)
        })
        .collect::<AppResult<Vec<_>>>()?;
    ids.sort();
    if ids.windows(2).any(|pair| pair[0] == pair[1]) {
        return Err(invalid());
    }
    Ok(ids)
}
