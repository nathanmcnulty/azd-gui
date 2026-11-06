import { describe, expect, it } from 'vitest';
import { roleIds, roleChanges, selectedRoleProblems, type PimRole, type PimRoleCatalog } from './pim-roles';
const role: PimRole = { id: 'role', displayName: 'Custom pilot', isBuiltIn: false, isEnabled: true, isPrivileged: true };
const catalog: PimRoleCatalog = { tenantId: 'tenant', source: 'live', retrievedAt: '2026-11-05T00:00:00Z', roles: [role] };
describe('selected role revalidation', () => {
  it('normalizes deterministic stable IDs without expanding an empty tier', () => {
    expect(roleIds(' B; a\nA,b ')).toEqual(['a', 'b']);
    expect(selectedRoleProblems(catalog, '', '')).toEqual([]);
    expect(selectedRoleProblems(catalog, 'role', '')).toEqual([]);
  });
  it.each([null, false])('never treats unknown or changed classification %s as privileged', classification => {
    expect(selectedRoleProblems({ ...catalog, roles: [{ ...role, isPrivileged: classification }] }, 'role', '')).toHaveLength(1);
  });
  it('rejects unknown classification in the less privileged tier too', () => {
    expect(selectedRoleProblems({ ...catalog, roles: [{ ...role, isPrivileged: null }] }, '', 'role')[0]).toContain('unknown');
  });
  it('identifies removed and disabled saved roles', () => {
    expect(selectedRoleProblems({ ...catalog, roles: [] }, 'role', '')[0]).toContain('missing');
    expect(selectedRoleProblems({ ...catalog, roles: [{ ...role, isEnabled: false }] }, 'role', '')[0]).toContain('disabled');
  });
  it.each(['displayName', 'isPrivileged', 'isBuiltIn', 'isEnabled'] as const)('requires explicit review of changed %s', key => {
    const changed = { ...role, [key]: key === 'displayName' ? 'Renamed role' : !role[key] };
    expect(roleChanges({ role }, { ...catalog, roles: [changed] }, ['role'])).toHaveLength(1);
    expect(roleChanges({ role }, { ...catalog, roles: [changed] }, [])).toEqual([]);
  });
});
