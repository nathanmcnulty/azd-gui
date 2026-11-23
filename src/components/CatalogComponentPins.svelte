<script lang="ts">
  import { openExternalUrl } from "../lib/bridge";
  import { reviewedCatalogComponentSnapshot } from "../lib/catalog-component-locks";
  import type { CatalogEntry } from "../lib/types";

  export let entry: CatalogEntry;
  let guideOpenFailed = false;

  $: snapshot = reviewedCatalogComponentSnapshot(entry);
  $: if (entry) guideOpenFailed = false;

  async function openUpdateGuide() {
    const selected = entry;
    const guideUrl = reviewedCatalogComponentSnapshot(selected)?.updateGuideUrl;
    if (!guideUrl) return;
    guideOpenFailed = false;
    try {
      await openExternalUrl(guideUrl);
    } catch {
      if (entry === selected) guideOpenFailed = true;
    }
  }
</script>

{#if snapshot}
  <section class="workspace-panel full" aria-label="Declared template components">
    <div class="section-toolbar">
      <div>
        <h2>Declared template components</h2>
        <p>From the reviewed template lock. Local file integrity requires checking a registered project.</p>
      </div>
      {#if snapshot.updateGuideUrl}
        <button class="secondary compact" on:click={openUpdateGuide}>Open update guide</button>
      {/if}
    </div>
    {#if guideOpenFailed}
      <p class="callout warning" role="alert">Could not open the update guide.</p>
    {/if}
    {#each snapshot.lock.components as component (component.id)}
      <article aria-label={component.id}>
        <h3>{component.id}</h3>
        <dl class="summary-list">
          <div><dt>Declared version</dt><dd>{component.version}</dd></div>
          <div><dt>Declared source repository</dt><dd>{component.sourceRepository}</dd></div>
          <div><dt>Declared source revision</dt><dd><code>{component.sourceRevision}</code></dd></div>
        </dl>
      </article>
    {/each}
  </section>
{/if}

<style>
  dd { overflow-wrap: anywhere; }
</style>
