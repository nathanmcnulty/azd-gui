import { describe, expect, it } from "vitest";
import {
  connectionBlockingGuidance,
  hasKnownTargetMismatch,
  toolBlockingGuidance
} from "./admin-guidance";
import type { ConnectionStatus, ConnectionTarget, ToolStatus } from "./types";

const target: ConnectionTarget = {
  tenantId: "tenant-a",
  subscriptionId: "subscription-a"
};

function connection(overrides: Partial<ConnectionStatus> = {}): ConnectionStatus {
  return {
    id: "azureCli",
    name: "Azure CLI",
    required: true,
    reason: "Deploy Azure resources.",
    state: "connected",
    tenantId: target.tenantId,
    subscriptionId: target.subscriptionId,
    ...overrides
  };
}

function tool(overrides: Partial<ToolStatus> = {}): ToolStatus {
  return {
    id: "azd",
    name: "Azure Developer CLI",
    kind: "tool",
    required: true,
    state: "ready",
    ...overrides
  };
}

describe("admin next-action guidance", () => {
  it("names the first required tool and its recoverable action", () => {
    expect(toolBlockingGuidance([tool({ state: "checking" })])).toBe(
      "Checking Azure Developer CLI. Wait for the result before continuing."
    );
    expect(toolBlockingGuidance([tool({ state: "missing", canInstall: true })])).toBe(
      "Review and install Azure Developer CLI, then select Check again."
    );
    expect(toolBlockingGuidance([tool({ state: "unknown" })])).toBe(
      "Could not verify Azure Developer CLI. Select Check again; if the check still fails, contact the template author."
    );
    expect(toolBlockingGuidance([tool({
      state: "unknown",
      documentationUrl: "https://learn.microsoft.com/azure/developer/azure-developer-cli/"
    })])).toBe(
      "Could not verify Azure Developer CLI. Select Check again; if the check still fails, view its instructions."
    );
  });

  it("distinguishes unavailable accounts from known target mismatches", () => {
    const unknown = connection({ state: "unknown", tenantId: undefined, subscriptionId: undefined });
    expect(hasKnownTargetMismatch(unknown, target)).toBe(false);
    expect(connectionBlockingGuidance([unknown], target)).toBe(
      "Connect Azure CLI to retry its account check."
    );

    const mismatch = connection({ tenantId: "tenant-b" });
    expect(hasKnownTargetMismatch(mismatch, target)).toBe(true);
    expect(connectionBlockingGuidance([mismatch], target)).toBe(
      "Reconnect Azure CLI to the selected Azure tenant and subscription."
    );
  });

  it("keeps connected accounts blocked when their target cannot be verified", () => {
    const unverified = connection({ tenantId: undefined, subscriptionId: undefined });
    expect(hasKnownTargetMismatch(unverified, target)).toBe(false);
    expect(connectionBlockingGuidance([unverified], target)).toBe(
      "Reconnect Azure CLI so its Azure tenant and subscription can be verified."
    );
  });

  it("identifies expired sign-ins and missing Graph scopes", () => {
    expect(connectionBlockingGuidance([connection({ state: "expired" })])).toBe(
      "Reconnect Azure CLI; its sign-in has expired."
    );
    expect(connectionBlockingGuidance([
      connection({
        id: "graphPowerShell",
        name: "Microsoft Graph PowerShell",
        scopes: ["Directory.Read.All"],
        grantedScopes: []
      })
    ])).toBe("Update Microsoft Graph PowerShell scopes to continue.");
  });
});
