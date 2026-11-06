import { act, cleanup, fireEvent, render, screen } from '@testing-library/svelte';
import { afterEach, expect, it, vi } from 'vitest';
import { invoke } from '@tauri-apps/api/core';
import PimSavedOverview from './PimSavedOverview.svelte';
vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn() }));
afterEach(() => { cleanup(); vi.mocked(invoke).mockReset(); });
const saved = { historical: true, hasDraft: true, hasState: true, attemptCount: 2, pendingWrites: 1, lastStatus: 'partialOrUncertain', details: null };
it('discovers pending writes locally, reveals only explicitly and hides on identity/configuration change', async () => {
  vi.mocked(invoke).mockResolvedValueOnce(saved).mockResolvedValueOnce({ ...saved, details: { receipt: 'private receipt' } }).mockResolvedValueOnce(saved);
  const view = render(PimSavedOverview, { projectPath: 'C:/pilot', environment: 'pilot', refreshKey: { account: 'old' } });
  await screen.findByText('Partial or uncertain; investigate and recheck');
  expect(invoke).toHaveBeenCalledExactlyOnceWith('inspect_pim_pilot', { projectPath: 'C:/pilot', environment: 'pilot', reveal: false });
  await fireEvent.click(screen.getByText('Reveal sensitive saved records'));
  await screen.findByText(/private receipt/);
  await view.rerender({ projectPath: 'C:/pilot', environment: 'pilot', refreshKey: { account: 'new' } });
  expect(screen.queryByText(/private receipt/)).toBeNull();
  await screen.findByText('Reveal sensitive saved records');
  expect(vi.mocked(invoke).mock.calls.every(([command]) => command === 'inspect_pim_pilot')).toBe(true);
});
it('ignores late sensitive replies when switching project or environment and reports failures privately', async () => {
  let resolve!: (v: unknown) => void;
  vi.mocked(invoke).mockImplementationOnce(() => new Promise(r => resolve=r)).mockRejectedValueOnce(new Error('private path'));
  const view = render(PimSavedOverview, { projectPath: 'C:/one', environment: 'pilot' });
  await view.rerender({ projectPath: 'C:/two', environment: 'other' });
  await screen.findByRole('alert');
  await act(() => resolve({ ...saved, details: 'old private data' }));
  expect(screen.queryByText(/old private data|private path/)).toBeNull();
});
