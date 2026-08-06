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

export const bundledCatalog: Catalog = {
  schemaVersion: "1.0",
  id: "bundled",
  title: "Vetted deployments",
  entries: [
    {
      id: "myworkid",
      title: "MyWorkID",
      summary: "Discover and secure identities that can access Microsoft 365 and Azure.",
      description:
        "A cross-platform Azure deployment with conditional collection, reporting, and Microsoft Graph options.",
      repository: "https://github.com/nathanmcnulty/azd-myworkid",
      commit: "913be87ebf9a569349846c3a6d9be7f4d2fdcd49",
      tags: ["Identity", "Microsoft Graph", "App Service"],
      platforms: ["windows", "macos", "linux"],
      source: "bundled",
      trusted: true,
      manifest: {
        schemaVersion: "1.0",
        minimumAzdVersion: "1.23.7",
        prerequisites: [
          {
            id: "azd",
            name: "Azure Developer CLI",
            kind: "tool",
            minimumVersion: "1.23.7",
            required: true,
            installProvider: "manual",
            documentationUrl: "https://learn.microsoft.com/azure/developer/azure-developer-cli/install-azd"
          },
          {
            id: "pwsh",
            name: "PowerShell",
            kind: "tool",
            minimumVersion: "7.4.0",
            required: true,
            installProvider: "manual",
            documentationUrl: "https://learn.microsoft.com/powershell/scripting/install/installing-powershell"
          }
        ],
        connections: [
          {
            id: "azd",
            name: "Azure Developer CLI",
            required: true,
            scopes: ["https://management.azure.com/.default", "https://graph.microsoft.com/.default"],
            reason:
              "Provision Azure resources and let the template's vetted hooks configure Microsoft Entra through azd-owned credentials."
          }
        ],
        configuration: {
          groups: [
            {
              id: "deployment",
              title: "Deployment package",
              description: "Use a published MyWorkID release or build the checked-in source.",
              fields: [
                {
                  id: "deploymentMode",
                  environmentVariable: "MYWORKID_DEPLOY_MODE",
                  label: "Deployment mode",
                  type: "select",
                  required: true,
                  default: "releaseZip",
                  options: [
                    {
                      label: "Published release",
                      value: "releaseZip",
                      description: "Download the upstream binaries without requiring .NET or Node.js."
                    },
                    {
                      label: "Build source locally",
                      value: "sourceBuild",
                      description: "Build the checked-in client and server; requires .NET 8, Node.js, and npm."
                    }
                  ]
                }
              ]
            },
            {
              id: "release",
              title: "Published release",
              description: "Choose the upstream MyWorkID package used by the default deployment mode.",
              condition: { field: "deploymentMode", equals: "releaseZip" },
              fields: [
                {
                  id: "releaseVersion",
                  environmentVariable: "MYWORKID_RELEASE_VERSION",
                  label: "Release version",
                  type: "text",
                  required: true,
                  default: "latest",
                  help: "Use latest or an upstream MyWorkID release tag."
                }
              ]
            },
            {
              id: "applications",
              title: "Microsoft Entra applications",
              description:
                "Name the app registrations created by the template and optionally allow local development redirects.",
              fields: [
                {
                  id: "backendApplicationName",
                  environmentVariable: "MYWORKID_BACKED_APPREG_NAME",
                  label: "Backend app registration",
                  type: "text",
                  required: true,
                  default: "ar-MyWorkID-backend"
                },
                {
                  id: "frontendApplicationName",
                  environmentVariable: "MYWORKID_FRONTEND_APPREG_NAME",
                  label: "Frontend app registration",
                  type: "text",
                  required: true,
                  default: "ar-MyWorkID-frontend"
                },
                {
                  id: "developerRedirectUris",
                  environmentVariable: "MYWORKID_DEV_REDIRECT_URLS",
                  label: "Developer redirect URIs",
                  type: "text",
                  help: "Optional comma-separated HTTPS or localhost redirect URIs."
                }
              ]
            },
            {
              id: "customDomains",
              title: "Custom domains",
              description:
                "Optionally configure CNAME-based App Service domains. DNS completion requires a later provision pass.",
              fields: [
                {
                  id: "customDomains",
                  environmentVariable: "MYWORKID_CUSTOM_DOMAINS",
                  label: "Custom domain names",
                  type: "text",
                  help: "Optional comma-separated host names. Apex/root domains require manual setup."
                },
                {
                  id: "managedCertificate",
                  environmentVariable: "MYWORKID_ENABLE_APP_SERVICE_MANAGED_CERTIFICATE",
                  label: "Request App Service managed certificates",
                  type: "boolean",
                  default: true
                }
              ]
            }
          ]
        },
        permissionChecks: [
          {
            id: "azure-subscription-access",
            name: "Azure subscription access",
            provider: "azure",
            required: true,
            description: "Can enumerate and deploy resources in the selected subscription."
          },
          {
            id: "entra-directory-access",
            name: "Microsoft Entra directory access",
            provider: "graph",
            required: true,
            description:
              "Can create application objects, service principals, groups, grants, and the directory assignments required by the default template flow."
          }
        ]
      }
    },
    {
      id: "maester-automation",
      title: "Maester — Automation Account",
      summary: "Schedule Maester security tests with Azure Automation.",
      description:
        "One of several deployment choices in the Maester repository, pinned to the automation-account subdirectory.",
      repository: "https://github.com/nathanmcnulty/azd-maester",
      commit: "21b45a1599522ff05e0bdee337caaac71dfe3e24",
      subdirectory: "automation-account",
      tags: ["Maester", "Security", "Automation"],
      platforms: ["windows", "macos", "linux"],
      source: "bundled",
      trusted: true,
      manifest: {
        schemaVersion: "1.0",
        minimumAzdVersion: "1.23.0",
        prerequisites: [
          {
            id: "azd",
            name: "Azure Developer CLI",
            kind: "tool",
            minimumVersion: "1.23.0",
            required: true,
            installProvider: "manual"
          },
          {
            id: "Microsoft.Graph",
            name: "Microsoft Graph PowerShell",
            kind: "powershellModule",
            minimumVersion: "2.0.0",
            required: true,
            installProvider: "psResource",
            installId: "Microsoft.Graph"
          }
        ],
        connections: [
          {
            id: "azd",
            name: "Azure Developer CLI",
            required: true,
            reason: "Provision the Automation Account and supporting resources."
          },
          {
            id: "graphPowerShell",
            name: "Microsoft Graph PowerShell",
            required: true,
            scopes: ["Application.ReadWrite.All", "RoleManagement.ReadWrite.Directory"],
            reason: "Create and grant the identity used for scheduled Maester tests."
          }
        ],
        configuration: {
          groups: [
            {
              id: "schedule",
              title: "Test schedule",
              fields: [
                {
                  id: "schedule",
                  environmentVariable: "MAESTER_SCHEDULE",
                  label: "Run frequency",
                  type: "select",
                  required: true,
                  default: "daily",
                  options: [
                    { label: "Daily", value: "daily" },
                    { label: "Every 12 hours", value: "12h" },
                    { label: "Weekly", value: "weekly" }
                  ]
                }
              ]
            }
          ]
        },
        permissionChecks: [
          {
            id: "graph-delegated-scopes",
            name: "Microsoft Graph delegated scopes",
            provider: "graph",
            required: true,
            description:
              "The Microsoft Graph PowerShell session includes every delegated scope requested by this template."
          }
        ]
      }
    },
    {
      id: "entra-health-monitoring",
      title: "Entra Health Monitoring",
      summary: "Deploy tenant health collection and reporting resources.",
      description:
        "A Windows-specific deployment that uses Azure CLI to obtain Microsoft Graph access for setup hooks.",
      repository: "https://github.com/nathanmcnulty/azd-entra-health-monitoring",
      commit: "d197df8357b74326e88f9861cd2011d8d33f20e9",
      tags: ["Entra", "Health", "Monitoring"],
      platforms: ["windows"],
      source: "bundled",
      trusted: true,
      manifest: {
        schemaVersion: "1.0",
        minimumAzdVersion: "1.23.0",
        prerequisites: [
          {
            id: "azd",
            name: "Azure Developer CLI",
            kind: "tool",
            minimumVersion: "1.23.0",
            required: true,
            installProvider: "manual"
          },
          {
            id: "az-cli",
            name: "Azure CLI",
            kind: "tool",
            minimumVersion: "2.60.0",
            required: true,
            installProvider: "azdTool",
            installId: "az-cli",
            documentationUrl: "https://learn.microsoft.com/cli/azure/install-azure-cli"
          }
        ],
        connections: [
          {
            id: "azd",
            name: "Azure Developer CLI",
            required: true,
            reason: "Provision monitoring infrastructure."
          },
          {
            id: "azureCli",
            name: "Azure CLI",
            required: true,
            scopes: ["https://graph.microsoft.com/.default"],
            reason: "Allow setup hooks to configure Microsoft Graph resources."
          }
        ],
        configuration: {
          groups: [
            {
              id: "tenant",
              title: "Tenant monitoring",
              fields: [
                {
                  id: "targetTenant",
                  environmentVariable: "ENTRA_HEALTH_TARGET_TENANT_ID",
                  label: "Tenant to monitor",
                  type: "tenant",
                  required: true
                },
                {
                  id: "enableTeams",
                  environmentVariable: "ENTRA_HEALTH_ENABLE_TEAMS",
                  label: "Send Teams notifications",
                  type: "boolean",
                  default: false
                },
                {
                  id: "teamsWebhook",
                  environmentVariable: "ENTRA_HEALTH_TEAMS_WEBHOOK",
                  label: "Teams webhook",
                  type: "secret",
                  required: true,
                  sensitive: true,
                  condition: { field: "enableTeams", equals: true }
                }
              ]
            }
          ]
        },
        permissionChecks: [
          {
            id: "azure-subscription-access",
            name: "Azure subscription access",
            provider: "azure",
            required: true,
            description:
              "Azure CLI can complete a live ARM read of the selected enabled subscription in the selected tenant."
          },
          {
            id: "graph-directory-read",
            name: "Microsoft Graph directory access",
            provider: "graph",
            required: true,
            description:
              "Azure CLI can query Microsoft Graph for the selected tenant before setup hooks run."
          }
        ]
      }
    }
  ]
};

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
    subscriptionName: "Development Subscription"
  },
  {
    id: "azureCli",
    name: "Azure CLI",
    required: false,
    reason: "Template-specific Azure operations.",
    state: "connected",
    account: "nathan@example.com",
    tenantId: "72f988bf-86f1-41af-91ab-2d7cd011db47"
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
    state: "disconnected"
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
    revision: "913be87",
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
    name: "MAESTER_SCHEDULE",
    value: "daily",
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
  tools: mockTools,
  connections: mockConnections,
  projects: mockProjects,
  platform: "windows",
  appVersion: "0.1.0",
  isNative: false
};
