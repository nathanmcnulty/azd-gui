export type ComponentStatus = "verified" | "modified" | "missing" | "unsupported";
export type ComponentReason = "invalidMetadata" | "manifestVersion" | "unsafePath" | "limitExceeded" | "unreadable";

export interface ManagedComponentFile {
  source: string;
  target: string;
  sha256: string;
  status: ComponentStatus;
  reason?: ComponentReason;
}

export interface ComponentProvenance {
  id: string;
  version: string;
  sourceRepository: string;
  sourceRevision: string;
  status: ComponentStatus;
  unknownMetadata: boolean;
  files: ManagedComponentFile[];
}

export interface ComponentProvenanceRead {
  state: "available" | "notFound" | "unsupported";
  reason?: ComponentReason;
  manifestVersion?: string;
  unknownMetadata: boolean;
  components: ComponentProvenance[];
}

export const componentStatusLabels: Record<ComponentStatus, string> = {
  verified: "Verified", modified: "Modified", missing: "Missing", unsupported: "Unsupported"
};

export const componentReasonText: Record<ComponentReason, string> = {
  invalidMetadata: "The metadata does not match the supported component lock contract.",
  manifestVersion: "This component lock version is not supported by this app.",
  unsafePath: "The path is unsafe, contains a link or reparse point, or is not a regular local file.",
  limitExceeded: "The local verification safety limit was exceeded.",
  unreadable: "The local file could not be read consistently."
};
