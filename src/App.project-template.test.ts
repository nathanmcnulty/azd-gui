import { cleanup, fireEvent, render, screen } from "@testing-library/svelte";
import { afterEach, describe, expect, it, vi } from "vitest";

vi.mock("./lib/bridge", async (importOriginal) => {
  const actual = await importOriginal<typeof import("./lib/bridge")>();
  const { mockBootstrap } = await import("./lib/mock-data");
  const entry = structuredClone(mockBootstrap.catalog.entries[0]);
  entry.id = "github-private-project";
  entry.manifest = {
    schemaVersion: "1.0", prerequisites: [], connections: [],
    configuration: { groups: [{ id: "custom", title: "Custom settings", fields: [{
      id: "customRegion", environmentVariable: "DEPLOY_REGION", label: "Custom deployment region", type: "text", required: true
    }] }] }
  };
  return {
    ...actual,
    bootstrap: async () => ({ ...structuredClone(mockBootstrap), isNative: true,
      catalog: { ...mockBootstrap.catalog, entries: [] },
      projects: [{ ...mockBootstrap.projects[0], templateId: entry.id, name: "Saved GitHub deployment" }]
    }),
    getProjectTemplate: async () => ({ entry, digest: "saved-contract-digest" }),
    getProjectEnvironmentValues: async () => [{
      name: "DEPLOY_REGION", value: "westus2", isSet: true, sensitive: false, declared: true, required: true
    }]
  };
});

import App from "./App.svelte";

afterEach(() => { cleanup(); window.localStorage.clear(); });

describe("saved project template contract", () => {
  it("reopens guided configuration without the original template in a catalog", async () => {
    render(App);
    await fireEvent.click(await screen.findByRole("button", { name: /^Projects,/ }));
    await fireEvent.click(await screen.findByRole("button", { name: /Saved GitHub deployment/ }));
    await screen.findByText("Active environment");
    await fireEvent.click(screen.getByRole("button", { name: "environments" }));
    expect(await screen.findByLabelText(/Custom deployment region/)).toHaveValue("westus2");
    expect(screen.queryByText("No manifest-defined fields")).not.toBeInTheDocument();
  });
});
