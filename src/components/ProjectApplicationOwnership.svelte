<script lang="ts">
  import { onDestroy } from "svelte";
  import { verifyProjectApplication, confirmProjectApplicationReview, type ApplicationVerification, type ApplicationReviewReceipt } from "../lib/application-ownership";

  export let projectPath: string;
  export let environment: string;
  export let tenantId: string;
  export let objectId: string;
  export let expectedApplicationId = "";
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
  const isId = (value: string) => /^[0-9a-f]{8}(?:-[0-9a-f]{4}){3}-[0-9a-f]{12}$/i.test(value)
    && value !== "00000000-0000-0000-0000-000000000000";
  let lastKey = "";
  let generation = 0;
  let busy = false;
  let accepted = false;
  let result: ApplicationVerification | null = null;
  let receipt: ApplicationReviewReceipt | null = null;
  let status = "Verify the saved application ID to review its properties.";
  $: key = JSON.stringify([projectPath, environment, tenantId, objectId, expectedApplicationId, context, inputsSaved]);
  $: if (key !== lastKey) reset(key);
  $: canVerify = inputsSaved && !!environment && isId(tenantId) && isId(objectId) && (!expectedApplicationId || isId(expectedApplicationId));

  function reset(value: string) {
    const hadEvidence = busy || result !== null || receipt !== null;
    lastKey = value;
    generation += 1;
    busy = false;
    accepted = false;
    result = null;
    receipt = null;
    if (hadEvidence) status = "The saved context changed. Verify again.";
  }

  async function verify() {
    if (!canVerify || busy) return;
    const current = ++generation;
    busy = true;
    accepted = false;
    receipt = null;
    result = null;
    status = "Checking the saved application.";
    try {
      const next = await verifyProjectApplication(projectPath, environment);
      if (current !== generation) return;
      if (next.state === "verified" && (next.binding.tenantId !== tenantId.toLowerCase() || next.binding.environment !== environment || next.properties.objectId !== objectId.toLowerCase()
        || (expectedApplicationId && next.properties.applicationId !== expectedApplicationId.toLowerCase()))) {
        status = "The returned application did not match this review.";
        return;
      }
      result = next;
      status = next.state === "verified" ? "Application properties verified. Review the comparison before recording a decision."
        : explanations[next.reason];
    } catch {
      if (current === generation) status = "The saved application could not be verified. Check the saved target and try again.";
    } finally {
      if (current === generation) busy = false;
    }
  }

  async function review() {
    if (!canVerify || busy || !accepted || result?.state !== "verified") return;
    const current = ++generation;
    const digest = result.verificationDigest;
    busy = true;
    accepted = false;
    try {
      const next = await confirmProjectApplicationReview(projectPath, environment, digest);
      if (current !== generation) return;
      if (next.verificationDigest !== digest || next.binding.objectId !== objectId.toLowerCase()
        || next.binding.tenantId !== tenantId.toLowerCase() || next.binding.environment !== environment) {
        throw new Error("Stale review response");
      }
      receipt = next;
      result = null;
      status = "Review decision recorded for this session. Ownership is unchanged.";
    } catch {
      if (current === generation) {
        result = null;
        status = "The review could not be recorded. Verify the current application again.";
      }
    } finally {
      if (current === generation) busy = false;
    }
  }
  onDestroy(() => { generation += 1; });
</script>

<section class="workspace-panel full" aria-label="Application ownership review">
  <h2>Review an existing application registration</h2>
  <dl class="summary-list columns">
    <div><dt>Saved tenant</dt><dd>{tenantId || "Save an exact tenant"}</dd></div>
    <div><dt>Saved object ID</dt><dd>{objectId || "Save an exact application object ID"}</dd></div>
    <div><dt>Expected application ID</dt><dd>{expectedApplicationId || "Read by object ID"}</dd></div>
  </dl>
  <p role="status">{status}</p>
  <button class="secondary" disabled={!canVerify || busy} on:click={verify}>Verify existing application</button>
  {#if !canVerify}<p>Save the tenant and stable application IDs before verifying.</p>{/if}
  {#if result?.state === "verified"}
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
