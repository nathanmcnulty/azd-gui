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

export function connectionTargetAssessment(
  connection: ConnectionStatus,
  target: ConnectionTarget | null,
  fresh: boolean
): { label: string; action: string } {
  if (!target) return { label: "Target not selected", action: "Choose an Azure tenant and subscription" };
  if (!fresh || connection.state === "checking") return { label: "Unverified", action: "Check this provider for the selected target" };
  if (connection.state === "expired") return { label: "Expired", action: "Reconnect this provider" };
  if (connection.wrongActiveAccount) return { label: "Wrong active account", action: "Reconnect this provider with the intended account" };
  if (connection.state === "disconnected") return { label: "No current connection", action: "Connect this provider" };
  if (connection.state !== "connected") return { label: "Unverified", action: "Reconnect this provider, then check again" };
  if (missingRequestedScopes(connection).length) return { label: "Grant update required", action: "Reconnect this provider to update scopes" };
  if (!connectionMeetsTarget(connection, target)) {
    const knownMismatch = Boolean(connection.tenantId && connection.tenantId.toLowerCase() !== target.tenantId.toLowerCase()) ||
      (["azureCli", "azPowerShell"].includes(connection.id) && Boolean(connection.subscriptionId && connection.subscriptionId.toLowerCase() !== target.subscriptionId.toLowerCase()));
    return knownMismatch
      ? { label: "Wrong target", action: "Reconnect this provider with the intended account" }
      : { label: "Unverified", action: "Reconnect this provider, then check again" };
  }
  return { label: "Verified for target", action: "Continue" };
}

export function retainGrantedGraphScopes(
  current: ConnectionStatus[],
  refreshed: ConnectionStatus[]
): ConnectionStatus[] {
  const previous = current.find(
    (connection) => connection.id === "graphPowerShell"
  );
  const grantedScopes = previous?.grantedScopes;
  const account = previous?.account?.trim().toLowerCase();
  const tenantId = previous?.tenantId?.trim().toLowerCase();
  if (previous?.state !== "connected" || !grantedScopes?.length || !account || !tenantId) {
    return refreshed;
  }
  return refreshed.map((connection) =>
    connection.id === "graphPowerShell" &&
    connection.state === "connected" &&
    connection.account?.trim().toLowerCase() === account &&
    connection.tenantId?.trim().toLowerCase() === tenantId &&
    !connection.grantedScopes?.length
      ? { ...connection, grantedScopes: [...grantedScopes] }
      : connection
  );
}
