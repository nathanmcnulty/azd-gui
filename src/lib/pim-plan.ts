import { invoke, isTauri } from '@tauri-apps/api/core';

export interface PimPlanConfiguration {
  previewExistingObjects: boolean;
  privilegedRoleIds: string[];
  lessPrivilegedRoleIds: string[];
  privilegedAuthenticationProfile: 'mfa' | 'phishingResistant';
  lessPrivilegedAuthenticationProfile: 'mfa' | 'phishingResistant';
  privilegedDeviceRequirement: 'none' | 'compliant' | 'hybridJoined' | 'compliantOrHybrid';
  lessPrivilegedDeviceRequirement: 'none' | 'compliant' | 'hybridJoined' | 'compliantOrHybrid';
}
export interface PimPlanInput {
  reviewId: string;
  account: string;
  tenantId: string;
  clientId: string;
  configuration: PimPlanConfiguration;
}
export interface PimPlanReview {
  sourceCommit: string;
  binding: { requirementsDigest: string; account: string; tenantId: string; clientId: string; scopes: string[] };
  configuration: PimPlanConfiguration;
}
export interface PimPlanArtifact {
  sourceCommit: string;
  requirementsDigest: string;
  configuration: PimPlanConfiguration;
  previewExistingObjects: boolean;
  ownershipReviewRequired: boolean;
  ownershipEstablished: boolean;
  executionAuthorized: boolean;
  ownershipReviewNotice: string;
  executionEvidence: PimProcessEvidence;
  plan: {
    mode: string;
    previewExistingObjects: boolean;
    ownershipReviewRequired: boolean;
    ownershipEstablished: boolean;
    executionAuthorized: boolean;
    configuration: { Mode: string; AdoptExisting: boolean; AdoptRoleContexts: boolean };
    generatedAt: string;
    warnings: string[];
    tiers: Record<string, {
      roles: { id: string; displayName: string }[];
      context: { id?: string; action: string; displayName: string; existing?: unknown; adopted?: boolean } | null;
      conditionalAccessPolicies: { id?: string; action: string; displayName?: string; body?: { displayName?: string }; existing?: unknown }[];
      roleRules: { action: string; role: { displayName: string }; existing?: { claimValue?: string }; desiredContextId?: string; activationMfa: { action: string } }[];
    }>;
  };
}
export interface PimProcessEvidence {
  active: boolean;
  processId: number;
  sessionId: string;
  context: {
    account: string; tenantId: string; clientId: string; resource: string; scopes: string[];
    principalFingerprint: string; verificationReads: number; pimReadsAttempted: number; pimReadsCompleted: number;
  };
}
export interface PimCancellationReceipt {
  requirementsDigest: string;
  snapshotSource: 'nativeRequestRevocationV1' | 'alreadyRevokedNativeWorkerV1' | 'historicalProcessEvidenceV1';
  pimPlanRequestInFlightAtRevocation: boolean | null;
  processEvidence: PimProcessEvidence;
}
function native() {
  if (!isTauri()) throw new Error('Open the Windows desktop app to review and run a PIM plan.');
}
export async function reviewPimPlan(input: PimPlanInput): Promise<PimPlanReview> {
  native();
  return invoke('review_pim_plan', { input });
}
export async function connectPimPlan(input: PimPlanInput, expectedDigest: string): Promise<PimProcessEvidence> {
  native();
  return invoke('request_pim_plan_session', { input, expectedDigest });
}
export async function getPimPlanEvidence(expectedDigest: string): Promise<PimProcessEvidence | null> {
  native();
  return invoke('get_pim_plan_evidence', { expectedDigest });
}
export async function runPimPlan(input: PimPlanInput, expectedDigest: string): Promise<PimPlanArtifact> {
  native();
  return invoke('run_pim_plan', { input, expectedDigest });
}
export async function cancelPimPlan(expectedDigest: string): Promise<PimCancellationReceipt | null> {
  if (isTauri()) return invoke('cancel_pim_plan', { expectedDigest });
  return null;
}
