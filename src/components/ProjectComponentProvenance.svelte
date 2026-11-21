<script lang="ts">
  import { onDestroy } from "svelte";
  import Icon from "./Icon.svelte";
  import StatusPill from "./StatusPill.svelte";
  import { getProjectComponentProvenance } from "../lib/bridge";
  import { componentReasonText, componentStatusLabels, type ComponentProvenanceRead } from "../lib/component-provenance";
  import type { ProjectSummary } from "../lib/types";

  export let project: ProjectSummary;
  const absent: ComponentProvenanceRead = { state: "notFound", unknownMetadata: false, components: [] };
  let result = absent;
  let loading = false;
  let request = 0;
  let currentPath = "";

  async function refresh(path = project.path) {
    const requestId = ++request;
    if (path !== currentPath) result = absent;
    currentPath = path;
    loading = true;
    try {
      const next = await getProjectComponentProvenance(path);
      if (requestId === request) result = next;
    } catch {
      if (requestId === request) result = { ...absent, state: "unsupported", reason: "unreadable" };
    } finally {
      if (requestId === request) loading = false;
    }
  }

  $: void refresh(project.path);
  onDestroy(() => { request += 1; });
</script>

{#if result.state !== "notFound"}
  <section class="workspace-panel full component-provenance" aria-label="Shared component provenance" aria-busy={loading}>
    <div class="section-toolbar">
      <div><h2>Shared components</h2><p>Installed provenance and local file integrity.</p></div>
      {#if loading}<StatusPill tone="accent" label="Reading" />{/if}
    </div>
    {#if loading}
      <p class="muted-copy">Checking the selected project's managed files…</p>
    {:else if result.state === "unsupported"}
      <div class="callout warning"><Icon name="warning" size={17} /><div><strong>Unsupported component metadata</strong><p>{componentReasonText[result.reason ?? "invalidMetadata"]}</p>{#if result.manifestVersion}<p>Manifest version: {result.manifestVersion}</p>{/if}</div></div>
    {:else}
      <p class="muted-copy">Verified means local bytes match the lock's SHA-256 hashes. The lock does not authenticate its source or establish that a newer version is available.</p>
      <p class="muted-copy">Only declared managed files are checked. Solution-owned extensions are outside this check.</p>
      {#if result.unknownMetadata || result.components.some((component) => component.unknownMetadata)}
        <p class="callout warning">Additional metadata is unsupported and was ignored. Only the recognized 1.0 fields are shown and checked.</p>
      {/if}
      {#if result.components.length === 0}
        <p>No shared components are declared in this lock.</p>
      {/if}
      {#each result.components as component}
        <article aria-label={`Component ${component.id}`}>
          <div class="section-toolbar">
            <h3>{component.id}</h3>
            <StatusPill tone={component.status === "verified" ? "neutral" : "warning"} label={componentStatusLabels[component.status]} />
          </div>
          <dl class="summary-list">
            <div><dt>Installed version</dt><dd>{component.version}</dd></div>
            <div><dt>Source repository (declared)</dt><dd>{component.sourceRepository}</dd></div>
            <div><dt>Source revision (declared)</dt><dd><code>{component.sourceRevision}</code></dd></div>
          </dl>
          <details>
            <summary>Managed files ({component.files.length})</summary>
            <ul aria-label={`Managed files for ${component.id}`}>
              {#each component.files as file}
                <li>
                  <p><code>{file.target}</code> <StatusPill tone={file.status === "verified" ? "neutral" : "warning"} label={componentStatusLabels[file.status]} /></p>
                  <dl class="summary-list">
                    <div><dt>Source path</dt><dd><code>{file.source}</code></dd></div>
                    <div><dt>Declared SHA-256</dt><dd><code>{file.sha256 || "Unsupported hash"}</code></dd></div>
                  </dl>
                  {#if file.reason}<p class="muted-copy">{componentReasonText[file.reason]}</p>{/if}
                </li>
              {/each}
            </ul>
          </details>
        </article>
      {/each}
    {/if}
    <div class="project-deployment-actions">
      <button class="secondary compact" disabled={loading} on:click={() => refresh()}><Icon name="refresh" size={14} /> Refresh components</button>
    </div>
  </section>
{/if}

<style>
  .component-provenance article { margin-block: 1rem; }
  .component-provenance dd, .component-provenance code { overflow-wrap: anywhere; }
  .component-provenance li { margin-block: 1rem; }
</style>
