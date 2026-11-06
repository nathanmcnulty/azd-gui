import { beforeEach, describe, expect, it, vi } from "vitest";
import type { CatalogEntry, ExecutionApprovalRequest, PrepareDeploymentRequest } from "../types";

const calls = vi.hoisted(() => ({
  approvals: [] as ExecutionApprovalRequest[],
  prepares: [] as PrepareDeploymentRequest[],
  starts: [] as string[]
}));

vi.mock("../bridge", () => ({
  getReviewedContractDigest: async () => "a".repeat(64),
  reviewWizardRequirements: async () => ({ snapshot: { digest: "c".repeat(64), requirements: {} }, selection: {} }),
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
      sourceMode: "catalog", projectPath: "C:\\Projects\\template", environmentName: "dev", location: "westus2",
      tenantId: "11111111-1111-1111-1111-111111111111",
      subscriptionId: "22222222-2222-2222-2222-222222222222",
      resourceGroup: "rg-template-dev"
    });

    expect(calls.approvals[0]?.snapshotDigest).toBe("a".repeat(64));
    expect(calls.approvals[0]?.requirements?.digest).toBe("c".repeat(64));
    expect(calls.prepares[0]?.reviewedContractDigest).toBe("a".repeat(64));
    expect(calls.approvals[0]?.target).toEqual({
      tenantId: calls.prepares[0]?.tenant,
      subscriptionId: calls.prepares[0]?.subscription,
      location: calls.prepares[0]?.location,
      resourceGroup: calls.prepares[0]?.resourceGroup
    });
  });

  it("uses the persisted project snapshot for a wizard lifecycle approval", async () => {
    await startApprovedDeploymentOperation("preview", "C:\\Projects\\template", "dev");

    expect(calls.approvals[0]).toMatchObject({
      action: "preview", templateId: entry.id, snapshotDigest: "b".repeat(64)
    });
    expect(calls.starts).toEqual(["preview"]);
  });
});

describe("acknowledgement enforcement at the preparation caller", () => {
  function acknowledgedEntry(): CatalogEntry {
    return { ...entry, manifest: { schemaVersion: "1.0", prerequisites: [], connections: [], configuration: { groups: [{ id: "scope", title: "Scope", fields: [
      { id: "scope", label: "Role scope", type: "select", environmentVariable: "SCOPE", options: [{ label: "All", value: "all" }, { label: "Selected", value: "selected" }] },
      { id: "ack", label: "Every role", type: "acknowledgement", required: true, environmentVariable: "ACK", condition: { field: "scope", equals: "all" } }
    ] }] } } };
  }
  const values = { projectPath: "C:\\Projects\\ack", environmentName: "dev", tenantId: "11111111-1111-1111-1111-111111111111", scope: "all", ack: true };
  const proofs = [{ fieldId: "ack", tenantId: values.tenantId, controllingValues: { scope: "all" } }];

  it.each([{}, { ack: false }, { ack: "true" }])("rejects saved/missing/false values even with a forged complete plan: %j", async (override) => {
    const candidate = acknowledgedEntry();
    await expect(materializeDeployment(candidate, { templateId: candidate.id, revision: 1, pages: [] }, { ...values, ...override })).rejects.toThrow(/Accept/);
    expect(calls.approvals).toEqual([]);
    expect(calls.prepares).toEqual([]);
  });
  it("rejects stale tenant and input proofs before requesting native execution approval", async () => {
    const candidate = acknowledgedEntry();
    await expect(materializeDeployment(candidate, { templateId: candidate.id, revision: 1, pages: [] }, { ...values, tenantId: "changed" }, undefined, proofs)).rejects.toThrow(/Accept/);
    await expect(materializeDeployment(candidate, { templateId: candidate.id, revision: 1, pages: [] }, values, undefined, [{ ...proofs[0], controllingValues: { scope: "selected" } }])).rejects.toThrow(/Accept/);
    expect(calls.approvals).toEqual([]);
  });
  it("passes explicit proof and identical reviewed values to the approval and native preparation", async () => {
    const candidate = acknowledgedEntry();
    const { calculateWizardPlan } = await import("../wizard");
    const plan = calculateWizardPlan(candidate, values, undefined, proofs);
    await materializeDeployment(candidate, plan, values, undefined, proofs);
    expect(calls.approvals[0]).toMatchObject({ acknowledgementProofs: proofs, acknowledgementValues: { SCOPE: "all", ACK: true } });
    expect(calls.prepares[0]).toMatchObject({ acknowledgementProofs: proofs, environmentValues: { SCOPE: "all", ACK: true } });
  });
  it("preserves explicit empty-string controllers in approval and preparation", async () => {
    const candidate = acknowledgedEntry();
    const fields = candidate.manifest!.configuration.groups[0].fields;
    fields[0] = { id: "scope", label: "Scope", type: "text", environmentVariable: "SCOPE", default: "" };
    fields[1].condition = { field: "scope", notEquals: "limited" };
    const blank = { ...values, scope: "" };
    const blankProofs = [{ ...proofs[0], controllingValues: { scope: "" } }];
    const { calculateWizardPlan } = await import("../wizard");
    await materializeDeployment(candidate, calculateWizardPlan(candidate, blank, undefined, blankProofs), blank, undefined, blankProofs);
    expect(calls.approvals[0].acknowledgementValues).toEqual({ SCOPE: "", ACK: true });
    expect(calls.prepares[0].environmentValues).toEqual({ SCOPE: "", ACK: true });
  });

  it("does not prepare when the wizard closes or changes while the native approval is open", async () => {
    const candidate = acknowledgedEntry();
    let checks = 0;
    await expect(materializeDeployment(candidate, { templateId: candidate.id, revision: 1, pages: [] }, values, undefined, proofs, () => ++checks === 1)).rejects.toThrow(/changed during approval/);
    expect(calls.approvals).toHaveLength(1);
    expect(calls.prepares).toEqual([]);
  });
  it("allows inactive acknowledgements without a proof", async () => {
    const candidate = acknowledgedEntry();
    await materializeDeployment(candidate, { templateId: candidate.id, revision: 1, pages: [] }, { ...values, scope: "selected", ack: false });
    expect(calls.prepares).toHaveLength(1);
  });
  it("forwards fresh proof to lifecycle approval and checks interruption before starting", async () => {
    await startApprovedDeploymentOperation("preview", values.projectPath, values.environmentName, proofs);
    expect(calls.approvals[0].acknowledgementProofs).toEqual(proofs);
    let checks = 0;
    await expect(startApprovedDeploymentOperation("up", values.projectPath, values.environmentName, proofs, () => ++checks === 1)).rejects.toThrow(/changed during approval/);
    expect(calls.starts).toEqual(["preview"]);
  });
});
