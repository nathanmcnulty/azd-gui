import type { RequirementCondition, TemplateManifest, WizardValues } from "./types";

export function requirementControllerIds(manifest: TemplateManifest): Set<string> {
  return new Set([
    ...manifest.prerequisites, ...manifest.connections, ...(manifest.permissionChecks ?? [])
  ].flatMap((requirement) => requirement.condition ? [requirement.condition.field] : [])
    .concat(manifest.connections.flatMap((connection) => connection.scopeGroups ?? [])
      .flatMap((group) => group.condition ? [group.condition.field] : [])));
}

function controllerValue(manifest: TemplateManifest, id: string, values: WizardValues) {
  const field = manifest.configuration.groups.flatMap((group) => group.fields).find((field) => field.id === id);
  if (field?.sensitive || field?.type === "secret") return undefined;
  return values[id] === undefined ? field?.default : values[id];
}

export function effectiveRequirements<T extends { condition?: RequirementCondition }>(
  requirements: T[], manifest: TemplateManifest, values: WizardValues
): T[] {
  return requirements.filter(({ condition }) => {
    if (!condition) return true;
    const value = controllerValue(manifest, condition.field, values);
    // Invalid input must never hide a requirement. Native validation will block execution.
    const field = manifest.configuration.groups.flatMap((group) => group.fields).find((field) => field.id === condition.field);
    const valid = field?.type === "boolean" ? typeof value === "boolean"
      : field?.type === "select" && typeof value === "string"
        && field.options?.some((option) => option.value === value);
    return !valid || value === condition.equals;
  });
}

export function requirementEnvironmentValues(manifest: TemplateManifest, values: WizardValues): Record<string, string> {
  const controllers = requirementControllerIds(manifest);
  return Object.fromEntries(manifest.configuration.groups.flatMap((group) => group.fields)
    .filter((field) => controllers.has(field.id))
    .map((field) => {
      const value = controllerValue(manifest, field.id, values);
      const validSelect = field.type === "select" && typeof value === "string"
        && field.options?.some((option) => option.value === value);
      return [field.environmentVariable, field.type === "boolean"
        ? typeof value === "boolean" ? String(value) : "invalid"
        // Native select-controller validation forbids controls in declared options.
        : validSelect ? value : "\0"];
    }));
}

/** Conservative browser/early-planning projection. Native snapshots remain authoritative. */
export function effectiveConnectionRequirements(manifest: TemplateManifest, values: WizardValues) {
  const compare = (a: string, b: string) => a < b ? -1 : a > b ? 1 : 0;
  return effectiveRequirements(manifest.connections, manifest, values).map(({ scopeGroups, ...connection }) => {
    const scopes = [
      ...(connection.scopes ?? []),
      ...effectiveRequirements(scopeGroups ?? [], manifest, values).flatMap((group) => group.scopes)
    ].sort((a, b) => compare(a.toLowerCase(), b.toLowerCase()) || compare(a, b));
    return { ...connection, scopes: scopes.filter((scope, index) => index === 0 || scope.toLowerCase() !== scopes[index - 1].toLowerCase()) };
  });
}
