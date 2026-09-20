import { act, cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/svelte";
import { afterEach, describe, expect, it, vi } from "vitest";
import * as bridge from "./lib/bridge";
import App from "./App.svelte";

async function useNoninteractiveProjectFixture() {
  const snapshot = await bridge.getProjectTemplate("C:\\Deployments\\maester-lab");
  snapshot!.entry.manifest!.requiresInteractiveTerminal = false;
  vi.spyOn(bridge, "getProjectTemplate").mockResolvedValue(snapshot);
}

afterEach(() => {
  vi.restoreAllMocks();
  cleanup();
  window.localStorage.clear();
  document.documentElement.dataset.theme = "";
});

describe("desktop experience", () => {
  it("routes a reopened interactive project to the selected environment's terminal", async () => {
    const terminal = vi.spyOn(bridge, "openExternalTerminal");
    const starts = vi.spyOn(bridge, "startAzdOperation");
    const approvals = vi.spyOn(bridge, "requestExecutionApproval");
    render(App);
    await fireEvent.click(screen.getByRole("button", { name: /^Projects,/ }));
    await fireEvent.click(screen.getByRole("button", { name: /maester/i }));
    await screen.findByRole("heading", { name: "Continue in a terminal" });
    expect(screen.queryByRole("button", { name: /run infrastructure preview/i })).not.toBeInTheDocument();
    expect(screen.queryByRole("button", { name: /deploy with azd up/i })).not.toBeInTheDocument();
    expect(screen.queryByRole("button", { name: /^provision/i })).not.toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Down" })).toBeDisabled();
    await fireEvent.click(screen.getByRole("button", { name: "environments" }));
    await fireEvent.click(screen.getByRole("button", { name: /use test environment/i }));
    await screen.findByRole("button", { name: /use dev environment/i });
    await fireEvent.click(screen.getByRole("button", { name: "overview" }));
    await waitFor(() => expect(screen.getByRole("button", { name: /open deployment terminal/i })).toBeEnabled());
    await fireEvent.click(screen.getByRole("button", { name: /open deployment terminal/i }));
    expect(terminal).toHaveBeenCalledWith("C:\\Deployments\\maester-lab", "test");
    expect(await screen.findByText("External terminal opened")).toBeInTheDocument();
    expect(screen.getByText("azd up --environment test", { selector: "code" })).toBeInTheDocument();
    expect(starts).not.toHaveBeenCalled();
    expect(approvals).not.toHaveBeenCalled();
  });

  it("preserves unsaved configuration across navigation, refresh, and environment selection attempts", async () => {
    const select = vi.spyOn(bridge, "selectProjectEnvironment");
    const reads = vi.spyOn(bridge, "getProjectEnvironmentValues");
    render(App);
    await fireEvent.click(screen.getByRole("button", { name: /^Projects,/ }));
    await fireEvent.click(screen.getByRole("button", { name: /maester/i }));
    await fireEvent.click(await screen.findByRole("button", { name: "environments" }));
    await fireEvent.change(await screen.findByRole("combobox", { name: /permission profile/i }), { target: { value: "Minimal" } });
    const initialReads = reads.mock.calls.length;
    await fireEvent.click(screen.getByRole("button", { name: /use test environment/i }));
    expect(select).not.toHaveBeenCalled();
    await fireEvent.click(screen.getByRole("button", { name: /^Settings/ }));
    expect(screen.getByRole("alert")).toHaveTextContent(/keep your changes/i);
    await fireEvent.click(screen.getByRole("button", { name: "resources" }));
    await fireEvent.click(screen.getAllByRole("button", { name: /^refresh$/i }).at(-1)!);
    expect(screen.getByRole("combobox", { name: /permission profile/i })).toHaveValue("Minimal");
    expect(reads).toHaveBeenCalledTimes(initialReads);
    await fireEvent.click(screen.getByRole("button", { name: /discard changes/i }));
    await fireEvent.click(screen.getByRole("button", { name: /all projects/i }));
    expect(screen.getByRole("button", { name: /myworkid/i })).toBeInTheDocument();
  });

  it("protects write-only advanced input and allows explicit discard without a write", async () => {
    const writes = vi.spyOn(bridge, "setProjectEnvironmentValues");
    const terminal = vi.spyOn(bridge, "openExternalTerminal");
    render(App);
    await fireEvent.click(screen.getByRole("button", { name: /^Projects,/ }));
    await fireEvent.click(screen.getByRole("button", { name: /maester/i }));
    await fireEvent.click(await screen.findByRole("button", { name: "environments" }));
    await fireEvent.input(await screen.findByLabelText(/variable name/i), { target: { value: "CUSTOM_SETTING" } });
    await fireEvent.input(screen.getByLabelText(/new value/i), { target: { value: "sensitive draft" } });
    await fireEvent.click(screen.getByRole("button", { name: /^terminal$/i }));
    expect(terminal).not.toHaveBeenCalled();
    await fireEvent.click(screen.getByRole("button", { name: /all projects/i }));
    expect(screen.getByLabelText(/new value/i)).toHaveValue("sensitive draft");
    expect(screen.getByRole("alert")).not.toHaveTextContent("sensitive draft");
    await fireEvent.click(screen.getByRole("button", { name: /discard variable/i }));
    expect(screen.getByLabelText(/new value/i)).toHaveValue("");
    expect(writes).not.toHaveBeenCalled();
    await fireEvent.click(screen.getByRole("button", { name: /all projects/i }));
    expect(screen.getByRole("button", { name: /myworkid/i })).toBeInTheDocument();
  });

  it("can discard changes when a condition hides the last manifest field", async () => {
    const snapshot = await bridge.getProjectTemplate("C:\\Deployments\\maester-lab");
    const manifest = snapshot!.entry.manifest!;
    const field = manifest.configuration.groups.flatMap((group) => group.fields)
      .find((field) => field.label === "Permission profile")!;
    expect(field).toBeDefined();
    manifest.configuration.groups = [{
      ...manifest.configuration.groups[0],
      fields: [{ ...field, condition: { field: field.id, notEquals: "Minimal" } }]
    }];
    vi.spyOn(bridge, "getProjectTemplate").mockResolvedValue(snapshot);
    render(App);
    await fireEvent.click(screen.getByRole("button", { name: /^Projects,/ }));
    await fireEvent.click(screen.getByRole("button", { name: /maester/i }));
    await fireEvent.click(await screen.findByRole("button", { name: "environments" }));
    await fireEvent.change(await screen.findByRole("combobox", { name: /permission profile/i }), { target: { value: "Minimal" } });
    expect(screen.queryByRole("combobox", { name: /permission profile/i })).not.toBeInTheDocument();
    expect(screen.getByRole("button", { name: /save configuration/i })).toBeEnabled();
    await fireEvent.click(screen.getByRole("button", { name: /discard changes/i }));
    expect(await screen.findByRole("combobox", { name: /permission profile/i })).toBeInTheDocument();
    expect(screen.getByText("No unsaved changes")).toBeInTheDocument();
  });

  it.each(["save", "discard"])("requires resolving unsaved configuration before preview (%s)", async (resolution) => {
    await useNoninteractiveProjectFixture();
    const writes = vi.spyOn(bridge, "setProjectEnvironmentValues");
    render(App);
    await fireEvent.click(screen.getByRole("button", { name: /^Projects,/ }));
    await fireEvent.click(screen.getByRole("button", { name: /maester/i }));
    await fireEvent.click(await screen.findByRole("button", { name: "environments" }));
    const field = await screen.findByRole("combobox", { name: /permission profile/i });
    const original = (field as HTMLSelectElement).value;
    await fireEvent.change(field, { target: { value: "Minimal" } });
    expect(screen.getByText("Unsaved changes", { selector: ".status-pill" })).toBeInTheDocument();
    await fireEvent.click(screen.getByRole("button", { name: "overview" }));
    await fireEvent.click(screen.getByRole("checkbox", { name: /approve this project's template code execution/i }));
    expect(screen.getByRole("button", { name: /run infrastructure preview/i })).toBeDisabled();
    expect(screen.getByRole("button", { name: /^provision/i })).toBeDisabled();
    expect(screen.getByRole("button", { name: /^deploy deploy all/i })).toBeDisabled();
    await fireEvent.click(screen.getByRole("button", { name: /review unsaved changes/i }));
    await fireEvent.click(screen.getByRole("button", { name: resolution === "save" ? /save configuration/i : /discard changes/i }));
    await screen.findByText("No unsaved changes");
    expect(screen.getByRole("combobox", { name: /permission profile/i })).toHaveValue(resolution === "save" ? "Minimal" : original);
    expect(writes).toHaveBeenCalledTimes(resolution === "save" ? 1 : 0);
    await fireEvent.click(screen.getByRole("button", { name: "overview" }));
    expect(screen.getByRole("button", { name: /run infrastructure preview/i })).toBeEnabled();
  });

  it("guides projects without an environment instead of leaving configuration loading", async () => {
    await useNoninteractiveProjectFixture();
    const data = await bridge.bootstrap();
    data.projects = data.projects.map((project) => ({ ...project, environment: undefined }));
    vi.spyOn(bridge, "bootstrap").mockResolvedValue(data);
    vi.spyOn(bridge, "listProjectEnvironments").mockResolvedValue([]);
    const reads = vi.spyOn(bridge, "getProjectEnvironmentValues");
    render(App);
    await fireEvent.click(screen.getByRole("button", { name: /^Projects,/ }));
    await fireEvent.click(screen.getByRole("button", { name: /maester/i }));
    await fireEvent.click(await screen.findByRole("checkbox", { name: /approve this project's template code execution/i }));
    expect(screen.getByRole("button", { name: "Down" })).toBeDisabled();
    await fireEvent.click(await screen.findByRole("button", { name: /manage environments/i }));
    expect(await screen.findByText("No environment selected")).toBeInTheDocument();
    expect(screen.queryByText("Reading safe environment metadata…")).not.toBeInTheDocument();
    expect(screen.queryByRole("button", { name: /save configuration/i })).not.toBeInTheDocument();
    expect(screen.getByRole("button", { name: /new environment/i })).toBeEnabled();
    expect(reads).not.toHaveBeenCalled();
  });

  it("blocks lifecycle actions after a failed configuration read and recovers with retry", async () => {
    await useNoninteractiveProjectFixture();
    const reads = vi.spyOn(bridge, "getProjectEnvironmentValues").mockRejectedValueOnce(new Error("secret diagnostic"));
    render(App);
    await fireEvent.click(screen.getByRole("button", { name: /^Projects,/ }));
    await fireEvent.click(screen.getByRole("button", { name: /maester/i }));
    await screen.findByText("Configuration unavailable");
    await fireEvent.click(screen.getByRole("checkbox", { name: /approve this project's template code execution/i }));
    expect(screen.getByRole("button", { name: /run infrastructure preview/i })).toBeDisabled();
    expect(screen.getByRole("button", { name: /^provision/i })).toBeDisabled();
    expect(screen.getByRole("button", { name: /^deploy deploy all/i })).toBeDisabled();
    // Teardown can remain available when saved configuration is unreadable.
    expect(screen.getByRole("button", { name: "Down" })).toBeEnabled();
    await fireEvent.click(screen.getByRole("button", { name: /review configuration/i }));
    expect(screen.getByText("Unavailable", { selector: ".status-pill" })).toBeInTheDocument();
    expect(screen.queryByText("Ready", { selector: ".status-pill" })).not.toBeInTheDocument();
    expect(screen.getByRole("alert")).not.toHaveTextContent("secret diagnostic");
    expect(screen.queryByRole("combobox", { name: /permission profile/i })).not.toBeInTheDocument();
    await fireEvent.click(screen.getByRole("button", { name: /retry configuration read/i }));
    expect(await screen.findByRole("combobox", { name: /permission profile/i })).toBeInTheDocument();
    expect(reads).toHaveBeenCalledTimes(2);
    expect(screen.queryByRole("alert")).not.toBeInTheDocument();
    await fireEvent.click(screen.getByRole("button", { name: "overview" }));
    expect(screen.getByRole("button", { name: /run infrastructure preview/i })).toBeEnabled();
  });

  it("opens a catalog template as a calculated deployment wizard", async () => {
    render(App);

    await fireEvent.click(screen.getByRole("button", { name: /browse catalog/i }));
    await fireEvent.click(
      screen.getAllByRole("button", { name: /configure deployment/i })[2]
    );

    expect(screen.getByRole("heading", { name: "Choose source" })).toBeInTheDocument();
    expect(screen.getByText(/STEP 1 OF 9/i)).toBeInTheDocument();
    expect(screen.getByRole("button", { name: /vetted catalog/i })).toHaveAttribute(
      "aria-pressed",
      "true"
    );
    const deploymentSteps = within(
      screen.getByRole("navigation", { name: /deployment steps/i })
    ).getAllByRole("button");
    expect(deploymentSteps[0]).toBeEnabled();
    expect(deploymentSteps.slice(1).every((step) => step.hasAttribute("disabled"))).toBe(true);

    const continueButton = screen.getByRole("button", { name: /^continue$/i });
    expect(continueButton).toBeDisabled();
    expect(continueButton).toHaveAccessibleDescription("Choose a project destination to continue.");

    const destination = screen.getByPlaceholderText(/empty destination folder/i);
    expect(destination).toBeRequired();
    await fireEvent.input(destination, {
      target: { value: "C:\\Deployments\\entra-health-dev" }
    });
    expect(continueButton).toBeEnabled();
    expect(continueButton).not.toHaveAttribute("aria-describedby");
    expect(screen.getByRole("status").textContent).toBe("");
  });

  it("does not let a deferred draft restore overwrite an early destination edit", async () => {
    let resolveDraft!: (draft: Record<string, string | boolean | string[]> | null) => void;
    const loadDraft = vi.spyOn(bridge, "loadWizardDraft").mockImplementation(
      () => new Promise((resolve) => {
        resolveDraft = resolve;
      })
    );
    render(App);

    await fireEvent.click(screen.getByRole("button", { name: /browse catalog/i }));
    await fireEvent.click(
      screen.getAllByRole("button", { name: /configure deployment/i })[2]
    );
    await waitFor(() => expect(loadDraft).toHaveBeenCalledWith(
      "entra-health-monitoring",
      expect.objectContaining({ id: "entra-health-monitoring" })
    ));

    const destination = screen.getByPlaceholderText(/empty destination folder/i);
    await fireEvent.input(destination, {
      target: { value: "C:\\Deployments\\early-edit" }
    });
    await act(() => {
      resolveDraft({ projectPath: "C:\\Deployments\\restored-overwrite" });
    });
    expect(destination).toHaveValue("C:\\Deployments\\early-edit");
    expect(screen.queryByRole("status", { name: /saved wizard choices/i })).not.toBeInTheDocument();
  });

  it("ignores an old load when the same template is reopened", async () => {
    const resolvers: Array<(draft: Record<string, string | boolean | string[]> | null) => void> = [];
    const loadDraft = vi.spyOn(bridge, "loadWizardDraft").mockImplementation(
      () => new Promise((resolve) => {
        resolvers.push(resolve);
      })
    );
    render(App);

    await fireEvent.click(screen.getByRole("button", { name: /browse catalog/i }));
    await fireEvent.click(
      screen.getAllByRole("button", { name: /configure deployment/i })[2]
    );
    await waitFor(() => expect(loadDraft).toHaveBeenCalledTimes(1));
    await fireEvent.click(screen.getByRole("button", { name: /close deployment/i }));
    await fireEvent.click(
      screen.getAllByRole("button", { name: /configure deployment/i })[2]
    );
    await waitFor(() => expect(loadDraft).toHaveBeenCalledTimes(2));

    await act(() => {
      resolvers[0]({ projectPath: "C:\\Deployments\\old-load" });
    });
    expect(screen.getByPlaceholderText(/empty destination folder/i)).toHaveValue("");
    expect(screen.queryByRole("status", { name: /saved wizard choices/i })).not.toBeInTheDocument();
    await act(() => {
      resolvers[1](null);
    });
  });

  it("shows a generic notice when saved choices are restored", async () => {
    vi.spyOn(bridge, "loadWizardDraft").mockResolvedValue({
      projectPath: "C:\\Deployments\\restored"
    });
    render(App);

    await fireEvent.click(screen.getByRole("button", { name: /browse catalog/i }));
    await fireEvent.click(
      screen.getAllByRole("button", { name: /configure deployment/i })[2]
    );
    await waitFor(() => {
      expect(screen.getByPlaceholderText(/empty destination folder/i)).toHaveValue(
        "C:\\Deployments\\restored"
      );
    });
    expect(screen.getByRole("status", { name: /saved wizard choices/i })).toHaveTextContent(
      /saved choices restored/i
    );
  });

  it("reports draft load failures without exposing the raw error", async () => {
    vi.spyOn(bridge, "loadWizardDraft").mockRejectedValue(
      new Error("raw provider secret")
    );
    render(App);

    await fireEvent.click(screen.getByRole("button", { name: /browse catalog/i }));
    await fireEvent.click(
      screen.getAllByRole("button", { name: /configure deployment/i })[2]
    );
    const notice = await screen.findByRole("status", { name: /saved wizard choices/i });
    expect(notice).toHaveTextContent(/could not be loaded/i);
    expect(notice).not.toHaveTextContent("raw provider secret");
  });

  it("retries a failed draft save with the latest values", async () => {
    const saveDraft = vi.spyOn(bridge, "saveWizardDraft");
    saveDraft.mockRejectedValueOnce(new Error("raw save failure"));
    saveDraft.mockResolvedValue(undefined);
    vi.spyOn(bridge, "loadWizardDraft").mockResolvedValue(null);
    render(App);

    await fireEvent.click(screen.getByRole("button", { name: /browse catalog/i }));
    await fireEvent.click(
      screen.getAllByRole("button", { name: /configure deployment/i })[2]
    );
    const destination = screen.getByPlaceholderText(/empty destination folder/i);
    await fireEvent.input(destination, {
      target: { value: "C:\\Deployments\\latest-draft" }
    });
    await waitFor(() => expect(saveDraft).toHaveBeenCalledTimes(1), { timeout: 1500 });

    const notice = screen.getByRole("status", { name: /saved wizard choices/i });
    expect(notice).toHaveTextContent(/could not be saved locally/i);
    expect(notice).not.toHaveTextContent("raw save failure");
    await fireEvent.click(within(notice).getByRole("button", { name: /retry saving/i }));
    await waitFor(() => expect(saveDraft).toHaveBeenCalledTimes(2));
    await waitFor(() => expect(screen.queryByRole("status", { name: /saved wizard choices/i })).not.toBeInTheDocument());
    expect(saveDraft).toHaveBeenLastCalledWith(
      "entra-health-monitoring",
      expect.objectContaining({ projectPath: "C:\\Deployments\\latest-draft" }),
      expect.objectContaining({ id: "entra-health-monitoring" })
    );
  });

  it("flushes the original draft identity before another wizard starts", async () => {
    const saveDraft = vi.spyOn(bridge, "saveWizardDraft").mockResolvedValue(undefined);
    vi.spyOn(bridge, "loadWizardDraft").mockResolvedValue(null);
    render(App);

    await fireEvent.click(screen.getByRole("button", { name: /browse catalog/i }));
    await fireEvent.click(
      screen.getAllByRole("button", { name: /configure deployment/i })[2]
    );
    await fireEvent.input(screen.getByPlaceholderText(/empty destination folder/i), {
      target: { value: "C:\\Deployments\\original-draft" }
    });
    await fireEvent.click(screen.getByRole("button", { name: /close deployment/i }));
    await fireEvent.click(
      screen.getAllByRole("button", { name: /configure deployment/i })[1]
    );

    await waitFor(() => expect(saveDraft).toHaveBeenCalledWith(
      "entra-health-monitoring",
      expect.objectContaining({ projectPath: "C:\\Deployments\\original-draft" }),
      expect.objectContaining({
        id: "entra-health-monitoring",
        repository: "https://github.com/nathanmcnulty/azd-entra-health-monitoring"
      })
    ));
  });

  it("ignores source inspection results after the wizard is closed and replaced", async () => {
    const staleInspection = await bridge.inspectGitHubTemplate("https://github.com/example/stale-template");
    let resolveInspection!: (inspection: Awaited<ReturnType<typeof bridge.inspectGitHubTemplate>>) => void;
    const inspectGitHub = vi.spyOn(bridge, "inspectGitHubTemplate").mockImplementation(
      () => new Promise((resolve) => {
        resolveInspection = resolve;
      })
    );
    render(App);

    await fireEvent.click(screen.getByRole("button", { name: /browse catalog/i }));
    await fireEvent.click(
      screen.getAllByRole("button", { name: /configure deployment/i })[2]
    );
    await fireEvent.click(screen.getByRole("button", { name: /github repository/i }));
    await fireEvent.input(screen.getByPlaceholderText(/github.com\/owner/i), {
      target: { value: "https://github.com/example/stale-template" }
    });
    await fireEvent.click(screen.getByRole("button", { name: /^continue$/i }));
    await waitFor(() => expect(inspectGitHub).toHaveBeenCalledWith("https://github.com/example/stale-template"));

    await fireEvent.click(screen.getByRole("button", { name: /close deployment/i }));
    await fireEvent.click(
      screen.getAllByRole("button", { name: /configure deployment/i })[1]
    );
    await act(() => {
      resolveInspection(staleInspection);
    });

    expect(screen.getByRole("heading", { name: "Choose source" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: /vetted catalog/i })).toHaveAttribute(
      "aria-pressed",
      "true"
    );
    expect(screen.queryByText("GitHub template")).not.toBeInTheDocument();
  });

  it("does not apply discovery results after the Azure target changes", async () => {
    const staleDiscovery = await bridge.discoverAzureScope();
    let resolveDiscovery!: (discovery: Awaited<ReturnType<typeof bridge.discoverAzureScope>>) => void;
    const discover = vi.spyOn(bridge, "discoverAzureScope").mockImplementation(
      () => new Promise((resolve) => {
        resolveDiscovery = resolve;
      })
    );
    render(App);

    await fireEvent.click(screen.getByRole("button", { name: /browse catalog/i }));
    await fireEvent.click(
      screen.getAllByRole("button", { name: /configure deployment/i })[2]
    );
    await fireEvent.input(screen.getByPlaceholderText(/empty destination folder/i), {
      target: { value: "C:\\Deployments\\discovery-race" }
    });
    for (let step = 0; step < 4; step += 1) {
      await fireEvent.click(screen.getByRole("button", { name: /^continue$/i }));
    }
    await waitFor(() => expect(discover).toHaveBeenCalledTimes(1));

    const tenant = screen.getByPlaceholderText("Tenant ID");
    const subscription = screen.getByPlaceholderText("Subscription ID");
    await fireEvent.input(tenant, { target: { value: "current-tenant" } });
    await fireEvent.input(subscription, { target: { value: "current-subscription" } });
    await act(() => {
      resolveDiscovery(staleDiscovery);
    });

    expect(screen.getByPlaceholderText("Tenant ID")).toHaveValue("current-tenant");
    expect(screen.getByPlaceholderText("Subscription ID")).toHaveValue("current-subscription");
  });

  it("does not restore a draft over an explicitly opened local project", async () => {
    let resolveDraft!: (draft: Record<string, string | boolean | string[]> | null) => void;
    const loadDraft = vi.spyOn(bridge, "loadWizardDraft").mockImplementation(
      () => new Promise((resolve) => {
        resolveDraft = resolve;
      })
    );
    render(App);

    await fireEvent.click(screen.getByRole("button", { name: /open project/i }));
    await waitFor(() => expect(loadDraft).toHaveBeenCalledWith(
      "maester-automation",
      expect.objectContaining({ id: "maester-automation" })
    ));
    const folder = screen.getByPlaceholderText("Choose a folder");
    expect(folder).toHaveValue("C:\\GitHub\\sample-azd-project");
    await act(() => {
      resolveDraft({
        sourceMode: "catalog",
        projectPath: "C:\\Deployments\\restored-draft"
      });
    });

    expect(folder).toHaveValue("C:\\GitHub\\sample-azd-project");
    expect(screen.getByRole("button", { name: /open existing project/i })).toHaveAttribute(
      "aria-pressed",
      "true"
    );
  });

  it("advances an external catalog template without source inspection", async () => {
    const inspectGitHub = vi.spyOn(bridge, "inspectGitHubTemplate");
    const inspectLocal = vi.spyOn(bridge, "inspectLocalTemplate");
    render(App);

    await fireEvent.click(screen.getByRole("button", { name: /^Settings/ }));
    await fireEvent.click(screen.getByRole("button", { name: /add HTTPS or GitHub source/i }));
    const dialog = screen.getByRole("dialog", { name: /add catalog source/i });
    await fireEvent.input(within(dialog).getByPlaceholderText(/github.com\/owner/i), {
      target: { value: "https://example.com/async-race.catalog.json" }
    });
    await fireEvent.click(within(dialog).getByRole("button", { name: /preview source/i }));
    await fireEvent.click(await within(dialog).findByRole("button", { name: /trust and add/i }));
    await waitFor(() => expect(screen.queryByRole("dialog", { name: /add catalog source/i })).not.toBeInTheDocument());

    await fireEvent.click(screen.getByRole("button", { name: "Catalog" }));
    await fireEvent.click(
      screen.getAllByRole("button", { name: /configure deployment/i })[3]
    );
    await fireEvent.input(screen.getByPlaceholderText(/empty destination folder/i), {
      target: { value: "C:\\Deployments\\external-race" }
    });
    await fireEvent.click(screen.getByRole("button", { name: /^continue$/i }));

    expect(await screen.findByRole("heading", { name: "MyWorkID" })).toBeInTheDocument();
    expect(screen.getByText("Source integrity")).toBeInTheDocument();
    expect(inspectGitHub).not.toHaveBeenCalled();
    expect(inspectLocal).not.toHaveBeenCalled();
  });

  it("discovers Azure scope when the wizard reaches environment configuration", async () => {
    render(App);

    await fireEvent.click(screen.getByRole("button", { name: /browse catalog/i }));
    await fireEvent.click(
      screen.getAllByRole("button", { name: /configure deployment/i })[2]
    );
    await fireEvent.input(screen.getByPlaceholderText(/empty destination folder/i), {
      target: { value: "C:\\Deployments\\entra-health-dev" }
    });

    for (let step = 0; step < 4; step += 1) {
      await fireEvent.click(screen.getByRole("button", { name: /^continue$/i }));
    }

    expect(screen.getByRole("heading", { name: /configure environment/i })).toBeInTheDocument();
    await waitFor(() => {
      expect(screen.getByRole("combobox", { name: /subscription/i })).toHaveValue(
        "11111111-1111-1111-1111-111111111111"
      );
    });
    expect(screen.getByRole("combobox", { name: /azure region/i })).toHaveValue("westus2");
    expect(screen.getByRole("button", { name: /^continue$/i })).toBeEnabled();
    const environmentName = screen.getByLabelText(/environment name/i);
    await fireEvent.input(environmentName, { target: { value: "" } });
    expect(screen.getByRole("button", { name: /^continue$/i })).toBeDisabled();
    expect(screen.getByRole("button", { name: /^continue$/i })).toHaveAccessibleDescription("Enter an environment name to continue.");
    await fireEvent.input(environmentName, { target: { value: "dev" } });
    expect(screen.getByRole("button", { name: /^continue$/i })).toBeEnabled();
  });

  it("routes interactive templates to the terminal handoff instead of preview", async () => {
    render(App);

    await fireEvent.click(screen.getByRole("button", { name: /browse catalog/i }));
    await fireEvent.click(
      screen.getAllByRole("button", { name: /configure deployment/i })[2]
    );
    await fireEvent.input(screen.getByPlaceholderText(/empty destination folder/i), {
      target: { value: "C:\\Deployments\\entra-health-permissions" }
    });

    for (let step = 0; step < 4; step += 1) {
      await fireEvent.click(screen.getByRole("button", { name: /^continue$/i }));
    }
    await waitFor(() => {
      expect(screen.getByRole("combobox", { name: /subscription/i })).toHaveValue(
        "11111111-1111-1111-1111-111111111111"
      );
    });
    await fireEvent.click(screen.getByRole("button", { name: /^continue$/i }));
    await fireEvent.input(screen.getByLabelText(/teams channel link/i), {
      target: { value: "https://teams.microsoft.com/l/channel/example/Alerts?groupId=group&tenantId=tenant" }
    });
    await fireEvent.click(screen.getByRole("button", { name: /^continue$/i }));
    await fireEvent.click(screen.getByRole("checkbox", { name: /approve pinned template code execution/i }));
    await fireEvent.click(screen.getByRole("button", { name: /^continue$/i }));

    const preflightPage = screen.getByRole("heading", { name: /prepare for interactive terminal/i }).closest("section")!;
    expect(preflightPage).toBeInTheDocument();
    expect(within(preflightPage).queryByRole("button", { name: /run checks and preview/i })).not.toBeInTheDocument();
    expect(within(preflightPage).getByRole("button", { name: /prepare and open terminal/i })).toBeInTheDocument();
    expect(within(preflightPage).queryByText("Infrastructure preview")).not.toBeInTheDocument();
  });

  it("edits grouped review sections and clears approval until the updated values are reviewed", async () => {
    render(App);

    await fireEvent.click(screen.getByRole("button", { name: /browse catalog/i }));
    await fireEvent.click(
      screen.getAllByRole("button", { name: /configure deployment/i })[2]
    );
    await fireEvent.input(screen.getByPlaceholderText(/empty destination folder/i), {
      target: { value: "C:\\Deployments\\entra-health-review" }
    });

    const continueWizard = async () => {
      const button = screen.getByRole("button", { name: /^continue$/i });
      await waitFor(() => expect(button).toBeEnabled());
      await fireEvent.click(button);
    };

    for (let step = 0; step < 4; step += 1) await continueWizard();
    await waitFor(() => {
      expect(screen.getByRole("combobox", { name: /subscription/i })).toHaveValue(
        "11111111-1111-1111-1111-111111111111"
      );
    });
    await continueWizard();

    const initialLink = "https://teams.microsoft.com/l/channel/example/Initial";
    await fireEvent.input(screen.getByLabelText(/teams channel link/i), {
      target: { value: initialLink }
    });
    await continueWizard();
    expect(screen.getByRole("heading", { name: /review deployment/i })).toBeInTheDocument();
    const configurationReview = screen.getByRole("region", { name: /review entra health alerts/i });
    expect(within(configurationReview).getByText(initialLink)).toBeInTheDocument();

    const approval = screen.getByRole("checkbox", { name: /approve pinned template code execution/i });
    expect(screen.getByRole("button", { name: /^continue$/i })).toHaveAccessibleDescription("Review your choices and approve template code execution to continue.");
    await fireEvent.click(approval);
    expect(approval).toBeChecked();
    expect(screen.getByRole("button", { name: /^continue$/i })).not.toHaveAttribute("aria-describedby");

    await fireEvent.click(screen.getByRole("button", { name: "Edit Entra health alerts" }));
    expect(screen.getByRole("heading", { name: "Entra health alerts" })).toBeInTheDocument();
    const updatedLink = "https://teams.microsoft.com/l/channel/example/Updated";
    await fireEvent.input(screen.getByLabelText(/teams channel link/i), {
      target: { value: updatedLink }
    });
    await continueWizard();
    expect(screen.getByRole("heading", { name: /review deployment/i })).toBeInTheDocument();
    expect(within(screen.getByRole("region", { name: /review entra health alerts/i })).getByText(updatedLink)).toBeInTheDocument();
    expect(screen.getByRole("checkbox", { name: /approve pinned template code execution/i })).not.toBeChecked();

    await fireEvent.click(screen.getByRole("button", { name: "Edit Azure environment" }));
    expect(screen.getByRole("heading", { name: /configure environment/i })).toBeInTheDocument();
    await fireEvent.input(screen.getByLabelText(/environment name/i), {
      target: { value: "reviewed" }
    });
    await continueWizard();
    await continueWizard();
    expect(screen.getByRole("heading", { name: /review deployment/i })).toBeInTheDocument();
    expect(screen.getByText("reviewed", { selector: ".summary-list dd" })).toBeInTheDocument();
    expect(screen.getByRole("checkbox", { name: /approve pinned template code execution/i })).not.toBeChecked();
  });

  it("navigates the command palette by keyboard and restores focus", async () => {
    render(App);
    const trigger = screen.getByRole("button", { name: /search projects, templates, and commands/i });
    trigger.focus();
    await fireEvent.click(trigger);

    const query = await screen.findByRole("combobox", { name: /search commands/i });
    expect(screen.getByRole("dialog", { name: /command palette/i })).toHaveAttribute(
      "aria-modal",
      "true"
    );
    await waitFor(() => expect(query).toHaveFocus());
    await fireEvent.keyDown(query, { key: "ArrowDown" });
    expect(screen.getByRole("option", { name: /open projects/i })).toHaveAttribute(
      "aria-selected",
      "true"
    );
    expect(screen.getByRole("option", { name: /open projects/i })).toHaveAttribute("tabindex", "-1");
    expect(await fireEvent.keyDown(query, { key: "Tab" })).toBe(false);
    expect(query).toHaveFocus();
    await fireEvent.keyDown(query, { key: "Enter" });
    expect(await screen.findByRole("heading", { name: "Projects" })).toBeInTheDocument();
    await waitFor(() => expect(trigger).toHaveFocus());

    await fireEvent.keyDown(document, { key: "p", ctrlKey: true });
    const reopenedQuery = await screen.findByRole("combobox", { name: /search commands/i });
    await fireEvent.keyDown(reopenedQuery, { key: "Escape" });
    expect(screen.queryByRole("dialog", { name: /command palette/i })).not.toBeInTheDocument();
    await waitFor(() => expect(trigger).toHaveFocus());
  });

  it("provides an accessible name for catalog filtering", async () => {
    render(App);

    await fireEvent.click(screen.getByRole("button", { name: /browse catalog/i }));
    expect(screen.getByRole("textbox", { name: "Filter templates and tags" })).toBeInTheDocument();
  });

  it("requires explicit confirmation before clearing local application data", async () => {
    render(App);

    await fireEvent.click(screen.getByRole("button", { name: "Settings" }));
    await fireEvent.click(screen.getByRole("button", { name: "Clear local data" }));

    expect(screen.getByRole("button", { name: "Confirm clear" })).toBeInTheDocument();
    expect(screen.getByText(/project folders and CLI credentials remain/i)).toBeInTheDocument();
    await fireEvent.click(screen.getByRole("button", { name: "Cancel" }));
    expect(screen.queryByRole("button", { name: "Confirm clear" })).not.toBeInTheDocument();
  });

  it("requires review before a vetted per-user module installation", async () => {
    render(App);

    await fireEvent.click(screen.getByRole("button", { name: "Tools" }));
    const reviewInstall = screen.getByRole("button", { name: "Review install" });
    reviewInstall.focus();
    await fireEvent.click(reviewInstall);

    const review = screen.getByRole("dialog", {
      name: /review Microsoft Graph PowerShell installation/i
    });
    expect(within(review).getByText("PowerShell Gallery (PSGallery)")).toBeInTheDocument();
    expect(within(review).getByText("Current user")).toBeInTheDocument();
    expect(within(review).getByText("Current-user PowerShell module path")).toBeInTheDocument();
    expect(
      within(review).getByText(/Install-PSResource Microsoft\.Graph.*-Quiet -AcceptLicense/)
    ).toBeInTheDocument();
    await waitFor(() => {
      expect(within(review).getByRole("button", { name: /install for current user/i })).toHaveFocus();
    });

    await fireEvent.keyDown(within(review).getByRole("button", { name: /install for current user/i }), {
      key: "Escape"
    });
    expect(screen.queryByRole("dialog", { name: /review Microsoft Graph/i })).not.toBeInTheDocument();
    await waitFor(() => expect(reviewInstall).toHaveFocus());

    await fireEvent.click(reviewInstall);
    const reopenedReview = screen.getByRole("dialog", {
      name: /review Microsoft Graph PowerShell installation/i
    });

    await fireEvent.click(within(reopenedReview).getByRole("button", { name: /install for current user/i }));
    await waitFor(() => {
      expect(screen.queryByRole("dialog", { name: /review Microsoft Graph/i })).not.toBeInTheDocument();
    });
    const graphRow = screen.getByText("Microsoft Graph PowerShell").closest<HTMLElement>(".tool-row")!;
    expect(within(graphRow).getAllByText("2.0.0")).toHaveLength(2);
    expect(within(graphRow).getByText("ready")).toBeInTheDocument();
  });

  it("surfaces azd-managed updates without blocking tool readiness", async () => {
    render(App);

    await fireEvent.click(screen.getByRole("button", { name: "Tools" }));
    const azureCliRow = screen.getByText("Azure CLI").closest<HTMLElement>(".tool-row")!;

    expect(within(azureCliRow).getByText("2.88.0")).toBeInTheDocument();
    expect(within(azureCliRow).getByText("Latest 2.89.0")).toBeInTheDocument();
    expect(within(azureCliRow).getByText("update available")).toBeInTheDocument();
    expect(within(azureCliRow).getByRole("button", { name: "Copy update preview" })).toBeEnabled();
  });

  it("uses the pinned Maester prerequisites instead of an unused Graph module", async () => {
    render(App);

    await fireEvent.click(screen.getByRole("button", { name: /browse catalog/i }));
    await fireEvent.click(
      screen.getAllByRole("button", { name: /configure deployment/i })[1]
    );
    await fireEvent.input(screen.getByPlaceholderText(/empty destination folder/i), {
      target: { value: "C:\\Deployments\\maester-scopes" }
    });
    await fireEvent.click(screen.getByRole("button", { name: /^continue$/i }));
    await fireEvent.click(screen.getByRole("button", { name: /^continue$/i }));

    expect(screen.getByRole("heading", { name: "Maester coverage" })).toBeInTheDocument();
    await fireEvent.click(screen.getByRole("button", { name: /^continue$/i }));
    expect(await screen.findByText("ExchangeOnlineManagement")).toBeInTheDocument();
    await waitFor(() => expect(screen.getByRole("button", { name: /^continue$/i })).toBeDisabled());
    await fireEvent.click(screen.getByRole("button", { name: /^back$/i }));
    await fireEvent.click(screen.getByRole("checkbox", { name: /include exchange checks/i }));
    await fireEvent.click(screen.getByRole("button", { name: /^continue$/i }));
    await waitFor(() => expect(screen.queryByText("ExchangeOnlineManagement")).not.toBeInTheDocument());
    expect(screen.getByText("Azure CLI")).toBeInTheDocument();
    expect(screen.getByText("PowerShell")).toBeInTheDocument();
    expect(screen.queryByText("Microsoft Graph PowerShell")).not.toBeInTheDocument();
    expect(screen.getByRole("button", { name: /^continue$/i })).toBeEnabled();
  });

  it("applies a template recommendation profile without replacing other values", async () => {
    render(App);

    await fireEvent.click(screen.getByRole("button", { name: /browse catalog/i }));
    await fireEvent.click(
      screen.getAllByRole("button", { name: /configure deployment/i })[1]
    );
    await fireEvent.input(screen.getByPlaceholderText(/empty destination folder/i), {
      target: { value: "C:\\Deployments\\maester-profile" }
    });
    for (let step = 0; step < 2; step += 1) {
      const continueButton = screen.getByRole("button", { name: /^continue$/i });
      await waitFor(() => expect(continueButton).toBeEnabled());
      await fireEvent.click(continueButton);
    }

    expect(screen.getByRole("heading", { name: "Maester coverage" })).toBeInTheDocument();
    expect(screen.getByText(/eight baseline managed-identity Graph application read roles/i)).toBeInTheDocument();
    await fireEvent.click(screen.getByRole("button", { name: /apply minimal pilot/i }));
    expect(screen.getByRole("checkbox", { name: /include exchange checks/i })).not.toBeChecked();
    expect(screen.getByRole("checkbox", { name: /include teams checks/i })).not.toBeChecked();
    expect(screen.getByRole("checkbox", { name: /include azure checks/i })).not.toBeChecked();
    expect(screen.getByRole("checkbox", { name: /include secured web dashboard/i })).not.toBeChecked();
  });

  it("shows azd credential expiration in the Connections center", async () => {
    render(App);

    const connectionsButton = screen.getByRole("button", { name: "Connections, 3 connected" });
    await fireEvent.click(connectionsButton);
    const azdCard = screen
      .getByRole("heading", { name: "Azure Developer CLI" })
      .closest<HTMLElement>(".connection-card")!;
    const powerShellCard = screen
      .getByRole("heading", { name: "Azure PowerShell" })
      .closest<HTMLElement>(".connection-card")!;

    expect(document.querySelector(".connection-icon")).not.toBeInTheDocument();
    expect(within(azdCard).getByText("connected")).toHaveClass("success");
    expect(within(powerShellCard).getByText("disconnected")).toHaveClass("danger");
    expect(within(azdCard).getByText("Expires")).toBeInTheDocument();
    expect(
      within(azdCard).getByText(new Date("2030-01-15T18:30:00Z").toLocaleString())
    ).toBeInTheDocument();
  });

  it("resizes and maximizes the output panel", async () => {
    render(App);

    await fireEvent.click(screen.getByRole("button", { name: "Output" }));
    const shell = document.querySelector<HTMLElement>(".app-shell")!;
    const separator = screen.getByRole("separator", { name: "Resize output" });
    expect(shell.style.getPropertyValue("--output-height")).toBe("210px");
    expect(separator).toHaveAttribute("aria-valuenow", "210");

    await fireEvent(
      separator,
      new MouseEvent("pointerdown", { bubbles: true, clientY: 500 })
    );
    await fireEvent(
      window,
      new MouseEvent("pointermove", { bubbles: true, clientY: 450 })
    );
    await fireEvent(window, new MouseEvent("pointerup", { bubbles: true }));
    expect(shell.style.getPropertyValue("--output-height")).toBe("260px");
    expect(separator).toHaveAttribute("aria-valuenow", "260");

    await fireEvent.click(screen.getByRole("button", { name: "Maximize output" }));
    expect(shell).toHaveClass("output-maximized");
    await fireEvent.click(screen.getByRole("button", { name: "Restore output size" }));
    expect(shell).not.toHaveClass("output-maximized");
  });

  it("offers an external-terminal fallback for interactive local hooks", async () => {
    render(App);

    await fireEvent.click(screen.getByRole("button", { name: /open project/i }));
    await fireEvent.click(screen.getByRole("button", { name: /^continue$/i }));

    expect(await screen.findByText(/interactive terminal required/i)).toBeInTheDocument();
    expect(screen.getByText(/local source can change/i)).toBeInTheDocument();
    expect(screen.getByText(/inferred from azure.yaml/i)).toBeInTheDocument();
    const continueButton = screen.getByRole("button", { name: /^continue$/i });
    expect(continueButton).toBeDisabled();
    await fireEvent.click(screen.getByRole("checkbox", { name: /trust this unvetted source/i }));
    expect(continueButton).toBeEnabled();
    await fireEvent.click(continueButton);
    await fireEvent.click(screen.getByRole("button", { name: /apply minimal pilot/i }));
    await fireEvent.click(screen.getByRole("button", { name: /^back$/i }));
    await fireEvent.click(screen.getByRole("button", { name: /open external terminal/i }));

    expect(await screen.findByText("External terminal opened")).toBeInTheDocument();
    expect(screen.getByText("C:\\GitHub\\sample-azd-project")).toBeInTheDocument();
    expect(screen.getByText("azd up --environment dev")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: /copy command/i })).toBeEnabled();
  });

  it("prepares an interactive template before opening its terminal", async () => {
    const prepares = vi.spyOn(bridge, "prepareDeployment");
    const writes = vi.spyOn(bridge, "setProjectEnvironmentValues");
    const starts = vi.spyOn(bridge, "startAzdOperation");
    render(App);

    await fireEvent.click(screen.getByRole("button", { name: /open project/i }));
    await fireEvent.click(screen.getByRole("button", { name: /^continue$/i }));
    await fireEvent.click(screen.getByRole("checkbox", { name: /trust this unvetted source/i }));

    for (let attempt = 0; attempt < 10 && !screen.queryByRole("heading", { name: /review deployment/i }); attempt += 1) {
      const minimalProfile = screen.queryByRole("button", { name: /apply minimal pilot/i });
      if (minimalProfile) await fireEvent.click(minimalProfile);
      const continueButton = screen.getByRole("button", { name: /^continue$/i });
      await waitFor(() => expect(continueButton).toBeEnabled());
      await fireEvent.click(continueButton);
    }

    expect(screen.getByRole("heading", { name: /review deployment/i })).toBeInTheDocument();
    await fireEvent.click(screen.getByRole("button", { name: /^\d+\. Notifications$/i }));
    await fireEvent.input(screen.getByLabelText(/mail recipient/i), { target: { value: "review@example.invalid" } });
    await fireEvent.click(screen.getByRole("button", { name: /Review deployment$/i }));
    await fireEvent.click(screen.getByRole("checkbox", { name: /approve pinned template code execution/i }));
    await fireEvent.click(screen.getByRole("button", { name: /prepare and open terminal/i }));

    expect(await screen.findByText("External terminal opened")).toBeInTheDocument();
    expect(screen.getByText("azd up --environment dev")).toBeInTheDocument();
    expect(screen.getByRole("heading", { name: /review deployment/i })).toBeInTheDocument();
    expect(prepares).toHaveBeenCalledTimes(1);
    expect(starts).not.toHaveBeenCalled();

    await fireEvent.click(screen.getByRole("button", { name: /dismiss terminal notice/i }));
    await fireEvent.click(screen.getByRole("button", { name: /^\d+\. Notifications$/i }));
    await fireEvent.input(screen.getByLabelText(/mail recipient/i), { target: { value: "" } });
    await fireEvent.click(screen.getByRole("button", { name: /Review deployment$/i }));
    await fireEvent.click(screen.getByRole("checkbox", { name: /approve pinned template code execution/i }));
    await fireEvent.click(screen.getByRole("button", { name: /prepare and open terminal/i }));
    expect(await screen.findByText("External terminal opened")).toBeInTheDocument();
    expect(prepares).toHaveBeenCalledTimes(1);
    expect(writes).toHaveBeenCalledWith(expect.any(String), "dev", expect.arrayContaining([{ name: "MAIL_RECIPIENT", value: "" }]));
    expect(starts).not.toHaveBeenCalled();

    await fireEvent.click(screen.getByRole("button", { name: /dismiss terminal notice/i }));
    const creates = vi.spyOn(bridge, "createProjectEnvironment");
    await fireEvent.click(screen.getByRole("button", { name: /Configure environment$/i }));
    await fireEvent.input(screen.getByLabelText(/environment name/i), { target: { value: "staging" } });
    await fireEvent.click(screen.getByRole("button", { name: /Review deployment$/i }));
    await fireEvent.click(screen.getByRole("checkbox", { name: /approve pinned template code execution/i }));
    await fireEvent.click(screen.getByRole("button", { name: /prepare and open terminal/i }));
    expect(await screen.findByText("External terminal opened")).toBeInTheDocument();
    expect(prepares).toHaveBeenCalledTimes(1);
    expect(creates).toHaveBeenCalledWith(expect.any(String), "staging", expect.any(String), expect.any(String));
    expect(writes).toHaveBeenCalledWith(expect.any(String), "staging", expect.any(Array));
    expect(starts).not.toHaveBeenCalled();
  });

  it("ignores late project metadata after closing and reopening the same project", async () => {
    const states = [
      {
        services: {
          stale: {
            project: { language: "TypeScript" },
            target: {
              resourceIds: [
                "/subscriptions/11111111-1111-1111-1111-111111111111/resourceGroups/rg/providers/Microsoft.Web/sites/stale"
              ]
            }
          }
        }
      },
      {
        services: {
          current: {
            project: { language: "TypeScript" },
            target: {
              resourceIds: [
                "/subscriptions/11111111-1111-1111-1111-111111111111/resourceGroups/rg/providers/Microsoft.Web/sites/current"
              ]
            }
          }
        }
      }
    ] as const;
    const resolvers: Array<(state: Awaited<ReturnType<typeof bridge.getProjectState>>) => void> = [];
    const getState = vi.spyOn(bridge, "getProjectState").mockImplementation(
      () => new Promise((resolve) => {
        resolvers.push(resolve);
      })
    );
    render(App);

    await fireEvent.click(screen.getByRole("button", { name: /^Projects,/ }));
    await fireEvent.click(screen.getByRole("button", { name: /maester/i }));
    await waitFor(() => expect(getState).toHaveBeenCalledTimes(1));
    await fireEvent.click(screen.getByRole("button", { name: /all projects/i }));
    await fireEvent.click(screen.getByRole("button", { name: /maester/i }));
    await waitFor(() => expect(getState).toHaveBeenCalledTimes(2));

    await act(() => {
      resolvers[1](states[1]);
    });
    await fireEvent.click(await screen.findByRole("button", { name: "resources" }));
    expect(await screen.findByRole("heading", { name: "current" })).toBeInTheDocument();
    await act(() => {
      resolvers[0](states[0]);
    });
    expect(screen.getByRole("heading", { name: "current" })).toBeInTheDocument();
    expect(screen.queryByRole("heading", { name: "stale" })).not.toBeInTheDocument();
  });

  it("does not apply a delayed environment selection to another project", async () => {
    const selectedEnvironments = [
      {
        name: "staging",
        dotEnvPath: "C:\\Deployments\\maester-lab\\.azure\\staging\\.env",
        hasLocal: true,
        hasRemote: false,
        isDefault: true
      }
    ];
    let resolveSelection!: (environments: Awaited<ReturnType<typeof bridge.selectProjectEnvironment>>) => void;
    const selectEnvironment = vi.spyOn(bridge, "selectProjectEnvironment").mockImplementation(
      () => new Promise((resolve) => {
        resolveSelection = resolve;
      })
    );
    render(App);

    await fireEvent.click(screen.getByRole("button", { name: /^Projects,/ }));
    await fireEvent.click(screen.getByRole("button", { name: /maester/i }));
    await fireEvent.click(await screen.findByRole("button", { name: "environments" }));
    await fireEvent.click(screen.getByRole("button", { name: /use test environment/i }));
    await waitFor(() => expect(selectEnvironment).toHaveBeenCalled());
    await fireEvent.click(screen.getByRole("button", { name: /all projects/i }));
    await fireEvent.click(screen.getByRole("button", { name: /myworkid/i }));
    await waitFor(() => {
      expect(screen.getByRole("heading", { name: "Active environment" }).closest("section"))
        .toHaveTextContent("dev");
    });

    await act(() => {
      resolveSelection(selectedEnvironments);
    });
    expect(screen.getByRole("heading", { name: "Active environment" }).closest("section"))
      .toHaveTextContent("dev");
    expect(screen.queryByText("staging", { selector: ".environment-hero strong" })).not.toBeInTheDocument();
  });

  it("does not apply a delayed environment creation to another project", async () => {
    const createdEnvironments = [
      {
        name: "staging",
        dotEnvPath: "C:\\Deployments\\maester-lab\\.azure\\staging\\.env",
        hasLocal: true,
        hasRemote: false,
        isDefault: true
      }
    ];
    let resolveCreation!: (environments: Awaited<ReturnType<typeof bridge.createProjectEnvironment>>) => void;
    const createEnvironment = vi.spyOn(bridge, "createProjectEnvironment").mockImplementation(
      () => new Promise((resolve) => {
        resolveCreation = resolve;
      })
    );
    render(App);

    await fireEvent.click(screen.getByRole("button", { name: /^Projects,/ }));
    await fireEvent.click(screen.getByRole("button", { name: /maester/i }));
    await fireEvent.click(await screen.findByRole("button", { name: "environments" }));
    await fireEvent.click(screen.getByRole("button", { name: /new environment/i }));
    await fireEvent.input(screen.getByPlaceholderText("staging"), {
      target: { value: "staging" }
    });
    await fireEvent.click(screen.getByRole("button", { name: /create environment/i }));
    await waitFor(() => expect(createEnvironment).toHaveBeenCalled());
    await fireEvent.click(screen.getByRole("button", { name: /all projects/i }));
    await fireEvent.click(screen.getByRole("button", { name: /myworkid/i }));
    await waitFor(() => {
      expect(screen.getByRole("heading", { name: "Active environment" }).closest("section"))
        .toHaveTextContent("dev");
    });

    await act(() => {
      resolveCreation(createdEnvironments);
    });
    expect(screen.getByRole("heading", { name: "Active environment" }).closest("section"))
      .toHaveTextContent("dev");
    expect(screen.queryByText("staging", { selector: ".environment-hero strong" })).not.toBeInTheDocument();
  });

  it("waits for a pending environment selection before reopening the same project", async () => {
    const realSelect = bridge.selectProjectEnvironment;
    let releaseSelection!: () => Promise<void>;
    const selectEnvironment = vi.spyOn(bridge, "selectProjectEnvironment").mockImplementation(
      (projectPath, environment) => new Promise((resolve, reject) => {
        releaseSelection = async () => {
          try {
            resolve(await realSelect(projectPath, environment));
          } catch (error) {
            reject(error);
          }
        };
      })
    );
    render(App);

    await fireEvent.click(screen.getByRole("button", { name: /^Projects,/ }));
    await fireEvent.click(screen.getByRole("button", { name: /maester/i }));
    await fireEvent.click(await screen.findByRole("button", { name: "environments" }));
    await fireEvent.click(screen.getByRole("button", { name: /use test environment/i }));
    await waitFor(() => expect(selectEnvironment).toHaveBeenCalledTimes(1));

    await fireEvent.click(screen.getByRole("button", { name: /all projects/i }));
    await fireEvent.click(screen.getByRole("button", { name: /maester/i }));
    await waitFor(() => {
      expect(screen.getByRole("heading", { name: /maester/i })).toBeInTheDocument();
      expect(screen.getByRole("status")).toHaveTextContent(/finishing the earlier environment change before loading this project/i);
    });
    expect(screen.queryByRole("button", { name: /new environment/i })).not.toBeInTheDocument();

    await act(async () => {
      await releaseSelection();
    });
    await waitFor(() => {
      expect(screen.getByRole("heading", { name: "Active environment" }).closest("section"))
        .toHaveTextContent("test");
    });
    await fireEvent.click(screen.getByRole("button", { name: "environments" }));
    expect(screen.getByRole("button", { name: /new environment/i })).toBeEnabled();
  });

  it("keeps the editor open until a pending configuration save finishes", async () => {
    let resolveSave!: () => void;
    const save = vi.spyOn(bridge, "setProjectEnvironmentValues").mockImplementation(
      () => new Promise((resolve) => {
        resolveSave = resolve;
      })
    );
    const reads = vi.spyOn(bridge, "getProjectEnvironmentValues");
    render(App);

    await fireEvent.click(screen.getByRole("button", { name: /^Projects,/ }));
    await fireEvent.click(screen.getByRole("button", { name: /maester/i }));
    await fireEvent.click(await screen.findByRole("button", { name: "environments" }));
    const permissionProfile = await screen.findByRole("combobox", { name: /permission profile/i });
    await fireEvent.change(permissionProfile, { target: { value: "Minimal" } });
    await fireEvent.click(screen.getByRole("button", { name: /save configuration/i }));
    await waitFor(() => expect(save).toHaveBeenCalled());

    await fireEvent.click(screen.getByRole("button", { name: /all projects/i }));
    expect(screen.getByRole("alert")).toHaveTextContent(/keep your changes/i);
    expect(screen.getByRole("combobox", { name: /permission profile/i })).toHaveValue("Minimal");
    await act(() => { resolveSave(); });
    await screen.findByText("No unsaved changes");
    await fireEvent.click(screen.getByRole("button", { name: /all projects/i }));
    await fireEvent.click(screen.getByRole("button", { name: /myworkid/i }));
    await waitFor(() => {
      expect(screen.getByRole("heading", { name: /myworkid/i })).toBeInTheDocument();
      expect(screen.queryByText(/refreshing azd project state/i)).not.toBeInTheDocument();
    });
    expect(reads).toHaveBeenCalled();
    expect(screen.getByRole("heading", { name: /myworkid/i })).toBeInTheDocument();
  });

  it("preserves edits made while a configuration save and reload are pending", async () => {
    const baselineValues = await bridge.getProjectEnvironmentValues(
      "C:\\Deployments\\maester-lab",
      "lab"
    );
    let readCount = 0;
    let resolveReload!: (values: Awaited<ReturnType<typeof bridge.getProjectEnvironmentValues>>) => void;
    const reads = vi.spyOn(bridge, "getProjectEnvironmentValues").mockImplementation(
      () => {
        readCount += 1;
        if (readCount === 1) return Promise.resolve(baselineValues);
        return new Promise((resolve) => {
          resolveReload = resolve;
        });
      }
    );
    let resolveSave!: () => void;
    const save = vi.spyOn(bridge, "setProjectEnvironmentValues").mockImplementation(
      () => new Promise((resolve) => {
        resolveSave = resolve;
      })
    );
    render(App);

    await fireEvent.click(screen.getByRole("button", { name: /^Projects,/ }));
    await fireEvent.click(screen.getByRole("button", { name: /maester/i }));
    await fireEvent.click(await screen.findByRole("button", { name: "environments" }));
    const permissionProfile = await screen.findByRole("combobox", { name: /permission profile/i });
    await fireEvent.change(permissionProfile, { target: { value: "Minimal" } });
    await fireEvent.click(screen.getByRole("button", { name: /save configuration/i }));
    await waitFor(() => expect(save).toHaveBeenCalled());
    const latestRecipient = "latest@example.invalid";
    await fireEvent.input(screen.getByLabelText(/mail recipient/i), {
      target: { value: latestRecipient }
    });
    await act(() => {
      resolveSave();
    });
    await waitFor(() => expect(reads).toHaveBeenCalledTimes(2));
    await act(async () => {
      resolveReload(baselineValues);
      await Promise.resolve();
    });

    await waitFor(() => expect(screen.getByRole("button", { name: "Save configuration" })).toBeEnabled());
    expect(screen.getByLabelText(/mail recipient/i)).toHaveValue(latestRecipient);
    expect(screen.getByText(/1 unsaved/i)).toBeInTheDocument();
    expect(screen.getByRole("button", { name: /save configuration/i })).toBeEnabled();
  });

  it("shows a generic configuration save error while retaining editable values", async () => {
    const save = vi.spyOn(bridge, "setProjectEnvironmentValues").mockRejectedValue(
      new Error("raw configuration secret")
    );
    render(App);

    await fireEvent.click(screen.getByRole("button", { name: /^Projects,/ }));
    await fireEvent.click(screen.getByRole("button", { name: /maester/i }));
    await fireEvent.click(await screen.findByRole("button", { name: "environments" }));
    const permissionProfile = await screen.findByRole("combobox", { name: /permission profile/i });
    await fireEvent.change(permissionProfile, { target: { value: "Minimal" } });
    await fireEvent.click(screen.getByRole("button", { name: /save configuration/i }));
    const error = await screen.findByRole("alert");

    expect(error).toHaveTextContent(/configuration could not be saved/i);
    expect(error).not.toHaveTextContent("raw configuration secret");
    expect(permissionProfile).toHaveValue("Minimal");
    expect(screen.getByText(/1 unsaved/i)).toBeInTheDocument();
    expect(save).toHaveBeenCalledTimes(1);
  });

  it("opens the post-deployment project workspace", async () => {
    render(App);
    await fireEvent.click(screen.getByRole("button", { name: /^Projects,/ }));
    await fireEvent.click(screen.getByRole("button", { name: /maester/i }));

    expect(screen.getByRole("navigation", { name: /project sections/i })).toBeInTheDocument();
    await waitFor(() => {
      expect(screen.getByText(/active environment/i)).toBeInTheDocument();
    });
    await fireEvent.click(screen.getByRole("button", { name: /^terminal$/i }));
    expect(await screen.findByText("External terminal opened")).toBeInTheDocument();
    await fireEvent.click(screen.getByRole("button", { name: /dismiss terminal notice/i }));

    await fireEvent.click(screen.getByRole("button", { name: "operations" }));
    expect(screen.getByText(/lifecycle history/i)).toBeInTheDocument();
    expect(screen.getByText("Deployment completed successfully.")).toBeInTheDocument();
    expect(screen.getByText(/3 recorded/i)).toBeInTheDocument();

    await fireEvent.click(screen.getByRole("button", { name: "resources" }));
    const resources = screen.getByRole("region", { name: /project resources/i });
    expect(within(resources).getByText(/services and Azure resources/i)).toBeInTheDocument();
    expect(within(resources).getByRole("heading", { name: "web" })).toBeInTheDocument();
    expect(within(resources).getByText("https://preview.example.azurewebsites.net/")).toBeInTheDocument();
    expect(within(resources).getByText("preview", { selector: ".azure-resource-row strong" })).toBeInTheDocument();
    expect(within(resources).getByText("Microsoft.Web/sites")).toBeInTheDocument();
    expect(within(resources).getByRole("button", { name: /open preview in Azure/i })).toBeEnabled();
    expect(within(resources).getByText(/raw azd state/i)).toBeInTheDocument();

    await fireEvent.click(within(resources).getByRole("button", { name: /refresh/i }));
    await waitFor(() => {
      expect(screen.getByRole("region", { name: /project resources/i })).toBeInTheDocument();
    });
  });

  it("selects and creates azd project environments", async () => {
    await useNoninteractiveProjectFixture();
    render(App);
    await fireEvent.click(screen.getByRole("button", { name: /^Projects,/ }));
    await fireEvent.click(screen.getByRole("button", { name: /maester/i }));
    await fireEvent.click(await screen.findByRole("button", { name: "environments" }));

    await fireEvent.click(screen.getByRole("button", { name: /use test environment/i }));
    await waitFor(() => {
      expect(screen.getByRole("button", { name: /use dev environment/i })).toBeInTheDocument();
      expect(screen.queryByRole("button", { name: /use test environment/i })).not.toBeInTheDocument();
    });

    await fireEvent.click(screen.getByRole("button", { name: /new environment/i }));
    await fireEvent.input(screen.getByLabelText(/environment name/i), {
      target: { value: "staging.us_(1)" }
    });
    await fireEvent.click(screen.getByRole("button", { name: /create environment/i }));

    await waitFor(() => {
      expect(
        screen.getByText("staging.us_(1)", { selector: ".environment-table strong" })
      ).toBeInTheDocument();
      expect(screen.queryByRole("button", { name: /use staging\.us_\(1\) environment/i })).not.toBeInTheDocument();
      expect(
        within(screen.getByRole("contentinfo")).getByRole("button", { name: /staging\.us_\(1\)/i })
      ).toBeInTheDocument();
      expect(screen.getByText(/required$/)).toBeInTheDocument();
    });
    await fireEvent.click(screen.getByRole("button", { name: "overview" }));
    expect(screen.getByRole("button", { name: /run infrastructure preview/i })).toBeDisabled();
  });

  it("updates manifest and write-only raw environment values", async () => {
    render(App);
    await fireEvent.click(screen.getByRole("button", { name: /^Projects,/ }));
    await fireEvent.click(screen.getByRole("button", { name: /maester/i }));
    await fireEvent.click(await screen.findByRole("button", { name: "environments" }));

    const permissionProfile = await screen.findByRole("combobox", { name: /permission profile/i });
    expect(permissionProfile).toHaveValue("Extended");
    await fireEvent.change(permissionProfile, { target: { value: "Minimal" } });
    await fireEvent.click(screen.getByRole("button", { name: /save configuration/i }));
    await waitFor(() => expect(permissionProfile).toHaveValue("Minimal"));

    await fireEvent.input(screen.getByLabelText(/variable name/i), {
      target: { value: "CUSTOM_SECRET" }
    });
    await fireEvent.input(screen.getByLabelText(/new value/i), {
      target: { value: "never-render-this-value" }
    });
    await fireEvent.click(screen.getByRole("button", { name: /set variable/i }));
    await waitFor(() => expect(screen.getByText("CUSTOM_SECRET")).toBeInTheDocument());
    expect(screen.queryByText("never-render-this-value")).not.toBeInTheDocument();
  });

  it("uses one collapsible navigation tree and persists the preferred width", async () => {
    const firstRender = render(App);
    const primaryNavigation = screen.getByRole("navigation", { name: /primary navigation/i });

    expect(screen.getAllByRole("navigation", { name: /primary navigation/i })).toHaveLength(1);
    const projects = within(primaryNavigation).getByRole("button", { name: /^Projects,/ });
    await fireEvent.click(projects);
    expect(projects).toHaveAttribute("aria-current", "page");

    await fireEvent.click(screen.getByRole("button", { name: /collapse navigation/i }));

    expect(screen.getAllByRole("navigation", { name: /primary navigation/i })).toHaveLength(1);
    expect(projects).toHaveAttribute("aria-current", "page");
    expect(projects).toHaveAttribute("title", "Projects");
    expect(screen.getByRole("button", { name: /expand navigation/i })).toBeInTheDocument();
    expect(JSON.parse(window.localStorage.getItem("azd-gui.settings") ?? "{}"))
      .toMatchObject({ navigationExpanded: false });

    firstRender.unmount();
    render(App);
    await waitFor(() => {
      expect(screen.getByRole("button", { name: /expand navigation/i })).toBeInTheDocument();
    });
  });

  it("persists the selected theme", async () => {
    const firstRender = render(App);
    await fireEvent.click(screen.getByRole("button", { name: /^Settings/ }));
    const darkTheme = screen.getByRole("button", { name: "Dark" });
    expect(darkTheme).toHaveAttribute("aria-pressed", "false");
    await fireEvent.click(darkTheme);

    expect(document.documentElement.dataset.theme).toBe("dark");
    expect(darkTheme).toHaveAttribute("aria-pressed", "true");
    expect(JSON.parse(window.localStorage.getItem("azd-gui.settings") ?? "{}"))
      .toMatchObject({ theme: "dark" });

    firstRender.unmount();
    document.documentElement.dataset.theme = "";
    render(App);

    await waitFor(() => expect(document.documentElement.dataset.theme).toBe("dark"));
  });

  it("adds, reviews, and removes an external catalog without replacing bundled entries", async () => {
    render(App);
    await fireEvent.click(screen.getByRole("button", { name: /^Settings/ }));
    const addSource = screen.getByRole("button", { name: /add HTTPS or GitHub source/i });
    addSource.focus();
    await fireEvent.click(addSource);

    let dialog = screen.getByRole("dialog", { name: /add catalog source/i });
    const sourceInput = within(dialog).getByPlaceholderText(/github.com\/owner/i);
    await waitFor(() => expect(sourceInput).toHaveFocus());
    await fireEvent.keyDown(sourceInput, { key: "Escape" });
    expect(screen.queryByRole("dialog", { name: /add catalog source/i })).not.toBeInTheDocument();
    await waitFor(() => expect(addSource).toHaveFocus());

    await fireEvent.click(addSource);
    dialog = screen.getByRole("dialog", { name: /add catalog source/i });
    await fireEvent.input(within(dialog).getByPlaceholderText(/github.com\/owner/i), {
      target: { value: "https://example.com/azd-gui.catalog.json" }
    });
    await fireEvent.click(within(dialog).getByRole("button", { name: /preview source/i }));

    expect(await within(dialog).findByText("External catalog preview")).toBeInTheDocument();
    await fireEvent.click(within(dialog).getByRole("button", { name: /trust and add/i }));

    await waitFor(() => {
      expect(screen.queryByRole("dialog", { name: /add catalog source/i })).not.toBeInTheDocument();
    });
    const externalSource = screen.getByText("External catalog preview").closest<HTMLElement>(".setting-row")!;
    expect(within(externalSource).getByText(/3 pinned templates/i)).toBeInTheDocument();
    expect(screen.getByText("Bundled catalog").closest<HTMLElement>(".setting-row")).toHaveTextContent(
      "3 pinned templates"
    );

    await fireEvent.click(within(externalSource).getByRole("button", { name: /review latest/i }));
    const refreshDialog = screen.getByRole("dialog", { name: /add catalog source/i });
    expect(within(refreshDialog).getByPlaceholderText(/github.com\/owner/i)).toHaveValue(
      "https://example.com/azd-gui.catalog.json"
    );
    await fireEvent.click(within(refreshDialog).getByRole("button", { name: /cancel/i }));

    await fireEvent.click(within(externalSource).getByRole("button", { name: /^remove$/i }));
    await fireEvent.click(within(externalSource).getByRole("button", { name: /confirm remove/i }));
    await waitFor(() => expect(screen.queryByText("External catalog preview")).not.toBeInTheDocument());
    await fireEvent.click(screen.getByRole("button", { name: "Catalog" }));
    expect(screen.getAllByRole("button", { name: /configure deployment/i })).toHaveLength(3);
  });
  it("selects a dashboard security group and clears it after an Azure target change", async () => {
    render(App);
    await fireEvent.click(screen.getByRole("button", { name: /browse catalog/i }));
    await fireEvent.click(screen.getAllByRole("button", { name: /configure deployment/i })[1]);
    await fireEvent.input(screen.getByPlaceholderText(/empty destination folder/i), {
      target: { value: "C:\\Deployments\\maester-security-group" }
    });

    const continueWizard = async () => {
      const button = screen.getByRole("button", { name: /^continue$/i });
      await waitFor(() => expect(button).toBeEnabled());
      await fireEvent.click(button);
    };

    await continueWizard();
    await continueWizard();
    await fireEvent.click(screen.getByRole("checkbox", { name: /include secured web dashboard/i }));
    await fireEvent.click(screen.getByRole("checkbox", { name: /include exchange checks/i }));
    await continueWizard();
    await continueWizard();
    await continueWizard();
    await waitFor(() => {
      expect(screen.getByRole("combobox", { name: /subscription/i })).toHaveValue(
        "11111111-1111-1111-1111-111111111111"
      );
    });
    await continueWizard();

    const groupId = "a1111111-1111-4111-8111-111111111111";
    await fireEvent.input(screen.getByLabelText("Find a security group by name"), {
      target: { value: "Az" }
    });
    await fireEvent.click(screen.getByRole("button", { name: "Search groups" }));
    expect(await screen.findByText("Azure dashboard admins (preview)")).toBeInTheDocument();
    await fireEvent.click(screen.getByRole("button", { name: /select Azure dashboard admins/i }));
    expect(screen.getByRole("textbox", { name: /allowed security group/i })).toHaveValue(groupId);

    await fireEvent.click(screen.getByRole("button", { name: /^back$/i }));
    expect(screen.getByRole("heading", { name: /configure environment/i })).toBeInTheDocument();
    expect(screen.queryByRole("region", { name: /security group search/i })).not.toBeInTheDocument();

    const tenant = screen.getByRole("combobox", { name: /^tenant$/i });
    const subscription = screen.getByRole("combobox", { name: /subscription/i });
    const sandboxTenant = "00000000-0000-0000-0000-000000000002";
    const sandboxSubscription = "22222222-2222-2222-2222-222222222222";
    await fireEvent.change(tenant, { target: { value: sandboxTenant } });
    await waitFor(() => expect(subscription).toHaveValue(sandboxSubscription));
    await fireEvent.change(subscription, { target: { value: sandboxSubscription } });
    await waitFor(() => expect(screen.getByRole("button", { name: /^continue$/i })).toBeDisabled());

    const originalTenant = "72f988bf-86f1-41af-91ab-2d7cd011db47";
    const originalSubscription = "11111111-1111-1111-1111-111111111111";
    await fireEvent.change(tenant, { target: { value: originalTenant } });
    await waitFor(() => expect(subscription).toHaveValue(originalSubscription));
    await fireEvent.change(subscription, { target: { value: originalSubscription } });

    await continueWizard();
    expect(screen.getByRole("heading", { name: /web dashboard/i })).toBeInTheDocument();
    expect(screen.getByRole("textbox", { name: /allowed security group/i })).toHaveValue("");
  });
});
