export type Platform = "windows" | "macos" | "linux";
export type ToolState = "ready" | "missing" | "outdated" | "checking" | "unknown";
export type ConnectionState = "connected" | "disconnected" | "expired" | "checking" | "unknown";
export type OperationState = "queued" | "running" | "succeeded" | "failed" | "cancelled";

export interface Catalog {
  schemaVersion: "1.0";
  id: string;
  title: string;
  entries: CatalogEntry[];
}

export interface ExternalCatalogPreview {
  catalog: Catalog;
  reviewToken: string;
}

export interface CatalogSourceSummary {
  id: string;
  label: string;
  url: string;
}

export interface CatalogEntry {
  id: string;
  title: string;
  summary: string;
  description: string;
  repository: string;
  commit: string;
  subdirectory?: string;
  tags: string[];
  platforms: Platform[];
  manifest?: TemplateManifest;
  source?: "bundled" | "external" | "local";
  trusted?: boolean;
}

export interface TemplateManifest {
  schemaVersion: "1.0";
  minimumAzdVersion?: string;
  prerequisites: Prerequisite[];
  connections: ConnectionRequirement[];
  configuration: {
    groups: ConfigurationGroup[];
  };
  permissionChecks?: PermissionCheck[];
}

export interface Prerequisite {
  id: string;
  name: string;
  kind: "tool" | "powershellModule";
  minimumVersion?: string;
  required: boolean;
  installProvider?: "azdTool" | "psResource" | "manual";
  installId?: string;
  documentationUrl?: string;
}

export interface ToolStatus extends Prerequisite {
  state: ToolState;
  installedVersion?: string;
  latestVersion?: string;
  updateAvailable?: boolean;
  detail?: string;
  installCommand?: string;
  canInstall?: boolean;
}

export interface ToolInstallPlan {
  id: string;
  toolName: string;
  provider: string;
  action: string;
  source: string;
  scope: string;
  destination: string;
  command: string;
  documentationUrl: string;
}

export interface ConnectionRequirement {
  id: "azd" | "azureCli" | "azPowerShell" | "graphPowerShell";
  name: string;
  required: boolean;
  scopes?: string[];
  reason: string;
}

export interface ConnectionStatus extends ConnectionRequirement {
  state: ConnectionState;
  grantedScopes?: string[];
  account?: string;
  tenantId?: string;
  subscriptionId?: string;
  subscriptionName?: string;
  expiresOn?: string;
  detail?: string;
}

export interface AzureTenant {
  tenantId: string;
  displayName?: string;
  defaultDomain?: string;
}

export interface AzureSubscription {
  id: string;
  name: string;
  tenantId: string;
  tenantDisplayName?: string;
  tenantDefaultDomain?: string;
  isDefault: boolean;
}

export interface AzureLocation {
  name: string;
  displayName: string;
  regionalDisplayName?: string;
}

export interface AzureResourceGroup {
  name: string;
  location: string;
}

export interface AzureDiscovery {
  provider: "azureCli";
  subscriptions: AzureSubscription[];
  tenants: AzureTenant[];
  locations: AzureLocation[];
  resourceGroups: AzureResourceGroup[];
  selectedSubscriptionId?: string;
  warnings: string[];
}

export interface ConfigurationGroup {
  id: string;
  title: string;
  description?: string;
  condition?: FieldCondition;
  fields: ConfigurationField[];
}

export interface FieldCondition {
  field: string;
  equals?: string | boolean;
  notEquals?: string | boolean;
  includes?: string;
}

export type FieldType =
  | "text"
  | "secret"
  | "boolean"
  | "select"
  | "multiselect"
  | "tenant"
  | "subscription"
  | "location"
  | "resourceGroup"
  | "entraObject";

export interface FieldOption {
  label: string;
  value: string;
  description?: string;
}

export interface ConfigurationField {
  id: string;
  environmentVariable: string;
  label: string;
  type: FieldType;
  required?: boolean;
  sensitive?: boolean;
  help?: string;
  placeholder?: string;
  default?: string | boolean | string[];
  options?: FieldOption[];
  pattern?: string;
  condition?: FieldCondition;
}

export interface PermissionCheck {
  id:
    | "azure-subscription-access"
    | "graph-directory-read"
    | "graph-delegated-scopes"
    | "entra-directory-access";
  name: string;
  provider: "azure" | "graph";
  required: boolean;
  description: string;
}

export interface PermissionCheckRequest {
  checks: PermissionCheck[];
  tenantId: string;
  subscriptionId: string;
  requiredGraphScopes: string[];
}

export interface PermissionCheckResult extends PermissionCheck {
  state: "passed" | "warning" | "failed";
  detail: string;
  blocking: boolean;
}

export type WizardPageKind =
  | "source"
  | "overview"
  | "prerequisites"
  | "connections"
  | "environment"
  | "configuration"
  | "preflight"
  | "review"
  | "deploy";

export interface WizardPage {
  id: string;
  kind: WizardPageKind;
  title: string;
  description: string;
  group?: ConfigurationGroup;
  complete: boolean;
  blockingIssues: string[];
}

export interface WizardPlan {
  templateId: string;
  pages: WizardPage[];
  revision: number;
}

export interface WizardValues {
  sourceMode?: "catalog" | "externalCatalog" | "github" | "localTemplate" | "existingProject";
  projectPath?: string;
  environmentName?: string;
  subscriptionId?: string;
  location?: string;
  tenantId?: string;
  [key: string]: string | boolean | string[] | undefined;
}

export interface OperationEvent {
  operationId: string;
  kind: string;
  state: OperationState;
  timestamp: string;
  stream?: "stdout" | "stderr" | "system";
  level: "trace" | "info" | "warning" | "error" | "success";
  message: string;
  progress?: number;
  outcome?: OperationOutcome;
}

export interface OperationOutcome {
  resourceState: "unchanged" | "changed" | "possiblyModified" | "unknown";
  summary: string;
  guidance: string;
  retryable: boolean;
}

export interface ProjectSummary {
  id: string;
  name: string;
  path: string;
  environment?: string;
  templateId?: string;
  revision?: string;
  lastOperation?: string;
  state: "ready" | "attention" | "deploying";
}

export interface ProjectEnvironment {
  name: string;
  dotEnvPath: string;
  hasLocal: boolean;
  hasRemote: boolean;
  isDefault: boolean;
}

export interface ProjectEnvironmentValue {
  name: string;
  value?: string;
  isSet: boolean;
  sensitive: boolean;
  declared: boolean;
  required: boolean;
}

export interface ProjectEnvironmentValueInput {
  name: string;
  value: string;
}

export interface AzdProjectState {
  name?: string;
  services?: Record<string, unknown>;
  [key: string]: unknown;
}

export interface AppSettings {
  theme: "system" | "dark" | "light";
  navigationExpanded: boolean;
}

export interface BootstrapData {
  catalog: Catalog;
  catalogSources: CatalogSourceSummary[];
  tools: ToolStatus[];
  connections: ConnectionStatus[];
  projects: ProjectSummary[];
  settings: AppSettings;
  platform: Platform;
  appVersion: string;
  isNative: boolean;
}

export interface PrepareDeploymentRequest {
  templateId: string;
  sourceMode: string;
  sourceTrustAccepted: boolean;
  sourcePath?: string;
  sourceRevision?: string;
  destination: string;
  environment: string;
  tenant?: string;
  subscription?: string;
  location: string;
  resourceGroup?: string;
  environmentValues: Record<string, string | boolean | string[] | undefined>;
}

export interface TemplateInspection {
  entry: CatalogEntry;
  sourcePath?: string;
  azureYamlFound: boolean;
  manifestSource: "catalog" | "embedded" | "inferred";
  interactiveHooks: boolean;
  warnings: string[];
}

export interface ExternalTerminalLaunch {
  workingDirectory: string;
  command: string;
}
