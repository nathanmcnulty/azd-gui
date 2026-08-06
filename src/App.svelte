<script lang="ts">
  import { onMount } from "svelte";
  import Icon from "./components/Icon.svelte";
  import StatusPill from "./components/StatusPill.svelte";
  import {
    bootstrap,
    cancelOperation,
    chooseFolder,
    createProjectEnvironment,
    discoverAzureScope,
    inspectGitHubTemplate,
    inspectLocalTemplate,
    installTool,
    onOperationEvent,
    openExternalTerminal,
    openExternalUrl,
    previewToolInstall,
    refreshConnections,
    refreshTools,
    prepareDeployment,
    startAzdOperation,
    login,
    previewExternalCatalog,
    saveCatalogSource,
    loadWizardDraft,
    saveWizardDraft,
    getProjectState,
    getOperationHistory,
    getProjectEnvironmentValues,
    listProjectEnvironments,
    recordOperation,
    runPermissionChecks,
    saveAppSettings,
    selectProjectEnvironment,
    setProjectEnvironmentValues
  } from "./lib/bridge";
  import { mockBootstrap } from "./lib/mock-data";
  import type {
    AzureDiscovery,
    AzdProjectState,
    BootstrapData,
    Catalog,
    CatalogEntry,
    ConfigurationField,
    ConnectionStatus,
    ExternalTerminalLaunch,
    OperationEvent,
    PermissionCheckResult,
    ProjectEnvironment,
    ProjectEnvironmentValue,
    TemplateInspection,
    ToolInstallPlan,
    ToolStatus,
    WizardPlan,
    WizardValues
  } from "./lib/types";
  import {
    connectionMeetsRequirements,
    missingRequestedScopes
  } from "./lib/connections";
  import { projectServices } from "./lib/project-state";
  import {
    calculateWizardPlan,
    environmentValuesForPlan,
    valuesRemovedByPlan,
    visibleFields
  } from "./lib/wizard";

  type NavId = "home" | "catalog" | "projects" | "connections" | "tools" | "settings";
  type IconName = "home" | "catalog" | "projects" | "connections" | "tools" | "settings";
  type ProjectDeploymentResult = {
    event: OperationEvent;
    projectPath: string;
    environment?: string;
    refreshState: "loading" | "ready" | "failed";
    state?: AzdProjectState;
    refreshError?: string;
  };

  const navigation: { id: NavId; label: string; icon: IconName }[] = [
    { id: "home", label: "Home", icon: "home" },
    { id: "catalog", label: "Catalog", icon: "catalog" },
    { id: "projects", label: "Projects", icon: "projects" },
    { id: "connections", label: "Connections", icon: "connections" },
    { id: "tools", label: "Tools", icon: "tools" },
    { id: "settings", label: "Settings", icon: "settings" }
  ];

  let data: BootstrapData = structuredClone(mockBootstrap);
  let loading = true;
  let activeNav: NavId = "home";
  let navigationExpanded = true;
  let outputOpen = false;
  let commandPaletteOpen = false;
  let commandQuery = "";
  let catalogQuery = "";
  let selectedEntry: CatalogEntry | null = null;
  let selectedInspection: TemplateInspection | null = null;
  let wizardPlan: WizardPlan | null = null;
  let wizardValues: WizardValues = {};
  let activeStep = 0;
  let removedValueWarning: string[] = [];
  let preflightRan = false;
  let permissionChecksRunning = false;
  let permissionCheckResults: PermissionCheckResult[] = [];
  let preflightOperationId: string | null = null;
  let activeOperationId: string | null = null;
  let activeOperationKind: string | null = null;
  let operationEvents: OperationEvent[] = [];
  let operationState: "idle" | "running" | "succeeded" | "failed" | "cancelled" = "idle";
  let lastOperationResult: OperationEvent | null = null;
  let deviceCode = false;
  let theme: "system" | "dark" | "light" = "system";
  let projectPrepared = false;
  let terminalLaunch: ExternalTerminalLaunch | null = null;
  let terminalOpening = false;
  let installPlan: ToolInstallPlan | null = null;
  let installingToolId: string | null = null;
  let selectedProject: import("./lib/types").ProjectSummary | null = null;
  let projectTab: "overview" | "environments" | "operations" | "resources" = "overview";
  let projectState: AzdProjectState | null = null;
  let projectEnvironments: ProjectEnvironment[] = [];
  let projectOperationHistory: OperationEvent[] = [];
  let projectOperations: OperationEvent[] = [];
  let projectPreview:
    | { projectPath: string; environment?: string; event: OperationEvent }
    | null = null;
  let projectDeploymentResult: ProjectDeploymentResult | null = null;
  let projectLoading = false;
  let environmentEditorOpen = false;
  let environmentBusy = false;
  let environmentError = "";
  let environmentName = "";
  let environmentSubscription = "";
  let environmentLocation = "";
  let projectEnvironmentValues: ProjectEnvironmentValue[] = [];
  let environmentConfigurationValues: Record<string, string> = {};
  let environmentConfigurationDirty: string[] = [];
  let environmentConfigurationLoaded = false;
  let environmentConfigurationBusy = false;
  let environmentConfigurationError = "";
  let rawVariableName = "";
  let rawVariableValue = "";
  let selectedProjectConfigurationFields: ConfigurationField[] = [];
  let visibleProjectConfigurationFields: ConfigurationField[] = [];
  let missingProjectConfiguration: ConfigurationField[] = [];
  let catalogSourceOpen = false;
  let catalogSourceUrl = "";
  let catalogPreview: Catalog | null = null;
  let catalogSourceError = "";
  let draftTimer: ReturnType<typeof setTimeout> | null = null;
  let azureDiscovery: AzureDiscovery | null = null;
  let azureDiscoveryLoading = false;
  let azureDiscoveryError = "";
  let manualAzureScope = false;
  let activeStepReady = false;
  const operationResolvers = new Map<
    string,
    { resolve: (event: OperationEvent) => void; reject: (error: Error) => void }
  >();
  const completedOperations = new Map<string, OperationEvent>();
  const operationProjectPaths = new Map<string, string>();
  const operationEnvironments = new Map<string, string | undefined>();

  $: activePage = wizardPlan?.pages[activeStep] ?? null;
  $: selectedProjectConfigurationFields = selectedProject?.templateId
    ? data.catalog.entries
        .find((entry) => entry.id === selectedProject?.templateId)
        ?.manifest?.configuration.groups.flatMap((group) => group.fields) ?? []
    : [];
  $: visibleProjectConfigurationFields = visibleFields(
    selectedProjectConfigurationFields,
    Object.fromEntries(
      selectedProjectConfigurationFields.map((field) => [
        field.id,
        field.type === "boolean"
          ? environmentConfigurationValues[field.id] === "true"
          : field.type === "multiselect"
            ? (environmentConfigurationValues[field.id] ?? "").split(",").filter(Boolean)
            : environmentConfigurationValues[field.id] ?? ""
      ])
    )
  );
  $: missingProjectConfiguration = environmentConfigurationLoaded
    ? visibleProjectConfigurationFields.filter((field) =>
        field.required &&
        !projectEnvironmentValues.find((value) => value.name === field.environmentVariable)?.isSet
      )
    : [];
  $: projectPreviewReady = Boolean(
    projectPreview?.event.state === "succeeded" &&
      selectedProject?.path === projectPreview?.projectPath &&
      selectedProject?.environment === projectPreview?.environment
  );
  $: activeOperationEvent = activeOperationId
    ? operationEvents.filter((event) => event.operationId === activeOperationId).at(-1) ?? null
    : null;
  $: deploymentServices = projectServices(projectDeploymentResult?.state);
  $: workspaceServices = projectServices(projectState);
  $: workspaceResourceCount = workspaceServices.reduce(
    (count, service) => count + service.resources.length,
    0
  );
  $: workspaceEndpointCount = workspaceServices.filter((service) => service.endpoint).length;
  $: {
    const mergedOperations = new Map(
      projectOperationHistory.map((event) => [event.operationId, event])
    );
    for (const event of operationEvents) {
      if (
        event.kind !== "system" &&
        selectedProject?.path === operationProjectPaths.get(event.operationId)
      ) {
        mergedOperations.set(event.operationId, event);
      }
    }
    projectOperations = [...mergedOperations.values()].sort(
      (left, right) => Date.parse(right.timestamp) - Date.parse(left.timestamp)
    );
  }
  $: filteredCatalog = data.catalog.entries.filter((entry) => {
    const query = catalogQuery.trim().toLowerCase();
    return (
      !query ||
      entry.title.toLowerCase().includes(query) ||
      entry.summary.toLowerCase().includes(query) ||
      entry.tags.some((tag) => tag.toLowerCase().includes(query))
    );
  });
  $: requiredTools =
    selectedEntry?.manifest?.prerequisites.map((required) => {
      const status = data.tools.find((tool) => tool.id === required.id);
      return status ? { ...status, ...required } : ({ ...required, state: "unknown" } as ToolStatus);
    }) ?? [];
  $: requiredConnections =
    selectedEntry?.manifest?.connections.map((required) => {
      const status = data.connections.find((connection) => connection.id === required.id);
      return status
        ? { ...status, ...required }
        : ({ ...required, state: "unknown" } as ConnectionStatus);
    }) ?? [];
  $: requiredToolsReady = requiredTools
    .filter((tool) => tool.required)
    .every((tool) => tool.state === "ready");
  $: requiredConnectionsReady = requiredConnections
    .filter((connection) => connection.required)
    .every(connectionMeetsRequirements);
  $: selectedPlatformSupported =
    !selectedEntry || selectedEntry.platforms.includes(data.platform);
  $: connectionAttentionCount = data.connections.filter((connection) => connection.state !== "connected").length;
  $: tenantMismatch =
    new Set(
      data.connections
        .filter((connection) => connection.state === "connected" && connection.tenantId)
        .map((connection) => connection.tenantId?.toLowerCase())
    ).size > 1;
  $: requiredTenantMismatch =
    new Set(
      requiredConnections
        .filter(
          (connection) => connection.required && connection.state === "connected" && connection.tenantId
        )
        .map((connection) => connection.tenantId?.toLowerCase())
    ).size > 1;
  $: selectedTenantMismatch = Boolean(
    wizardValues.tenantId &&
      requiredConnections
        .filter(
          (connection) => connection.required && connection.state === "connected" && connection.tenantId
        )
        .some(
          (connection) =>
            connection.tenantId?.toLowerCase() !== String(wizardValues.tenantId).toLowerCase()
        )
  );
  $: {
    const page = activePage;
    if (!page) {
      activeStepReady = false;
    } else if (page.kind === "source") {
      activeStepReady =
        wizardValues.sourceMode === "localTemplate" || wizardValues.sourceMode === "github"
          ? Boolean(wizardValues.sourcePath && wizardValues.projectPath)
          : Boolean(wizardValues.projectPath);
    } else if (page.kind === "environment") {
      activeStepReady = Boolean(
        wizardValues.environmentName &&
          wizardValues.tenantId &&
          wizardValues.subscriptionId &&
          wizardValues.location &&
          !selectedTenantMismatch
      );
    } else if (page.kind === "overview") {
      activeStepReady = selectedPlatformSupported;
    } else if (page.kind === "prerequisites") {
      activeStepReady = requiredToolsReady;
    } else if (page.kind === "connections") {
      activeStepReady = requiredConnectionsReady && !requiredTenantMismatch;
    } else if (page.kind === "configuration") {
      activeStepReady = page.blockingIssues.length === 0;
    } else if (page.kind === "preflight") {
      activeStepReady = preflightRan;
    } else {
      activeStepReady = true;
    }
  }

  onMount(() => {
    document.addEventListener("keydown", handleGlobalKeydown);
    let unlisten: () => void = () => {};
    void (async () => {
      unlisten = await onOperationEvent(handleOperationEvent);
      try {
        data = await bootstrap();
        theme = data.settings.theme;
        navigationExpanded = data.settings.navigationExpanded;
        applyTheme(theme);
      } catch (error) {
        addSystemLog(`Native backend unavailable: ${String(error)}. Using preview data.`, "warning");
      } finally {
        loading = false;
      }
    })();
    return () => {
      document.removeEventListener("keydown", handleGlobalKeydown);
      if (draftTimer) clearTimeout(draftTimer);
      unlisten();
    };
  });

  function handleGlobalKeydown(event: KeyboardEvent) {
    if ((event.ctrlKey || event.metaKey) && (event.key === "p" || event.key === "k")) {
      event.preventDefault();
      commandPaletteOpen = true;
      setTimeout(() => document.getElementById("command-query")?.focus(), 0);
    }
    if (event.key === "Escape") {
      commandPaletteOpen = false;
      if (!installingToolId) installPlan = null;
    }
  }

  function navigate(id: NavId) {
    activeNav = id;
    selectedEntry = null;
    if (id !== "projects") selectedProject = null;
  }

  function toggleNavigation() {
    navigationExpanded = !navigationExpanded;
    void persistAppSettings();
  }

  function initializeValues(entry: CatalogEntry): WizardValues {
    const values: WizardValues = {
      sourceMode: "catalog",
      projectPath: "",
      sourcePath: "",
      environmentName: "dev",
      subscriptionId: "",
      location: "westus2",
      tenantId: ""
    };
    if (entry.source === "external") values.sourceMode = "externalCatalog";
    for (const group of entry.manifest?.configuration.groups ?? []) {
      for (const field of group.fields) {
        if (field.default !== undefined) values[field.id] = field.default;
      }
    }
    return values;
  }

  function startWizard(entry: CatalogEntry) {
    selectedEntry = entry;
    selectedInspection = null;
    terminalLaunch = null;
    wizardValues = initializeValues(entry);
    wizardPlan = calculateWizardPlan(entry, wizardValues);
    activeStep = 0;
    removedValueWarning = [];
    preflightRan = false;
    permissionChecksRunning = false;
    permissionCheckResults = [];
    operationEvents = [];
    operationState = "idle";
    activeOperationKind = null;
    lastOperationResult = null;
    projectPrepared = false;
    azureDiscovery = null;
    azureDiscoveryLoading = false;
    azureDiscoveryError = "";
    manualAzureScope = false;
    void loadWizardDraft(entry.id).then((draft) => {
      if (!draft || selectedEntry?.id !== entry.id) return;
      wizardValues = { ...wizardValues, ...draft };
      wizardPlan = calculateWizardPlan(entry, wizardValues, wizardPlan ?? undefined);
    });
  }

  function setWizardValue(id: string, value: string | boolean | string[]) {
    wizardValues = { ...wizardValues, [id]: value };
    preflightRan = false;
    permissionCheckResults = [];
    if (!selectedEntry || !wizardPlan) return;
    const nextPlan = calculateWizardPlan(selectedEntry, wizardValues, wizardPlan);
    const removed = valuesRemovedByPlan(wizardPlan, nextPlan);
    removedValueWarning = removed.filter((field) => wizardValues[field] !== undefined && wizardValues[field] !== "");
    const pageId = activePage?.id;
    wizardPlan = nextPlan;
    const nextIndex = wizardPlan.pages.findIndex((page) => page.id === pageId);
    activeStep = nextIndex >= 0 ? nextIndex : Math.min(activeStep, wizardPlan.pages.length - 1);
    if (draftTimer) clearTimeout(draftTimer);
    draftTimer = setTimeout(() => void saveCurrentDraft(), 500);
  }

  async function saveCurrentDraft() {
    if (!selectedEntry) return;
    const sensitive = new Set(
      (selectedEntry.manifest?.configuration.groups ?? [])
        .flatMap((group) => group.fields)
        .filter((field) => field.sensitive || field.type === "secret")
        .map((field) => field.id)
    );
    const values = Object.fromEntries(
      Object.entries(wizardValues).filter(([key, value]) => !sensitive.has(key) && value !== undefined)
    );
    await saveWizardDraft(selectedEntry.id, values);
  }

  async function nextStep() {
    if (!wizardPlan) return;
    if (activePage?.kind === "source" && wizardValues.sourceMode !== "catalog") {
      try {
        const sourcePath = String(
          wizardValues.sourceMode === "existingProject"
            ? wizardValues.projectPath
            : wizardValues.sourcePath
        );
        const cachedInspection =
          wizardValues.sourceMode !== "github" && selectedInspection?.sourcePath === sourcePath
            ? selectedInspection
            : null;
        const inspectionAlreadyReported = Boolean(cachedInspection);
        const inspection =
          cachedInspection ??
          (wizardValues.sourceMode === "github"
            ? await inspectGitHubTemplate(sourcePath)
            : await inspectLocalTemplate(sourcePath));
        selectedEntry = inspection.entry;
        selectedInspection = inspection;
        for (const group of selectedEntry.manifest?.configuration.groups ?? []) {
          for (const field of group.fields) {
            if (wizardValues[field.id] === undefined && field.default !== undefined) {
              wizardValues = { ...wizardValues, [field.id]: field.default };
            }
          }
        }
        wizardPlan = calculateWizardPlan(selectedEntry, wizardValues, wizardPlan);
        if (!inspectionAlreadyReported) {
          inspection.warnings.forEach((warning) => addSystemLog(warning, "warning"));
          if (inspection.warnings.length) outputOpen = true;
        }
        activeStep = Math.max(
          0,
          wizardPlan.pages.findIndex((page) => page.id === "overview")
        );
        return;
      } catch (error) {
        addSystemLog(`Source inspection failed: ${String(error)}`, "error");
        outputOpen = true;
        return;
      }
    }
    activeStep = Math.min(activeStep + 1, wizardPlan.pages.length - 1);
    if (wizardPlan.pages[activeStep]?.kind === "environment" && !azureDiscovery && !manualAzureScope) {
      void loadAzureDiscovery();
    }
  }

  async function loadAzureDiscovery(subscriptionId?: string, refresh = false) {
    if (azureDiscoveryLoading) return;
    azureDiscoveryLoading = true;
    azureDiscoveryError = "";
    try {
      let result = await discoverAzureScope(subscriptionId, refresh);
      if (!subscriptionId) {
        const current = result.subscriptions.find(
          (subscription) => subscription.id.toLowerCase() === String(wizardValues.subscriptionId).toLowerCase()
        );
        const preferred = current ?? result.subscriptions.find((subscription) => subscription.isDefault) ?? result.subscriptions[0];
        if (preferred) {
          setWizardValue("subscriptionId", preferred.id);
          setWizardValue("tenantId", preferred.tenantId);
          result = await discoverAzureScope(preferred.id);
        }
      }
      azureDiscovery = result;
      const selected = result.subscriptions.find(
        (subscription) =>
          subscription.id.toLowerCase() ===
          String(result.selectedSubscriptionId ?? wizardValues.subscriptionId).toLowerCase()
      );
      if (selected) {
        setWizardValue("subscriptionId", selected.id);
        setWizardValue("tenantId", selected.tenantId);
      }
      if (
        result.locations.length > 0 &&
        !result.locations.some((location) => location.name === wizardValues.location)
      ) {
        setWizardValue("location", result.locations[0].name);
      }
      result.warnings.forEach((warning) => addSystemLog(warning, "warning"));
    } catch (error) {
      azureDiscoveryError = String(error);
      if (!azureDiscovery) manualAzureScope = true;
    } finally {
      azureDiscoveryLoading = false;
    }
  }

  function selectAzureSubscription(subscriptionId: string) {
    const selected = azureDiscovery?.subscriptions.find(
      (subscription) => subscription.id.toLowerCase() === subscriptionId.toLowerCase()
    );
    setWizardValue("subscriptionId", subscriptionId);
    if (selected) setWizardValue("tenantId", selected.tenantId);
    void loadAzureDiscovery(subscriptionId);
  }

  function selectAzureTenant(tenantId: string) {
    setWizardValue("tenantId", tenantId);
    const candidates = azureDiscovery?.subscriptions.filter(
      (subscription) => subscription.tenantId.toLowerCase() === tenantId.toLowerCase()
    ) ?? [];
    const currentIsValid = candidates.some(
      (subscription) => subscription.id.toLowerCase() === String(wizardValues.subscriptionId).toLowerCase()
    );
    if (!currentIsValid) {
      const preferred = candidates.find((subscription) => subscription.isDefault) ?? candidates[0];
      if (preferred) selectAzureSubscription(preferred.id);
    }
  }

  function toggleManualAzureScope() {
    manualAzureScope = !manualAzureScope;
    if (!manualAzureScope && !azureDiscovery) void loadAzureDiscovery();
  }

  function previousStep() {
    activeStep = Math.max(activeStep - 1, 0);
  }

  async function selectLocalFolder() {
    const selected = await chooseFolder();
    if (selected) setWizardValue("projectPath", selected);
  }

  async function selectSourceFolder() {
    const selected = await chooseFolder();
    if (selected) setWizardValue("sourcePath", selected);
  }

  async function openLocalProject(copyAsTemplate = false) {
    const selected = await chooseFolder();
    if (!selected) return;
    try {
      const inspection = await inspectLocalTemplate(selected);
      startWizard(inspection.entry);
      selectedInspection = inspection;
      wizardValues = {
        ...wizardValues,
        sourceMode: copyAsTemplate ? "localTemplate" : "existingProject",
        sourcePath: selected,
        projectPath: copyAsTemplate ? "" : selected
      };
      wizardPlan = calculateWizardPlan(inspection.entry, wizardValues, wizardPlan ?? undefined);
      if (inspection.warnings.length) {
        inspection.warnings.forEach((warning) => addSystemLog(warning, "warning"));
        outputOpen = true;
      }
    } catch (error) {
      addSystemLog(`Could not inspect local project: ${String(error)}`, "error");
      outputOpen = true;
    }
  }

  async function checkTools() {
    data = { ...data, tools: data.tools.map((tool) => ({ ...tool, state: "checking" })) };
    try {
      data = { ...data, tools: await refreshTools() };
    } catch (error) {
      addSystemLog(`Tool check failed: ${String(error)}`, "error");
    }
  }

  async function requestToolInstall(tool: ToolStatus) {
    try {
      installPlan = await previewToolInstall(tool.id);
      setTimeout(() => document.getElementById("confirm-tool-install")?.focus(), 0);
    } catch (error) {
      addSystemLog(`Could not prepare installation: ${String(error)}`, "error");
      outputOpen = true;
    }
  }

  async function confirmToolInstall() {
    if (!installPlan || installingToolId) return;
    const plan = installPlan;
    installingToolId = plan.id;
    addSystemLog(`Approved: ${plan.action} from ${plan.source} for ${plan.scope.toLowerCase()}.`, "info");
    try {
      const operationId = await installTool(plan.id);
      outputOpen = true;
      if (data.isNative) {
        await waitForOperation(operationId);
        await checkTools();
      } else {
        data = {
          ...data,
          tools: data.tools.map((item) =>
            item.id === plan.id ? { ...item, state: "ready", installedVersion: item.minimumVersion } : item
          )
        };
      }
      installPlan = null;
    } catch (error) {
      addSystemLog(`Installation failed: ${String(error)}`, "error");
      outputOpen = true;
    } finally {
      installingToolId = null;
    }
  }

  async function checkConnections() {
    azureDiscovery = null;
    data = {
      ...data,
      connections: data.connections.map((connection) => ({ ...connection, state: "checking" }))
    };
    try {
      data = { ...data, connections: await refreshConnections() };
    } catch (error) {
      addSystemLog(`Connection check failed: ${String(error)}`, "error");
    }
  }

  async function connect(provider: ConnectionStatus) {
    addSystemLog(`Starting ${provider.name} sign-in.`, "info");
    outputOpen = true;
    try {
      const operationId = await login(provider.id, provider.scopes ?? [], deviceCode);
      if (data.isNative) {
        await waitForOperation(operationId);
        await checkConnections();
      } else {
        data = {
          ...data,
          connections: data.connections.map((connection) =>
            connection.id === provider.id
              ? {
                  ...connection,
                  state: "connected",
                  account: "preview@example.com",
                  tenantId: "72f988bf-86f1-41af-91ab-2d7cd011db47",
                  grantedScopes:
                    provider.id === "graphPowerShell" ? [...(provider.scopes ?? [])] : connection.grantedScopes
                }
              : connection
          )
        };
      }
    } catch (error) {
      addSystemLog(`Sign-in failed: ${String(error)}`, "error");
    }
  }

  async function connectRequired() {
    for (const connection of requiredConnections.filter(
      (item) => !connectionMeetsRequirements(item)
    )) {
      await connect(connection);
    }
  }

  async function runPreflight() {
    if (!selectedEntry || activeOperationId) return;
    preflightRan = false;
    outputOpen = true;
    if (!selectedPlatformSupported) {
      addSystemLog(`This template does not support ${data.platform}.`, "error");
      return;
    }
    if (!requiredToolsReady) {
      addSystemLog("Install or update every required tool before preflight.", "error");
      return;
    }
    if (!requiredConnectionsReady || requiredTenantMismatch) {
      addSystemLog("Connect the required providers to one tenant before preflight.", "error");
      return;
    }
    if (!wizardValues.tenantId || !wizardValues.subscriptionId) {
      addSystemLog("Select an Azure tenant and subscription before preflight.", "error");
      return;
    }
    if (selectedTenantMismatch) {
      addSystemLog("The selected subscription tenant does not match every required connection.", "error");
      return;
    }
    const configurationIssues =
      wizardPlan?.pages.flatMap((page) => page.blockingIssues) ?? [];
    if (configurationIssues.length > 0) {
      configurationIssues.forEach((issue) => addSystemLog(issue, "error"));
      return;
    }
    const checks = selectedEntry.manifest?.permissionChecks ?? [];
    permissionCheckResults = [];
    if (checks.length > 0) {
      permissionChecksRunning = true;
      addSystemLog(`Running ${checks.length} registered permission check${checks.length === 1 ? "" : "s"}.`, "info");
      try {
        const requiredGraphScopes = Array.from(
          new Set(
            (selectedEntry.manifest?.connections ?? [])
              .filter((connection) => connection.id === "graphPowerShell")
              .flatMap((connection) => connection.scopes ?? [])
          )
        );
        permissionCheckResults = await runPermissionChecks({
          checks,
          tenantId: String(wizardValues.tenantId),
          subscriptionId: String(wizardValues.subscriptionId),
          requiredGraphScopes
        });
        for (const result of permissionCheckResults) {
          addSystemLog(
            `${result.name}: ${result.detail}`,
            result.state === "failed" ? "error" : result.state === "warning" ? "warning" : "success"
          );
        }
        if (permissionCheckResults.some((result) => result.blocking)) {
          addSystemLog("Resolve the required permission failures before infrastructure preview.", "error");
          return;
        }
      } catch (error) {
        addSystemLog(`Permission checks failed: ${String(error)}`, "error");
        return;
      } finally {
        permissionChecksRunning = false;
      }
    }
    addSystemLog("Validated template manifest and pinned source revision.", "success");
    addSystemLog("Checked required tools, connection state, and environment inputs.", "success");
    try {
      const projectPath = await materializeProject();
      addSystemLog("Starting azd provision --preview --no-prompt.", "info");
      const operationId = await startAzdOperation(
        "preview",
        projectPath,
        String(wizardValues.environmentName || "dev")
      );
      preflightOperationId = operationId;
      operationProjectPaths.set(operationId, projectPath);
      operationEnvironments.set(operationId, String(wizardValues.environmentName || "dev"));
      activeOperationId = operationId;
      activeOperationKind = "preview";
      operationState = "running";
      if (!data.isNative) simulateOperation(operationId, "preview");
    } catch (error) {
      addSystemLog(`Preflight failed: ${String(error)}`, "error");
      operationState = "idle";
      activeOperationKind = null;
    }
  }

  async function materializeProject(): Promise<string> {
    if (!selectedEntry) throw new Error("No template is selected.");
    let projectPath = String(wizardValues.projectPath || "");
    if (!projectPrepared) {
      addSystemLog("Materializing the pinned template and applying environment values.", "info");
      if (!wizardPlan) throw new Error("The deployment plan is not available.");
      const environmentValues = environmentValuesForPlan(wizardPlan, wizardValues);
      const project = await prepareDeployment({
        templateId: selectedEntry.id,
        sourceMode: String(wizardValues.sourceMode ?? "catalog"),
        sourcePath: String(wizardValues.sourcePath || "") || undefined,
        sourceRevision: selectedEntry.commit === "local" ? undefined : selectedEntry.commit,
        destination: projectPath,
        environment: String(wizardValues.environmentName || "dev"),
        tenant: String(wizardValues.tenantId || "") || undefined,
        subscription: String(wizardValues.subscriptionId || "") || undefined,
        location: String(wizardValues.location || "westus2"),
        resourceGroup: String(wizardValues.resourceGroup || "") || undefined,
        environmentValues
      });
      projectPath = project.path;
      wizardValues = { ...wizardValues, projectPath };
      data = {
        ...data,
        projects: [project, ...data.projects.filter((existing) => existing.path !== project.path)]
      };
      projectPrepared = true;
    }
    return projectPath;
  }

  async function runDeployment(action = "up") {
    if (!selectedEntry || activeOperationId) return;
    operationState = "running";
    lastOperationResult = null;
    outputOpen = true;
    addSystemLog(`Starting azd ${action} --no-prompt`, "info");
    try {
      const projectPath = await materializeProject();
      const operationId = await startAzdOperation(
        action,
        projectPath,
        String(wizardValues.environmentName || "dev")
      );
      operationProjectPaths.set(operationId, projectPath);
      operationEnvironments.set(operationId, String(wizardValues.environmentName || "dev"));
      activeOperationId = operationId;
      activeOperationKind = action;
      if (!data.isNative) simulateOperation(operationId, action);
    } catch (error) {
      operationState = "failed";
      activeOperationKind = null;
      addSystemLog(`Could not start deployment: ${String(error)}`, "error");
    }
  }

  function simulateOperation(operationId: string, action: string) {
    const messages =
      action === "preview"
        ? ([
            ["Reading environment configuration", 15],
            ["Resolving Azure subscription and location", 32],
            ["Compiling infrastructure", 55],
            ["Comparing proposed Azure resource changes", 78],
            ["Infrastructure preview completed", 100]
          ] as const)
        : ([
            ["Reading environment configuration", 12],
            ["Resolving Azure subscription and location", 25],
            ["Generating infrastructure deployment plan", 42],
            ["Provisioning Azure resources", 67],
            ["Packaging application services", 82],
            ["Deploying application", 94],
            ["Deployment completed", 100]
          ] as const);
    messages.forEach(([message, progress], index) => {
      setTimeout(() => {
        handleOperationEvent({
          operationId,
          kind: action,
          state: progress === 100 ? "succeeded" : "running",
          timestamp: new Date().toISOString(),
          stream: "stdout",
          level: progress === 100 ? "success" : "info",
          message,
          progress,
          outcome:
            progress === 100
              ? {
                  resourceState: ["up", "provision", "deploy", "down"].includes(action)
                    ? "changed"
                    : "unchanged",
                  summary:
                    action === "up"
                      ? "Deployment completed successfully."
                      : action === "preview"
                        ? "Infrastructure preview completed successfully."
                        : `${action} completed successfully.`,
                  guidance: ["up", "provision", "deploy", "down"].includes(action)
                    ? "Refresh project state to inspect the resulting Azure resources."
                    : "No Azure resource changes were applied by this action.",
                  retryable: false
                }
              : undefined
        });
      }, 450 * (index + 1));
    });
  }

  function handleOperationEvent(event: OperationEvent) {
    operationEvents = [...operationEvents.slice(-499), event];
    if (event.state === "succeeded" || event.state === "failed" || event.state === "cancelled") {
      if (["up", "provision", "deploy", "refresh", "show", "down"].includes(event.kind)) {
        lastOperationResult = event;
      }
      completedOperations.set(event.operationId, event);
      const waiter = operationResolvers.get(event.operationId);
      if (waiter) {
        operationResolvers.delete(event.operationId);
        if (event.state === "succeeded") waiter.resolve(event);
        else waiter.reject(new Error(event.message));
      }
      const projectPath =
        operationProjectPaths.get(event.operationId) ||
        selectedProject?.path ||
        String(wizardValues.projectPath || "");
      const operationEnvironment = operationEnvironments.get(event.operationId);
      if (
        projectPath &&
        ["up", "provision", "preview", "deploy", "refresh", "show", "down"].includes(event.kind)
      ) {
        void recordOperation(event, projectPath);
        const projectState = event.state === "succeeded" ? "ready" : "attention";
        data = {
          ...data,
          projects: data.projects.map((project) =>
            project.path === projectPath
              ? { ...project, state: projectState, lastOperation: event.timestamp }
              : project
          )
        };
        if (selectedProject?.path === projectPath) {
          selectedProject = {
            ...selectedProject,
            state: projectState,
            lastOperation: event.timestamp
          };
          projectOperationHistory = [
            event,
            ...projectOperationHistory.filter((item) => item.operationId !== event.operationId)
          ];
        }
      }
      if (
        event.kind === "preview" &&
        event.state === "succeeded" &&
        selectedProject?.path === projectPath &&
        selectedProject.environment === operationEnvironment
      ) {
        projectPreview = { projectPath, environment: operationEnvironment, event };
      }
      if (
        event.kind === "up" &&
        selectedProject?.path === projectPath &&
        selectedProject.environment === operationEnvironment
      ) {
        const result: ProjectDeploymentResult = {
          event,
          projectPath,
          environment: operationEnvironment,
          refreshState: "loading"
        };
        projectDeploymentResult = result;
        void refreshDeploymentResult(result);
      }
      operationProjectPaths.delete(event.operationId);
      operationEnvironments.delete(event.operationId);
    }
    if (
      preflightOperationId === event.operationId &&
      (event.state === "succeeded" || event.state === "failed" || event.state === "cancelled")
    ) {
      preflightRan = event.state === "succeeded";
      preflightOperationId = null;
      activeOperationId = null;
      activeOperationKind = null;
      operationState = "idle";
      if (preflightRan) {
        addSystemLog("Infrastructure preview completed. No Azure changes were applied.", "success");
      }
      return;
    }
    if (event.state === "succeeded" || event.state === "failed" || event.state === "cancelled") {
      operationState = event.state;
      activeOperationId = null;
      activeOperationKind = null;
    } else {
      operationState = "running";
      activeOperationId = event.operationId;
      activeOperationKind = event.kind;
    }
  }

  async function refreshDeploymentResult(result: ProjectDeploymentResult | null = projectDeploymentResult) {
    if (!result) return;
    projectDeploymentResult = { ...result, refreshState: "loading", refreshError: undefined };
    try {
      const state = await getProjectState(result.projectPath, result.environment);
      if (projectDeploymentResult?.event.operationId !== result.event.operationId) return;
      projectDeploymentResult = { ...result, refreshState: "ready", state };
      if (
        selectedProject?.path === result.projectPath &&
        selectedProject.environment === result.environment
      ) {
        projectState = state;
      }
    } catch (error) {
      if (projectDeploymentResult?.event.operationId !== result.event.operationId) return;
      projectDeploymentResult = {
        ...result,
        refreshState: "failed",
        refreshError: String(error)
      };
    }
  }

  function waitForOperation(operationId: string): Promise<OperationEvent> {
    const completed = completedOperations.get(operationId);
    if (completed) {
      completedOperations.delete(operationId);
      return completed.state === "succeeded"
        ? Promise.resolve(completed)
        : Promise.reject(new Error(completed.message));
    }
    return new Promise((resolve, reject) => {
      operationResolvers.set(operationId, { resolve, reject });
      setTimeout(() => {
        if (operationResolvers.delete(operationId)) {
          reject(new Error("Operation status timed out."));
        }
      }, 15 * 60 * 1000);
    });
  }

  async function stopOperation() {
    if (!activeOperationId) return;
    const operationId = activeOperationId;
    const kind = activeOperationKind ?? "up";
    await cancelOperation(operationId);
    if (!data.isNative) {
      const mutating = ["up", "provision", "deploy", "down"].includes(kind);
      handleOperationEvent({
        operationId,
        kind,
        state: "cancelled",
        timestamp: new Date().toISOString(),
        stream: "system",
        level: "warning",
        message: "Cancellation requested by user.",
        outcome: {
          resourceState: mutating ? "possiblyModified" : "unchanged",
          summary: `${kind === "up" ? "Deployment" : kind === "preview" ? "Infrastructure preview" : kind} was cancelled.`,
          guidance: mutating
            ? "The local process stopped, but Azure operations already accepted may continue. Review the output and refresh state before retrying."
            : "No Azure resource changes were applied by this action. Retry when ready.",
          retryable: true
        }
      });
    }
  }

  function addSystemLog(message: string, level: OperationEvent["level"]) {
    handleOperationEvent({
      operationId: activeOperationId ?? "system",
      kind: "system",
      state: activeOperationId ? "running" : "succeeded",
      timestamp: new Date().toISOString(),
      stream: "system",
      level,
      message
    });
  }

  function hydrateEnvironmentConfiguration(values: ProjectEnvironmentValue[]) {
    projectEnvironmentValues = values;
    const next: Record<string, string> = {};
    for (const field of selectedProjectConfigurationFields) {
      const status = values.find((value) => value.name === field.environmentVariable);
      if (status?.value !== undefined) next[field.id] = status.value;
      else if (!status?.isSet && field.default !== undefined) {
        next[field.id] = Array.isArray(field.default) ? field.default.join(",") : String(field.default);
      } else next[field.id] = "";
    }
    environmentConfigurationValues = next;
    environmentConfigurationDirty = [];
    environmentConfigurationLoaded = true;
  }

  async function loadEnvironmentConfiguration(projectPath: string, environment: string) {
    environmentConfigurationLoaded = false;
    environmentConfigurationError = "";
    try {
      hydrateEnvironmentConfiguration(
        await getProjectEnvironmentValues(projectPath, environment)
      );
    } catch (error) {
      projectEnvironmentValues = [];
      environmentConfigurationError = String(error);
    } finally {
      environmentConfigurationLoaded = true;
    }
  }

  function setEnvironmentConfigurationValue(field: ConfigurationField, value: string) {
    projectPreview = null;
    environmentConfigurationValues = { ...environmentConfigurationValues, [field.id]: value };
    if (!environmentConfigurationDirty.includes(field.environmentVariable)) {
      environmentConfigurationDirty = [...environmentConfigurationDirty, field.environmentVariable];
    }
  }

  async function saveEnvironmentConfiguration() {
    if (!selectedProject?.environment || environmentConfigurationBusy) return;
    const fields = selectedProjectConfigurationFields;
    const values = environmentConfigurationDirty
      .map((name) => fields.find((field) => field.environmentVariable === name))
      .filter((field): field is ConfigurationField => Boolean(field))
      .map((field) => ({ name: field.environmentVariable, value: environmentConfigurationValues[field.id] ?? "" }))
      .filter((value) => value.value !== "" || !projectEnvironmentValues.find((item) => item.name === value.name)?.sensitive);
    if (values.length === 0) return;
    environmentConfigurationBusy = true;
    environmentConfigurationError = "";
    try {
      await setProjectEnvironmentValues(selectedProject.path, selectedProject.environment, values);
      projectPreview = null;
      addSystemLog(`Updated ${values.length} azd environment value${values.length === 1 ? "" : "s"}.`, "success");
      await loadEnvironmentConfiguration(selectedProject.path, selectedProject.environment);
    } catch (error) {
      environmentConfigurationError = String(error);
    } finally {
      environmentConfigurationBusy = false;
    }
  }

  async function setRawEnvironmentVariable() {
    if (!selectedProject?.environment || !rawVariableName.trim() || !rawVariableValue || environmentConfigurationBusy) return;
    environmentConfigurationBusy = true;
    environmentConfigurationError = "";
    try {
      await setProjectEnvironmentValues(selectedProject.path, selectedProject.environment, [
        { name: rawVariableName.trim(), value: rawVariableValue }
      ]);
      projectPreview = null;
      addSystemLog(`Updated azd environment variable '${rawVariableName.trim()}'. Its value was not logged.`, "success");
      rawVariableName = "";
      rawVariableValue = "";
      await loadEnvironmentConfiguration(selectedProject.path, selectedProject.environment);
    } catch (error) {
      environmentConfigurationError = String(error);
    } finally {
      environmentConfigurationBusy = false;
    }
  }

  async function copyText(value: string) {
    await navigator.clipboard?.writeText(value);
    addSystemLog("Copied command to clipboard.", "success");
  }

  function copyTerminalCommand() {
    if (terminalLaunch) void copyText(terminalLaunch.command);
  }

  async function openProjectUrl(url: string | undefined) {
    if (!url) return;
    try {
      await openExternalUrl(url);
    } catch (error) {
      addSystemLog(`Could not open link: ${String(error)}`, "error");
      outputOpen = true;
    }
  }

  async function launchExternalTerminal(projectPath: string, environment?: string) {
    if (!projectPath || terminalOpening) return;
    terminalOpening = true;
    try {
      terminalLaunch = await openExternalTerminal(projectPath, environment);
      addSystemLog(`Opened an external terminal in ${terminalLaunch.workingDirectory}.`, "success");
    } catch (error) {
      addSystemLog(`Could not open an external terminal: ${String(error)}`, "error");
      outputOpen = true;
    } finally {
      terminalOpening = false;
    }
  }

  function launchSelectedProjectTerminal() {
    if (selectedProject) void launchExternalTerminal(selectedProject.path, selectedProject.environment);
  }

  async function openProjectWorkspace(
    project: import("./lib/types").ProjectSummary,
    preserveView = false
  ) {
    if (!preserveView) {
      selectedEntry = null;
      selectedInspection = null;
      projectPreview = null;
      projectDeploymentResult = null;
      terminalLaunch = null;
      projectTab = "overview";
    }
    selectedProject = project;
    activeNav = "projects";
    environmentEditorOpen = false;
    environmentError = "";
    environmentConfigurationLoaded = false;
    projectLoading = true;
    const [stateResult, environmentsResult, historyResult] = await Promise.allSettled([
      getProjectState(project.path, project.environment),
      listProjectEnvironments(project.path),
      getOperationHistory(project.path)
    ]);
    projectState = stateResult.status === "fulfilled" ? stateResult.value : null;
    projectEnvironments =
      environmentsResult.status === "fulfilled"
        ? environmentsResult.value
        : project.environment
          ? [{ name: project.environment, dotEnvPath: "", hasLocal: false, hasRemote: false, isDefault: true }]
          : [];
    const currentEnvironment = projectEnvironments.find((environment) => environment.isDefault)?.name ?? project.environment;
    if (environmentsResult.status === "fulfilled") applyProjectEnvironments(projectEnvironments);
    if (currentEnvironment) {
      await loadEnvironmentConfiguration(project.path, currentEnvironment);
    }
    projectOperationHistory = historyResult.status === "fulfilled" ? historyResult.value : [];
    const failures = [stateResult, environmentsResult, historyResult].filter(
      (result) => result.status === "rejected"
    );
    if (failures.length > 0) {
      addSystemLog(
        `Could not refresh ${failures.length} project data source${failures.length === 1 ? "" : "s"}.`,
        "warning"
      );
      outputOpen = true;
    }
    projectLoading = false;
  }

  function refreshSelectedProject() {
    if (selectedProject) void openProjectWorkspace(selectedProject, true);
  }

  function applyProjectEnvironments(environments: ProjectEnvironment[]) {
    projectEnvironments = environments;
    const current = environments.find((environment) => environment.isDefault);
    if (!current || !selectedProject) return;
    selectedProject = { ...selectedProject, environment: current.name };
    data = {
      ...data,
      projects: data.projects.map((project) =>
        project.path === selectedProject?.path ? { ...project, environment: current.name } : project
      )
    };
  }

  function openEnvironmentEditor() {
    environmentName = "";
    environmentSubscription =
      data.connections.find((connection) => connection.id === "azd")?.subscriptionId ?? "";
    environmentLocation = "";
    environmentError = "";
    environmentEditorOpen = true;
  }

  async function refreshEnvironmentProjectState(projectPath: string, environment: string) {
    try {
      projectState = await getProjectState(projectPath, environment);
    } catch (error) {
      projectState = null;
      addSystemLog(
        `The environment changed, but azd project state could not be refreshed: ${String(error)}`,
        "warning"
      );
      outputOpen = true;
    }
  }

  async function createEnvironment() {
    if (!selectedProject || environmentBusy || !environmentName.trim()) return;
    environmentBusy = true;
    environmentError = "";
    try {
      const environments = await createProjectEnvironment(
        selectedProject.path,
        environmentName.trim(),
        environmentSubscription.trim() || undefined,
        environmentLocation.trim() || undefined
      );
      projectPreview = null;
      applyProjectEnvironments(environments);
      addSystemLog(`Created and selected azd environment '${environmentName.trim()}'.`, "success");
      environmentEditorOpen = false;
      await refreshEnvironmentProjectState(selectedProject.path, environmentName.trim());
      await loadEnvironmentConfiguration(selectedProject.path, environmentName.trim());
    } catch (error) {
      environmentError = String(error);
    } finally {
      environmentBusy = false;
    }
  }

  async function selectEnvironment(environment: ProjectEnvironment) {
    if (!selectedProject || environmentBusy || environment.isDefault) return;
    environmentBusy = true;
    environmentError = "";
    try {
      const environments = await selectProjectEnvironment(selectedProject.path, environment.name);
      projectPreview = null;
      applyProjectEnvironments(environments);
      addSystemLog(`Selected azd environment '${environment.name}'.`, "success");
      await refreshEnvironmentProjectState(selectedProject.path, environment.name);
      await loadEnvironmentConfiguration(selectedProject.path, environment.name);
    } catch (error) {
      environmentError = String(error);
    } finally {
      environmentBusy = false;
    }
  }

  type ProjectAction = "preview" | "up" | "provision" | "deploy" | "refresh" | "down";

  async function runProjectAction(action: ProjectAction) {
    if (!selectedProject || activeOperationId) return;
    if (["preview", "up", "provision", "deploy"].includes(action) && missingProjectConfiguration.length > 0) {
      projectTab = "environments";
      environmentConfigurationError = "Set the required environment configuration before deployment.";
      return;
    }
    if (action === "up" && !projectPreviewReady) {
      addSystemLog("Run a successful infrastructure preview for the current environment before azd up.", "warning");
      outputOpen = true;
      return;
    }
    if (
      action === "down" &&
      !window.confirm(
        `Delete Azure resources for ${selectedProject.name}/${selectedProject.environment ?? "current environment"}?`
      )
    ) {
      return;
    }
    outputOpen = true;
    operationState = "running";
    lastOperationResult = null;
    if (["preview", "up", "provision", "deploy", "refresh", "down"].includes(action)) {
      projectPreview = null;
    }
    try {
      const operationId = await startAzdOperation(action, selectedProject.path, selectedProject.environment);
      if (action === "up") projectDeploymentResult = null;
      operationProjectPaths.set(operationId, selectedProject.path);
      operationEnvironments.set(operationId, selectedProject.environment);
      activeOperationId = operationId;
      activeOperationKind = action;
      if (!data.isNative) simulateOperation(operationId, action);
    } catch (error) {
      operationState = "failed";
      activeOperationKind = null;
      addSystemLog(`Could not start azd ${action}: ${String(error)}`, "error");
    }
  }

  function retryProjectOperation(event: OperationEvent) {
    if (["preview", "provision", "deploy", "refresh", "down"].includes(event.kind)) {
      void runProjectAction(event.kind as Exclude<ProjectAction, "up">);
    }
  }

  function resourceStateLabel(event: OperationEvent) {
    switch (event.outcome?.resourceState) {
      case "changed":
        return "Azure changed";
      case "unchanged":
        return "Azure unchanged";
      case "possiblyModified":
        return "Partial changes possible";
      default:
        return "State unknown";
    }
  }

  function applyTheme(value: "system" | "dark" | "light") {
    document.documentElement.dataset.theme = value === "system" ? "" : value;
  }

  function setTheme(value: "system" | "dark" | "light") {
    theme = value;
    applyTheme(value);
    void persistAppSettings();
  }

  async function persistAppSettings() {
    try {
      await saveAppSettings({ theme, navigationExpanded });
    } catch (error) {
      addSystemLog(`Could not save application settings: ${String(error)}`, "warning");
    }
  }

  async function previewCatalogSource() {
    catalogSourceError = "";
    catalogPreview = null;
    try {
      catalogPreview = await previewExternalCatalog(catalogSourceUrl.trim());
    } catch (error) {
      catalogSourceError = String(error);
    }
  }

  async function trustCatalogSource() {
    if (!catalogPreview) return;
    await saveCatalogSource({
      id: catalogPreview.id,
      label: catalogPreview.title,
      url: catalogSourceUrl.trim(),
      trusted: true,
      acceptedCatalog: catalogPreview
    });
    const externalEntries = catalogPreview.entries.map((entry) => ({
      ...entry,
      source: "external" as const,
      trusted: true
    }));
    data = {
      ...data,
      catalog: {
        ...data.catalog,
        entries: [
          ...data.catalog.entries.filter((entry) => entry.source !== "external"),
          ...externalEntries
        ]
      }
    };
    catalogSourceOpen = false;
    catalogSourceUrl = "";
    catalogPreview = null;
  }

  const commands = [
    { label: "Browse catalog", action: () => navigate("catalog") },
    { label: "Open projects", action: () => navigate("projects") },
    { label: "Check connections", action: () => { navigate("connections"); void checkConnections(); } },
    { label: "Check tools", action: () => { navigate("tools"); void checkTools(); } },
    { label: "Toggle output", action: () => (outputOpen = !outputOpen) },
    { label: "Use dark theme", action: () => setTheme("dark") },
    { label: "Use light theme", action: () => setTheme("light") }
  ];
</script>

<svelte:head>
  <title>{selectedEntry ? `${selectedEntry.title} — ` : ""}Azure Deployment Studio</title>
</svelte:head>

<div class="app-shell" class:output-open={outputOpen} class:nav-collapsed={!navigationExpanded}>
  <header class="titlebar">
    <div class="brand-button">
      <span class="brand-mark"><Icon name="cloud" size={17} /></span>
      <span>Azure Deployment Studio</span>
    </div>
    <button class="command-trigger" on:click={() => (commandPaletteOpen = true)}>
      <Icon name="search" size={14} />
      <span>Search projects, templates, and commands</span>
      <kbd>Ctrl P</kbd>
    </button>
    <div class="window-context">
      {#if loading}<span class="spinner small"></span>{/if}
      <StatusPill tone={data.isNative ? "success" : "neutral"} label={data.isNative ? "Native" : "Preview"} />
    </div>
  </header>

  <aside class:collapsed={!navigationExpanded} class="sidebar">
    {#if selectedEntry && wizardPlan}
      <div class="sidebar-heading">
        <span>DEPLOYMENT</span>
        <button aria-label="Close deployment" title="Close deployment" on:click={() => (selectedEntry = null)}><Icon name="close" size={15} /></button>
      </div>
      <div class="wizard-template" title={selectedEntry.title}>
        <div class="template-monogram">{selectedEntry.title.slice(0, 2).toUpperCase()}</div>
        <div class="wizard-template-details">
          <strong>{selectedEntry.title}</strong>
          <span>{selectedEntry.commit.slice(0, 8)}</span>
        </div>
      </div>
      <nav class="step-list" aria-label="Deployment steps">
        {#each wizardPlan.pages as page, index}
          <button
            class:active={activeStep === index}
            class:complete={index < activeStep || page.complete}
            disabled={index > activeStep}
            on:click={() => (activeStep = index)}
            aria-current={activeStep === index ? "step" : undefined}
            aria-label={`${index + 1}. ${page.title}`}
            title={page.title}
          >
            <span class="step-marker">
              {#if index < activeStep}<Icon name="check" size={12} />{:else}{index + 1}{/if}
            </span>
            <span class="step-copy">
              <strong>{page.title}</strong>
              <small>{page.description}</small>
            </span>
          </button>
        {/each}
      </nav>
    {:else}
      <div class="sidebar-heading navigation-heading"><span>NAVIGATION</span></div>
      <nav class="sidebar-nav" aria-label="Primary navigation">
        {#each navigation as item}
          <button
            class:active={activeNav === item.id}
            aria-current={activeNav === item.id ? "page" : undefined}
            aria-label={item.id === "connections" ? `${item.label}, ${connectionAttentionCount} require attention` : item.id === "projects" ? `${item.label}, ${data.projects.length} projects` : item.label}
            title={item.label}
            on:click={() => navigate(item.id)}
          >
            <Icon name={item.icon} size={18} />
            <span class="nav-label">{item.label}</span>
            {#if item.id === "connections"}
              <span class="nav-count">{connectionAttentionCount}</span>
            {:else if item.id === "projects"}
              <span class="nav-count">{data.projects.length}</span>
            {/if}
          </button>
        {/each}
      </nav>
    {/if}
    <div class="sidebar-footer">
      <div class="sidebar-versions" aria-hidden={!navigationExpanded}>
        <span>azd {data.tools.find((tool) => tool.id === "azd")?.installedVersion ?? "not found"}</span>
        <span>App {data.appVersion}</span>
      </div>
      <button
        class="navigation-toggle"
        aria-label={navigationExpanded ? "Collapse navigation" : "Expand navigation"}
        aria-expanded={navigationExpanded}
        title={navigationExpanded ? "Collapse navigation" : "Expand navigation"}
        on:click={toggleNavigation}
      ><Icon name="chevron" size={15} /></button>
    </div>
  </aside>

  <main class="workspace">
    {#if terminalLaunch}
      <div class="terminal-launch-notice" role="status">
        <Icon name="terminal" size={18} />
        <div>
          <strong>External terminal opened</strong>
          <span>{terminalLaunch.workingDirectory}</span>
          <code>{terminalLaunch.command}</code>
        </div>
        <div class="terminal-launch-actions">
          <button class="secondary compact" on:click={copyTerminalCommand}>Copy command</button>
          <button class="icon-button" aria-label="Dismiss terminal notice" on:click={() => (terminalLaunch = null)}><Icon name="close" size={14} /></button>
        </div>
      </div>
    {/if}
    {#if selectedEntry && wizardPlan && activePage}
      <section class="wizard-page">
        <div class="wizard-header">
          <div>
            <div class="eyebrow">STEP {activeStep + 1} OF {wizardPlan.pages.length}</div>
            <h1>{activePage.title}</h1>
            <p>{activePage.description}</p>
          </div>
          <StatusPill tone={selectedEntry.trusted ? "success" : "warning"} label={selectedEntry.trusted ? "Vetted" : "Unvetted"} />
        </div>

        <div class="wizard-content">
          {#if removedValueWarning.length}
            <div class="callout warning">
              <Icon name="warning" size={18} />
              <div>
                <strong>The wizard changed</strong>
                <p>{removedValueWarning.length} value(s) belong to pages that are no longer active. They are preserved unless you finish this deployment.</p>
              </div>
              <button on:click={() => (removedValueWarning = [])}>Dismiss</button>
            </div>
          {/if}

          {#if activePage.kind === "source"}
            <div class="source-grid">
              <button
                class:active={wizardValues.sourceMode === "catalog" || wizardValues.sourceMode === "externalCatalog"}
                class="choice-card"
                on:click={() => setWizardValue("sourceMode", selectedEntry?.source === "external" ? "externalCatalog" : "catalog")}
              >
                <Icon name="catalog" size={24} />
                <strong>Vetted catalog</strong>
                <span>Use the pinned, reviewed template selected from the catalog.</span>
              </button>
              <button
                class:active={wizardValues.sourceMode === "existingProject"}
                class="choice-card"
                on:click={() => setWizardValue("sourceMode", "existingProject")}
              >
                <Icon name="folder" size={24} />
                <strong>Open existing project</strong>
                <span>Work with a local folder containing an azure.yaml file.</span>
              </button>
              <button
                class:active={wizardValues.sourceMode === "localTemplate"}
                class="choice-card"
                on:click={() => setWizardValue("sourceMode", "localTemplate")}
              >
                <Icon name="copy" size={24} />
                <strong>Copy local template</strong>
                <span>Create a new project without changing the source folder.</span>
              </button>
              <button
                class:active={wizardValues.sourceMode === "github"}
                class="choice-card"
                on:click={() => setWizardValue("sourceMode", "github")}
              >
                <Icon name="github" size={24} />
                <strong>GitHub repository</strong>
                <span>Resolve the current revision, review it, and pin it before use.</span>
              </button>
            </div>
            {#if wizardValues.sourceMode !== "catalog" && wizardValues.sourceMode !== "externalCatalog"}
              <label class="field full">
                <span>{wizardValues.sourceMode === "github" ? "Repository URL" : "Folder"}</span>
                <div class="input-action">
                  <input
                    value={String(wizardValues.sourceMode === "existingProject" ? wizardValues.projectPath ?? "" : wizardValues.sourcePath ?? "")}
                    on:input={(event) => setWizardValue(wizardValues.sourceMode === "existingProject" ? "projectPath" : "sourcePath", event.currentTarget.value)}
                    placeholder={wizardValues.sourceMode === "github" ? "https://github.com/owner/repository" : "Choose a folder"}
                  />
                  {#if wizardValues.sourceMode !== "github"}
                    <button class="secondary" on:click={wizardValues.sourceMode === "existingProject" ? selectLocalFolder : selectSourceFolder}><Icon name="folder" size={15} /> Browse</button>
                  {/if}
                </div>
              </label>
              {#if wizardValues.sourceMode === "localTemplate"}
                <label class="field full">
                  <span>New project destination</span>
                  <div class="input-action">
                    <input
                      value={String(wizardValues.projectPath ?? "")}
                      on:input={(event) => setWizardValue("projectPath", event.currentTarget.value)}
                      placeholder="Choose an empty destination folder"
                    />
                    <button class="secondary" on:click={selectLocalFolder}><Icon name="folder" size={15} /> Browse</button>
                  </div>
                </label>
              {/if}
            {:else}
              <label class="field full">
                <span>New project destination <em>Required</em></span>
                <div class="input-action">
                  <input
                    value={String(wizardValues.projectPath ?? "")}
                    on:input={(event) => setWizardValue("projectPath", event.currentTarget.value)}
                    placeholder="Choose an empty destination folder"
                  />
                  <button class="secondary" on:click={selectLocalFolder}><Icon name="folder" size={15} /> Browse</button>
                </div>
              </label>
            {/if}
          {:else if activePage.kind === "overview"}
            <div class="overview-card">
              <div class="overview-icon">{selectedEntry.title.slice(0, 2).toUpperCase()}</div>
              <div class="overview-main">
                <div class="row">
                  <h2>{selectedEntry.title}</h2>
                  <StatusPill tone="success" label="Pinned revision" />
                </div>
                <p>{selectedEntry.description}</p>
                <dl class="metadata">
                  <div><dt>Repository</dt><dd>{selectedEntry.repository}</dd></div>
                  <div><dt>Commit</dt><dd><code>{selectedEntry.commit}</code></dd></div>
                  {#if selectedEntry.subdirectory}<div><dt>Template path</dt><dd>{selectedEntry.subdirectory}</dd></div>{/if}
                  <div><dt>Platforms</dt><dd>{selectedEntry.platforms.join(", ")}</dd></div>
                  <div><dt>Manifest</dt><dd>Catalog-reviewed metadata</dd></div>
                </dl>
                <div class="tag-row">{#each selectedEntry.tags as tag}<span class="tag">{tag}</span>{/each}</div>
              </div>
            </div>
            <div class="callout info">
              <Icon name="check" size={18} />
              <div><strong>Source integrity</strong><p>The deployment will use this exact commit. Future repository changes do not alter this project.</p></div>
            </div>
            {#if selectedInspection?.interactiveHooks}
              <div class="callout warning interactive-hook-warning">
                <Icon name="warning" size={18} />
                <div><strong>Interactive hooks detected</strong><p>Guided deployment uses <code>--no-prompt</code>. If a hook requires input, finish preflight and use the external-terminal fallback.</p></div>
                {#if wizardValues.sourceMode === "existingProject"}
                  <button class="secondary" disabled={terminalOpening} on:click={() => launchExternalTerminal(String(wizardValues.projectPath), String(wizardValues.environmentName || "dev"))}><Icon name="terminal" size={14} /> Open external terminal</button>
                {/if}
              </div>
            {/if}
          {:else if activePage.kind === "prerequisites"}
            <div class="section-toolbar">
              <div><h2>Required for this template</h2><p>Checks are read-only until you approve an installation.</p></div>
              <button class="secondary" on:click={checkTools}><Icon name="refresh" size={15} /> Check again</button>
            </div>
            <div class="data-list">
              {#each requiredTools as tool}
                <div class="data-row">
                  <div class={`state-icon ${tool.state}`}>
                    {#if tool.state === "ready"}<Icon name="check" size={16} />{:else}<Icon name="tools" size={16} />{/if}
                  </div>
                  <div class="data-primary">
                    <strong>{tool.name}</strong>
                    <span>{tool.installedVersion ? `Installed ${tool.installedVersion}` : "Not detected"} · Required {tool.minimumVersion ?? "any version"}{tool.updateAvailable && tool.latestVersion ? ` · Latest ${tool.latestVersion}` : ""}</span>
                  </div>
                  <StatusPill tone={tool.updateAvailable ? "warning" : tool.state === "ready" ? "success" : tool.state === "missing" ? "warning" : "neutral"} label={tool.updateAvailable ? "update available" : tool.state} />
                  {#if tool.updateAvailable && tool.installCommand}
                    <button class="secondary" on:click={() => copyText(tool.installCommand!)}>Copy update preview</button>
                  {:else if tool.state !== "ready"}
                    {#if tool.canInstall}
                      <button class="secondary" on:click={() => requestToolInstall(tool)}>Review install</button>
                    {:else}
                      <button class="secondary" disabled={!tool.documentationUrl} on:click={() => openProjectUrl(tool.documentationUrl)}>View instructions</button>
                    {/if}
                  {/if}
                  {#if tool.installCommand && !tool.updateAvailable}
                    <button class="icon-button" title="Copy install command" on:click={() => copyText(tool.installCommand!)}><Icon name="copy" size={15} /></button>
                  {/if}
                </div>
              {/each}
            </div>
          {:else if activePage.kind === "connections"}
            <div class="section-toolbar">
              <div><h2>Template connections</h2><p>Each tool keeps its own credentials. Tokens never pass through this app.</p></div>
              <button class="primary" on:click={connectRequired}><Icon name="connections" size={15} /> Connect required</button>
            </div>
            <label class="toggle-line">
              <input type="checkbox" bind:checked={deviceCode} />
              <span><strong>Use device code</strong><small>Use when a browser or broker cannot complete sign-in.</small></span>
            </label>
            {#if requiredTenantMismatch}
              <div class="callout warning">
                <Icon name="warning" size={18} />
                <div><strong>Connected tenants do not match</strong><p>Reconnect the affected tools to the same target tenant before deploying.</p></div>
              </div>
            {/if}
            <div class="data-list">
              {#each requiredConnections as connection}
                {@const missingScopes = missingRequestedScopes(connection)}
                <div class="data-row connection-row">
                  <div class={`state-icon ${connection.state}`}>
                    <Icon name="connections" size={16} />
                  </div>
                  <div class="data-primary">
                    <strong>{connection.name}</strong>
                    <span>{connection.reason}</span>
                    {#if connection.account}<small>{connection.account} · {connection.tenantId}{connection.subscriptionName ? ` · ${connection.subscriptionName}` : ""}</small>{/if}
                    {#if connection.expiresOn}<small>Expires <time datetime={connection.expiresOn}>{new Date(connection.expiresOn).toLocaleString()}</time></small>{/if}
                    {#if connection.id === "graphPowerShell" && connection.scopes?.length}
                      <small>Requested scopes: {connection.scopes.join(", ")}</small>
                      <small>Granted scopes: {connection.grantedScopes?.length ? connection.grantedScopes.join(", ") : "None detected"}</small>
                    {/if}
                    {#if missingScopes.length}<small class="scope-warning">Missing scopes: {missingScopes.join(", ")}</small>{/if}
                  </div>
                  <StatusPill tone={connection.state === "connected" && missingScopes.length === 0 ? "success" : "warning"} label={missingScopes.length ? "scope update required" : connection.state} />
                  <button class="secondary" on:click={() => connect(connection)}>
                    {connection.state === "connected" ? (missingScopes.length ? "Update scopes" : "Reconnect") : "Connect"}
                  </button>
                </div>
              {/each}
            </div>
          {:else if activePage.kind === "environment"}
            <div class="section-toolbar">
              <div><h2>Azure environment</h2><p>Choose from the subscriptions available to the signed-in Azure CLI account.</p></div>
              <div class="scope-actions">
                <button
                  class="secondary"
                  disabled={azureDiscoveryLoading}
                  on:click={() => {
                    manualAzureScope = false;
                    void loadAzureDiscovery(undefined, true);
                  }}
                ><Icon name="refresh" size={15} /> Refresh</button>
                <button class="text-button" on:click={toggleManualAzureScope}>
                  {manualAzureScope ? "Use Azure CLI discovery" : "Enter IDs manually"}
                </button>
              </div>
            </div>
            {#if azureDiscoveryLoading}
              <div class="project-loading"><span class="spinner large"></span><span>Loading Azure subscriptions and deployment locations…</span></div>
            {/if}
            {#if azureDiscoveryError}
              <div class="callout warning">
                <Icon name="warning" size={18} />
                <div><strong>Azure CLI discovery is unavailable</strong><p>{azureDiscoveryError} You can still enter the tenant and subscription IDs manually.</p></div>
              </div>
            {/if}
            {#if selectedTenantMismatch}
              <div class="callout warning">
                <Icon name="warning" size={18} />
                <div><strong>Selected tenant does not match</strong><p>Reconnect the required tools to this tenant or choose a subscription in their connected tenant.</p></div>
              </div>
            {/if}
            <div class="form-grid">
              <label class="field">
                <span>Environment name <em>Required</em></span>
                <input value={String(wizardValues.environmentName ?? "")} on:input={(e) => setWizardValue("environmentName", e.currentTarget.value)} placeholder="dev" />
                <small>Stored by azd under the project’s .azure folder.</small>
              </label>
              {#if manualAzureScope || !azureDiscovery}
                <label class="field">
                  <span>Tenant</span>
                  <input value={String(wizardValues.tenantId ?? "")} on:input={(e) => setWizardValue("tenantId", e.currentTarget.value)} placeholder="Tenant ID" />
                </label>
                <label class="field full">
                  <span>Subscription <em>Required</em></span>
                  <input value={String(wizardValues.subscriptionId ?? "")} on:input={(e) => setWizardValue("subscriptionId", e.currentTarget.value)} placeholder="Subscription ID" />
                  <small>Manual entry remains available when Azure CLI is not installed or signed in.</small>
                </label>
              {:else}
                <label class="field">
                  <span>Tenant</span>
                  <select value={String(wizardValues.tenantId ?? "")} on:change={(e) => selectAzureTenant(e.currentTarget.value)}>
                    {#each azureDiscovery.tenants as tenant}
                      <option value={tenant.tenantId}>{tenant.displayName ?? tenant.defaultDomain ?? tenant.tenantId}</option>
                    {/each}
                  </select>
                </label>
                <label class="field full">
                  <span>Subscription <em>Required</em></span>
                  <select value={String(wizardValues.subscriptionId ?? "")} on:change={(e) => selectAzureSubscription(e.currentTarget.value)}>
                    {#each azureDiscovery.subscriptions.filter((subscription) => subscription.tenantId.toLowerCase() === String(wizardValues.tenantId).toLowerCase()) as subscription}
                      <option value={subscription.id}>{subscription.name}{subscription.isDefault ? " (current)" : ""}</option>
                    {/each}
                  </select>
                  <small>{wizardValues.subscriptionId}</small>
                </label>
              {/if}
              <label class="field">
                <span>Azure region <em>Required</em></span>
                <select value={String(wizardValues.location ?? "")} on:change={(e) => setWizardValue("location", e.currentTarget.value)}>
                  {#if !manualAzureScope && azureDiscovery?.locations.length}
                    {#each azureDiscovery.locations as location}
                      <option value={location.name}>{location.regionalDisplayName ?? location.displayName}</option>
                    {/each}
                  {:else}
                    <option value="westus2">West US 2</option>
                    <option value="eastus">East US</option>
                    <option value="centralus">Central US</option>
                    <option value="westeurope">West Europe</option>
                  {/if}
                </select>
              </label>
              <label class="field">
                <span>Resource group</span>
                <input list="azure-resource-groups" value={String(wizardValues.resourceGroup ?? "")} on:input={(e) => setWizardValue("resourceGroup", e.currentTarget.value)} placeholder="Created from environment name" />
                <small>Choose an existing group or enter a new name.</small>
              </label>
            </div>
            <datalist id="azure-resource-groups">
              {#each azureDiscovery?.resourceGroups ?? [] as group}<option value={group.name}>{group.location}</option>{/each}
            </datalist>
          {:else if activePage.kind === "configuration" && activePage.group}
            <div class="form-grid">
              {#each activePage.group.fields as field}
                <div class:full={field.type === "multiselect" || field.type === "secret"} class="field">
                  {#if field.type === "boolean"}
                    <label class="toggle-line card">
                      <input
                        type="checkbox"
                        checked={Boolean(wizardValues[field.id])}
                        on:change={(e) => setWizardValue(field.id, e.currentTarget.checked)}
                      />
                      <span><strong>{field.label}</strong>{#if field.help}<small>{field.help}</small>{/if}</span>
                    </label>
                  {:else if field.type === "select"}
                    <label>
                      <span>{field.label}{#if field.required} <em>Required</em>{/if}</span>
                      <select value={String(wizardValues[field.id] ?? "")} on:change={(e) => setWizardValue(field.id, e.currentTarget.value)}>
                        {#each field.options ?? [] as option}<option value={option.value}>{option.label}</option>{/each}
                      </select>
                    </label>
                  {:else}
                    <label>
                      <span>{field.label}{#if field.required} <em>Required</em>{/if}</span>
                      <input
                        type={field.type === "secret" ? "password" : "text"}
                        value={String(wizardValues[field.id] ?? "")}
                        on:input={(e) => setWizardValue(field.id, e.currentTarget.value)}
                        placeholder={field.placeholder ?? ""}
                        autocomplete={field.type === "secret" ? "new-password" : "off"}
                      />
                      {#if field.help}<small>{field.help}</small>{/if}
                      {#if field.sensitive}<small class="sensitive-note">This value is never saved in the app draft or logs.</small>{/if}
                    </label>
                  {/if}
                </div>
              {/each}
            </div>
            {#if activePage.blockingIssues.length}
              <div class="validation-list">
                {#each activePage.blockingIssues as issue}<span><Icon name="warning" size={14} /> {issue}</span>{/each}
              </div>
            {/if}
          {:else if activePage.kind === "preflight"}
            <div class="preflight-hero" class:complete={preflightRan}>
              <div class="preflight-ring">
                {#if preflightRan}<Icon name="check" size={34} />{:else}<Icon name="cloud" size={34} />{/if}
              </div>
              <h2>{preflightRan ? "Ready to deploy" : "No Azure changes yet"}</h2>
              <p>{preflightRan ? "Manifest, tools, connections, environment inputs, and infrastructure preview passed." : "Run checks and generate an infrastructure preview before materializing the project."}</p>
              <button class="primary large" on:click={runPreflight} disabled={permissionChecksRunning || Boolean(activeOperationId)}><Icon name="play" size={16} /> {permissionChecksRunning ? "Checking access…" : preflightRan ? "Run again" : "Run preflight"}</button>
            </div>
            <div class="check-grid">
              {#each ["Pinned source revision", "Platform compatibility", "Required tools", "Account alignment", "Environment inputs", "Registered permissions", "Infrastructure preview"] as check}
                <div class:complete={preflightRan} class="check-item"><Icon name={preflightRan ? "check" : "chevron"} size={15} /><span>{check}</span></div>
              {/each}
            </div>
            {#if permissionCheckResults.length > 0}
              <section class="permission-results" aria-label="Permission check results">
                {#each permissionCheckResults as result}
                  <article class={`permission-result ${result.state}`}>
                    <Icon name={result.state === "passed" ? "check" : "warning"} size={17} />
                    <div>
                      <strong>{result.name}</strong>
                      <span>{result.detail}</span>
                    </div>
                    <StatusPill
                      tone={result.state === "passed" ? "success" : result.state === "failed" ? "danger" : "warning"}
                      label={result.state}
                    />
                  </article>
                {/each}
              </section>
            {/if}
          {:else if activePage.kind === "review"}
            <div class="review-grid">
              <section>
                <h3>Project</h3>
                <dl class="summary-list">
                  <div><dt>Template</dt><dd>{selectedEntry.title}</dd></div>
                  <div><dt>Revision</dt><dd><code>{selectedEntry.commit.slice(0, 12)}</code></dd></div>
                  <div><dt>Destination</dt><dd>{wizardValues.projectPath || "Choose when materialized"}</dd></div>
                </dl>
              </section>
              <section>
                <h3>Azure environment</h3>
                <dl class="summary-list">
                  <div><dt>Name</dt><dd>{wizardValues.environmentName}</dd></div>
                  <div><dt>Region</dt><dd>{wizardValues.location}</dd></div>
                  <div><dt>Subscription</dt><dd>{wizardValues.subscriptionId || "Connected default"}</dd></div>
                </dl>
              </section>
              <section class="full">
                <h3>Template values</h3>
                <dl class="summary-list columns">
                  {#each Object.entries(wizardValues).filter(([key]) => !["projectPath", "subscriptionId", "tenantId"].includes(key)) as [key, value]}
                    <div><dt>{key}</dt><dd>{key.toLowerCase().includes("secret") || key.toLowerCase().includes("webhook") ? "••••••••" : String(value)}</dd></div>
                  {/each}
                </dl>
              </section>
            </div>
            <div class="callout warning interactive-hook-warning">
              <Icon name="warning" size={18} />
              <div>
                <strong>{selectedInspection?.interactiveHooks ? "Interactive template code will run" : "Template code will run"}</strong>
                <p>{selectedInspection?.interactiveHooks ? "The guided path remains available, but a hook that requests input may require the external terminal." : "Continuing allows azd to execute the hooks in this pinned template. Review the repository if the source is not familiar."}</p>
              </div>
              {#if selectedInspection?.interactiveHooks}
                <button class="secondary" disabled={terminalOpening || !projectPrepared} on:click={() => launchExternalTerminal(String(wizardValues.projectPath), String(wizardValues.environmentName || "dev"))}><Icon name="terminal" size={14} /> Open external terminal</button>
              {/if}
            </div>
          {:else if activePage.kind === "deploy"}
            <div class="deployment-hero">
              <div class={`deployment-orb ${operationState}`}>
                {#if operationState === "running"}<span class="spinner"></span>{:else if operationState === "succeeded"}<Icon name="check" size={38} />{:else if operationState === "failed" || operationState === "cancelled"}<Icon name="warning" size={38} />{:else}<Icon name="cloud" size={38} />{/if}
              </div>
              <h2>{operationState === "succeeded" ? "Deployment complete" : operationState === "failed" ? "Deployment needs attention" : operationState === "cancelled" ? "Deployment cancelled" : operationState === "running" ? "Deploying with azd" : "Ready for azd up"}</h2>
              <p>{operationState === "running" ? "You can continue using the app while the operation runs." : lastOperationResult?.outcome?.summary ?? "The complete guided deployment runs without CLI prompts."}</p>
              {#if operationState === "running"}
                <button class="danger" on:click={stopOperation}><Icon name="stop" size={15} /> Cancel deployment</button>
              {:else}
                <button class="primary large" on:click={() => runDeployment("up")}><Icon name="play" size={16} /> {operationState === "succeeded" ? "Deploy again" : lastOperationResult?.outcome?.retryable ? "Retry azd up" : "Deploy with azd up"}</button>
              {/if}
            </div>
            {#if lastOperationResult?.outcome}
              <div class={`operation-outcome ${lastOperationResult.outcome.resourceState}`}>
                <StatusPill
                  tone={lastOperationResult.outcome.resourceState === "changed" ? "success" : lastOperationResult.outcome.resourceState === "possiblyModified" ? "warning" : "neutral"}
                  label={resourceStateLabel(lastOperationResult)}
                />
                <div>
                  <strong>{lastOperationResult.outcome.summary}</strong>
                  <p>{lastOperationResult.outcome.guidance}</p>
                </div>
              </div>
            {/if}
            {#if selectedInspection?.interactiveHooks}
              <div class="callout warning interactive-hook-warning">
                <Icon name="terminal" size={18} />
                <div><strong>Need interactive input?</strong><p>Open a terminal in the materialized project and run the copyable command without <code>--no-prompt</code>.</p></div>
                <button class="secondary" disabled={terminalOpening || !projectPrepared || operationState === "running"} on:click={() => launchExternalTerminal(String(wizardValues.projectPath), String(wizardValues.environmentName || "dev"))}>Open external terminal</button>
              </div>
            {/if}
            <div class="deployment-progress">
              <div class="progress-track"><span style={`width: ${operationEvents.at(-1)?.progress ?? 0}%`}></span></div>
              <div><span>{operationEvents.at(-1)?.message ?? "Waiting to start"}</span><strong>{operationEvents.at(-1)?.progress ?? 0}%</strong></div>
            </div>
          {/if}
        </div>

        <footer class="wizard-footer">
          <button class="secondary" disabled={activeStep === 0} on:click={previousStep}>Back</button>
          <div class="footer-spacer"></div>
          {#if activeStep < wizardPlan.pages.length - 1}
            <button class="primary" disabled={!activeStepReady} on:click={nextStep}>Continue <Icon name="chevron" size={15} /></button>
          {/if}
        </footer>
      </section>
    {:else if activeNav === "home"}
      <section class="page home-page">
        <div class="hero">
          <div>
            <span class="eyebrow">AZURE DEVELOPER CLI</span>
            <h1>Deploy trusted Azure solutions with confidence.</h1>
            <p>Choose a vetted template, verify what it needs, connect the right accounts, and follow one guided path to Azure.</p>
            <div class="hero-actions">
              <button class="primary large" on:click={() => navigate("catalog")}><Icon name="catalog" size={17} /> Browse catalog</button>
              <button class="secondary large" on:click={() => openLocalProject(false)}><Icon name="folder" size={17} /> Open project</button>
            </div>
          </div>
          <div class="readiness-card">
            <div class="readiness-heading"><span>Local readiness</span><strong>{data.tools.filter((t) => t.state === "ready").length}/{data.tools.length}</strong></div>
            <div class="readiness-track"><span style={`width:${(data.tools.filter((t) => t.state === "ready").length / Math.max(1, data.tools.length)) * 100}%`}></span></div>
            {#each data.tools.slice(0, 3) as tool}
              <div class="mini-status"><span class={`dot ${tool.state}`}></span><span>{tool.name}</span><small>{tool.installedVersion ?? "Needs attention"}</small></div>
            {/each}
          </div>
        </div>
        <div class="page-section">
          <div class="section-toolbar"><div><h2>Continue working</h2><p>Your recent azd projects and environments.</p></div><button class="text-button" on:click={() => navigate("projects")}>View all <Icon name="chevron" size={14} /></button></div>
          <div class="project-grid">
            {#each data.projects as project}
              <button class="project-card" on:click={() => navigate("projects")}>
                <div class="project-icon"><Icon name="cloud" size={21} /></div>
                <div><strong>{project.name}</strong><span>{project.environment} · {project.path}</span><small>{project.lastOperation}</small></div>
                <StatusPill tone={project.state === "ready" ? "success" : "warning"} label={project.state} />
              </button>
            {/each}
          </div>
        </div>
      </section>
    {:else if activeNav === "catalog"}
      <section class="page">
        <div class="page-header">
          <div><span class="eyebrow">TRUSTED SOURCES</span><h1>Template catalog</h1><p>Reviewed, pinned azd deployments that can be configured before any code runs.</p></div>
          <button class="secondary" on:click={() => (catalogSourceOpen = true)}><Icon name="github" size={16} /> Add source</button>
        </div>
        <div class="catalog-toolbar">
          <label><Icon name="search" size={15} /><input bind:value={catalogQuery} placeholder="Filter templates and tags" /></label>
          <span>{filteredCatalog.length} templates</span>
        </div>
        <div class="catalog-grid">
          {#each filteredCatalog as entry}
            <article class="catalog-card">
              <div class="card-top"><div class="catalog-monogram">{entry.title.slice(0, 2).toUpperCase()}</div><StatusPill tone="success" label="Vetted" /></div>
              <h2>{entry.title}</h2>
              <p>{entry.summary}</p>
              <div class="tag-row">{#each entry.tags.slice(0, 3) as tag}<span class="tag">{tag}</span>{/each}</div>
              <div class="catalog-meta"><span>{entry.platforms.join(" · ")}</span><code>{entry.commit.slice(0, 7)}</code></div>
              <button class="primary full-width" on:click={() => startWizard(entry)}>Configure deployment <Icon name="chevron" size={15} /></button>
            </article>
          {/each}
        </div>
      </section>
    {:else if activeNav === "projects"}
      <section class="page">
        {#if selectedProject}
          <div class="project-workspace-header">
            <button class="text-button back-link" on:click={() => (selectedProject = null)}>‹ All projects</button>
            <div class="page-header">
              <div>
                <span class="eyebrow">AZD PROJECT</span>
                <h1>{selectedProject.name}</h1>
                <p>{selectedProject.path}</p>
              </div>
              <div class="project-actions">
                <button class="secondary" disabled={terminalOpening} on:click={launchSelectedProjectTerminal}><Icon name="terminal" size={15} /> Terminal</button>
                <button class="secondary" on:click={() => runProjectAction("refresh")}><Icon name="refresh" size={15} /> Refresh</button>
                <button class="danger" on:click={() => runProjectAction("down")}>Down</button>
              </div>
            </div>
          </div>
          <nav class="project-tabs" aria-label="Project sections">
            {#each ["overview", "environments", "operations", "resources"] as tab}
              <button class:active={projectTab === tab} on:click={() => (projectTab = tab as typeof projectTab)}>{tab}</button>
            {/each}
          </nav>
          {#if projectLoading}
            <div class="project-loading"><span class="spinner"></span><span>Refreshing azd project state…</span></div>
          {:else if projectTab === "overview"}
            <div class="workspace-grid">
              <section class="workspace-panel">
                <h2>Active environment</h2>
                <div class="environment-hero"><Icon name="cloud" size={25} /><div><strong>{selectedProject.environment ?? "No environment"}</strong><span>{data.connections.find((connection) => connection.id === "azd")?.account ?? "Connect azd to deploy"}</span></div><StatusPill tone="success" label="Current" /></div>
              </section>
              <section class="workspace-panel">
                <h2>Source</h2>
                <dl class="summary-list"><div><dt>Template</dt><dd>{selectedProject.templateId ?? "Local project"}</dd></div><div><dt>Revision</dt><dd><code>{selectedProject.revision ?? "local"}</code></dd></div><div><dt>Last activity</dt><dd>{selectedProject.lastOperation ?? "No recorded operations"}</dd></div></dl>
              </section>
              {#if activeOperationKind === "up" || projectDeploymentResult}
                <section class="workspace-panel full project-deployment-result" aria-label="Latest deployment">
                  <div class="section-toolbar">
                    <div><h2>{activeOperationKind === "up" ? "Deployment in progress" : "Latest deployment"}</h2><p>The complete <code>azd up</code> operation and refreshed project state.</p></div>
                    {#if activeOperationKind === "up"}
                      <StatusPill tone="accent" label="Running" />
                    {:else if projectDeploymentResult}
                      <StatusPill
                        tone={projectDeploymentResult.event.state === "succeeded" ? "success" : projectDeploymentResult.event.state === "failed" ? "danger" : "warning"}
                        label={projectDeploymentResult.event.state}
                      />
                    {/if}
                  </div>
                  {#if activeOperationKind === "up"}
                    <div class="project-deployment-progress">
                      <div class="progress-track"><span style={`width: ${activeOperationEvent?.progress ?? 0}%`}></span></div>
                      <div><span>{activeOperationEvent?.message ?? "Starting azd up…"}</span><strong>{activeOperationEvent?.progress ?? 0}%</strong></div>
                      <button class="danger" on:click={stopOperation}><Icon name="stop" size={14} /> Cancel deployment</button>
                    </div>
                  {:else if projectDeploymentResult}
                    <div class={`project-deployment-summary ${projectDeploymentResult.event.state}`}>
                      <Icon name={projectDeploymentResult.event.state === "succeeded" ? "check" : "warning"} size={22} />
                      <div>
                        <strong>{projectDeploymentResult.event.outcome?.summary ?? projectDeploymentResult.event.message}</strong>
                        <p>{projectDeploymentResult.event.outcome?.guidance ?? "Review the operation output for details."}</p>
                      </div>
                      {#if projectDeploymentResult.event.outcome}
                        <StatusPill
                          tone={projectDeploymentResult.event.outcome.resourceState === "changed" ? "success" : projectDeploymentResult.event.outcome.resourceState === "possiblyModified" ? "warning" : "neutral"}
                          label={resourceStateLabel(projectDeploymentResult.event)}
                        />
                      {/if}
                    </div>
                    {#if projectDeploymentResult.refreshState === "loading"}
                      <div class="project-state-refresh"><span class="spinner"></span><span>Refreshing service endpoints and Azure resource state from <code>azd show</code>…</span></div>
                    {:else if projectDeploymentResult.refreshState === "failed"}
                      <div class="callout warning"><Icon name="warning" size={17} /><div><strong>Deployment state could not be refreshed</strong><p>{projectDeploymentResult.refreshError}</p></div><button class="secondary compact" on:click={() => refreshDeploymentResult()}>Retry refresh</button></div>
                    {:else if deploymentServices.length > 0}
                      <div class="deployed-service-grid">
                        {#each deploymentServices as service}
                          <article class="deployed-service-card">
                            <div><h3>{service.name}</h3>{#if service.language}<StatusPill tone="neutral" label={service.language} />{/if}</div>
                            {#if service.endpoint}<code>{service.endpoint}</code>{:else}<span>No public endpoint returned</span>{/if}
                            <small>{service.resources.length} Azure resource{service.resources.length === 1 ? "" : "s"}</small>
                            <div>
                              {#if service.endpoint}<button class="secondary compact" on:click={() => openProjectUrl(service.endpoint)}>Open endpoint</button>{/if}
                              {#if service.resources[0]}<button class="secondary compact" on:click={() => openProjectUrl(service.resources[0].portalUrl)}>Open in Azure</button>{/if}
                            </div>
                          </article>
                        {/each}
                      </div>
                    {:else}
                      <div class="callout info"><Icon name="cloud" size={17} /><div><strong>No deployed service endpoints were returned</strong><p>The operation result is still recorded. Inspect structured state for infrastructure-only templates or resources without public ingress.</p></div></div>
                    {/if}
                    <div class="project-deployment-actions">
                      <button class="secondary" on:click={() => (outputOpen = true)}><Icon name="terminal" size={14} /> View output</button>
                      <button class="secondary" on:click={() => (projectTab = "resources")}><Icon name="cloud" size={14} /> View structured state</button>
                      {#if projectDeploymentResult.event.state !== "succeeded"}
                        <button class="primary" disabled={Boolean(activeOperationId)} on:click={() => runProjectAction("preview")}><Icon name="search" size={14} /> Preview before retry</button>
                      {/if}
                    </div>
                  {/if}
                </section>
              {/if}
              <section class="workspace-panel full project-preflight" aria-label="Deployment preflight">
                <div class="section-toolbar">
                  <div><h2>Deployment preflight</h2><p>Preview infrastructure changes for the selected environment before running the complete deployment.</p></div>
                  {#if projectPreviewReady}
                    <StatusPill tone="success" label="Preview current" />
                  {:else}
                    <StatusPill tone="warning" label="Preview required" />
                  {/if}
                </div>
                {#if missingProjectConfiguration.length > 0}
                  <div class="callout warning"><Icon name="warning" size={17} /><div><strong>Configuration required</strong><p>Set {missingProjectConfiguration.map((field) => field.label).join(", ")} before previewing this environment.</p></div></div>
                {:else if projectPreviewReady && projectPreview?.event.outcome}
                  <div class="project-preview-result">
                    <StatusPill tone="success" label={resourceStateLabel(projectPreview.event)} />
                    <div>
                      <strong>{projectPreview.event.outcome.summary}</strong>
                      <p>{projectPreview.event.outcome.guidance}</p>
                      <small>{selectedProject.environment ?? "Current environment"} · {new Date(projectPreview.event.timestamp).toLocaleString()}</small>
                    </div>
                  </div>
                {:else}
                  <div class="project-preview-pending"><Icon name="cloud" size={20} /><div><strong>No current preview</strong><p>A prior result remains in operation history, but a fresh preview is required after reopening the project or changing its environment configuration.</p></div></div>
                {/if}
                <div class="project-preflight-actions">
                  <button
                    class="secondary"
                    disabled={Boolean(activeOperationId) || !environmentConfigurationLoaded || missingProjectConfiguration.length > 0 || !selectedProject.environment}
                    on:click={() => runProjectAction("preview")}
                  ><Icon name="search" size={15} /> {projectPreviewReady ? "Run preview again" : "Run infrastructure preview"}</button>
                  <button
                    class="primary"
                    disabled={Boolean(activeOperationId) || !projectPreviewReady}
                    title={!projectPreviewReady ? "Complete a current infrastructure preview first" : undefined}
                    on:click={() => runProjectAction("up")}
                  ><Icon name="play" size={15} /> Deploy with azd up</button>
                </div>
              </section>
              <section class="workspace-panel full">
                <div class="section-toolbar"><div><h2>Lifecycle</h2><p>Run individual azd stages when you do not need the full guided workflow.</p></div></div>
                <div class="lifecycle-actions">
                  <button disabled={missingProjectConfiguration.length > 0} on:click={() => runProjectAction("provision")}><Icon name="cloud" size={19} /><strong>Provision</strong><span>Apply infrastructure changes</span></button>
                  <button disabled={missingProjectConfiguration.length > 0} on:click={() => runProjectAction("deploy")}><Icon name="play" size={19} /><strong>Deploy</strong><span>Deploy all application services</span></button>
                  <button on:click={() => runProjectAction("refresh")}><Icon name="refresh" size={19} /><strong>Refresh</strong><span>Refresh environment state</span></button>
                </div>
              </section>
            </div>
          {:else if projectTab === "environments"}
            <div class="environment-management">
              <div class="environment-toolbar">
                <div><h2>Project environments</h2><p>azd owns the values under <code>.azure</code>; the app only invokes supported environment commands.</p></div>
                <button class="primary" disabled={environmentBusy} on:click={openEnvironmentEditor}><Icon name="plus" size={15} /> New environment</button>
              </div>
              {#if environmentEditorOpen}
                <form class="environment-form" on:submit|preventDefault={createEnvironment}>
                  <label><span>Environment name</span><input bind:value={environmentName} required maxlength="64" pattern={"[A-Za-z0-9()._-]{1,64}"} placeholder="staging" /></label>
                  <label><span>Subscription ID <small>Optional</small></span><input bind:value={environmentSubscription} placeholder="00000000-0000-0000-0000-000000000000" /></label>
                  <label><span>Azure location <small>Optional</small></span><input bind:value={environmentLocation} placeholder="westus2" /></label>
                  <div class="environment-form-actions">
                    <button type="button" class="secondary" disabled={environmentBusy} on:click={() => (environmentEditorOpen = false)}>Cancel</button>
                    <button type="submit" class="primary" disabled={environmentBusy || !environmentName.trim()}>{environmentBusy ? "Creating…" : "Create environment"}</button>
                  </div>
                </form>
              {/if}
              {#if environmentError}<div class="inline-error" role="alert">{environmentError}</div>{/if}
              <div class="table-card simple-table environment-table">
                <div class="simple-table-header"><span>Environment</span><span>Status</span><span>Storage</span><span>Action</span></div>
                {#if projectEnvironments.length === 0}
                  <div class="empty-state"><Icon name="cloud" size={28} /><h2>No environments</h2><p>Create an azd environment before provisioning or deploying this project.</p></div>
                {/if}
                {#each projectEnvironments as environment}
                  <div class="simple-table-row" aria-current={environment.isDefault ? "true" : undefined}>
                    <strong>{environment.name}</strong>
                    <StatusPill tone={environment.isDefault ? "success" : "neutral"} label={environment.isDefault ? "Current" : "Available"} />
                    <span>{environment.hasLocal && environment.hasRemote ? "Local + remote" : environment.hasLocal ? "Local" : environment.hasRemote ? "Remote" : "Not materialized"}</span>
                    {#if environment.isDefault}
                      <span class="current-environment-label">Selected</span>
                    {:else}
                      <button class="secondary compact" disabled={environmentBusy} aria-label={`Use ${environment.name} environment`} on:click={() => selectEnvironment(environment)}>Use environment</button>
                    {/if}
                  </div>
                {/each}
              </div>
              <section class="workspace-panel environment-configuration-panel">
                <div class="environment-toolbar">
                  <div><h2>Environment configuration</h2><p>Manifest fields can be read back unless marked sensitive. Raw values are always write-only.</p></div>
                  {#if missingProjectConfiguration.length > 0}
                    <StatusPill tone="warning" label={`${missingProjectConfiguration.length} required`} />
                  {:else if environmentConfigurationLoaded}
                    <StatusPill tone="success" label="Ready" />
                  {/if}
                </div>
                {#if environmentConfigurationError}<div class="inline-error" role="alert">{environmentConfigurationError}</div>{/if}
                {#if !environmentConfigurationLoaded}
                  <div class="project-loading"><span class="spinner"></span><span>Reading safe environment metadata…</span></div>
                {:else}
                  {#if missingProjectConfiguration.length > 0}
                    <div class="callout warning"><Icon name="warning" size={17} /><div><strong>Configuration required before deployment</strong><p>{missingProjectConfiguration.map((field) => field.label).join(", ")}</p></div></div>
                  {/if}
                  {#if visibleProjectConfigurationFields.length > 0}
                    <div class="configuration-editor-grid">
                      {#each visibleProjectConfigurationFields as field}
                        <label class:configuration-checkbox={field.type === "boolean"}>
                          <span>{field.label}{#if field.required}<small>Required</small>{/if}</span>
                          {#if field.type === "boolean"}
                            <input type="checkbox" checked={environmentConfigurationValues[field.id] === "true"} on:change={(event) => setEnvironmentConfigurationValue(field, event.currentTarget.checked ? "true" : "false")} />
                          {:else if field.type === "select"}
                            <select value={environmentConfigurationValues[field.id] ?? ""} on:change={(event) => setEnvironmentConfigurationValue(field, event.currentTarget.value)}>
                              <option value="">Choose a value</option>
                              {#each field.options ?? [] as option}<option value={option.value}>{option.label}</option>{/each}
                            </select>
                          {:else}
                            <input type={field.type === "secret" || field.sensitive ? "password" : "text"} value={environmentConfigurationValues[field.id] ?? ""} placeholder={projectEnvironmentValues.find((value) => value.name === field.environmentVariable)?.sensitive && projectEnvironmentValues.find((value) => value.name === field.environmentVariable)?.isSet ? "Configured — enter to replace" : field.placeholder ?? "Enter a value"} autocomplete={field.type === "secret" || field.sensitive ? "new-password" : "off"} on:input={(event) => setEnvironmentConfigurationValue(field, event.currentTarget.value)} />
                          {/if}
                          <small><code>{field.environmentVariable}</code>{#if field.help} · {field.help}{/if}</small>
                        </label>
                      {/each}
                    </div>
                    <div class="environment-configuration-actions">
                      <span>{environmentConfigurationDirty.length === 0 ? "No unsaved changes" : `${environmentConfigurationDirty.length} unsaved`}</span>
                      <button class="primary" disabled={environmentConfigurationBusy || environmentConfigurationDirty.length === 0} on:click={saveEnvironmentConfiguration}>{environmentConfigurationBusy ? "Saving…" : "Save configuration"}</button>
                    </div>
                  {:else}
                    <div class="empty-configuration"><strong>No manifest-defined fields</strong><span>Add a raw environment variable below when the template documentation requires one.</span></div>
                  {/if}
                  <div class="raw-environment-editor">
                    <div><strong>Advanced variables</strong><span>Existing raw values stay masked and are never returned to the UI.</span></div>
                    {#if projectEnvironmentValues.some((value) => !value.declared)}
                      <div class="raw-variable-list">
                        {#each projectEnvironmentValues.filter((value) => !value.declared) as value}<code>{value.name}</code>{/each}
                      </div>
                    {/if}
                    <form on:submit|preventDefault={setRawEnvironmentVariable}>
                      <label><span>Variable name</span><input bind:value={rawVariableName} required maxlength="128" pattern="[A-Z][A-Z0-9_]*" placeholder="CUSTOM_SETTING" /></label>
                      <label><span>New value</span><input bind:value={rawVariableValue} type="password" required autocomplete="new-password" placeholder="Write-only value" /></label>
                      <button class="secondary" disabled={environmentConfigurationBusy || !rawVariableName.trim() || !rawVariableValue}>Set variable</button>
                    </form>
                  </div>
                {/if}
              </section>
            </div>
          {:else if projectTab === "operations"}
            <div class="table-card operations-list">
              <div class="operation-history-header">
                <div><h2>Lifecycle history</h2><p>Terminal outcomes retained locally for this project.</p></div>
                <span>{projectOperations.length} recorded</span>
              </div>
              {#if projectOperations.length === 0}
                <div class="empty-state"><Icon name="terminal" size={28} /><h2>No recorded operations</h2><p>Preview and lifecycle outcomes will appear here and in the output panel.</p></div>
              {/if}
              {#each projectOperations as event}
                <div class="operation-row">
                  <span class={`status-dot ${event.state === "succeeded" ? "online" : ""}`}></span>
                  <div>
                    <strong>azd {event.kind}</strong>
                    <span>{event.outcome?.summary ?? event.message}</span>
                    {#if event.outcome && event.state !== "succeeded"}<small>{event.outcome.guidance}</small>{/if}
                  </div>
                  <time>{new Date(event.timestamp).toLocaleString()}</time>
                  <StatusPill tone={event.level === "error" ? "danger" : event.level === "success" ? "success" : "neutral"} label={event.state} />
                  {#if event.outcome?.retryable && ["preview", "provision", "deploy", "refresh", "down"].includes(event.kind)}
                    <button class="secondary compact" disabled={Boolean(activeOperationId)} on:click={() => retryProjectOperation(event)}>Retry</button>
                  {/if}
                </div>
              {/each}
            </div>
          {:else}
            <section class="resource-state" aria-label="Project resources">
              <div class="section-toolbar">
                <div><h2>Services and Azure resources</h2><p>Current state returned by <code>azd show --output json</code> for {selectedProject.environment ?? "the active environment"}.</p></div>
                <button class="secondary" on:click={refreshSelectedProject}><Icon name="refresh" size={14} /> Refresh</button>
              </div>
              {#if projectState}
                <div class="resource-summary" aria-label="Resource summary">
                  <div><strong>{workspaceServices.length}</strong><span>Service{workspaceServices.length === 1 ? "" : "s"}</span></div>
                  <div><strong>{workspaceEndpointCount}</strong><span>Public endpoint{workspaceEndpointCount === 1 ? "" : "s"}</span></div>
                  <div><strong>{workspaceResourceCount}</strong><span>Azure resource{workspaceResourceCount === 1 ? "" : "s"}</span></div>
                </div>
                {#if workspaceServices.length > 0}
                  <div class="project-resource-grid">
                    {#each workspaceServices as service}
                      <article class="project-resource-service">
                        <header>
                          <div><h3>{service.name}</h3>{#if service.projectPath}<small>{service.projectPath}</small>{/if}</div>
                          {#if service.language}<StatusPill tone="neutral" label={service.language} />{/if}
                        </header>
                        <div class="resource-endpoint">
                          <span>Public endpoint</span>
                          {#if service.endpoint}
                            <code title={service.endpoint}>{service.endpoint}</code>
                            <button class="secondary compact" on:click={() => openProjectUrl(service.endpoint)}>Open endpoint</button>
                          {:else}
                            <p>No public endpoint returned.</p>
                          {/if}
                        </div>
                        <div class="azure-resource-list">
                          <div class="azure-resource-list-title"><strong>Azure resources</strong><span>{service.resources.length}</span></div>
                          {#if service.resources.length === 0}
                            <p>No resource IDs were returned for this service.</p>
                          {/if}
                          {#each service.resources as resource}
                            <div class="azure-resource-row">
                              <div><strong>{resource.name}</strong><span>{resource.type}</span><small>{resource.resourceGroup}</small></div>
                              <button class="secondary compact" aria-label={`Open ${resource.name} in Azure`} on:click={() => openProjectUrl(resource.portalUrl)}>Open in Azure</button>
                            </div>
                          {/each}
                        </div>
                      </article>
                    {/each}
                  </div>
                {:else}
                  <div class="callout info"><Icon name="cloud" size={17} /><div><strong>No service records were returned</strong><p>This can be normal for infrastructure-only templates. The raw azd state remains available below.</p></div></div>
                {/if}
                <details class="raw-project-state">
                  <summary>Raw azd state</summary>
                  <pre>{JSON.stringify(projectState, null, 2)}</pre>
                </details>
              {:else}
                <div class="callout warning"><Icon name="warning" size={17} /><div><strong>Project state is unavailable</strong><p>Refresh after confirming this folder and environment can be read by <code>azd show</code>. Details are available in Output.</p></div></div>
              {/if}
            </section>
          {/if}
        {:else}
          <div class="page-header"><div><span class="eyebrow">LOCAL WORKSPACES</span><h1>Projects</h1><p>Environments, deployments, and Azure resources managed through azd.</p></div><button class="primary" on:click={() => openLocalProject(false)}><Icon name="folder" size={16} /> Open local project</button></div>
          <div class="table-card">
            <div class="table-header"><span>Project</span><span>Environment</span><span>Revision</span><span>Last activity</span><span>Status</span></div>
            {#each data.projects as project}
              <button class="table-row" on:click={() => openProjectWorkspace(project)}>
                <span><strong>{project.name}</strong><small>{project.path}</small></span>
                <span>{project.environment}</span><code>{project.revision}</code><span>{project.lastOperation}</span>
                <StatusPill tone={project.state === "ready" ? "success" : "warning"} label={project.state} />
              </button>
            {/each}
          </div>
        {/if}
      </section>
    {:else if activeNav === "connections"}
      <section class="page">
        <div class="page-header"><div><span class="eyebrow">AUTHENTICATION</span><h1>Connections</h1><p>See which Azure tools are connected without sharing or storing their tokens.</p></div><button class="secondary" on:click={checkConnections}><Icon name="refresh" size={15} /> Check all</button></div>
        <div class="callout info"><Icon name="connections" size={18} /><div><strong>Credentials stay with each tool</strong><p>Sign-in uses the tool’s native broker, system browser, or device-code flow. This app retains status only.</p></div></div>
        {#if tenantMismatch}<div class="callout warning"><Icon name="warning" size={18} /><div><strong>Tenant mismatch detected</strong><p>Connected tools are using different tenants. Reconnect them before starting a deployment that requires multiple providers.</p></div></div>{/if}
        <div class="connection-grid">
          {#each data.connections as connection}
            <article class="connection-card">
              <div class={`connection-icon ${connection.state}`}><Icon name="connections" size={21} /></div>
              <div class="row"><h2>{connection.name}</h2><StatusPill tone={connection.state === "connected" ? "success" : "warning"} label={connection.state} /></div>
              <p>{connection.account ?? "No active account detected"}</p>
              <dl><div><dt>Tenant</dt><dd>{connection.tenantId ?? "—"}</dd></div><div><dt>Subscription</dt><dd>{connection.subscriptionName ?? "—"}</dd></div><div><dt>Expires</dt><dd>{connection.expiresOn ? new Date(connection.expiresOn).toLocaleString() : "—"}</dd></div><div><dt>Credential owner</dt><dd>{connection.name}</dd></div>{#if connection.id === "graphPowerShell"}<div><dt>Granted scopes</dt><dd title={connection.grantedScopes?.join(", ")}>{connection.grantedScopes?.length ? connection.grantedScopes.join(", ") : "—"}</dd></div>{/if}</dl>
              <button class="secondary full-width" on:click={() => connect(connection)}>{connection.state === "connected" ? "Reconnect" : "Connect"}</button>
            </article>
          {/each}
        </div>
      </section>
    {:else if activeNav === "tools"}
      <section class="page">
        <div class="page-header"><div><span class="eyebrow">PREREQUISITES</span><h1>Tools and modules</h1><p>Detect local versions and use vetted per-user installation paths where available.</p></div><button class="secondary" on:click={checkTools}><Icon name="refresh" size={15} /> Check all</button></div>
        <div class="table-card tool-table">
          {#each data.tools as tool}
            <div class="tool-row">
              <div class={`state-icon ${tool.state}`}><Icon name={tool.state === "ready" ? "check" : "tools"} size={16} /></div>
              <div><strong>{tool.name}</strong><span>{tool.kind === "powershellModule" ? "PowerShell module" : "Command-line tool"}</span></div>
              <div><small>Installed</small><span>{tool.installedVersion ?? "Not found"}</span>{#if tool.updateAvailable && tool.latestVersion}<small>Latest {tool.latestVersion}</small>{/if}</div>
              <div><small>Required</small><span>{tool.minimumVersion ?? "Any"}</span></div>
              <StatusPill tone={tool.updateAvailable ? "warning" : tool.state === "ready" ? "success" : "warning"} label={tool.updateAvailable ? "update available" : tool.state} />
              {#if tool.updateAvailable && tool.installCommand}
                <button class="secondary" on:click={() => copyText(tool.installCommand!)}>Copy update preview</button>
              {:else if tool.state !== "ready"}
                {#if tool.canInstall}<button class="secondary" on:click={() => requestToolInstall(tool)}>Review install</button>{:else}<button class="secondary" disabled={!tool.documentationUrl} on:click={() => openProjectUrl(tool.documentationUrl)}>View instructions</button>{/if}
              {/if}
            </div>
          {/each}
        </div>
      </section>
    {:else}
      <section class="page settings-page">
        <div class="page-header"><div><span class="eyebrow">PREFERENCES</span><h1>Settings</h1><p>Appearance, catalog sources, storage, and diagnostics.</p></div></div>
        <div class="settings-group">
          <h2>Appearance</h2>
          <div class="segmented">
            {#each ["system", "dark", "light"] as option}
              <button class:active={theme === option} on:click={() => setTheme(option as typeof theme)}>{option}</button>
            {/each}
          </div>
        </div>
        <div class="settings-group">
          <h2>Catalog sources</h2>
          <div class="setting-row"><div><strong>Bundled catalog</strong><span>Available offline · {data.catalog.entries.length} pinned templates</span></div><StatusPill tone="success" label="Trusted" /></div>
          <button class="secondary" on:click={() => (catalogSourceOpen = true)}><Icon name="github" size={15} /> Add HTTPS or GitHub source</button>
        </div>
        <div class="settings-group">
          <h2>Privacy</h2>
          <div class="setting-row"><div><strong>Telemetry</strong><span>No usage or deployment telemetry is collected.</span></div><StatusPill tone="neutral" label="Off" /></div>
          <div class="setting-row"><div><strong>Secret storage</strong><span>Tokens and sensitive wizard fields are excluded from app storage and logs.</span></div><StatusPill tone="success" label="Protected" /></div>
        </div>
      </section>
    {/if}
  </main>

  {#if outputOpen}
    <section class="output-panel" aria-label="Operation output">
      <header>
        <div><Icon name="terminal" size={15} /><strong>OUTPUT</strong><span>azd</span></div>
        <div>
          {#if activeOperationId}<button class="output-cancel" on:click={stopOperation}>Cancel {activeOperationKind ?? "operation"}</button>{/if}
          <button title="Clear output" on:click={() => (operationEvents = [])}>Clear</button>
          <button aria-label="Close output" on:click={() => (outputOpen = false)}><Icon name="close" size={15} /></button>
        </div>
      </header>
      <div class="output-lines" aria-live="polite">
        {#if operationEvents.length === 0}<div class="empty-output">Run a check or deployment to see output.</div>{/if}
        {#each operationEvents as event}
          <div class={`output-line ${event.level}`}>
            <time>{new Date(event.timestamp).toLocaleTimeString([], { hour12: false })}</time>
            <span class="stream">{event.stream ?? "system"}</span>
            <span>{event.message}</span>
          </div>
        {/each}
      </div>
    </section>
  {/if}

  <footer class="statusbar">
    <button on:click={() => (outputOpen = !outputOpen)}><Icon name="terminal" size={13} /> {operationState === "running" ? "azd running" : "Output"}</button>
    <div class="status-spacer"></div>
    <button on:click={() => navigate("connections")}><span class={`status-dot ${data.connections.some((c) => c.state === "connected") ? "online" : ""}`}></span>{data.connections.filter((c) => c.state === "connected").length} connected</button>
    <button on:click={() => navigate("projects")}><Icon name="cloud" size={13} /> {selectedProject?.environment ?? data.projects[0]?.environment ?? "No environment"}</button>
    <span>{data.platform}</span>
  </footer>
</div>

{#if installPlan}
  <div class="modal-backdrop" role="presentation" on:click={() => { if (!installingToolId) installPlan = null; }}>
    <dialog open class="source-dialog install-dialog" aria-label={`Review ${installPlan.toolName} installation`} aria-describedby="install-review-description" on:click|stopPropagation>
      <header>
        <div><span class="eyebrow">INSTALLATION APPROVAL</span><h2>Review before installing</h2></div>
        <button class="icon-button" aria-label="Close install review" disabled={Boolean(installingToolId)} on:click={() => (installPlan = null)}><Icon name="close" size={16} /></button>
      </header>
      <p id="install-review-description">The app will run only this registered, non-elevated installation action.</p>
      <dl class="install-plan-details">
        <div><dt>Action</dt><dd>{installPlan.action}</dd></div>
        <div><dt>Provider</dt><dd>{installPlan.provider}</dd></div>
        <div><dt>Source</dt><dd>{installPlan.source}</dd></div>
        <div><dt>Scope</dt><dd>{installPlan.scope}</dd></div>
        <div class="full"><dt>Destination</dt><dd>{installPlan.destination}</dd></div>
      </dl>
      <div class="install-command">
        <div><span>Exact command</span><button class="icon-button" aria-label="Copy install command" title="Copy install command" on:click={() => copyText(installPlan!.command)}><Icon name="copy" size={15} /></button></div>
        <code>{installPlan.command}</code>
      </div>
      <div class="callout info install-safety"><Icon name="check" size={17} /><div><strong>No administrator elevation</strong><p>This writes to the current-user PowerShell module path. Tokens and credentials are not involved.</p></div></div>
      <footer>
        <button class="secondary" disabled={Boolean(installingToolId)} on:click={() => (installPlan = null)}>Cancel</button>
        <button class="text-button" on:click={() => openProjectUrl(installPlan!.documentationUrl)}>Official documentation</button>
        <div class="footer-spacer"></div>
        <button id="confirm-tool-install" class="primary" disabled={Boolean(installingToolId)} on:click={confirmToolInstall}>{installingToolId ? "Installing…" : "Install for current user"}</button>
      </footer>
    </dialog>
  </div>
{/if}

{#if commandPaletteOpen}
  <div class="modal-backdrop" role="presentation" on:click={() => (commandPaletteOpen = false)}>
    <dialog open class="command-palette" aria-label="Command palette" on:click|stopPropagation>
      <label><Icon name="search" size={17} /><input id="command-query" bind:value={commandQuery} placeholder="Type a command" /></label>
      <div>
        {#each commands.filter((command) => command.label.toLowerCase().includes(commandQuery.toLowerCase())) as command}
          <button on:click={() => { command.action(); commandPaletteOpen = false; commandQuery = ""; }}>
            <Icon name="chevron" size={14} /><span>{command.label}</span>
          </button>
        {/each}
      </div>
      <footer><span><kbd>↑↓</kbd> navigate</span><span><kbd>Enter</kbd> select</span><span><kbd>Esc</kbd> close</span></footer>
    </dialog>
  </div>
{/if}

{#if catalogSourceOpen}
  <div class="modal-backdrop" role="presentation" on:click={() => (catalogSourceOpen = false)}>
    <dialog open class="source-dialog" aria-label="Add catalog source" on:click|stopPropagation>
      <header>
        <div><span class="eyebrow">EXTERNAL CATALOG</span><h2>Add a catalog source</h2></div>
        <button class="icon-button" aria-label="Close" on:click={() => (catalogSourceOpen = false)}><Icon name="close" size={16} /></button>
      </header>
      <p>Enter a direct HTTPS catalog URL or a GitHub repository containing <code>azd-gui.catalog.json</code>.</p>
      <label class="field">
        <span>Catalog URL</span>
        <input bind:value={catalogSourceUrl} placeholder="https://github.com/owner/catalog-repository" />
      </label>
      {#if catalogSourceError}
        <div class="callout warning"><Icon name="warning" size={16} /><div><strong>Could not preview source</strong><p>{catalogSourceError}</p></div></div>
      {/if}
      {#if catalogPreview}
        <div class="catalog-preview">
          <div><strong>{catalogPreview.title}</strong><span>{catalogPreview.entries.length} pinned template(s)</span></div>
          <StatusPill tone="warning" label="Review required" />
        </div>
        <div class="preview-entries">
          {#each catalogPreview.entries.slice(0, 4) as entry}
            <div><span>{entry.title}</span><code>{entry.commit.slice(0, 8)}</code></div>
          {/each}
        </div>
        <div class="callout warning"><Icon name="warning" size={16} /><div><strong>Trust this publisher?</strong><p>Templates can run hooks during deployment. Verify the source and pinned revisions before adding it.</p></div></div>
      {/if}
      <footer>
        <button class="secondary" on:click={() => (catalogSourceOpen = false)}>Cancel</button>
        <div class="footer-spacer"></div>
        {#if catalogPreview}
          <button class="primary" on:click={trustCatalogSource}>Trust and add</button>
        {:else}
          <button class="primary" disabled={!catalogSourceUrl.trim()} on:click={previewCatalogSource}>Preview source</button>
        {/if}
      </footer>
    </dialog>
  </div>
{/if}
