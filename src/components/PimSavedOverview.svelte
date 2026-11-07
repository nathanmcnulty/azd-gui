<script lang="ts">
  import { onDestroy } from 'svelte';
  import { invoke } from '@tauri-apps/api/core';
  export let projectPath = '';
  export let environment = 'pilot';
  export let refreshKey: unknown = null;
  let result: any = null;
  let loading = false;
  let error = '';
  let generation = 0;
  let previous = '';
  $: context = JSON.stringify([projectPath, environment, refreshKey]);
  $: if (context !== previous) { previous = context; void refresh(); }
  async function refresh(reveal = false) {
    const current = ++generation;
    result = null; error = ''; loading = true;
    if (!projectPath || !environment) { loading = false; return; }
    try {
      const value = await invoke('inspect_pim_pilot', { projectPath, environment, reveal });
      if (current === generation) result = value;
    } catch { if (current === generation) error = 'Saved pilot records could not be read safely. Preserve the workspace and investigate before continuing.'; }
    finally { if (current === generation) loading = false; }
  }
  onDestroy(() => { generation += 1; });
</script>
<section class="workspace-panel full" aria-label="Saved PIM overview">
  <h2>Project overview and recovery</h2>
  <p>Environment: {environment} · {projectPath}</p>
  <p>Historical local records. Opening this project does not sign in, run hooks or restore an application approval.</p>
  {#if loading}<p role="status">Reading saved pilot records…</p>{/if}
  {#if error}<p role="alert">{error}</p>{/if}
  {#if result}
    <dl class="summary-list columns">
      <div><dt>Saved configuration</dt><dd>{result.hasDraft ? 'Available to restore' : 'No saved draft'}</dd></div>
      <div><dt>Saved attempts</dt><dd>{result.attemptCount}</dd></div>
      <div><dt>Last application</dt><dd>{result.lastStatus === 'appliedNeedsVerification' ? 'Applied; read-back verification required' : result.lastStatus ? 'Partial or uncertain; investigate and recheck' : 'No attempt receipt'}</dd></div>
      <div><dt>Last attempt started</dt><dd>{result.lastStartedAt || "No recorded attempt time"}</dd></div>
      <div><dt>Pending writes</dt><dd>{result.pendingWrites}</dd></div>
      <div><dt>Verification</dt><dd>Stale / not checked in this session</dd></div>
      <div><dt>Next action</dt><dd>{result.hasDraft ? 'Restore configuration, review identity, then explicitly sign in and recheck.' : 'Configure selected roles and review the target before sign-in.'}</dd></div>
    </dl>
    {#if result.pendingWrites || result.hasState || result.attemptCount}<p role="note">Preserve all checkpoints and receipts. Investigate uncertain writes and reconcile with a read-only recheck. A missing policy never authorizes replaying a create. Restore the original selection and controls if pending work is outside the current scope.</p>{/if}
    {#if result.details}<pre>{JSON.stringify(result.details, null, 2)}</pre><button class="secondary" on:click={() => refresh()}>Hide sensitive records</button>
    {:else}<button class="secondary" on:click={() => refresh(true)}>Reveal sensitive saved records</button>{/if}
  {/if}
  <button class="secondary" disabled={loading} on:click={() => refresh()}>Refresh saved records</button>
</section>
<style>pre { max-height: 24rem; overflow: auto; white-space: pre-wrap; overflow-wrap: anywhere; }</style>
