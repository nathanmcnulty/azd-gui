<script lang="ts">
  import Icon from "./Icon.svelte";
  import type { OperationEvent } from "../lib/types";

  export let events: OperationEvent[];
  export let activeOperationId: string | null;
  export let activeOperationKind: string | null;
  export let onCancel: () => void | Promise<void>;
  export let onClear: () => void;
  export let onClose: () => void;
</script>

<section class="output-panel" aria-label="Operation output">
  <header>
    <div><Icon name="terminal" size={15} /><strong>OUTPUT</strong><span>azd</span></div>
    <div>
      {#if activeOperationId}<button class="output-cancel" on:click={onCancel}>Cancel {activeOperationKind ?? "operation"}</button>{/if}
      <button title="Clear output" on:click={onClear}>Clear</button>
      <button aria-label="Close output" on:click={onClose}><Icon name="close" size={15} /></button>
    </div>
  </header>
  <div class="output-lines" aria-live="polite">
    {#if events.length === 0}<div class="empty-output">Run a check or deployment to see output.</div>{/if}
    {#each events as event}
      <div class={`output-line ${event.level}`}>
        <time>{new Date(event.timestamp).toLocaleTimeString([], { hour12: false })}</time>
        <span class="stream">{event.stream ?? "system"}</span>
        <span>{event.message}</span>
      </div>
    {/each}
  </div>
</section>
