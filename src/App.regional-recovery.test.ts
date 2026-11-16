import { cleanup, fireEvent, render, screen, waitFor, within } from '@testing-library/svelte';
import { afterEach, expect, it, vi } from 'vitest';
import App from './App.svelte';
import * as bridge from './lib/bridge';
import type { ExternalTerminalLaunch } from './lib/types';

afterEach(() => { cleanup(); vi.restoreAllMocks(); window.localStorage.clear(); });

const tenantId = '72f988bf-86f1-41af-91ab-2d7cd011db47';
const subscriptionId = '11111111-1111-1111-1111-111111111111';
const projectPath = 'C:\\Deployments\\maester-regional-fixture';

async function next() {
  const button = screen.getByRole('button', { name: /^continue$/i });
  try { await waitFor(() => expect(button).toBeEnabled()); }
  catch { throw new Error(`Continue stayed disabled at ${document.querySelector('.wizard-header h1')?.textContent}: ${document.querySelector('.wizard-next-guidance')?.textContent}`); }
  await fireEvent.click(button);
}

async function reachDeploymentReview() {
  render(App);
  await fireEvent.click(screen.getByRole('button', { name: /browse catalog/i }));
  await fireEvent.click(screen.getAllByRole('button', { name: /configure deployment/i })[1]);
  await fireEvent.input(screen.getByPlaceholderText(/empty destination folder/i), { target: { value: projectPath } });
  await next(); await next();
  await screen.findByRole('heading', { name: 'Maester coverage' });
  await fireEvent.click(screen.getByRole('button', { name: /apply minimal pilot/i }));
  await next(); await next(); await next();
  await screen.findByRole('heading', { name: 'Azure environment' });
  await fireEvent.click(screen.getByRole('button', { name: 'Enter IDs manually' }));
  await screen.findByText(/Verified for target/);
  for (let attempt = 0; attempt < 12 && !screen.queryByRole('heading', { name: 'Review deployment' }); attempt += 1) {
    await next();
  }
  await screen.findByRole('heading', { name: 'Review deployment' });
}

it('recovers from a quota rejection in a new region with Minimal choices and a fresh handoff', async () => {
  vi.spyOn(bridge, 'loadWizardDraft').mockResolvedValue({
    tenantId, subscriptionId, location: 'westus2', environmentName: 'maester-regional-fixture',
    resourceGroup: 'rg-maester-regional-fixture'
  });
  const originalReadiness = bridge.checkRequirementPrerequisites;
  const readiness = vi.spyOn(bridge, 'checkRequirementPrerequisites').mockImplementation(async (...args) =>
    (await originalReadiness(...args)).map(tool => ({ ...tool, state: 'ready' as const, installedVersion: 'fixture' })));
  vi.spyOn(bridge, 'checkRequirementPermissions').mockResolvedValue([]);
  const review = vi.spyOn(bridge, 'reviewWizardRequirements');
  const writeEnvironment = vi.spyOn(bridge, 'setProjectEnvironmentValues');
  const terminal: ExternalTerminalLaunch = {
    projectPath, environment: 'maester-regional-fixture', workingDirectory: projectPath,
    command: 'azd up --environment maester-regional-fixture', opened: true, historical: false
  };
  const launch = vi.spyOn(bridge, 'openExternalTerminal').mockResolvedValue(terminal);
  const restored = vi.spyOn(bridge, 'getExternalTerminalHandoff').mockResolvedValue(null);

  await reachDeploymentReview();
  await fireEvent.click(screen.getByRole('checkbox', { name: /approve pinned template code execution/i }));
  await fireEvent.click(screen.getByRole('button', { name: 'Prepare and open terminal' }));
  await screen.findByRole('heading', { name: 'External terminal opened' });
  expect(launch).toHaveBeenCalledTimes(1);
  expect(launch).toHaveBeenLastCalledWith(projectPath, 'maester-regional-fixture');

  await fireEvent.input(screen.getByLabelText(/Azure quota error code or short quota detail/i), {
    target: { value: 'Conflict: The operation could not be completed because it exceeded your quota for Automation accounts in your subscription in the region.' }
  });
  await fireEvent.click(screen.getByRole('button', { name: 'Record stopped attempt' }));
  expect(await screen.findByRole('heading', { name: 'Quota rejection reported' })).toBeInTheDocument();
  expect(screen.getByText(/explicitly reports an Automation Account regional quota limit/i)).toBeInTheDocument();
  expect(screen.queryByRole('region', { name: 'Terminal handoff' })).not.toBeInTheDocument();
  expect(screen.getByRole('button', { name: /Prepare and open terminal/i })).toBeDisabled();
  expect(launch).toHaveBeenCalledTimes(1);

  await fireEvent.click(screen.getByRole('button', { name: 'Choose a different subscription or region and rerun readiness' }));
  const resourceGroup = screen.getByPlaceholderText('Created from environment name');
  await fireEvent.input(resourceGroup, { target: { value: 'rg-maester-regional-fixture-edited' } });
  expect(screen.getByRole('heading', { name: /Previous quota rejection/ })).toBeInTheDocument();
  expect(screen.getByText(/Choose a different subscription or region before creating another terminal handoff/)).toBeInTheDocument();

  const region = screen.getByRole('combobox', { name: /Azure region/i });
  expect(region).toHaveValue('westus2');
  const westusReadinessCallsBeforeChange = readiness.mock.calls.filter(([selection]) => selection.target?.location === 'westus').length;
  await fireEvent.change(region, { target: { value: 'westus' } });
  await waitFor(() => expect(readiness.mock.calls.filter(([selection]) => selection.target?.location === 'westus').length).toBeGreaterThan(westusReadinessCallsBeforeChange));
  expect(screen.getByRole('heading', { name: /Previous quota rejection/ })).toBeInTheDocument();
  expect(screen.getByText(/historical evidence for the previous target/i)).toBeInTheDocument();
  expect(screen.queryByRole('region', { name: 'Terminal handoff' })).not.toBeInTheDocument();

  // Continue through the current target's checks; the catalog source and Minimal profile are preserved.
  for (let attempt = 0; attempt < 12 && !screen.queryByRole('heading', { name: 'Review deployment' }); attempt += 1) await next();
  await screen.findByRole('heading', { name: 'Review deployment' });
  expect(screen.getByText('westus')).toBeInTheDocument();
  const coverage = screen.getByRole('region', { name: 'Review Maester coverage' });
  expect(within(coverage).getByText('Include Exchange checks').parentElement).toHaveTextContent('No');
  const recoveredReview = review.mock.calls.find(([entry, values]) => entry.id === 'maester-automation' && values.location === 'westus');
  expect(recoveredReview?.[0]).toMatchObject({ commit: '1cbaf9064a6ac4c61dd175bf2f61ca08367de0ff' });
  expect(recoveredReview?.[1]).toMatchObject({
    includeWebApp: false,
    includeExchange: false,
    includeTeams: false,
    includeAzure: false,
    permissionProfile: 'Minimal',
    mailRecipient: ''
  });
  await fireEvent.click(screen.getByRole('checkbox', { name: /approve pinned template code execution/i }));
  await fireEvent.click(screen.getByRole('button', { name: 'Prepare and open terminal' }));
  await waitFor(() => expect(launch).toHaveBeenCalledTimes(2));
  await screen.findByRole('heading', { name: 'External terminal opened' });
  expect(restored).not.toHaveBeenCalled();
  await expect(launch.mock.results[1].value).resolves.toMatchObject({ historical: false, opened: true });
  await waitFor(() => expect(writeEnvironment).toHaveBeenLastCalledWith(
    projectPath,
    'maester-regional-fixture',
    expect.arrayContaining([expect.objectContaining({ name: 'AZURE_LOCATION', value: 'westus' })])
  ));
  expect(recoveredReview).toBeDefined();

  await fireEvent.click(screen.getByRole('button', { name: 'Edit Azure environment' }));
  const returnedRegion = screen.getByRole('combobox', { name: /Azure region/i });
  const failedTargetReadinessCallsBeforeReturn = readiness.mock.calls.filter(([selection]) => selection.target?.location === 'westus2').length;
  await fireEvent.change(returnedRegion, { target: { value: 'westus2' } });
  await waitFor(() => expect(readiness.mock.calls.filter(([selection]) => selection.target?.location === 'westus2').length).toBeGreaterThan(failedTargetReadinessCallsBeforeReturn));
  expect(screen.getByRole('heading', { name: /Previous quota rejection/ })).toBeInTheDocument();
  expect(screen.getByText(/Choose a different subscription or region before creating another terminal handoff/)).toBeInTheDocument();
  for (let attempt = 0; attempt < 12 && !screen.queryByRole('heading', { name: 'Review deployment' }); attempt += 1) await next();
  await screen.findByRole('heading', { name: 'Review deployment' });
  expect(screen.getByRole('button', { name: 'Prepare and open terminal' })).toBeDisabled();
  expect(launch).toHaveBeenCalledTimes(2);
}, 15000);

it('keeps an inconclusive terminal result distinct from a confirmed quota response', async () => {
  vi.spyOn(bridge, 'loadWizardDraft').mockResolvedValue({ tenantId, subscriptionId, location: 'westus2' });
  const originalReadiness = bridge.checkRequirementPrerequisites;
  vi.spyOn(bridge, 'checkRequirementPrerequisites').mockImplementation(async (...args) =>
    (await originalReadiness(...args)).map(tool => ({ ...tool, state: 'ready' as const, installedVersion: 'fixture' })));
  vi.spyOn(bridge, 'checkRequirementPermissions').mockResolvedValue([]);
  vi.spyOn(bridge, 'openExternalTerminal').mockResolvedValue({
    projectPath, environment: 'dev', workingDirectory: projectPath,
    command: 'azd up --environment dev', opened: true, historical: false
  });
  await reachDeploymentReview();
  await fireEvent.click(screen.getByRole('checkbox', { name: /approve pinned template code execution/i }));
  await fireEvent.click(screen.getByRole('button', { name: 'Prepare and open terminal' }));
  await screen.findByRole('heading', { name: 'External terminal opened' });
  await fireEvent.input(screen.getByLabelText(/Azure quota error code or short quota detail/i), { target: { value: 'OperationNotAllowed' } });
  await fireEvent.click(screen.getByRole('button', { name: 'Record stopped attempt' }));
  expect(await screen.findByRole('heading', { name: 'Quota could not be confirmed' })).toBeInTheDocument();
  expect(screen.getByText(/Quota exhaustion is unconfirmed/)).toBeInTheDocument();
});
