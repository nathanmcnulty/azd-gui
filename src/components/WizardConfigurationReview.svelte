<script lang="ts">
  import { createEventDispatcher } from "svelte";
  import type { AcknowledgementProof, ConfigurationField, ConfigurationGroup, WizardPage, WizardValues } from "../lib/types";

  import { acknowledgementMatches } from "../lib/acknowledgements";

  export let acknowledgementProofs: AcknowledgementProof[] = [];
  export let pages: WizardPage[];
  export let values: WizardValues;
  const dispatch = createEventDispatcher<{ edit: string }>();

  function acknowledgementDetails(field: ConfigurationField, group: ConfigurationGroup): string {
    const conditions = [group.condition, field.condition].filter((condition) => condition != null);
    return conditions.map((condition) => {
      const controller = pages.flatMap((page) => page.group?.fields ?? []).find((item) => item.id === condition.field);
      const operator = condition.equals != null ? "equals" : condition.notEquals != null ? "does not equal" : "includes";
      const expected = condition.equals ?? condition.notEquals ?? condition.includes;
      return `${controller?.label ?? condition.field} (${condition.field}) ${operator} ${JSON.stringify(expected)}; current value: ${JSON.stringify(values[condition.field])}`;
    }).join(". ");
  }

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
        <div><dt>{field.label}</dt><dd>
          {#if field.type === "acknowledgement" && page.group}
            <strong>{acknowledgementMatches(field, page.group, values, acknowledgementProofs) ? "Acknowledged" : "Fresh acknowledgement required"}</strong>
            <small>{acknowledgementDetails(field, page.group)}</small>
            <small>Tenant: {String(values.tenantId ?? "Not selected")}</small>
            <small>Persisted variable: {field.environmentVariable} = {values[field.id] === true ? "true" : "false"}</small>
          {:else}{displayValue(field)}{/if}
        </dd></div>
      {/each}
    </dl>
  </section>
{/each}
