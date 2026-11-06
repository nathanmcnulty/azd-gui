<script lang="ts">
  import { onDestroy, onMount, createEventDispatcher } from 'svelte';
  import { invoke, isTauri } from '@tauri-apps/api/core';
  import { onOperationEvent, cancelOperation } from '../lib/bridge';
  import type { ProjectSummary, OperationEvent } from '../lib/types';
  export let project: ProjectSummary;
  let target: { tenant?: string; subscription?: string; resourceGroup?: string; digest?: string } = {};
  export let verificationContext: unknown = null;
  const dispatch = createEventDispatcher();
  let operation: string | null = null;
  let starting = false;
  let listening = false;
  let generation = 0;
  let status = 'Not checked in this session. Saved validation reports are historical records.';
  let previous = '';
  let unlisten = () => {};
  const early = new Map<string, OperationEvent>();
  $: key = JSON.stringify([project.path, project.environment, project.templateId, project.revision, verificationContext]);
  $: if (key !== previous) { previous = key; invalidate(); }
  function invalidate(refresh = true) {
    generation += 1; starting = false;
    status = 'Stale / not checked for this account, environment, source and configuration.';
    const old = operation; operation = null;
    if (old) void cancelOperation(old).catch(() => {});
    target = {};
    if (refresh) void readTarget(generation);
  }
  async function readTarget(current: number) {
    if (!isTauri() || !project.environment) return;
    try {
      const value = await invoke<typeof target>('get_project_verification_target', { projectPath: project.path, environment: project.environment });
      if (current === generation) target = value;
    } catch { if (current === generation) status = 'Saved target could not be read. Refresh the project configuration before checking.'; }
  }
  function event(value: OperationEvent) {
    if (value.kind !== 'validate' || !['succeeded','failed','cancelled'].includes(value.state)) return;
    if (starting && !operation) { early.set(value.operationId, value); return; }
    if (value.operationId !== operation) return;
    operation = null;
    status = value.state === 'succeeded' ? 'Read-only validation completed. Review the report for resource group access; this does not prove application behavior.' : value.state === 'cancelled' ? 'Check cancelled. Previous reports remain historical. Retry when ready.' : 'Check failed. Review the report and current account access, then retry. No sign-in was started.';
    dispatch('finished', value);
  }
  async function run() {
    if (!target.digest || !listening || !project.environment || starting || operation) return;
    const current = ++generation;
    starting = true; status = 'Starting read-only resource group validation…'; early.clear();
    try {
      if (!isTauri()) throw new Error('Desktop required');
      const id = await invoke<string>('start_project_verification', { projectPath: project.path, environment: project.environment, expectedTargetDigest: target.digest });
      if (current !== generation) { await cancelOperation(id); return; }
      operation = id; starting = false; status = 'Checking the saved tenant, subscription and resource group using the existing Azure CLI account…';
      const terminal = early.get(id); early.clear(); if (terminal) event(terminal);
    } catch { if (current === generation) status = 'Check could not start. Confirm the saved target and existing Azure CLI sign-in, then retry. No template hooks were run.'; }
    finally { if (current === generation) starting = false; }
  }
  async function cancel() { invalidate(); status = 'Check cancelled. Previous reports remain historical. Retry when ready.'; }
  onMount(() => { let active = true; void onOperationEvent(event).then(stop => { if (active) { unlisten = stop; listening = true; } else stop(); }).catch(() => { if (active) status = 'Operation updates are unavailable. Reopen the project before checking.'; }); return () => { active = false; unlisten(); }; });
  onDestroy(() => invalidate(false));
</script>
<section class="workspace-panel full" aria-label="Supported verification">
  <h2>Verify the deployment check</h2>
  <p>The project’s saved environment is the verification target. A changed azd default does not select another target.</p>
  <p>Target: {project.environment ?? 'Choose an environment'} · {project.path}</p>
  <dl class="summary-list columns"><div><dt>Tenant</dt><dd>{target.tenant || 'Saved tenant required'}</dd></div><div><dt>Subscription</dt><dd>{target.subscription || 'Saved subscription required'}</dd></div><div><dt>Resource group</dt><dd>{target.resourceGroup || 'Saved deployed group required'}</dd></div></dl>
  <p>Reads the saved tenant, subscription and deployed resource group with the existing Azure CLI account. Writes a private local validation report. No interactive sign-in or project hooks.</p>
  <p role="status">{status}</p>
  <button class="primary" disabled={!target.digest || !listening || !project.environment || starting || !!operation} on:click={run}>Run read-only validation</button>
  <button class="secondary" disabled={!starting && !operation} on:click={cancel}>Cancel validation</button>
  <p>Next: review each recorded check. Resolve failed access or missing resource group checks before retrying. When finished, review cleanup for the dedicated group; deleting it requires separate approval.</p>
</section>
