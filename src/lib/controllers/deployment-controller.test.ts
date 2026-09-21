import { beforeEach, describe, expect, it, vi } from "vitest";
import type { CatalogEntry, ExecutionApprovalRequest, PrepareDeploymentRequest } from "../types";

const calls = vi.hoisted(() => ({
  approvals: [] as ExecutionApprovalRequest[],
  prepares: [] as PrepareDeploymentRequest[],
  starts: [] as string[]
}));

vi.mock("../bridge", () => ({
  getReviewedContractDigest: async () => "a".repeat(64),
  getProjectTemplate: async () => ({ entry, digest: "b".repeat(64) }),
  requestExecutionApproval: async (request: ExecutionApprovalRequest) => {
    calls.approvals.push(request);
    return { token: "approval", projectPath: request.projectPath };
  },
  prepareDeployment: async (request: PrepareDeploymentRequest) => {
    calls.prepares.push(request);
    return { id: "project", name: "project", path: request.destination, environment: request.environment, state: "ready" };
  },
  startAzdOperation: async (action: string) => {
    calls.starts.push(action);
    return "operation";
  }
}));

import { materializeDeployment, startApprovedDeploymentOperation } from "./deployment-controller";

const entry: CatalogEntry = {
  id: "template", title: "Template", summary: "", description: "", repository: "https://example.test/template",
  commit: "0123456789abcdef0123456789abcdef01234567", tags: [], platforms: [], trusted: true
};

beforeEach(() => {
  calls.approvals = [];
  calls.prepares = [];
  calls.starts = [];
});

describe("wizard deployment approvals", () => {
  it("binds preparation to the reviewed contract digest", async () => {
    await materializeDeployment(entry, { templateId: entry.id, revision: 1, pages: [] }, {
      sourceMode: "catalog", projectPath: "C:\\Projects\\template", environmentName: "dev", location: "westus2"
    });

    expect(calls.approvals[0]?.snapshotDigest).toBe("a".repeat(64));
    expect(calls.prepares[0]?.reviewedContractDigest).toBe("a".repeat(64));
  });

  it("uses the persisted project snapshot for a wizard lifecycle approval", async () => {
    await startApprovedDeploymentOperation("preview", "C:\\Projects\\template", "dev");

    expect(calls.approvals[0]).toMatchObject({
      action: "preview", templateId: entry.id, snapshotDigest: "b".repeat(64)
    });
    expect(calls.starts).toEqual(["preview"]);
  });
});
