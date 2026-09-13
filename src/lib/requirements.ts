import type { RequirementCondition, TemplateManifest, WizardValues } from "./types";

export function requirementControllerIds(manifest: TemplateManifest): Set<string> {
  return new Set([
    ...manifest.prerequisites, ...manifest.connections, ...(manifest.permissionChecks ?? [])
  ].flatMap((requirement) => requirement.condition ? [requirement.condition.field] : []));
}

function controllerValue(manifest: TemplateManifest, id: string, values: WizardValues) {
  const field = manifest.configuration.groups.flatMap((group) => group.fields).find((field) => field.id === id);
  return values[id] === undefined ? field?.default : values[id];
}

export function effectiveRequirements<T extends { condition?: RequirementCondition }>(
  requirements: T[], manifest: TemplateManifest, values: WizardValues
): T[] {
  return requirements.filter(({ condition }) => {
    if (!condition) return true;
    const value = controllerValue(manifest, condition.field, values);
    // Invalid input must never hide a requirement. Native validation will block execution.
    return typeof value !== "boolean" || value === condition.equals;
  });
}

export function requirementEnvironmentValues(manifest: TemplateManifest, values: WizardValues): Record<string, string> {
  const controllers = requirementControllerIds(manifest);
  return Object.fromEntries(manifest.configuration.groups.flatMap((group) => group.fields)
    .filter((field) => controllers.has(field.id))
    .map((field) => {
      const value = controllerValue(manifest, field.id, values);
      return [field.environmentVariable, typeof value === "boolean" ? String(value) : "invalid"];
    }));
}
