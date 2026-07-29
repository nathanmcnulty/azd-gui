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
  detail?: string;
  installCommand?: string;
  canInstall?: boolean;
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
  account?: string;
  tenantId?: string;
  expiresOn?: string;
  detail?: string;
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
  id: string;
  name: string;
  provider: "azure" | "graph";
  required: boolean;
  description: string;
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

export interface BootstrapData {
  catalog: Catalog;
  tools: ToolStatus[];
  connections: ConnectionStatus[];
  projects: ProjectSummary[];
  platform: Platform;
  appVersion: string;
  isNative: boolean;
}

export interface CatalogSource {
  id: string;
  label: string;
  url: string;
  trusted: boolean;
  pinnedRevision?: string;
  acceptedCatalog?: Catalog;
}

export interface PrepareDeploymentRequest {
  templateId: string;
  sourceMode: string;
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
