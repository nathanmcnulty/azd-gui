import {
  prepareDeployment,
  requestExecutionApproval,
  startAzdOperation
} from "../bridge";
import type {
  CatalogEntry,
  ProjectSummary,
  WizardPlan,
  WizardValues
} from "../types";
import { environmentValuesForPlan } from "../wizard";

export async function materializeDeployment(
  entry: CatalogEntry,
  plan: WizardPlan,
  values: WizardValues
): Promise<{ project: ProjectSummary; values: WizardValues }> {
  const projectPath = String(values.projectPath || "");
  const environment = String(values.environmentName || "dev");
  const revision = entry.commit === "local" ? undefined : entry.commit;
  const approval = await requestExecutionApproval({
    action: "prepare",
    projectPath,
    environment,
    templateId: entry.id,
    revision
  });
  const project = await prepareDeployment({
    templateId: entry.id,
    sourceMode: String(values.sourceMode ?? "catalog"),
    executionApprovalToken: approval.token,
    sourcePath: String(values.sourcePath || "") || undefined,
    sourceRevision: revision,
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
