import type {
  CatalogEntry,
  ConfigurationField,
  FieldCondition,
  WizardPage,
  WizardPlan,
  WizardValues
} from "./types";

export function conditionMatches(condition: FieldCondition | undefined, values: WizardValues): boolean {
  if (!condition) return true;
  const actual = values[condition.field];
  if (condition.equals != null && actual !== condition.equals) return false;
  if (condition.notEquals != null && actual === condition.notEquals) return false;
  if (condition.includes != null) {
    if (!Array.isArray(actual) || !actual.includes(condition.includes)) return false;
  }
  return true;
}

export function visibleFields(fields: ConfigurationField[], values: WizardValues) {
  return fields.filter((field) => conditionMatches(field.condition, values));
}

export function validateField(field: ConfigurationField, value: WizardValues[string]): string | null {
  if (field.required && (value === undefined || value === "" || (Array.isArray(value) && value.length === 0))) {
    return `${field.label} is required.`;
  }
  if (value === undefined || value === "" || (Array.isArray(value) && value.length === 0)) return null;
  if (field.type === "multiselect") {
    if (!Array.isArray(value)) return `${field.label} requires one or more choices.`;
    const allowed = new Set((field.options ?? []).map((option) => option.value));
    if (value.some((item) => !allowed.has(item))) return `${field.label} contains an unsupported choice.`;
  }
  if (field.type === "select" && typeof value === "string" && field.options?.length) {
    if (!field.options.some((option) => option.value === value)) return `${field.label} contains an unsupported choice.`;
  }
  if (["tenant", "subscription", "entraObject"].includes(field.type) && typeof value === "string") {
    if (!/^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/i.test(value) || /^0{8}-0{4}-0{4}-0{4}-0{12}$/i.test(value)) {
      return `${field.label} must be a valid object ID.`;
    }
  }
  if (field.type === "location" && typeof value === "string" && !/^[A-Za-z0-9]{1,64}$/.test(value)) {
    return `${field.label} is not a valid Azure region name.`;
  }
  if (
    field.type === "resourceGroup" &&
    typeof value === "string" &&
    (!/^[A-Za-z0-9._()\-]{1,90}$/.test(value) || value.endsWith("."))
  ) {
    return `${field.label} is not a valid resource group name.`;
  }
  if (field.pattern && typeof value === "string" && value) {
    try {
      if (!new RegExp(field.pattern).test(value)) return `${field.label} is not in the expected format.`;
    } catch {
      return `${field.label} has an invalid validation rule.`;
    }
  }
  return null;
}

function standardPage(
  id: string,
  kind: WizardPage["kind"],
  title: string,
  description: string
): WizardPage {
  return { id, kind, title, description, complete: false, blockingIssues: [] };
}

export function calculateWizardPlan(
  entry: CatalogEntry,
  values: WizardValues,
  previous?: WizardPlan
): WizardPlan {
  const manifest = entry.manifest;
  const pages: WizardPage[] = [
    standardPage("source", "source", "Choose source", "Select where this deployment starts."),
    standardPage("overview", "overview", "Review template", "Understand the source, revision, and compatibility.")
  ];

  if ((manifest?.prerequisites.length ?? 0) > 0 || manifest?.minimumAzdVersion) {
    pages.push(
      standardPage(
        "prerequisites",
        "prerequisites",
        "Check prerequisites",
        "Verify the tools and modules required by this template."
      )
    );
  }

  if ((manifest?.connections.length ?? 0) > 0) {
    pages.push(
      standardPage(
        "connections",
        "connections",
        "Connect accounts",
        "Authenticate only the tools this deployment needs."
      )
    );
  }

  pages.push(
    standardPage(
      "environment",
      "environment",
      "Configure environment",
      "Choose the Azure environment, subscription, and region."
    )
  );

  for (const group of manifest?.configuration.groups ?? []) {
    if (!conditionMatches(group.condition, values)) continue;
    const fields = visibleFields(group.fields, values);
    if (fields.length === 0) continue;
    const blockingIssues = fields
      .map((field) => validateField(field, values[field.id]))
      .filter((issue): issue is string => issue !== null);
    pages.push({
      id: `configuration:${group.id}`,
      kind: "configuration",
      title: group.title,
      description: group.description ?? "Configure template options.",
      group: { ...group, fields },
      complete: blockingIssues.length === 0,
      blockingIssues
    });
  }

  pages.push(
    standardPage("review", "review", "Review deployment", "Confirm values and approve pinned template code execution."),
    standardPage("preflight", "preflight",
      manifest?.requiresInteractiveTerminal ? "Prepare for terminal" : "Run checks and preview",
      manifest?.requiresInteractiveTerminal
        ? "Check access and prepare the project for interactive deployment."
        : "Validate access and run the template's infrastructure preview."),
    standardPage("deploy", "deploy",
      manifest?.requiresInteractiveTerminal ? "Continue in terminal" : "Deploy",
      manifest?.requiresInteractiveTerminal
        ? "Complete deployment in the external terminal and refresh project state."
        : "Run azd up and follow live progress.")
  );

  return {
    templateId: entry.id,
    pages,
    revision: (previous?.revision ?? 0) + 1
  };
}

export function valuesRemovedByPlan(previous: WizardPlan, next: WizardPlan): string[] {
  const nextIds = new Set(next.pages.map((page) => page.id));
  return previous.pages
    .filter((page) => page.kind === "configuration" && !nextIds.has(page.id))
    .flatMap((page) => page.group?.fields.map((field) => field.id) ?? []);
}

export function sensitiveFieldIds(entry: CatalogEntry | null): Set<string> {
  return new Set(
    (entry?.manifest?.configuration.groups ?? [])
      .flatMap((group) => group.fields)
      .filter((field) => field.sensitive || field.type === "secret")
      .map((field) => field.id)
  );
}

export function environmentValuesForPlan(plan: WizardPlan, values: WizardValues, includeEmpty = false) {
  return Object.fromEntries(
    plan.pages
      .filter((page) => page.kind === "configuration")
      .flatMap((page) => page.group?.fields ?? [])
      .filter((field) => values[field.id] !== undefined && (includeEmpty || values[field.id] !== ""))
      .map((field) => [field.environmentVariable, values[field.id]])
  );
}
