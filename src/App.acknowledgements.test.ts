import { cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/svelte";
import { afterEach, describe, expect, it, vi } from "vitest";
import App from "./App.svelte";
import * as bridge from "./lib/bridge";
import { mockBootstrap } from "./lib/mock-data";
import type { CatalogEntry, ProjectEnvironmentValue } from "./lib/types";

const tenantId = "11111111-1111-4111-8111-111111111111";
const otherTenantId = "22222222-2222-4222-8222-222222222222";
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

const optionalControllerCases = [
  { type: "text" as const, value: "", persisted: "" },
  { type: "select" as const, value: "", persisted: "" },
  { type: "boolean" as const, value: false, persisted: "false" },
  { type: "multiselect" as const, value: [] as string[], persisted: "" }
];

function optionalControllerEntry(type: typeof optionalControllerCases[number]["type"]) {
  const entry = acknowledgementEntry();
  const scope = entry.manifest!.configuration.groups[0].fields[0];
  scope.type = type;
  scope.required = false;
  delete scope.default;
  if (type === "text" || type === "boolean") delete scope.options;
  entry.manifest!.configuration.groups[1].fields[0].condition = type === "boolean"
    ? { field: "scope", equals: true }
    : type === "multiselect" ? { field: "scope", includes: "all" } : { field: "scope", equals: "all" };
  return entry;
}

function expectEmptyController(type: typeof optionalControllerCases[number]["type"]) {
  if (type === "text") expect(screen.getByRole("textbox", { name: "Directory scope" })).toHaveValue("");
  else if (type === "select") expect(screen.getByRole("combobox", { name: "Directory scope" })).toHaveValue("");
  else if (type === "boolean") expect(screen.getByRole("checkbox", { name: "Directory scope" })).not.toBeChecked();
  else {
    const field = screen.getByRole("group", { name: "Directory scope" });
    for (const checkbox of within(field).getAllByRole("checkbox")) expect(checkbox).not.toBeChecked();
  }
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
  const reads = vi.spyOn(bridge, "getProjectEnvironmentValues").mockResolvedValue(stored);
  const writes = vi.spyOn(bridge, "setProjectEnvironmentValues").mockResolvedValue(undefined);
  const approvals = vi.spyOn(bridge, "requestExecutionApproval").mockImplementation(async (request) => ({
    token: "acknowledgement-test-approval", projectPath: request.projectPath
  }));
  const starts = vi.spyOn(bridge, "startAzdOperation").mockResolvedValue("acknowledgement-test-operation");
  return { entry, project, saveDraft, reads, stored, writes, approvals, starts };
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

async function openProject() {
  await fireEvent.click(await screen.findByRole("button", { name: /^Projects,/ }));
  await fireEvent.click(await screen.findByRole("button", { name: /Acknowledgement project/ }));
  await screen.findByRole("button", { name: "environments" });
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

describe("fresh conditional acknowledgements", () => {
  it.each(optionalControllerCases)("submits an explicit displayed $type controller value when no default exists", async ({ type, value }) => {
    const { approvals, starts, project } = setup(optionalControllerEntry(type));
    vi.mocked(bridge.loadWizardDraft).mockResolvedValue({
      projectPath: "C:\\Deployments\\acknowledgement-test", environmentName: "dev", tenantId, subscriptionId
    });
    const prepare = vi.spyOn(bridge, "prepareDeployment").mockResolvedValue({
      ...project, path: "C:\\Deployments\\acknowledgement-test"
    });
    render(App);
    await openWizard();
    expectEmptyController(type);
    expect(screen.queryByRole("checkbox", { name: new RegExp(acknowledgementLabel) })).not.toBeInTheDocument();
    await continueWizard();
    expect(screen.getByRole("heading", { name: "Review deployment" })).toBeInTheDocument();
    await fireEvent.click(screen.getByRole("checkbox", { name: /approve pinned template code execution/i }));
    await continueWizard();
    await fireEvent.click(screen.getByRole("button", { name: /^run checks and preview$/i }));
    await waitFor(() => expect(starts).toHaveBeenCalledTimes(1));
    expect(approvals).toHaveBeenCalledWith(expect.objectContaining({
      action: "prepare", acknowledgementValues: expect.objectContaining({ SCOPE: value, DIRECTORY_ENABLED: true }), acknowledgementProofs: []
    }));
    expect(prepare).toHaveBeenCalledWith(expect.objectContaining({
      environmentValues: expect.objectContaining({ SCOPE: value, DIRECTORY_ENABLED: true }), acknowledgementProofs: []
    }));
  });

  it.each(optionalControllerCases)("initializes a missing saved $type controller only on explicit save without trapping navigation", async ({ type, persisted }) => {
    const { reads, stored, writes, project, approvals, starts } = setup(optionalControllerEntry(type));
    const missing = stored.filter((field) => field.name !== "SCOPE");
    reads.mockResolvedValue(missing);
    writes.mockImplementation(async (_path, _environment, values) => {
      reads.mockResolvedValue([...missing, ...values.map(({ name, value }) => ({
        name, value, isSet: true, sensitive: false, declared: true, required: false
      }))]);
    });
    render(App);
    await openProject();
    await fireEvent.click(await screen.findByRole("checkbox", { name: /reviewed this project's current files/i }));
    expect(screen.getByRole("button", { name: /run infrastructure preview/i })).toBeDisabled();
    await fireEvent.click(screen.getByRole("button", { name: "environments" }));
    await screen.findByText(/Save configuration to initialize the displayed acknowledgement controls: Directory scope/);
    expectEmptyController(type);
    expect(screen.getByText("No unsaved changes")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: /save configuration/i })).toBeEnabled();
    expect(screen.getByRole("button", { name: /discard changes/i })).toBeDisabled();
    expect(writes).not.toHaveBeenCalled();
    await fireEvent.click(screen.getByRole("button", { name: /all projects/i }));
    expect(screen.queryByText(/Keep your changes before continuing/)).not.toBeInTheDocument();
    await fireEvent.click(screen.getByRole("button", { name: /Acknowledgement project/ }));
    await fireEvent.click(await screen.findByRole("button", { name: "environments" }));
    await screen.findByText(/Save configuration to initialize the displayed acknowledgement controls/);
    expect(writes).not.toHaveBeenCalled();
    await fireEvent.click(screen.getByRole("button", { name: /save configuration/i }));
    await waitFor(() => expect(writes).toHaveBeenCalledWith(project.path, "dev", [{ name: "SCOPE", value: persisted }]));
    await waitFor(() => expect(screen.queryByText(/Save configuration to initialize the displayed acknowledgement controls/)).not.toBeInTheDocument());
    expect(screen.getByRole("button", { name: /save configuration/i })).toBeDisabled();
    expect(screen.queryByRole("checkbox", { name: new RegExp(acknowledgementLabel) })).not.toBeInTheDocument();
    await fireEvent.click(screen.getByRole("button", { name: "overview" }));
    await fireEvent.click(screen.getByRole("checkbox", { name: /reviewed this project's current files/i }));
    expect(screen.getByRole("button", { name: /run infrastructure preview/i })).toBeEnabled();
    await fireEvent.click(screen.getByRole("button", { name: /run infrastructure preview/i }));
    await waitFor(() => expect(starts).toHaveBeenCalledTimes(1));
    expect(approvals).toHaveBeenCalledWith(expect.objectContaining({ action: "preview", acknowledgementProofs: [] }));
    expect(writes).toHaveBeenCalledTimes(1);
  });

  it("uses a declared optional controller default without requiring initialization or writing on read", async () => {
    const entry = optionalControllerEntry("text");
    entry.manifest!.configuration.groups[0].fields[0].default = "selected";
    const { reads, stored, writes } = setup(entry);
    reads.mockResolvedValue(stored.filter((field) => field.name !== "SCOPE"));
    render(App);
    await openProject();
    await fireEvent.click(screen.getByRole("button", { name: "environments" }));
    expect(await screen.findByRole("textbox", { name: "Directory scope" })).toHaveValue("selected");
    expect(screen.queryByText(/Save configuration to initialize the displayed acknowledgement controls/)).not.toBeInTheDocument();
    expect(screen.getByRole("button", { name: /save configuration/i })).toBeDisabled();
    expect(writes).not.toHaveBeenCalled();
    await fireEvent.click(screen.getByRole("button", { name: "overview" }));
    await fireEvent.click(screen.getByRole("checkbox", { name: /reviewed this project's current files/i }));
    expect(screen.getByRole("button", { name: /run infrastructure preview/i })).toBeEnabled();
  });

  it("blocks until explicit acceptance and reviews both controllers, tenant, and persisted variable", async () => {
    setup();
    render(App);
    await openWizard();
    await continueWizard();
    const checkbox = acknowledgementCheckbox();
    expect(checkbox).not.toBeChecked();
    expect(checkbox).toHaveAttribute("aria-required", "true");
    expect(checkbox).toHaveAttribute("aria-invalid", "true");
    expect(checkbox).toHaveAccessibleDescription("Acknowledge the current tenant and scope before continuing.");
    expect(screen.getByRole("button", { name: /^continue$/i })).toBeDisabled();
    await fireEvent.click(checkbox);
    expect(checkbox).toBeChecked();
    expect(checkbox).toHaveAttribute("aria-invalid", "false");
    await continueWizard();
    const review = screen.getByRole("region", { name: "Review Directory acknowledgement" });
    expect(review).toHaveTextContent("Acknowledged");
    expect(review).toHaveTextContent('Enable directory integration (enabled) equals true; current value: true');
    expect(review).toHaveTextContent('Directory scope (scope) equals "all"; current value: "all"');
    expect(review).toHaveTextContent(`Tenant: ${tenantId}`);
    expect(review).toHaveTextContent("Persisted variable: ACK = true");
  });

  it.each(["field", "group"])("does not resurrect acceptance after the %s controller changes away and back", async (controller) => {
    setup();
    render(App);
    await openWizard();
    await continueWizard();
    await fireEvent.click(acknowledgementCheckbox());
    await fireEvent.click(screen.getByRole("button", { name: /^back$/i }));
    if (controller === "field") {
      const scope = screen.getByRole("combobox", { name: /Directory scope/ });
      await fireEvent.change(scope, { target: { value: "selected" } });
      await fireEvent.change(scope, { target: { value: "all" } });
    } else {
      const enabled = screen.getByRole("checkbox", { name: /Enable directory integration/ });
      await fireEvent.click(enabled);
      await fireEvent.click(enabled);
    }
    await continueWizard();
    expect(acknowledgementCheckbox()).not.toBeChecked();
    expect(screen.getByRole("button", { name: /^continue$/i })).toBeDisabled();
  });

  it("requires another check when the tenant changes away and back", async () => {
    setup();
    render(App);
    await openWizard();
    await continueWizard();
    await fireEvent.click(acknowledgementCheckbox());
    await fireEvent.click(screen.getByRole("button", { name: /^back$/i }));
    await fireEvent.click(screen.getByRole("button", { name: /^back$/i }));
    const tenant = screen.getByPlaceholderText("Tenant ID");
    await fireEvent.input(tenant, { target: { value: otherTenantId } });
    await fireEvent.input(tenant, { target: { value: tenantId } });
    await continueWizard();
    await continueWizard();
    expect(acknowledgementCheckbox()).not.toBeChecked();
    expect(screen.getByRole("button", { name: /^continue$/i })).toBeDisabled();
  });

  it("excludes acceptance from draft saves and starts unchecked even when a reopened draft contains true", async () => {
    const { saveDraft } = setup();
    render(App);
    await openWizard();
    await continueWizard();
    await fireEvent.click(acknowledgementCheckbox());
    await fireEvent.click(screen.getByRole("button", { name: /close deployment/i }));
    await waitFor(() => expect(saveDraft).toHaveBeenCalled());
    for (const [, values] of saveDraft.mock.calls) expect(values).not.toHaveProperty("ack");
    await fireEvent.click(screen.getByRole("button", { name: /configure deployment/i }));
    await screen.findByRole("status", { name: /saved wizard choices/i });
    for (let step = 0; step < 4; step += 1) await continueWizard();
    expect(acknowledgementCheckbox()).not.toBeChecked();
    expect(screen.getByRole("button", { name: /^continue$/i })).toBeDisabled();
  });

  it("never treats a default or profile assignment as a checkbox interaction", async () => {
    const entry = acknowledgementEntry();
    // Defense in depth against malformed frontend data; native manifest validation rejects this contract.
    entry.manifest!.configuration.groups[1].fields[0].default = true;
    entry.manifest!.recommendationProfiles = [{ id: "all", title: "All objects", description: "Select all directory objects.", values: { scope: "all", ack: true } }];
    setup(entry);
    render(App);
    await openWizard();
    await fireEvent.click(screen.getByRole("button", { name: "Apply All objects" }));
    await continueWizard();
    expect(acknowledgementCheckbox()).not.toBeChecked();
    expect(screen.getByRole("button", { name: /^continue$/i })).toBeDisabled();
  });

  it("clears wizard acceptance after cancelled approval and requires a fresh check before retrying", async () => {
    const { approvals, starts, project } = setup();
    approvals.mockRejectedValueOnce(new Error("Execution approval cancelled."));
    const prepare = vi.spyOn(bridge, "prepareDeployment").mockResolvedValue({
      ...project, path: "C:\\Deployments\\acknowledgement-test"
    });
    render(App);
    await openWizard();
    await continueWizard();
    await fireEvent.click(acknowledgementCheckbox());
    await continueWizard();
    await fireEvent.click(screen.getByRole("checkbox", { name: /approve pinned template code execution/i }));
    await continueWizard();
    await fireEvent.click(screen.getByRole("button", { name: /^run checks and preview$/i }));
    await waitFor(() => expect(approvals).toHaveBeenCalledTimes(1));
    await waitFor(() => expect(screen.getByRole("button", { name: /^run checks and preview$/i })).toBeDisabled());
    expect(prepare).not.toHaveBeenCalled();
    expect(starts).not.toHaveBeenCalled();
    await fireEvent.click(screen.getByRole("button", { name: /^back$/i }));
    const review = screen.getByRole("region", { name: "Review Directory acknowledgement" });
    expect(review).toHaveTextContent("Fresh acknowledgement required");
    await fireEvent.click(within(review).getByRole("button", { name: "Edit Directory acknowledgement" }));
    expect(acknowledgementCheckbox()).not.toBeChecked();
    expect(screen.getByRole("button", { name: /^continue$/i })).toBeDisabled();
    await fireEvent.click(acknowledgementCheckbox());
    await continueWizard();
    await fireEvent.click(screen.getByRole("checkbox", { name: /approve pinned template code execution/i }));
    await continueWizard();
    await fireEvent.click(screen.getByRole("button", { name: /^run checks and preview$/i }));
    await waitFor(() => expect(starts).toHaveBeenCalledTimes(1));
    expect(prepare).toHaveBeenCalledTimes(1);
    expect(approvals.mock.calls.map(([request]) => request.action)).toEqual(["prepare", "prepare", "preview"]);
    expect(prepare).toHaveBeenCalledWith(expect.objectContaining({
      acknowledgementProofs: [{ fieldId: "ack", tenantId, controllingValues: { scope: "all", enabled: true } }]
    }));
  });

  it("requires an explicit check for saved true before preview without an unnecessary configuration write", async () => {
    const { writes, approvals, starts } = setup();
    render(App);
    await openProject();
    await fireEvent.click(await screen.findByRole("checkbox", { name: /reviewed this project's current files/i }));
    expect(screen.getByRole("button", { name: /run infrastructure preview/i })).toBeDisabled();
    await fireEvent.click(screen.getByRole("button", { name: "environments" }));
    const checkbox = await screen.findByRole("checkbox", { name: new RegExp(acknowledgementLabel) });
    expect(checkbox).not.toBeChecked();
    await fireEvent.click(checkbox);
    expect(screen.getByText("No unsaved changes")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: /save configuration/i })).toBeDisabled();
    expect(writes).not.toHaveBeenCalled();
    await fireEvent.click(screen.getByRole("button", { name: "overview" }));
    const preview = screen.getByRole("button", { name: /run infrastructure preview/i });
    expect(preview).toBeEnabled();
    await fireEvent.click(preview);
    await waitFor(() => expect(approvals).toHaveBeenCalledWith(expect.objectContaining({
      action: "preview", acknowledgementProofs: [{ fieldId: "ack", tenantId, controllingValues: { scope: "all", enabled: true } }]
    })));
    await waitFor(() => expect(starts).toHaveBeenCalledWith("preview", expect.any(String), "dev", "acknowledgement-test-approval"));
    expect(writes).not.toHaveBeenCalled();
  });

  it("clears project acceptance after discarding edits and closing then reopening the workspace", async () => {
    const { writes } = setup();
    render(App);
    await openProject();
    await fireEvent.click(screen.getByRole("button", { name: "environments" }));
    await screen.findByRole("checkbox", { name: new RegExp(acknowledgementLabel) });
    await fireEvent.click(acknowledgementCheckbox());
    const scope = screen.getByRole("combobox", { name: /Directory scope/ });
    await fireEvent.change(scope, { target: { value: "selected" } });
    await fireEvent.change(scope, { target: { value: "all" } });
    expect(acknowledgementCheckbox()).not.toBeChecked();
    await fireEvent.click(acknowledgementCheckbox());
    await fireEvent.click(screen.getByRole("button", { name: /discard changes/i }));
    expect(acknowledgementCheckbox()).not.toBeChecked();
    expect(screen.getByText("No unsaved changes")).toBeInTheDocument();
    await fireEvent.click(acknowledgementCheckbox());
    await fireEvent.click(screen.getByRole("button", { name: /all projects/i }));
    await fireEvent.click(screen.getByRole("button", { name: /Acknowledgement project/ }));
    await fireEvent.click(await screen.findByRole("button", { name: "environments" }));
    expect(await screen.findByRole("checkbox", { name: new RegExp(acknowledgementLabel) })).not.toBeChecked();
    expect(within(screen.getByRole("region", { name: "Review Directory acknowledgement" })).getByText("Fresh acknowledgement required")).toBeInTheDocument();
    expect(writes).not.toHaveBeenCalled();
  });

  it("clears saved-project acceptance after cancelled approval and keeps preview blocked until another check", async () => {
    const { approvals, starts, writes } = setup();
    approvals.mockRejectedValueOnce(new Error("Execution approval cancelled."));
    render(App);
    await openProject();
    await fireEvent.click(await screen.findByRole("checkbox", { name: /reviewed this project's current files/i }));
    await fireEvent.click(screen.getByRole("button", { name: "environments" }));
    await screen.findByRole("checkbox", { name: new RegExp(acknowledgementLabel) });
    await fireEvent.click(acknowledgementCheckbox());
    await fireEvent.click(screen.getByRole("button", { name: "overview" }));
    await fireEvent.click(screen.getByRole("button", { name: /run infrastructure preview/i }));
    await waitFor(() => expect(approvals).toHaveBeenCalledTimes(1));
    await waitFor(() => expect(screen.getByRole("button", { name: /run infrastructure preview/i })).toBeDisabled());
    expect(starts).not.toHaveBeenCalled();
    await fireEvent.click(screen.getByRole("button", { name: "environments" }));
    expect(acknowledgementCheckbox()).not.toBeChecked();
    expect(screen.getByText("No unsaved changes")).toBeInTheDocument();
    await fireEvent.click(acknowledgementCheckbox());
    await fireEvent.click(screen.getByRole("button", { name: "overview" }));
    expect(screen.getByRole("button", { name: /run infrastructure preview/i })).toBeEnabled();
    await fireEvent.click(screen.getByRole("button", { name: /run infrastructure preview/i }));
    await waitFor(() => expect(starts).toHaveBeenCalledTimes(1));
    expect(approvals).toHaveBeenCalledTimes(2);
    expect(approvals).toHaveBeenLastCalledWith(expect.objectContaining({
      action: "preview", acknowledgementProofs: [{ fieldId: "ack", tenantId, controllingValues: { scope: "all", enabled: true } }]
    }));
    expect(writes).not.toHaveBeenCalled();
  });
});
