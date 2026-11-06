import {
  getProjectTemplate,
  getReviewedContractDigest,
  prepareDeployment,
  requestExecutionApproval,
  startAzdOperation
} from "../bridge";
import type {
  AcknowledgementProof,
  CatalogEntry,
  LocalSourceBinding,
  ProjectSummary,
  WizardPlan,
  WizardValues
} from "../types";
import { acknowledgementEnvironmentValues, assertAcknowledgements } from "../acknowledgements";
import { environmentValuesForPlan } from "../wizard";

export async function materializeDeployment(
  entry: CatalogEntry,
  plan: WizardPlan,
  values: WizardValues,
  sourceBinding?: LocalSourceBinding,
  acknowledgementProofs: AcknowledgementProof[] = [],
  isCurrent: () => boolean = () => true
): Promise<{ project: ProjectSummary; values: WizardValues }> {
  assertAcknowledgements(entry.manifest, values, acknowledgementProofs);
  const acknowledgementValues = acknowledgementEnvironmentValues(entry.manifest, values);
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
  const reviewedContractDigest = await getReviewedContractDigest(entry);
  if (!isCurrent()) throw new Error("Configuration changed. Review and acknowledge the current choices again.");
  const approval = await requestExecutionApproval({
    action: "prepare",
    acknowledgementProofs,
    acknowledgementValues,
    projectPath,
    environment,
    templateId: entry.id,
    revision,
    snapshotDigest: reviewedContractDigest,
    sourceBinding: binding,
    target: {
      tenantId: String(values.tenantId || "") || undefined,
      subscriptionId: String(values.subscriptionId || "") || undefined,
      location: String(values.location || "westus2"),
      resourceGroup: String(values.resourceGroup || "") || undefined
    }
  });
  if (!isCurrent()) throw new Error("Configuration changed during approval. Review and acknowledge again.");
  const project = await prepareDeployment({
    acknowledgementProofs,
    templateId: entry.id,
    sourceMode,
    executionApprovalToken: approval.token,
    sourcePath,
    sourceRevision: revision,
    sourceBinding: binding,
    reviewedContractDigest,
    destination: projectPath,
    environment,
    tenant: String(values.tenantId || "") || undefined,
    subscription: String(values.subscriptionId || "") || undefined,
    location: String(values.location || "westus2"),
    resourceGroup: String(values.resourceGroup || "") || undefined,
    environmentValues: { ...environmentValuesForPlan(plan, values), ...acknowledgementValues }
  });
  return { project, values: { ...values, projectPath: project.path } };
}

export async function startApprovedDeploymentOperation(
  action: "preview" | "up" | "provision" | "deploy",
  projectPath: string,
  environment: string,
  acknowledgementProofs: AcknowledgementProof[] = [],
  isCurrent: () => boolean = () => true
): Promise<string> {
  const snapshot = await getProjectTemplate(projectPath);
  if (!snapshot) throw new Error("This project needs a saved reviewed template contract before lifecycle actions.");
  if (!isCurrent()) throw new Error("Configuration changed. Review and acknowledge the current choices again.");
  const approval = await requestExecutionApproval({
    action,
    acknowledgementProofs,
    projectPath,
    environment,
    templateId: snapshot.entry.id,
    revision: snapshot.entry.commit === "local" ? undefined : snapshot.entry.commit,
    snapshotDigest: snapshot.digest
  });
  if (!isCurrent()) throw new Error("Configuration changed during approval. Review and acknowledge again.");
  return startAzdOperation(action, approval.projectPath, environment, approval.token);
}
