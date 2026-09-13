import { act, cleanup, render, screen, waitFor } from "@testing-library/svelte";
import { afterEach, describe, expect, it, vi } from "vitest";
import ProjectDeploymentReceipt from "./ProjectDeploymentReceipt.svelte";
import type { DeploymentReceiptRead } from "../lib/types";

const readReceipt = vi.hoisted(() => vi.fn());
vi.mock("../lib/bridge", () => ({ getProjectDeploymentReceipt: readReceipt }));
afterEach(() => { cleanup(); readReceipt.mockReset(); });

const project = { id: "sample", name: "Sample", path: "C:\\Projects\\sample", environment: "dev", state: "ready" } as const;
const receipt: NonNullable<DeploymentReceiptRead["receipt"]> = {
  generatedAt: "2026-09-13T12:00:00Z", mode: "enforce",
  summary: { applied: 2, unchanged: 1, skipped: 0, warnings: 1, failed: 0 },
  artifactCount: 1, outstandingActionCount: 3
};

describe("project deployment receipt", () => {
  it("shows outstanding work separately from deployment success", async () => {
    readReceipt.mockResolvedValue({ state: "available", receipt });
    render(ProjectDeploymentReceipt, { project });
    expect(await screen.findByText("Needs review")).toBeInTheDocument();
    expect(screen.getByText("Follow-up actions", { selector: "dt" }).nextElementSibling).toHaveTextContent("3");
    expect(screen.getByText(/do not establish deployment or verification success/)).toBeInTheDocument();
    expect(screen.getByText(/not bound to the selected environment/)).toBeInTheDocument();
  });

  it("labels plans without presenting them as completed deployments", async () => {
    readReceipt.mockResolvedValue({ state: "available", receipt: {
      ...receipt, mode: "plan", summary: { ...receipt.summary, applied: 0, warnings: 0 }, outstandingActionCount: 0
    } });
    render(ProjectDeploymentReceipt, { project });
    expect(await screen.findByText("Plan recorded")).toBeInTheDocument();
    expect(screen.getByText(/This is a plan. No applied actions/)).toBeInTheDocument();
    expect(screen.queryByText(/successful|verified/i)).not.toBeInTheDocument();
  });

  it("ignores a late receipt from the previous project", async () => {
    let resolveOld!: (result: DeploymentReceiptRead) => void;
    readReceipt.mockImplementationOnce(() => new Promise((resolve) => { resolveOld = resolve; }));
    readReceipt.mockResolvedValueOnce({ state: "notFound" });
    const view = render(ProjectDeploymentReceipt, { project });
    await waitFor(() => expect(readReceipt).toHaveBeenCalledWith(project.path));
    await view.rerender({ project: { ...project, path: "C:\\Projects\\other" } });
    expect(await screen.findByText(/No receipt yet/)).toBeInTheDocument();
    await act(() => { resolveOld({ state: "available", receipt }); });
    expect(screen.getByText(/No receipt yet/)).toBeInTheDocument();
    expect(screen.queryByText("Applied", { selector: "dt" })).not.toBeInTheDocument();
  });

  it("keeps raw read failures out of the workspace", async () => {
    readReceipt.mockRejectedValue(new Error("secret-diagnostic-value"));
    render(ProjectDeploymentReceipt, { project });
    expect(await screen.findByText("Receipt unavailable")).toBeInTheDocument();
    expect(screen.queryByText(/secret-diagnostic-value/)).not.toBeInTheDocument();
  });
});
