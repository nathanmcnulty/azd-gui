<script lang="ts">
  import { onDestroy } from "svelte";
  import {
    getProjectApplicationReviewContext, verifyProjectApplication, confirmProjectApplicationReview,
    type ApplicationTarget, type ApplicationReviewContext, type ApplicationVerification, type ApplicationReviewReceipt
  } from "../lib/application-ownership";

  export let projectPath: string;
  export let environment: string;
  export let sourceContractDigest: string;
  export let context: unknown;
  export let inputsSaved = true;

  const explanations = {
    unavailable: "A verified application read is unavailable.",
    accessDenied: "The application read was denied.",
    incomplete: "The application response was incomplete.",
    cancelled: "The application read was cancelled.",
    differentTenant: "The verification used a different tenant.",
    differentOperator: "The verified operator changed.",
    differentObject: "The returned object ID did not match.",
    differentApplication: "The returned application ID did not match.",
    unexpectedProperties: "The application properties did not match the required configuration."
  };
  const bindingKeys = [
    "tenantId", "subscriptionId", "environment", "targetResourceDigest",
    "sourceContractDigest", "inputsDigest", "objectId", "expectedApplicationId"
  ] as const;
  function matchesTarget(binding: ApplicationTarget, target: ApplicationTarget): boolean {
    return bindingKeys.every((key) => binding[key] === target[key]);
  }

  let lastKey = "";
  let generation = 0;
  let busy = false;
  let accepted = false;
  let target: ApplicationReviewContext | null = null;
  let result: Extract<ApplicationVerification, { state: "verified" }> | null = null;
  let receipt: ApplicationReviewReceipt | null = null;
  let status = "";
  $: key = JSON.stringify([projectPath, environment, sourceContractDigest, context, inputsSaved]);
  $: if (key !== lastKey) reset(key);

  function reset(value: string) {
    const changed = lastKey !== "" && (target !== null || busy);
    lastKey = value;
    generation += 1;
    busy = false;
    clearEvidence();
    target = null;
    if (inputsSaved && environment) {
      void loadContext(changed ? "The saved context changed. Verify again." : undefined);
    } else {
      status = changed ? "The saved context changed. Save your inputs before verifying." : "Save the target and stable application IDs before verifying.";
    }
  }

  function clearEvidence() {
    accepted = false;
    result = null;
    receipt = null;
  }

  async function loadContext(message?: string) {
    const current = ++generation;
    clearEvidence();
    target = null;
    busy = true;
    status = "Loading the saved application target.";
    try {
      const saved = await getProjectApplicationReviewContext(projectPath, environment);
      if (current !== generation) return;
      if (saved.sourceContractDigest !== sourceContractDigest || saved.environment !== environment) {
        status = "The saved project source or environment changed. Reopen the project to review it.";
        return;
      }
      target = saved;
      status = message ?? "Verify the saved application ID to review its properties.";
    } catch {
      if (current === generation) status = "The saved application target is unavailable. Save the tenant, subscription, resource group and stable IDs, then reload.";
    } finally {
      if (current === generation) busy = false;
    }
  }

  async function verify() {
    if (!target || !inputsSaved || busy) return;
    const displayed = target;
    const current = ++generation;
    busy = true;
    clearEvidence();
    status = "Checking the saved application.";
    try {
      const next = await verifyProjectApplication({ projectPath, environment, contextDigest: displayed.contextDigest });
      if (current !== generation) return;
      if (next.state !== "verified") {
        status = explanations[next.reason];
      } else if (!matchesTarget(next.binding, displayed) || next.properties.objectId !== displayed.objectId
        || (displayed.expectedApplicationId && next.properties.applicationId !== displayed.expectedApplicationId)) {
        await loadContext("The returned application or target did not match this review. Verify again.");
      } else {
        result = next;
        status = "Application properties verified. Review the comparison before recording a decision.";
      }
    } catch {
      if (current === generation) await loadContext("The review target could not be verified. Review the saved target and verify again.");
    } finally {
      if (current === generation) busy = false;
    }
  }

  async function review() {
    if (!target || !inputsSaved || busy || !accepted || !result) return;
    const displayed = target;
    const verification = result;
    const current = ++generation;
    busy = true;
    accepted = false;
    try {
      const next = await confirmProjectApplicationReview(
        { projectPath, environment, contextDigest: displayed.contextDigest }, verification.verificationDigest
      );
      if (current !== generation) return;
      if (!matchesTarget(next.binding, displayed) || next.binding.operatorDigest !== verification.binding.operatorDigest
        || next.verificationDigest !== verification.verificationDigest || next.verifiedAt !== verification.checkedAt
        || next.applicationId !== verification.properties.applicationId) {
        throw new Error("Stale review response");
      }
      receipt = next;
      result = null;
      status = "Review decision recorded for this session. Ownership is unchanged.";
    } catch {
      if (current === generation) await loadContext("The review could not be recorded. Review the saved target and verify again.");
    } finally {
      if (current === generation) busy = false;
    }
  }
  onDestroy(() => { generation += 1; });
</script>

<section class="workspace-panel full" aria-label="Application ownership review">
  <h2>Review an existing application registration</h2>
  {#if target}
    <dl class="summary-list columns">
      <div><dt>Saved tenant</dt><dd>{target.tenantId}</dd></div>
      <div><dt>Saved subscription</dt><dd>{target.subscriptionId}</dd></div>
      <div><dt>Saved resource group</dt><dd>{target.resourceGroup}</dd></div>
      <div><dt>Saved object ID</dt><dd>{target.objectId}</dd></div>
      <div><dt>Expected application ID</dt><dd>{target.expectedApplicationId ?? "Read by object ID"}</dd></div>
    </dl>
  {/if}
  <p role="status">{status}</p>
  <button class="secondary" disabled={!target || !inputsSaved || busy} on:click={verify}>Verify existing application</button>
  {#if !target && inputsSaved && environment && !busy}
    <button class="secondary" on:click={() => loadContext()}>Reload saved application target</button>
  {/if}
  {#if result}
    <dl class="summary-list columns">
      <div><dt>Verified application ID</dt><dd>{result.properties.applicationId}</dd></div>
      <div><dt>Audience</dt><dd>Single tenant</dd></div>
      <div><dt>Identifier URI</dt><dd>Matches the expected URI</dd></div>
      <div><dt>Access-token version</dt><dd>{result.properties.accessTokenVersion}</dd></div>
      <div><dt>Verified at</dt><dd><time datetime={result.checkedAt}>{result.checkedAt}</time></dd></div>
    </dl>
    <label><input type="checkbox" bind:checked={accepted} /> I reviewed this exact verified application</label>
    <button class="secondary" disabled={!accepted || busy} on:click={review}>Record review decision</button>
  {/if}
  {#if receipt}
    <details><summary>Review receipt</summary><pre>{JSON.stringify(receipt, null, 2)}</pre></details>
  {/if}
</section>
