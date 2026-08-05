import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/svelte";
import { afterEach, describe, expect, it, vi } from "vitest";
import type { OperationEvent } from "./lib/types";

const operationHarness = vi.hoisted(() => ({
  handler: undefined as ((event: OperationEvent) => void) | undefined,
  starts: [] as string[]
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
    recordOperation: async () => undefined
  };
});

import App from "./App.svelte";

afterEach(() => {
  cleanup();
  operationHarness.handler = undefined;
  operationHarness.starts = [];
});

describe("structured operation results", () => {
  it("warns about possible partial changes and retries the registered action", async () => {
    render(App);

    await fireEvent.click(screen.getAllByRole("button", { name: "Projects" })[0]);
    await fireEvent.click(screen.getByRole("button", { name: /maester/i }));
    await waitFor(() => expect(screen.getByText(/active environment/i)).toBeInTheDocument());

    await fireEvent.click(screen.getAllByRole("button", { name: "Provision" })[0]);
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
