<script lang="ts">
  import deploymentCheckLock from "../../catalog/component-locks/deployment-check.json";
  import type { CatalogEntry } from "../lib/types";

  export let entry: CatalogEntry;

  // The bundled lock is a byte-for-byte snapshot of this template path and commit.
  $: matchesReviewedPin = entry.source === "bundled" && entry.trusted === true &&
    entry.id === "deployment-check" &&
    entry.repository === "https://github.com/nathanmcnulty/azd-reference" &&
    entry.commit === "abc6e896620990e68360a63dd14efda66a3f75cd" &&
    entry.subdirectory === "examples/deployment-check";
</script>

{#if matchesReviewedPin}
  <section class="workspace-panel full" aria-label="Declared template components">
    <div class="section-toolbar">
      <div>
        <h2>Declared template components</h2>
        <p>From the reviewed template lock. Local file integrity is checked separately after preparation.</p>
      </div>
    </div>
    {#each deploymentCheckLock.components as component (component.id)}
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
