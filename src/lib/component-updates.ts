import { invoke, isTauri } from "@tauri-apps/api/core";
import type { ComponentProvenance } from "./component-provenance";

export interface ComponentUpdate {
  id: string;
  state: "unavailable" | "noNewer" | "updateAvailable";
  version: string | null;
  compatibility: "unknown" | "breaking" | "noBreakingDeclared";
  complete: boolean;
  historyIncomplete: boolean;
  breakingVersions: string[];
  migrationNotes: string[];
}

export interface ComponentUpdates {
  state: "fresh" | "stale" | "unavailable";
  checkedAt: string | null;
  updates: ComponentUpdate[];
}

export async function checkComponentUpdates(
  components: ReadonlyArray<Pick<ComponentProvenance, "id" | "version" | "sourceRepository" | "sourceRevision">> = [],
  force = false
): Promise<ComponentUpdates> {
  const unavailable: ComponentUpdates = { state: "unavailable", checkedAt: null, updates: [] };
  try {
    if (!isTauri()) return unavailable;
    return (await invoke<ComponentUpdates>("check_component_updates", {
      installed: components.map(({ id, version, sourceRepository, sourceRevision }) => ({ id, version, sourceRepository, sourceRevision })),
      force
    })) ?? unavailable;
  } catch {
    return unavailable;
  }
}
