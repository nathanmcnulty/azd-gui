import { beforeEach, describe, expect, it, vi } from "vitest";
import type { ExecutionApprovalRequest, ProjectSummary } from "../types";

const calls = vi.hoisted(() => ({
  approvals: [] as ExecutionApprovalRequest[],
  starts: [] as Array<[string, string, string | undefined, string | undefined]>
}));

vi.mock("../bridge", () => ({
  getOperationHistory: vi.fn(),
  getProjectState: vi.fn(),
  getProjectTemplate: vi.fn(),
  listProjectEnvironments: vi.fn(),
  requestExecutionApproval: async (request: ExecutionApprovalRequest) => {
    calls.approvals.push(request);
    return { token: "approval-token", projectPath: request.projectPath };
  },
  startAzdOperation: async (...args: [string, string, string | undefined, string | undefined]) => {
    calls.starts.push(args);
    return "operation-id";
  }
}));

import { getOperationHistory, getProjectState, getProjectTemplate, listProjectEnvironments } from "../bridge";
import { loadProjectWorkspace, startProjectOperation } from "./project-operation-controller";

const project: ProjectSummary = {
  id: "project",
  name: "Project",
  path: "C:\\Projects\\project",
  state: "ready"
};

describe("loadProjectWorkspace", () => {
  beforeEach(() => {
    vi.mocked(getProjectState).mockReset().mockResolvedValue({ name: "project" });
    vi.mocked(listProjectEnvironments).mockReset();
    vi.mocked(getProjectTemplate).mockResolvedValue(null);
    vi.mocked(getOperationHistory).mockResolvedValue([]);
  });

  it("waits for the discovered environment before reading its resources", async () => {
    let resolveEnvironments!: (value: Awaited<ReturnType<typeof listProjectEnvironments>>) => void;
    vi.mocked(listProjectEnvironments).mockReturnValue(new Promise((resolve) => {
      resolveEnvironments = resolve;
    }));
    const loading = loadProjectWorkspace({ ...project, environment: "dev" });
    expect(getProjectState).not.toHaveBeenCalled();
    resolveEnvironments([{ name: "production", dotEnvPath: "", hasLocal: true, hasRemote: false, isDefault: true }]);

    const snapshot = await loading;
    expect(getProjectState).toHaveBeenCalledExactlyOnceWith(project.path, "production");
    expect(snapshot.currentEnvironment).toBe("production");
    expect(snapshot.failedSources).toBe(0);
  });

  it("uses the saved environment and reports discovery failures", async () => {
    vi.mocked(listProjectEnvironments).mockRejectedValue(new Error("unavailable"));
    const snapshot = await loadProjectWorkspace({ ...project, environment: "dev" });
    expect(getProjectState).toHaveBeenCalledExactlyOnceWith(project.path, "dev");
    expect(snapshot.currentEnvironment).toBe("dev");
    expect(snapshot.failedSources).toBe(1);
  });

  it("does not retain another environment's state when the current read fails", async () => {
    vi.mocked(listProjectEnvironments).mockResolvedValue([{ name: "production", dotEnvPath: "", hasLocal: true, hasRemote: false, isDefault: true }]);
    vi.mocked(getProjectState).mockRejectedValue(new Error("unavailable"));
    const snapshot = await loadProjectWorkspace({ ...project, environment: "dev" });
    expect(snapshot.currentEnvironment).toBe("production");
    expect(snapshot.state).toBeNull();
    expect(snapshot.failedSources).toBe(1);
  });
});

describe("startProjectOperation", () => {
  beforeEach(() => {
    calls.approvals = [];
    calls.starts = [];
  });

  it("rejects a mutating action without an environment before requesting approval", async () => {
    await expect(startProjectOperation(project, "down")).rejects.toThrow(/select an environment/i);
    expect(calls.approvals).toEqual([]);
    expect(calls.starts).toEqual([]);
  });

  it("binds the approved and executed lifecycle action to the same environment", async () => {
    await startProjectOperation({ ...project, environment: "production" }, "down");

    expect(calls.approvals[0]?.environment).toBe("production");
    expect(calls.starts[0]).toEqual([
      "down",
      project.path,
      "production",
      "approval-token"
    ]);
  });
});
