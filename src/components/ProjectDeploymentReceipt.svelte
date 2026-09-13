<script lang="ts">
  import { onDestroy } from "svelte";
  import Icon from "./Icon.svelte";
  import StatusPill from "./StatusPill.svelte";
  import { getProjectDeploymentReceipt } from "../lib/bridge";
  import type { DeploymentReceiptRead, ProjectSummary } from "../lib/types";

  export let project: ProjectSummary;
  let result: DeploymentReceiptRead = { state: "notFound" };
  let loading = false;
  let request = 0;

  async function refresh() {
    const requestId = ++request;
    loading = true;
    try {
      const next = await getProjectDeploymentReceipt(project.path);
      if (requestId === request) result = next;
    } catch {
      if (requestId === request) result = { state: "invalid" };
    } finally {
      if (requestId === request) loading = false;
    }
  }

  $: projectKey = `${project.path}\u0000${project.environment ?? ""}`;
  $: if (projectKey) void refresh();
  $: receipt = result.receipt;
  $: needsReview = Boolean(receipt && (receipt.summary.failed || receipt.summary.warnings || receipt.outstandingActionCount));
  onDestroy(() => { request += 1; });
</script>

<section class="workspace-panel full" aria-label="Deployment receipt">
  <div class="section-toolbar">
    <div><h2>Deployment receipt</h2><p>Actions and follow-up work recorded by the template.</p></div>
    {#if loading}
      <StatusPill tone="accent" label="Reading" />
    {:else if result.state === "available" && receipt}
      <StatusPill tone={needsReview ? "warning" : "neutral"} label={needsReview ? "Needs review" : receipt.mode === "plan" ? "Plan recorded" : "Actions recorded"} />
    {/if}
  </div>

  {#if loading}
    <p class="muted-copy">Reading the selected project's receipt…</p>
  {:else if result.state === "available" && receipt}
    <p>{receipt.mode === "plan" ? "This is a plan. No applied actions are recorded." : "These counts describe recorded actions; they do not establish deployment or verification success."}</p>
    <dl class="summary-list columns">
      <div><dt>Recorded</dt><dd>{new Date(receipt.generatedAt).toLocaleString()}</dd></div>
      <div><dt>Applied</dt><dd>{receipt.summary.applied}</dd></div>
      <div><dt>Unchanged</dt><dd>{receipt.summary.unchanged}</dd></div>
      <div><dt>Skipped</dt><dd>{receipt.summary.skipped}</dd></div>
      <div><dt>Warnings</dt><dd>{receipt.summary.warnings}</dd></div>
      <div><dt>Failed</dt><dd>{receipt.summary.failed}</dd></div>
      <div><dt>Follow-up actions</dt><dd>{receipt.outstandingActionCount}</dd></div>
      <div><dt>Referenced reports</dt><dd>{receipt.artifactCount}</dd></div>
    </dl>
    {#if receipt.outstandingActionCount > 0}
      <p>Review the template's reports for the outstanding actions before considering this deployment complete.</p>
    {/if}
    <p class="muted-copy">This local receipt is not bound to the selected environment, Azure target, or template revision. Confirm those details in the project's records.</p>
  {:else if result.state === "notFound"}
    <p class="muted-copy">No receipt yet. Templates that support receipts write <code>reports/deployment-receipt.json</code>.</p>
  {:else}
    <div class="callout warning"><Icon name="warning" size={17} /><div><strong>Receipt unavailable</strong><p>{result.state === "tooLarge" ? "The receipt exceeds the 1 MiB read limit." : result.state === "unsafePath" ? "The receipt path contains a link or reparse point and was not read." : "The file is not a supported version 1.0 receipt."}</p></div></div>
  {/if}
  <div class="project-deployment-actions">
    <button class="secondary compact" disabled={loading} on:click={refresh}><Icon name="refresh" size={14} /> Refresh receipt</button>
  </div>
</section>
