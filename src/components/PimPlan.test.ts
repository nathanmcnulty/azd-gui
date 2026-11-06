import { cleanup, fireEvent, render, screen, waitFor } from '@testing-library/svelte';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import PimPlan from './PimPlan.svelte';
import * as api from '../lib/pim-plan';
import { tick } from 'svelte';

const review: api.PimPlanReview = {
  sourceCommit: 'pinned-template', binding: {
    requirementsDigest: 'reviewed-config', account: 'operator@example.test',
    tenantId: '11111111-1111-4111-8111-111111111111', clientId: '22222222-2222-4222-8222-222222222222',
    scopes: ['AuthenticationContext.Read.All', 'Policy.Read.All', 'RoleManagement.Read.Directory']
  }, configuration: { previewExistingObjects: false, privilegedRoleIds: ['33333333-3333-4333-8333-333333333333'], lessPrivilegedRoleIds: [],
    privilegedAuthenticationProfile: 'mfa', lessPrivilegedAuthenticationProfile: 'mfa',
    privilegedDeviceRequirement: 'none', lessPrivilegedDeviceRequirement: 'none' }
};
const observed: api.PimProcessEvidence = { active: true, processId: 1234, sessionId: 'owned-session', context: {
  account: review.binding.account, tenantId: review.binding.tenantId, clientId: review.binding.clientId,
  resource: 'https://graph.microsoft.com', scopes: [...review.binding.scopes, 'User.Read'], principalFingerprint: 'a'.repeat(64),
  verificationReads: 1, pimReadsAttempted: 0, pimReadsCompleted: 0
} };
function cancellation(inFlight: boolean | null = true, processEvidence: api.PimProcessEvidence = { ...observed, active: false }, digest = review.binding.requirementsDigest): api.PimCancellationReceipt {
  return { requirementsDigest: digest, snapshotSource: inFlight === null ? 'historicalProcessEvidenceV1' : 'nativeRequestRevocationV1',
    pimPlanRequestInFlightAtRevocation: inFlight, processEvidence };
}
const artifact: api.PimPlanArtifact = { sourceCommit: review.sourceCommit, requirementsDigest: review.binding.requirementsDigest,
  configuration: review.configuration, previewExistingObjects: false, ownershipReviewRequired: true, ownershipEstablished: false, executionAuthorized: false,
  ownershipReviewNotice: 'Existing-object preview is off. Review ownership before any enforcement.',
  executionEvidence: { ...observed, context: { ...observed.context, pimReadsAttempted: 7, pimReadsCompleted: 7 } },
  plan: { mode: 'plan', previewExistingObjects: false, ownershipReviewRequired: true, ownershipEstablished: false, executionAuthorized: false,
    configuration: { Mode: 'plan', AdoptExisting: false, AdoptRoleContexts: false }, generatedAt: '2026-10-30T00:00:00Z', warnings: ['Review the proposals'], tiers: {
    privileged: { roles: [{ id: 'role', displayName: '<script>fixture</script>' }],
      context: { action: 'create', displayName: 'PIM Privileged Roles' }, conditionalAccessPolicies: [{ action: 'create' }],
      roleRules: [{ action: 'update', role: { displayName: 'Fixture role' }, activationMfa: { action: 'remove' } }] }
  } } };

beforeEach(() => {
  vi.spyOn(api, 'cancelPimPlan').mockResolvedValue(null);
  vi.spyOn(api, 'reviewPimPlan').mockResolvedValue(structuredClone(review));
  vi.spyOn(api, 'connectPimPlan').mockResolvedValue(structuredClone(observed));
  vi.spyOn(api, 'getPimPlanEvidence').mockResolvedValue(null);
  vi.spyOn(api, 'runPimPlan').mockResolvedValue(structuredClone(artifact));
});
afterEach(() => { cleanup(); vi.restoreAllMocks(); vi.unstubAllGlobals(); });

async function prepare(preview = false) {
  render(PimPlan);
  await fireEvent.input(screen.getByLabelText('Account'), { target: { value: review.binding.account } });
  await fireEvent.input(screen.getByLabelText('Tenant ID'), { target: { value: review.binding.tenantId } });
  await fireEvent.input(screen.getByLabelText('Existing client ID'), { target: { value: review.binding.clientId } });
  await fireEvent.input(screen.getAllByLabelText('Role definition IDs')[0], { target: { value: review.configuration.privilegedRoleIds.join(',') } });
  if (preview) await fireEvent.click(screen.getByRole('checkbox', { name: 'Preview changes to existing PIM objects (read-only)' }));
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
    expect(screen.getByText(/Actual granted scopes:/)).toHaveTextContent('User.Read');
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

  it.each(['cancel', 'account', 'tenant', 'client', 'roles', 'profile', 'device', 'preview'])('discards late sign-in after %s', async change => {
    let resolve!: (value: api.PimProcessEvidence) => void;
    vi.mocked(api.connectPimPlan).mockImplementationOnce(() => new Promise<api.PimProcessEvidence>(done => { resolve = done; }));
    await prepare();
    await fireEvent.click(screen.getByRole('button', { name: 'Approve sign-in' }));
    await waitFor(() => expect(api.connectPimPlan).toHaveBeenCalledTimes(1));
    if (change === 'cancel') await fireEvent.click(screen.getByRole('button', { name: 'Cancel' }));
    else if (change === 'roles') await fireEvent.input(screen.getAllByLabelText('Role definition IDs')[0], { target: { value: 'other' } });
    else if (change === 'profile') await fireEvent.change(screen.getAllByLabelText('Authentication')[0], { target: { value: 'phishingResistant' } });
    else if (change === 'device') await fireEvent.change(screen.getAllByLabelText('Device requirement')[0], { target: { value: 'compliant' } });
    else if (change === 'preview') await fireEvent.click(screen.getByRole('checkbox', { name: 'Preview changes to existing PIM objects (read-only)' }));
    else await fireEvent.input(screen.getByLabelText(change === 'account' ? 'Account' : change === 'tenant' ? 'Tenant ID' : 'Existing client ID'), { target: { value: 'changed' } });
    resolve(observed);
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

  it('defaults to strict planning and requires a fresh review when preview changes', async () => {
    await prepare();
    expect(screen.getByRole('checkbox', { name: 'Preview changes to existing PIM objects (read-only)' })).not.toBeChecked();
    expect(api.reviewPimPlan).toHaveBeenCalledWith(expect.objectContaining({ configuration: expect.objectContaining({ previewExistingObjects: false }) }));
    await connect();
    await fireEvent.click(screen.getByRole('button', { name: 'Generate read-only plan' }));
    await screen.findByRole('region', { name: 'PIM plan artifact' });
    await fireEvent.click(screen.getByRole('checkbox', { name: 'Preview changes to existing PIM objects (read-only)' }));
    expect(screen.queryByRole('region', { name: 'PIM plan artifact' })).not.toBeInTheDocument();
    expect(screen.queryByRole('region', { name: 'PIM plan review' })).not.toBeInTheDocument();
    await waitFor(() => expect(api.cancelPimPlan).toHaveBeenCalledWith(review.binding.requirementsDigest));
  });

  it('labels existing-object proposals and preserves the ownership warning in complete JSON', async () => {
    const previewReview = structuredClone(review);
    previewReview.configuration.previewExistingObjects = true;
    const previewArtifact = structuredClone(artifact);
    previewArtifact.configuration.previewExistingObjects = true;
    previewArtifact.previewExistingObjects = true;
    previewArtifact.ownershipReviewNotice = 'Preview proposes updates only; ownership is not established.';
    previewArtifact.plan.previewExistingObjects = true;
    previewArtifact.plan.configuration.AdoptExisting = true;
    previewArtifact.plan.configuration.AdoptRoleContexts = true;
    previewArtifact.plan.tiers.privileged.context = { id: 'c1', displayName: 'Existing context', action: 'adoptAndUpdate', adopted: true, existing: { description: 'Existing description' } };
    previewArtifact.plan.tiers.privileged.conditionalAccessPolicies = [{ id: 'existing-policy', action: 'adoptAndUpdate', body: { displayName: 'Existing policy' }, existing: { state: 'disabled' } }];
    vi.mocked(api.reviewPimPlan).mockResolvedValueOnce(previewReview);
    vi.mocked(api.runPimPlan).mockResolvedValueOnce(previewArtifact);
    await prepare(true); await connect();
    expect(api.connectPimPlan).toHaveBeenCalledWith(expect.objectContaining({ configuration: expect.objectContaining({ previewExistingObjects: true }) }), review.binding.requirementsDigest);
    await fireEvent.click(screen.getByRole('button', { name: 'Generate read-only plan' }));
    await screen.findByRole('region', { name: 'PIM plan artifact' });
    expect(screen.getByText('Ownership review is required. This artifact establishes no ownership and authorizes no execution.')).toBeInTheDocument();
    expect(screen.getByText(/Existing context \(c1\)/)).toHaveTextContent('proposed update — ownership not established');
    expect(screen.getByText(/Existing policy \(existing-policy\)/)).toHaveTextContent('proposed update — ownership not established');
    expect(document.querySelector('pre')?.textContent).toContain('"ownershipEstablished": false');
    expect(document.querySelector('pre')?.textContent).toContain('Existing description');
  });

  it.each(['preview', 'configuration', 'plan', 'ownership', 'execution', 'plannerFlags'])('rejects a mismatched or unsafe %s marker', async change => {
    const invalid = structuredClone(artifact);
    if (change === 'preview') invalid.previewExistingObjects = true;
    else if (change === 'configuration') invalid.configuration.previewExistingObjects = true;
    else if (change === 'plan') invalid.plan.previewExistingObjects = true;
    else if (change === 'ownership') invalid.ownershipEstablished = true;
    else if (change === 'execution') invalid.executionAuthorized = true;
    else invalid.plan.configuration.AdoptExisting = true;
    vi.mocked(api.runPimPlan).mockResolvedValueOnce(invalid);
    await prepare(); await connect();
    await fireEvent.click(screen.getByRole('button', { name: 'Generate read-only plan' }));
    await screen.findByRole('alert');
    expect(screen.queryByRole('region', { name: 'PIM plan artifact' })).not.toBeInTheDocument();
  });

  it('discards a late strict plan after enabling existing-object preview', async () => {
    let finish!: (value: api.PimPlanArtifact) => void;
    vi.mocked(api.runPimPlan).mockImplementationOnce(() => new Promise(done => { finish = done; }));
    await prepare(); await connect();
    await fireEvent.click(screen.getByRole('button', { name: 'Generate read-only plan' }));
    await waitFor(() => expect(api.runPimPlan).toHaveBeenCalledTimes(1));
    await fireEvent.click(screen.getByRole('checkbox', { name: 'Preview changes to existing PIM objects (read-only)' }));
    finish(artifact); await Promise.resolve(); await tick();
    expect(screen.queryByRole('region', { name: 'PIM plan artifact' })).not.toBeInTheDocument();
  });

  it('shows cached in-flight read evidence without another cloud read and clears it on cancellation', async () => {
    vi.mocked(api.getPimPlanEvidence).mockResolvedValue({ ...observed, context: { ...observed.context, pimReadsAttempted: 1, pimReadsCompleted: 0 } });
    let resolve!: (value: api.PimPlanArtifact) => void;
    vi.mocked(api.runPimPlan).mockImplementationOnce(() => new Promise(done => { resolve = done; }));
    await prepare(); await connect();
    await fireEvent.click(screen.getByRole('button', { name: 'Generate read-only plan' }));
    await screen.findByText('Protected plan reads: 1 started, 0 completed.');
    expect(api.getPimPlanEvidence).toHaveBeenCalledWith(review.binding.requirementsDigest);
    await fireEvent.click(screen.getByRole('button', { name: 'Cancel' }));
    resolve(artifact);
    await waitFor(() => expect(screen.queryByText(/Protected plan reads:/)).not.toBeInTheDocument());
    expect(api.connectPimPlan).toHaveBeenCalledTimes(1);
  });

  it('copies the complete artifact including observed evidence only on explicit request', async () => {
    const writeText = vi.fn().mockResolvedValue(undefined);
    vi.stubGlobal('navigator', Object.assign(Object.create(navigator), { clipboard: { writeText } }));
    await prepare(); await connect();
    await fireEvent.click(screen.getByRole('button', { name: 'Generate read-only plan' }));
    await screen.findByRole('region', { name: 'PIM plan artifact' });
    expect(writeText).not.toHaveBeenCalled();
    await fireEvent.click(screen.getByRole('button', { name: 'Copy complete plan JSON' }));
    expect(JSON.parse(writeText.mock.calls[0][0])).toEqual(artifact);
  });

  it('requires a fresh review after failure and accepts its newly owned process', async () => {
    vi.mocked(api.runPimPlan).mockRejectedValueOnce(new Error('Graph read denied'));
    vi.mocked(api.cancelPimPlan).mockResolvedValueOnce(cancellation(null, { ...observed, active: false, context: { ...observed.context, pimReadsAttempted: 1 } }));
    await prepare(); await connect();
    await fireEvent.click(screen.getByRole('button', { name: 'Generate read-only plan' }));
    await screen.findByRole('alert');
    await screen.findByText('Historical failure receipt; this process cannot authorize more reads.');
    expect(screen.queryByRole('button', { name: 'Approve sign-in' })).not.toBeInTheDocument();
    expect(api.connectPimPlan).toHaveBeenCalledTimes(1);
    await fireEvent.click(screen.getByRole('button', { name: 'Review configuration' }));
    await screen.findByRole('region', { name: 'PIM plan review' });
    vi.mocked(api.connectPimPlan).mockResolvedValueOnce({ ...observed, processId: 5678, sessionId: 'replacement-session' });
    await connect();
    expect(api.connectPimPlan).toHaveBeenCalledTimes(2);
    expect(screen.getByText(/Worker PID: 5678/)).toBeInTheDocument();
  });

  it('disposes a newly authenticated process if its returned identity cannot be accepted', async () => {
    vi.mocked(api.connectPimPlan).mockResolvedValueOnce({ ...observed, context: { ...observed.context, account: 'other@example.test' } });
    await prepare();
    await fireEvent.click(screen.getByRole('button', { name: 'Approve sign-in' }));
    await screen.findByRole('alert');
    await waitFor(() => expect(api.cancelPimPlan).toHaveBeenCalledWith(review.binding.requirementsDigest));
    expect(screen.queryByRole('button', { name: 'Approve sign-in' })).not.toBeInTheDocument();
  });

  it('cannot restore an active receipt from a poll that completes after terminal failure cleanup', async () => {
    let finishPoll!: (value: api.PimProcessEvidence) => void;
    let failOperation!: (cause: Error) => void;
    vi.mocked(api.getPimPlanEvidence).mockImplementationOnce(() => new Promise(done => { finishPoll = done; }));
    vi.mocked(api.runPimPlan).mockImplementationOnce(() => new Promise((_, reject) => { failOperation = reject; }));
    vi.mocked(api.cancelPimPlan).mockResolvedValueOnce(cancellation(null));
    await prepare(); await connect();
    await fireEvent.click(screen.getByRole('button', { name: 'Generate read-only plan' }));
    await waitFor(() => expect(api.getPimPlanEvidence).toHaveBeenCalledTimes(1));
    failOperation(new Error('Graph read denied'));
    await screen.findByText('Historical failure receipt; this process cannot authorize more reads.');
    finishPoll(observed);
    await Promise.resolve(); await tick();
    await waitFor(() => expect(screen.getByText('Historical failure receipt; this process cannot authorize more reads.')).toBeInTheDocument());
    expect(screen.queryByText('The native session remains available for its registered reads.')).not.toBeInTheDocument();
    expect(api.connectPimPlan).toHaveBeenCalledTimes(1);
  });

  it('retains an explicitly requested cancellation receipt while discarding the late plan', async () => {
    const receipt = cancellation(true, { ...observed, active: false, context: { ...observed.context, pimReadsAttempted: 1 } });
    vi.mocked(api.cancelPimPlan).mockResolvedValueOnce(receipt);
    let finish!: (value: api.PimPlanArtifact) => void;
    vi.mocked(api.runPimPlan).mockImplementationOnce(() => new Promise(done => { finish = done; }));
    await prepare(); await connect();
    await fireEvent.click(screen.getByRole('button', { name: 'Generate read-only plan' }));
    await fireEvent.click(screen.getByRole('button', { name: 'Cancel' }));
    await screen.findByRole('region', { name: 'PIM cancellation receipt' });
    expect(screen.getByText(/Process metadata at cancellation processing: 1 plan dispatch attempts, 0 validated completions/)).toBeInTheDocument();
    expect(screen.getByText('Native planner request was in flight when this cancellation revoked the session.')).toBeInTheDocument();
    expect(screen.getByText(review.binding.requirementsDigest)).toBeInTheDocument();
    finish(artifact); await Promise.resolve(); await tick();
    expect(screen.queryByRole('region', { name: 'PIM plan artifact' })).not.toBeInTheDocument();
    await fireEvent.input(screen.getByLabelText('Account'), { target: { value: 'changed@example.test' } });
    expect(screen.queryByRole('region', { name: 'PIM cancellation receipt' })).not.toBeInTheDocument();
  });

  it.each([false, null])('distinguishes completed/unknown cancellation snapshot %s from in-flight evidence', async inFlight => {
    vi.mocked(api.cancelPimPlan).mockResolvedValueOnce(cancellation(inFlight, { ...observed, active: false, context: { ...observed.context, pimReadsAttempted: 7, pimReadsCompleted: 7 } }));
    await prepare(); await connect();
    await fireEvent.click(screen.getByRole('button', { name: 'Generate read-only plan' }));
    await screen.findByRole('region', { name: 'PIM plan artifact' });
    await fireEvent.click(screen.getByRole('button', { name: 'Disconnect and clear plan' }));
    await screen.findByRole('region', { name: 'PIM cancellation receipt' });
    expect(screen.getByText(inFlight === false ? 'No native planner request was in flight when this cancellation revoked the session.' : 'This action has no fresh native in-flight cancellation snapshot; only historical process evidence is available.')).toBeInTheDocument();
    expect(screen.queryByText('Native planner request was in flight when this cancellation revoked the session.')).not.toBeInTheDocument();
  });

  it('copies the native cancellation digest and snapshot only on explicit request', async () => {
    const receipt = cancellation(false);
    vi.mocked(api.cancelPimPlan).mockResolvedValueOnce(receipt);
    const writeText = vi.fn().mockResolvedValue(undefined);
    vi.stubGlobal('navigator', Object.assign(Object.create(navigator), { clipboard: { writeText } }));
    await prepare(); await connect();
    await fireEvent.click(screen.getByRole('button', { name: 'Disconnect and clear plan' }));
    await screen.findByRole('region', { name: 'PIM cancellation receipt' });
    expect(writeText).not.toHaveBeenCalled();
    await fireEvent.click(screen.getByRole('button', { name: 'Copy cancellation receipt' }));
    expect(JSON.parse(writeText.mock.calls[0][0])).toEqual(receipt);
  });

  it.each(['digest', 'legacy', 'historicalTrue'])('rejects cancellation evidence with invalid %s correlation', async kind => {
    let receipt = cancellation(true);
    if (kind === 'digest') receipt.requirementsDigest = 'another-review';
    else if (kind === 'historicalTrue') receipt.snapshotSource = 'historicalProcessEvidenceV1';
    else receipt = { ...observed, active: false } as unknown as api.PimCancellationReceipt;
    vi.mocked(api.cancelPimPlan).mockResolvedValueOnce(receipt);
    await prepare(); await connect();
    await fireEvent.click(screen.getByRole('button', { name: 'Disconnect and clear plan' }));
    await screen.findByRole('alert');
    expect(screen.queryByRole('region', { name: 'PIM cancellation receipt' })).not.toBeInTheDocument();
  });
});
