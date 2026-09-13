import {
  prepareDeployment,
  requestExecutionApproval,
  startAzdOperation
} from "../bridge";
import type {
  CatalogEntry,
  LocalSourceBinding,
  ProjectSummary,
  WizardPlan,
  WizardValues
} from "../types";
import { environmentValuesForPlan } from "../wizard";

export async function materializeDeployment(
  entry: CatalogEntry,
  plan: WizardPlan,
  values: WizardValues,
  sourceBinding?: LocalSourceBinding
): Promise<{ project: ProjectSummary; values: WizardValues }> {
  const projectPath = String(values.projectPath || "");
  const environment = String(values.environmentName || "dev");
  const revision = entry.commit === "local" ? undefined : entry.commit;
  const sourceMode = String(values.sourceMode ?? "catalog");
  const localSource = sourceMode === "localTemplate" || sourceMode === "existingProject";
  const sourcePath = localSource
    ? sourceMode === "existingProject"
      ? projectPath
      : String(values.sourcePath || "") || undefined
    : String(values.sourcePath || "") || undefined;
  const binding = localSource ? sourceBinding : undefined;
  const approval = await requestExecutionApproval({
    action: "prepare",
    projectPath,
    environment,
    templateId: entry.id,
    revision,
    sourceBinding: binding
  });
  const project = await prepareDeployment({
    templateId: entry.id,
    sourceMode,
    executionApprovalToken: approval.token,
    sourcePath,
    sourceRevision: revision,
    sourceBinding: binding,
    destination: projectPath,
    environment,
    tenant: String(values.tenantId || "") || undefined,
    subscription: String(values.subscriptionId || "") || undefined,
    location: String(values.location || "westus2"),
    resourceGroup: String(values.resourceGroup || "") || undefined,
    environmentValues: environmentValuesForPlan(plan, values)
  });
  return { project, values: { ...values, projectPath: project.path } };
}

export async function startApprovedDeploymentOperation(
  action: "preview" | "up" | "provision" | "deploy",
  entry: CatalogEntry,
  projectPath: string,
  environment: string
): Promise<string> {
  const approval = await requestExecutionApproval({
    action,
    projectPath,
    environment,
    templateId: entry.id,
    revision: entry.commit === "local" ? undefined : entry.commit
  });
  return startAzdOperation(action, approval.projectPath, environment, approval.token);
}
