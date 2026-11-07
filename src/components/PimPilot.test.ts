import { cleanup, fireEvent, render, screen, waitFor } from '@testing-library/svelte';
import { afterEach, beforeEach, expect, it, vi } from 'vitest';
import { invoke } from '@tauri-apps/api/core';
import * as planApi from '../lib/pim-plan';
import PimPilot from './PimPilot.svelte';

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn(), isTauri: () => true }));
const input = { reviewId: 'review', account: 'operator@example.test', tenantId: 'tenant-id', clientId: 'client', configuration: {
  previewExistingObjects: true, privilegedRoleIds: ['role'], lessPrivilegedRoleIds: [],
  privilegedAuthenticationProfile: 'mfa' as const, lessPrivilegedAuthenticationProfile: 'mfa' as const,
  privilegedDeviceRequirement: 'none' as const, lessPrivilegedDeviceRequirement: 'none' as const,
} };
const review = { digest: 'exact-live-review', status: 'reviewRequired', state: { pendingRoleRules: {} }, recoveryBlockers: [],
  plan: { warnings: [], tiers: { privileged: { roles: [{ id: 'role', displayName: '<script>role</script>' }], context: null,
    conditionalAccessPolicies: [{ id: 'policy-id', action: 'adoptAndUpdate', existing: { state: 'disabled' }, body: { state: 'enabled' } }],
    roleRules: [{ action: 'update', role: { displayName: 'Test role' }, policyId: 'role-policy-id', existing: { claimValue: 'c9' }, desiredContextId: 'c1', activationMfa: { action: 'remove', existing: { enabledRules: ['MultiFactorAuthentication', 'Justification'] }, desiredEnabledRules: ['Justification'] } }],
  } } } };
beforeEach(() => {
  vi.spyOn(planApi, 'cancelPimPlan').mockResolvedValue(null);
  vi.mocked(invoke).mockImplementation(async (command) => command === 'review_pim_pilot' ? { requirementsDigest: 'binding' } : structuredClone(review));
});
afterEach(() => { cleanup(); vi.restoreAllMocks(); vi.mocked(invoke).mockReset(); });
async function connect() {
  const view = render(PimPilot, { input, projects: [{ path: 'C:/pilot', name: 'Pilot' }] });
  await fireEvent.change(screen.getByLabelText('Project'), { target: { value: 'C:/pilot' } });
  await fireEvent.click(screen.getByText('Review sign-in and recheck pilot'));
  await screen.findByText(/Review exact-live-review/);
  return view;
}
it('shows exact IDs, before/after and MFA replacement and requires fresh explicit approval', async () => {
  await connect();
  expect(document.querySelector('script')).toBeNull();
  expect(screen.getByText(/Conditional Access policy-id/)).toBeInTheDocument();
  expect(screen.getByText(/Justification and ticketing are preserved/)).toHaveTextContent('MultiFactorAuthentication');
  const apply = screen.getByText('Approve application / resume');
  expect(apply).toBeDisabled();
  await fireEvent.click(screen.getByRole('checkbox'));
  await fireEvent.input(screen.getByLabelText('Type tenant ID'), { target: { value: input.tenantId } });
  await fireEvent.click(apply);
  await waitFor(() => expect(invoke).toHaveBeenCalledWith('run_pim_pilot', expect.objectContaining({ expectedDigest: 'binding', approvedDigest: 'exact-live-review' })));
  expect(apply).toBeDisabled();
});
it('invalidates approval and revokes the worker when identity or configuration changes', async () => {
  const view = await connect();
  await view.rerender({ input: { ...input, tenantId: 'different-tenant' } });
  await waitFor(() => expect(planApi.cancelPimPlan).toHaveBeenCalledWith('binding'));
  expect(screen.queryByText('Approve application / resume')).toBeNull();
});
it('blocks resume for unresolved writes outside the selected scope', async () => {
  vi.mocked(invoke).mockImplementation(async command => command === 'review_pim_pilot' ? { requirementsDigest: 'binding' } : { ...structuredClone(review), recoveryBlockers: ['Restore omitted pending role'] });
  await connect();
  await fireEvent.click(screen.getByRole('checkbox'));
  await fireEvent.input(screen.getByLabelText('Type tenant ID'), { target: { value: input.tenantId } });
  expect(screen.getByText('Restore omitted pending role')).toBeInTheDocument();
  expect(screen.getByText('Approve application / resume')).toBeDisabled();
});
it('discards a late worker reply after cancellation', async () => {
  let finish!: (value: unknown) => void;
  vi.mocked(invoke).mockImplementation(async command => command === 'review_pim_pilot' ? { requirementsDigest: 'binding' } : new Promise(resolve => { finish = resolve; }));
  render(PimPilot, { input, projects: [{ path: 'C:/pilot', name: 'Pilot' }] });
  await fireEvent.change(screen.getByLabelText('Project'), { target: { value: 'C:/pilot' } });
  await fireEvent.click(screen.getByText('Review sign-in and recheck pilot'));
  await waitFor(() => expect(finish).toBeDefined());
  await fireEvent.click(screen.getByText('Cancel pilot session'));
  finish(review);
  await waitFor(() => expect(planApi.cancelPimPlan).toHaveBeenCalledWith('binding'));
  expect(screen.queryByText(/Review exact-live-review/)).toBeNull();
});

it('saves a local draft without connecting or granting application authority', async () => {
 vi.mocked(invoke).mockImplementation(async command => command==='inspect_pim_pilot' ? {hasDraft:false,attemptCount:0,pendingWrites:0} : null);
 render(PimPilot,{input,selectedProject:{path:'C:/pilot',environment:'recovery'}});
 await fireEvent.click(screen.getByText('Save pilot configuration locally'));
 await waitFor(()=>expect(invoke).toHaveBeenCalledWith('save_pim_pilot_draft',{input:{projectPath:'C:/pilot',environment:'recovery',input}}));
 expect(vi.mocked(invoke).mock.calls.some(([command])=>/connect|run_pim|review_pim/.test(command))).toBe(false);
 expect(screen.queryByText('Approve application / resume')).toBeNull();
});
it('persists the selected environment after reactive invalidation without connecting', async () => {
 vi.mocked(invoke).mockImplementation(async command => command==='inspect_pim_pilot' ? {hasDraft:false,attemptCount:0,pendingWrites:0} : null);
 render(PimPilot,{input,projects:[{path:'C:/pilot',name:'Pilot'}]});
 await fireEvent.change(screen.getByLabelText('Project'),{target:{value:'C:/pilot'}});
 await fireEvent.input(screen.getByLabelText('Environment'),{target:{value:'recovery'}});
 await fireEvent.change(screen.getByLabelText('Environment'));
 await waitFor(()=>expect(invoke).toHaveBeenCalledWith('select_pim_environment',{projectPath:'C:/pilot',environment:'recovery'}));
 expect(vi.mocked(invoke).mock.calls.some(([command])=>/connect|run_pim|review_pim/.test(command))).toBe(false);
});
