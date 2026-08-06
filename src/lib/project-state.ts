import type { AzdProjectState } from "./types";

export interface ProjectServiceResult {
  name: string;
  endpoint?: string;
  language?: string;
  projectPath?: string;
  resources: ProjectResourceResult[];
}

export interface ProjectResourceResult {
  id: string;
  portalUrl: string;
  name: string;
  type: string;
  resourceGroup: string;
}

function record(value: unknown): Record<string, unknown> | null {
  return value !== null && typeof value === "object" && !Array.isArray(value)
    ? (value as Record<string, unknown>)
    : null;
}

function text(value: unknown): string | undefined {
  return typeof value === "string" && value.trim() ? value.trim() : undefined;
}

function httpsUrl(value: unknown): string | undefined {
  const candidate = text(value);
  if (!candidate) return undefined;
  try {
    const parsed = new URL(candidate);
    return parsed.protocol === "https:" ? parsed.toString() : undefined;
  } catch {
    return undefined;
  }
}

export function azurePortalUrl(resourceId: string): string | null {
  const candidate = resourceId.trim();
  if (
    !/^\/subscriptions\/[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}\/resourceGroups\/[^/#?]+(?:\/providers\/[^#?]+)?$/i.test(
      candidate
    )
  ) {
    return null;
  }
  return `https://portal.azure.com/#resource${encodeURI(candidate)}`;
}

function projectResource(resourceId: string): ProjectResourceResult | null {
  const portalUrl = azurePortalUrl(resourceId);
  if (!portalUrl) return null;
  const segments = resourceId.split("/").filter(Boolean);
  const providersIndex = segments.findIndex((segment) => segment.toLowerCase() === "providers");
  const provider = providersIndex >= 0 ? segments[providersIndex + 1] : undefined;
  const resourceSegments = providersIndex >= 0 ? segments.slice(providersIndex + 2) : [];
  const typeSegments = resourceSegments.filter((_, index) => index % 2 === 0);
  const nameSegments = resourceSegments.filter((_, index) => index % 2 === 1);

  return {
    id: resourceId,
    portalUrl,
    name: nameSegments.at(-1) ?? segments.at(-1) ?? resourceId,
    type: provider ? [provider, ...typeSegments].join("/") : "Resource group",
    resourceGroup: segments[3] ?? ""
  };
}

export function projectServices(state: AzdProjectState | null | undefined): ProjectServiceResult[] {
  const services = record(state?.services);
  if (!services) return [];

  return Object.entries(services)
    .flatMap(([name, value]) => {
      const service = record(value);
      if (!service) return [];
      const project = record(service.project);
      const target = record(service.target);
      const resourceIds = Array.isArray(target?.resourceIds) ? target.resourceIds : [];
      const resources = resourceIds.flatMap((value) => {
        const id = text(value);
        if (!id) return [];
        const resource = projectResource(id);
        return resource ? [resource] : [];
      });

      return [{
        name,
        endpoint:
          httpsUrl(service.ingressUrl) ??
          httpsUrl(service.ingresUrl) ??
          httpsUrl(service.endpoint),
        language: text(project?.language),
        projectPath: text(project?.path),
        resources
      }];
    })
    .sort((left, right) => left.name.localeCompare(right.name));
}
