import { describe, expect, it } from "vitest";
import { bundledCatalog } from "./mock-data";
import { calculateWizardPlan, validateField, valuesRemovedByPlan, visibleFields } from "./wizard";

describe("wizard planning", () => {
  it("adds conditional pages when their controlling value is enabled", () => {
    const entry = bundledCatalog.entries[0];
    const first = calculateWizardPlan(entry, { enableGraphExport: false });
    const next = calculateWizardPlan(entry, { enableGraphExport: true }, first);

    expect(first.pages.some((page) => page.id === "configuration:graphExport")).toBe(false);
    expect(next.pages.some((page) => page.id === "configuration:graphExport")).toBe(true);
    expect(next.revision).toBe(2);
  });

  it("reports values from removed conditional pages", () => {
    const entry = bundledCatalog.entries[0];
    const first = calculateWizardPlan(entry, { enableGraphExport: true });
    const next = calculateWizardPlan(entry, { enableGraphExport: false }, first);

    expect(valuesRemovedByPlan(first, next)).toContain("graphTenant");
  });

  it("filters conditional fields and validates required values", () => {
    const entry = bundledCatalog.entries[2];
    const fields = entry.manifest!.configuration.groups[0].fields;

    expect(visibleFields(fields, { enableTeams: false }).map((field) => field.id)).not.toContain("teamsWebhook");
    const webhook = fields.find((field) => field.id === "teamsWebhook")!;
    expect(validateField(webhook, "")).toContain("required");
  });
});
