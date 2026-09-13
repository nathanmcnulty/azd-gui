import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/svelte";
import { afterEach, expect, it, vi } from "vitest";

const recovery = vi.hoisted(() => ({ applied: false }));
vi.mock("./lib/bridge", async (importOriginal) => {
  const actual = await importOriginal<typeof import("./lib/bridge")>();
  const { mockBootstrap } = await import("./lib/mock-data");
  const entry = { ...structuredClone(mockBootstrap.catalog.entries[0]), id: "external-pilot.sample" };
  return {
    ...actual,
    bootstrap: async () => ({ ...structuredClone(mockBootstrap),
      projects: [{ ...mockBootstrap.projects[0], name: "Legacy external deployment", templateId: entry.id, revision: entry.commit }]
    }),
    getProjectTemplate: async () => {
      if (!recovery.applied) throw new Error("Review the catalog again.");
      return { entry, digest: "reviewed" };
    },
    refreshProjectTemplate: async () => {
      recovery.applied = true;
      return { entry, digest: "reviewed" };
    },
    getProjectEnvironmentValues: async () => []
  };
});
import App from "./App.svelte";

afterEach(() => { cleanup(); recovery.applied = false; window.localStorage.clear(); });

it("recovers a missing external project contract through explicit catalog review", async () => {
  render(App);
  await fireEvent.click(await screen.findByRole("button", { name: /^Projects,/ }));
  await fireEvent.click(await screen.findByRole("button", { name: /Legacy external deployment/ }));
  expect(await screen.findByText("Catalog contract needs review")).toBeInTheDocument();
  await fireEvent.click(screen.getByRole("button", { name: "Use reviewed catalog contract" }));
  await waitFor(() => expect(screen.queryByText("Catalog contract needs review")).not.toBeInTheDocument());
  expect(recovery.applied).toBe(true);
});
