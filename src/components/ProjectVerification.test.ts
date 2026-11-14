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
 return { view, event: (state: OperationEvent['state'], id='operation') => act(() => event({ operationId:id, kind:'validate', state, message:'', timestamp:'', stream:'system', level:'info' })) };
}
it('runs only after click, matches native operation, and distinguishes read-back from application behavior', async () => {
 const {event}=await setup(); expect(vi.mocked(invoke).mock.calls.some(([command]) => command === 'start_project_verification')).toBe(false);
 await fireEvent.click(screen.getByText('Run read-only validation'));
 expect(invoke).toHaveBeenCalledWith('start_project_verification', { projectPath: project.path, environment: 'dev', expectedTargetDigest: 'target-digest' });
 await event('succeeded','unrelated'); expect(screen.queryByText(/Read-only validation completed/)).toBeNull();
 await event('succeeded'); await screen.findByText(/Read-only validation completed/);
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
it('revokes a late start after cancellation without accepting its result', async () => {
 const {event}=await setup(); let resolve!: (id:string)=>void;
 vi.mocked(invoke).mockImplementationOnce(() => new Promise(r => resolve=r));
 await fireEvent.click(screen.getByText('Run read-only validation')); await fireEvent.click(screen.getByText('Cancel validation'));
 await act(() => resolve('late'));
 expect(bridge.cancelOperation).toHaveBeenCalledWith('late');
 await event('succeeded','late'); expect(screen.queryByText(/Read-only validation completed/)).toBeNull();
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
