<script lang="ts">
  import Icon from "./Icon.svelte";
  import type { OperationEvent } from "../lib/types";

  export let events: OperationEvent[];
  export let activeOperationId: string | null;
  export let activeOperationKind: string | null;
  export let maximized: boolean;
  export let height: number;
  export let minimumHeight: number;
  export let maximumHeight: number;
  export let onCancel: () => void | Promise<void>;
  export let onClear: () => void;
  export let onClose: () => void;
  export let onResizeStart: (event: PointerEvent) => void;
  export let onResizeKeydown: (event: KeyboardEvent) => void;
  export let onToggleMaximize: () => void;
</script>

<section class:maximized class="output-panel" aria-label="Operation output">
  <!-- Keyboard-focusable separators are interactive under the WAI-ARIA separator pattern. -->
  <!-- svelte-ignore a11y_no_noninteractive_tabindex a11y_no_noninteractive_element_interactions -->
  <div
    class="output-resize-handle"
    role="separator"
    aria-label="Resize output"
    aria-orientation="horizontal"
    aria-valuemin={minimumHeight}
    aria-valuemax={maximumHeight}
    aria-valuenow={height}
    aria-valuetext={`${height} pixels high`}
    aria-describedby="output-resize-instructions"
    title="Drag, use Up and Down, or press Home and End to resize output"
    tabindex={maximized ? -1 : 0}
    on:pointerdown={onResizeStart}
    on:keydown={onResizeKeydown}
  ></div>
  <span id="output-resize-instructions" class="visually-hidden">Use Up and Down to resize. Home sets the minimum height; End sets the maximum height.</span>
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
  <div class="output-lines" role="log" aria-live="polite">
    {#if events.length === 0}<div class="empty-output">Run a check or deployment to see output.</div>{/if}
    {#each events as event}
      <div class:system={event.stream === "system"} class={`output-line ${event.level}`}>
        {#if event.stream === "system"}<time>{new Date(event.timestamp).toLocaleTimeString([], { hour12: false })}</time>{/if}
        <span class="output-message">{event.message}</span>
      </div>
    {/each}
  </div>
</section>
