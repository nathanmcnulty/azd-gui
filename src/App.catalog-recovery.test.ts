import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/svelte";
import { afterEach, expect, it, vi } from "vitest";

const recovery = vi.hoisted(() => ({ applied: false, local: false, failed: false }));
vi.mock("./lib/bridge", async (importOriginal) => {
  const actual = await importOriginal<typeof import("./lib/bridge")>();
  const { mockBootstrap } = await import("./lib/mock-data");
  const entry = { ...structuredClone(mockBootstrap.catalog.entries[0]), id: "external-pilot.sample" };
  return {
    ...actual,
    bootstrap: async () => ({ ...structuredClone(mockBootstrap),
      projects: [{ ...mockBootstrap.projects[0], name: recovery.local ? "Legacy local deployment" : "Legacy external deployment", templateId: recovery.local ? "legacy-local" : entry.id, revision: recovery.local ? "local" : entry.commit }]
    }),
    getProjectTemplate: async () => {
      if (!recovery.applied) throw new Error("Review the catalog again.");
      return { entry, digest: "reviewed" };
    },
    refreshProjectTemplate: async () => {
      recovery.applied = true;
      return { entry, digest: "reviewed" };
    },
    getOperationHistory: async () => recovery.failed ? [{
      operationId: "failed-provision", kind: "provision", state: "failed", timestamp: new Date().toISOString(),
      level: "error", message: "Provisioning failed.", outcome: { resourceState: "possiblyModified", summary: "Provisioning failed.", guidance: "Review before retrying.", retryable: true }
    }] : [],
    getProjectEnvironmentValues: async () => []
  };
});
import App from "./App.svelte";

afterEach(() => { cleanup(); recovery.applied = false; recovery.local = false; recovery.failed = false; window.localStorage.clear(); });

it("recovers a missing external project contract through explicit catalog review", async () => {
  render(App);
  await fireEvent.click(await screen.findByRole("button", { name: /^Projects,/ }));
  await fireEvent.click(await screen.findByRole("button", { name: /Legacy external deployment/ }));
  expect(await screen.findByText("Saved template contract needs review")).toBeInTheDocument();
  expect(screen.getByRole("button", { name: "Refresh" })).toBeEnabled();
  expect(screen.getByRole("button", { name: "Down" })).toBeDisabled();
  await fireEvent.click(screen.getByRole("button", { name: "Review and add exact contract" }));
  await waitFor(() => expect(screen.queryByText("Saved template contract needs review")).not.toBeInTheDocument());
  expect(recovery.applied).toBe(true);
});

it("offers local re-review and keeps failed lifecycle retries blocked without a saved contract", async () => {
  recovery.local = true;
  recovery.failed = true;
  render(App);
  await fireEvent.click(await screen.findByRole("button", { name: /^Projects,/ }));
  await fireEvent.click(await screen.findByRole("button", { name: /Legacy local deployment/ }));
  expect(await screen.findByRole("button", { name: "Review current project files" })).toBeEnabled();
  await fireEvent.click(screen.getByRole("button", { name: "operations" }));
  expect(await screen.findByRole("button", { name: "Retry" })).toBeDisabled();
  await fireEvent.click(screen.getByRole("button", { name: "overview" }));
  await fireEvent.click(screen.getByRole("button", { name: "Review current project files" }));
  expect(await screen.findByRole("heading", { name: "Choose source" })).toBeInTheDocument();
});
