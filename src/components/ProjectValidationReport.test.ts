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
        bindingState: "legacyUnbound",
        nextActions: [],
        summary: { pass: 1, fail: 0, warning: 1, info: 0, skipped: 0, planned: 0 },
        checks: [{ phase: "infrastructure", status: "pass", sideEffect: "readOnly", durationMs: 5 }, { phase: "identity", status: "warning", sideEffect: "readOnly", durationMs: 3 }]
      }
    });

    render(ProjectValidationReport, { project });

    expect(await screen.findByText("Legacy record")).toBeInTheDocument();
    expect(screen.getByText("Check 1")).toBeInTheDocument();
    expect(screen.getByText(/verification checks recorded/i)).toBeInTheDocument();
    expect(screen.queryByText(/Latest deployment/i)).not.toBeInTheDocument();
  });

  it("renders missing and unsafe reports without exposing parser details", async () => {
    readReport.mockResolvedValue({ state: "unsafePath" });

    render(ProjectValidationReport, { project });

    await waitFor(() => expect(screen.getByText(/path is unsafe/i)).toBeInTheDocument());
    expect(screen.queryByText(/secret|exception|stack/i)).not.toBeInTheDocument();
  });

  it("does not present a mismatched producer pass as healthy", async () => {
    readReport.mockResolvedValue({
      state: "available",
      report: {
        startedAt: "2026-09-12T12:00:00Z", completedAt: "2026-09-12T12:00:01Z",
        durationMs: 1000, mode: "verify", outcome: "passed", environmentBinding: "environmentMatch",
        bindingState: "mismatch",
        nextActions: [{ code: "verifyPermissions", owner: "identityAdmin", priority: "required" }],
        summary: { pass: 1, fail: 0, warning: 0, info: 0, skipped: 0, planned: 0 },
        checks: []
      }
    });
    render(ProjectValidationReport, { project });
    expect(await screen.findByText(/belongs to different project, target, source, or operation/)).toBeInTheDocument();
    expect(screen.getByText(/Verify the required permissions/)).toBeInTheDocument();
    expect(screen.getByText("Context mismatch").closest(".status-pill")).toHaveClass("danger");
    expect(screen.queryByText("passed", { selector: ".status-pill" })).not.toBeInTheDocument();
  });

  it.each([
    ["stale", "Stale record"],
    ["legacyUnbound", "Legacy record"],
    ["insufficientContext", "Context incomplete"]
  ] as const)("does not present a %s producer pass as current", async (bindingState, label) => {
    readReport.mockResolvedValue({
      state: "available",
      report: {
        startedAt: "2026-09-12T12:00:00Z", completedAt: "2026-09-12T12:00:01Z",
        durationMs: 1000, mode: "verify", outcome: "passed", environmentBinding: "environmentMatch",
        bindingState, nextActions: [],
        summary: { pass: 1, fail: 0, warning: 0, info: 0, skipped: 0, planned: 0 }, checks: []
      }
    });
    render(ProjectValidationReport, { project });
    expect(await screen.findByText(label)).toBeInTheDocument();
    expect(screen.queryByText("passed", { selector: ".status-pill" })).not.toBeInTheDocument();
  });

  it("shows plan totals and removes old results while switching projects", async () => {
    readReport.mockResolvedValueOnce({ state: "available", report: {
      startedAt: "2026-09-12T12:00:00Z", completedAt: "2026-09-12T12:00:01Z",
      durationMs: 1000, mode: "plan", outcome: "planned", environmentBinding: "environmentUnbound",
      bindingState: "legacyUnbound", nextActions: [],
      summary: { pass: 0, fail: 0, warning: 0, info: 0, skipped: 1, planned: 3 },
      checks: [{ phase: "context", status: "planned", sideEffect: "none", durationMs: 0 }]
    } });
    const view = render(ProjectValidationReport, { project });
    expect(await screen.findByText("Check 1")).toBeInTheDocument();
    expect(screen.getByText("Planned", { selector: "dt" }).nextElementSibling).toHaveTextContent("3");
    readReport.mockImplementationOnce(() => new Promise(() => {}));
    await view.rerender({ project: { ...project, path: "C:\\Projects\\other" } });
    await waitFor(() => expect(screen.queryByText("Check 1")).not.toBeInTheDocument());
    expect(screen.getByText(/Reading the selected project's report/)).toBeInTheDocument();
  });
});
