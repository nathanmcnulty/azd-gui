import type {
  AzureDiscovery,
  BootstrapData,
  Catalog,
  ConnectionStatus,
  OperationEvent,
  ProjectEnvironment,
  ProjectEnvironmentValue,
  ProjectSummary,
  ToolStatus
} from "./types";

import catalogDocument from "../../catalog/azd-gui.catalog.json";
import entraHealthManifest from "../../catalog/manifests/entra-health-monitoring.json";
import maesterManifest from "../../catalog/manifests/maester-automation.json";
import myworkidManifest from "../../catalog/manifests/myworkid.json";
import deploymentCheckManifest from "../../catalog/manifests/deployment-check.json";

const bundledManifests = {
  "manifests/myworkid.json": myworkidManifest,
  "manifests/deployment-check.json": deploymentCheckManifest,
  "manifests/maester-automation.json": maesterManifest,
  "manifests/entra-health-monitoring.json": entraHealthManifest
} as const;

export const bundledCatalog = {
  ...catalogDocument,
  entries: catalogDocument.entries.map((entry) => ({
    ...entry,
    manifest: bundledManifests[entry.manifest as keyof typeof bundledManifests],
    source: "bundled",
    trusted: true
  }))
} as unknown as Catalog;

export const mockTools: ToolStatus[] = [
  {
    id: "azd",
    name: "Azure Developer CLI",
    kind: "tool",
    minimumVersion: "1.23.7",
    installedVersion: "1.23.16",
    required: true,
    state: "ready",
    installProvider: "manual"
  },
  {
    id: "az-cli",
    name: "Azure CLI",
    kind: "tool",
    minimumVersion: "2.60.0",
    installedVersion: "2.88.0",
    latestVersion: "2.89.0",
    updateAvailable: true,
    required: false,
    state: "ready",
    installProvider: "azdTool",
    canInstall: false,
    installCommand: "azd tool upgrade az-cli --dry-run",
    documentationUrl:
      "https://learn.microsoft.com/azure/developer/azure-developer-cli/development-tools"
  },
  {
    id: "pwsh",
    name: "PowerShell",
    kind: "tool",
    minimumVersion: "7.4.0",
    installedVersion: "7.5.2",
    required: false,
    state: "ready",
    installProvider: "manual"
  },
  {
    id: "Microsoft.Graph",
    name: "Microsoft Graph PowerShell",
    kind: "powershellModule",
    minimumVersion: "2.0.0",
    required: false,
    state: "missing",
    installProvider: "psResource",
    installId: "Microsoft.Graph",
    canInstall: true,
    installCommand:
      'pwsh -NoLogo -NoProfile -NonInteractive -Command "Install-PSResource Microsoft.Graph -Scope CurrentUser -TrustRepository -Quiet -AcceptLicense"',
    documentationUrl:
      "https://learn.microsoft.com/powershell/module/microsoft.powershell.psresourceget/install-psresource?view=powershellget-3.x"
  }
];

export const mockConnections: ConnectionStatus[] = [
  {
    id: "azd",
    name: "Azure Developer CLI",
    required: true,
    reason: "Deploy Azure resources.",
    state: "connected",
    account: "nathan@example.com",
    tenantId: "72f988bf-86f1-41af-91ab-2d7cd011db47",
    subscriptionId: "11111111-1111-1111-1111-111111111111",
    subscriptionName: "Development Subscription",
    expiresOn: "2030-01-15T18:30:00Z"
  },
  {
    id: "azureCli",
    name: "Azure CLI",
    required: false,
    reason: "Template-specific Azure operations.",
    state: "connected",
    account: "nathan@example.com",
    tenantId: "72f988bf-86f1-41af-91ab-2d7cd011db47",
    subscriptionId: "11111111-1111-1111-1111-111111111111",
    subscriptionName: "Development Subscription"
  },
  {
    id: "azPowerShell",
    name: "Azure PowerShell",
    required: false,
    reason: "Template-specific PowerShell operations.",
    state: "disconnected"
  },
  {
    id: "graphPowerShell",
    name: "Microsoft Graph PowerShell",
    required: false,
    reason: "Template-specific Graph operations.",
    state: "connected",
    account: "nathan@example.com",
    tenantId: "72f988bf-86f1-41af-91ab-2d7cd011db47",
    grantedScopes: ["Application.ReadWrite.All"]
  }
];

export const mockAzureDiscovery: AzureDiscovery = {
  provider: "azureCli",
  tenants: [
    {
      tenantId: "72f988bf-86f1-41af-91ab-2d7cd011db47",
      displayName: "Contoso Development",
      defaultDomain: "contoso.onmicrosoft.com"
    },
    {
      tenantId: "00000000-0000-0000-0000-000000000002",
      displayName: "Sandbox Directory",
      defaultDomain: "sandbox.onmicrosoft.com"
    }
  ],
  subscriptions: [
    {
      id: "11111111-1111-1111-1111-111111111111",
      name: "Development Subscription",
      tenantId: "72f988bf-86f1-41af-91ab-2d7cd011db47",
      tenantDisplayName: "Contoso Development",
      tenantDefaultDomain: "contoso.onmicrosoft.com",
      isDefault: true
    },
    {
      id: "22222222-2222-2222-2222-222222222222",
      name: "Sandbox Subscription",
      tenantId: "00000000-0000-0000-0000-000000000002",
      tenantDisplayName: "Sandbox Directory",
      tenantDefaultDomain: "sandbox.onmicrosoft.com",
      isDefault: false
    }
  ],
  locations: [
    { name: "eastus", displayName: "East US", regionalDisplayName: "(US) East US" },
    { name: "westus2", displayName: "West US 2", regionalDisplayName: "(US) West US 2" },
    { name: "westeurope", displayName: "West Europe", regionalDisplayName: "(Europe) West Europe" }
  ],
  resourceGroups: [
    { name: "rg-azd-development", location: "westus2" },
    { name: "rg-shared-services", location: "eastus" }
  ],
  selectedSubscriptionId: "11111111-1111-1111-1111-111111111111",
  warnings: []
};

export const mockProjects: ProjectSummary[] = [
  {
    id: "myworkid-dev",
    name: "myworkid",
    path: "C:\\GitHub\\azd-myworkid",
    environment: "dev",
    templateId: "myworkid",
    revision: "0a6f0ae",
    lastOperation: "2 hours ago",
    state: "ready"
  },
  {
    id: "maester-lab",
    name: "maester-automation",
    path: "C:\\Deployments\\maester-lab",
    environment: "lab",
    templateId: "maester-automation",
    revision: "21b45a1",
    lastOperation: "Yesterday",
    state: "attention"
  }
];

export const mockProjectEnvironments: ProjectEnvironment[] = [
  {
    name: "dev",
    dotEnvPath: "C:\\Deployments\\maester-lab\\.azure\\dev\\.env",
    hasLocal: true,
    hasRemote: false,
    isDefault: true
  },
  {
    name: "test",
    dotEnvPath: "C:\\Deployments\\maester-lab\\.azure\\test\\.env",
    hasLocal: true,
    hasRemote: false,
    isDefault: false
  }
];

export const mockProjectEnvironmentValues: ProjectEnvironmentValue[] = [
  {
    name: "INCLUDE_WEB_APP",
    value: "false",
    isSet: true,
    sensitive: false,
    declared: true,
    required: false
  },
  {
    name: "INCLUDE_EXCHANGE",
    value: "true",
    isSet: true,
    sensitive: false,
    declared: true,
    required: false
  },
  {
    name: "INCLUDE_TEAMS",
    value: "true",
    isSet: true,
    sensitive: false,
    declared: true,
    required: false
  },
  {
    name: "INCLUDE_AZURE",
    value: "true",
    isSet: true,
    sensitive: false,
    declared: true,
    required: false
  },
  {
    name: "PERMISSION_PROFILE",
    value: "Extended",
    isSet: true,
    sensitive: false,
    declared: true,
    required: true
  },
  {
    name: "AZURE_LOCATION",
    isSet: true,
    sensitive: true,
    declared: false,
    required: false
  }
];

export const mockOperationHistory: OperationEvent[] = [
  {
    operationId: "preview-history-up",
    kind: "up",
    state: "succeeded",
    timestamp: "2026-08-05T18:42:00.000Z",
    stream: "system",
    level: "success",
    message: "Deployment completed successfully.",
    outcome: {
      resourceState: "changed",
      summary: "Deployment completed successfully.",
      guidance: "Refresh project state to inspect the resulting Azure resources.",
      retryable: false
    }
  },
  {
    operationId: "preview-history-provision",
    kind: "provision",
    state: "failed",
    timestamp: "2026-08-04T21:16:00.000Z",
    stream: "system",
    level: "error",
    message: "Provisioning did not complete.",
    outcome: {
      resourceState: "possiblyModified",
      summary: "Provisioning did not complete.",
      guidance: "Review the output and Azure state before retrying.",
      retryable: true
    }
  },
  {
    operationId: "preview-history-refresh",
    kind: "refresh",
    state: "succeeded",
    timestamp: "2026-08-03T16:05:00.000Z",
    stream: "system",
    level: "success",
    message: "Environment state refreshed.",
    outcome: {
      resourceState: "unchanged",
      summary: "Environment state refreshed.",
      guidance: "No Azure resource changes were applied by this action.",
      retryable: false
    }
  }
];

export const mockBootstrap: BootstrapData = {
  catalog: bundledCatalog,
  catalogSources: [],
  tools: mockTools,
  connections: mockConnections,
  projects: mockProjects,
  settings: {
    theme: "system",
    navigationExpanded: true
  },
  platform: "windows",
  appVersion: "0.1.1",
  isNative: false
};
