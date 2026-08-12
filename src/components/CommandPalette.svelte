<script lang="ts">
  import { onMount } from "svelte";
  import Icon from "./Icon.svelte";
  import { trapFocus } from "../lib/trap-focus";

  interface Command {
    label: string;
    action: () => void;
  }

  export let query: string;
  export let commands: Command[];
  export let onClose: () => void;

  let dialog: HTMLDialogElement;
  let queryInput: HTMLInputElement;
  let selectedIndex = 0;
  $: matchingCommands = commands.filter((command) =>
    command.label.toLowerCase().includes(query.toLowerCase())
  );
  $: if (selectedIndex >= matchingCommands.length) selectedIndex = 0;

  onMount(() => {
    const previousFocus = document.activeElement as HTMLElement | null;
    if (typeof dialog.showModal === "function") dialog.showModal();
    else dialog.setAttribute("open", "");
    queryInput.focus();
    return () => {
      if (dialog.open && typeof dialog.close === "function") dialog.close();
      queueMicrotask(() => previousFocus?.isConnected && previousFocus.focus());
    };
  });

  function run(command: Command) {
    command.action();
    query = "";
    onClose();
  }

  function handleKeydown(event: KeyboardEvent) {
    if (event.key === "Escape") {
      event.preventDefault();
      onClose();
      return;
    }
    if (!matchingCommands.length) return;
    if (event.key === "ArrowDown") {
      event.preventDefault();
      selectedIndex = (selectedIndex + 1) % matchingCommands.length;
    } else if (event.key === "ArrowUp") {
      event.preventDefault();
      selectedIndex = (selectedIndex - 1 + matchingCommands.length) % matchingCommands.length;
    } else if (event.key === "Enter") {
      event.preventDefault();
      run(matchingCommands[selectedIndex]);
    }
  }
</script>

<dialog bind:this={dialog} use:trapFocus class="command-palette" aria-label="Command palette" aria-modal="true" on:keydown={handleKeydown}>
  <label><Icon name="search" size={17} /><input bind:this={queryInput} bind:value={query} on:input={() => (selectedIndex = 0)} placeholder="Type a command" role="combobox" aria-label="Search commands" aria-expanded="true" aria-controls="command-results" aria-activedescendant={matchingCommands.length ? `command-result-${selectedIndex}` : undefined} /></label>
  <div id="command-results" role="listbox" aria-label="Commands">
    {#each matchingCommands as command, index}
      <button id={`command-result-${index}`} role="option" tabindex="-1" aria-selected={index === selectedIndex} on:mouseenter={() => (selectedIndex = index)} on:click={() => run(command)}>
        <Icon name="chevron" size={14} /><span>{command.label}</span>
      </button>
    {/each}
  </div>
  <footer><span><kbd>↑↓</kbd> navigate</span><span><kbd>Enter</kbd> select</span><span><kbd>Esc</kbd> close</span></footer>
</dialog>
