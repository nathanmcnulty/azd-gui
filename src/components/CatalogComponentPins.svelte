<script lang="ts">
  import { catalogComponentPins } from "../lib/catalog-component-locks";
  import type { CatalogEntry } from "../lib/types";

  export let entry: CatalogEntry;

  $: components = catalogComponentPins(entry);
</script>

{#if components}
  <section class="workspace-panel full" aria-label="Declared template components">
    <div class="section-toolbar">
      <div>
        <h2>Declared template components</h2>
        <p>From the reviewed template lock. Local file integrity is checked separately after preparation.</p>
      </div>
    </div>
    {#each components as component (component.id)}
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
