import { readFileSync } from "node:fs";
import Ajv2020 from "ajv/dist/2020.js";
import addFormats from "ajv-formats";
import { describe, expect, it } from "vitest";
import fixture from "./fixtures/conditional-requirements-v1.1.json";
import type { TemplateManifest } from "./types";
import { effectiveConnectionRequirements, effectiveRequirements, requirementControllerIds, requirementEnvironmentValues } from "./requirements";

const schema = JSON.parse(readFileSync("schemas/azd-gui.schema.json", "utf8"));
const ajv = new Ajv2020({ allErrors: true, strict: true, allowUnionTypes: true });
addFormats(ajv);
const validate = ajv.compile(schema);
const manifest = fixture as TemplateManifest;

describe("versioned conditional requirement contract", () => {
  it("validates the same mode/scope-group fixture consumed by Rust", () => {
    expect(validate(fixture), JSON.stringify(validate.errors)).toBe(true);
  });

  it("requires the explicit new version for scope groups and select requirement conditions", () => {
    const old = structuredClone(fixture);
    old.schemaVersion = "1.0";
    expect(validate(old)).toBe(false);
    const selectOnly = { ...old, connections: [], prerequisites: [{
      id: "pwsh", name: "PowerShell", kind: "tool", required: true,
      condition: { field: "mode", equals: "enforced" }
    }] };
    expect(validate(selectOnly)).toBe(false);
    expect(validate({ ...selectOnly, schemaVersion: "1.1" })).toBe(true);
  });

  it("rejects executable scope-group data and scope groups on unrelated providers", () => {
    const executable = structuredClone(fixture) as unknown as Record<string, any>;
    executable.connections[0].scopeGroups[0].command = "pwsh";
    expect(validate(executable)).toBe(false);
    const unrelated = structuredClone(fixture);
    unrelated.connections[0].id = "azureCli";
    expect(validate(unrelated)).toBe(false);
  });

  it("projects plan/enforced scopes without leaking inactive groups", () => {
    const plan = effectiveConnectionRequirements(manifest, {});
    expect(plan).toHaveLength(1);
    expect(plan[0].scopes).toEqual(["Directory.Read.All"]);
    expect(JSON.stringify(plan)).not.toContain("Policy.ReadWrite");
    expect(JSON.stringify(plan)).not.toContain("scopeGroups");
    const enforced = effectiveConnectionRequirements(manifest, { mode: "enforced", includeAzure: true });
    expect(enforced).toHaveLength(2);
    expect(enforced.find(c => c.id === "graphPowerShell")?.scopes).toEqual(["Directory.Read.All", "Policy.ReadWrite.ConditionalAccess"]);
    expect(effectiveRequirements(manifest.permissionChecks!, manifest, { includeAzure: false })).toHaveLength(1);
  });

  it("finds scope-group-only controllers and sends only their typed declared values", () => {
    expect([...requirementControllerIds(manifest)].sort()).toEqual(["includeAzure", "mode"]);
    expect(requirementEnvironmentValues(manifest, { secret: "not-a-controller" })).toEqual({ INCLUDE_AZURE: "false", MODE: "plan" });
    expect(requirementEnvironmentValues(manifest, { mode: false }).MODE).toBe("\0");
  });

  it("keeps requirements visible for malformed values until native validation blocks them", () => {
    const invalid = effectiveConnectionRequirements(manifest, { mode: "unknown" });
    expect(invalid[0].scopes).toContain("Policy.ReadWrite.ConditionalAccess");
    expect(requirementEnvironmentValues(manifest, { mode: "unknown" }).MODE).toBe("\0");
  });

  it("rejects ambiguous provider rows and scope-group sizes beyond the native checker limit", () => {
    const duplicate = structuredClone(fixture);
    duplicate.connections.push({ ...duplicate.connections[0], reason: "Conflicting row" });
    expect(validate(duplicate)).toBe(false);
    const oversized = structuredClone(fixture);
    oversized.connections[0].scopeGroups![0].scopes = Array.from({ length: 65 }, (_, index) => `Scope${index}.Read`);
    expect(validate(oversized)).toBe(false);
  });

  it("deduplicates Graph case variants in the same order as native snapshots", () => {
    const variants = structuredClone(manifest);
    variants.connections[0].scopeGroups![0].scopes = ["directory.read.all", "Directory.Read.All"];
    expect(effectiveConnectionRequirements(variants, { mode: "enforced" })[0].scopes).toEqual(["Directory.Read.All"]);
  });
});
