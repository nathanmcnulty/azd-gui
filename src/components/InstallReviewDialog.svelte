<script lang="ts">
  import { onMount } from "svelte";
  import Icon from "./Icon.svelte";
  import { trapFocus } from "../lib/trap-focus";
  import type { ToolInstallPlan } from "../lib/types";

  export let plan: ToolInstallPlan;
  export let installing: boolean;
  export let onClose: () => void;
  export let onConfirm: () => void | Promise<void>;
  export let onCopy: (value: string) => void | Promise<void>;
  export let onOpenDocumentation: (url: string) => void | Promise<void>;

  let dialog: HTMLDialogElement;
  let confirmButton: HTMLButtonElement;

  onMount(() => {
    const previousFocus = document.activeElement as HTMLElement | null;
    if (typeof dialog.showModal === "function") dialog.showModal();
    else dialog.setAttribute("open", "");
    confirmButton.focus();
    return () => {
      if (dialog.open && typeof dialog.close === "function") dialog.close();
      queueMicrotask(() => previousFocus?.isConnected && previousFocus.focus());
    };
  });

  function handleKeydown(event: KeyboardEvent) {
    if (event.key === "Escape" && !installing) {
      event.preventDefault();
      onClose();
    }
  }
</script>

<dialog bind:this={dialog} use:trapFocus class="source-dialog install-dialog" aria-label={`Review ${plan.toolName} installation`} aria-describedby="install-review-description" aria-modal="true" on:keydown={handleKeydown}>
  <header>
    <div><span class="eyebrow">INSTALLATION APPROVAL</span><h2>Review before installing</h2></div>
    <button class="icon-button" aria-label="Close install review" disabled={installing} on:click={onClose}><Icon name="close" size={16} /></button>
  </header>
  <p id="install-review-description">The app will run only this registered, non-elevated installation action.</p>
  <dl class="install-plan-details">
    <div><dt>Action</dt><dd>{plan.action}</dd></div>
    <div><dt>Provider</dt><dd>{plan.provider}</dd></div>
    <div><dt>Source</dt><dd>{plan.source}</dd></div>
    <div><dt>Scope</dt><dd>{plan.scope}</dd></div>
    <div><dt>Exact version</dt><dd>{plan.exactVersion}</dd></div>
    <div class="full"><dt>Destination</dt><dd>{plan.destination}</dd></div>
  </dl>
  <div class="install-command">
    <div><span>Exact command</span><button class="icon-button" aria-label="Copy install command" title="Copy install command" on:click={() => onCopy(plan.command)}><Icon name="copy" size={15} /></button></div>
    <code>{plan.command}</code>
  </div>
  <div class="callout info install-safety"><Icon name="check" size={17} /><div><strong>No administrator elevation</strong><p>This writes to the current-user PowerShell module path. Tokens and credentials are not involved.</p></div></div>
  <footer>
    <button class="secondary" disabled={installing} on:click={onClose}>Cancel</button>
    <button class="text-button" on:click={() => onOpenDocumentation(plan.documentationUrl)}>Official documentation</button>
    <div class="footer-spacer"></div>
    <button bind:this={confirmButton} class="primary" disabled={installing} on:click={onConfirm}>{installing ? "Installing…" : "Install for current user"}</button>
  </footer>
</dialog>
