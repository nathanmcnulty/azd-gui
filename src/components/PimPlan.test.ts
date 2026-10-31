import { cleanup, fireEvent, render, screen, waitFor } from '@testing-library/svelte';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import PimPlan from './PimPlan.svelte';
import * as api from '../lib/pim-plan';

const review: api.PimPlanReview = {
  sourceCommit: 'pinned-template', binding: {
    requirementsDigest: 'reviewed-config', account: 'operator@example.test',
    tenantId: '11111111-1111-4111-8111-111111111111', clientId: '22222222-2222-4222-8222-222222222222',
    scopes: ['AuthenticationContext.Read.All', 'Policy.Read.All', 'RoleManagement.Read.Directory']
  }, configuration: { privilegedRoleIds: ['33333333-3333-4333-8333-333333333333'], lessPrivilegedRoleIds: [],
    privilegedAuthenticationProfile: 'mfa', lessPrivilegedAuthenticationProfile: 'mfa',
    privilegedDeviceRequirement: 'none', lessPrivilegedDeviceRequirement: 'none' }
};
const artifact: api.PimPlanArtifact = { sourceCommit: review.sourceCommit, requirementsDigest: review.binding.requirementsDigest,
  plan: { mode: 'plan', generatedAt: '2026-10-30T00:00:00Z', warnings: ['Review the proposals'], tiers: {
    privileged: { roles: [{ id: 'role', displayName: '<script>fixture</script>' }],
      context: { action: 'create', displayName: 'PIM Privileged Roles' }, conditionalAccessPolicies: [{ action: 'create' }],
      roleRules: [{ action: 'update', role: { displayName: 'Fixture role' }, activationMfa: { action: 'remove' } }] }
  } } };

beforeEach(() => {
  vi.spyOn(api, 'cancelPimPlan').mockResolvedValue(undefined);
  vi.spyOn(api, 'reviewPimPlan').mockResolvedValue(structuredClone(review));
  vi.spyOn(api, 'connectPimPlan').mockResolvedValue(undefined);
  vi.spyOn(api, 'runPimPlan').mockResolvedValue(structuredClone(artifact));
});
afterEach(() => { cleanup(); vi.restoreAllMocks(); });

async function prepare() {
  render(PimPlan);
  await fireEvent.input(screen.getByLabelText('Account'), { target: { value: review.binding.account } });
  await fireEvent.input(screen.getByLabelText('Tenant ID'), { target: { value: review.binding.tenantId } });
  await fireEvent.input(screen.getByLabelText('Existing client ID'), { target: { value: review.binding.clientId } });
  await fireEvent.input(screen.getAllByLabelText('Role definition IDs')[0], { target: { value: review.configuration.privilegedRoleIds.join(',') } });
  await fireEvent.click(screen.getByRole('button', { name: 'Review configuration' }));
  await screen.findByRole('region', { name: 'PIM plan review' });
}
async function connect() {
  await fireEvent.click(screen.getByRole('button', { name: 'Approve sign-in' }));
  await waitFor(() => expect(screen.getByRole('button', { name: 'Generate read-only plan' })).toBeEnabled());
}
describe('PIM read-only workflow', () => {
  it('requires explicit sign-in and generation and displays the complete bound artifact', async () => {
    await prepare();
    expect(api.connectPimPlan).not.toHaveBeenCalled();
    expect(api.runPimPlan).not.toHaveBeenCalled();
    expect(screen.getByRole('button', { name: 'Generate read-only plan' })).toBeDisabled();
    await connect();
    expect(api.connectPimPlan).toHaveBeenCalledWith(expect.objectContaining({ account: review.binding.account, configuration: review.configuration }), review.binding.requirementsDigest);
    await fireEvent.click(screen.getByRole('button', { name: 'Generate read-only plan' }));
    await screen.findByRole('region', { name: 'PIM plan artifact' });
    expect(screen.getByText(/Fixture role: context rule update/)).toBeInTheDocument();
    expect(screen.getByText('Review the proposals', { exact: true })).toBeInTheDocument();
    expect(document.querySelector('pre')?.textContent).toContain('<script>fixture</script>');
    expect(document.querySelector('pre script')).toBeNull();
    await fireEvent.input(screen.getByLabelText('Account'), { target: { value: 'changed@example.test' } });
    expect(screen.queryByRole('region', { name: 'PIM plan artifact' })).not.toBeInTheDocument();
    expect(screen.queryByRole('region', { name: 'PIM plan review' })).not.toBeInTheDocument();
  });

  it.each(['cancel', 'account', 'tenant', 'client', 'roles', 'profile', 'device'])('discards late sign-in after %s', async change => {
    let resolve!: () => void;
    vi.mocked(api.connectPimPlan).mockImplementationOnce(() => new Promise<void>(done => { resolve = done; }));
    await prepare();
    await fireEvent.click(screen.getByRole('button', { name: 'Approve sign-in' }));
    await waitFor(() => expect(api.connectPimPlan).toHaveBeenCalledTimes(1));
    if (change === 'cancel') await fireEvent.click(screen.getByRole('button', { name: 'Cancel' }));
    else if (change === 'roles') await fireEvent.input(screen.getAllByLabelText('Role definition IDs')[0], { target: { value: 'other' } });
    else if (change === 'profile') await fireEvent.change(screen.getAllByLabelText('Authentication')[0], { target: { value: 'phishingResistant' } });
    else if (change === 'device') await fireEvent.change(screen.getAllByLabelText('Device requirement')[0], { target: { value: 'compliant' } });
    else await fireEvent.input(screen.getByLabelText(change === 'account' ? 'Account' : change === 'tenant' ? 'Tenant ID' : 'Existing client ID'), { target: { value: 'changed' } });
    resolve();
    await waitFor(() => expect(screen.queryByRole('region', { name: 'PIM plan review' })).not.toBeInTheDocument());
    expect(api.runPimPlan).not.toHaveBeenCalled();
  });

  it('discards a late plan after cancellation', async () => {
    let resolve!: (value: api.PimPlanArtifact) => void;
    vi.mocked(api.runPimPlan).mockImplementationOnce(() => new Promise(done => { resolve = done; }));
    await prepare(); await connect();
    await fireEvent.click(screen.getByRole('button', { name: 'Generate read-only plan' }));
    await waitFor(() => expect(api.runPimPlan).toHaveBeenCalledTimes(1));
    await fireEvent.click(screen.getByRole('button', { name: 'Cancel' }));
    resolve(artifact);
    await waitFor(() => expect(screen.queryByRole('region', { name: 'PIM plan artifact' })).not.toBeInTheDocument());
  });

  it('rejects an artifact from another configuration', async () => {
    vi.mocked(api.runPimPlan).mockResolvedValueOnce({ ...artifact, requirementsDigest: 'stale' });
    await prepare(); await connect();
    await fireEvent.click(screen.getByRole('button', { name: 'Generate read-only plan' }));
    await screen.findByRole('alert');
    expect(screen.queryByRole('region', { name: 'PIM plan artifact' })).not.toBeInTheDocument();
  });
});
