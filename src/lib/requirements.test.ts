import { describe, expect, it } from "vitest";
import { bundledCatalog } from "./mock-data";
import { effectiveRequirements, requirementEnvironmentValues } from "./requirements";
import { calculateWizardPlan } from "./wizard";

describe("optional requirements", () => {
  const entry = bundledCatalog.entries.find((entry) => entry.id === "maester-automation") ?? bundledCatalog.entries[1];
  const manifest = entry.manifest!;

  it("preserves guided defaults while Minimal removes only optional feature gates", () => {
    const defaults = Object.fromEntries(manifest.configuration.groups.flatMap((group) => group.fields)
      .filter((field) => field.default !== undefined).map((field) => [field.id, field.default]));
    expect(defaults).toMatchObject({ includeWebApp: false, includeExchange: true, includeTeams: true, includeAzure: true, permissionProfile: "Extended" });
    const minimal = { ...defaults, ...manifest.recommendationProfiles!.find((profile) => profile.id === "minimalPilot")!.values };
    expect(minimal).toMatchObject({ includeWebApp: false, includeExchange: false, includeTeams: false, includeAzure: false, permissionProfile: "Minimal", mailRecipient: "" });
    const tools = (values: typeof minimal) => effectiveRequirements(manifest.prerequisites, manifest, values).map((item) => item.id);
    const checks = (values: typeof minimal) => effectiveRequirements(manifest.permissionChecks!, manifest, values).map((item) => item.id);
    expect(tools(minimal)).toEqual(["azd", "az-cli", "pwsh"]);
    expect(checks(minimal)).toEqual(["entra-directory-access", "azure-subscription-access"]);
    const expanded = { ...minimal, includeExchange: true, includeWebApp: true };
    expect(tools(expanded)).toContain("exchange-online-management");
    expect(checks(expanded)).toContain("graph-directory-read");
    expect(calculateWizardPlan(entry, expanded).pages.some((page) => page.id === "configuration:webApp")).toBe(true);
    expect(calculateWizardPlan(entry, minimal).pages.some((page) => page.id === "configuration:webApp")).toBe(false);
    expect(tools(minimal)).not.toContain("exchange-online-management");
    expect(checks(minimal)).not.toContain("graph-directory-read");
  });

  it("resolves defaults, explicit choices, and invalid input without hiding requirements", () => {
    const ids = (values: Record<string, boolean | string>) => effectiveRequirements(manifest.prerequisites, manifest, values).map((item) => item.id);
    expect(ids({})).toContain("exchange-online-management");
    expect(ids({ includeExchange: false })).not.toContain("exchange-online-management");
    expect(ids({ includeExchange: "false" })).toContain("exchange-online-management");
    expect(requirementEnvironmentValues(manifest, { includeExchange: false, secret: "hidden" })).toEqual({ INCLUDE_EXCHANGE: "false", INCLUDE_WEB_APP: "false" });
    expect(requirementEnvironmentValues(manifest, { includeExchange: "false" }).INCLUDE_EXCHANGE).toBe("invalid");
  });

  it("keeps baseline access while selecting feature-specific checks", () => {
    const checks = effectiveRequirements(manifest.permissionChecks!, manifest, { includeWebApp: false });
    expect(checks.map((item) => item.id)).toEqual(["entra-directory-access", "azure-subscription-access"]);
    expect(effectiveRequirements(manifest.permissionChecks!, manifest, { includeWebApp: true })).toHaveLength(3);
    expect(effectiveRequirements(manifest.connections, manifest, { includeExchange: false, includeAzure: false })).toEqual(manifest.connections);
  });

  it("removes inactive connection scopes and their page without removing baseline tools", () => {
    const sample = structuredClone(entry);
    sample.manifest!.connections = [{
      id: "graphPowerShell", name: "Optional Graph", required: true, reason: "Optional feature",
      scopes: ["Directory.Read.All"], condition: { field: "includeExchange", equals: true }
    }];
    expect(effectiveRequirements(sample.manifest!.connections, sample.manifest!, { includeExchange: false })).toEqual([]);
    expect(calculateWizardPlan(sample, { includeExchange: false }).pages.some((page) => page.kind === "connections")).toBe(false);
    expect(calculateWizardPlan(sample, { includeExchange: true }).pages.some((page) => page.kind === "connections")).toBe(true);
    expect(calculateWizardPlan(sample, { includeExchange: false }).pages.some((page) => page.kind === "prerequisites")).toBe(true);
  });

  it("puts controlling feature choices before the gates and keeps unrelated configuration after environment", () => {
    const pages = calculateWizardPlan(entry, {}).pages.map((page) => page.id);
    expect(pages.indexOf("configuration:features")).toBeLessThan(pages.indexOf("prerequisites"));
    expect(pages.indexOf("environment")).toBeLessThan(pages.indexOf("configuration:notifications"));
    expect(new Set(pages).size).toBe(pages.length);
  });
});
