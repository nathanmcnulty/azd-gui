import { afterEach, describe, expect, it, vi } from "vitest";
import { readFileSync } from "node:fs";
import Ajv2020 from "ajv/dist/2020.js";
import addFormats from "ajv-formats";
const native = vi.hoisted(() => ({ invoke: vi.fn(), isTauri: vi.fn() }));
vi.mock("@tauri-apps/api/core", () => native);
import { getProjectReportArtifacts } from "./bridge";
afterEach(() => { native.invoke.mockReset(); native.isTauri.mockReset(); });
const ajv = new Ajv2020({ allErrors: true, strict: true, allowUnionTypes: true }); addFormats(ajv);
const validate = ajv.compile(JSON.parse(readFileSync("schemas/azd-gui.schema.json", "utf8")));
function manifest(path = "reports/plan.json") {
  return { schemaVersion: "1.0", prerequisites: [], connections: [], configuration: { groups: [] }, postDeployment: { schemaVersion: "1.0", artifacts: [{ id: "plan", label: "Plan", path, format: "json", stage: "plan", required: true, sensitive: true }] } };
}
describe("report artifact contract", () => {
  it("accepts bounded metadata", () => expect(validate(manifest()), JSON.stringify(validate.errors)).toBe(true));
  it.each(["../secret", "/reports/a", "reports/../a", "reports/./a", "reports/a\\b", "reports/a:b", "reports//a", "reports/a\n"])("rejects unsafe path %s", path => expect(validate(manifest(path))).toBe(false));
  it("rejects unknown executable fields, versions and unlimited declarations", () => {
    const m = manifest(); expect(validate({ ...m, postDeployment: { ...m.postDeployment, command: "pwsh" } })).toBe(false);
    expect(validate({ ...m, postDeployment: { ...m.postDeployment, schemaVersion: "2.0" } })).toBe(false);
    expect(validate({ ...m, postDeployment: { ...m.postDeployment, artifacts: Array(33).fill(m.postDeployment.artifacts[0]) } })).toBe(false);
  });
  it("uses an exact registered project and metadata identity, never a requested file path", async () => {
    native.isTauri.mockReturnValue(true); native.invoke.mockResolvedValue({ artifacts: [] });
    await getProjectReportArtifacts("/project", "plan", "digest");
    expect(native.invoke).toHaveBeenCalledExactlyOnceWith("get_project_report_artifacts", { projectPath: "/project", revealId: "plan", snapshotDigest: "digest" });
  });
  it("does not pretend to read files in browser preview", async () => {
    native.isTauri.mockReturnValue(false); expect((await getProjectReportArtifacts("/project")).artifacts).toEqual([]); expect(native.invoke).not.toHaveBeenCalled();
  });
});
