import { cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/svelte";
import { afterEach, describe, expect, it } from "vitest";
import App from "./App.svelte";

afterEach(() => {
  cleanup();
  window.localStorage.clear();
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
    const deploymentSteps = within(
      screen.getByRole("navigation", { name: /deployment steps/i })
    ).getAllByRole("button");
    expect(deploymentSteps[0]).toBeEnabled();
    expect(deploymentSteps.slice(1).every((step) => step.hasAttribute("disabled"))).toBe(true);

    const continueButton = screen.getByRole("button", { name: /continue/i });
    expect(continueButton).toBeDisabled();

    await fireEvent.input(screen.getByPlaceholderText(/empty destination folder/i), {
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
    await fireEvent.input(screen.getByLabelText(/tenant to monitor/i), {
      target: { value: "22222222-2222-2222-2222-222222222222" }
    });
    await fireEvent.click(screen.getByRole("button", { name: /continue/i }));
    await fireEvent.click(screen.getByRole("button", { name: "Run preflight" }));

    const results = await screen.findByRole("region", { name: /permission check results/i });
    expect(within(results).getByText("Azure subscription access")).toBeInTheDocument();
    expect(within(results).getByText("Microsoft Graph directory access")).toBeInTheDocument();
    expect(within(results).getAllByText("passed")).toHaveLength(2);
  });

  it("opens the command palette from the keyboard", async () => {
    render(App);
    await fireEvent.keyDown(document, { key: "p", ctrlKey: true });

    await waitFor(() => {
      expect(screen.getByRole("dialog", { name: /command palette/i })).toBeInTheDocument();
    });
    await waitFor(() => {
      expect(screen.getByPlaceholderText(/type a command/i)).toHaveFocus();
    });
  });

  it("opens the post-deployment project workspace", async () => {
    render(App);
    await fireEvent.click(screen.getByRole("button", { name: /^Projects,/ }));
    await fireEvent.click(screen.getByRole("button", { name: /maester/i }));

    expect(screen.getByRole("navigation", { name: /project sections/i })).toBeInTheDocument();
    await waitFor(() => {
      expect(screen.getByText(/active environment/i)).toBeInTheDocument();
    });

    await fireEvent.click(screen.getByRole("button", { name: "operations" }));
    expect(screen.getByText(/lifecycle history/i)).toBeInTheDocument();
    expect(screen.getByText("Deployment completed successfully.")).toBeInTheDocument();
    expect(screen.getByText(/3 recorded/i)).toBeInTheDocument();

    await fireEvent.click(screen.getByRole("button", { name: "resources" }));
    expect(screen.getByText(/structured azd state/i)).toBeInTheDocument();
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
    expect(window.localStorage.getItem("azd-gui.navigation.expanded")).toBe("false");

    firstRender.unmount();
    render(App);
    await waitFor(() => {
      expect(screen.getByRole("button", { name: /expand navigation/i })).toBeInTheDocument();
    });
  });
});
