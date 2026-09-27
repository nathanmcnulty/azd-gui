import { describe, expect, it } from "vitest";
import {
  connectionMeetsRequirements,
  connectionMeetsTarget,
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
    account: "admin@example.test",
    tenantId: "11111111-1111-1111-1111-111111111111",
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

  it("requires tenant alignment and provider-specific subscription alignment", () => {
    const target = {
      tenantId: "11111111-1111-1111-1111-111111111111",
      subscriptionId: "22222222-2222-2222-2222-222222222222"
    };
    const grantedScopes = ["Application.ReadWrite.All", "RoleManagement.ReadWrite.Directory"];
    expect(
      connectionMeetsTarget(graphConnection({ tenantId: target.tenantId, grantedScopes }), target)
    ).toBe(true);
    expect(
      connectionMeetsTarget(graphConnection({ tenantId: "different", grantedScopes }), target)
    ).toBe(false);
    expect(
      connectionMeetsTarget(
        graphConnection({
          id: "azureCli",
          tenantId: target.tenantId,
          subscriptionId: target.subscriptionId
        }),
        target
      )
    ).toBe(true);
    expect(
      connectionMeetsTarget(
        graphConnection({
          id: "azureCli",
          tenantId: target.tenantId,
          subscriptionId: "different"
        }),
        target
      )
    ).toBe(false);
  });

  it("retains confirmed Graph scopes when refresh finds the persisted user context", () => {
    const current = [graphConnection()];
    const refreshed = [graphConnection({ account: "ADMIN@example.test", grantedScopes: [] })];

    expect(retainGrantedGraphScopes(current, refreshed)[0].grantedScopes).toEqual([
      "application.readwrite.all"
    ]);
  });

  it("does not retain Graph scopes for a disconnected refresh", () => {
    const current = [graphConnection()];
    const refreshed = [graphConnection({ state: "disconnected", grantedScopes: [] })];

    expect(retainGrantedGraphScopes(current, refreshed)[0].grantedScopes).toEqual([]);
  });

  it("does not carry grants to another account or tenant", () => {
    const current = [graphConnection({ grantedScopes: ["Application.ReadWrite.All", "RoleManagement.ReadWrite.Directory"] })];
    const target = {
      tenantId: "22222222-2222-2222-2222-222222222222",
      subscriptionId: "33333333-3333-3333-3333-333333333333"
    };
    const [connection] = retainGrantedGraphScopes(current, [graphConnection({
      account: "other@example.test",
      grantedScopes: []
    })]);
    expect(connection.grantedScopes).toEqual([]);
    expect(connectionMeetsTarget(connection, {
      ...target,
      tenantId: "11111111-1111-1111-1111-111111111111"
    })).toBe(false);
    expect(retainGrantedGraphScopes(current, [graphConnection({
      tenantId: target.tenantId,
      grantedScopes: []
    })])[0].grantedScopes).toEqual([]);
  });

  it("does not carry grants when either identity is unknown or the old session is expired", () => {
    const current = [graphConnection()];
    expect(retainGrantedGraphScopes(current, [graphConnection({ account: undefined, grantedScopes: [] })])[0].grantedScopes).toEqual([]);
    expect(retainGrantedGraphScopes(current, [graphConnection({ tenantId: undefined, grantedScopes: [] })])[0].grantedScopes).toEqual([]);
    expect(retainGrantedGraphScopes([graphConnection({ account: undefined })], [graphConnection({ grantedScopes: [] })])[0].grantedScopes).toEqual([]);
    expect(retainGrantedGraphScopes([graphConnection({ state: "expired" })], [graphConnection({ grantedScopes: [] })])[0].grantedScopes).toEqual([]);
  });
});
