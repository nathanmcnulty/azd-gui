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

  it("validates the bundled catalog and immutable revisions", () => {
    const validate = ajv.compile(json("schemas/azd-gui.catalog.schema.json"));
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

  it("rejects manifests that attempt to define executable commands", () => {
    const manifest = json("catalog/manifests/myworkid.json");
    manifest.preflight = { command: "arbitrary executable" };

    expect(validateManifest(manifest)).toBe(false);
  });
});
