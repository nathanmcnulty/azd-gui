import { cleanup, render, screen, waitFor } from "@testing-library/svelte";
import { afterEach, describe, expect, it, vi } from "vitest";
import ProjectValidationReport from "./ProjectValidationReport.svelte";

const readReport = vi.hoisted(() => vi.fn());

vi.mock("../lib/bridge", () => ({
  getProjectValidationReport: readReport
}));

afterEach(() => {
  cleanup();
  readReport.mockReset();
});

const project = { id: "sample", name: "Sample", path: "C:\\Projects\\sample", environment: "dev", state: "ready" } as const;

describe("project validation report", () => {
  it("shows recorded checks independently from deployment state", async () => {
    readReport.mockResolvedValue({
      state: "available",
      report: {
        startedAt: "2026-09-12T12:00:00Z",
        completedAt: "2026-09-12T12:00:01Z",
        durationMs: 1000,
        mode: "verify",
        outcome: "passedWithWarnings",
        environmentBinding: "environmentMatch",
        summary: { pass: 1, fail: 0, warning: 1, info: 0, skipped: 0, planned: 0 },
        checks: [{ id: "infrastructure.resource-group", phase: "infrastructure", status: "pass", sideEffect: "readOnly", durationMs: 5 }]
      }
    });

    render(ProjectValidationReport, { project });

    expect(await screen.findByText("passedWithWarnings")).toBeInTheDocument();
    expect(screen.getByText("infrastructure.resource-group")).toBeInTheDocument();
    expect(screen.getByText(/read-only validation checks/i)).toBeInTheDocument();
    expect(screen.queryByText(/Latest deployment/i)).not.toBeInTheDocument();
  });

  it("renders missing and unsafe reports without exposing parser details", async () => {
    readReport.mockResolvedValue({ state: "unsafePath" });

    render(ProjectValidationReport, { project });

    await waitFor(() => expect(screen.getByText(/path is unsafe/i)).toBeInTheDocument());
    expect(screen.queryByText(/secret|exception|stack/i)).not.toBeInTheDocument();
  });
});
