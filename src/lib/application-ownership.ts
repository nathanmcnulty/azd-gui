import { invoke, isTauri } from "@tauri-apps/api/core";

export interface ApplicationTarget {
  tenantId: string;
  subscriptionId: string;
  environment: string;
  targetResourceDigest: string;
  sourceContractDigest: string;
  inputsDigest: string;
  objectId: string;
  expectedApplicationId?: string;
}

export interface ApplicationReviewContext extends ApplicationTarget {
  resourceGroup: string;
  contextDigest: string;
}

export interface ApplicationReviewRequest {
  projectPath: string;
  environment: string;
  contextDigest: string;
}

export type ApplicationVerification =
  | { state: "unknown"; reason: "unavailable" | "providerUnavailable" | "accessDenied" | "incomplete" | "cancelled" }
  | { state: "foreign"; reason: "differentTenant" | "differentOperator" }
  | { state: "mismatch"; reason: "differentObject" | "differentApplication" | "unexpectedProperties" }
  | {
      kind: "verification";
      state: "verified";
      binding: ApplicationReviewReceipt["binding"];
      checkedAt: string;
      verificationDigest: string;
      properties: {
        objectId: string;
        applicationId: string;
        singleTenant: true;
        identifierUriMatches: true;
        accessTokenVersion: 2;
      };
    };

export interface ApplicationReviewReceipt {
  kind: "ownershipReviewDecision";
  binding: ApplicationTarget & {
    provider: "entra-application-registration";
    cloud: "AzureCloud";
    operatorDigest: string;
  };
  applicationId: string;
  verifiedAt: string;
  reviewedAt: string;
  verificationDigest: string;
  receiptDigest: string;
}

export async function getProjectApplicationReviewContext(
  projectPath: string, environment: string
): Promise<ApplicationReviewContext> {
  if (!isTauri()) throw new Error("Native application review is unavailable.");
  return invoke("get_project_application_review_context", { projectPath, environment });
}

export async function verifyProjectApplication(request: ApplicationReviewRequest): Promise<ApplicationVerification> {
  if (!isTauri()) return { state: "unknown", reason: "unavailable" };
  return invoke("verify_project_application_ownership", { request });
}

export async function confirmProjectApplicationReview(
  request: ApplicationReviewRequest, verificationDigest: string
): Promise<ApplicationReviewReceipt> {
  if (!isTauri()) throw new Error("Native review is unavailable.");
  return invoke("confirm_project_application_review", { request, verificationDigest });
}
