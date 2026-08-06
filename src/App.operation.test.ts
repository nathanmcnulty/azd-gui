import { cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/svelte";
import { afterEach, describe, expect, it, vi } from "vitest";
import type { OperationEvent } from "./lib/types";

const operationHarness = vi.hoisted(() => ({
  handler: undefined as ((event: OperationEvent) => void) | undefined,
  starts: [] as string[],
  cancellations: [] as string[],
  stateReads: 0,
  failStateRefresh: false
}));

vi.mock("./lib/bridge", async (importOriginal) => {
  const actual = await importOriginal<typeof import("./lib/bridge")>();
  const { mockBootstrap } = await import("./lib/mock-data");

  return {
    ...actual,
    bootstrap: async () => ({ ...structuredClone(mockBootstrap), isNative: true }),
    onOperationEvent: async (handler: (event: OperationEvent) => void) => {
      operationHarness.handler = handler;
      return () => undefined;
    },
    startAzdOperation: async (action: string) => {
      operationHarness.starts.push(action);
      return `native-${action}-${operationHarness.starts.length}`;
    },
    cancelOperation: async (operationId: string) => {
      operationHarness.cancellations.push(operationId);
    },
    getProjectState: async () => {
      operationHarness.stateReads += 1;
      if (operationHarness.failStateRefresh) throw new Error("azd show was unavailable");
      return {
        name: "maester-automation",
        services: {
          web: {
            project: { path: "./src/web", language: "node" },
            target: {
              resourceIds: [
                "/subscriptions/11111111-1111-1111-1111-111111111111/resourceGroups/rg-preview/providers/Microsoft.Web/sites/preview"
              ]
            },
            ingressUrl: "https://preview.example.azurewebsites.net"
          }
        }
      };
    },
    getOperationHistory: async () => [],
    recordOperation: async () => undefined
  };
});

import App from "./App.svelte";

afterEach(() => {
  cleanup();
  operationHarness.handler = undefined;
  operationHarness.starts = [];
  operationHarness.cancellations = [];
  operationHarness.stateReads = 0;
  operationHarness.failStateRefresh = false;
});

describe("structured operation results", () => {
  it("requires a current infrastructure preview before azd up", async () => {
    render(App);

    await fireEvent.click(screen.getByRole("button", { name: /^Projects,/ }));
    await fireEvent.click(screen.getByRole("button", { name: /maester/i }));
    await waitFor(() => expect(screen.getByText(/active environment/i)).toBeInTheDocument());

    const deploy = screen.getByRole("button", { name: /deploy with azd up/i });
    expect(deploy).toBeDisabled();

    await fireEvent.click(screen.getByRole("button", { name: /run infrastructure preview/i }));
    await waitFor(() => expect(operationHarness.starts).toEqual(["preview"]));

    operationHarness.handler?.({
      operationId: "native-preview-1",
      kind: "preview",
      state: "succeeded",
      timestamp: "2026-08-05T20:15:00.000Z",
      stream: "system",
      level: "success",
      message: "Infrastructure preview completed successfully.",
      progress: 100,
      outcome: {
        resourceState: "unchanged",
        summary: "Infrastructure preview completed successfully.",
        guidance: "No Azure resource changes were applied by this action.",
        retryable: false
      }
    });

    await waitFor(() => expect(deploy).toBeEnabled());
    const preflight = screen.getByRole("region", { name: /deployment preflight/i });
    expect(within(preflight).getByText("Infrastructure preview completed successfully.")).toBeInTheDocument();
    expect(within(preflight).getByText("Azure unchanged")).toBeInTheDocument();

    await fireEvent.click(deploy);
    await waitFor(() => expect(operationHarness.starts).toEqual(["preview", "up"]));
    expect(deploy).toBeDisabled();

    operationHarness.handler?.({
      operationId: "native-up-2",
      kind: "up",
      state: "succeeded",
      timestamp: "2026-08-05T20:20:00.000Z",
      stream: "system",
      level: "success",
      message: "Operation completed successfully.",
      progress: 100,
      outcome: {
        resourceState: "changed",
        summary: "Deployment completed successfully.",
        guidance: "Refresh project state to inspect the resulting Azure resources.",
        retryable: false
      }
    });

    const deployment = await screen.findByRole("region", { name: /latest deployment/i });
    await waitFor(() => expect(within(deployment).getByText("web")).toBeInTheDocument());
    expect(within(deployment).getByText("Deployment completed successfully.")).toBeInTheDocument();
    expect(within(deployment).getByText("https://preview.example.azurewebsites.net/")).toBeInTheDocument();
    expect(within(deployment).getByRole("button", { name: /open endpoint/i })).toBeEnabled();
    expect(operationHarness.stateReads).toBeGreaterThanOrEqual(2);
  });

  it("refreshes partial deployment state and requires a new preview before retrying", async () => {
    render(App);

    await fireEvent.click(screen.getByRole("button", { name: /^Projects,/ }));
    await fireEvent.click(screen.getByRole("button", { name: /maester/i }));
    await waitFor(() => expect(screen.getByText(/active environment/i)).toBeInTheDocument());
    await fireEvent.click(screen.getByRole("button", { name: /run infrastructure preview/i }));
    await waitFor(() => expect(operationHarness.starts).toEqual(["preview"]));
    operationHarness.handler?.({
      operationId: "native-preview-1",
      kind: "preview",
      state: "succeeded",
      timestamp: "2026-08-05T20:15:00.000Z",
      stream: "system",
      level: "success",
      message: "Infrastructure preview completed successfully.",
      outcome: {
        resourceState: "unchanged",
        summary: "Infrastructure preview completed successfully.",
        guidance: "No Azure resource changes were applied by this action.",
        retryable: false
      }
    });
    const deploy = screen.getByRole("button", { name: /deploy with azd up/i });
    await waitFor(() => expect(deploy).toBeEnabled());
    await fireEvent.click(deploy);
    await waitFor(() => expect(operationHarness.starts).toEqual(["preview", "up"]));
    operationHarness.failStateRefresh = true;
    operationHarness.handler?.({
      operationId: "native-up-2",
      kind: "up",
      state: "failed",
      timestamp: "2026-08-05T20:20:00.000Z",
      stream: "system",
      level: "error",
      message: "Process exited with status 1.",
      progress: 64,
      outcome: {
        resourceState: "possiblyModified",
        summary: "Deployment did not complete.",
        guidance: "Review the output and template hooks before retrying. Azure may contain partial changes.",
        retryable: true
      }
    });

    const deployment = await screen.findByRole("region", { name: /latest deployment/i });
    expect(within(deployment).getByText("Deployment did not complete.")).toBeInTheDocument();
    expect(within(deployment).getByText("Partial changes possible")).toBeInTheDocument();
    await waitFor(() => {
      expect(within(deployment).getByText(/deployment state could not be refreshed/i)).toBeInTheDocument();
    });

    operationHarness.failStateRefresh = false;
    await fireEvent.click(within(deployment).getByRole("button", { name: /retry refresh/i }));
    await waitFor(() => expect(within(deployment).getByText("web")).toBeInTheDocument());
    await fireEvent.click(within(deployment).getByRole("button", { name: /preview before retry/i }));
    await waitFor(() => expect(operationHarness.starts).toEqual(["preview", "up", "preview"]));
  });

  it("cancels a running azd up and reports possible partial changes", async () => {
    render(App);

    await fireEvent.click(screen.getByRole("button", { name: /^Projects,/ }));
    await fireEvent.click(screen.getByRole("button", { name: /maester/i }));
    await waitFor(() => expect(screen.getByText(/active environment/i)).toBeInTheDocument());
    await fireEvent.click(screen.getByRole("button", { name: /run infrastructure preview/i }));
    await waitFor(() => expect(operationHarness.starts).toEqual(["preview"]));
    operationHarness.handler?.({
      operationId: "native-preview-1",
      kind: "preview",
      state: "succeeded",
      timestamp: "2026-08-05T20:15:00.000Z",
      stream: "system",
      level: "success",
      message: "Infrastructure preview completed successfully.",
      outcome: {
        resourceState: "unchanged",
        summary: "Infrastructure preview completed successfully.",
        guidance: "No Azure resource changes were applied by this action.",
        retryable: false
      }
    });
    const deploy = screen.getByRole("button", { name: /deploy with azd up/i });
    await waitFor(() => expect(deploy).toBeEnabled());
    await fireEvent.click(deploy);
    await waitFor(() => expect(operationHarness.starts).toEqual(["preview", "up"]));

    const running = screen.getByRole("region", { name: /latest deployment/i });
    await fireEvent.click(within(running).getByRole("button", { name: /cancel deployment/i }));
    expect(operationHarness.cancellations).toEqual(["native-up-2"]);
    operationHarness.handler?.({
      operationId: "native-up-2",
      kind: "up",
      state: "cancelled",
      timestamp: "2026-08-05T20:20:00.000Z",
      stream: "system",
      level: "warning",
      message: "Operation cancelled.",
      progress: 42,
      outcome: {
        resourceState: "possiblyModified",
        summary: "Deployment was cancelled.",
        guidance: "The local process stopped, but Azure operations already accepted may continue.",
        retryable: true
      }
    });

    await waitFor(() => expect(within(running).getByText("Deployment was cancelled.")).toBeInTheDocument());
    expect(within(running).getByText("Partial changes possible")).toBeInTheDocument();
  });

  it("invalidates the current preview when environment configuration changes", async () => {
    render(App);

    await fireEvent.click(screen.getByRole("button", { name: /^Projects,/ }));
    await fireEvent.click(screen.getByRole("button", { name: /maester/i }));
    await waitFor(() => expect(screen.getByText(/active environment/i)).toBeInTheDocument());
    await fireEvent.click(screen.getByRole("button", { name: /run infrastructure preview/i }));
    await waitFor(() => expect(operationHarness.starts).toEqual(["preview"]));

    operationHarness.handler?.({
      operationId: "native-preview-1",
      kind: "preview",
      state: "succeeded",
      timestamp: "2026-08-05T20:15:00.000Z",
      stream: "system",
      level: "success",
      message: "Infrastructure preview completed successfully.",
      outcome: {
        resourceState: "unchanged",
        summary: "Infrastructure preview completed successfully.",
        guidance: "No Azure resource changes were applied by this action.",
        retryable: false
      }
    });
    await waitFor(() => {
      expect(screen.getByRole("button", { name: /deploy with azd up/i })).toBeEnabled();
    });

    await fireEvent.click(screen.getByRole("button", { name: "environments" }));
    const schedule = await screen.findByRole("combobox", { name: /run frequency/i });
    await fireEvent.change(schedule, { target: { value: "weekly" } });
    await fireEvent.click(screen.getByRole("button", { name: "overview" }));

    expect(screen.getByRole("button", { name: /deploy with azd up/i })).toBeDisabled();
    expect(screen.getByText(/no current preview/i)).toBeInTheDocument();
  });

  it("warns about possible partial changes and retries the registered action", async () => {
    render(App);

    await fireEvent.click(screen.getByRole("button", { name: /^Projects,/ }));
    await fireEvent.click(screen.getByRole("button", { name: /maester/i }));
    await waitFor(() => expect(screen.getByText(/active environment/i)).toBeInTheDocument());

    await fireEvent.click(screen.getByRole("button", { name: /provision apply infrastructure changes/i }));
    await waitFor(() => expect(operationHarness.starts).toEqual(["provision"]));

    operationHarness.handler?.({
      operationId: "native-provision-1",
      kind: "provision",
      state: "failed",
      timestamp: new Date().toISOString(),
      stream: "stderr",
      level: "error",
      message: "azd provision exited with code 1.",
      progress: 64,
      outcome: {
        resourceState: "possiblyModified",
        summary: "Provisioning did not complete.",
        guidance:
          "Some Azure resources may have changed before the operation stopped. Review the output, inspect the environment, and retry when the cause is resolved.",
        retryable: true
      }
    });

    await fireEvent.click(screen.getByRole("button", { name: "operations" }));
    expect(screen.getByText("Provisioning did not complete.")).toBeInTheDocument();
    expect(screen.getByText(/some Azure resources may have changed/i)).toBeInTheDocument();

    await fireEvent.click(screen.getByRole("button", { name: "Retry" }));
    await waitFor(() => expect(operationHarness.starts).toEqual(["provision", "provision"]));
  });
});
