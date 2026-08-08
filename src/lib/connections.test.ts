import { describe, expect, it } from "vitest";
import {
  connectionMeetsRequirements,
  missingRequestedScopes,
  retainGrantedGraphScopes
} from "./connections";
import type { ConnectionStatus } from "./types";

function graphConnection(overrides: Partial<ConnectionStatus> = {}): ConnectionStatus {
  return {
    id: "graphPowerShell",
    name: "Microsoft Graph PowerShell",
    required: true,
    reason: "Graph operations.",
    state: "connected",
    scopes: ["Application.ReadWrite.All", "RoleManagement.ReadWrite.Directory"],
    grantedScopes: ["application.readwrite.all"],
    ...overrides
  };
}

describe("connection scope requirements", () => {
  it("compares Graph scopes case-insensitively and reports missing grants", () => {
    const connection = graphConnection();

    expect(missingRequestedScopes(connection)).toEqual([
      "RoleManagement.ReadWrite.Directory"
    ]);
    expect(connectionMeetsRequirements(connection)).toBe(false);
  });

  it("accepts a connected Graph context with every requested scope", () => {
    const connection = graphConnection({
      grantedScopes: ["Application.ReadWrite.All", "RoleManagement.ReadWrite.Directory"]
    });

    expect(missingRequestedScopes(connection)).toEqual([]);
    expect(connectionMeetsRequirements(connection)).toBe(true);
  });

  it("does not infer grant requirements for providers without inspectable scopes", () => {
    const connection = graphConnection({
      id: "azureCli",
      scopes: ["https://graph.microsoft.com/.default"],
      grantedScopes: []
    });

    expect(missingRequestedScopes(connection)).toEqual([]);
    expect(connectionMeetsRequirements(connection)).toBe(true);
  });

  it("requires reconnecting an expired provider", () => {
    expect(connectionMeetsRequirements(graphConnection({ state: "expired" }))).toBe(false);
  });

  it("retains confirmed Graph scopes when refresh finds the persisted user context", () => {
    const current = [graphConnection()];
    const refreshed = [graphConnection({ grantedScopes: [] })];

    expect(retainGrantedGraphScopes(current, refreshed)[0].grantedScopes).toEqual([
      "application.readwrite.all"
    ]);
  });

  it("does not retain Graph scopes for a disconnected refresh", () => {
    const current = [graphConnection()];
    const refreshed = [graphConnection({ state: "disconnected", grantedScopes: [] })];

    expect(retainGrantedGraphScopes(current, refreshed)[0].grantedScopes).toEqual([]);
  });
});
