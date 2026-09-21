import type { EvidenceBindingState, EvidenceNextAction } from "./types";

export const evidenceBindingDetail: Record<EvidenceBindingState, string> = {
  current: "The project, environment, Azure target, reviewed source, and GUI operation match this record.",
  stale: "A later resource-changing operation exists. Refresh or rerun the relevant verification before relying on this record.",
  mismatch: "This record belongs to different project, target, source, or operation context and cannot describe the selected workspace.",
  legacyUnbound: "This older record has no management binding. Use its counts as local history only.",
  insufficientContext: "The app cannot independently compare every required project, target, source, and operation value."
};

const actionText: Record<EvidenceNextAction["code"], string> = {
  reviewWarnings: "Review recorded warnings",
  retryOperation: "Review the failure and retry the operation",
  verifyResources: "Verify the resulting Azure resources",
  verifyPermissions: "Verify the required permissions",
  proveDelivery: "Complete delivery verification",
  completeCleanup: "Complete and verify cleanup"
};

const ownerText: Record<EvidenceNextAction["owner"], string> = {
  operator: "Operator",
  subscriptionOwner: "Subscription owner",
  identityAdmin: "Identity administrator",
  messagingAdmin: "Messaging administrator",
  applicationOwner: "Application owner",
  templateMaintainer: "Template maintainer"
};

export function evidenceActionText(action: EvidenceNextAction): string {
  return `${actionText[action.code]} · ${ownerText[action.owner]} · ${action.priority}`;
}
