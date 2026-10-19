import { act, cleanup, fireEvent, render, screen, waitFor } from "@testing-library/svelte";
import { afterEach, describe, expect, it, vi } from "vitest";
import App from "./App.svelte";
import * as bridge from "./lib/bridge";
import { mockBootstrap } from "./lib/mock-data";
import type { CatalogEntry, ProjectEnvironmentValue } from "./lib/types";

const tenantId = "11111111-1111-4111-8111-111111111111";
const subscriptionId = "33333333-3333-4333-8333-333333333333";
const acknowledgementLabel = "I understand the directory-wide scope";

function acknowledgementEntry(): CatalogEntry {
  return {
    ...structuredClone(mockBootstrap.catalog.entries[0]),
    id: "acknowledgement-test",
    title: "Acknowledgement test template",
    trusted: true,
    source: "bundled",
    manifest: {
      schemaVersion: "1.0",
      prerequisites: [],
      connections: [],
      configuration: { groups: [
        {
          id: "choices", title: "Directory choices", fields: [
            { id: "scope", environmentVariable: "SCOPE", label: "Directory scope", type: "select", required: true,
              default: "all", options: [{ label: "All objects", value: "all" }, { label: "Selected objects", value: "selected" }] },
            { id: "enabled", environmentVariable: "DIRECTORY_ENABLED", label: "Enable directory integration", type: "boolean", default: true }
          ]
        },
        {
          id: "acknowledgements", title: "Directory acknowledgement", condition: { field: "enabled", equals: true }, fields: [
            { id: "ack", environmentVariable: "ACK", label: acknowledgementLabel, type: "acknowledgement", required: true,
              help: "This choice can change every object in the selected tenant.",
              validationMessage: "Acknowledge the current tenant and scope before continuing.",
              condition: { field: "scope", equals: "all" } }
          ]
        }
      ] }
    }
  };
}

function setup(entry = acknowledgementEntry()) {
  const data = structuredClone(mockBootstrap);
  data.isNative = true;
  data.catalog.entries = [entry];
  data.projects = [{ ...data.projects[0], name: "Acknowledgement project", templateId: entry.id, environment: "dev" }];
  const project = data.projects[0];
  vi.spyOn(bridge, "bootstrap").mockResolvedValue(data);
  vi.spyOn(bridge, "onOperationEvent").mockResolvedValue(() => {});
  vi.spyOn(bridge, "discoverAzureScope").mockRejectedValue(new Error("Use the test's manual Azure target."));
  vi.spyOn(bridge, "loadWizardDraft").mockResolvedValue({
    projectPath: "C:\\Deployments\\acknowledgement-test", environmentName: "dev", tenantId, subscriptionId,
    scope: "all", enabled: true, ack: true
  });
  const saveDraft = vi.spyOn(bridge, "saveWizardDraft").mockResolvedValue(undefined);
  vi.spyOn(bridge, "getProjectTemplate").mockResolvedValue({ entry, digest: "acknowledgement-test-contract" });
  vi.spyOn(bridge, "listProjectEnvironments").mockResolvedValue([
    { name: "dev", dotEnvPath: "", hasLocal: true, hasRemote: false, isDefault: true }
  ]);
  const stored: ProjectEnvironmentValue[] = [
    { name: "AZURE_TENANT_ID", value: tenantId, isSet: true, sensitive: false, declared: true, required: true },
    { name: "SCOPE", value: "all", isSet: true, sensitive: false, declared: true, required: true },
    { name: "DIRECTORY_ENABLED", value: "true", isSet: true, sensitive: false, declared: true, required: false },
    { name: "ACK", value: "true", isSet: true, sensitive: false, declared: true, required: true }
  ];
  vi.spyOn(bridge, "getProjectEnvironmentValues").mockResolvedValue(stored);
  const writes = vi.spyOn(bridge, "setProjectEnvironmentValues").mockResolvedValue(undefined);
  const approvals = vi.spyOn(bridge, "requestExecutionApproval").mockImplementation(async (request) => ({
    token: "acknowledgement-test-approval", projectPath: request.projectPath
  }));
  const starts = vi.spyOn(bridge, "startAzdOperation").mockResolvedValue("acknowledgement-test-operation");
  return { entry, project, saveDraft, writes, approvals, starts };
}

async function continueWizard() {
  const button = screen.getByRole("button", { name: /^continue$/i });
  await waitFor(() => expect(button).toBeEnabled());
  await fireEvent.click(button);
}

async function openWizard() {
  await fireEvent.click(await screen.findByRole("button", { name: /browse catalog/i }));
  await fireEvent.click(await screen.findByRole("button", { name: /configure deployment/i }));
  await screen.findByRole("status", { name: /saved wizard choices/i });
  await continueWizard();
  await continueWizard();
  await screen.findByPlaceholderText("Tenant ID");
  await continueWizard();
  await screen.findByRole("heading", { name: "Directory choices" });
}

function acknowledgementCheckbox() {
  return screen.getByRole("checkbox", { name: new RegExp(acknowledgementLabel) });
}

afterEach(() => {
  cleanup();
  vi.restoreAllMocks();
  vi.unstubAllGlobals();
  window.localStorage.clear();
  document.documentElement.dataset.theme = "";
});


describe("acknowledgement preparation interruptions", () => {
  it.each([false, true])("does not adopt a late prepare result or launch after close/reopen (interactive=%s)", async (interactive) => {
    const entry = acknowledgementEntry();
    entry.manifest!.requiresInteractiveTerminal = interactive;
    const { starts, approvals } = setup(entry);
    const terminal = vi.spyOn(bridge, "openExternalTerminal");
    let completePrepare!: (project: Awaited<ReturnType<typeof bridge.prepareDeployment>>) => void;
    const prepare = vi.spyOn(bridge, "prepareDeployment").mockImplementation(() => new Promise((resolve) => {
      completePrepare = resolve;
    }));
    render(App);
    await openWizard();
    await continueWizard();
    await fireEvent.click(acknowledgementCheckbox());
    await continueWizard();
    await fireEvent.click(screen.getByRole("checkbox", { name: /approve pinned template code execution/i }));
    await continueWizard();
    await fireEvent.click(screen.getByRole("button", { name: interactive ? /^Prepare and open terminal$/i : /^Run checks and preview$/i }));
    await waitFor(() => expect(prepare).toHaveBeenCalledTimes(1));
    expect(approvals).toHaveBeenCalledTimes(1);
    const originalRequest = prepare.mock.calls[0][0];
    await fireEvent.click(screen.getByRole("button", { name: /close deployment/i }));
    await fireEvent.click(screen.getByRole("button", { name: /configure deployment/i }));
    await screen.findByRole("status", { name: /saved wizard choices/i });
    const destination = screen.getByPlaceholderText(/empty destination folder/i);
    const newDestination = "C:\\Deployments\\new-wizard-destination";
    await fireEvent.input(destination, { target: { value: newDestination } });
    await act(() => completePrepare({
      id: "old-prepared", name: "Old prepared project", path: originalRequest.destination,
      environment: originalRequest.environment, state: "ready"
    }));
    await waitFor(() => expect(destination).toHaveValue(newDestination));
    expect(starts).not.toHaveBeenCalled();
    expect(terminal).not.toHaveBeenCalled();
    // The new wizard still has its own unaccepted acknowledgement, even though
    // the old project completed with ACK=true and a proof moments earlier.
    for (let step = 0; step < 4; step += 1) await continueWizard();
    expect(acknowledgementCheckbox()).not.toBeChecked();
    expect(screen.getByRole("button", { name: /^continue$/i })).toBeDisabled();
  });

  it.each([false, true])("does not mutate a prepared project's environment after close while listing (exists=%s)", async (exists) => {
    const { starts } = setup();
    let emit!: Parameters<typeof bridge.onOperationEvent>[0];
    vi.mocked(bridge.onOperationEvent).mockImplementation(async (listener) => { emit = listener; return () => {}; });
    const create = vi.spyOn(bridge, "createProjectEnvironment");
    const select = vi.spyOn(bridge, "selectProjectEnvironment");
    render(App);
    await openWizard();
    await continueWizard();
    await fireEvent.click(acknowledgementCheckbox());
    await continueWizard();
    await fireEvent.click(screen.getByRole("checkbox", { name: /approve pinned template code execution/i }));
    await continueWizard();
    await fireEvent.click(screen.getByRole("button", { name: /^Run checks and preview$/i }));
    await waitFor(() => expect(starts).toHaveBeenCalledTimes(1));
    await act(() => emit({ operationId: "acknowledgement-test-operation", kind: "preview", state: "succeeded", timestamp: new Date().toISOString(), stream: "stdout", level: "success", message: "Preview completed", progress: 100 }));
    for (let step = 0; step < 4; step += 1) await fireEvent.click(screen.getByRole("button", { name: /^back$/i }));
    await fireEvent.input(screen.getByPlaceholderText("dev"), { target: { value: "second" } });
    await continueWizard();
    await continueWizard();
    await fireEvent.click(acknowledgementCheckbox());
    await continueWizard();
    await fireEvent.click(screen.getByRole("checkbox", { name: /approve pinned template code execution/i }));
    await continueWizard();
    let completeList!: (environments: Awaited<ReturnType<typeof bridge.listProjectEnvironments>>) => void;
    vi.mocked(bridge.listProjectEnvironments).mockImplementation(() => new Promise((resolve) => { completeList = resolve; }));
    await fireEvent.click(screen.getByRole("button", { name: /^Run checks and preview$/i }));
    await waitFor(() => expect(completeList).toBeDefined());
    await fireEvent.click(screen.getByRole("button", { name: /close deployment/i }));
    await fireEvent.click(screen.getByRole("button", { name: /configure deployment/i }));
    await screen.findByRole("status", { name: /saved wizard choices/i });
    await act(() => completeList(exists ? [{ name: "second", dotEnvPath: "", hasLocal: true, hasRemote: false, isDefault: false }] : []));
    expect(create).not.toHaveBeenCalled();
    expect(select).not.toHaveBeenCalled();
    expect(starts).toHaveBeenCalledTimes(1);
  });


  it.each(["reopen", "edit"])("does not mark the current wizard previewed after an old completion (%s)", async (change) => {
    const { starts } = setup();
    let emit!: Parameters<typeof bridge.onOperationEvent>[0];
    vi.mocked(bridge.onOperationEvent).mockImplementation(async (listener) => { emit = listener; return () => {}; });
    render(App);
    await openWizard();
    await continueWizard();
    await fireEvent.click(acknowledgementCheckbox());
    await continueWizard();
    await fireEvent.click(screen.getByRole("checkbox", { name: /approve pinned template code execution/i }));
    await continueWizard();
    await fireEvent.click(screen.getByRole("button", { name: /^Run checks and preview$/i }));
    await waitFor(() => expect(starts).toHaveBeenCalledTimes(1));
    if (change === "reopen") {
      await fireEvent.click(screen.getByRole("button", { name: /close deployment/i }));
      await fireEvent.click(screen.getByRole("button", { name: /configure deployment/i }));
      await screen.findByRole("status", { name: /saved wizard choices/i });
      for (let step = 0; step < 4; step += 1) await continueWizard();
    } else {
      for (let step = 0; step < 3; step += 1) await fireEvent.click(screen.getByRole("button", { name: /^back$/i }));
      const scope = screen.getByRole("combobox", { name: /Directory scope/ });
      await fireEvent.change(scope, { target: { value: "selected" } });
      await fireEvent.change(scope, { target: { value: "all" } });
      await continueWizard();
    }
    expect(acknowledgementCheckbox()).not.toBeChecked();
    await fireEvent.click(acknowledgementCheckbox());
    await continueWizard();
    await fireEvent.click(screen.getByRole("checkbox", { name: /approve pinned template code execution/i }));
    await continueWizard();
    await act(() => emit({ operationId: "acknowledgement-test-operation", kind: "preview", state: "succeeded", timestamp: new Date().toISOString(), stream: "stdout", level: "success", message: "Old preview completed", progress: 100 }));
    expect(screen.getByRole("heading", { name: "Run checks and template preview" })).toBeInTheDocument();
    expect(screen.queryByRole("heading", { name: "Preview completed" })).not.toBeInTheDocument();
    expect(screen.getByRole("button", { name: /^continue$/i })).toBeDisabled();
    expect(screen.getByRole("button", { name: /^Run checks and preview$/i })).toBeEnabled();
  });

});
