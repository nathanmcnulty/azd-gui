import {
  getOperationHistory,
  getProjectState,
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
  state: AzdProjectState | null;
  environments: ProjectEnvironment[];
  history: OperationEvent[];
  currentEnvironment?: string;
  failedSources: number;
};

export async function loadProjectWorkspace(project: ProjectSummary): Promise<ProjectWorkspaceSnapshot> {
  const [stateResult, environmentsResult, historyResult] = await Promise.allSettled([
    getProjectState(project.path, project.environment),
    listProjectEnvironments(project.path),
    getOperationHistory(project.path)
  ]);
  const environments =
    environmentsResult.status === "fulfilled"
      ? environmentsResult.value
      : project.environment
        ? [{ name: project.environment, dotEnvPath: "", hasLocal: false, hasRemote: false, isDefault: true }]
        : [];
  return {
    state: stateResult.status === "fulfilled" ? stateResult.value : null,
    environments,
    history: historyResult.status === "fulfilled" ? historyResult.value : [],
    currentEnvironment:
      environments.find((environment) => environment.isDefault)?.name ?? project.environment,
    failedSources: [stateResult, environmentsResult, historyResult].filter(
      (result) => result.status === "rejected"
    ).length
  };
}

export type ProjectAction = "preview" | "up" | "provision" | "deploy" | "refresh" | "down";

export async function startProjectOperation(
  project: ProjectSummary,
  action: ProjectAction
): Promise<string> {
  const requiresApproval = ["preview", "up", "provision", "deploy", "down"].includes(action);
  const approval = requiresApproval
    ? await requestExecutionApproval({
        action: action as Exclude<ProjectAction, "refresh">,
        projectPath: project.path,
        environment: project.environment ?? "dev",
        templateId: project.templateId ?? "",
        revision: project.revision
      })
    : undefined;
  return startAzdOperation(
    action,
    approval?.projectPath ?? project.path,
    project.environment,
    approval?.token
  );
}
