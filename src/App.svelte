<script lang="ts">
  import { onMount } from "svelte";
  import Icon from "./components/Icon.svelte";
  import StatusPill from "./components/StatusPill.svelte";
  import {
    bootstrap,
    cancelOperation,
    chooseFolder,
    discoverAzureScope,
    inspectGitHubTemplate,
    inspectLocalTemplate,
    installTool,
    onOperationEvent,
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
    listProjectEnvironments,
    recordOperation
  } from "./lib/bridge";
  import { mockBootstrap } from "./lib/mock-data";
  import type {
    AzureDiscovery,
    BootstrapData,
    Catalog,
    CatalogEntry,
    ConfigurationField,
    ConnectionStatus,
    OperationEvent,
    ToolStatus,
    WizardPage,
    WizardPlan,
    WizardValues
  } from "./lib/types";
  import {
    calculateWizardPlan,
    environmentValuesForPlan,
    valuesRemovedByPlan
  } from "./lib/wizard";

  type NavId = "home" | "catalog" | "projects" | "connections" | "tools" | "settings";
  type IconName = "home" | "catalog" | "projects" | "connections" | "tools" | "settings";

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
  let sidebarOpen = true;
  let outputOpen = false;
  let commandPaletteOpen = false;
  let commandQuery = "";
  let catalogQuery = "";
  let selectedEntry: CatalogEntry | null = null;
  let wizardPlan: WizardPlan | null = null;
  let wizardValues: WizardValues = {};
  let activeStep = 0;
  let removedValueWarning: string[] = [];
  let preflightRan = false;
  let preflightOperationId: string | null = null;
  let activeOperationId: string | null = null;
  let operationEvents: OperationEvent[] = [];
  let operationState: "idle" | "running" | "succeeded" | "failed" | "cancelled" = "idle";
  let deviceCode = false;
  let theme: "system" | "dark" | "light" = "system";
  let projectPrepared = false;
  let selectedProject: import("./lib/types").ProjectSummary | null = null;
  let projectTab: "overview" | "environments" | "operations" | "resources" = "overview";
  let projectState: Record<string, unknown> | null = null;
  let projectEnvironments: Record<string, unknown> | null = null;
  let projectLoading = false;
  let catalogSourceOpen = false;
  let catalogSourceUrl = "";
  let catalogPreview: Catalog | null = null;
  let catalogSourceError = "";
  let draftTimer: ReturnType<typeof setTimeout> | null = null;
  let azureDiscovery: AzureDiscovery | null = null;
  let azureDiscoveryLoading = false;
  let azureDiscoveryError = "";
  let manualAzureScope = false;
  const operationResolvers = new Map<
    string,
    { resolve: (event: OperationEvent) => void; reject: (error: Error) => void }
  >();
  const completedOperations = new Map<string, OperationEvent>();

  $: activePage = wizardPlan?.pages[activeStep] ?? null;
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
    .every((connection) => connection.state === "connected");
  $: selectedPlatformSupported =
    !selectedEntry || selectedEntry.platforms.includes(data.platform);
  $: activeNavigationLabel = navigation.find((item) => item.id === activeNav)?.label ?? "Home";
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

  onMount(() => {
    document.addEventListener("keydown", handleGlobalKeydown);
    let unlisten: () => void = () => {};
    void (async () => {
      unlisten = await onOperationEvent(handleOperationEvent);
      try {
        data = await bootstrap();
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
    if (event.key === "Escape") commandPaletteOpen = false;
  }

  function navigate(id: NavId) {
    activeNav = id;
    selectedEntry = null;
    if (id !== "projects") selectedProject = null;
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
    wizardValues = initializeValues(entry);
    wizardPlan = calculateWizardPlan(entry, wizardValues);
    activeStep = 0;
    removedValueWarning = [];
    preflightRan = false;
    operationEvents = [];
    operationState = "idle";
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

  function stepIsReady(page: WizardPage | null): boolean {
    if (!page) return false;
    if (page.kind === "source") {
      if (wizardValues.sourceMode === "localTemplate") {
        return Boolean(wizardValues.sourcePath && wizardValues.projectPath);
      }
      if (wizardValues.sourceMode === "github") {
        return Boolean(wizardValues.sourcePath && wizardValues.projectPath);
      }
      return Boolean(wizardValues.projectPath);
    }
    if (page.kind === "environment") {
      return Boolean(
        wizardValues.environmentName &&
          wizardValues.tenantId &&
          wizardValues.subscriptionId &&
          wizardValues.location &&
          !selectedTenantMismatch
      );
    }
    if (page.kind === "overview") return selectedPlatformSupported;
    if (page.kind === "prerequisites") return requiredToolsReady;
    if (page.kind === "connections") return requiredConnectionsReady && !requiredTenantMismatch;
    if (page.kind === "configuration") return page.blockingIssues.length === 0;
    if (page.kind === "preflight") return preflightRan;
    return true;
  }

  async function nextStep() {
    if (!wizardPlan) return;
    if (activePage?.kind === "source" && wizardValues.sourceMode !== "catalog") {
      try {
        const inspection =
          wizardValues.sourceMode === "github"
            ? await inspectGitHubTemplate(String(wizardValues.sourcePath))
            : await inspectLocalTemplate(
                String(
                  wizardValues.sourceMode === "existingProject"
                    ? wizardValues.projectPath
                    : wizardValues.sourcePath
                )
              );
        selectedEntry = inspection.entry;
        for (const group of selectedEntry.manifest?.configuration.groups ?? []) {
          for (const field of group.fields) {
            if (wizardValues[field.id] === undefined && field.default !== undefined) {
              wizardValues = { ...wizardValues, [field.id]: field.default };
            }
          }
        }
        wizardPlan = calculateWizardPlan(selectedEntry, wizardValues, wizardPlan);
        inspection.warnings.forEach((warning) => addSystemLog(warning, "warning"));
        if (inspection.warnings.length) outputOpen = true;
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

  async function oneClickInstall(tool: ToolStatus) {
    addSystemLog(`Preparing per-user installation for ${tool.name}.`, "info");
    try {
      const operationId = await installTool(tool.id);
      outputOpen = true;
      if (data.isNative) {
        await waitForOperation(operationId);
        await checkTools();
      } else {
        data = {
          ...data,
          tools: data.tools.map((item) =>
            item.id === tool.id ? { ...item, state: "ready", installedVersion: item.minimumVersion } : item
          )
        };
      }
    } catch (error) {
      addSystemLog(`Installation failed: ${String(error)}`, "error");
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
                  tenantId: "72f988bf-86f1-41af-91ab-2d7cd011db47"
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
    for (const connection of requiredConnections.filter((item) => item.state !== "connected")) {
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
      activeOperationId = operationId;
      operationState = "running";
      if (!data.isNative) simulateOperation(operationId, "preview");
    } catch (error) {
      addSystemLog(`Preflight failed: ${String(error)}`, "error");
      operationState = "idle";
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
    outputOpen = true;
    addSystemLog(`Starting azd ${action} --no-prompt`, "info");
    try {
      const projectPath = await materializeProject();
      const operationId = await startAzdOperation(
        action,
        projectPath,
        String(wizardValues.environmentName || "dev")
      );
      activeOperationId = operationId;
      if (!data.isNative) simulateOperation(operationId, action);
    } catch (error) {
      operationState = "failed";
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
          progress
        });
      }, 450 * (index + 1));
    });
  }

  function handleOperationEvent(event: OperationEvent) {
    operationEvents = [...operationEvents.slice(-499), event];
    if (event.state === "succeeded" || event.state === "failed" || event.state === "cancelled") {
      completedOperations.set(event.operationId, event);
      const waiter = operationResolvers.get(event.operationId);
      if (waiter) {
        operationResolvers.delete(event.operationId);
        if (event.state === "succeeded") waiter.resolve(event);
        else waiter.reject(new Error(event.message));
      }
      const projectPath = selectedProject?.path || String(wizardValues.projectPath || "");
      if (
        projectPath &&
        ["up", "provision", "preview", "deploy", "refresh", "show", "down"].includes(event.kind)
      ) {
        void recordOperation(event, projectPath);
      }
    }
    if (
      preflightOperationId === event.operationId &&
      (event.state === "succeeded" || event.state === "failed" || event.state === "cancelled")
    ) {
      preflightRan = event.state === "succeeded";
      preflightOperationId = null;
      activeOperationId = null;
      operationState = "idle";
      if (preflightRan) {
        addSystemLog("Infrastructure preview completed. No Azure changes were applied.", "success");
      }
      return;
    }
    if (event.state === "succeeded" || event.state === "failed" || event.state === "cancelled") {
      operationState = event.state;
      activeOperationId = null;
    } else {
      operationState = "running";
      activeOperationId = event.operationId;
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
    await cancelOperation(activeOperationId);
    handleOperationEvent({
      operationId: activeOperationId,
      kind: "up",
      state: "cancelled",
      timestamp: new Date().toISOString(),
      stream: "system",
      level: "warning",
      message: "Cancellation requested by user."
    });
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

  async function copyText(value: string) {
    await navigator.clipboard?.writeText(value);
    addSystemLog("Copied command to clipboard.", "success");
  }

  async function openProjectWorkspace(project: import("./lib/types").ProjectSummary) {
    selectedEntry = null;
    selectedProject = project;
    activeNav = "projects";
    projectTab = "overview";
    projectLoading = true;
    try {
      [projectState, projectEnvironments] = await Promise.all([
        getProjectState(project.path, project.environment),
        listProjectEnvironments(project.path)
      ]);
    } catch (error) {
      addSystemLog(`Could not refresh project state: ${String(error)}`, "warning");
      outputOpen = true;
    } finally {
      projectLoading = false;
    }
  }

  function refreshSelectedProject() {
    if (selectedProject) void openProjectWorkspace(selectedProject);
  }

  function environmentRows(): { name: string; isDefault?: boolean }[] {
    const values = projectEnvironments?.environments;
    return Array.isArray(values)
      ? values.filter(
          (value): value is { name: string; isDefault?: boolean } =>
            typeof value === "object" && value !== null && typeof (value as { name?: unknown }).name === "string"
        )
      : selectedProject?.environment
        ? [{ name: selectedProject.environment, isDefault: true }]
        : [];
  }

  async function runProjectAction(action: "provision" | "deploy" | "refresh" | "down") {
    if (!selectedProject || activeOperationId) return;
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
    const operationId = await startAzdOperation(action, selectedProject.path, selectedProject.environment);
    activeOperationId = operationId;
    if (!data.isNative) simulateOperation(operationId, action);
  }

  function setTheme(value: "system" | "dark" | "light") {
    theme = value;
    document.documentElement.dataset.theme = value === "system" ? "" : value;
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

<div class="app-shell" class:output-open={outputOpen}>
  <header class="titlebar">
    <button class="brand-button" aria-label="Toggle navigation" on:click={() => (sidebarOpen = !sidebarOpen)}>
      <span class="brand-mark"><Icon name="cloud" size={17} /></span>
      <span>Azure Deployment Studio</span>
    </button>
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

  <aside class="activity-bar" aria-label="Primary navigation">
    <div class="activity-items">
      {#each navigation.slice(0, 5) as item}
        <button
          class:active={activeNav === item.id && !selectedEntry}
          class="activity-button"
          aria-label={item.label}
          title={item.label}
          on:click={() => navigate(item.id)}
        >
          <Icon name={item.icon} size={22} />
        </button>
      {/each}
    </div>
    <button
      class:active={activeNav === "settings" && !selectedEntry}
      class="activity-button"
      aria-label="Settings"
      title="Settings"
      on:click={() => navigate("settings")}
    >
      <Icon name="settings" size={22} />
    </button>
  </aside>

  {#if sidebarOpen}
    <aside class="sidebar">
      {#if selectedEntry && wizardPlan}
        <div class="sidebar-heading">
          <span>DEPLOYMENT</span>
          <button aria-label="Close deployment" on:click={() => (selectedEntry = null)}><Icon name="close" size={15} /></button>
        </div>
        <div class="wizard-template">
          <div class="template-monogram">{selectedEntry.title.slice(0, 2).toUpperCase()}</div>
          <div>
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
            >
              <span class="step-marker">
                {#if index < activeStep}<Icon name="check" size={12} />{:else}{index + 1}{/if}
              </span>
              <span>
                <strong>{page.title}</strong>
                <small>{page.description}</small>
              </span>
            </button>
          {/each}
        </nav>
      {:else}
        <div class="sidebar-heading"><span>{activeNavigationLabel.toUpperCase()}</span></div>
        <nav class="sidebar-nav">
          {#each navigation as item}
            <button class:active={activeNav === item.id} on:click={() => navigate(item.id)}>
              <Icon name={item.icon} size={16} />
              <span>{item.label}</span>
              {#if item.id === "connections"}
                <span class="nav-count">{data.connections.filter((c) => c.state !== "connected").length}</span>
              {:else if item.id === "projects"}
                <span class="nav-count">{data.projects.length}</span>
              {/if}
            </button>
          {/each}
        </nav>
        <div class="sidebar-footer">
          <span>azd {data.tools.find((tool) => tool.id === "azd")?.installedVersion ?? "not found"}</span>
          <span>App {data.appVersion}</span>
        </div>
      {/if}
    </aside>
  {/if}

  <main class:wide={!sidebarOpen} class="workspace">
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
                    <span>{tool.installedVersion ? `Installed ${tool.installedVersion}` : "Not detected"} · Required {tool.minimumVersion ?? "any version"}</span>
                  </div>
                  <StatusPill tone={tool.state === "ready" ? "success" : tool.state === "missing" ? "warning" : "neutral"} label={tool.state} />
                  {#if tool.state !== "ready"}
                    <button class="secondary" disabled={!tool.canInstall} on:click={() => oneClickInstall(tool)}>Install for me</button>
                  {/if}
                  {#if tool.installCommand}
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
                <div class="data-row connection-row">
                  <div class={`state-icon ${connection.state}`}>
                    <Icon name="connections" size={16} />
                  </div>
                  <div class="data-primary">
                    <strong>{connection.name}</strong>
                    <span>{connection.reason}</span>
                    {#if connection.account}<small>{connection.account} · {connection.tenantId}{connection.subscriptionName ? ` · ${connection.subscriptionName}` : ""}</small>{/if}
                  </div>
                  <StatusPill tone={connection.state === "connected" ? "success" : "warning"} label={connection.state} />
                  <button class="secondary" on:click={() => connect(connection)}>
                    {connection.state === "connected" ? "Reconnect" : "Connect"}
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
              <button class="primary large" on:click={runPreflight}><Icon name="play" size={16} /> {preflightRan ? "Run again" : "Run preflight"}</button>
            </div>
            <div class="check-grid">
              {#each ["Pinned source revision", "Platform compatibility", "Required tools", "Account alignment", "Environment inputs", "Infrastructure preview"] as check}
                <div class:complete={preflightRan} class="check-item"><Icon name={preflightRan ? "check" : "chevron"} size={15} /><span>{check}</span></div>
              {/each}
            </div>
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
            <div class="callout warning">
              <Icon name="warning" size={18} />
              <div><strong>Template code will run</strong><p>Continuing allows azd to execute the hooks in this pinned template. Review the repository if the source is not familiar.</p></div>
            </div>
          {:else if activePage.kind === "deploy"}
            <div class="deployment-hero">
              <div class={`deployment-orb ${operationState}`}>
                {#if operationState === "running"}<span class="spinner"></span>{:else if operationState === "succeeded"}<Icon name="check" size={38} />{:else}<Icon name="cloud" size={38} />{/if}
              </div>
              <h2>{operationState === "succeeded" ? "Deployment complete" : operationState === "running" ? "Deploying with azd" : "Ready for azd up"}</h2>
              <p>{operationState === "running" ? "You can continue using the app while the operation runs." : "The complete guided deployment runs without CLI prompts."}</p>
              {#if operationState === "running"}
                <button class="danger" on:click={stopOperation}><Icon name="stop" size={15} /> Cancel deployment</button>
              {:else}
                <button class="primary large" on:click={() => runDeployment("up")}><Icon name="play" size={16} /> {operationState === "succeeded" ? "Deploy again" : "Deploy with azd up"}</button>
              {/if}
            </div>
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
            <button class="primary" disabled={!stepIsReady(activePage)} on:click={nextStep}>Continue <Icon name="chevron" size={15} /></button>
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
                <button class="secondary" on:click={() => runProjectAction("refresh")}><Icon name="refresh" size={15} /> Refresh</button>
                <button class="secondary" on:click={() => runProjectAction("provision")}>Provision</button>
                <button class="secondary" on:click={() => runProjectAction("deploy")}>Deploy</button>
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
              <section class="workspace-panel full">
                <div class="section-toolbar"><div><h2>Lifecycle</h2><p>Run individual azd stages when you do not need the full guided workflow.</p></div></div>
                <div class="lifecycle-actions">
                  <button on:click={() => runProjectAction("provision")}><Icon name="cloud" size={19} /><strong>Provision</strong><span>Apply infrastructure changes</span></button>
                  <button on:click={() => runProjectAction("deploy")}><Icon name="play" size={19} /><strong>Deploy</strong><span>Deploy all application services</span></button>
                  <button on:click={() => runProjectAction("refresh")}><Icon name="refresh" size={19} /><strong>Refresh</strong><span>Refresh environment state</span></button>
                </div>
              </section>
            </div>
          {:else if projectTab === "environments"}
            <div class="table-card simple-table">
              <div class="simple-table-header"><span>Environment</span><span>Status</span><span>Project</span></div>
              {#each environmentRows() as environment}
                <div class="simple-table-row"><strong>{environment.name}</strong><StatusPill tone={environment.isDefault ? "success" : "neutral"} label={environment.isDefault ? "Current" : "Available"} /><span>{selectedProject.name}</span></div>
              {/each}
            </div>
          {:else if projectTab === "operations"}
            <div class="table-card operations-list">
              {#if operationEvents.filter((event) => event.kind !== "system").length === 0}
                <div class="empty-state"><Icon name="terminal" size={28} /><h2>No operations this session</h2><p>Provision, deploy, refresh, and down activity will appear here and in the output panel.</p></div>
              {/if}
              {#each operationEvents.filter((event) => event.kind !== "system").slice().reverse() as event}
                <div class="operation-row"><span class={`status-dot ${event.state === "succeeded" ? "online" : ""}`}></span><div><strong>azd {event.kind}</strong><span>{event.message}</span></div><time>{new Date(event.timestamp).toLocaleTimeString()}</time><StatusPill tone={event.level === "error" ? "danger" : event.level === "success" ? "success" : "neutral"} label={event.state} /></div>
              {/each}
            </div>
          {:else}
            <div class="resource-state">
              <div class="section-toolbar"><div><h2>Structured azd state</h2><p>Resource endpoints and service details returned by <code>azd show --output json</code>.</p></div><button class="secondary" on:click={refreshSelectedProject}><Icon name="refresh" size={14} /> Refresh</button></div>
              <pre>{JSON.stringify(projectState ?? {}, null, 2)}</pre>
            </div>
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
              <dl><div><dt>Tenant</dt><dd>{connection.tenantId ?? "—"}</dd></div><div><dt>Subscription</dt><dd>{connection.subscriptionName ?? "—"}</dd></div><div><dt>Credential owner</dt><dd>{connection.name}</dd></div></dl>
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
              <div><small>Installed</small><span>{tool.installedVersion ?? "Not found"}</span></div>
              <div><small>Required</small><span>{tool.minimumVersion ?? "Any"}</span></div>
              <StatusPill tone={tool.state === "ready" ? "success" : "warning"} label={tool.state} />
              {#if tool.state !== "ready"}<button class="secondary" disabled={!tool.canInstall} on:click={() => oneClickInstall(tool)}>Install for me</button>{/if}
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
    <button on:click={() => navigate("projects")}><Icon name="cloud" size={13} /> {data.projects[0]?.environment ?? "No environment"}</button>
    <span>{data.platform}</span>
  </footer>
</div>

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
