import { describe, expect, it } from "vitest";
import { bundledCatalog } from "./mock-data";
import {
  calculateWizardPlan,
  conditionMatches,
  environmentValuesForPlan,
  sensitiveFieldIds,
  validateField,
  valuesRemovedByPlan,
  visibleFields
} from "./wizard";

describe("wizard planning", () => {
  it("treats native null values as missing for required fields", () => {
    const field = { id: "name", environmentVariable: "NAME", label: "Name", type: "text" as const, required: true };
    expect(validateField(field, JSON.parse("null"))).toBe("Name is required.");
    expect(validateField({ ...field, required: false }, JSON.parse("null"))).toBeNull();
  });
  it("labels interactive deployment steps as a terminal handoff", () => {
    const entry = structuredClone(bundledCatalog.entries[0]);
    entry.manifest!.requiresInteractiveTerminal = true;
    const pages = calculateWizardPlan(entry, {}).pages;
    expect(pages.find((page) => page.kind === "preflight")?.title).toBe("Prepare for terminal");
    expect(pages.find((page) => page.kind === "deploy")?.title).toBe("Continue in terminal");
    entry.manifest!.requiresInteractiveTerminal = false;
    expect(calculateWizardPlan(entry, {}).pages.find((page) => page.kind === "deploy")?.title).toBe("Deploy");
  });
  it("accepts native conditions whose unused comparisons are null", () => {
    const condition = JSON.parse('{"field":"components","equals":null,"notEquals":null,"includes":"worker"}');
    expect(conditionMatches(condition, { components: ["web", "worker"] })).toBe(true);
    expect(conditionMatches(condition, { components: ["web"] })).toBe(false);
  });
  it("shows prerequisite readiness when only minimumAzdVersion is declared", () => {
    const entry = structuredClone(bundledCatalog.entries[0]);
    entry.manifest!.prerequisites = [];
    entry.manifest!.minimumAzdVersion = "1.23.7";
    expect(calculateWizardPlan(entry, {}).pages.some((page) => page.kind === "prerequisites")).toBe(true);
  });
  it("adds conditional pages when their controlling value is enabled", () => {
    const entry = bundledCatalog.entries[0];
    const first = calculateWizardPlan(entry, { deploymentMode: "sourceBuild" });
    const next = calculateWizardPlan(entry, { deploymentMode: "releaseZip" }, first);

    expect(first.pages.some((page) => page.id === "configuration:release")).toBe(false);
    expect(next.pages.some((page) => page.id === "configuration:release")).toBe(true);
    expect(next.revision).toBe(2);
  });

  it("reports values from removed conditional pages", () => {
    const entry = bundledCatalog.entries[0];
    const first = calculateWizardPlan(entry, {
      deploymentMode: "releaseZip",
      releaseVersion: "latest"
    });
    const next = calculateWizardPlan(entry, { deploymentMode: "sourceBuild" }, first);

    expect(valuesRemovedByPlan(first, next)).toContain("releaseVersion");
    expect(
      environmentValuesForPlan(next, {
        deploymentMode: "sourceBuild",
        releaseVersion: "latest"
      })
    ).toEqual({ MYWORKID_DEPLOY_MODE: "sourceBuild" });
    const refreshed = calculateWizardPlan(entry, {
      deploymentMode: "releaseZip",
      releaseVersion: "v2.1.0"
    });
    expect(environmentValuesForPlan(refreshed, {
      deploymentMode: "releaseZip",
      releaseVersion: "v2.1.0"
    })).toMatchObject({ MYWORKID_RELEASE_VERSION: "v2.1.0" });
  });

  it("filters conditional fields and validates required values", () => {
    const entry = bundledCatalog.entries[2];
    const fields = entry.manifest!.configuration.groups[0].fields;

    expect(visibleFields(fields, {}).map((field) => field.id)).toContain("teamsChannelLink");
    const teamsChannelLink = fields.find((field) => field.id === "teamsChannelLink")!;
    expect(validateField(teamsChannelLink, "")).toContain("required");
  });

  it("treats neutral field names as sensitive when the manifest says so", () => {
    const entry = structuredClone(bundledCatalog.entries[0]);
    entry.manifest!.configuration.groups[0].fields.push({
      id: "neutralValue",
      environmentVariable: "NEUTRAL_VALUE",
      label: "Neutral value",
      type: "text",
      sensitive: true
    });

    expect(sensitiveFieldIds(entry)).toContain("neutralValue");
  });

  it("preserves multiselect values as arrays and validates specialized IDs", () => {
    const entry = structuredClone(bundledCatalog.entries[0]);
    entry.manifest!.configuration.groups = [{
      id: "choices",
      title: "Choices",
      fields: [{
        id: "services",
        environmentVariable: "SERVICES",
        label: "Services",
        type: "multiselect",
        required: true,
        options: [
          { label: "Web", value: "web" },
          { label: "Worker", value: "worker" }
        ]
      }, {
        id: "groupId",
        environmentVariable: "GROUP_ID",
        label: "Security group",
        type: "entraObject",
        required: true
      }]
    }];
    const plan = calculateWizardPlan(entry, {
      services: ["web", "worker"],
      groupId: "11111111-1111-1111-1111-111111111111"
    });
    expect(environmentValuesForPlan(plan, {
      services: ["web", "worker"],
      groupId: "11111111-1111-1111-1111-111111111111"
    })).toEqual({
      SERVICES: ["web", "worker"],
      GROUP_ID: "11111111-1111-1111-1111-111111111111"
    });
    expect(validateField(entry.manifest!.configuration.groups[0].fields[0], "web")).toContain("one or more");
    expect(validateField(entry.manifest!.configuration.groups[0].fields[1], "invalid")).toContain("object ID");
  });

  it("rejects resource group names ending in a period", () => {
    expect(validateField({
      id: "resourceGroup",
      environmentVariable: "AZURE_RESOURCE_GROUP",
      label: "Resource group",
      type: "resourceGroup"
    }, "rg-preview.")).toContain("resource group name");
  });
});
