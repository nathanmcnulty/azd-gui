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
