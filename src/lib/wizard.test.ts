import { describe, expect, it } from "vitest";
import { bundledCatalog } from "./mock-data";
import {
  calculateWizardPlan,
  environmentValuesForPlan,
  sensitiveFieldIds,
  validateField,
  valuesRemovedByPlan,
  visibleFields
} from "./wizard";

describe("wizard planning", () => {
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
  });

  it("filters conditional fields and validates required values", () => {
    const entry = bundledCatalog.entries[2];
    const fields = entry.manifest!.configuration.groups[0].fields;

    expect(visibleFields(fields, { enableTeams: false }).map((field) => field.id)).not.toContain("teamsWebhook");
    const webhook = fields.find((field) => field.id === "teamsWebhook")!;
    expect(validateField(webhook, "")).toContain("required");
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
});
