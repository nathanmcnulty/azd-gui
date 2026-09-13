import { beforeEach, describe, expect, it, vi } from "vitest";
import type { ExecutionApprovalRequest, ProjectSummary } from "../types";

const calls = vi.hoisted(() => ({
  approvals: [] as ExecutionApprovalRequest[],
  starts: [] as Array<[string, string, string | undefined, string | undefined]>
}));

vi.mock("../bridge", () => ({
  getOperationHistory: vi.fn(),
  getProjectState: vi.fn(),
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

import { startProjectOperation } from "./project-operation-controller";

const project: ProjectSummary = {
  id: "project",
  name: "Project",
  path: "C:\\Projects\\project",
  state: "ready"
};

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
