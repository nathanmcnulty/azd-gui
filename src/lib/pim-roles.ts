import { invoke, isTauri } from '@tauri-apps/api/core';
export interface PimRole { id: string; displayName: string; isBuiltIn: boolean | null; isEnabled: boolean | null; isPrivileged: boolean | null }
export interface PimRoleCatalog { tenantId: string; retrievedAt: string; source: 'live' | 'cached'; roles: PimRole[] }
export const roleIds = (value: string) => [...new Set(value.split(/[\s,;]+/).filter(Boolean).map(id => id.toLowerCase()))].sort();
export function selectedRoleProblems(catalog: PimRoleCatalog, privileged: string, less: string): string[] {
  return [[privileged, true], [less, false]].flatMap(([text, tier]) => roleIds(text as string).flatMap(id => {
    const role = catalog.roles.find(role => role.id === id);
    if (!role) return [`${id}: missing from this tenant. Remove it or correct the manual ID.`];
    if (role.isEnabled !== true) return [`${role.displayName}: disabled or availability unknown.`];
    if (role.isPrivileged === null) return [`${role.displayName}: privilege classification unknown.`];
    if (role.isPrivileged !== tier) return [`${role.displayName}: classification changed or selected in the wrong tier. Review and move it explicitly.`];
    return [];
  }));
}
export function roleChanges(previous: Record<string, PimRole>, catalog: PimRoleCatalog, ids: string[]): string[] {
  return ids.flatMap(id => {
    const old = previous[id], role = catalog.roles.find(role => role.id === id);
    return old && role && (['displayName', 'isEnabled', 'isBuiltIn', 'isPrivileged'] as const).some(key => old[key] !== role[key]) ? [`${old.displayName} (${id}) changed to ${role.displayName}; review its classification, availability and role type.`] : [];
  });
}
export async function cachedPimRoles(tenantId: string): Promise<PimRoleCatalog | null> {
  return isTauri() ? invoke('cached_pim_roles', { tenantId }) : null;
}
export async function refreshPimRoles(expectedDigest: string): Promise<PimRoleCatalog> {
  if (!isTauri()) throw new Error('Open the Windows desktop app to refresh tenant roles.');
  return invoke('refresh_pim_roles', { expectedDigest });
}
