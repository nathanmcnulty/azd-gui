import { afterEach, describe, expect, it, vi } from "vitest";

const native = vi.hoisted(() => ({ invoke: vi.fn(), isTauri: vi.fn() }));
vi.mock("@tauri-apps/api/core", () => native);
import { getProjectComponentProvenance } from "./bridge";

afterEach(() => { native.invoke.mockReset(); native.isTauri.mockReset(); });

describe("component provenance bridge", () => {
  it("reads only the selected native project through its dedicated command", async () => {
    native.isTauri.mockReturnValue(true);
    const result = { state: "available", manifestVersion: "1.0", unknownMetadata: false, components: [] };
    native.invoke.mockResolvedValue(result);
    expect(await getProjectComponentProvenance("C:\\Projects\\sample")).toEqual(result);
    expect(native.invoke).toHaveBeenCalledExactlyOnceWith("get_project_component_provenance", { projectPath: "C:\\Projects\\sample" });
  });

  it("leaves browser-only preview unchanged without pretending to verify local files", async () => {
    native.isTauri.mockReturnValue(false);
    expect(await getProjectComponentProvenance("C:\\Projects\\sample")).toEqual({ state: "notFound", unknownMetadata: false, components: [] });
    expect(native.invoke).not.toHaveBeenCalled();
  });
});
