import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import Ajv2020 from "ajv/dist/2020.js";
import addFormats from "ajv-formats";
import { describe, expect, it } from "vitest";

function json(path: string) {
  return JSON.parse(readFileSync(resolve(process.cwd(), path), "utf8"));
}

describe("catalog contracts", () => {
  const ajv = new Ajv2020({ allErrors: true, strict: true, allowUnionTypes: true });
  addFormats(ajv);
  const validateManifest = ajv.compile(json("schemas/azd-gui.schema.json"));
  const validateCatalog = ajv.compile(json("schemas/azd-gui.catalog.schema.json"));

  it("validates the bundled catalog and immutable revisions", () => {
    const validate = validateCatalog;
    const catalog = json("catalog/azd-gui.catalog.json");

    expect(validate(catalog), JSON.stringify(validate.errors, null, 2)).toBe(true);
    expect(catalog.entries.every((entry: { commit: string }) => entry.commit.length === 40)).toBe(true);
  });

  it.each([
    "catalog/manifests/myworkid.json",
    "catalog/manifests/maester-automation.json",
    "catalog/manifests/entra-health-monitoring.json"
  ])("validates %s", (path) => {
    expect(validateManifest(json(path)), JSON.stringify(validateManifest.errors, null, 2)).toBe(true);
  });

  it("marks Maester's interactive azd hooks as terminal-required", () => {
    expect(json("catalog/manifests/maester-automation.json").requiresInteractiveTerminal).toBe(true);
  });

  it("requires paired manifest digests for external catalogs", () => {
    const validate = validateCatalog;
    const catalog = json("catalog/azd-gui.catalog.json");
    catalog.id = "external-test";
    expect(validate(catalog)).toBe(false);
    for (const entry of catalog.entries) entry.manifestSha256 = "a".repeat(64);
    expect(validate(catalog), JSON.stringify(validate.errors)).toBe(true);
    delete catalog.entries[0].manifest;
    expect(validate(catalog)).toBe(false);
  });

  it("rejects manifests that attempt to define executable commands", () => {
    const manifest = json("catalog/manifests/myworkid.json");
    manifest.preflight = { command: "arbitrary executable" };

    expect(validateManifest(manifest)).toBe(false);
  });

  it("rejects unregistered and provider-mismatched permission checks", () => {
    const manifest = json("catalog/manifests/entra-health-monitoring.json");
    manifest.permissionChecks[0].id = "run-manifest-command";
    expect(validateManifest(manifest)).toBe(false);

    manifest.permissionChecks[0].id = "azure-subscription-access";
    manifest.permissionChecks[0].provider = "graph";
    expect(validateManifest(manifest)).toBe(false);
  });
});
