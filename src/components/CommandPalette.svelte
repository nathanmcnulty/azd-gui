<script lang="ts">
  import Icon from "./Icon.svelte";

  interface Command {
    label: string;
    action: () => void;
  }

  export let query: string;
  export let commands: Command[];
  export let onClose: () => void;

  function run(command: Command) {
    command.action();
    query = "";
    onClose();
  }
</script>

<div class="modal-backdrop" role="presentation" on:click={onClose}>
  <dialog open class="command-palette" aria-label="Command palette" on:click|stopPropagation>
    <label><Icon name="search" size={17} /><input id="command-query" bind:value={query} placeholder="Type a command" /></label>
    <div>
      {#each commands.filter((command) => command.label.toLowerCase().includes(query.toLowerCase())) as command}
        <button on:click={() => run(command)}>
          <Icon name="chevron" size={14} /><span>{command.label}</span>
        </button>
      {/each}
    </div>
    <footer><span><kbd>↑↓</kbd> navigate</span><span><kbd>Enter</kbd> select</span><span><kbd>Esc</kbd> close</span></footer>
  </dialog>
</div>
