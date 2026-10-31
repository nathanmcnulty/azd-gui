import { invoke, isTauri } from '@tauri-apps/api/core';

export interface PimPlanConfiguration {
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
  plan: {
    mode: string;
    generatedAt: string;
    warnings: string[];
    tiers: Record<string, {
      roles: { id: string; displayName: string }[];
      context: { action: string; displayName: string } | null;
      conditionalAccessPolicies: { action: string; displayName?: string }[];
      roleRules: { action: string; role: { displayName: string }; activationMfa: { action: string } }[];
    }>;
  };
}
function native() {
  if (!isTauri()) throw new Error('Open the Windows desktop app to review and run a PIM plan.');
}
export async function reviewPimPlan(input: PimPlanInput): Promise<PimPlanReview> {
  native();
  return invoke('review_pim_plan', { input });
}
export async function connectPimPlan(input: PimPlanInput, expectedDigest: string): Promise<void> {
  native();
  await invoke('request_pim_plan_session', { input, expectedDigest });
}
export async function runPimPlan(input: PimPlanInput, expectedDigest: string): Promise<PimPlanArtifact> {
  native();
  return invoke('run_pim_plan', { input, expectedDigest });
}
export async function cancelPimPlan(expectedDigest: string): Promise<void> {
  if (isTauri()) await invoke('cancel_pim_plan', { expectedDigest });
}
