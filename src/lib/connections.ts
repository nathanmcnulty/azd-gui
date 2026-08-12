import type { ConnectionStatus, ConnectionTarget } from "./types";

export function missingRequestedScopes(connection: ConnectionStatus): string[] {
  if (connection.id !== "graphPowerShell" || connection.state !== "connected") return [];
  const granted = new Set(
    (connection.grantedScopes ?? []).map((scope) => scope.toLowerCase())
  );
  return (connection.scopes ?? []).filter(
    (scope) => !granted.has(scope.toLowerCase())
  );
}

export function connectionMeetsRequirements(connection: ConnectionStatus): boolean {
  return connection.state === "connected" && missingRequestedScopes(connection).length === 0;
}

export function connectionMeetsTarget(
  connection: ConnectionStatus,
  target: ConnectionTarget
): boolean {
  if (!connectionMeetsRequirements(connection)) return false;
  if (connection.tenantId?.toLowerCase() !== target.tenantId.toLowerCase()) return false;
  if (["azureCli", "azPowerShell"].includes(connection.id)) {
    return connection.subscriptionId?.toLowerCase() === target.subscriptionId.toLowerCase();
  }
  return true;
}

export function retainGrantedGraphScopes(
  current: ConnectionStatus[],
  refreshed: ConnectionStatus[]
): ConnectionStatus[] {
  const grantedScopes = current.find(
    (connection) => connection.id === "graphPowerShell"
  )?.grantedScopes;
  if (!grantedScopes?.length) return refreshed;
  return refreshed.map((connection) =>
    connection.id === "graphPowerShell" &&
    connection.state === "connected" &&
    !connection.grantedScopes?.length
      ? { ...connection, grantedScopes: [...grantedScopes] }
      : connection
  );
}
