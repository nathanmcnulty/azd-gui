<script lang="ts">
  import StatusPill from './StatusPill.svelte';
  import { onDestroy, createEventDispatcher } from 'svelte';
  import { open } from '@tauri-apps/plugin-dialog';
  import { invoke } from '@tauri-apps/api/core';
  import { cancelPimPlan, type PimPlanInput } from '../lib/pim-plan';
  export let input: PimPlanInput;
  export let projects: { path: string; name: string; templateId?: string | null; environment?: string | null }[] = [];
  const dispatch = createEventDispatcher();
  let projectPath = '';
  let environment = 'pilot';
  let bindingDigest = '';
  let result: any = null;
  let busy = false;
  let error = '';
  let approved = false;
  let typedTenant = '';
  let generation = 0;
  let prior = '';
  let disposal: Promise<void> = Promise.resolve();
  $: context = JSON.stringify({ input, projectPath, environment });
  $: if (context !== prior) { prior = context; invalidate(); }
  $: pending = result?.state?.pendingRoleRules && Object.keys(result.state.pendingRoleRules).length > 0 ||
    Object.values(result?.state?.contexts ?? {}).some((v: any) => v.pendingIntent) ||
    Object.values(result?.state?.conditionalAccessPolicies ?? {}).some((v: any) => v.pendingCreate || v.pendingIntent);
  $: remaining = result?.plan ? Object.values(result.plan.tiers).some((tier: any) => tier.context?.action !== undefined && tier.context.action !== 'none' ||
    tier.conditionalAccessPolicies.some((p: any) => p.action !== 'none') || tier.roleRules.some((r: any) => r.action !== 'none' || r.activationMfa.action !== 'none')) : true;
  function invalidate() {
    generation += 1; approved = false; typedTenant = ''; result = null; error = ''; busy = false;
    const old = bindingDigest; bindingDigest = '';
    const current = generation;
    if (old) disposal = disposal.catch(() => {}).then(async () => { await cancelPimPlan(old); });
    void disposal.catch(() => { if (current === generation) error = 'Cancellation needs verification. Reconnect and recheck this project environment.'; });
  }
  const request = () => ({ projectPath, environment, input });
  async function run(action: 'connect' | 'recheck' | 'apply') {
    const current = generation;
    const expectedApproval = result?.digest;
    approved = false; busy = true; error = '';
    try {
      await disposal;
      if (current !== generation) return;
      let value;
      if (action === 'connect') {
        const binding = await invoke<{ requirementsDigest: string }>('review_pim_pilot', { input: request() });
        if (current !== generation) return;
        bindingDigest = binding.requirementsDigest;
        value = await invoke('connect_pim_pilot', { input: request(), expectedDigest: bindingDigest });
      } else value = await invoke('run_pim_pilot', { input: request(), expectedDigest: bindingDigest, approvedDigest: action === 'apply' ? expectedApproval : null });
      if (current === generation) { result = value; typedTenant = ''; }
    } catch (cause) {
      if (current === generation) {
        error = `${cause} Reconnect and recheck before approving again. Existing checkpoints and receipts are retained. If existing-object conflicts block planning, use the preview checkbox above to inspect exact objects; duplicate names must be resolved manually.`;
        const old = bindingDigest; bindingDigest = ''; result = null;
        if (old) { disposal = disposal.catch(() => {}).then(async () => { await cancelPimPlan(old); }); await disposal; }
      }
    } finally { if (current === generation) busy = false; }
  }
  async function registerWorkspace() {
    try {
      const path = await open({ directory: true, multiple: false, title: 'Choose a private PIM pilot workspace' });
      if (typeof path !== 'string') return;
      const project = await invoke<any>('register_pim_pilot', { projectPath: path });
      projects = [...projects.filter(p => p.path !== project.path), project];
      projectPath = project.path;
    } catch (cause) { error = String(cause); }
  }
  async function restore() {
    invalidate(); const current = generation;
    try {
      const saved = await invoke<any>('load_pim_pilot', { projectPath, environment });
      if (current !== generation) return;
      if (saved?.identity && saved?.configuration) dispatch('restore', { ...saved.identity, configuration: saved.configuration });
      else error = 'No saved pilot draft. Configure the selected roles above.';
    } catch (cause) { if (current === generation) error = String(cause); }
  }
  onDestroy(invalidate);
</script>

<section class="pilot">
  <h2>Apply a selected-role pilot</h2>
  <p>Choose a registered azd-pim project and environment. Sign-in for this stage requests write permissions; it does not approve application or new consent.</p>
  <button class="secondary" on:click={registerWorkspace} disabled={busy}>Register pilot workspace</button>
  <label>Project <select bind:value={projectPath} disabled={busy}><option value="">Select project</option>{#each projects as project}<option value={project.path}>{project.name} — {project.path}</option>{/each}</select></label>
  <label>Environment <input bind:value={environment} disabled={busy} /></label>
  <button class="secondary" on:click={restore} disabled={!projectPath || busy}>Restore saved pilot configuration</button>
  <button class="secondary" on:click={() => run('connect')} disabled={!projectPath || busy}>Review sign-in and recheck pilot</button>
  {#if bindingDigest}<button class="secondary" on:click={() => run('recheck')} disabled={busy}>Read-only recheck</button>{/if}
  <button class="secondary" on:click={invalidate} disabled={!bindingDigest && !busy}>Cancel pilot session</button>
  {#if busy}<p role="status">Working in the owned pilot process… Cancellation may leave changes needing verification.</p>{/if}
  {#if error}<p role="alert">{error}</p>{/if}
  {#if result}
    <p><StatusPill tone={result.digest && !remaining && !pending ? "success" : "warning"} label={result.digest ? !remaining && !pending ? "Planner settings match" : "Review required" : result.receipt?.completed ? "Applied · recheck required" : "Partial or uncertain"} /> · Tenant {input.tenantId} · {projectPath} · {environment}</p>
    {#if result.receipt}<h3>Latest attempt receipt</h3>
      {#each result.receipt.writeResults ?? [] as write}<p>{write.method} {write.uri}: <strong>{write.outcome === 'confirmed' ? 'Write succeeded; recheck required' : write.outcome === 'rejected' ? 'Write failed (Graph rejected it)' : 'Outcome uncertain; verification required'}</strong></p>{/each}
      <pre>{JSON.stringify(result.receipt, null, 2)}</pre>{/if}
    {#each result.recoveryBlockers ?? [] as blocker}<p role="alert">{blocker}</p>{/each}
    {#if result.state}<details open><summary>Durable operation results</summary>
      {#each Object.entries(result.state.contexts ?? {}) as [key, value]}<p>Context {key} · {(value as any).id} · {(value as any).pendingIntent ? 'pending / needs verification' : 'checkpoint completed'}</p>{/each}
      {#each Object.entries(result.state.conditionalAccessPolicies ?? {}) as [key, value]}<p>Conditional Access {key} · {(value as any).id ?? 'server ID uncertain'} · {(value as any).pendingCreate || (value as any).pendingIntent ? 'pending / needs verification' : 'checkpoint completed'}</p>{/each}
      {#each Object.entries(result.state.appliedRoleRules ?? {}) as [key, value]}<p>Role {(value as any).roleDisplayName} · {key} · context {(value as any).desiredContextId} {result.state.pendingRoleRules?.[key] ? 'last recorded completion; current operation pending / needs verification' : 'last recorded completion'}; legacy MFA removed: {JSON.stringify((value as any).removedLegacyActivationMfa)}</p>{/each}
      {#each Object.entries(result.state.pendingRoleRules ?? {}) as [key, value]}<p>Role {(value as any).roleDisplayName} · {key} · pending / needs verification</p>{/each}
    </details>{/if}
    {#if pending}<p role="alert">Pending or uncertain writes exist. This live plan reconciles their current state; no ownership checkpoint is cleared by a recheck. Review every remaining operation before approving a resume.</p>{/if}
    {#if !remaining && !pending}<p>Read-only recheck found matching planner-managed settings and no pending intents. Inspect complete current policies for additional effective conditions, exclusions and controls; the planner compares its managed subset. Human activation validation below is still required.</p>{/if}
    {#if result.digest}
      <h3>Exact proposed changes</h3>
      <p>Review {result.digest}. Existing display-name matches do not establish ownership. Approval below covers only the exact IDs and current/proposed properties shown here.</p>
      {#each Object.entries(result.plan.tiers) as [name, tier]}
        {@const t = tier as any}
        <h4>{name}</h4>
        {#each t.roles as role}<p>{role.displayName} · {role.id}</p>{/each}
        {#if t.context}<p>Authentication context {t.context.id}: {t.context.action}. Proposed: available, named {t.context.displayName}, description “Authentication context managed by azd-pim for {name} Microsoft Entra role activation.”</p><details><summary>Current context</summary><pre>{JSON.stringify(t.context.existing, null, 2)}</pre></details>{/if}
        {#each t.conditionalAccessPolicies as policy}<details open><summary>Conditional Access {policy.id ?? 'new server-assigned ID'}: {policy.action}</summary><p>Before</p><pre>{JSON.stringify(policy.existing, null, 2)}</pre><p>After</p><pre>{JSON.stringify(policy.body, null, 2)}</pre></details>{/each}
        {#each t.roleRules as rule}<details open><summary>{rule.role.displayName}: {rule.action}; legacy activation MFA: {rule.activationMfa.action}</summary><p>Policy {rule.policyId}. Context {rule.existing.claimValue || 'none'} → {rule.desiredContextId} (enabled).</p><p>Activation requirements: {JSON.stringify(rule.activationMfa.existing.enabledRules)} → {JSON.stringify(rule.activationMfa.desiredEnabledRules)}. Justification and ticketing are preserved. A failed attachment attempts legacy-MFA restoration; interrupted writes require rechecking.</p></details>{/each}
      {/each}
      {#each result.plan.warnings as warning}<p>{warning}</p>{/each}
      <details><summary>Complete plan and ownership checkpoint</summary><pre>{JSON.stringify({ plan: result.plan, state: result.state }, null, 2)}</pre></details>
      <label><input type="checkbox" bind:checked={approved} disabled={busy} /> I approve these exact changes, including shown adoptions and legacy MFA replacement.</label>
      <label>Type tenant ID <input bind:value={typedTenant} disabled={busy} /></label>
      <button class="primary" on:click={() => run('apply')} disabled={busy || (result.recoveryBlockers?.length ?? 0) > 0 || !approved || typedTenant.trim().toLowerCase() !== input.tenantId.trim().toLowerCase()}>Approve application / resume</button>
    {:else}<p>Application has finished or stopped. Run a read-only recheck to verify completed settings and review remaining changes before a new approval.</p>{/if}
  {/if}
  <h3>Validate the pilot</h3>
  <p>Recheck live configuration first. Then use a designated eligible test administrator for each selected tier: activate the selected role, verify fresh MFA or the selected authentication strength and device controls, confirm emergency-access procedures, and inspect sign-in and PIM audit results. Check preserved justification/ticketing and test expected denial on a noncompliant device where configured. A successful write receipt does not prove activation behavior.</p>
  <p>Receipts and the existing ownership checkpoint are saved under the selected project's .azure/environment directory. Restart by restoring the draft, reconnecting, and rechecking. No automatic resume or approval survives restart. Role selection is forward configuration; removing a role later does not undo its previous settings.</p>
</section>
<style>
  .pilot { margin-top: 2rem; padding: 20px; border: 1px solid var(--border); border-radius: 10px; overflow-wrap: anywhere; }
  label { display: block; margin: .9rem 0; }
  input:not([type="checkbox"]), select { display: block; width: 100%; max-width: 900px; margin-top: 8px; padding: 9px 10px; box-sizing: border-box; border: 1px solid var(--border); border-radius: var(--radius); background: var(--input); color: var(--text); }
  input[type="checkbox"] { margin-right: 6px; }
  pre { max-height: 24rem; overflow: auto; white-space: pre-wrap; overflow-wrap: anywhere; font-size: 12px; padding: 12px; background: var(--input); border-radius: var(--radius); }
  button { margin: .3rem; } details { margin: .8rem 0; } summary { cursor: pointer; }
</style>
