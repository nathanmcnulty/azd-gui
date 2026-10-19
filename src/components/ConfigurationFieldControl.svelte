<script lang="ts">
  import { createEventDispatcher } from "svelte";
  import type { ConfigurationField, ConnectionTarget, WizardValues } from "../lib/types";
  import { acknowledgementMessage } from "../lib/acknowledgements";
  import SecurityGroupPicker from "./SecurityGroupPicker.svelte";

  export let field: ConfigurationField;
  export let value: WizardValues[string] = "";
  export let configuredSensitive = false;
  export let acknowledgementAvailable = true;
  export let lookupTarget: ConnectionTarget | null = null;

  const dispatch = createEventDispatcher<{ change: WizardValues[string] }>();

  $: selectedValues = Array.isArray(value)
    ? value
    : typeof value === "string" && value
      ? value.split(",").map((item) => item.trim()).filter(Boolean)
      : [];
  $: selectedOption = field.options?.find((option) => option.value === value);
  $: sensitive = field.sensitive || field.type === "secret";
  $: fallbackPattern = ["tenant", "subscription", "entraObject"].includes(field.type)
    ? "[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{12}"
    : field.type === "location"
      ? "[A-Za-z0-9]{1,64}"
      : field.type === "resourceGroup"
        ? "[A-Za-z0-9._()\\-]{1,90}"
        : undefined;
  $: placeholder = configuredSensitive
    ? "Configured — enter to replace"
    : field.placeholder ??
      (field.type === "tenant" || field.type === "subscription" || field.type === "entraObject"
        ? "00000000-0000-0000-0000-000000000000"
        : field.type === "location"
          ? "westus2"
          : field.type === "resourceGroup"
            ? "rg-example"
            : "");

  function change(value: WizardValues[string]) {
    dispatch("change", value);
  }

  function toggleOption(optionValue: string, checked: boolean) {
    const next = checked
      ? [...selectedValues, optionValue]
      : selectedValues.filter((value) => value !== optionValue);
    change(next);
  }
</script>

<div class:full={field.type === "multiselect" || sensitive} class="field">
  {#if field.type === "acknowledgement"}
    <label class="toggle-line card">
      <input type="checkbox" checked={value === true} disabled={!acknowledgementAvailable} required aria-required="true"
        aria-invalid={value !== true} aria-describedby={`acknowledgement-${field.id}-message`}
        on:change={(event) => change(event.currentTarget.checked)} />
      <span><strong>{field.label} <em>Required</em></strong>
        {#if field.help}<small>{field.help}</small>{/if}
      </span>
    </label>
    <small id={`acknowledgement-${field.id}-message`}>{!acknowledgementAvailable ? "Select an available tenant and controlling scope before acknowledging." : value === true ? "Accepted for the current scope and tenant." : acknowledgementMessage(field)}</small>
  {:else if field.type === "boolean"}
    <label class="toggle-line card">
      <input type="checkbox" checked={Boolean(value)} on:change={(event) => change(event.currentTarget.checked)} />
      <span>
        <strong>{field.label}{#if field.required} <em>Required</em>{/if}</strong>
        {#if field.help}<small>{field.help}</small>{/if}
      </span>
    </label>
  {:else if field.type === "select"}
    <label>
      <span>{field.label}{#if field.required} <em>Required</em>{/if}</span>
      <select value={String(value ?? "")} on:change={(event) => change(event.currentTarget.value)} required={field.required}>
        <option value="">Choose a value</option>
        {#each field.options ?? [] as option}
          <option value={option.value}>{option.label}</option>
        {/each}
      </select>
      {#if selectedOption?.description}<small class="option-description">{selectedOption.description}</small>{/if}
    </label>
  {:else if field.type === "multiselect"}
    <fieldset class="multiselect-field">
      <legend>{field.label}{#if field.required} <em>Required</em>{/if}</legend>
      {#each field.options ?? [] as option}
        <label class="option-choice">
          <input
            type="checkbox"
            checked={selectedValues.includes(option.value)}
            on:change={(event) => toggleOption(option.value, event.currentTarget.checked)}
          />
          <span>
            <strong>{option.label}</strong>
            {#if option.description}<small>{option.description}</small>{/if}
          </span>
        </label>
      {/each}
      {#if field.help}<small>{field.help}</small>{/if}
    </fieldset>
  {:else}
    <label>
      <span>{field.label}{#if field.required} <em>Required</em>{/if}</span>
      <input
        type={sensitive ? "password" : "text"}
        value={String(value ?? "")}
        placeholder={placeholder}
        autocomplete={sensitive ? "new-password" : "off"}
        on:input={(event) => change(event.currentTarget.value)}
        required={field.required}
        pattern={field.pattern ?? fallbackPattern}
      />
      {#if field.help}<small>{field.help}</small>{/if}
    </label>
  {/if}
  {#if field.type !== "boolean" && sensitive}<small class="sensitive-note">This value is never saved in the app draft or logs.</small>{/if}
  {#if ["entraObject", "tenant", "subscription", "location", "resourceGroup"].includes(field.type)}
    <small class="field-type-note">{field.type === "entraObject" ? "Enter a Microsoft Entra object ID." : field.type === "tenant" ? "Enter a tenant ID." : field.type === "subscription" ? "Enter a subscription ID." : field.type === "location" ? "Enter an Azure region name." : "Enter an Azure resource group name."}</small>
  {/if}
  {#if field.lookup === "securityGroup" && field.type === "entraObject" && !sensitive}
    {#if lookupTarget}
      <SecurityGroupPicker target={lookupTarget} value={String(value ?? "")} on:change={(event) => change(event.detail)} />
    {:else}
      <small>Group search is available in the deployment wizard after choosing an Azure tenant and subscription. You can enter the ID manually.</small>
    {/if}
  {/if}
  <small><code>{field.environmentVariable}</code></small>
</div>
