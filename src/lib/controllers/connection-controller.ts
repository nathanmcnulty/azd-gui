import { refreshConnections } from "../bridge";
import {
  connectionMeetsTarget,
  retainGrantedGraphScopes
} from "../connections";
import type { ConnectionStatus, ConnectionTarget } from "../types";

export async function refreshConnectionList(
  current: ConnectionStatus[],
  target?: ConnectionTarget
): Promise<ConnectionStatus[]> {
  return retainGrantedGraphScopes(current, await refreshConnections(target));
}

export async function verifyConnectionContext(
  provider: ConnectionStatus,
  current: ConnectionStatus[],
  target?: ConnectionTarget,
  delays = [0, 400, 1200]
): Promise<{ connections: ConnectionStatus[]; connected: boolean }> {
  let connections = current;
  for (const [attempt, delay] of delays.entries()) {
    if (delay) await new Promise((resolve) => setTimeout(resolve, delay));
    try {
      connections = await refreshConnectionList(connections, target);
      const refreshed = connections.find((connection) => connection.id === provider.id);
      if (refreshed?.state === "connected" && (!target || connectionMeetsTarget(refreshed, target))) {
        return { connections, connected: true };
      }
    } catch (error) {
      if (attempt === delays.length - 1) {
        connections = connections.map((connection) =>
          connection.id === provider.id
            ? { ...connection, state: "unknown", detail: `Status check failed: ${String(error)}` }
            : connection
        );
      }
    }
  }
  return { connections, connected: false };
}

export function previewConnectedProvider(
  current: ConnectionStatus[],
  provider: ConnectionStatus,
  target?: ConnectionTarget
): ConnectionStatus[] {
  return current.map((connection) =>
    connection.id === provider.id
      ? {
          ...connection,
          state: "connected",
          account: "preview@example.com",
          tenantId: target?.tenantId ?? "72f988bf-86f1-41af-91ab-2d7cd011db47",
          subscriptionId:
            provider.id === "azureCli" || provider.id === "azPowerShell"
              ? target?.subscriptionId ?? connection.subscriptionId
              : connection.subscriptionId,
          grantedScopes:
            provider.id === "graphPowerShell"
              ? [...(provider.scopes ?? [])]
              : connection.grantedScopes
        }
      : connection
  );
}
