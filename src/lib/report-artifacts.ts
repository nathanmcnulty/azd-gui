import type { ReportArtifactDeclaration } from "./types";
export interface ArtifactRead {
  context: { snapshotDigest: string; environment: string | null; revision: string | null } | null;
  observedAt: string;
  artifacts: Array<{
    declaration: ReportArtifactDeclaration;
    state: "available" | "hidden" | "missing" | "unsafePath" | "tooLarge" | "invalid" | "unreadable";
    content?: string; sha256?: string; modifiedAt?: string;
  }>;
}
export const artifactStateLabels = {
  available: "Available, verification unproven", hidden: "Sensitive content hidden",
  missing: "Missing", unsafePath: "Unsafe path", tooLarge: "Report exceeds 64 KiB",
  invalid: "Invalid content", unreadable: "Unreadable"
};
