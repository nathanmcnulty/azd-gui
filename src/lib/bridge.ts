import type { ComponentProvenanceRead } from "./component-provenance";
import { invoke, isTauri } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { open } from "@tauri-apps/plugin-dialog";
import { openUrl } from "@tauri-apps/plugin-opener";
import {
  mockAzureDiscovery,
  mockBootstrap,
  mockOperationHistory,
  mockProjectEnvironments,
  mockProjectEnvironmentValues
} from "./mock-data";
import type {
  AppSettings,
  AzureDiscovery,
  AzdProjectState,
  BootstrapData,
  Catalog,
  CatalogEntry,
  DeploymentReceiptRead,
  ConnectionStatus,
  ConnectionTarget,
  ProfileProvider,
  VisibleProfiles,
  ExternalCatalogPreview,
  ExecutionApprovalGrant,
  ExecutionApprovalRequest,
  ExternalTerminalLaunch,
  OperationEvent,
  PermissionCheckRequest,
  PermissionCheckResult,
  ProjectEnvironment,
  ProjectEnvironmentValue,
  ProjectEnvironmentValueInput,
  ProjectSummary,
  ProjectTemplateSnapshot,
  PrepareDeploymentRequest,
  TemplateInspection,
  ToolInstallApprovalGrant,
  ToolInstallPlan,
  ToolStatus,
  TemplateManifest,
  ValidationReportRead,
  SecurityGroupSearch
} from "./types";

let previewProjectEnvironments = structuredClone(mockProjectEnvironments);

export async function searchSecurityGroups(target: ConnectionTarget, query: string): Promise<SecurityGroupSearch> {
  if (!isTauri()) {
    const groups = [{ id: "a1111111-1111-4111-8111-111111111111", displayName: "Azure dashboard admins (preview)" }];
    return { ...target, groups: groups.filter((group) => group.displayName.toLowerCase().startsWith(query.trim().toLowerCase())), hasMore: false };
  }
  return invoke<SecurityGroupSearch>("search_security_groups", { ...target, query });
}
const previewInspectedEntries = new Map<string, ProjectTemplateSnapshot["entry"]>();
const previewProjectTemplates = new Map<string, ProjectTemplateSnapshot>();
let previewCatalogReview: (ExternalCatalogPreview & { sourceUrl: string }) | null = null;
function createPreviewEnvironmentValueState(): Record<string, ProjectEnvironmentValue[]> {
  return {
    dev: structuredClone(mockProjectEnvironmentValues),
    test: structuredClone(mockProjectEnvironmentValues)
  };
}
let previewProjectEnvironmentValues = createPreviewEnvironmentValueState();

const previewSettingsKey = "azd-gui.settings";

function readPreviewSettings(): AppSettings {
  try {
    const stored = window.localStorage.getItem(previewSettingsKey);
    if (!stored) return structuredClone(mockBootstrap.settings);
    const value = JSON.parse(stored) as Partial<AppSettings>;
    if (
      !["system", "dark", "light"].includes(value.theme ?? "") ||
      typeof value.navigationExpanded !== "boolean"
    ) {
      return structuredClone(mockBootstrap.settings);
    }
    return value as AppSettings;
  } catch {
    return structuredClone(mockBootstrap.settings);
  }
}

export async function bootstrap(): Promise<BootstrapData> {
  if (!isTauri()) {
    previewProjectEnvironments = structuredClone(mockProjectEnvironments);
    previewProjectEnvironmentValues = createPreviewEnvironmentValueState();
    previewInspectedEntries.clear();
    previewProjectTemplates.clear();
    return { ...structuredClone(mockBootstrap), settings: readPreviewSettings() };
  }
  return invoke<BootstrapData>("bootstrap");
}

export async function saveAppSettings(settings: AppSettings): Promise<void> {
  if (!isTauri()) {
    window.localStorage.setItem(previewSettingsKey, JSON.stringify(settings));
    return;
  }
  await invoke("save_app_settings", { settings });
}

export async function clearLocalAppData(): Promise<boolean> {
  if (!isTauri()) {
    window.localStorage.removeItem(previewSettingsKey);
    previewInspectedEntries.clear();
    previewProjectTemplates.clear();
    return true;
  }
  return invoke<boolean>("clear_local_app_data");
}

export async function chooseFolder(): Promise<string | null> {
  if (!isTauri()) return "C:\\GitHub\\sample-azd-project";
  const selected = await open({ directory: true, multiple: false, title: "Select an azd project or template" });
  return typeof selected === "string" ? selected : null;
}

export async function inspectLocalTemplate(path: string): Promise<TemplateInspection> {
  if (!isTauri()) {
    const entry = structuredClone(
      mockBootstrap.catalog.entries.find((candidate) => candidate.id === "maester-automation")!
    );
    entry.source = "local";
    entry.trusted = false;
    entry.commit = "local";
    previewInspectedEntries.set(entry.id, structuredClone(entry));
    return {
      entry,
      sourcePath: path,
      azureYamlFound: true,
      manifestSource: "inferred",
      interactiveHooks: true,
      warnings: [
        "Local templates require review. Review hooks before deploying.",
        "Interactive hooks were detected. Use the external terminal if input is required."
      ]
    };
  }
  return invoke<TemplateInspection>("inspect_local_template", { path });
}

export async function inspectGitHubTemplate(url: string): Promise<TemplateInspection> {
  if (!isTauri()) {
    const entry = structuredClone(
      mockBootstrap.catalog.entries.find((candidate) => candidate.id === "maester-automation")!
    );
    entry.id = "github-template";
    entry.title = url.split("/").filter(Boolean).at(-1) ?? "GitHub template";
    entry.repository = url;
    entry.commit = "0123456789abcdef0123456789abcdef01234567";
    entry.source = "external";
    entry.trusted = false;
    previewInspectedEntries.set(entry.id, structuredClone(entry));
    return {
      entry,
      azureYamlFound: true,
      manifestSource: "inferred",
      interactiveHooks: false,
      warnings: ["This repository requires review. Review the pinned revision before deployment."]
    };
  }
  return invoke<TemplateInspection>("inspect_github_template", { url });
}

export async function previewExternalCatalog(url: string): Promise<ExternalCatalogPreview> {
  if (!isTauri()) {
    const catalog = structuredClone(mockBootstrap.catalog);
    catalog.id = "external-preview";
    catalog.title = "External catalog preview";
    catalog.entries = catalog.entries.map((entry) => ({
      ...entry,
      id: `external-${catalog.id}.${entry.id}`,
      source: "external",
      trusted: false
    }));
    previewCatalogReview = {
      catalog,
      reviewToken: `preview-${catalog.id}`,
      sourceUrl: url
    };
    return {
      catalog: structuredClone(catalog),
      reviewToken: previewCatalogReview.reviewToken
    };
  }
  return invoke<ExternalCatalogPreview>("preview_external_catalog", { url });
}

export async function saveCatalogSource(url: string, reviewToken: string): Promise<Catalog> {
  if (!isTauri()) {
    const review = previewCatalogReview;
    previewCatalogReview = null;
    if (
      !review ||
      review.reviewToken !== reviewToken ||
      review.sourceUrl !== url
    ) {
      throw new Error("Catalog review expired. Preview the source again.");
    }
    return structuredClone(review.catalog);
  }
  return invoke<Catalog>("save_catalog_source", { url, reviewToken });
}

export async function removeCatalogSource(id: string): Promise<void> {
  if (!isTauri()) return;
  await invoke("remove_catalog_source", { id });
}

export async function refreshTools(): Promise<ToolStatus[]> {
  if (!isTauri()) return structuredClone(mockBootstrap.tools);
  return invoke<ToolStatus[]>("check_tools");
}

export async function checkTemplatePrerequisites(
  manifest: TemplateManifest,
  environmentValues: Record<string, string> = {}
): Promise<ToolStatus[]> {
  if (!isTauri()) {
    return manifest.prerequisites.filter((prerequisite) => {
      if (!prerequisite.condition) return true;
      const field = manifest.configuration.groups.flatMap((group) => group.fields).find((field) => field.id === prerequisite.condition!.field);
      const value = field ? environmentValues[field.environmentVariable] ?? String(field.default) : "invalid";
      return !["true", "false"].includes(value) || value === String(prerequisite.condition.equals);
    }).map((prerequisite) => {
      const tool = mockBootstrap.tools.find((candidate) => candidate.id === prerequisite.id);
      return tool
        ? { ...tool, ...prerequisite }
        : ({ ...prerequisite, state: "unknown", canInstall: false } as ToolStatus);
    });
  }
  return invoke<ToolStatus[]>("check_template_prerequisites", { manifest, environmentValues });
}

export async function listProjects(): Promise<ProjectSummary[]> {
  if (!isTauri()) return structuredClone(mockBootstrap.projects);
  return invoke<ProjectSummary[]>("list_projects");
}

export async function previewToolInstall(id: string): Promise<ToolInstallPlan> {
  if (!isTauri()) {
    const tool = mockBootstrap.tools.find((candidate) => candidate.id === id);
    if (!tool?.canInstall || !["Az.Accounts", "Microsoft.Graph"].includes(id)) {
      throw new Error(`${id} has no vetted non-elevated installation strategy.`);
    }
    const exactVersion = tool.minimumVersion ?? "1.0.0";
    const script = `Install-PSResource ${id} -Version '${exactVersion}' -Repository PSGallery -Scope CurrentUser -TrustRepository -Quiet -AcceptLicense`;
    return {
      id,
      toolName: tool.name,
      provider: "PSResourceGet",
      action: `Install or update the ${tool.name} module`,
      source: "PowerShell Gallery (PSGallery)",
      scope: "Current user",
      destination: "Current-user PowerShell module path",
      exactVersion,
      command: `pwsh -NoLogo -NoProfile -NonInteractive -Command "${script}"`,
      documentationUrl:
        "https://learn.microsoft.com/powershell/module/microsoft.powershell.psresourceget/install-psresource?view=powershellget-3.x"
    };
  }
  return invoke<ToolInstallPlan>("preview_tool_install", { id });
}

export async function requestToolInstallApproval(
  id: string,
  exactVersion: string
): Promise<ToolInstallApprovalGrant> {
  if (!isTauri()) return { token: `mock-tool-approval-${id}-${exactVersion}` };
  return invoke<ToolInstallApprovalGrant>("request_tool_install_approval", { id, exactVersion });
}

export async function installTool(
  id: string,
  exactVersion: string,
  approvalToken: string
): Promise<string> {
  if (!isTauri()) return `mock-install-${id}`;
  return invoke<string>("install_tool", { id, exactVersion, approvalToken });
}

export async function refreshConnections(target?: ConnectionTarget): Promise<ConnectionStatus[]> {
  if (!isTauri()) return structuredClone(mockBootstrap.connections);
  return invoke<ConnectionStatus[]>("check_connections", { target });
}

export async function listVisibleProfiles(providerId: ProfileProvider): Promise<VisibleProfiles> {
  if (!isTauri()) return { providerId, state: "available", profiles: [], hasMore: false };
  return invoke<VisibleProfiles>("list_visible_profiles", { providerId });
}

export async function discoverAzureScope(
  subscriptionId?: string,
  refresh = false
): Promise<AzureDiscovery> {
  if (!isTauri()) {
    const discovery = structuredClone(mockAzureDiscovery);
    if (!subscriptionId) {
      discovery.locations = [];
      discovery.resourceGroups = [];
      delete discovery.selectedSubscriptionId;
      return discovery;
    }
    const selected = discovery.subscriptions.find(
      (subscription) => subscription.id.toLowerCase() === subscriptionId.toLowerCase()
    );
    if (!selected) throw new Error("The selected subscription is unavailable.");
    discovery.selectedSubscriptionId = selected.id;
    return discovery;
  }
  return invoke<AzureDiscovery>("discover_azure_scope", { subscriptionId, refresh });
}

export async function runPermissionChecks(
  request: PermissionCheckRequest
): Promise<PermissionCheckResult[]> {
  if (!isTauri()) {
    return request.checks.map((check) => ({
      ...check,
      state: check.id === "entra-directory-access" ? "warning" : "passed",
      detail:
        check.id === "azure-subscription-access"
          ? "Azure CLI completed a live ARM read of the selected enabled subscription in the selected tenant."
          : check.id === "graph-delegated-scopes"
            ? "Microsoft Graph PowerShell has every manifest-requested delegated scope."
            : check.id === "entra-directory-access"
              ? "Baseline directory access is available. Write privileges cannot be verified safely without changing the directory."
              : "Azure CLI can query Microsoft Graph for the selected tenant.",
      blocking: false
    }));
  }
  return invoke<PermissionCheckResult[]>("run_permission_checks", { request });
}

export async function login(
  providerId: ConnectionStatus["id"],
  scopes: string[] = [],
  target?: ConnectionTarget
): Promise<string> {
  if (!isTauri()) return `mock-login-${providerId}`;
  return invoke<string>("start_login", { providerId, scopes, target });
}

export async function startAzdOperation(
  action: string,
  projectPath: string,
  environment?: string,
  executionApprovalToken?: string
): Promise<string> {
  if (!isTauri()) return `mock-${action}-${Date.now()}`;
  return invoke<string>("start_azd_operation", { action, projectPath, environment, executionApprovalToken });
}

export async function requestExecutionApproval(
  request: ExecutionApprovalRequest
): Promise<ExecutionApprovalGrant> {
  if (!isTauri()) return { token: `mock-approval-${Date.now()}`, projectPath: request.projectPath };
  return invoke<ExecutionApprovalGrant>("request_execution_approval", { request });
}

export async function openExternalTerminal(
  projectPath: string,
  environment?: string
): Promise<ExternalTerminalLaunch> {
  if (!isTauri()) {
    return {
      workingDirectory: projectPath,
      command: environment ? `azd up --environment ${environment}` : "azd up"
    };
  }
  return invoke<ExternalTerminalLaunch>("open_external_terminal", { projectPath, environment });
}

export async function prepareDeployment(request: PrepareDeploymentRequest): Promise<ProjectSummary> {
  if (!isTauri()) {
    const entry = previewInspectedEntries.get(request.templateId)
      ?? mockBootstrap.catalog.entries.find((candidate) => candidate.id === request.templateId);
    if (entry) previewProjectTemplates.set(request.destination, { entry: structuredClone(entry), digest: "browser-preview" });
    return {
      id: `${request.templateId}-${request.environment}`,
      name: request.templateId,
      path: request.destination || "C:\\Deployments\\preview",
      environment: request.environment,
      templateId: request.templateId,
      revision: "preview",
      lastOperation: "Just now",
      state: "ready"
    };
  }
  return invoke<ProjectSummary>("prepare_deployment", { request });
}

export async function getReviewedContractDigest(entry: CatalogEntry): Promise<string> {
  if (!isTauri()) return "browser-preview";
  return invoke<string>("reviewed_contract_digest", { entry });
}

export async function getProjectTemplate(projectPath: string): Promise<ProjectTemplateSnapshot | null> {
  if (!isTauri()) {
    const saved = previewProjectTemplates.get(projectPath);
    if (saved) return structuredClone(saved);
    const project = mockBootstrap.projects.find((candidate) => candidate.path === projectPath);
    const entry = mockBootstrap.catalog.entries.find((candidate) => candidate.id === project?.templateId);
    return entry ? { entry: structuredClone(entry), digest: "browser-preview" } : null;
  }
  return invoke<ProjectTemplateSnapshot | null>("get_project_template", { projectPath });
}

export async function refreshProjectTemplate(projectPath: string): Promise<ProjectTemplateSnapshot> {
  if (!isTauri()) {
    const project = mockBootstrap.projects.find((candidate) => candidate.path === projectPath);
    const entry = mockBootstrap.catalog.entries.find((candidate) => candidate.id === project?.templateId && candidate.commit === project?.revision);
    if (!entry || !entry.id.startsWith("external-")) throw new Error("Review the exact catalog template and revision first.");
    const snapshot = { entry: structuredClone(entry), digest: "browser-preview" };
    previewProjectTemplates.set(projectPath, snapshot);
    return snapshot;
  }
  return invoke<ProjectTemplateSnapshot>("refresh_project_template", { projectPath });
}

export async function saveWizardDraft(
  templateId: string,
  values: Record<string, string | boolean | string[] | undefined>,
  identity: CatalogEntry
): Promise<void> {
  if (!isTauri()) return;
  await invoke("save_wizard_draft", { templateId, values, identity });
}

export async function loadWizardDraft(
  templateId: string,
  identity: CatalogEntry
): Promise<Record<string, string | boolean | string[]> | null> {
  if (!isTauri()) return null;
  return invoke("load_wizard_draft", { templateId, identity });
}

export async function getProjectState(
  projectPath: string,
  environment?: string
): Promise<AzdProjectState> {
  if (!isTauri()) {
    return {
      name: projectPath.split(/[\\/]/).filter(Boolean).at(-1),
      services: {
        web: {
          project: { path: "./src/web", language: "node" },
          target: {
            resourceIds: [
              "/subscriptions/11111111-1111-1111-1111-111111111111/resourceGroups/rg-preview/providers/Microsoft.Web/sites/preview"
            ]
          },
          ingressUrl: "https://preview.example.azurewebsites.net"
        }
      },
      environment: environment ?? "dev"
    };
  }
  return invoke<AzdProjectState>("get_project_state", { projectPath, environment });
}

export async function getProjectValidationReport(projectPath: string): Promise<ValidationReportRead> {
  if (!isTauri()) return { state: "notFound" };
  return invoke<ValidationReportRead>("get_project_validation_report", { projectPath });
}

export async function getProjectComponentProvenance(projectPath: string): Promise<ComponentProvenanceRead> {
  if (!isTauri()) return { state: "notFound", unknownMetadata: false, components: [] };
  return invoke<ComponentProvenanceRead>("get_project_component_provenance", { projectPath });
}

export async function getProjectDeploymentReceipt(projectPath: string): Promise<DeploymentReceiptRead> {
  if (!isTauri()) return { state: "notFound" };
  return invoke<DeploymentReceiptRead>("get_project_deployment_receipt", { projectPath });
}

export async function openExternalUrl(url: string): Promise<void> {
  const parsed = new URL(url);
  if (parsed.protocol !== "https:") throw new Error("Only HTTPS links can be opened.");
  if (!isTauri()) {
    window.open(parsed.toString(), "_blank", "noopener,noreferrer");
    return;
  }
  await openUrl(parsed.toString());
}

export async function listProjectEnvironments(
  projectPath: string
): Promise<ProjectEnvironment[]> {
  if (!isTauri()) return structuredClone(previewProjectEnvironments);
  return invoke<ProjectEnvironment[]>("list_project_environments", { projectPath });
}

export async function createProjectEnvironment(
  projectPath: string,
  environment: string,
  subscription?: string,
  location?: string
): Promise<ProjectEnvironment[]> {
  if (!isTauri()) {
    if (previewProjectEnvironments.some((item) => item.name === environment)) {
      throw new Error(`Environment '${environment}' already exists.`);
    }
    previewProjectEnvironments = [
      ...previewProjectEnvironments.map((item) => ({ ...item, isDefault: false })),
      {
        name: environment,
        dotEnvPath: `${projectPath}\\.azure\\${environment}\\.env`,
        hasLocal: true,
        hasRemote: false,
        isDefault: true
      }
    ];
    previewProjectEnvironmentValues[environment] = mockProjectEnvironmentValues
      .filter((value) => value.declared)
      .map((value) => ({ ...value, value: undefined, isSet: false }));
    return structuredClone(previewProjectEnvironments);
  }
  return invoke<ProjectEnvironment[]>("create_project_environment", {
    projectPath,
    environment,
    subscription,
    location
  });
}

export async function selectProjectEnvironment(
  projectPath: string,
  environment: string
): Promise<ProjectEnvironment[]> {
  if (!isTauri()) {
    if (!previewProjectEnvironments.some((item) => item.name === environment)) {
      throw new Error(`Environment '${environment}' does not exist.`);
    }
    previewProjectEnvironments = previewProjectEnvironments.map((item) => ({
      ...item,
      isDefault: item.name === environment
    }));
    return structuredClone(previewProjectEnvironments);
  }
  return invoke<ProjectEnvironment[]>("select_project_environment", { projectPath, environment });
}

export async function getProjectEnvironmentValues(
  projectPath: string,
  environment: string
): Promise<ProjectEnvironmentValue[]> {
  if (!isTauri()) {
    return structuredClone(
      previewProjectEnvironmentValues[environment] ??
        mockProjectEnvironmentValues
          .filter((value) => value.declared)
          .map((value) => ({ ...value, value: undefined, isSet: false }))
    );
  }
  return invoke<ProjectEnvironmentValue[]>("get_project_environment_values", {
    projectPath,
    environment
  });
}

export async function setProjectEnvironmentValues(
  projectPath: string,
  environment: string,
  values: ProjectEnvironmentValueInput[]
): Promise<void> {
  if (!isTauri()) {
    const environmentValues =
      previewProjectEnvironmentValues[environment] ??
      mockProjectEnvironmentValues
        .filter((value) => value.declared)
        .map((value) => ({ ...value, value: undefined, isSet: false }));
    for (const value of values) {
      const existing = environmentValues.find((item) => item.name === value.name);
      if (existing) {
        existing.isSet = true;
        existing.value = existing.sensitive ? undefined : value.value;
      } else {
        environmentValues.push({
          name: value.name,
          isSet: true,
          sensitive: true,
          declared: false,
          required: false
        });
      }
    }
    previewProjectEnvironmentValues[environment] = environmentValues;
    return;
  }
  await invoke("set_project_environment_values", { projectPath, environment, values });
}

export async function getOperationHistory(projectPath: string, environment?: string): Promise<OperationEvent[]> {
  if (!isTauri()) return structuredClone(mockOperationHistory);
  return invoke<OperationEvent[]>("get_operation_history", { projectPath, environment });
}

export async function cancelOperation(operationId: string): Promise<void> {
  if (!isTauri()) return;
  await invoke("cancel_operation", { operationId });
}

export async function onOperationEvent(handler: (event: OperationEvent) => void): Promise<UnlistenFn> {
  if (!isTauri()) return () => undefined;
  return listen<OperationEvent>("operation-event", ({ payload }) => handler(payload));
}

export async function getProjectReportArtifacts(projectPath: string, revealId: string | null = null, snapshotDigest: string | null = null): Promise<import("./report-artifacts").ArtifactRead> {
  if (!isTauri()) return { context: null, observedAt: "", artifacts: [] };
  return invoke("get_project_report_artifacts", { projectPath, revealId, snapshotDigest });
}
