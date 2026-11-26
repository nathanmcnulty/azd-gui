<script lang="ts">
  import { onDestroy } from "svelte";
  import Icon from "./Icon.svelte";
  import { openExternalUrl } from "../lib/bridge";
  import { checkComponentUpdates, type ComponentUpdates } from "../lib/component-updates";
  import { reviewedCatalogComponentSnapshot } from "../lib/catalog-component-locks";
  import type { CatalogEntry } from "../lib/types";

  export let entry: CatalogEntry;
  let guideOpenFailed = false;
  let updates: ComponentUpdates | null = null;
  let checking = false;
  let request = 0;
  let expiry: ReturnType<typeof setTimeout> | undefined;

  $: snapshot = reviewedCatalogComponentSnapshot(entry);
  $: if (entry) guideOpenFailed = false;
  $: void refreshUpdates(snapshot);

  async function refreshUpdates(selected: ReturnType<typeof reviewedCatalogComponentSnapshot>, force = false) {
    const id = ++request;
    clearTimeout(expiry);
    updates = null;
    checking = !!selected?.lock.components.length;
    if (!selected || !checking) return;
    const next = await checkComponentUpdates(selected.lock.components, force);
    if (id !== request) return;
    updates = next;
    checking = false;
    if (next.state === "fresh" && next.checkedAt) {
      expiry = setTimeout(() => {
        if (id === request) updates = { ...next, state: "stale" };
      }, Math.max(0, Date.parse(next.checkedAt) + 15 * 60_000 - Date.now()));
    }
  }

  onDestroy(() => { request += 1; clearTimeout(expiry); });

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
      <div>
        {#if snapshot.lock.components.length}
          <button class="secondary compact" disabled={checking} on:click={() => refreshUpdates(snapshot, true)}>Check for updates</button>
        {/if}
        {#if snapshot.updateGuideUrl}
          <button class="secondary compact" on:click={openUpdateGuide}>Open update guide</button>
        {/if}
      </div>
    </div>
    {#if guideOpenFailed}
      <p class="callout warning" role="alert">Could not open the update guide.</p>
    {/if}
    {#if checking}
      <p role="status">Checking shared component updates.</p>
    {:else if updates?.state === "stale"}
      <p class="callout warning">Update information is stale. Use Check for updates to refresh.</p>
    {:else if updates?.state === "unavailable"}
      <p>Update information is unavailable.</p>
    {/if}
    {#each snapshot.lock.components as component, index (component.id)}
      {@const update = updates?.state === "fresh" && updates.updates[index]?.id === component.id ? updates.updates[index] : undefined}
      <article aria-label={component.id}>
        <h3>{component.id}</h3>
        <dl class="summary-list">
          <div><dt>Declared version</dt><dd>{component.version}</dd></div>
          <div><dt>Declared source repository</dt><dd>{component.sourceRepository}</dd></div>
          <div><dt>Declared source revision</dt><dd><code>{component.sourceRevision}</code></dd></div>
        </dl>
        {#if update?.state === "updateAvailable"}
          <p>Available version: <strong>{update.version}</strong></p>
          {#if update.compatibility === "breaking"}
            <p class="callout warning"><Icon name="warning" size={17} /> Breaking changes declared in {update.breakingVersions.join(", ")}.</p>
            {#each update.migrationNotes as note}<p>{note}</p>{/each}
          {:else if update.compatibility === "unknown"}
            <p>Update compatibility is unknown.</p>
          {/if}
          {#if update.historyIncomplete || !update.complete}
            <p>Some release or compatibility information could not be verified.</p>
          {/if}
        {/if}
      </article>
    {/each}
  </section>
{/if}

<style>
  dd { overflow-wrap: anywhere; }
</style>
