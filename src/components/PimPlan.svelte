<script lang="ts">
  import { onDestroy } from 'svelte';
  import { cancelPimPlan, connectPimPlan, getPimPlanEvidence, reviewPimPlan, runPimPlan, type PimProcessEvidence, type PimPlanArtifact, type PimPlanConfiguration, type PimPlanInput, type PimPlanReview } from '../lib/pim-plan';

  let account = '';
  let tenantId = '';
  let clientId = '';
  let privilegedIds = '';
  let lessPrivilegedIds = '';
  let privilegedProfile: PimPlanConfiguration['privilegedAuthenticationProfile'] = 'mfa';
  let lessPrivilegedProfile: PimPlanConfiguration['lessPrivilegedAuthenticationProfile'] = 'mfa';
  let privilegedDevice: PimPlanConfiguration['privilegedDeviceRequirement'] = 'none';
  let lessPrivilegedDevice: PimPlanConfiguration['lessPrivilegedDeviceRequirement'] = 'none';
  let previewExistingObjects = false;
  let review: PimPlanReview | null = null;
  let reviewedInput: PimPlanInput | null = null;
  let artifact: PimPlanArtifact | null = null;
  let evidence: PimProcessEvidence | null = null;
  let copyStatus = '';
  let cancellationReceipt: PimProcessEvidence | null = null;
  let explicitCancellationEpoch = -1;
  const pollers = new Set<ReturnType<typeof setInterval>>();
  let connected = false;
  let busy = '';
  let error = '';
  let epoch = 0;
  let pollGeneration = 0;
  let disposal: Promise<void> = Promise.resolve();

  function invalidate() {
    const digest = review?.binding.requirementsDigest;
    epoch += 1;
    const cancelledEpoch = epoch;
    pollGeneration += 1;
    review = null;
    reviewedInput = null;
    artifact = null;
    evidence = null;
    copyStatus = '';
    cancellationReceipt = null;
    for (const timer of pollers) clearInterval(timer);
    pollers.clear();
    connected = false;
    busy = '';
    error = '';
    if (digest) disposal = disposal.catch(() => {}).then(async () => {
      const receipt = await cancelPimPlan(digest);
      if (cancelledEpoch === epoch && explicitCancellationEpoch === cancelledEpoch) cancellationReceipt = receipt;
    });
  }
  function cancelExplicitly() { explicitCancellationEpoch = epoch + 1; invalidate(); }
  function acceptEvidence(value: PimProcessEvidence, selected: PimPlanReview) {
    const context = value.context;
    if (context.account.toLowerCase() !== selected.binding.account || context.tenantId.toLowerCase() !== selected.binding.tenantId.toLowerCase() ||
        context.clientId.toLowerCase() !== selected.binding.clientId.toLowerCase() || context.resource !== 'https://graph.microsoft.com' ||
        !/^[a-f0-9]{64}$/.test(context.principalFingerprint) || context.verificationReads !== 1 ||
        selected.binding.scopes.some(scope => !context.scopes.some(actual => actual.toLowerCase() === scope.toLowerCase()))) {
      throw new Error('Observed Graph evidence does not match the reviewed identity and permissions.');
    }
    if (evidence && (value.sessionId !== evidence.sessionId || value.processId !== evidence.processId || context.principalFingerprint !== evidence.context.principalFingerprint)) {
      throw new Error('The owned Graph process changed. Review and sign in again.');
    }
    if (evidence && !evidence.active && value.active) throw new Error('A terminal process receipt cannot become active again.');
    if (!evidence || context.pimReadsAttempted >= evidence.context.pimReadsAttempted && context.pimReadsCompleted >= evidence.context.pimReadsCompleted) evidence = value;
  }
  async function stopAfterFailure(cause: unknown, current: number, selected: PimPlanReview) {
    if (current !== epoch) return;
    pollGeneration += 1;
    for (const timer of pollers) clearInterval(timer);
    pollers.clear();
    error = String(cause); connected = false;
    review = null; reviewedInput = null;
    let terminal: PimProcessEvidence | null = null;
    disposal = disposal.catch(() => {}).then(async () => { terminal = await cancelPimPlan(selected.binding.requirementsDigest); });
    try {
      await disposal;
      if (current === epoch) {
        if (evidence) evidence = { ...evidence, active: false };
        if (terminal) acceptEvidence(terminal, selected);
      }
    } catch {
      if (current === epoch) { evidence = null; error += ' Session cleanup could not be confirmed.'; }
    }
  }
  onDestroy(invalidate);

  function input(): PimPlanInput {
    const ids = (value: string) => value.split(/[\s,;]+/).map(v => v.trim()).filter(Boolean);
    return { reviewId: crypto.randomUUID(), account, tenantId, clientId, configuration: {
      privilegedRoleIds: ids(privilegedIds), lessPrivilegedRoleIds: ids(lessPrivilegedIds),
      privilegedAuthenticationProfile: privilegedProfile, lessPrivilegedAuthenticationProfile: lessPrivilegedProfile,
      privilegedDeviceRequirement: privilegedDevice, lessPrivilegedDeviceRequirement: lessPrivilegedDevice,
      previewExistingObjects
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
      const observed = await connectPimPlan(candidate, selected.binding.requirementsDigest);
      if (current === epoch) {
        if (!observed.active) throw new Error('The Graph process is no longer active. Review and sign in again.');
        acceptEvidence(observed, selected); connected = true;
      }
    } catch (cause) { await stopAfterFailure(cause, current, selected); }
    finally { if (current === epoch) busy = ''; }
  }
  async function generate() {
    if (!review || !reviewedInput || !connected) return;
    const current = epoch;
    const selected = review;
    busy = 'Reading current policies and building the plan…';
    error = '';
    artifact = null;
    copyStatus = '';
    const polling = ++pollGeneration;
    const timer = setInterval(() => {
      void getPimPlanEvidence(selected.binding.requirementsDigest).then(value => {
        if (current === epoch && polling === pollGeneration && value) acceptEvidence(value, selected);
      }).catch(() => { /* Final operation reply remains authoritative; polling never probes Graph. */ });
    }, 500);
    pollers.add(timer);
    try {
      const result = await runPimPlan(reviewedInput, selected.binding.requirementsDigest);
      if (current !== epoch) return;
      if (result.requirementsDigest !== selected.binding.requirementsDigest || result.sourceCommit !== selected.sourceCommit || result.plan.mode !== 'plan' || !result.executionEvidence.active) {
        throw new Error('The returned plan does not match this review. Review the configuration again.');
      }
      const preview = selected.configuration.previewExistingObjects;
      if (result.previewExistingObjects !== preview || result.configuration.previewExistingObjects !== preview || result.plan.previewExistingObjects !== preview ||
          result.ownershipReviewRequired !== true || result.ownershipEstablished !== false || result.executionAuthorized !== false ||
          result.plan.ownershipReviewRequired !== true || result.plan.ownershipEstablished !== false || result.plan.executionAuthorized !== false ||
          result.plan.configuration.Mode !== 'plan' || result.plan.configuration.AdoptExisting !== preview || result.plan.configuration.AdoptRoleContexts !== preview) {
        throw new Error('The returned preview choice or ownership status does not match this read-only review.');
      }
      acceptEvidence(result.executionEvidence, selected);
      artifact = result;
    } catch (cause) { await stopAfterFailure(cause, current, selected); }
    finally { clearInterval(timer); pollers.delete(timer); if (current === epoch) busy = ''; }
  }
  async function copyArtifact() {
    if (!artifact) return;
    const current = epoch;
    try { await navigator.clipboard.writeText(JSON.stringify(artifact, null, 2)); if (current === epoch) copyStatus = 'Complete plan JSON copied. Keep tenant details private.'; }
    catch { if (current === epoch) copyStatus = 'Copy failed. Select the complete JSON below to copy it manually.'; }
  }
  async function copyCancellation() {
    if (!cancellationReceipt) return;
    const current = epoch;
    try { await navigator.clipboard.writeText(JSON.stringify(cancellationReceipt, null, 2)); if (current === epoch) copyStatus = 'Cancellation receipt copied. Keep tenant details private.'; }
    catch { if (current === epoch) copyStatus = 'Copy failed. The session remains closed.'; }
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
    <fieldset class="pim-existing-objects"><legend>Existing objects</legend>
      <label class="pim-preview-choice"><input type="checkbox" bind:checked={previewExistingObjects} />Preview changes to existing PIM objects (read-only)</label>
      <small>Off by default. Preview can show proposed updates to existing contexts, policies and role-context links. It does not establish ownership, adopt objects or apply changes.</small>
    </fieldset>
  </div>
  <div class="hero-actions"><button class="secondary" disabled={!!busy} on:click={prepare}>Review configuration</button>
    {#if busy || connected}<button class="secondary" on:click={cancelExplicitly}>{busy ? 'Cancel' : 'Disconnect and clear plan'}</button>{/if}</div>
  {#if error}<p role="alert">{error}</p>{/if}
  {#if busy}<p role="status">{busy}</p>{/if}
  {#if cancellationReceipt}
    <section class="pim-review" aria-label="PIM cancellation receipt"><h2>Session closed</h2>
      <p>At cancellation request: {cancellationReceipt.context.pimReadsAttempted} plan dispatch attempts, {cancellationReceipt.context.pimReadsCompleted} validated completions. Worker PID: {cancellationReceipt.processId}.</p>
      <p>This historical receipt cannot authorize another read. Dispatch counters do not independently prove network traffic.</p>
      <button class="secondary" on:click={copyCancellation}>Copy cancellation receipt</button>
      {#if copyStatus}<p role="status">{copyStatus}</p>{/if}
    </section>
  {/if}
  {#if evidence}
    <p aria-live="polite">Protected plan reads: {evidence.context.pimReadsAttempted} started, {evidence.context.pimReadsCompleted} completed.</p>
    <details class="pim-review"><summary>Verified process evidence</summary>
      <p>Observed account: {evidence.context.account}<br />Tenant: {evidence.context.tenantId}<br />Client: {evidence.context.clientId}</p>
      <p>Worker PID: {evidence.processId}; session: <code>{evidence.sessionId}</code></p>
      <p>{evidence.active ? 'The native session remains available for its registered reads.' : 'Historical failure receipt; this process cannot authorize more reads.'}</p>
      <p>Principal fingerprint: <code>{evidence.context.principalFingerprint}</code></p>
      <p>Verification reads: {evidence.context.verificationReads}. Actual granted scopes: {evidence.context.scopes.join(', ')}</p>
      <p>Existing consent can return a broader grant set. This operation uses only its registered read routes and changes no consent.</p>
    </details>
  {/if}
  {#if review}
    <section aria-label="PIM plan review" class="pim-review"><h2>Read-only plan review</h2>
      <p>Account: {review.binding.account}<br />Tenant: {review.binding.tenantId}<br />Client: {review.binding.clientId}</p>
      <p>Required read permissions: {review.binding.scopes.join(', ')}</p>
      <p>Pinned template: <code>{review.sourceCommit}</code></p>
      <p>Existing-object preview: <strong>{review.configuration.previewExistingObjects ? 'On — ownership review required' : 'Off — existing conflicts stop planning'}</strong></p>
      {#if review.configuration.previewExistingObjects}<p>Existing objects remain unchanged. Any adoption/update labels in the planner JSON describe proposals only. No ownership or execution approval is granted.</p>{/if}
      <p>Emergency groups, notifications and session revocation are excluded.</p>
      <div class="hero-actions"><button class="secondary" disabled={!!busy || connected} on:click={connect}>Approve sign-in</button>
        <button class="primary" disabled={!!busy || !connected} on:click={generate}>Generate read-only plan</button></div>
    </section>
  {/if}
  {#if artifact}
    <section aria-label="PIM plan artifact" class="pim-review"><h2>Proposed changes</h2>
      <p>Generated {artifact.plan.generatedAt}. Review these proposals before considering enforcement elsewhere.</p>
      <p role="note">Ownership review is required. This artifact establishes no ownership and authorizes no execution.</p>
      <p>{artifact.ownershipReviewNotice}</p>
      {#if artifact.previewExistingObjects}<p>Existing-object actions below are proposals only. Context proposals also enable availability and replace the description with azd-pim's managed description; review the existing context and intended settings before enforcement elsewhere.</p>{/if}
      {#each Object.entries(artifact.plan.tiers) as [tier, details]}
        <h3>{tier === 'privileged' ? 'Privileged roles' : 'Less-privileged roles'}: {details.roles.length}</h3>
        {#if details.context}<p>Authentication context: {artifact.previewExistingObjects && details.context.action === 'adoptAndUpdate' ? 'proposed update — ownership not established' : details.context.action} — {details.context.displayName}{details.context.id ? ` (${details.context.id})` : ''}</p>{/if}
        <ul>{#each details.roleRules as rule}<li>{rule.role.displayName}: context rule {rule.action}; legacy activation MFA {rule.activationMfa.action}{#if artifact.previewExistingObjects && rule.existing?.claimValue} — existing context {rule.existing.claimValue}; proposed context {rule.desiredContextId}{/if}</li>{/each}</ul>
        <p>Conditional Access proposals: {details.conditionalAccessPolicies.map(policy => policy.action).join(', ') || 'none'}</p>
        {#if artifact.previewExistingObjects}<ul>{#each details.conditionalAccessPolicies as policy}<li>{policy.body?.displayName || policy.displayName || 'Conditional Access policy'}{policy.id ? ` (${policy.id})` : ''}: {policy.action === 'adoptAndUpdate' ? 'proposed update — ownership not established' : policy.action}</li>{/each}</ul>{/if}
      {/each}
      <ul>{#each artifact.plan.warnings as warning}<li>{warning}</li>{/each}</ul>
      <button class="secondary" on:click={copyArtifact}>Copy complete plan JSON</button>
      {#if copyStatus}<p role="status">{copyStatus}</p>{/if}
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
  .pim-preview-choice { display: flex; align-items: flex-start; }
  .pim-existing-objects { grid-column: 1 / -1; }
  .pim-preview-choice input { width: auto; margin-top: 3px; }
  small { color: var(--muted); line-height: 1.5; }
  .pim-review { margin-top: 24px; padding: 20px; border: 1px solid var(--border, #364152); border-radius: 10px; overflow-wrap: anywhere; }
  pre { max-height: 560px; overflow: auto; white-space: pre-wrap; font-size: 12px; }
</style>
