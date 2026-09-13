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

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExternalCatalogPreview {
    pub catalog: Catalog,
    pub review_token: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CatalogSourceSummary {
    pub id: String,
    pub label: String,
    pub url: String,
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
    pub requires_interactive_terminal: bool,
    #[serde(default)]
    pub prerequisites: Vec<Prerequisite>,
    #[serde(default)]
    pub connections: Vec<ConnectionRequirement>,
    #[serde(default)]
    pub configuration: Configuration,
    #[serde(default)]
    pub recommendation_profiles: Vec<RecommendationProfile>,
    #[serde(default)]
    pub permission_checks: Vec<PermissionCheck>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RecommendationProfile {
    pub id: String,
    pub title: String,
    pub description: String,
    pub values: std::collections::HashMap<String, serde_json::Value>,
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
    pub executable_path: Option<String>,
    #[serde(default)]
    pub latest_version: Option<String>,
    #[serde(default)]
    pub update_available: bool,
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
pub struct ToolInstallPlan {
    pub id: String,
    pub tool_name: String,
    pub provider: String,
    pub action: String,
    pub source: String,
    pub scope: String,
    pub destination: String,
    pub exact_version: String,
    pub command: String,
    pub documentation_url: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolInstallApprovalGrant {
    pub token: String,
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

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConnectionTarget {
    pub tenant_id: String,
    pub subscription_id: String,
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
    pub granted_scopes: Vec<String>,
    #[serde(default)]
    pub account: Option<String>,
    #[serde(default)]
    pub tenant_id: Option<String>,
    #[serde(default)]
    pub subscription_id: Option<String>,
    #[serde(default)]
    pub subscription_name: Option<String>,
    #[serde(default)]
    pub expires_on: Option<String>,
    #[serde(default)]
    pub detail: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AzureTenant {
    pub tenant_id: String,
    #[serde(default)]
    pub display_name: Option<String>,
    #[serde(default)]
    pub default_domain: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AzureSubscription {
    pub id: String,
    pub name: String,
    pub tenant_id: String,
    #[serde(default)]
    pub tenant_display_name: Option<String>,
    #[serde(default)]
    pub tenant_default_domain: Option<String>,
    pub is_default: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AzureLocation {
    pub name: String,
    pub display_name: String,
    #[serde(default)]
    pub regional_display_name: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AzureResourceGroup {
    pub name: String,
    pub location: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AzureDiscovery {
    pub provider: String,
    pub subscriptions: Vec<AzureSubscription>,
    pub tenants: Vec<AzureTenant>,
    pub locations: Vec<AzureLocation>,
    pub resource_groups: Vec<AzureResourceGroup>,
    #[serde(default)]
    pub selected_subscription_id: Option<String>,
    #[serde(default)]
    pub warnings: Vec<String>,
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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub equals: Option<serde_json::Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub not_equals: Option<serde_json::Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
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
pub struct PermissionCheckRequest {
    pub checks: Vec<PermissionCheck>,
    pub tenant_id: String,
    pub subscription_id: String,
    #[serde(default)]
    pub required_graph_scopes: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PermissionCheckResult {
    pub id: String,
    pub name: String,
    pub provider: String,
    pub required: bool,
    pub description: String,
    pub state: String,
    pub detail: String,
    pub blocking: bool,
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

/// The reviewed template contract, never the entered environment values.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectTemplateSnapshot {
    pub entry: CatalogEntry,
    pub digest: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectEnvironment {
    #[serde(alias = "Name")]
    pub name: String,
    #[serde(default, alias = "DotEnvPath")]
    pub dot_env_path: String,
    #[serde(default, alias = "HasLocal")]
    pub has_local: bool,
    #[serde(default, alias = "HasRemote")]
    pub has_remote: bool,
    #[serde(default, alias = "IsDefault")]
    pub is_default: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectEnvironmentValue {
    pub name: String,
    #[serde(default)]
    pub value: Option<String>,
    pub is_set: bool,
    pub sensitive: bool,
    pub declared: bool,
    pub required: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectEnvironmentValueInput {
    pub name: String,
    pub value: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum ThemePreference {
    System,
    Dark,
    Light,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct AppSettings {
    pub theme: ThemePreference,
    pub navigation_expanded: bool,
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            theme: ThemePreference::System,
            navigation_expanded: true,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BootstrapData {
    pub catalog: Catalog,
    pub catalog_sources: Vec<CatalogSourceSummary>,
    pub tools: Vec<ToolStatus>,
    pub connections: Vec<ConnectionStatus>,
    pub projects: Vec<ProjectSummary>,
    pub settings: AppSettings,
    pub platform: String,
    pub app_version: String,
    pub is_native: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TemplateInspection {
    pub entry: CatalogEntry,
    pub source_path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_binding: Option<LocalSourceBinding>,
    pub azure_yaml_found: bool,
    pub manifest_source: String,
    pub interactive_hooks: bool,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct LocalSourceBinding {
    pub source_path: String,
    pub content_digest: String,
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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub outcome: Option<OperationOutcome>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OperationOutcome {
    pub resource_state: String,
    pub summary: String,
    pub guidance: String,
    pub retryable: bool,
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

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ExecutionApprovalRequest {
    pub action: String,
    pub project_path: String,
    pub environment: String,
    pub template_id: String,
    #[serde(default)]
    pub revision: Option<String>,
    #[serde(default)]
    pub source_binding: Option<LocalSourceBinding>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExecutionApprovalGrant {
    pub token: String,
    pub project_path: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PrepareDeploymentRequest {
    pub template_id: String,
    pub source_mode: String,
    pub execution_approval_token: String,
    #[serde(default)]
    pub source_path: Option<String>,
    #[serde(default)]
    pub source_revision: Option<String>,
    #[serde(default)]
    pub source_binding: Option<LocalSourceBinding>,
    pub destination: String,
    pub environment: String,
    #[serde(default)]
    pub tenant: Option<String>,
    #[serde(default)]
    pub subscription: Option<String>,
    pub location: String,
    #[serde(default)]
    pub resource_group: Option<String>,
    #[serde(default)]
    pub environment_values: HashMap<String, serde_json::Value>,
}
