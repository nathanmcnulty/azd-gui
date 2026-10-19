import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import Ajv2020 from "ajv/dist/2020.js";
import addFormats from "ajv-formats";
import { describe, expect, it } from "vitest";

function json(path: string) {
  return JSON.parse(readFileSync(resolve(process.cwd(), path), "utf8"));
}

function acknowledgementManifest() {
  return {
    schemaVersion: "1.0",
    prerequisites: [],
    connections: [],
    configuration: {
      groups: [
        {
          id: "features",
          title: "Optional features",
          fields: [
            {
              id: "includeDirectoryChanges",
              environmentVariable: "INCLUDE_DIRECTORY_CHANGES",
              label: "Include directory changes",
              type: "boolean",
              default: false
            },
            {
              id: "deploymentMode",
              environmentVariable: "DEPLOYMENT_MODE",
              label: "Deployment mode",
              type: "select",
              default: "preview",
              options: [
                { label: "Preview", value: "preview" },
                { label: "Apply", value: "apply" }
              ]
            }
          ]
        },
        {
          id: "directoryChanges",
          title: "Directory changes",
          condition: { field: "deploymentMode", equals: "apply" },
          fields: [
            {
              id: "acknowledgeDirectoryChanges",
              environmentVariable: "ACKNOWLEDGE_DIRECTORY_CHANGES",
              label: "I understand that this option changes tenant-wide directory settings.",
              help: "Review the selected tenant and the enabled option before continuing.",
              type: "acknowledgement",
              required: true,
              condition: { field: "includeDirectoryChanges", equals: true }
            }
          ]
        }
      ]
    }
  };
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
    "catalog/manifests/deployment-check.json",
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

  it("allows only registered lookups on non-sensitive Entra fields", () => {
    const manifest = json("catalog/manifests/maester-automation.json");
    const field = manifest.configuration.groups[1].fields[0];
    expect(validateManifest(manifest), JSON.stringify(validateManifest.errors)).toBe(true);
    field.lookup = "arbitrary-url";
    expect(validateManifest(manifest)).toBe(false);
    field.lookup = "securityGroup";
    field.sensitive = true;
    expect(validateManifest(manifest)).toBe(false);
    field.sensitive = false;
    field.type = "text";
    expect(validateManifest(manifest)).toBe(false);
  });

  it("restricts requirement conditions to explicit boolean equality", () => {
    const manifest = json("catalog/manifests/maester-automation.json");
    manifest.prerequisites[0].condition = { field: "includeExchange", equals: false };
    expect(validateManifest(manifest), JSON.stringify(validateManifest.errors)).toBe(true);
    manifest.prerequisites[0].condition.equals = "false";
    expect(validateManifest(manifest)).toBe(false);
    manifest.prerequisites[0].condition = { field: "includeExchange", notEquals: true };
    expect(validateManifest(manifest)).toBe(false);
  });

  describe("conditional acknowledgements", () => {
    // Use mutable JSON fixtures so presence (including null/empty properties) is tested.
    function fixture() {
      return JSON.parse(JSON.stringify(acknowledgementManifest()));
    }

    function acknowledgement(manifest: ReturnType<typeof fixture>) {
      return manifest.configuration.groups[1].fields[0];
    }

    it("validates the documented acknowledgement authoring example", () => {
      const documentation = readFileSync(resolve(process.cwd(), "docs/conditional-acknowledgements.md"), "utf8");
      const example = documentation.match(/```json\n([\s\S]*?)\n```/);
      expect(example).not.toBeNull();
      const manifest = fixture();
      manifest.configuration.groups = [JSON.parse(example![1])];
      expect(validateManifest(manifest), JSON.stringify(validateManifest.errors)).toBe(true);
    });

    it("accepts a required acknowledgement with independent group and field controllers", () => {
      const manifest = fixture();
      expect(validateManifest(manifest), JSON.stringify(validateManifest.errors)).toBe(true);
      acknowledgement(manifest).sensitive = false;
      acknowledgement(manifest).validationMessage = "Acknowledge these directory changes to continue.";
      expect(validateManifest(manifest), JSON.stringify(validateManifest.errors)).toBe(true);
    });

    it("does not require a conditional group in addition to the field condition", () => {
      const manifest = fixture();
      delete manifest.configuration.groups[1].condition;
      expect(validateManifest(manifest), JSON.stringify(validateManifest.errors)).toBe(true);
    });

    it.each([
      { field: "includeDirectoryChanges", equals: true },
      { field: "includeDirectoryChanges", notEquals: false },
      { field: "deploymentMode", equals: "apply" },
      { field: "deploymentMode", notEquals: "preview" }
    ])("retains existing condition operators: %j", (condition) => {
      const manifest = fixture();
      acknowledgement(manifest).condition = condition;
      expect(validateManifest(manifest), JSON.stringify(validateManifest.errors)).toBe(true);
    });

    it("accepts an includes condition on a declared multiselect controller", () => {
      const manifest = fixture();
      const controller = manifest.configuration.groups[0].fields[1];
      controller.type = "multiselect";
      controller.default = [];
      manifest.configuration.groups[1].condition = { field: "deploymentMode", includes: "apply" };
      acknowledgement(manifest).condition = { field: "deploymentMode", includes: "apply" };
      expect(validateManifest(manifest), JSON.stringify(validateManifest.errors)).toBe(true);
    });

    it.each(["required", "condition"])("requires explicit %s on the acknowledgement itself", (property) => {
      const manifest = fixture();
      delete acknowledgement(manifest)[property];
      expect(validateManifest(manifest)).toBe(false);
    });

    it.each([
      ["required", false],
      ["required", "true"],
      ["condition", null],
      ["condition", {}],
      ["condition", { field: "includeDirectoryChanges" }],
      ["condition", { field: "includeDirectoryChanges", equals: true, notEquals: false }],
      ["label", ""],
      ["sensitive", true],
      ["default", true],
      ["default", false],
      ["default", ""],
      ["default", []],
      ["default", null],
      ["lookup", "securityGroup"],
      ["options", []],
      ["options", [{ label: "Accepted", value: "true" }]],
      ["pattern", ""],
      ["placeholder", ""],
      ["validationMessage", ""],
      ["validationMessage", false],
      ["validationMessage", null]
    ])("rejects invalid acknowledgement %s=%j", (property, value) => {
      const manifest = fixture();
      acknowledgement(manifest)[property as string] = value;
      expect(validateManifest(manifest)).toBe(false);
    });

    it.each(["text", "secret", "boolean", "select", "multiselect", "tenant", "subscription", "location", "resourceGroup", "entraObject"])(
      "rejects acknowledgement-only validationMessage on %s fields",
      (type) => {
        const manifest = fixture();
        const field = manifest.configuration.groups[0].fields[0];
        field.type = type;
        field.validationMessage = "Please acknowledge.";
        expect(validateManifest(manifest)).toBe(false);
      }
    );

    it("keeps ordinary boolean defaults valid without acknowledgement-only metadata", () => {
      const manifest = fixture();
      const field = manifest.configuration.groups[0].fields[0];
      for (const value of [true, false]) {
        field.default = value;
        expect(validateManifest(manifest), JSON.stringify(validateManifest.errors)).toBe(true);
      }
    });
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
