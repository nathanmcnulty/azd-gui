<script lang="ts">
  import { onDestroy } from 'svelte';
  import { cancelPimPlan, connectPimPlan, reviewPimPlan, runPimPlan, type PimPlanArtifact, type PimPlanConfiguration, type PimPlanInput, type PimPlanReview } from '../lib/pim-plan';

  let account = '';
  let tenantId = '';
  let clientId = '';
  let privilegedIds = '';
  let lessPrivilegedIds = '';
  let privilegedProfile: PimPlanConfiguration['privilegedAuthenticationProfile'] = 'mfa';
  let lessPrivilegedProfile: PimPlanConfiguration['lessPrivilegedAuthenticationProfile'] = 'mfa';
  let privilegedDevice: PimPlanConfiguration['privilegedDeviceRequirement'] = 'none';
  let lessPrivilegedDevice: PimPlanConfiguration['lessPrivilegedDeviceRequirement'] = 'none';
  let review: PimPlanReview | null = null;
  let reviewedInput: PimPlanInput | null = null;
  let artifact: PimPlanArtifact | null = null;
  let connected = false;
  let busy = '';
  let error = '';
  let epoch = 0;
  let disposal: Promise<void> = Promise.resolve();

  function invalidate() {
    const digest = review?.binding.requirementsDigest;
    epoch += 1;
    review = null;
    reviewedInput = null;
    artifact = null;
    connected = false;
    busy = '';
    error = '';
    if (digest) disposal = disposal.catch(() => {}).then(() => cancelPimPlan(digest));
  }
  onDestroy(invalidate);

  function input(): PimPlanInput {
    const ids = (value: string) => value.split(/[\s,;]+/).map(v => v.trim()).filter(Boolean);
    return { reviewId: crypto.randomUUID(), account, tenantId, clientId, configuration: {
      privilegedRoleIds: ids(privilegedIds), lessPrivilegedRoleIds: ids(lessPrivilegedIds),
      privilegedAuthenticationProfile: privilegedProfile, lessPrivilegedAuthenticationProfile: lessPrivilegedProfile,
      privilegedDeviceRequirement: privilegedDevice, lessPrivilegedDeviceRequirement: lessPrivilegedDevice
    } };
  }
  async function prepare() {
    invalidate();
    const current = epoch;
    const candidate = input();
    busy = 'Reviewing configuration…';
    try {
      await disposal;
      if (current !== epoch) return;
      const result = await reviewPimPlan(candidate);
      if (current !== epoch) return;
      review = result;
      reviewedInput = candidate;
    } catch (cause) { if (current === epoch) error = String(cause); }
    finally { if (current === epoch) busy = ''; }
  }
  async function connect() {
    if (!review || !reviewedInput) return;
    const current = epoch;
    const selected = review;
    const candidate = reviewedInput;
    busy = 'Waiting for approved sign-in…';
    error = '';
    try {
      await disposal;
      if (current !== epoch) return;
      await connectPimPlan(candidate, selected.binding.requirementsDigest);
      if (current === epoch) connected = true;
    } catch (cause) { if (current === epoch) { error = String(cause); connected = false; } }
    finally { if (current === epoch) busy = ''; }
  }
  async function generate() {
    if (!review || !reviewedInput || !connected) return;
    const current = epoch;
    const selected = review;
    busy = 'Reading current policies and building the plan…';
    error = '';
    artifact = null;
    try {
      const result = await runPimPlan(reviewedInput, selected.binding.requirementsDigest);
      if (current !== epoch) return;
      if (result.requirementsDigest !== selected.binding.requirementsDigest || result.sourceCommit !== selected.sourceCommit || result.plan.mode !== 'plan') {
        throw new Error('The returned plan does not match this review. Review the configuration again.');
      }
      artifact = result;
    } catch (cause) { if (current === epoch) { error = String(cause); connected = false; } }
    finally { if (current === epoch) busy = ''; }
  }
</script>

<section class="page pim-plan">
  <div class="page-header"><div><span class="eyebrow">AZD-PIM</span><h1>Review a PIM solution plan</h1>
    <p>Compare selected roles with proposed authentication contexts, Conditional Access policies and PIM policy rules. This workflow only reads; applying the plan is outside this workflow.</p></div></div>
  <div class="pim-fields" on:input={invalidate} on:change={invalidate}>
    <fieldset><legend>Graph identity</legend>
      <label>Account<input type="email" autocomplete="username" bind:value={account} placeholder="you@example.com" /></label>
      <label>Tenant ID<input bind:value={tenantId} spellcheck="false" /></label>
      <label>Existing client ID<input bind:value={clientId} spellcheck="false" /></label>
    </fieldset>
    {#each ['privileged', 'lessPrivileged'] as tier}
      <fieldset><legend>{tier === 'privileged' ? 'Privileged roles' : 'Less-privileged roles'}</legend>
        <label>Role definition IDs{#if tier === 'privileged'}<textarea bind:value={privilegedIds} rows="3"></textarea>{:else}<textarea bind:value={lessPrivilegedIds} rows="3"></textarea>{/if}</label>
        <small>Separate IDs with commas or new lines. Graph verifies the role’s classification. Leave this tier empty to exclude it.</small>
        <label>Authentication{#if tier === 'privileged'}<select bind:value={privilegedProfile}><option value="mfa">Multifactor authentication</option><option value="phishingResistant">Phishing-resistant authentication</option></select>{:else}<select bind:value={lessPrivilegedProfile}><option value="mfa">Multifactor authentication</option><option value="phishingResistant">Phishing-resistant authentication</option></select>{/if}</label>
        <label>Device requirement{#if tier === 'privileged'}<select bind:value={privilegedDevice}><option value="none">None</option><option value="compliant">Compliant</option><option value="hybridJoined">Hybrid joined</option><option value="compliantOrHybrid">Compliant or hybrid joined</option></select>{:else}<select bind:value={lessPrivilegedDevice}><option value="none">None</option><option value="compliant">Compliant</option><option value="hybridJoined">Hybrid joined</option><option value="compliantOrHybrid">Compliant or hybrid joined</option></select>{/if}</label>
      </fieldset>
    {/each}
  </div>
  <div class="hero-actions"><button class="secondary" disabled={!!busy} on:click={prepare}>Review configuration</button>
    {#if busy || connected}<button class="secondary" on:click={invalidate}>{busy ? 'Cancel' : 'Disconnect and clear plan'}</button>{/if}</div>
  {#if error}<p role="alert">{error}</p>{/if}
  {#if busy}<p role="status">{busy}</p>{/if}
  {#if review}
    <section aria-label="PIM plan review" class="pim-review"><h2>Read-only plan review</h2>
      <p>Account: {review.binding.account}<br />Tenant: {review.binding.tenantId}<br />Client: {review.binding.clientId}</p>
      <p>Required read permissions: {review.binding.scopes.join(', ')}</p>
      <p>Pinned template: <code>{review.sourceCommit}</code></p>
      <p>Emergency groups, notifications and session revocation are excluded.</p>
      <div class="hero-actions"><button class="secondary" disabled={!!busy || connected} on:click={connect}>Approve sign-in</button>
        <button class="primary" disabled={!!busy || !connected} on:click={generate}>Generate read-only plan</button></div>
    </section>
  {/if}
  {#if artifact}
    <section aria-label="PIM plan artifact" class="pim-review"><h2>Proposed changes</h2>
      <p>Generated {artifact.plan.generatedAt}. Review these proposals before considering enforcement elsewhere.</p>
      {#each Object.entries(artifact.plan.tiers) as [tier, details]}
        <h3>{tier === 'privileged' ? 'Privileged roles' : 'Less-privileged roles'}: {details.roles.length}</h3>
        {#if details.context}<p>Authentication context: {details.context.action} — {details.context.displayName}</p>{/if}
        <ul>{#each details.roleRules as rule}<li>{rule.role.displayName}: context rule {rule.action}; legacy activation MFA {rule.activationMfa.action}</li>{/each}</ul>
        <p>Conditional Access proposals: {details.conditionalAccessPolicies.map(policy => policy.action).join(', ') || 'none'}</p>
      {/each}
      <ul>{#each artifact.plan.warnings as warning}<li>{warning}</li>{/each}</ul>
      <details><summary>Complete plan JSON</summary><pre>{JSON.stringify(artifact, null, 2)}</pre></details>
    </section>
  {/if}
</section>

<style>
  .pim-fields { display: grid; grid-template-columns: repeat(auto-fit, minmax(240px, 1fr)); gap: 20px; margin: 24px 0; }
  fieldset { min-width: 0; border: 1px solid var(--border, #364152); border-radius: 10px; padding: 18px; }
  label { display: grid; gap: 8px; margin: 14px 0; }
  input, textarea, select { width: 100%; box-sizing: border-box; padding: 9px 10px; border: 1px solid var(--border); border-radius: var(--radius); background: var(--input); color: var(--text); font: inherit; }
  input:focus, textarea:focus, select:focus { outline: 1px solid var(--accent); border-color: var(--accent); }
  small { color: var(--muted); line-height: 1.5; }
  .pim-review { margin-top: 24px; padding: 20px; border: 1px solid var(--border, #364152); border-radius: 10px; overflow-wrap: anywhere; }
  pre { max-height: 560px; overflow: auto; white-space: pre-wrap; font-size: 12px; }
</style>
