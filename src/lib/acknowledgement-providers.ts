import { invoke, isTauri } from "@tauri-apps/api/core";
import type { AcknowledgementProvider, AcknowledgementProviderView, AcknowledgementProviderViews } from "./types";

export function providerKey(declaration: AcknowledgementProvider): string {
  return declaration.id + ":" + declaration.classification;
}

export async function readAcknowledgementProviderViews(
  tenantId: string,
  declarations: AcknowledgementProvider[]
): Promise<AcknowledgementProviderViews> {
  if (!isTauri() || !tenantId || declarations.length === 0) return {};
  const distinct = new Map(declarations.map((declaration) => [providerKey(declaration), declaration]));
  const views: AcknowledgementProviderViews = {};
  await Promise.all([...distinct].map(async ([key, declaration]) => {
    try {
      const view = await invoke<Omit<AcknowledgementProviderView, "tenantId"> | null>(
        "acknowledgement_provider_view", { tenantId, declaration }
      );
      if (view) views[key] = { ...view, tenantId: tenantId.trim().toLowerCase() };
    } catch {
      // Local evidence is optional. An unreadable result cannot be acknowledged.
    }
  }));
  return views;
}
