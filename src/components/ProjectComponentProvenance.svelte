<script lang="ts">
  import { onDestroy } from "svelte";
  import Icon from "./Icon.svelte";
  import StatusPill from "./StatusPill.svelte";
  import { getProjectComponentProvenance, getTemplateComponentProvenance } from "../lib/bridge";
  import { checkComponentUpdates, type ComponentUpdates } from "../lib/component-updates";
  import { componentReasonText, componentStatusLabels, type ComponentProvenanceRead } from "../lib/component-provenance";
  import type { LocalSourceBinding, ProjectSummary } from "../lib/types";

  export let project: ProjectSummary | undefined = undefined;
  export let template: LocalSourceBinding | undefined = undefined;
  const absent: ComponentProvenanceRead = { state: "notFound", unknownMetadata: false, components: [] };
  let result = absent;
  let loading = false;
  let request = 0;
  let currentSource = "";
  let updates: ComponentUpdates | null = null;
  let checking = false;
  let updateRequest = 0;
  let expiry: ReturnType<typeof setTimeout> | undefined;

  async function refreshUpdates(force = false) {
    if (checking || result.state !== "available" || !result.components.length) return;
    const id = ++updateRequest;
    checking = true;
    const next = await checkComponentUpdates(result.components, force);
    if (id !== updateRequest) return;
    updates = next;
    checking = false;
    clearTimeout(expiry);
    if (next.state === "fresh" && next.checkedAt) {
      const remaining = Date.parse(next.checkedAt) + 15 * 60_000 - Date.now();
      expiry = setTimeout(() => {
        if (id === updateRequest && updates) updates = { ...updates, state: "stale" };
      }, Math.max(0, remaining));
    }
  }

  async function refresh(source: string, path: string, binding?: LocalSourceBinding, forceUpdates = false) {
    if (forceUpdates && (loading || checking)) return;
    const requestId = ++request;
    ++updateRequest;
    clearTimeout(expiry);
    checking = false;
    updates = null;
    if (source !== currentSource) result = absent;
    currentSource = source;
    if (!path && !binding) {
      loading = false;
      return;
    }
    loading = true;
    try {
      const next = binding
        ? await getTemplateComponentProvenance(binding)
        : await getProjectComponentProvenance(path);
      if (requestId === request) {
        result = next ?? absent;
        void refreshUpdates(forceUpdates);
      }
    } catch {
      if (requestId === request) result = { ...absent, state: "unsupported", reason: "unreadable" };
    } finally {
      if (requestId === request) loading = false;
    }
  }

  $: sourceKey = template ? template.sourcePath + "|" + template.contentDigest : project?.path ?? "";
  $: if (sourceKey !== currentSource) void refresh(sourceKey, project?.path ?? "", template);
  onDestroy(() => { request += 1; updateRequest += 1; clearTimeout(expiry); });
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
      {#if checking}
        <p class="muted-copy" role="status">Checking shared component updates…</p>
      {/if}
      {#if updates?.state === "stale"}
        <p class="callout warning">Update information is stale. Last successful check: {updates.checkedAt}.</p>
      {/if}
      {#each result.components as component}
        {@const update = updates?.updates.find((item) => item.id === component.id)}
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
          {#if update?.historyIncomplete}
            <p class="callout warning">Release history is incomplete. Some versions could not be checked.</p>
          {/if}
          {#if update?.state === "updateAvailable"}
            <p>Available version: <strong>{update.version}</strong></p>
            {#if update.compatibility === "breaking"}
              <p class="callout warning"><Icon name="warning" size={17} /> Breaking changes declared in {update.breakingVersions.join(", ")}.</p>
              {#each update.migrationNotes as note}<p>{note}</p>{/each}
              {#if !update.complete}<p>Compatibility information is incomplete for other versions in this update.</p>{/if}
            {:else if update.compatibility === "noBreakingDeclared"}
              <p>The publisher declared no breaking changes for the releases in this update.</p>
            {:else}
              <p>Update compatibility is unknown.</p>
            {/if}
          {:else if update?.state === "noNewer" && updates?.state === "fresh"}
            <p>No newer verified release found.</p>
          {:else if !checking || updates}
            <p>Update information is unavailable for this component.</p>
          {/if}
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
      {#if result.state === "available" && result.components.length}
        <button class="secondary compact" disabled={checking || loading} on:click={() => refresh(sourceKey, project?.path ?? "", template, true)}><Icon name="refresh" size={14} /> Check for updates</button>
      {/if}
      <button class="secondary compact" disabled={loading} on:click={() => refresh(sourceKey, project?.path ?? "", template)}><Icon name="refresh" size={14} /> Refresh components</button>
    </div>
  </section>
{/if}

<style>
  .component-provenance article { margin-block: 1rem; }
  .component-provenance dd, .component-provenance code { overflow-wrap: anywhere; }
  .component-provenance li { margin-block: 1rem; }
</style>
