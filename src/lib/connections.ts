import type { ConnectionStatus } from "./types";

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
