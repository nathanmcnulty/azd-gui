import { connectionMeetsRequirements, connectionMeetsTarget, missingRequestedScopes } from "./connections";
import type { ConnectionStatus, ConnectionTarget, ToolStatus } from "./types";

export function toolBlockingGuidance(tools: ToolStatus[]): string {
  const tool = tools.find((item) => item.required && item.state !== "ready");
  if (!tool) return "";

  switch (tool.state) {
    case "checking":
      return `Checking ${tool.name}. Wait for the result before continuing.`;
    case "missing":
      return tool.canInstall
        ? `Review and install ${tool.name}, then select Check again.`
        : `Install ${tool.name} from its instructions, then select Check again.`;
    case "outdated":
      return tool.canInstall
        ? `Review and update ${tool.name}, then select Check again.`
        : `Update ${tool.name} from its instructions, then select Check again.`;
    case "unknown":
      return tool.documentationUrl
        ? `Could not verify ${tool.name}. Select Check again; if the check still fails, view its instructions.`
        : `Could not verify ${tool.name}. Select Check again; if the check still fails, contact the template author.`;
    default:
      return `Resolve ${tool.name}, then select Check again.`;
  }
}

export function hasKnownTargetMismatch(
  connection: ConnectionStatus,
  target: ConnectionTarget
): boolean {
  if (connection.state !== "connected") return false;
  if (connection.tenantId && connection.tenantId.toLowerCase() !== target.tenantId.toLowerCase()) {
    return true;
  }
  return ["azureCli", "azPowerShell"].includes(connection.id) && Boolean(
    connection.subscriptionId &&
    connection.subscriptionId.toLowerCase() !== target.subscriptionId.toLowerCase()
  );
}

export function connectionBlockingGuidance(
  connections: ConnectionStatus[],
  target?: ConnectionTarget | null
): string {
  const connection = connections.find((item) =>
    item.required && (
      !connectionMeetsRequirements(item) ||
      Boolean(target && !connectionMeetsTarget(item, target))
    )
  );
  if (!connection) return "";

  if (connection.state === "checking") {
    return `Checking ${connection.name}. Wait for the result before continuing.`;
  }
  if (connection.state === "expired") {
    return `Reconnect ${connection.name}; its sign-in has expired.`;
  }
  if (connection.state === "disconnected") {
    return `Connect ${connection.name} to continue.`;
  }
  if (connection.state === "unknown") {
    return `Connect ${connection.name} to retry its account check.`;
  }
  if (missingRequestedScopes(connection).length) {
    return `Update ${connection.name} scopes to continue.`;
  }
  const targetLabel = ["azureCli", "azPowerShell"].includes(connection.id)
    ? "Azure tenant and subscription"
    : "Azure tenant";
  if (target && hasKnownTargetMismatch(connection, target)) {
    return `Reconnect ${connection.name} to the selected ${targetLabel}.`;
  }
  if (target) {
    return `Reconnect ${connection.name} so its ${targetLabel} can be verified.`;
  }
  return `Reconnect ${connection.name} to continue.`;
}
