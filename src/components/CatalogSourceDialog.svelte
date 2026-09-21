<script lang="ts">
  import { onMount } from "svelte";
  import Icon from "./Icon.svelte";
  import StatusPill from "./StatusPill.svelte";
  import { trapFocus } from "../lib/trap-focus";
  import type { ExternalCatalogPreview } from "../lib/types";

  export let sourceUrl: string;
  export let preview: ExternalCatalogPreview | null;
  export let error: string;
  export let onClose: () => void;
  export let onPreview: () => void | Promise<void>;
  export let onTrust: () => void | Promise<void>;

  let dialog: HTMLDialogElement;
  let sourceInput: HTMLInputElement;

  onMount(() => {
    const previousFocus = document.activeElement as HTMLElement | null;
    if (typeof dialog.showModal === "function") dialog.showModal();
    else dialog.setAttribute("open", "");
    sourceInput.focus();
    return () => {
      if (dialog.open && typeof dialog.close === "function") dialog.close();
      queueMicrotask(() => previousFocus?.isConnected && previousFocus.focus());
    };
  });

  function handleKeydown(event: KeyboardEvent) {
    if (event.key === "Escape") {
      event.preventDefault();
      onClose();
    }
  }
</script>

<dialog bind:this={dialog} use:trapFocus class="source-dialog" aria-label="Add catalog source" aria-modal="true" on:keydown={handleKeydown}>
  <header>
    <div><span class="eyebrow">EXTERNAL CATALOG</span><h2>Review this catalog source</h2></div>
    <button class="icon-button" aria-label="Close" on:click={onClose}><Icon name="close" size={16} /></button>
  </header>
  <p>Enter a direct HTTPS catalog URL or a GitHub repository containing <code>azd-gui.catalog.json</code>.</p>
  <label class="field">
    <span>Catalog URL</span>
    <input bind:this={sourceInput} bind:value={sourceUrl} type="url" required placeholder="https://github.com/owner/catalog-repository" />
  </label>
  {#if error}
    <div class="callout warning"><Icon name="warning" size={16} /><div><strong>Could not preview source</strong><p>{error}</p></div></div>
  {/if}
  {#if preview}
    <div class="catalog-preview">
      <div><strong>{preview.catalog.title}</strong><span>{preview.catalog.entries.length} pinned template(s)</span></div>
      <StatusPill tone="warning" label="Review required" />
    </div>
    <div class="preview-entries">
      {#each preview.catalog.entries.slice(0, 4) as entry}
        <div><span>{entry.title}</span><code>{entry.commit.slice(0, 8)}</code></div>
      {/each}
    </div>
    <div class="callout warning"><Icon name="warning" size={16} /><div><strong>Review before adding</strong><p>Templates can run hooks during deployment. Verify the source and pinned revisions before adding it.</p></div></div>
  {/if}
  <footer>
    <button class="secondary" on:click={onClose}>Cancel</button>
    <div class="footer-spacer"></div>
    {#if preview}
      <button class="primary" on:click={onTrust}>Review and add</button>
    {:else}
      <button class="primary" disabled={!sourceUrl.trim()} on:click={onPreview}>Preview source</button>
    {/if}
  </footer>
</dialog>
