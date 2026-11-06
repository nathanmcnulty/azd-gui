<script lang="ts">
  import { onDestroy } from 'svelte';
  import PimRolePicker from './PimRolePicker.svelte';
  import { cachedPimRoles, refreshPimRoles, roleChanges, roleIds, selectedRoleProblems, type PimRole, type PimRoleCatalog } from '../lib/pim-roles';
  let catalog: PimRoleCatalog | null = null;
  let roleNotice = '';
  let selectedMetadata: Record<string, PimRole> = {};
  let priorIdentity = '';
  let identityGeneration = 0;
  $: identity = `${account.trim().toLowerCase()}|${tenantId.trim().toLowerCase()}|${clientId.trim().toLowerCase()}`;
  $: if (identity !== priorIdentity) void identityChanged(identity);
  $: roleProblems = catalog ? selectedRoleProblems(catalog, privilegedIds, lessPrivilegedIds) : [];
  $: metadataChanges = catalog ? roleChanges(selectedMetadata, catalog, [...roleIds(privilegedIds), ...roleIds(lessPrivilegedIds)]) : [];
  async function identityChanged(value: string) {
    priorIdentity = value;
    const generation = ++identityGeneration;
    catalog = null;
    selectedMetadata = {};
    roleNotice = 'Identity changed. Selected IDs are retained and must be checked against this tenant before planning.';
    if (!/^[a-f0-9-]{36}$/i.test(tenantId.trim())) return;
    try {
      const cached = await cachedPimRoles(tenantId.trim());
      if (generation === identityGeneration) catalog = cached;
    } catch { if (generation === identityGeneration) roleNotice = 'Saved role list could not be read. Refresh tenant roles or use manual IDs.'; }
  }
  function rememberSelectedRoles() {
    selectedMetadata = Object.fromEntries((catalog?.roles ?? []).filter(role => [...roleIds(privilegedIds), ...roleIds(lessPrivilegedIds)].includes(role.id)).map(role => [role.id, role]));
  }
  function rememberNewSelectedRoles() {
    // First discovery establishes a baseline; refreshing cannot acknowledge changes.
    const previous = selectedMetadata;
    rememberSelectedRoles();
    selectedMetadata = { ...selectedMetadata, ...previous };
  }
  function selectRole(id: string, privileged: boolean, checked: boolean) {
    privilegedIds = roleIds(privilegedIds).filter(value => value !== id).join('\n');
    lessPrivilegedIds = roleIds(lessPrivilegedIds).filter(value => value !== id).join('\n');
    if (checked) {
      if (privileged) privilegedIds = [...roleIds(privilegedIds), id].sort().join('\n');
      else lessPrivilegedIds = [...roleIds(lessPrivilegedIds), id].sort().join('\n');
    }
    const metadata = { ...selectedMetadata };
    delete metadata[id];
    const role = catalog?.roles.find(role => role.id === id);
    if (checked && role) metadata[id] = role;
    selectedMetadata = metadata;
    invalidate();
  }
  function acceptRoleChanges() { rememberSelectedRoles(); invalidate(); }
  async function refreshRoles() {
    if (!review || !connected) return;
    const current = epoch;
    const selected = review;
    busy = 'Refreshing complete tenant role list…';
    roleNotice = '';
    identityGeneration += 1;
    artifact = null;
    try {
      const fresh = await refreshPimRoles(selected.binding.requirementsDigest);
      if (current !== epoch) return;
      if (fresh.tenantId.toLowerCase() !== tenantId.trim().toLowerCase() || fresh.source !== 'live') throw new Error('Role list tenant or source mismatch.');
      catalog = fresh;
      rememberNewSelectedRoles();
      roleNotice = 'All role pages refreshed. New roles remain unselected.';
    } catch (cause) {
      if (current !== epoch) return;
      if (catalog) catalog = { ...catalog, source: 'cached' };
      roleNotice = 'Refresh failed or was incomplete. The previous complete list and its retrieval time are retained; it is not current. Check read permissions or reconnect.';
      await stopAfterFailure(cause, current, selected);
    } finally { if (current === epoch) busy = ''; }
  }
  import { cancelPimPlan, connectPimPlan, getPimPlanEvidence, reviewPimPlan, runPimPlan, type PimCancellationReceipt, type PimProcessEvidence, type PimPlanArtifact, type PimPlanConfiguration, type PimPlanInput, type PimPlanReview } from '../lib/pim-plan';

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
  let cancellationReceipt: PimCancellationReceipt | null = null;
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
    if (catalog) catalog = { ...catalog, source: 'cached' };
    copyStatus = '';
    cancellationReceipt = null;
    for (const timer of pollers) clearInterval(timer);
    pollers.clear();
    connected = false;
    busy = '';
    error = '';
    if (digest) disposal = disposal.catch(() => {}).then(async () => {
      const receipt = await cancelPimPlan(digest);
      if (cancelledEpoch === epoch && explicitCancellationEpoch === cancelledEpoch && receipt) {
        validateCancellation(receipt, digest);
        cancellationReceipt = receipt;
      }
    });
    void disposal.catch(() => { if (cancelledEpoch === epoch) error = 'Session cleanup or cancellation evidence could not be confirmed.'; });
  }
  function cancelExplicitly() { explicitCancellationEpoch = epoch + 1; invalidate(); }
  function validateCancellation(receipt: PimCancellationReceipt, digest: string) {
    const fresh = receipt.snapshotSource === 'nativeRequestRevocationV1';
    if (receipt.requirementsDigest !== digest || receipt.processEvidence?.active !== false ||
        fresh && typeof receipt.pimPlanRequestInFlightAtRevocation !== 'boolean' ||
        !fresh && (receipt.pimPlanRequestInFlightAtRevocation !== null ||
          !['alreadyRevokedNativeWorkerV1', 'historicalProcessEvidenceV1'].includes(receipt.snapshotSource))) {
      throw new Error('Cancellation receipt does not match this reviewed native session.');
    }
  }
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
    if (catalog) catalog = { ...catalog, source: 'cached' };
    review = null; reviewedInput = null;
    let terminal: PimProcessEvidence | null = null;
    disposal = disposal.catch(() => {}).then(async () => {
      const receipt = await cancelPimPlan(selected.binding.requirementsDigest);
      if (receipt) { validateCancellation(receipt, selected.binding.requirementsDigest); terminal = receipt.processEvidence; }
    });
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
  onDestroy(() => { identityGeneration += 1; invalidate(); });

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
    if (roleProblems.length || metadataChanges.length) { error = 'Resolve selected-role warnings before reviewing configuration.'; return; }
    rememberSelectedRoles();
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
    identityGeneration += 1; // A late disk-cache load cannot replace fresh generation inventory.
    const polling = ++pollGeneration;
    const timer = setInterval(() => {
      void getPimPlanEvidence(selected.binding.requirementsDigest).then(value => {
        if (current === epoch && polling === pollGeneration && value) acceptEvidence(value, selected);
      }).catch(() => { /* Final operation reply remains authoritative; polling never probes Graph. */ });
    }, 500);
    pollers.add(timer);
    try {
      const fresh = await refreshPimRoles(selected.binding.requirementsDigest);
      if (current !== epoch) return;
      if (fresh.tenantId.toLowerCase() !== selected.binding.tenantId.toLowerCase() || fresh.source !== 'live') throw new Error('Fresh role revalidation returned a different tenant or source.');
      catalog = fresh;
      const problems = [...selectedRoleProblems(fresh, privilegedIds, lessPrivilegedIds), ...roleChanges(selectedMetadata, fresh, [...roleIds(privilegedIds), ...roleIds(lessPrivilegedIds)])];
      if (problems.length) throw new Error('Selected roles changed. ' + problems.join(' '));
      rememberNewSelectedRoles();
      const result = await runPimPlan(reviewedInput, selected.binding.requirementsDigest, fresh.roles.filter(role => [...roleIds(privilegedIds), ...roleIds(lessPrivilegedIds)].includes(role.id)));
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
  </div>
  <PimRolePicker {catalog} {privilegedIds} {lessPrivilegedIds} select={selectRole} />
  <div class="hero-actions"><button class="secondary" disabled={!!busy || !connected} on:click={refreshRoles}>Refresh tenant roles</button></div>
  <p>Role discovery uses the existing owned Graph session and read permissions. Review and sign in with an empty pilot to load roles first, then select roles, configure policy and review again. Configuration changes close the old reviewed session.</p>
  {#if roleNotice}<p role="note">{roleNotice}</p>{/if}
  {#if roleProblems.length}<ul aria-label="Selected role problems">{#each roleProblems as problem}<li>{problem}</li>{/each}</ul>{/if}
  {#if metadataChanges.length}<ul>{#each metadataChanges as change}<li>{change}</li>{/each}</ul><button class="secondary" on:click={acceptRoleChanges}>Accept current role metadata</button>{/if}
  <div class="pim-fields" on:input={invalidate} on:change={invalidate}>
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
      {#if cancellationReceipt.pimPlanRequestInFlightAtRevocation === true}<p>Native planner request was in flight when this cancellation revoked the session.</p>
      {:else if cancellationReceipt.pimPlanRequestInFlightAtRevocation === false}<p>No native planner request was in flight when this cancellation revoked the session.</p>
      {:else}<p>This action has no fresh native in-flight cancellation snapshot; only historical process evidence is available.</p>{/if}
      <p>Review digest: <code>{cancellationReceipt.requirementsDigest}</code></p>
      <p>Process metadata at cancellation processing: {cancellationReceipt.processEvidence.context.pimReadsAttempted} plan dispatch attempts, {cancellationReceipt.processEvidence.context.pimReadsCompleted} validated completions. Worker PID: {cancellationReceipt.processEvidence.processId}.</p>
      <p>This receipt cannot authorize another read. Native request state and dispatch counters do not independently prove that an HTTP transfer was interrupted.</p>
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
      <p>Selected pilot: {[...review.configuration.privilegedRoleIds, ...review.configuration.lessPrivilegedRoleIds].map(id => `${catalog?.roles.find(role => role.id === id)?.displayName ?? 'Manual role'} (${id})`).join(', ') || 'No roles selected — connect to discover roles first.'}</p>
      <p>Emergency groups, notifications and session revocation are excluded.</p>
      <div class="hero-actions"><button class="secondary" disabled={!!busy || connected} on:click={connect}>Approve sign-in</button>
        <button class="primary" disabled={!!busy || !connected || !roleIds(privilegedIds).length && !roleIds(lessPrivilegedIds).length} on:click={generate}>Generate read-only plan</button></div>
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
