import { invoke, isTauri } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { open } from "@tauri-apps/plugin-dialog";
import {
  mockAzureDiscovery,
  mockBootstrap,
  mockOperationHistory,
  mockProjectEnvironments,
  mockProjectEnvironmentValues
} from "./mock-data";
import type {
  AzureDiscovery,
  BootstrapData,
  Catalog,
  CatalogSource,
  ConnectionStatus,
  OperationEvent,
  PermissionCheckRequest,
  PermissionCheckResult,
  ProjectEnvironment,
  ProjectEnvironmentValue,
  ProjectEnvironmentValueInput,
  ProjectSummary,
  PrepareDeploymentRequest,
  TemplateInspection,
  ToolStatus
} from "./types";

let previewProjectEnvironments = structuredClone(mockProjectEnvironments);
function createPreviewEnvironmentValueState(): Record<string, ProjectEnvironmentValue[]> {
  return {
    dev: structuredClone(mockProjectEnvironmentValues),
    test: structuredClone(mockProjectEnvironmentValues)
  };
}
let previewProjectEnvironmentValues = createPreviewEnvironmentValueState();

export async function bootstrap(): Promise<BootstrapData> {
  if (!isTauri()) {
    previewProjectEnvironments = structuredClone(mockProjectEnvironments);
    previewProjectEnvironmentValues = createPreviewEnvironmentValueState();
    return structuredClone(mockBootstrap);
  }
  return invoke<BootstrapData>("bootstrap");
}

export async function chooseFolder(): Promise<string | null> {
  if (!isTauri()) return "C:\\GitHub\\sample-azd-project";
  const selected = await open({ directory: true, multiple: false, title: "Select an azd project or template" });
  return typeof selected === "string" ? selected : null;
}

export async function inspectLocalTemplate(path: string): Promise<TemplateInspection> {
  if (!isTauri()) {
    const entry = structuredClone(mockBootstrap.catalog.entries[0]);
    entry.source = "local";
    entry.trusted = false;
    return {
      entry,
      sourcePath: path,
      azureYamlFound: true,
      manifestSource: "inferred",
      interactiveHooks: false,
      warnings: ["Local templates are unvetted. Review hooks before deploying."]
    };
  }
  return invoke<TemplateInspection>("inspect_local_template", { path });
}

export async function inspectGitHubTemplate(url: string): Promise<TemplateInspection> {
  if (!isTauri()) {
    const entry = structuredClone(mockBootstrap.catalog.entries[0]);
    entry.id = "github-template";
    entry.title = url.split("/").filter(Boolean).at(-1) ?? "GitHub template";
    entry.repository = url;
    entry.commit = "0123456789abcdef0123456789abcdef01234567";
    entry.source = "external";
    entry.trusted = false;
    return {
      entry,
      azureYamlFound: true,
      manifestSource: "inferred",
      interactiveHooks: false,
      warnings: ["This repository is unvetted. Review the pinned revision before deployment."]
    };
  }
  return invoke<TemplateInspection>("inspect_github_template", { url });
}

export async function previewExternalCatalog(url: string): Promise<Catalog> {
  if (!isTauri()) {
    const catalog = structuredClone(mockBootstrap.catalog);
    catalog.id = "external-preview";
    catalog.title = "External catalog preview";
    catalog.entries = catalog.entries.map((entry) => ({
      ...entry,
      source: "external",
      trusted: false
    }));
    return catalog;
  }
  return invoke<Catalog>("preview_external_catalog", { url });
}

export async function saveCatalogSource(source: CatalogSource): Promise<void> {
  if (!isTauri()) return;
  await invoke("save_catalog_source", { source });
}

export async function refreshTools(): Promise<ToolStatus[]> {
  if (!isTauri()) return structuredClone(mockBootstrap.tools);
  return invoke<ToolStatus[]>("check_tools");
}

export async function installTool(id: string): Promise<string> {
  if (!isTauri()) return `mock-install-${id}`;
  return invoke<string>("install_tool", { id });
}

export async function refreshConnections(): Promise<ConnectionStatus[]> {
  if (!isTauri()) return structuredClone(mockBootstrap.connections);
  return invoke<ConnectionStatus[]>("check_connections");
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
  deviceCode = false
): Promise<string> {
  if (!isTauri()) return `mock-login-${providerId}`;
  return invoke<string>("start_login", { providerId, scopes, deviceCode });
}

export async function startAzdOperation(
  action: string,
  projectPath: string,
  environment?: string
): Promise<string> {
  if (!isTauri()) return `mock-${action}-${Date.now()}`;
  return invoke<string>("start_azd_operation", { action, projectPath, environment });
}

export async function prepareDeployment(request: PrepareDeploymentRequest): Promise<ProjectSummary> {
  if (!isTauri()) {
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

export async function saveWizardDraft(
  templateId: string,
  values: Record<string, string | boolean | string[] | undefined>
): Promise<void> {
  if (!isTauri()) return;
  await invoke("save_wizard_draft", { templateId, values });
}

export async function loadWizardDraft(
  templateId: string
): Promise<Record<string, string | boolean | string[]> | null> {
  if (!isTauri()) return null;
  return invoke("load_wizard_draft", { templateId });
}

export async function getProjectState(
  projectPath: string,
  environment?: string
): Promise<Record<string, unknown>> {
  if (!isTauri()) {
    return {
      name: projectPath.split(/[\\/]/).filter(Boolean).at(-1),
      services: [
        {
          name: "web",
          endpoint: "https://preview.example.azurewebsites.net"
        }
      ],
      environment: environment ?? "dev"
    };
  }
  return invoke("get_project_state", { projectPath, environment });
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

export async function recordOperation(
  event: OperationEvent,
  projectPath: string
): Promise<void> {
  if (!isTauri()) return;
  await invoke("record_operation", { event, projectPath });
}

export async function getOperationHistory(projectPath: string): Promise<OperationEvent[]> {
  if (!isTauri()) return structuredClone(mockOperationHistory);
  return invoke<OperationEvent[]>("get_operation_history", { projectPath });
}

export async function cancelOperation(operationId: string): Promise<void> {
  if (!isTauri()) return;
  await invoke("cancel_operation", { operationId });
}

export async function onOperationEvent(handler: (event: OperationEvent) => void): Promise<UnlistenFn> {
  if (!isTauri()) return () => undefined;
  return listen<OperationEvent>("operation-event", ({ payload }) => handler(payload));
}
