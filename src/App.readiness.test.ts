import { act, cleanup, fireEvent, render, screen, waitFor, within } from '@testing-library/svelte';
import { afterEach, expect, it, vi } from 'vitest';
import App from './App.svelte';
import * as bridge from './lib/bridge';
import type { ToolStatus } from './lib/types';

afterEach(() => { cleanup(); vi.restoreAllMocks(); window.localStorage.clear(); });
const tenantId = '72f988bf-86f1-41af-91ab-2d7cd011db47';
const subscriptionId = '11111111-1111-1111-1111-111111111111';
const otherSubscription = '22222222-2222-2222-2222-222222222222';

async function next() {
  const button = screen.getByRole('button', { name: /^continue$/i });
  await waitFor(() => expect(button).toBeEnabled());
  await fireEvent.click(button);
}
async function features() {
  render(App);
  await fireEvent.click(screen.getByRole('button', { name: /browse catalog/i }));
  await fireEvent.click(screen.getAllByRole('button', { name: /configure deployment/i })[1]);
  await fireEvent.input(screen.getByPlaceholderText(/empty destination folder/i), { target: { value: 'C:\\Deployments\\readiness-fixture' } });
  await next(); await next();
  await screen.findByRole('heading', { name: 'Maester coverage' });
}
async function environment() {
  await features();
  await fireEvent.click(screen.getByRole('button', { name: /apply minimal pilot/i }));
  await next(); await next(); await next();
  await screen.findByRole('heading', { name: 'Azure environment' });
  await fireEvent.click(screen.getByRole('button', { name: 'Enter IDs manually' }));
}

it('uses the bundled pin and removes Exchange for Minimal while retaining grant and terminal disclosures', async () => {
  const data = await bridge.bootstrap();
  expect(data.catalog.entries.find(entry => entry.id === 'maester-automation')?.commit).toBe('1cbaf9064a6ac4c61dd175bf2f61ca08367de0ff');
  const login = vi.spyOn(bridge, 'login'); const prepare = vi.spyOn(bridge, 'prepareDeployment');
  await features();
  expect(screen.getByText('ExchangeOnlineManagement')).toBeInTheDocument();
  expect(screen.getByText(/for Include Exchange checks = true/)).toBeInTheDocument();
  await fireEvent.click(screen.getByRole('button', { name: /apply minimal pilot/i }));
  await waitFor(() => expect(screen.queryByText('ExchangeOnlineManagement')).not.toBeInTheDocument());
  expect(screen.queryByText('Microsoft Graph PowerShell')).not.toBeInTheDocument();
  expect(screen.getByText(/Minimal profile's eight baseline managed-identity Graph application read roles/)).toBeInTheDocument();
  expect(screen.getByText('Terminal handoff required.')).toBeInTheDocument();
  expect(screen.getByText(/deployed identity’s grants are separate/)).toBeInTheDocument();
  await fireEvent.click(screen.getByRole('checkbox', { name: /include exchange checks/i }));
  expect(await screen.findByText('ExchangeOnlineManagement')).toBeInTheDocument();
  expect(screen.getByText(/for Include Exchange checks = true/)).toBeInTheDocument();
  await fireEvent.click(screen.getByRole('checkbox', { name: /include exchange checks/i }));
  await waitFor(() => expect(screen.queryByText('ExchangeOnlineManagement')).not.toBeInTheDocument());
  expect(login).not.toHaveBeenCalled(); expect(prepare).not.toHaveBeenCalled();
});

function deferred<T>() {
  let resolve!: (value: T) => void;
  let reject!: (reason: Error) => void;
  const promise = new Promise<T>((yes, no) => { resolve = yes; reject = no; });
  return { promise, resolve, reject };
}

it.each(['success', 'error'] as const)('ignores an old A tool %s after A → B → A and a completed new A check', async outcome => {
  vi.spyOn(bridge, 'loadWizardDraft').mockResolvedValue({ tenantId, subscriptionId });
  const check = bridge.checkRequirementPrerequisites;
  const probe = vi.spyOn(bridge, 'checkRequirementPrerequisites').mockImplementation(check);
  await environment();
  await screen.findByText(/Verified for target/);
  await waitFor(() => expect(screen.getByText('Azure CLI').parentElement).toHaveTextContent('Detected'));
  const statuses = (await probe.mock.results.at(-1)!.value) as ToolStatus[];
  const ready = statuses.map(tool => ({ ...tool, state: 'ready' as const }));
  const missing = ready.map(tool => tool.id === 'az-cli' ? { ...tool, state: 'missing' as const } : tool);
  const oldA = deferred<ToolStatus[]>(); const b = deferred<ToolStatus[]>(); const newA = deferred<ToolStatus[]>();
  probe.mockImplementationOnce(() => oldA.promise).mockImplementationOnce(() => b.promise).mockImplementationOnce(() => newA.promise);
  const count = probe.mock.calls.length;
  await fireEvent.click(screen.getByRole('button', { name: 'Check tools again' }));
  await waitFor(() => expect(probe).toHaveBeenCalledTimes(count + 1));
  await fireEvent.input(screen.getByPlaceholderText('Subscription ID'), { target: { value: otherSubscription } });
  await waitFor(() => expect(probe).toHaveBeenCalledTimes(count + 2));
  await fireEvent.input(screen.getByPlaceholderText('Subscription ID'), { target: { value: subscriptionId } });
  await waitFor(() => expect(probe).toHaveBeenCalledTimes(count + 3));
  await act(() => newA.resolve(outcome === 'success' ? missing : ready));
  expect(probe).toHaveBeenCalledTimes(count + 3);
  const cli = () => screen.getByText('Azure CLI').parentElement!;
  const expected = outcome === 'success' ? 'Not installed or not found' : 'Detected';
  await waitFor(() => expect(cli()).toHaveTextContent(expected));
  await act(() => outcome === 'success' ? oldA.resolve(ready) : oldA.reject(new Error('obsolete A failure')));
  expect(cli()).toHaveTextContent(expected);
  expect(cli()).not.toHaveTextContent('Detection could not be verified');
  await act(() => b.resolve(ready));
  expect(cli()).toHaveTextContent(expected);
});

it.each(['account', 'derived tenant', 'provider cloud'] as const)('invalidates pending tool readiness when the %s changes', async change => {
  const connections = await bridge.refreshConnections();
  const refresh = vi.spyOn(bridge, 'refreshConnections').mockResolvedValue(connections);
  const check = bridge.checkRequirementPrerequisites;
  const probe = vi.spyOn(bridge, 'checkRequirementPrerequisites').mockImplementation(check);
  await features();
  await waitFor(() => expect(screen.getByText('Azure CLI').parentElement).toHaveTextContent('Detected'));
  const statuses = await probe.mock.results.at(-1)!.value;
  const old = deferred<ToolStatus[]>(); const current = deferred<ToolStatus[]>();
  probe.mockImplementationOnce(() => old.promise).mockImplementationOnce(() => current.promise);
  const count = probe.mock.calls.length;
  await fireEvent.click(screen.getByRole('button', { name: 'Check tools again' }));
  await waitFor(() => expect(probe).toHaveBeenCalledTimes(count + 1));
  refresh.mockResolvedValue(connections.map(connection => ({ ...connection,
    ...(change === 'account' ? { account: 'new-operator@example.test' } : change === 'provider cloud' ? { azureCloud: 'AzureUSGovernment' } : { tenantId: '22222222-2222-4222-8222-222222222222' }) })));
  await fireEvent.click(screen.getByRole('button', { name: 'Check accounts again' }));
  await waitFor(() => expect(probe).toHaveBeenCalledTimes(count + 2));
  await act(() => old.reject(new Error('old identity failure')));
  expect(screen.queryByText(/Detection could not be verified/)).not.toBeInTheDocument();
  await act(() => current.resolve(statuses));
  expect(screen.getByText('Azure CLI').parentElement).toHaveTextContent('Detected');
});

it('invalidates pending tool readiness when a generic cloud configuration choice changes', async () => {
  const data = await bridge.bootstrap();
  data.catalog.entries.find(entry => entry.id === 'maester-automation')!.manifest!.configuration.groups[0].fields.push({
    id: 'cloud', environmentVariable: 'FIXTURE_CLOUD', label: 'Fixture cloud', type: 'select', default: 'AzureCloud',
    options: [{ label: 'Public fixture', value: 'AzureCloud' }, { label: 'Other fixture', value: 'OtherCloud' }]
  });
  vi.spyOn(bridge, 'bootstrap').mockResolvedValue(data);
  const check = bridge.checkRequirementPrerequisites;
  const probe = vi.spyOn(bridge, 'checkRequirementPrerequisites').mockImplementation(check);
  await features();
  await waitFor(() => expect(screen.getByText('Azure CLI').parentElement).toHaveTextContent('Detected'));
  const statuses = await probe.mock.results.at(-1)!.value;
  const old = deferred<ToolStatus[]>(); const current = deferred<ToolStatus[]>();
  probe.mockImplementationOnce(() => old.promise).mockImplementationOnce(() => current.promise);
  const count = probe.mock.calls.length;
  await fireEvent.click(screen.getByRole('button', { name: 'Check tools again' }));
  await waitFor(() => expect(probe).toHaveBeenCalledTimes(count + 1));
  await fireEvent.change(screen.getByRole('combobox', { name: /Fixture cloud/i }), { target: { value: 'OtherCloud' } });
  await waitFor(() => expect(probe).toHaveBeenCalledTimes(count + 2));
  await act(() => old.reject(new Error('old cloud failure')));
  expect(screen.queryByText(/Detection could not be verified/)).not.toBeInTheDocument();
  await act(() => current.resolve(statuses));
  expect(screen.getByText('Azure CLI').parentElement).toHaveTextContent('Detected');
});

it('does not treat a known non-public provider cloud as verified subscription access', async () => {
  const connections = await bridge.refreshConnections();
  vi.spyOn(bridge, 'loadWizardDraft').mockResolvedValue({ tenantId, subscriptionId });
  const refresh = vi.spyOn(bridge, 'refreshConnections').mockResolvedValue(connections);
  await environment();
  await screen.findByText(/Verified for target/);
  refresh.mockResolvedValue(connections.map(connection => connection.id === 'azureCli'
    ? { ...connection, azureCloud: 'AzureUSGovernment', state: 'unknown' } : connection));
  await fireEvent.click(screen.getByRole('button', { name: 'Check accounts again' }));
  await screen.findByText(/Unsupported cloud/);
  expect(screen.getByText('Azure CLI account').parentElement).toHaveTextContent('AzureUSGovernment');
  expect(screen.queryByText(/Verified for target/)).not.toBeInTheDocument();
  expect(screen.getByText(/Verified for tenant/)).toHaveTextContent('does not check subscription access');
  expect(screen.getByRole('button', { name: /^continue$/i })).toBeDisabled();
});

it.each(['success', 'error'] as const)('ignores an old A account %s after A → B → A and a completed new A check', async outcome => {
  const connections = await bridge.refreshConnections();
  vi.spyOn(bridge, 'loadWizardDraft').mockResolvedValue({ tenantId, subscriptionId });
  const refresh = vi.spyOn(bridge, 'refreshConnections').mockResolvedValue(connections);
  await environment();
  await screen.findByText(/Verified for target/);
  const oldA = deferred<typeof connections>(); const b = deferred<typeof connections>(); const newA = deferred<typeof connections>();
  refresh.mockImplementationOnce(() => oldA.promise).mockImplementationOnce(() => b.promise).mockImplementationOnce(() => newA.promise);
  const count = refresh.mock.calls.length;
  await fireEvent.click(screen.getByRole('button', { name: 'Check accounts again' }));
  await waitFor(() => expect(refresh).toHaveBeenCalledTimes(count + 1));
  await fireEvent.input(screen.getByPlaceholderText('Subscription ID'), { target: { value: otherSubscription } });
  await waitFor(() => expect(refresh).toHaveBeenCalledTimes(count + 2));
  await fireEvent.input(screen.getByPlaceholderText('Subscription ID'), { target: { value: subscriptionId } });
  await waitFor(() => expect(refresh).toHaveBeenCalledTimes(count + 3));
  await act(() => newA.resolve(connections));
  await screen.findByText(/Verified for target/);
  await act(() => outcome === 'success' ? oldA.resolve(connections.map(connection => ({ ...connection, state: 'disconnected' }))) : oldA.reject(new Error('obsolete A account failure')));
  expect(screen.getByText(/Verified for target/)).toBeInTheDocument();
  expect(screen.getByText(/Verified for tenant/)).toHaveTextContent('does not check subscription access');
  expect(screen.getByText(/deployed identity’s grants are separate/)).toBeInTheDocument();
  expect(screen.queryByText(/Connection check failed/)).not.toBeInTheDocument();
  await act(() => b.resolve(connections.map(connection => ({ ...connection, state: 'disconnected' }))));
  expect(screen.getByText(/Verified for target/)).toBeInTheDocument();
});

it('explains a missing tool and the next action without unrelated optional blockers', async () => {
  const check = bridge.checkRequirementPrerequisites;
  vi.spyOn(bridge, 'checkRequirementPrerequisites').mockImplementation(async (...args) =>
    (await check(...args)).map(({ condition: _condition, ...tool }) => tool.id === 'exchange-online-management' ? { ...tool, state: 'missing', canInstall: false } : tool));
  await features(); await next();
  const row = (await screen.findByText('ExchangeOnlineManagement')).closest('.data-row') as HTMLElement;
  expect(within(row).getByText(/Used by the template’s PowerShell setup hooks for Include Exchange checks = true/)).toBeInTheDocument();
  expect(within(row).getByText(/Not installed or not found/)).toBeInTheDocument();
  expect(within(row).getByText(/Next: Use the installation instructions, then check again/)).toBeInTheDocument();
  expect(within(row).getByRole('button', { name: 'View instructions' })).toBeEnabled();
  expect(screen.getByRole('button', { name: /^continue$/i })).toBeDisabled();
  expect(screen.queryByText('Microsoft Graph PowerShell')).not.toBeInTheDocument();
});

it('labels an optional undetected tool without blocking Continue or suggesting a required update', async () => {
  const data = await bridge.bootstrap();
  const manifest = data.catalog.entries.find(entry => entry.id === 'maester-automation')!.manifest!;
  manifest.prerequisites = manifest.prerequisites.map(tool => tool.id === 'exchange-online-management' ? { ...tool, required: false } : tool);
  vi.spyOn(bridge, 'bootstrap').mockResolvedValue(data);
  await features(); await next();
  const row = (await screen.findByText('ExchangeOnlineManagement')).closest('.data-row') as HTMLElement;
  expect(within(row).getByText(/Optional; does not block continuing/)).toBeInTheDocument();
  expect(within(row).getByText('optional · unknown')).toBeInTheDocument();
  expect(screen.getByRole('button', { name: /^continue$/i })).toBeEnabled();
  const cliRow = screen.getByText('Azure CLI').closest('.data-row') as HTMLElement;
  expect(within(cliRow).getByText(/no tool change needed/)).toBeInTheDocument();
});

it('distinguishes a connected account with the wrong target from a verified account', async () => {
  vi.spyOn(bridge, 'loadWizardDraft').mockResolvedValue({ tenantId, subscriptionId });
  await environment();
  await screen.findByText(/Verified for target/);
  expect(screen.getByText(/Verified for tenant/)).toHaveTextContent('does not check subscription access');
  await fireEvent.input(screen.getByPlaceholderText('Subscription ID'), { target: { value: otherSubscription } });
  await waitFor(() => expect(screen.getByText(/Wrong target/)).toBeInTheDocument());
  expect(screen.getByText('Selected target does not match')).toBeInTheDocument();
  expect(screen.getByText('Azure CLI account').parentElement).toHaveTextContent('connected');
  expect(screen.getByText('Azure CLI account').parentElement).toHaveTextContent('Reconnect this provider with the intended account');
  expect(screen.getByRole('button', { name: /^continue$/i })).toBeDisabled();
});

it('keeps the new target unverified when an old target check finishes late', async () => {
  const connections = await bridge.refreshConnections();
  vi.spyOn(bridge, 'loadWizardDraft').mockResolvedValue({ tenantId, subscriptionId });
  const refresh = vi.spyOn(bridge, 'refreshConnections').mockResolvedValue(connections);
  await environment();
  await screen.findByText(/Verified for target/);
  expect(screen.getByText(/Verified for tenant/)).toBeInTheDocument();
  let finishOld!: (value: typeof connections) => void;
  let finishNew!: (value: typeof connections) => void;
  refresh.mockImplementationOnce(() => new Promise(resolve => { finishOld = resolve; }))
    .mockImplementationOnce(() => new Promise(resolve => { finishNew = resolve; }));
  await fireEvent.click(screen.getByRole('button', { name: 'Check accounts again' }));
  await fireEvent.input(screen.getByPlaceholderText('Subscription ID'), { target: { value: otherSubscription } });
  await waitFor(() => expect(refresh).toHaveBeenLastCalledWith({ tenantId, subscriptionId: otherSubscription }));
  await act(() => finishOld(connections));
  expect(screen.queryByText(/Verified for target/)).not.toBeInTheDocument();
  expect(screen.queryByText(/Verified for tenant/)).not.toBeInTheDocument();
  expect(screen.getAllByText(/Unverified/)).toHaveLength(2);
  expect(screen.getByRole('button', { name: /^continue$/i })).toBeDisabled();
  await act(() => finishNew(connections));
  await screen.findByText(/Wrong target/);
});

it('does not show an unusable retry on the existing-project wizard route', async () => {
  const check = bridge.checkRequirementPrerequisites;
  const tools = vi.spyOn(bridge, 'checkRequirementPrerequisites').mockImplementation(async (...args) =>
    (await check(...args)).map(tool => ({ ...tool, state: 'ready' })));
  vi.spyOn(bridge, 'checkRequirementPermissions').mockResolvedValue([]);
  const launch = vi.spyOn(bridge, 'openExternalTerminal').mockResolvedValue({ projectPath: 'C:\\GitHub\\sample-azd-project', workingDirectory: 'C:\\GitHub\\sample-azd-project', environment: 'dev', command: 'azd up --environment dev', opened: false, historical: false });
  const prepare = vi.spyOn(bridge, 'prepareDeployment');
  render(App);
  await fireEvent.click(screen.getByRole('button', { name: /open project/i }));
  await next();
  await waitFor(() => expect(tools).toHaveBeenCalled());
  await waitFor(() => expect(screen.getByRole('button', { name: 'Open external terminal' })).toBeEnabled());
  await fireEvent.click(screen.getByRole('button', { name: 'Open external terminal' }));
  await screen.findByRole('region', { name: 'Terminal handoff' });
  expect(screen.queryByRole('button', { name: 'Retry terminal handoff' })).not.toBeInTheDocument();
  expect(screen.getByRole('button', { name: 'Open external terminal' })).toBeEnabled();
  expect(launch).toHaveBeenCalledTimes(1); expect(prepare).not.toHaveBeenCalled();
});
