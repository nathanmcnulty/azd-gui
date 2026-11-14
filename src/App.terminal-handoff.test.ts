import { cleanup, fireEvent, render, screen, waitFor, within } from '@testing-library/svelte';
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
