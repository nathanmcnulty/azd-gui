<script lang="ts">
  import type { ExternalTerminalLaunch } from '../lib/types';
  import Icon from './Icon.svelte';
  export let handoff: ExternalTerminalLaunch;
  export let busy = false;
  export let onCopy: (text: string) => void;
  export let onRetry: () => void;
  export let onDismiss: () => void;
</script>

<section class="terminal-launch-notice" aria-label="Terminal handoff">
  <Icon name="terminal" size={18} />
  <div>
  <h2>{handoff.opened ? handoff.historical ? 'Last terminal handoff opened' : 'External terminal opened' : 'Terminal not opened'}</h2>
  <p><strong>Prepared, deployment not observed.</strong> Opening a terminal does not run the command or prove deployment.</p>
  {#if handoff.detail}<p>{handoff.detail}</p>{/if}
  <p>Project directory: <code>{handoff.workingDirectory}</code></p>
  <p>Reviewed command: <code>{handoff.command}</code></p>
  <p>Use this directory in your terminal. Review the source, account and selected environment before running the command yourself. Retry opens a terminal only; it does not prepare the project again.</p>
  </div>
  <div class="terminal-launch-actions">
    <button class="secondary compact" on:click={() => onCopy(handoff.workingDirectory)}>Copy project directory</button>
    <button class="secondary compact" on:click={() => onCopy(handoff.command)}>Copy command</button>
    <button class="secondary compact" disabled={busy} on:click={onRetry}>Retry terminal handoff</button>
    <button class="secondary compact" on:click={onDismiss} aria-label="Dismiss terminal notice">Dismiss</button>
  </div>
</section>

<style>
  .terminal-launch-notice {
    position: static;
    width: auto;
    margin: 24px 32px;
    grid-template-columns: 20px minmax(0, 1fr);
    align-items: start;
  }
  .terminal-launch-actions {
    grid-column: 2;
    flex-wrap: wrap;
  }
  .terminal-launch-notice code {
    overflow: visible;
    white-space: pre-wrap;
    overflow-wrap: anywhere;
  }
  h2 { margin: 0; font-size: 16px; }
  p { margin: 6px 0; }
</style>
