<script lang="ts">
  import { createEventDispatcher } from "svelte";
  import type { ConfigurationField, WizardPage, WizardValues } from "../lib/types";

  export let pages: WizardPage[];
  export let values: WizardValues;
  const dispatch = createEventDispatcher<{ edit: string }>();

  function displayValue(field: ConfigurationField): string {
    const value = values[field.id];
    if (value === undefined || value === "" || (Array.isArray(value) && value.length === 0)) return "Not set";
    if (field.sensitive || field.type === "secret") return "Configured (hidden)";
    if (typeof value === "boolean") return value ? "Yes" : "No";
    const optionLabel = (value: string) => field.options?.find((option) => option.value === value)?.label ?? value;
    if (Array.isArray(value)) return value.map(optionLabel).join(", ");
    return optionLabel(String(value));
  }
</script>

{#each pages.filter((page) => page.kind === "configuration" && page.group) as page}
  <section class="full" aria-label={`Review ${page.title}`}>
    <div class="review-section-heading">
      <h3>{page.title}</h3>
      <button class="text-button" aria-label={`Edit ${page.title}`} on:click={() => dispatch("edit", page.id)}>Edit</button>
    </div>
    <dl class="summary-list columns">
      {#each page.group?.fields ?? [] as field}
        <div><dt>{field.label}</dt><dd>{displayValue(field)}</dd></div>
      {/each}
    </dl>
  </section>
{/each}
