import {
  getOperationHistory,
  getProjectState,
  getProjectTemplate,
  listProjectEnvironments,
  requestExecutionApproval,
  startAzdOperation
} from "../bridge";
import type {
  AzdProjectState,
  OperationEvent,
  ProjectEnvironment,
  ProjectSummary
} from "../types";

export type ProjectWorkspaceSnapshot = {
  template: Awaited<ReturnType<typeof getProjectTemplate>>;
  state: AzdProjectState | null;
  environments: ProjectEnvironment[];
  history: OperationEvent[];
  currentEnvironment?: string;
  failedSources: number;
};

export async function loadProjectWorkspace(project: ProjectSummary): Promise<ProjectWorkspaceSnapshot> {
  const [environmentsResult, historyResult, templateResult] = await Promise.allSettled([
    listProjectEnvironments(project.path),
    getOperationHistory(project.path),
    getProjectTemplate(project.path)
  ]);
  const environments =
    environmentsResult.status === "fulfilled"
      ? environmentsResult.value
      : project.environment
        ? [{ name: project.environment, dotEnvPath: "", hasLocal: false, hasRemote: false, isDefault: true }]
        : [];
  const currentEnvironment =
    environments.find((environment) => environment.isDefault)?.name ?? project.environment;
  const [stateResult] = await Promise.allSettled([
    getProjectState(project.path, currentEnvironment)
  ]);
  return {
    template: templateResult.status === "fulfilled" ? templateResult.value : null,
    state: stateResult.status === "fulfilled" ? stateResult.value : null,
    environments,
    history: historyResult.status === "fulfilled" ? historyResult.value : [],
    currentEnvironment,
    failedSources: [stateResult, environmentsResult, historyResult, templateResult].filter(
      (result) => result.status === "rejected"
    ).length
  };
}

export type ProjectAction = "preview" | "up" | "provision" | "deploy" | "refresh" | "down";

export async function startProjectOperation(
  project: ProjectSummary,
  action: ProjectAction,
  snapshotDigest?: string
): Promise<string> {
  const requiresApproval = ["preview", "up", "provision", "deploy", "down"].includes(action);
  const environment = project.environment;
  if (requiresApproval) {
    if (!environment?.trim()) {
      throw new Error("Select an environment before starting this lifecycle action.");
    }
    const approval = await requestExecutionApproval({
      action: action as Exclude<ProjectAction, "refresh">,
      projectPath: project.path,
      environment,
      templateId: project.templateId ?? "",
      revision: project.revision,
      snapshotDigest
    });
    return startAzdOperation(action, approval.projectPath, environment, approval.token);
  }
  return startAzdOperation(action, project.path, environment);
}
