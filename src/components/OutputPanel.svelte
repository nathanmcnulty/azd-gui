<script lang="ts">
  import Icon from "./Icon.svelte";
  import type { OperationEvent } from "../lib/types";

  export let events: OperationEvent[];
  export let activeOperationId: string | null;
  export let activeOperationKind: string | null;
  export let maximized: boolean;
  export let onCancel: () => void | Promise<void>;
  export let onClear: () => void;
  export let onClose: () => void;
  export let onResizeStart: (event: PointerEvent) => void;
  export let onResizeKeydown: (event: KeyboardEvent) => void;
  export let onToggleMaximize: () => void;
</script>

<section class:maximized class="output-panel" aria-label="Operation output">
  <button
    type="button"
    class="output-resize-handle"
    aria-label="Resize output"
    title="Drag or use arrow keys to resize output"
    tabindex={maximized ? -1 : 0}
    on:pointerdown={onResizeStart}
    on:keydown={onResizeKeydown}
  ></button>
  <header>
    <div><Icon name="terminal" size={15} /><strong>OUTPUT</strong><span>azd</span></div>
    <div>
      {#if activeOperationId}<button class="output-cancel" on:click={onCancel}>Cancel {activeOperationKind ?? "operation"}</button>{/if}
      <button title="Clear output" on:click={onClear}>Clear</button>
      <button
        class="output-maximize"
        aria-label={maximized ? "Restore output size" : "Maximize output"}
        title={maximized ? "Restore output size" : "Maximize output"}
        on:click={onToggleMaximize}
      ><Icon name="maximize" size={14} /></button>
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
