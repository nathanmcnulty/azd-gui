use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Catalog {
    pub schema_version: String,
    pub id: String,
    pub title: String,
    pub entries: Vec<CatalogEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CatalogEntry {
    pub id: String,
    pub title: String,
    pub summary: String,
    pub description: String,
    pub repository: String,
    pub commit: String,
    #[serde(default)]
    pub subdirectory: Option<String>,
    pub tags: Vec<String>,
    pub platforms: Vec<String>,
    #[serde(default)]
    pub manifest: Option<TemplateManifest>,
    #[serde(default)]
    pub source: Option<String>,
    #[serde(default)]
    pub trusted: bool,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CatalogFile {
    pub schema_version: String,
    pub id: String,
    pub title: String,
    pub entries: Vec<CatalogFileEntry>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CatalogFileEntry {
    pub id: String,
    pub title: String,
    pub summary: String,
    pub description: String,
    pub repository: String,
    pub commit: String,
    #[serde(default)]
    pub subdirectory: Option<String>,
    pub tags: Vec<String>,
    pub platforms: Vec<String>,
    #[serde(default)]
    pub manifest: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct TemplateManifest {
    pub schema_version: String,
    #[serde(default)]
    pub minimum_azd_version: Option<String>,
    #[serde(default)]
    pub prerequisites: Vec<Prerequisite>,
    #[serde(default)]
    pub connections: Vec<ConnectionRequirement>,
    #[serde(default)]
    pub configuration: Configuration,
    #[serde(default)]
    pub permission_checks: Vec<PermissionCheck>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Prerequisite {
    pub id: String,
    pub name: String,
    pub kind: String,
    #[serde(default)]
    pub minimum_version: Option<String>,
    pub required: bool,
    #[serde(default)]
    pub install_provider: Option<String>,
    #[serde(default)]
    pub install_id: Option<String>,
    #[serde(default)]
    pub documentation_url: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolStatus {
    pub id: String,
    pub name: String,
    pub kind: String,
    #[serde(default)]
    pub minimum_version: Option<String>,
    pub required: bool,
    pub state: String,
    #[serde(default)]
    pub installed_version: Option<String>,
    #[serde(default)]
    pub detail: Option<String>,
    #[serde(default)]
    pub install_provider: Option<String>,
    #[serde(default)]
    pub install_id: Option<String>,
    #[serde(default)]
    pub documentation_url: Option<String>,
    #[serde(default)]
    pub install_command: Option<String>,
    pub can_install: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConnectionRequirement {
    pub id: String,
    pub name: String,
    pub required: bool,
    #[serde(default)]
    pub scopes: Vec<String>,
    pub reason: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConnectionStatus {
    pub id: String,
    pub name: String,
    pub required: bool,
    pub reason: String,
    pub state: String,
    #[serde(default)]
    pub scopes: Vec<String>,
    #[serde(default)]
    pub account: Option<String>,
    #[serde(default)]
    pub tenant_id: Option<String>,
    #[serde(default)]
    pub expires_on: Option<String>,
    #[serde(default)]
    pub detail: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct Configuration {
    #[serde(default)]
    pub groups: Vec<ConfigurationGroup>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConfigurationGroup {
    pub id: String,
    pub title: String,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub condition: Option<FieldCondition>,
    pub fields: Vec<ConfigurationField>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FieldCondition {
    pub field: String,
    #[serde(default)]
    pub equals: Option<serde_json::Value>,
    #[serde(default)]
    pub not_equals: Option<serde_json::Value>,
    #[serde(default)]
    pub includes: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConfigurationField {
    pub id: String,
    pub environment_variable: String,
    pub label: String,
    #[serde(rename = "type")]
    pub field_type: String,
    #[serde(default)]
    pub required: bool,
    #[serde(default)]
    pub sensitive: bool,
    #[serde(default)]
    pub help: Option<String>,
    #[serde(default)]
    pub placeholder: Option<String>,
    #[serde(default)]
    pub default: Option<serde_json::Value>,
    #[serde(default)]
    pub options: Vec<FieldOption>,
    #[serde(default)]
    pub pattern: Option<String>,
    #[serde(default)]
    pub condition: Option<FieldCondition>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FieldOption {
    pub label: String,
    pub value: String,
    #[serde(default)]
    pub description: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PermissionCheck {
    pub id: String,
    pub name: String,
    pub provider: String,
    pub required: bool,
    pub description: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectSummary {
    pub id: String,
    pub name: String,
    pub path: String,
    #[serde(default)]
    pub environment: Option<String>,
    #[serde(default)]
    pub template_id: Option<String>,
    #[serde(default)]
    pub revision: Option<String>,
    #[serde(default)]
    pub last_operation: Option<String>,
    pub state: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BootstrapData {
    pub catalog: Catalog,
    pub tools: Vec<ToolStatus>,
    pub connections: Vec<ConnectionStatus>,
    pub projects: Vec<ProjectSummary>,
    pub platform: String,
    pub app_version: String,
    pub is_native: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TemplateInspection {
    pub entry: CatalogEntry,
    pub source_path: Option<String>,
    pub azure_yaml_found: bool,
    pub manifest_source: String,
    pub interactive_hooks: bool,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OperationEvent {
    pub operation_id: String,
    pub kind: String,
    pub state: String,
    pub timestamp: String,
    pub stream: String,
    pub level: String,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub progress: Option<u8>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CatalogSource {
    pub id: String,
    pub label: String,
    pub url: String,
    pub trusted: bool,
    #[serde(default)]
    pub pinned_revision: Option<String>,
    #[serde(default)]
    pub accepted_catalog: Option<Catalog>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PrepareDeploymentRequest {
    pub template_id: String,
    pub source_mode: String,
    #[serde(default)]
    pub source_path: Option<String>,
    #[serde(default)]
    pub source_revision: Option<String>,
    pub destination: String,
    pub environment: String,
    #[serde(default)]
    pub subscription: Option<String>,
    pub location: String,
    #[serde(default)]
    pub environment_values: HashMap<String, serde_json::Value>,
}
