import pimFixture from "./fixtures/pim-provider-acknowledgement.json";
import { describe, expect, it } from "vitest";
import type { AcknowledgementProviderViews, CatalogEntry, TemplateManifest, WizardValues } from "./types";
import { acknowledgementEnvironmentValues, acknowledgementFields, acknowledgementIssues, changeAcknowledgementValues, withoutAcknowledgements } from "./acknowledgements";
import { calculateWizardPlan, environmentValuesForPlan, validateField } from "./wizard";

const tenantId = "11111111-1111-1111-1111-111111111111";
const entry: CatalogEntry = {
  id: "ack", title: "Ack", summary: "", description: "", repository: "", commit: "local", tags: [], platforms: [],
  manifest: { schemaVersion: "1.0", prerequisites: [], connections: [], configuration: { groups: [
    { id: "choices", title: "Choices", fields: [
      { id: "scope", environmentVariable: "SCOPE", type: "select", label: "Scope", options: [{ label: "All", value: "all" }, { label: "Selected", value: "selected" }] },
      { id: "enabled", environmentVariable: "ENABLED", type: "boolean", label: "Enabled", default: true }
    ] },
    { id: "confirm", title: "Confirm", condition: { field: "enabled", equals: true }, fields: [
      { id: "ack", environmentVariable: "ACK", label: "All roles", type: "acknowledgement", required: true,
        condition: { field: "scope", equals: "all" }, validationMessage: "Acknowledge every role." }
    ] }
  ] } }
};
const initial: WizardValues = { tenantId, scope: "all", enabled: true };
const field = acknowledgementFields(entry.manifest)[0].field;

function accept(values = initial) { return changeAcknowledgementValues(entry.manifest, values, [], "ack", true); }

describe("fresh acknowledgement proofs", () => {
  it.each([undefined, null, false, "true", "false", [], ["true"]])("requires an actual true value, rejecting %j", (value) => {
    expect(validateField(field, value as WizardValues[string])).toBe("Acknowledge every role.");
    expect(acknowledgementIssues(entry.manifest, { ...initial, ack: value as WizardValues[string] })).toEqual(["Acknowledge every role."]);
  });
  it("blocks a saved true without a fresh proof and accepts only the matching typed snapshot", () => {
    expect(acknowledgementIssues(entry.manifest, { ...initial, ack: true })).toHaveLength(1);
    const accepted = accept();
    expect(accepted.proofs).toEqual([{ fieldId: "ack", tenantId, controllingValues: { scope: "all", enabled: true } }]);
    expect(acknowledgementIssues(entry.manifest, accepted.values, accepted.proofs)).toEqual([]);
    expect(calculateWizardPlan(entry, accepted.values).pages.find((page) => page.id === "configuration:confirm")?.complete).toBe(false);
    expect(calculateWizardPlan(entry, accepted.values, undefined, accepted.proofs).pages.find((page) => page.id === "configuration:confirm")?.complete).toBe(true);
  });
  it.each(["scope", "enabled", "tenantId"])("permanently invalidates the prior proof after %s changes and changes back", (id) => {
    const accepted = accept();
    const value = id === "scope" ? "selected" : id === "enabled" ? false : "22222222-2222-2222-2222-222222222222";
    const changed = changeAcknowledgementValues(entry.manifest, accepted.values, accepted.proofs, id, value);
    expect(changed.proofs).toEqual([]);
    expect(changed.values.ack).toBe(false);
    const restored = changeAcknowledgementValues(entry.manifest, changed.values, changed.proofs, id, initial[id]);
    expect(acknowledgementIssues(entry.manifest, restored.values, restored.proofs)).toHaveLength(1);
    const reaccepted = changeAcknowledgementValues(entry.manifest, restored.values, restored.proofs, "ack", true);
    expect(acknowledgementIssues(entry.manifest, reaccepted.values, reaccepted.proofs)).toEqual([]);
  });
  it("keeps unrelated edits and repeated identical selection stable, but never substitutes a missing tenant", () => {
    const accepted = accept();
    expect(changeAcknowledgementValues(entry.manifest, accepted.values, accepted.proofs, "other", "new").proofs).toEqual(accepted.proofs);
    expect(changeAcknowledgementValues(entry.manifest, accepted.values, accepted.proofs, "scope", "all").proofs).toEqual(accepted.proofs);
    expect(accept({ ...initial, tenantId: "" }).proofs).toEqual([]);
    expect(accept({ ...initial, tenantId: "" }).values.ack).toBe(false);
  });
  it("does not block or transmit inactive acknowledgements", () => {
    for (const values of [{ ...initial, scope: "selected", ack: true }, { ...initial, enabled: false, ack: true }]) {
      expect(acknowledgementIssues(entry.manifest, values)).toEqual([]);
      expect(environmentValuesForPlan(calculateWizardPlan(entry, values), values)).not.toHaveProperty("ACK");
    }
  });
  it("strips only acknowledgements from saved choices and projects only bound native values", () => {
    expect(withoutAcknowledgements(entry.manifest, { ...initial, ack: true, other: "keep" })).toEqual({ ...initial, other: "keep" });
    expect(acknowledgementEnvironmentValues(entry.manifest, { ...initial, ack: true, other: "private" })).toEqual({ SCOPE: "all", ENABLED: true, ACK: true });
  });
  it("binds the complete multiselect, not merely the option that made a condition active", () => {
    const manifest = structuredClone(entry.manifest!);
    manifest.configuration.groups[0].fields[0].type = "multiselect";
    manifest.configuration.groups[1].fields[0].condition = { field: "scope", includes: "all" };
    const accepted = changeAcknowledgementValues(manifest, { ...initial, scope: ["all"] }, [], "ack", true);
    const changed = changeAcknowledgementValues(manifest, accepted.values, accepted.proofs, "scope", ["all", "another"]);
    expect(changed.proofs).toEqual([]);
    expect(acknowledgementIssues(manifest, changed.values, changed.proofs)).toHaveLength(1);
  });
  it("places mixed feature acknowledgements after target selection without blocking the earlier feature page", () => {
    const mixed = structuredClone(entry);
    mixed.manifest!.configuration.groups[0].fields.push(field);
    mixed.manifest!.configuration.groups.pop();
    mixed.manifest!.prerequisites = [{ id: "tool", name: "Tool", kind: "tool", required: true, condition: { field: "enabled", equals: true } }];
    const plan = calculateWizardPlan(mixed, { scope: "all", enabled: true });
    expect(plan.pages.find((page) => page.id === "configuration:choices")?.complete).toBe(true);
    const index = plan.pages.findIndex((page) => page.id === "configuration:choices:acknowledgements");
    expect(index).toBeGreaterThan(plan.pages.findIndex((page) => page.kind === "environment"));
    expect(plan.pages[index].complete).toBe(false);
  });
});

describe("provider-backed PIM acknowledgements", () => {
  const manifest = pimFixture as TemplateManifest;
  const key = "entra-pim-role-definitions:privileged";
  const fieldId = "confirmAllPrivilegedRoles";
  const lessFieldId = "confirmAllLessPrivilegedRoles";
  const first = "aaaaaaaa-aaaa-4aaa-aaaa-aaaaaaaaaaaa";
  const second = "bbbbbbbb-bbbb-4bbb-bbbb-bbbbbbbbbbbb";
  const replacement = "cccccccc-cccc-4ccc-cccc-cccccccccccc";
  const all = { tenantId, privilegedScope: "all", lessPrivilegedScope: "all" };
  const makeView = (ids: string[], revision: string, tenant = tenantId): AcknowledgementProviderViews => ({
    [key]: { tenantId: tenant, ids, digest: revision.repeat(64), revision }
  });

  it("requires a new acknowledgement for same-count A/B to A/C refresh and preserves the independent tier", () => {
    const before = makeView([first, second], "a");
    const after = makeView([first, replacement], "b");
    const accepted = changeAcknowledgementValues(manifest, all, [], fieldId, true, before);
    const both = changeAcknowledgementValues(manifest, accepted.values, accepted.proofs, lessFieldId, true, before);
    expect(acknowledgementIssues(manifest, both.values, both.proofs, before)).toEqual([]);
    expect(acknowledgementIssues(manifest, both.values, both.proofs, after)).toEqual(["Accept I acknowledge this targets every privileged role for the current scope and tenant before continuing."]);
    const fresh = changeAcknowledgementValues(manifest, both.values, both.proofs, fieldId, true, after);
    expect(acknowledgementIssues(manifest, fresh.values, fresh.proofs, after)).toEqual([]);
    expect(fresh.proofs.find(proof => proof.fieldId === lessFieldId)?.provider).toBeUndefined();
  });

  it("rejects unknown, empty or foreign evidence and saved true without an explicit proof", () => {
    for (const views of [{}, makeView([], "a"), makeView([first], "a", "22222222-2222-4222-8222-222222222222")]) {
      expect(changeAcknowledgementValues(manifest, all, [], fieldId, true, views).proofs).toEqual([]);
    }
    const view = makeView([first], "a");
    expect(acknowledgementIssues(manifest, { ...all, [fieldId]: true }, [], view)).toHaveLength(2);
    const accepted = changeAcknowledgementValues(manifest, all, [], fieldId, true, view);
    const changed = changeAcknowledgementValues(manifest, accepted.values, accepted.proofs, "tenantId", "22222222-2222-4222-8222-222222222222", view);
    expect(changed.proofs).toEqual([]);
  });

  it("checks optional selected IDs against the complete result and binds canonical selections", () => {
    const configured = structuredClone(manifest);
    configured.configuration.groups[0].fields[2].acknowledgementProvider!.selectedIdsField = "roleIds";
    const view = makeView([first, second], "a");
    const accepted = changeAcknowledgementValues(configured, { ...all, roleIds: [second, first] }, [], fieldId, true, view);
    expect(accepted.proofs[0].provider?.selectedIds).toEqual([first, second]);
    expect(changeAcknowledgementValues(configured, accepted.values, accepted.proofs, "roleIds", [replacement], view).proofs).toEqual([]);
  });
});