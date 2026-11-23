import { act, cleanup, fireEvent, render, screen, waitFor } from '@testing-library/svelte';
import { afterEach, expect, it, vi } from 'vitest';
import { invoke } from '@tauri-apps/api/core';
import * as bridge from '../lib/bridge';
import ProjectVerification from './ProjectVerification.svelte';
import type { OperationEvent } from '../lib/types';
vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn(), isTauri: () => true }));
vi.mock('../lib/bridge', () => ({ onOperationEvent: vi.fn(), cancelOperation: vi.fn() }));
const project = { id: 'p', path: 'C:/check', name: 'Check', environment: 'dev', state: 'ready' as const };
afterEach(() => { cleanup(); vi.resetAllMocks(); });
async function setup() {
 let event!: (e: OperationEvent) => void;
 vi.mocked(bridge.onOperationEvent).mockImplementation(async handler => { event=handler; return () => {}; });
 vi.mocked(invoke).mockImplementation(async command => command === 'get_project_verification_target' ? { tenant:'tenant', subscription:'subscription', resourceGroup:'group', digest:'target-digest' } : 'operation'); vi.mocked(bridge.cancelOperation).mockResolvedValue(undefined);
 const view=render(ProjectVerification, { project, verificationContext: 'account-a' });
 await waitFor(() => expect(event).toBeDefined());
 return { view, event: (state: OperationEvent['state'], id='operation', exitCode?: number) => act(() => event({ operationId:id, kind:'validate', state, message:'private-probe-output', timestamp:'2026-11-23T02:01:45+00:00', stream:'system', level:'info', exitCode })) };
}
it('runs only after click, matches native operation, and distinguishes read-back from application behavior', async () => {
 const {event}=await setup(); expect(vi.mocked(invoke).mock.calls.some(([command]) => command === 'start_project_verification')).toBe(false);
 await fireEvent.click(screen.getByText('Run read-only validation'));
 expect(invoke).toHaveBeenCalledWith('start_project_verification', { projectPath: project.path, environment: 'dev', expectedTargetDigest: 'target-digest' });
 await event('succeeded','unrelated'); expect(screen.queryByText(/Read-only validation completed/)).toBeNull();
 await event('succeeded','operation',0); await screen.findByText(/Read-only validation completed/);
 expect(screen.getByText('2026-11-23T02:01:45+00:00')).toBeVisible();
 expect(screen.getByText('0')).toBeVisible();
 expect(screen.queryByText('private-probe-output')).toBeNull();
 expect(screen.getByText(/does not prove application behavior/)).toBeInTheDocument();
});
it('invalidates on account/config/source change, cancels and ignores old completion, and permits retry', async () => {
 const {view,event}=await setup(); await fireEvent.click(screen.getByText('Run read-only validation'));
 await view.rerender({ project, verificationContext:'account-b' });
 expect(bridge.cancelOperation).toHaveBeenCalledWith('operation');
 await event('succeeded'); expect(screen.queryByText(/Read-only validation completed/)).toBeNull();
 await fireEvent.click(screen.getByText('Run read-only validation')); await event('failed');
 await screen.findByText(/Check failed/);
 expect(screen.getByText('Run read-only validation')).toBeEnabled();
});
it('waits for the native cancellation result when cancelling before the start returns', async () => {
 const {event}=await setup(); let resolve!: (id:string)=>void;
 vi.mocked(invoke).mockImplementationOnce(() => new Promise(r => resolve=r));
 await fireEvent.click(screen.getByText('Run read-only validation'));
 await fireEvent.click(screen.getByText('Cancel validation'));
 expect(screen.getByRole('status')).toHaveTextContent(/Cancellation requested/);
 expect(screen.getByText('Run read-only validation')).toBeDisabled();
 await act(() => resolve('late'));
 expect(bridge.cancelOperation).toHaveBeenCalledWith('late');
 expect(screen.getByRole('status')).toHaveTextContent(/Cancellation requested/);
 await event('cancelled','late');
 expect(screen.getByRole('status')).toHaveTextContent(/Check cancelled/);
 expect(screen.getByText('Run read-only validation')).toBeEnabled();
});

it('keeps the real failure when a cancellation request cannot be confirmed', async () => {
 const {event}=await setup();
 vi.mocked(bridge.cancelOperation).mockRejectedValueOnce(new Error('private cancel failure'));
 await fireEvent.click(screen.getByText('Run read-only validation'));
 await fireEvent.click(screen.getByText('Cancel validation'));
 expect(screen.getByRole('status')).toHaveTextContent(/Cancellation could not be confirmed/);
 expect(screen.getByText('Run read-only validation')).toBeDisabled();
 await event('failed','operation',37);
 expect(screen.getByRole('status')).toHaveTextContent(/Check failed/);
 expect(screen.getByText('37')).toBeVisible();
 expect(screen.queryByText('private cancel failure')).toBeNull();
});

it('shows a completion already received before a cancelling start returns', async () => {
 const {event}=await setup(); let resolve!: (id:string)=>void;
 vi.mocked(invoke).mockImplementationOnce(() => new Promise(r => resolve=r));
 await fireEvent.click(screen.getByText('Run read-only validation'));
 await fireEvent.click(screen.getByText('Cancel validation'));
 await event('succeeded','late',0);
 await act(() => resolve('late'));
 expect(bridge.cancelOperation).not.toHaveBeenCalled();
 expect(screen.getByRole('status')).toHaveTextContent(/Read-only validation completed/);
});

it('waits for native event registration before allowing a check', async () => {
 let listen!: (stop:()=>void)=>void;
 vi.mocked(bridge.onOperationEvent).mockImplementation(() => new Promise(resolve => listen=resolve));
 vi.mocked(invoke).mockResolvedValue({ digest:'target-digest' });
 render(ProjectVerification,{project});
 expect(screen.getByText('Run read-only validation')).toBeDisabled();
 await fireEvent.click(screen.getByText('Run read-only validation')); expect(vi.mocked(invoke).mock.calls.some(([command]) => command === 'start_project_verification')).toBe(false);
 await act(() => listen(() => {}));
 expect(screen.getByText('Run read-only validation')).toBeEnabled();
});
