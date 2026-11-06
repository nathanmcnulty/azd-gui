import type { AcknowledgementProof, ConfigurationField, ConfigurationGroup, TemplateManifest, WizardValues } from "./types";
import { conditionMatches } from "./wizard";

export function acknowledgementFields(manifest: TemplateManifest | undefined) {
  return (manifest?.configuration.groups ?? []).flatMap((group) =>
    group.fields.filter((field) => field.type === "acknowledgement").map((field) => ({ field, group })));
}

export function acknowledgementControllerFields(manifest: TemplateManifest | undefined): ConfigurationField[] {
  const ids = new Set(acknowledgementFields(manifest).flatMap(({ field, group }) => [field.condition?.field, group.condition?.field]));
  return (manifest?.configuration.groups ?? []).flatMap((group) => group.fields).filter((field) => ids.has(field.id));
}

export function withAcknowledgementControllerValues(manifest: TemplateManifest | undefined, values: WizardValues): WizardValues {
  const explicit = { ...values };
  for (const field of acknowledgementControllerFields(manifest)) {
    if (explicit[field.id] === undefined) explicit[field.id] = field.default ??
      (field.type === "boolean" ? false : field.type === "multiselect" ? [] : "");
  }
  return explicit;
}

export function acknowledgementMessage(field: ConfigurationField): string {
  return field.validationMessage ?? `Accept ${field.label} for the current scope and tenant before continuing.`;
}

export function acknowledgementSnapshot(field: ConfigurationField, group: ConfigurationGroup, values: WizardValues): AcknowledgementProof | null {
  if (!field.condition || !conditionMatches(group.condition, values) || !conditionMatches(field.condition, values)) return null;
  const tenantId = String(values.tenantId ?? "").trim().toLowerCase();
  if (!tenantId) return null;
  const controllingValues: AcknowledgementProof["controllingValues"] = {};
  for (const id of [field.condition.field, ...(group.condition ? [group.condition.field] : [])]) {
    const value = values[id];
    if (value === undefined || value === null) return null;
    controllingValues[id] = Array.isArray(value) ? [...value] : value;
  }
  return { fieldId: field.id, tenantId, controllingValues };
}

export function canAcknowledge(manifest: TemplateManifest | undefined, fieldId: string, values: WizardValues): boolean {
  const declaration = acknowledgementFields(manifest).find(({ field }) => field.id === fieldId);
  return declaration !== undefined && acknowledgementSnapshot(declaration.field, declaration.group, values) !== null;
}

export function acknowledgementMatches(field: ConfigurationField, group: ConfigurationGroup, values: WizardValues, proofs: AcknowledgementProof[]): boolean {
  const current = acknowledgementSnapshot(field, group, values);
  const proof = proofs.find((proof) => proof.fieldId === field.id);
  return values[field.id] === true && current !== null && proof !== undefined &&
    current.tenantId === proof.tenantId &&
    Object.keys(current.controllingValues).length === Object.keys(proof.controllingValues).length &&
    Object.entries(current.controllingValues).every(([id, value]) => JSON.stringify(value) === JSON.stringify(proof.controllingValues[id]));
}

export function acknowledgementIssues(manifest: TemplateManifest | undefined, values: WizardValues, proofs: AcknowledgementProof[] = []): string[] {
  return acknowledgementFields(manifest)
    .filter(({ field, group }) => conditionMatches(group.condition, values) && conditionMatches(field.condition, values))
    .filter(({ field, group }) => !acknowledgementMatches(field, group, values, proofs))
    .map(({ field }) => acknowledgementMessage(field));
}

export function assertAcknowledgements(manifest: TemplateManifest | undefined, values: WizardValues, proofs: AcknowledgementProof[] = []) {
  const issues = acknowledgementIssues(manifest, values, proofs);
  if (issues.length) throw new Error(issues.join(" "));
}

/** Called only from an explicit field interaction. Removing stale proofs prevents A→B→A resurrection. */
export function changeAcknowledgementValues(manifest: TemplateManifest | undefined, previous: WizardValues, proofs: AcknowledgementProof[], id: string, value: WizardValues[string]) {
  const values = { ...previous, [id]: value };
  const retained: AcknowledgementProof[] = [];
  for (const { field, group } of acknowledgementFields(manifest)) {
    if (id === field.id) {
      const snapshot = value === true ? acknowledgementSnapshot(field, group, values) : null;
      values[field.id] = snapshot !== null;
      if (snapshot) retained.push(snapshot);
    } else if (acknowledgementMatches(field, group, values, proofs)) {
      retained.push(proofs.find((proof) => proof.fieldId === field.id)!);
    } else {
      values[field.id] = false;
    }
  }
  return { values, proofs: retained };
}

export function withoutAcknowledgements(manifest: TemplateManifest | undefined, values: WizardValues): WizardValues {
  const ids = new Set(acknowledgementFields(manifest).map(({ field }) => field.id));
  return Object.fromEntries(Object.entries(values).filter(([id]) => !ids.has(id)));
}

export function acknowledgementEnvironmentValues(manifest: TemplateManifest | undefined, values: WizardValues) {
  values = withAcknowledgementControllerValues(manifest, values);
  const ids = new Set(acknowledgementFields(manifest).flatMap(({ field, group }) => [field.id, field.condition?.field, group.condition?.field]));
  return Object.fromEntries((manifest?.configuration.groups ?? []).flatMap((group) => group.fields)
    .filter((field) => ids.has(field.id) && values[field.id] !== undefined)
    .map((field) => [field.environmentVariable, values[field.id]]));
}
