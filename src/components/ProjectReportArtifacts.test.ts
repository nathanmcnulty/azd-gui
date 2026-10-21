import { act, cleanup, fireEvent, render, screen, waitFor } from "@testing-library/svelte";
import { afterEach, describe, expect, it, vi } from "vitest";
import ProjectReportArtifacts from "./ProjectReportArtifacts.svelte";
import type { ArtifactRead } from "../lib/report-artifacts";
const readReports = vi.hoisted(() => vi.fn());
vi.mock("../lib/bridge", () => ({ getProjectReportArtifacts: readReports }));
afterEach(() => { cleanup(); readReports.mockReset(); });
const project = { id: "one", name: "One", path: "/workspace/one", environment: "dev", revision: "a".repeat(40), state: "ready" } as const;
function data(state: ArtifactRead["artifacts"][number]["state"] = "available", sensitive = false): ArtifactRead {
  return { context: { snapshotDigest: "b".repeat(64), environment: "dev", revision: project.revision }, observedAt: "2026-10-21T00:00:00Z", artifacts: [{ declaration: { id: "plan", label: "Change plan", path: "reports/plan.json", format: "json", stage: "plan", required: true, sensitive }, state, ...(state === "available" ? { content: "synthetic report content", modifiedAt: "2026-10-20T00:00:00Z", sha256: "c".repeat(64) } : {}) }] };
}
describe("read-only report artifacts", () => {
  it("keeps the optional panel absent without declarations", async () => {
    readReports.mockResolvedValue({ context: null, observedAt: "", artifacts: [] });
    render(ProjectReportArtifacts, { project });
    await waitFor(() => expect(screen.queryByText("Deployment reports")).not.toBeInTheDocument());
    expect(readReports).toHaveBeenCalledWith(project.path, null, null);
  });
  it("shows observed context and keeps operational verification explicitly unproven", async () => {
    readReports.mockResolvedValue(data()); render(ProjectReportArtifacts, { project });
    expect(await screen.findByText("synthetic report content")).toBeInTheDocument();
    expect(screen.getByText(/Deployment success is not operational verification/)).toBeInTheDocument();
    expect(screen.getByText(/freshness and verification remain unproven/)).toBeInTheDocument();
    expect(screen.getByText("dev")).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: /run|deploy|verify|approve/i })).not.toBeInTheDocument();
  });
  it("requires explicit sensitive reveal and hides content on refresh", async () => {
    readReports.mockResolvedValueOnce(data("hidden", true)).mockResolvedValueOnce(data("available", true)).mockResolvedValueOnce(data("hidden", true));
    render(ProjectReportArtifacts, { project });
    await fireEvent.click(await screen.findByRole("button", { name: "Reveal sensitive report: Change plan" }));
    expect(readReports).toHaveBeenLastCalledWith(project.path, "plan", "b".repeat(64));
    expect(await screen.findByText("synthetic report content")).toBeInTheDocument();
    await fireEvent.click(screen.getByRole("button", { name: "Hide sensitive content" }));
    await screen.findByRole("button", { name: "Reveal sensitive report: Change plan" });
    expect(screen.queryByText("synthetic report content")).not.toBeInTheDocument();
  });
  it("renders HTML and Markdown as inert text without loading links or images", async () => {
    const result = data(); result.artifacts[0].declaration.format = "markdown";
    result.artifacts[0].content = '<img src="https://example.test/leak" onerror="alert(1)">[run](javascript:alert(1))';
    readReports.mockResolvedValue(result); const view = render(ProjectReportArtifacts, { project });
    await screen.findByText(result.artifacts[0].content);
    expect(view.container.querySelector("img,script,a,iframe")).toBeNull();
  });
  it.each(["missing", "unsafePath", "tooLarge", "invalid", "unreadable"] as const)("shows %s without payload or a passed result", async state => {
    readReports.mockResolvedValue(data(state)); render(ProjectReportArtifacts, { project });
    await screen.findByRole("article", { name: "Change plan" });
    expect(document.querySelector("pre")).toBeNull();
    expect(screen.queryByText("Passed")).not.toBeInTheDocument();
  });
  it("discards old context and ignores pending replies after an environment switch", async () => {
    let resolveOld!: (v: ArtifactRead) => void;
    readReports.mockResolvedValueOnce(data()).mockImplementationOnce(() => new Promise(resolve => { resolveOld = resolve; })).mockResolvedValueOnce({ context: null, observedAt: "", artifacts: [] });
    const view = render(ProjectReportArtifacts, { project });
    await screen.findByText("synthetic report content");
    await fireEvent.click(screen.getByRole("button", { name: "Refresh reports" }));
    expect(screen.queryByText("synthetic report content")).not.toBeInTheDocument();
    await view.rerender({ project: { ...project, environment: "prod" } });
    await act(() => resolveOld(data()));
    await waitFor(() => expect(screen.queryByText("synthetic report content")).not.toBeInTheDocument());
  });
  it("clears revealed content when reading fails and shows only a generic error", async () => {
    readReports.mockResolvedValueOnce(data("available", true)).mockRejectedValueOnce(new Error("private file data"));
    render(ProjectReportArtifacts, { project }); await screen.findByText("synthetic report content");
    await fireEvent.click(screen.getByRole("button", { name: "Refresh reports" }));
    await screen.findByRole("alert"); expect(screen.queryByText("private file data")).not.toBeInTheDocument();
    expect(screen.queryByText("synthetic report content")).not.toBeInTheDocument();
  });
  it.each(["snapshot", "tenant", "configuration", "outputs"])("clears sensitive content when %s context changes at the same path", async kind => {
    let resolveOld!: (v: ArtifactRead) => void;
    readReports.mockResolvedValueOnce(data("available", true)).mockImplementationOnce(() => new Promise(resolve => { resolveOld = resolve; })).mockResolvedValueOnce(data("hidden", true));
    const view = render(ProjectReportArtifacts, { project, reportContext: { [kind]: "before" } });
    await screen.findByText("synthetic report content");
    await fireEvent.click(screen.getByRole("button", { name: "Refresh reports" }));
    await view.rerender({ project, reportContext: { [kind]: "after" } });
    await screen.findByRole("button", { name: "Reveal sensitive report: Change plan" });
    await act(() => resolveOld(data("available", true)));
    expect(screen.queryByText("synthetic report content")).not.toBeInTheDocument();
  });

});
