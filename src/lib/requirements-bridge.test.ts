import { beforeEach, describe, expect, it, vi } from "vitest";
import { mockBootstrap } from "./mock-data";
import fixture from "./fixtures/conditional-requirements-v1.1.json";
import type { RequirementSnapshot, TemplateManifest } from "./types";

const native = vi.hoisted(() => ({ enabled: true, invoke: vi.fn() }));
vi.mock("@tauri-apps/api/core", () => ({ isTauri: () => native.enabled, invoke: native.invoke }));
import { checkRequirementPermissions, checkRequirementPrerequisites, reviewWizardRequirements } from "./bridge";

beforeEach(() => { native.enabled = true; native.invoke.mockReset(); });

describe("native requirement review callers", () => {
  it("uses the native snapshot for tools and permission probes without sending executable arrays", async () => {
    const entry = { ...structuredClone(mockBootstrap.catalog.entries[0]), manifest: fixture as TemplateManifest };
    const snapshot: RequirementSnapshot = { digest: "native-effective-digest", requirements: {
      schemaVersion: "1.0", contractDigest: "native-contract", environment: "dev", target: {},
      controllerValues: { MODE: "enforced", INCLUDE_AZURE: "false" }, prerequisites: [], connections: [], permissionChecks: []
    } };
    native.invoke.mockImplementation(async (command) => command === "reviewed_contract_digest" ? "native-contract"
      : command === "get_effective_requirements" ? snapshot : []);
    const review = await reviewWizardRequirements(entry, { mode: "enforced", includeAzure: false, environmentName: "dev", tenantId: "tenant", secret: "withheld" });
    expect(review.snapshot).toBe(snapshot);
    expect(review.selection.environmentValues).toEqual({ MODE: "enforced", INCLUDE_AZURE: "false" });
    await checkRequirementPrerequisites(review.selection, review.snapshot);
    await checkRequirementPermissions(review.selection, review.snapshot);
    for (const command of ["check_requirement_prerequisites", "check_requirement_permissions"]) {
      expect(native.invoke).toHaveBeenCalledWith(command, { selection: review.selection, expectedDigest: snapshot.digest });
    }
    expect(JSON.stringify(native.invoke.mock.calls.slice(1))).not.toContain("withheld");
  });

  it("browser preview changes the effective scope set in both directions without claiming native proof", async () => {
    native.enabled = false;
    const entry = { ...structuredClone(mockBootstrap.catalog.entries[0]), manifest: fixture as TemplateManifest };
    const plan = await reviewWizardRequirements(entry, { mode: "plan", includeAzure: false });
    const enforced = await reviewWizardRequirements(entry, { mode: "enforced", includeAzure: false });
    const reduced = await reviewWizardRequirements(entry, { mode: "plan", includeAzure: false });
    expect(plan.snapshot.digest).toMatch(/^browser-preview:/);
    expect(enforced.snapshot.digest).not.toBe(plan.snapshot.digest);
    expect(reduced.snapshot.digest).toBe(plan.snapshot.digest);
    expect(plan.snapshot.requirements.connections[0].scopes).toEqual(["Directory.Read.All"]);
    expect(enforced.snapshot.requirements.connections[0].scopes).toEqual(["Directory.Read.All", "Policy.ReadWrite.ConditionalAccess"]);
  });
});
