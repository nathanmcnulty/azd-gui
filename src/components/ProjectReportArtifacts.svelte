<script lang="ts">
  import { onDestroy } from "svelte";
  import { getProjectReportArtifacts } from "../lib/bridge";
  import { artifactStateLabels, type ArtifactRead } from "../lib/report-artifacts";
  import type { ProjectSummary } from "../lib/types";
  export let project: ProjectSummary;
  // Identity-only invalidation signal; never serialize or log configuration values.
  export let reportContext: unknown = null;
  let previousContext: unknown;
  const empty = (): ArtifactRead => ({ context: null, observedAt: "", artifacts: [] });
  let result = empty();
  let loading = false;
  let failed = false;
  let sequence = 0;
  let binding = "";
  let stale = false;
  function projectBinding(p: ProjectSummary) { return JSON.stringify([p.path, p.environment, p.revision, p.lastOperation, p.state]); }
  async function refresh(current = projectBinding(project), revealId: string | null = null) {
    const token = ++sequence;
    const digest = revealId ? result.context?.snapshotDigest ?? null : null;
    // Clear previous payloads immediately, including after context changes or hiding.
    result = empty(); loading = true; failed = false;
    const path = project.path;
    try {
      const next = await getProjectReportArtifacts(path, revealId, digest);
      if (token !== sequence || current !== projectBinding(project)) return;
      result = next; stale = false;
    } catch { if (token === sequence) failed = true; }
    finally { if (token === sequence) loading = false; }
  }
  $: if (projectBinding(project) !== binding || reportContext !== previousContext) {
    previousContext = reportContext;
    binding = projectBinding(project); stale = true; void refresh(binding);
  }
  onDestroy(() => { ++sequence; result = empty(); });
</script>

{#if loading || failed || result.artifacts.length > 0}
<section class="workspace-panel full report-artifacts" aria-label="Declared deployment reports" aria-busy={loading}>
  <h2>Deployment reports</h2>
  <p>Deployment success is not operational verification. Report contents are untrusted evidence, not permission to run an action.</p>
  <p>These files are not proven to belong to the current deployment, tenant or configuration. Their freshness and verification remain unproven.</p>
  {#if loading}<p>{stale ? "Context changed. Discarding stale content and reading current reports…" : "Reading reports…"}</p>
  {:else if failed}<p role="alert">Reports are unavailable. No verification result is implied.</p>
  {:else}
    <dl><dt>Selected environment</dt><dd>{result.context?.environment ?? "Unbound"}</dd><dt>Selected source revision</dt><dd>{result.context?.revision ?? "Unbound"}</dd><dt>Observed at</dt><dd>{result.observedAt}</dd></dl>
    {#each result.artifacts as artifact (artifact.declaration.id)}
      <article aria-label={artifact.declaration.label}>
        <h3>{artifact.declaration.label}</h3>
        <p>{artifact.declaration.path} · {artifact.declaration.stage} · {artifact.declaration.required ? "Required report" : "Optional report"}</p>
        <p>{artifactStateLabels[artifact.state]}</p>
        {#if artifact.modifiedAt}<p>File modified at: {artifact.modifiedAt} (not proof of generation time)</p>{/if}
        {#if artifact.sha256}<p>Observed SHA-256: <code>{artifact.sha256}</code></p>{/if}
        {#if artifact.state === "hidden"}
          <button class="secondary compact" on:click={() => refresh(binding, artifact.declaration.id)}>Reveal sensitive report: {artifact.declaration.label}</button>
        {:else if artifact.content !== undefined}
          <p>{artifact.declaration.format === "markdown" ? "Markdown shown as inert text; links and HTML are disabled." : "Read-only report text"}</p>
          <pre>{artifact.content}</pre>
          {#if artifact.declaration.sensitive}<button class="secondary compact" on:click={() => refresh()}>Hide sensitive content</button>{/if}
        {/if}
      </article>
    {/each}
  {/if}
  <button class="secondary compact" disabled={loading} on:click={() => refresh()}>Refresh reports</button>
</section>
{/if}
<style>
  .report-artifacts article { margin-block: 1rem; }
  .report-artifacts pre { max-height: 24rem; overflow: auto; white-space: pre-wrap; overflow-wrap: anywhere; }
  .report-artifacts dd, .report-artifacts code { overflow-wrap: anywhere; }
</style>
