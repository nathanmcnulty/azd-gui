<script lang="ts">
  import Icon from "./Icon.svelte";
  export let tone: "success" | "warning" | "danger" | "neutral" | "accent" = "neutral";
  export let label: string;
  export let dot = true;
</script>

<span class={`status-pill ${tone}${dot ? " has-dot" : ""}`}>
  {#if !dot && tone === "success"}<Icon name="check" size={13} />{/if}
  {label}
</span>
