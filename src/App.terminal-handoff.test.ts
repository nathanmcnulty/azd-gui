import { act, cleanup, fireEvent, render, screen, waitFor, within } from '@testing-library/svelte';
import { afterEach, expect, it, vi } from 'vitest';
import App from './App.svelte';
import * as bridge from './lib/bridge';
import type { ExternalTerminalLaunch } from './lib/types';

afterEach(() => { cleanup(); vi.restoreAllMocks(); vi.unstubAllGlobals(); window.localStorage.clear(); });

const failed: ExternalTerminalLaunch = {
  projectPath: 'C:\\Deployments\\maester-lab', environment: 'dev', workingDirectory: 'C:\\Deployments\\maester-lab',
  command: 'azd up --environment dev', opened: false, historical: false, detail: 'fixture launch failure'
};

it('retains copyable instructions after failure, retries only launch, and restores the historical handoff', async () => {
  const prepare = vi.spyOn(bridge, 'prepareDeployment');
  const start = vi.spyOn(bridge, 'startAzdOperation');
  const approve = vi.spyOn(bridge, 'requestExecutionApproval');
  const launch = vi.spyOn(bridge, 'openExternalTerminal').mockResolvedValueOnce(failed)
    .mockResolvedValueOnce({ ...failed, opened: true, detail: undefined });
  const saved = vi.spyOn(bridge, 'getExternalTerminalHandoff').mockResolvedValue(null);
  const copy = vi.fn().mockResolvedValue(undefined);
  vi.stubGlobal('navigator', { userAgent: navigator.userAgent, clipboard: { writeText: copy } });
  render(App);
  await fireEvent.click(screen.getByRole('button', { name: /^Projects,/ }));
  await fireEvent.click(screen.getByRole('button', { name: /maester/i }));
  await screen.findByRole('heading', { name: 'Continue in a terminal' });
  await fireEvent.click(screen.getByRole('button', { name: /^Terminal$/i }));
  const notice = await screen.findByRole('region', { name: 'Terminal handoff' });
  expect(within(notice).getByText('Terminal not opened')).toBeInTheDocument();
  expect(within(notice).getByText('Prepared, deployment not observed.')).toBeInTheDocument();
  await fireEvent.click(within(notice).getByRole('button', { name: 'Copy command' }));
  await fireEvent.click(within(notice).getByRole('button', { name: 'Copy project directory' }));
  expect(copy.mock.calls).toEqual([[failed.command], [failed.workingDirectory]]);
  await fireEvent.click(within(notice).getByRole('button', { name: 'Retry terminal handoff' }));
  expect(await screen.findByRole('heading', { name: 'External terminal opened' })).toBeInTheDocument();
  expect(launch).toHaveBeenCalledTimes(2);
  expect(launch).toHaveBeenLastCalledWith(failed.projectPath, 'dev');
  expect(prepare).not.toHaveBeenCalled(); expect(start).not.toHaveBeenCalled(); expect(approve).not.toHaveBeenCalled();
  saved.mockResolvedValue({ ...failed, opened: true, historical: true, detail: undefined });
  await fireEvent.click(screen.getByRole('button', { name: /All projects/i }));
  await fireEvent.click(screen.getByRole('button', { name: /maester/i }));
  expect(await screen.findByRole('heading', { name: 'Last terminal handoff opened' })).toBeInTheDocument();
  expect(launch).toHaveBeenCalledTimes(2);
  expect(screen.queryByText('Deployment completed')).not.toBeInTheDocument();
});

it('does not transfer a late launch reply to another navigation context', async () => {
  let finish!: (handoff: ExternalTerminalLaunch) => void;
  vi.spyOn(bridge, 'openExternalTerminal').mockImplementation(() => new Promise(resolve => { finish = resolve; }));
  render(App);
  await fireEvent.click(screen.getByRole('button', { name: /^Projects,/ }));
  await fireEvent.click(screen.getByRole('button', { name: /maester/i }));
  await screen.findByRole('heading', { name: 'Continue in a terminal' });
  await fireEvent.click(screen.getByRole('button', { name: /^Terminal$/i }));
  await fireEvent.click(screen.getByRole('button', { name: /^Catalog$/i }));
  finish(failed);
  await waitFor(() => expect(screen.queryByRole('region', { name: 'Terminal handoff' })).not.toBeInTheDocument());
});

it('routes handoff retry through the unsaved project edit guard', async () => {
  vi.spyOn(bridge, 'getExternalTerminalHandoff').mockResolvedValue({ ...failed, historical: true });
  const launch = vi.spyOn(bridge, 'openExternalTerminal');
  const prepare = vi.spyOn(bridge, 'prepareDeployment');
  render(App);
  await fireEvent.click(screen.getByRole('button', { name: /^Projects,/ }));
  await fireEvent.click(screen.getByRole('button', { name: /maester/i }));
  await screen.findByRole('heading', { name: 'Terminal not opened' });
  await fireEvent.click(screen.getByRole('button', { name: 'environments' }));
  await fireEvent.input(await screen.findByLabelText(/variable name/i), { target: { value: 'LOCAL_DRAFT' } });
  await fireEvent.input(screen.getByLabelText(/new value/i), { target: { value: 'unsaved' } });
  await fireEvent.click(screen.getByRole('button', { name: 'Retry terminal handoff' }));
  expect(launch).not.toHaveBeenCalled(); expect(prepare).not.toHaveBeenCalled();
  expect(screen.getByRole('alert')).toBeInTheDocument();
});

it('cannot retry a saved handoff for another environment', async () => {
  vi.spyOn(bridge, 'getExternalTerminalHandoff').mockResolvedValue({ ...failed, environment: 'other', historical: true });
  const launch = vi.spyOn(bridge, 'openExternalTerminal');
  render(App);
  await fireEvent.click(screen.getByRole('button', { name: /^Projects,/ }));
  await fireEvent.click(screen.getByRole('button', { name: /maester/i }));
  await screen.findByRole('heading', { name: 'Terminal not opened' });
  await fireEvent.click(screen.getByRole('button', { name: 'Retry terminal handoff' }));
  expect(launch).not.toHaveBeenCalled();
});

it.each(['hook', 'source'])('rechecks %s approval after deferred wizard permission checks', async (approval) => {
  const launch = vi.spyOn(bridge, 'openExternalTerminal');
  const prepare = vi.spyOn(bridge, 'prepareDeployment');
  render(App);
  await fireEvent.click(screen.getByRole('button', { name: /open project/i }));
  await fireEvent.click(screen.getByRole('button', { name: /^continue$/i }));
  await fireEvent.click(screen.getByRole('checkbox', { name: /reviewed this source and approve/i }));
  for (let attempt = 0; attempt < 10 && !screen.queryByRole('heading', { name: /review deployment/i }); attempt += 1) {
    const profile = screen.queryByRole('button', { name: /apply minimal pilot/i });
    if (profile) await fireEvent.click(profile);
    const next = screen.getByRole('button', { name: /^continue$/i });
    await waitFor(() => expect(next).toBeEnabled()); await fireEvent.click(next);
  }
  await fireEvent.click(screen.getByRole('checkbox', { name: /approve pinned template code execution/i }));
  await fireEvent.click(screen.getByRole('button', { name: /prepare and open terminal/i }));
  await screen.findByRole('heading', { name: 'External terminal opened' });
  expect(prepare).toHaveBeenCalledTimes(1);
  launch.mockClear();
  let finish!: (value: Awaited<ReturnType<typeof bridge.checkRequirementPermissions>>) => void;
  vi.spyOn(bridge, 'checkRequirementPermissions').mockImplementationOnce(() => new Promise(resolve => { finish = resolve; }));
  await fireEvent.click(screen.getByRole('button', { name: 'Retry terminal handoff' }));
  await waitFor(() => expect(finish).toBeDefined());
  if (approval === 'source') await fireEvent.click(screen.getByRole('button', { name: /Review template$/i }));
  await fireEvent.click(screen.getByRole('checkbox', { name: approval === 'hook' ? /approve pinned template code execution/i : /reviewed this source and approve/i }));
  await act(() => finish([]));
  expect(launch).not.toHaveBeenCalled();
  expect(prepare).toHaveBeenCalledTimes(1);
});

it('blocks both primary Terminal and Retry during an active lifecycle operation', async () => {
  const data = await bridge.bootstrap(); data.isNative = true;
  vi.spyOn(bridge, 'bootstrap').mockResolvedValue(data);
  vi.spyOn(bridge, 'getExternalTerminalHandoff').mockResolvedValue(failed);
  vi.spyOn(bridge, 'startAzdOperation').mockResolvedValue('active-refresh');
  const launch = vi.spyOn(bridge, 'openExternalTerminal');
  render(App);
  await fireEvent.click(screen.getByRole('button', { name: /^Projects,/ }));
  await fireEvent.click(screen.getByRole('button', { name: /maester/i }));
  await screen.findByRole('heading', { name: 'Continue in a terminal' });
  await fireEvent.click(screen.getByRole('button', { name: /Refresh Refresh environment state/i }));
  await waitFor(() => expect(screen.getByRole('button', { name: /^Terminal$/i })).toBeDisabled());
  expect(screen.getByRole('button', { name: 'Retry terminal handoff' })).toBeDisabled();
  await fireEvent.click(screen.getByRole('button', { name: /^Terminal$/i }));
  await fireEvent.click(screen.getByRole('button', { name: 'Retry terminal handoff' }));
  expect(launch).not.toHaveBeenCalled();
});

it.each(['running', 'failed'] as const)('a late terminal outcome cannot overwrite a newer %s lifecycle or its history', async (state) => {
  const data = await bridge.bootstrap(); data.isNative = true;
  data.projects = data.projects.map(project => project.path === failed.projectPath ? { ...project, state: 'deploying' } : project);
  vi.spyOn(bridge, 'bootstrap').mockResolvedValue(data);
  let emit!: Parameters<typeof bridge.onOperationEvent>[0];
  vi.spyOn(bridge, 'onOperationEvent').mockImplementation(async listener => { emit = listener; return () => {}; });
  let finish!: (handoff: ExternalTerminalLaunch) => void;
  vi.spyOn(bridge, 'openExternalTerminal').mockImplementation(() => new Promise(resolve => { finish = resolve; }));
  vi.spyOn(bridge, 'startAzdOperation').mockResolvedValue('lifecycle-refresh');
  render(App);
  await fireEvent.click(screen.getByRole('button', { name: /^Projects,/ }));
  await fireEvent.click(screen.getByRole('button', { name: /maester/i }));
  await screen.findByRole('heading', { name: 'Continue in a terminal' });
  await fireEvent.click(screen.getByRole('button', { name: /^Terminal$/i }));
  await fireEvent.click(screen.getByRole('button', { name: /Refresh Refresh environment state/i }));
  await waitFor(() => expect(screen.getByRole('button', { name: /^Terminal$/i })).toBeDisabled());
  await act(() => emit({ operationId: 'lifecycle-refresh', kind: 'refresh', state, timestamp: new Date().toISOString(), stream: 'system', level: state === 'failed' ? 'error' : 'info', message: `New lifecycle ${state}` }));
  await act(() => finish({ ...failed, opened: true, detail: undefined }));
  await screen.findByRole('heading', { name: 'External terminal opened' });
  await fireEvent.click(screen.getByRole('button', { name: 'operations' }));
  if (state === 'running') expect(screen.getByRole('button', { name: 'Cancel refresh' })).toBeInTheDocument();
  expect(screen.getByText(`New lifecycle ${state}`, { selector: '.operation-row span' })).toBeInTheDocument();
  await fireEvent.click(screen.getByRole('button', { name: /All projects/i }));
  expect(screen.getByRole('button', { name: /maester/i })).toHaveTextContent(state === 'failed' ? 'attention' : 'deploying');
});

it('does not restore copyable instructions when the native contract check rejects them', async () => {
  vi.spyOn(bridge, 'getExternalTerminalHandoff').mockRejectedValue(new Error('Current contract blocks terminal handoff'));
  render(App);
  await fireEvent.click(screen.getByRole('button', { name: /^Projects,/ }));
  await fireEvent.click(screen.getByRole('button', { name: /maester/i }));
  await screen.findByRole('heading', { name: 'Continue in a terminal' });
  expect(screen.queryByRole('region', { name: 'Terminal handoff' })).not.toBeInTheDocument();
  expect(screen.queryByText(/Reviewed command:/)).not.toBeInTheDocument();
});

it('removes old instructions when retry is rejected by the current native contract', async () => {
  vi.spyOn(bridge, 'getExternalTerminalHandoff').mockResolvedValue(failed);
  vi.spyOn(bridge, 'openExternalTerminal').mockRejectedValue(new Error('Current contract blocks terminal handoff'));
  render(App);
  await fireEvent.click(screen.getByRole('button', { name: /^Projects,/ }));
  await fireEvent.click(screen.getByRole('button', { name: /maester/i }));
  await screen.findByRole('heading', { name: 'Terminal not opened' });
  await fireEvent.click(screen.getByRole('button', { name: 'Retry terminal handoff' }));
  await waitFor(() => expect(screen.queryByRole('region', { name: 'Terminal handoff' })).not.toBeInTheDocument());
  expect(screen.queryByText(/Reviewed command:/)).not.toBeInTheDocument();
});
