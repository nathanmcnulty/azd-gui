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
  requiresReview?: boolean;
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
  manifestSha256?: string;
  source?: "bundled" | "external" | "local";
  trusted?: boolean;
}

export interface TemplateManifest {
  schemaVersion: "1.0";
  minimumAzdVersion?: string;
  requiresInteractiveTerminal?: boolean;
  prerequisites: Prerequisite[];
  connections: ConnectionRequirement[];
  configuration: {
    groups: ConfigurationGroup[];
  };
  recommendationProfiles?: RecommendationProfile[];
  permissionChecks?: PermissionCheck[];
}

export interface RecommendationProfile {
  id: string;
  title: string;
  description: string;
  values: Record<string, string | boolean | string[]>;
}

export interface Prerequisite {
  condition?: RequirementCondition;
  id: string;
  name: string;
  kind: "tool" | "powershellModule";
  minimumVersion?: string;
  required: boolean;
  installProvider?: "azdTool" | "psResource" | "manual";
  installId?: string;
  documentationUrl?: string;
}

export interface RequirementCondition {
  field: string;
  equals: boolean;
}

export interface ToolStatus extends Prerequisite {
  state: ToolState;
  installedVersion?: string;
  executablePath?: string;
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
  exactVersion: string;
  command: string;
  documentationUrl: string;
}

export interface ToolInstallApprovalGrant {
  token: string;
}

export interface ConnectionRequirement {
  condition?: RequirementCondition;
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

export interface ConnectionTarget {
  tenantId: string;
  subscriptionId: string;
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
  lookup?: "securityGroup";
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

export interface SecurityGroupSearch {
  tenantId: string;
  subscriptionId: string;
  groups: { id: string; displayName: string }[];
  hasMore: boolean;
}

export interface PermissionCheck {
  condition?: RequirementCondition;
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
  environmentBinding?: "current" | "other" | "unbound";
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

export interface ProjectTemplateSnapshot {
  entry: CatalogEntry;
  digest: string;
}

export type ValidationReportState = "available" | "notFound" | "invalid" | "futureVersion" | "unsafePath" | "tooLarge";
export type EvidenceBindingState = "current" | "stale" | "mismatch" | "legacyUnbound" | "insufficientContext";
export type EvidenceActionCode = "reviewWarnings" | "retryOperation" | "verifyResources" | "verifyPermissions" | "proveDelivery" | "completeCleanup";
export type EvidenceActionOwner = "operator" | "subscriptionOwner" | "identityAdmin" | "messagingAdmin" | "applicationOwner" | "templateMaintainer";
export type EvidenceActionPriority = "required" | "recommended";

export interface EvidenceNextAction {
  code: EvidenceActionCode;
  owner: EvidenceActionOwner;
  priority: EvidenceActionPriority;
}

export interface DeploymentReceiptRead {
  state: ValidationReportState;
  receipt?: {
    generatedAt: string;
    mode: "plan" | "enforce";
    summary: { applied: number; unchanged: number; skipped: number; warnings: number; failed: number };
    artifactCount: number;
    outstandingActionCount: number;
    bindingState: EvidenceBindingState;
    nextActions: EvidenceNextAction[];
  };
}
export type ValidationMode = "plan" | "verify" | "delivery";
export type ValidationOutcome = "planned" | "passed" | "passedWithWarnings" | "failed";
export type ValidationPhase = "context" | "infrastructure" | "identity" | "configuration" | "security" | "runtime" | "delivery";
export type ValidationStatus = "pass" | "fail" | "warning" | "info" | "skipped" | "planned";
export type ValidationSideEffect = "none" | "readOnly" | "negativeProbe" | "syntheticDelivery";
export type ValidationEnvironmentBinding = "environmentMatch" | "environmentMismatch" | "environmentUnbound";

export interface ValidationSummary {
  pass: number;
  fail: number;
  warning: number;
  info: number;
  skipped: number;
  planned: number;
}

export interface ValidationCheck {
  phase: ValidationPhase;
  status: ValidationStatus;
  sideEffect: ValidationSideEffect;
  durationMs: number;
}

export interface ValidationReport {
  startedAt: string;
  completedAt: string;
  durationMs: number;
  mode: ValidationMode;
  outcome: ValidationOutcome;
  summary: ValidationSummary;
  environmentBinding: ValidationEnvironmentBinding;
  bindingState: EvidenceBindingState;
  nextActions: EvidenceNextAction[];
  checks: ValidationCheck[];
}

export interface ValidationReportRead {
  state: ValidationReportState;
  report?: ValidationReport;
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
  executionApprovalToken: string;
  sourcePath?: string;
  sourceRevision?: string;
  sourceBinding?: LocalSourceBinding;
  reviewedContractDigest: string;
  destination: string;
  environment: string;
  tenant?: string;
  subscription?: string;
  location: string;
  resourceGroup?: string;
  environmentValues: Record<string, string | boolean | string[] | undefined>;
}

export interface ExecutionApprovalRequest {
  action: "prepare" | "preview" | "up" | "provision" | "deploy" | "down";
  projectPath: string;
  environment: string;
  templateId: string;
  revision?: string;
  snapshotDigest?: string;
  sourceBinding?: LocalSourceBinding;
  target?: ExecutionTarget;
}

export interface ExecutionTarget {
  tenantId?: string;
  subscriptionId?: string;
  location?: string;
  resourceGroup?: string;
}

export interface ExecutionApprovalGrant {
  token: string;
  projectPath: string;
}

export interface TemplateInspection {
  entry: CatalogEntry;
  sourcePath?: string;
  sourceBinding?: LocalSourceBinding;
  azureYamlFound: boolean;
  manifestSource: "catalog" | "embedded" | "inferred";
  interactiveHooks: boolean;
  warnings: string[];
}

export interface LocalSourceBinding {
  sourcePath: string;
  contentDigest: string;
}

export interface ExternalTerminalLaunch {
  workingDirectory: string;
  command: string;
}
