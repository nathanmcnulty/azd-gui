import { cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/svelte";
import { afterEach, describe, expect, it } from "vitest";
import App from "./App.svelte";

afterEach(cleanup);

describe("desktop experience", () => {
  it("opens a catalog template as a calculated deployment wizard", async () => {
    render(App);

    await fireEvent.click(screen.getByRole("button", { name: /browse catalog/i }));
    await fireEvent.click(
      screen.getAllByRole("button", { name: /configure deployment/i })[0]
    );

    expect(screen.getByRole("heading", { name: "Choose source" })).toBeInTheDocument();
    expect(screen.getByText(/STEP 1 OF 12/i)).toBeInTheDocument();
    const deploymentSteps = within(
      screen.getByRole("navigation", { name: /deployment steps/i })
    ).getAllByRole("button");
    expect(deploymentSteps[0]).toBeEnabled();
    expect(deploymentSteps.slice(1).every((step) => step.hasAttribute("disabled"))).toBe(true);

    const continueButton = screen.getByRole("button", { name: /continue/i });
    expect(continueButton).toBeDisabled();

    await fireEvent.input(screen.getByPlaceholderText(/empty destination folder/i), {
      target: { value: "C:\\Deployments\\myworkid-dev" }
    });
    expect(continueButton).toBeEnabled();
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
    await fireEvent.click(screen.getAllByRole("button", { name: "Projects" })[0]);
    await fireEvent.click(screen.getByRole("button", { name: /myworkid/i }));

    expect(screen.getByRole("navigation", { name: /project sections/i })).toBeInTheDocument();
    await waitFor(() => {
      expect(screen.getByText(/active environment/i)).toBeInTheDocument();
    });

    await fireEvent.click(screen.getByRole("button", { name: "resources" }));
    expect(screen.getByText(/structured azd state/i)).toBeInTheDocument();
  });
});
