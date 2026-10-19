import { act, cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/svelte";
import { afterEach, describe, expect, it, vi } from "vitest";
import ProjectComponentProvenance from "./ProjectComponentProvenance.svelte";
import type { ComponentProvenanceRead, ComponentStatus } from "../lib/component-provenance";

const readComponents = vi.hoisted(() => vi.fn());
vi.mock("../lib/bridge", () => ({ getProjectComponentProvenance: readComponents }));
afterEach(() => { cleanup(); readComponents.mockReset(); });

const project = { id: "sample", name: "Sample", path: "C:\\Projects\\sample", environment: "dev", state: "ready" } as const;
const absent: ComponentProvenanceRead = { state: "notFound", unknownMetadata: false, components: [] };
const result = (status: ComponentStatus = "verified"): ComponentProvenanceRead => ({
  state: "available", manifestVersion: "1.0", unknownMetadata: false,
  components: [{ id: "deployment-validation", version: "1.2.3-preview", sourceRepository: "https://example.com/private/source",
    sourceRevision: "0123456789abcdef0123456789abcdef01234567", status, unknownMetadata: false,
    files: [{ source: "components/engine.psm1", target: "scripts/vendor/engine.psm1", sha256: "a".repeat(64), status }] }]
});

describe("local component provenance", () => {
  it("adds no visible panel when the optional lock is absent", async () => {
    readComponents.mockResolvedValue(absent);
    const view = render(ProjectComponentProvenance, { project });
    await waitFor(() => expect(readComponents).toHaveBeenCalledWith(project.path));
    expect(view.container.childElementCount).toBe(0);
    expect(view.container.textContent).toBe("");
  });

  it("shows exact installed provenance and managed hashes without a source link or sync action", async () => {
    readComponents.mockResolvedValue(result());
    render(ProjectComponentProvenance, { project });
    const component = await screen.findByRole("article", { name: "Component deployment-validation" });
    expect(within(component).getByText("1.2.3-preview")).toBeInTheDocument();
    expect(within(component).getByText("0123456789abcdef0123456789abcdef01234567")).toBeInTheDocument();
    expect(within(component).getByText("a".repeat(64))).toBeInTheDocument();
    expect(within(component).getAllByText("Verified")).toHaveLength(2);
    expect(screen.getByText(/Solution-owned extensions are outside/)).toBeInTheDocument();
    expect(screen.getByText(/does not authenticate its source/)).toBeInTheDocument();
    expect(screen.queryByRole("link")).not.toBeInTheDocument();
    expect(screen.queryByRole("button", { name: /sync|update|deploy/i })).not.toBeInTheDocument();
  });

  it.each(["modified", "missing", "unsupported"] as const)("distinguishes %s managed files", async (status) => {
    readComponents.mockResolvedValue(result(status));
    render(ProjectComponentProvenance, { project });
    const component = await screen.findByRole("article", { name: "Component deployment-validation" });
    expect(within(component).getAllByText(status[0].toUpperCase() + status.slice(1))).toHaveLength(2);
    expect(screen.queryByText("Verified")).not.toBeInTheDocument();
  });

  it("labels future and unknown metadata without inventing update availability", async () => {
    readComponents.mockResolvedValueOnce({ ...absent, state: "unsupported", reason: "manifestVersion", manifestVersion: "2.0" });
    render(ProjectComponentProvenance, { project });
    expect(await screen.findByText("Unsupported component metadata")).toBeInTheDocument();
    expect(screen.getByText("Manifest version: 2.0")).toBeInTheDocument();
    readComponents.mockResolvedValueOnce({ ...result(), unknownMetadata: true });
    await fireEvent.click(screen.getByRole("button", { name: "Refresh components" }));
    expect(await screen.findByText(/Additional metadata is unsupported/)).toBeInTheDocument();
  });

  it("renders untrusted source metadata only as inert text", async () => {
    const data = result();
    data.components[0].sourceRepository = "javascript:alert('<img src=x onerror=alert(1)>')";
    readComponents.mockResolvedValue(data);
    render(ProjectComponentProvenance, { project });
    expect(await screen.findByText(data.components[0].sourceRepository)).toBeInTheDocument();
    expect(screen.queryByRole("link")).not.toBeInTheDocument();
    expect(document.querySelector("img")).toBeNull();
  });

  it("clears stale provenance immediately on project change and ignores old responses", async () => {
    let resolveOld!: (data: ComponentProvenanceRead) => void;
    readComponents.mockResolvedValueOnce(result());
    const view = render(ProjectComponentProvenance, { project });
    await screen.findByText("Shared components");
    readComponents.mockImplementationOnce(() => new Promise((resolve) => { resolveOld = resolve; }));
    await fireEvent.click(screen.getByRole("button", { name: "Refresh components" }));
    expect(screen.getByRole("button", { name: "Refresh components" })).toBeDisabled();
    readComponents.mockResolvedValueOnce(absent);
    await view.rerender({ project: { ...project, path: "C:\\Projects\\other" } });
    await waitFor(() => expect(readComponents).toHaveBeenLastCalledWith("C:\\Projects\\other"));
    expect(screen.queryByText("Shared components")).not.toBeInTheDocument();
    await act(() => resolveOld(result("modified")));
    expect(screen.queryByText("Shared components")).not.toBeInTheDocument();
  });

  it("ignores completion after unmount and hides raw read errors", async () => {
    let reject!: (error: Error) => void;
    readComponents.mockImplementationOnce(() => new Promise((_, rejectPromise) => { reject = rejectPromise; }));
    const view = render(ProjectComponentProvenance, { project });
    await waitFor(() => expect(readComponents).toHaveBeenCalled());
    view.unmount();
    await act(() => reject(new Error("secret-path-marker")));
    expect(screen.queryByText(/secret-path-marker/)).not.toBeInTheDocument();
    readComponents.mockRejectedValueOnce(new Error("secret-path-marker"));
    render(ProjectComponentProvenance, { project });
    expect(await screen.findByText("Unsupported component metadata")).toBeInTheDocument();
    expect(screen.queryByText(/secret-path-marker/)).not.toBeInTheDocument();
  });
});
