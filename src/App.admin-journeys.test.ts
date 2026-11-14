import { cleanup, fireEvent, render, screen, waitFor } from '@testing-library/svelte';
import { afterEach, expect, it, vi } from 'vitest';
const native = vi.hoisted(() => ({ invoke: vi.fn() }));
vi.mock('@tauri-apps/api/core', () => ({ invoke: native.invoke, isTauri: () => true }));
vi.mock('./lib/bridge', async original => {
 const bridge=await original<typeof import('./lib/bridge')>();
 const {mockBootstrap}=await import('./lib/mock-data');
 return { ...bridge, bootstrap: async () => ({...structuredClone(mockBootstrap), projects:[{id:'pim',name:'Saved PIM project',path:'C:/private-pilot',environment:'recovery',templateId:'pim-pilot',revision:'48bdbadb6a02c06fb4144e616ed9562b905b36ec',state:'attention'}, {id:'check',name:'Saved deployment check',path:'C:/private-check',environment:'dev',templateId:'deployment-check',revision:'abc6e896620990e68360a63dd14efda66a3f75cd',state:'ready'}]}), onOperationEvent:async()=>()=>{} };
});
import App from './App.svelte';
afterEach(() => { cleanup(); vi.resetAllMocks(); window.localStorage.clear(); });
it('discovers PIM in catalog through native route without generic preparation or sign-in',async()=>{
 render(App); await fireEvent.click(await screen.findByText('Browse catalog'));
 const card=(await screen.findByRole('heading',{name:'Selected-role PIM pilot'})).closest('article')!;
 await fireEvent.click(card.querySelector('button')!);
 await screen.findByRole('heading',{name:'Selected-role PIM pilot'});
 expect(native.invoke.mock.calls.every(([name])=>name==='dispose_graph_session')).toBe(true);
});
it('reopens saved PIM recovery from Home without invoking azd, Graph or old approval',async()=>{
 native.invoke.mockImplementation(async(command)=> command === 'inspect_pim_pilot' ? {hasDraft:true,hasState:true,pendingWrites:2,attemptCount:3,lastStatus:'partialOrUncertain'} : null);
 render(App); await fireEvent.click(await screen.findByRole('button',{name:/Saved PIM project/}));
 await screen.findByText('Partial or uncertain; investigate and recheck');
 expect(screen.getByLabelText('Environment')).toHaveValue('recovery');
 expect(native.invoke).toHaveBeenCalledWith('inspect_pim_pilot',{projectPath:'C:/private-pilot',environment:'recovery',reveal:false});
 expect(native.invoke.mock.calls.every(([name])=>['inspect_pim_pilot','dispose_graph_session'].includes(name))).toBe(true);
 expect(screen.queryByText('Approve application / resume')).toBeNull();
});

it('opens deployment-check and shows the native saved target even when generic environment values are hidden',async()=>{
 const {mockBootstrap}=await import('./lib/mock-data');
 const entry=mockBootstrap.catalog.entries.find(item=>item.id==='deployment-check')!;
 native.invoke.mockImplementation(async command=>{
  if(command==='get_project_template')return {entry,digest:'contract'};
  if(command==='list_project_environments')return [{name:'prod',isDefault:true,hasLocal:true,hasRemote:false,dotEnvPath:''},{name:'dev',isDefault:false,hasLocal:true,hasRemote:false,dotEnvPath:''}];
  if(command==='get_project_state')return {name:'Check'};
  if(command==='get_operation_history')return [];
  if(command==='get_project_environment_values')return ['AZURE_TENANT_ID','AZURE_SUBSCRIPTION_ID','AZURE_RESOURCE_GROUP'].map(name=>({name,isSet:true,sensitive:true,declared:false}));
  if(command==='get_project_validation_report'||command==='get_project_deployment_receipt')return {state:'notFound'};
  if(command==='get_project_verification_target')return {tenant:'11111111-1111-4111-8111-111111111111',subscription:'22222222-2222-4222-8222-222222222222',resourceGroup:'rg-fixture',digest:'reviewed-target'};
  if(command==='start_project_verification')return 'check-operation';
  return null;
 });
 render(App);await fireEvent.click(await screen.findByRole('button',{name:/Saved deployment check/}));
 await screen.findByText('rg-fixture');
 expect(native.invoke).toHaveBeenCalledWith('get_project_verification_target',{projectPath:'C:/private-check',environment:'dev'});
 expect(screen.getByText('11111111-1111-4111-8111-111111111111')).toBeInTheDocument();
 expect(native.invoke.mock.calls.some(([name])=>name==='start_project_verification')).toBe(false);
 const run=screen.getByText('Run read-only validation');await waitFor(()=>expect(run).toBeEnabled());await fireEvent.click(run);
 expect(native.invoke).toHaveBeenCalledWith('start_project_verification',{projectPath:'C:/private-check',environment:'dev',expectedTargetDigest:'reviewed-target'});
 expect(native.invoke.mock.calls.some(([name])=>/get_project_state|request.*session|start_azd_operation|request_execution_approval/.test(name))).toBe(false);
});
