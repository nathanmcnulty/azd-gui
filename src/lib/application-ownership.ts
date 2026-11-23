import { invoke, isTauri } from "@tauri-apps/api/core";

export type ApplicationVerification =
  | { state: "unknown"; reason: "unavailable" | "accessDenied" | "incomplete" | "cancelled" }
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
  binding: {
    provider: "entra-application-registration";
    cloud: "AzureCloud";
    tenantId: string;
    subscriptionId: string;
    environment: string;
    operatorDigest: string;
    targetResourceDigest: string;
    sourceContractDigest: string;
    inputsDigest: string;
    objectId: string;
    expectedApplicationId?: string;
  };
  applicationId: string;
  verifiedAt: string;
  reviewedAt: string;
  verificationDigest: string;
  receiptDigest: string;
}

export async function verifyProjectApplication(projectPath: string, environment: string): Promise<ApplicationVerification> {
  if (!isTauri()) return { state: "unknown", reason: "unavailable" };
  return invoke("verify_project_application_ownership", { projectPath, environment });
}

export async function confirmProjectApplicationReview(
  projectPath: string, environment: string, verificationDigest: string
): Promise<ApplicationReviewReceipt> {
  if (!isTauri()) throw new Error("Native review is unavailable.");
  return invoke("confirm_project_application_review", { projectPath, environment, verificationDigest });
}
