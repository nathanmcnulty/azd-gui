import { cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/svelte";
import { afterEach, describe, expect, it } from "vitest";
import App from "./App.svelte";

afterEach(() => {
  cleanup();
  window.localStorage.clear();
  document.documentElement.dataset.theme = "";
});

describe("desktop experience", () => {
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

    const continueButton = screen.getByRole("button", { name: /continue/i });
    expect(continueButton).toBeDisabled();

    const destination = screen.getByPlaceholderText(/empty destination folder/i);
    expect(destination).toBeRequired();
    await fireEvent.input(destination, {
      target: { value: "C:\\Deployments\\entra-health-dev" }
    });
    expect(continueButton).toBeEnabled();
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
      await fireEvent.click(screen.getByRole("button", { name: /continue/i }));
    }

    expect(screen.getByRole("heading", { name: /configure environment/i })).toBeInTheDocument();
    await waitFor(() => {
      expect(screen.getByRole("combobox", { name: /subscription/i })).toHaveValue(
        "11111111-1111-1111-1111-111111111111"
      );
    });
    expect(screen.getByRole("combobox", { name: /azure region/i })).toHaveValue("westus2");
    expect(screen.getByRole("button", { name: /continue/i })).toBeEnabled();
  });

  it("runs registered permission checks before infrastructure preview", async () => {
    render(App);

    await fireEvent.click(screen.getByRole("button", { name: /browse catalog/i }));
    await fireEvent.click(
      screen.getAllByRole("button", { name: /configure deployment/i })[2]
    );
    await fireEvent.input(screen.getByPlaceholderText(/empty destination folder/i), {
      target: { value: "C:\\Deployments\\entra-health-permissions" }
    });

    for (let step = 0; step < 4; step += 1) {
      await fireEvent.click(screen.getByRole("button", { name: /continue/i }));
    }
    await waitFor(() => {
      expect(screen.getByRole("combobox", { name: /subscription/i })).toHaveValue(
        "11111111-1111-1111-1111-111111111111"
      );
    });
    await fireEvent.click(screen.getByRole("button", { name: /continue/i }));
    await fireEvent.input(screen.getByLabelText(/teams channel link/i), {
      target: { value: "https://teams.microsoft.com/l/channel/example/Alerts?groupId=group&tenantId=tenant" }
    });
    await fireEvent.click(screen.getByRole("button", { name: /continue/i }));
    await fireEvent.click(screen.getByRole("checkbox", { name: /approve pinned template code execution/i }));
    await fireEvent.click(screen.getByRole("button", { name: /continue/i }));
    await fireEvent.click(screen.getAllByRole("button", { name: /run checks and preview/i }).at(-1)!);

    const results = await screen.findByRole("region", { name: /permission check results/i });
    expect(within(results).getByText("Azure subscription access")).toBeInTheDocument();
    expect(within(results).getByText("Microsoft Graph directory access")).toBeInTheDocument();
    expect(within(results).getAllByText("passed")).toHaveLength(2);
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
    await fireEvent.click(screen.getByRole("button", { name: /continue/i }));
    await fireEvent.click(screen.getByRole("button", { name: /continue/i }));

    expect(screen.getByText("Azure CLI")).toBeInTheDocument();
    expect(screen.getByText("PowerShell")).toBeInTheDocument();
    expect(screen.queryByText("Microsoft Graph PowerShell")).not.toBeInTheDocument();
    expect(screen.getByRole("button", { name: /continue/i })).toBeEnabled();
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
    await fireEvent.click(screen.getByRole("button", { name: /continue/i }));

    expect(await screen.findByText(/interactive terminal required/i)).toBeInTheDocument();
    expect(screen.getByText(/local source can change/i)).toBeInTheDocument();
    expect(screen.getByText(/inferred from azure.yaml/i)).toBeInTheDocument();
    const continueButton = screen.getByRole("button", { name: /continue/i });
    expect(continueButton).toBeDisabled();
    await fireEvent.click(screen.getByRole("checkbox", { name: /trust this unvetted source/i }));
    expect(continueButton).toBeEnabled();
    await fireEvent.click(screen.getByRole("button", { name: /open external terminal/i }));

    expect(await screen.findByText("External terminal opened")).toBeInTheDocument();
    expect(screen.getByText("C:\\GitHub\\sample-azd-project")).toBeInTheDocument();
    expect(screen.getByText("azd up --environment dev")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: /copy command/i })).toBeEnabled();
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
});
