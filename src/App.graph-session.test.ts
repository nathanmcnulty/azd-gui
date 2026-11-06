import { act, cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/svelte";
import { afterEach, describe, expect, it, vi } from "vitest";
import App from "./App.svelte";
import * as bridge from "./lib/bridge";
import { mockBootstrap } from "./lib/mock-data";
import fixture from "./lib/fixtures/conditional-requirements-v1.1.json";
import { effectiveConnectionRequirements, effectiveRequirements } from "./lib/requirements";
import type { ConnectionStatus, TemplateManifest } from "./lib/types";

const tenantId = "11111111-1111-4111-8111-111111111111";
const clientId = "22222222-2222-4222-8222-222222222222";
function setup() {
  vi.stubGlobal("isTauri", true);
  const data = structuredClone(mockBootstrap);
  data.isNative = true;
  const entry = structuredClone(data.catalog.entries[0]);
  entry.manifest = structuredClone(fixture) as TemplateManifest;
  entry.manifest.prerequisites = [];
  data.catalog.entries = [entry];
  const graph: ConnectionStatus = { ...data.connections.find((item) => item.id === "graphPowerShell")!, state: "disconnected", grantedScopes: [], account: undefined, tenantId: undefined };
  data.connections = [graph];
  vi.spyOn(bridge, "bootstrap").mockResolvedValue(data);
  vi.spyOn(bridge, "onOperationEvent").mockResolvedValue(() => {});
  vi.spyOn(bridge, "loadWizardDraft").mockResolvedValue({ projectPath: "C:\\Deployments\\graph-test", tenantId, environmentName: "dev" });
  vi.spyOn(bridge, "saveWizardDraft").mockResolvedValue(undefined);
  vi.spyOn(bridge, "checkRequirementPrerequisites").mockResolvedValue([]);
  vi.spyOn(bridge, "reviewWizardRequirements").mockImplementation(async (selected, values) => ({
    selection: { reviewedContractDigest: "test-contract", environment: "dev", target: { tenantId: String(values.tenantId || "") } },
    snapshot: { digest: `test-requirements-${values.mode}`, requirements: {
      schemaVersion: "1.0", contractDigest: "test-contract", environment: "dev", target: { tenantId: String(values.tenantId || "") },
      controllerValues: {}, prerequisites: [], connections: effectiveConnectionRequirements(selected.manifest!, values),
      permissionChecks: effectiveRequirements(selected.manifest!.permissionChecks ?? [], selected.manifest!, values)
    } }
  }));
  const dispose = vi.spyOn(bridge, "disposeGraphSession").mockResolvedValue(undefined);
  const request = vi.spyOn(bridge, "requestGraphSession").mockResolvedValue(undefined);
  const refresh = vi.spyOn(bridge, "refreshConnections").mockResolvedValue([graph]);
  return { graph, dispose, request, refresh };
}
async function openConnections() {
  render(App);
  await fireEvent.click(await screen.findByRole("button", { name: /browse catalog/i }));
  await fireEvent.click(await screen.findByRole("button", { name: /configure deployment/i }));
  await screen.findByRole("status", { name: /saved wizard choices/i });
  for (let i = 0; i < 3; i++) {
    const button = screen.getByRole("button", { name: /^continue$/i });
    await waitFor(() => expect(button).toBeEnabled());
    await fireEvent.click(button);
  }
  await screen.findByRole("region", { name: "Owned Graph session" });
  await fireEvent.input(screen.getByLabelText("Expected Graph account"), { target: { value: "operator@example.test" } });
  await fireEvent.input(screen.getByLabelText("Application client ID"), { target: { value: clientId } });
}
afterEach(() => {
  cleanup(); vi.restoreAllMocks(); vi.unstubAllGlobals(); window.localStorage.clear();
});

describe("owned Graph session lifecycle in the actual App", () => {
  it.each(["cancel", "account", "client", "tenant", "close"])("discards late sign-in after %s and keeps scopes unverified", async (change) => {
    const { request, refresh, dispose } = setup();
    let finish!: () => void;
    request.mockImplementationOnce(() => new Promise<void>((resolve) => { finish = resolve; }));
    await openConnections();
    await fireEvent.click(screen.getByRole("button", { name: /^Connect$/ }));
    await waitFor(() => expect(request).toHaveBeenCalledTimes(1));
    expect(request.mock.calls[0].slice(2)).toEqual(["operator@example.test", clientId]);
    const disposals = dispose.mock.calls.length;
    if (change === "cancel") await fireEvent.click(screen.getByRole("button", { name: "Cancel Graph sign-in" }));
    else if (change === "close") await fireEvent.click(screen.getByRole("button", { name: /close deployment/i }));
    else await fireEvent.input(screen.getByLabelText(change === "account" ? "Expected Graph account" : change === "client" ? "Application client ID" : "Graph tenant ID"), {
      target: { value: change === "account" ? "other@example.test" : "33333333-3333-4333-8333-333333333333" }
    });
    await waitFor(() => expect(dispose.mock.calls.length).toBeGreaterThan(disposals));
    await act(() => finish());
    expect(refresh).not.toHaveBeenCalled();
    if (change !== "close") {
      expect(screen.queryByRole("button", { name: "Cancel Graph sign-in" })).not.toBeInTheDocument();
      expect(screen.getByText("Granted scopes: None detected")).toBeInTheDocument();
      expect(screen.getByRole("button", { name: /^continue$/i })).toBeDisabled();
    }
  });

  it("does not restore scopes from a connection refresh completed after identity editing", async () => {
    const { request, refresh, graph } = setup();
    let finish!: (value: typeof graph[]) => void;
    refresh.mockImplementationOnce(() => new Promise((resolve) => { finish = resolve; }));
    await openConnections();
    await fireEvent.click(screen.getByRole("button", { name: /^Connect$/ }));
    await waitFor(() => expect(refresh).toHaveBeenCalledTimes(1));
    await fireEvent.input(screen.getByLabelText("Expected Graph account"), { target: { value: "other@example.test" } });
    await act(() => finish([{ ...graph, state: "connected", account: "operator@example.test", tenantId, grantedScopes: ["Directory.Read.All"] }]));
    expect(screen.getByText("Granted scopes: None detected")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: /^continue$/i })).toBeDisabled();
    // A subsequent explicit request uses the newly selected identity.
    refresh.mockResolvedValue([graph]);
    await fireEvent.click(screen.getByRole("button", { name: /^Connect$/ }));
    await waitFor(() => expect(request).toHaveBeenCalledTimes(2));
    expect(request.mock.calls[1][2]).toBe("other@example.test");
    expect(within(screen.getByRole("region", { name: "Owned Graph session" })).getByLabelText("Expected Graph account")).toHaveValue("other@example.test");
  });
});
