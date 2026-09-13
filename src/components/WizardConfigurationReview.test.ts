import { cleanup, fireEvent, render, screen } from "@testing-library/svelte";
import { afterEach, describe, expect, it, vi } from "vitest";
import WizardConfigurationReview from "./WizardConfigurationReview.svelte";
import { calculateWizardPlan } from "../lib/wizard";
import type { CatalogEntry, WizardValues } from "../lib/types";

afterEach(cleanup);
const entry: CatalogEntry = {
  id: "review", title: "Review", summary: "", description: "", repository: "", commit: "local", tags: [], platforms: [], trusted: false,
  manifest: { schemaVersion: "1.0", prerequisites: [], connections: [], permissionChecks: [], configuration: { groups: [
    { id: "coverage", title: "Coverage", fields: [
      { id: "enabled", label: "Include dashboard", environmentVariable: "ENABLED", type: "boolean" },
      { id: "tier", label: "Permission profile", environmentVariable: "TIER", type: "select", options: [{ value: "min", label: "Minimal access" }] },
      { id: "services", label: "Selected services", environmentVariable: "SERVICES", type: "multiselect", options: [{ value: "ex", label: "Exchange" }, { value: "az", label: "Azure" }] },
      { id: "key", label: "Access key", environmentVariable: "KEY", type: "secret" },
      { id: "privateValue", label: "Sensitive setting", environmentVariable: "PRIVATE", type: "text", sensitive: true },
      { id: "optional", label: "Optional recipient", environmentVariable: "OPTIONAL", type: "text" },
      { id: "hiddenField", label: "Hidden field", environmentVariable: "HIDDEN", type: "text", condition: { field: "enabled", equals: true } }
    ] },
    { id: "hidden", title: "Hidden group", condition: { field: "enabled", equals: true }, fields: [{ id: "hiddenGroupValue", label: "Hidden group value", environmentVariable: "HIDDEN_GROUP", type: "text" }] }
  ] } }
};
const values: WizardValues = { enabled: false, tier: "min", services: ["ex", "az"], key: "never-render-key", privateValue: "never-render-private", hiddenField: "never-render-hidden", hiddenGroupValue: "never-render-group", sourceMode: "catalog", sourcePath: "never-render-internal" };

describe("configuration review", () => {
  it("uses visible labels and readable choices without exposing secrets or internal values", () => {
    const { container } = render(WizardConfigurationReview, { pages: calculateWizardPlan(entry, values).pages, values });
    expect(screen.getByText("Include dashboard")).toBeInTheDocument();
    expect(screen.getByText("No")).toBeInTheDocument();
    expect(screen.getByText("Minimal access")).toBeInTheDocument();
    expect(screen.getByText("Exchange, Azure")).toBeInTheDocument();
    expect(screen.getAllByText("Configured (hidden)")).toHaveLength(2);
    expect(screen.getByText("Not set")).toBeInTheDocument();
    expect(container.textContent).not.toMatch(/never-render|Hidden field|Hidden group|sourceMode/);
  });

  it("requests editing the matching configuration page", async () => {
    const edit = vi.fn();
    render(WizardConfigurationReview, { props: { pages: calculateWizardPlan(entry, values).pages, values }, events: { edit } });
    await fireEvent.click(screen.getByRole("button", { name: "Edit Coverage" }));
    expect(edit.mock.calls[0][0].detail).toBe("configuration:coverage");
  });
});
