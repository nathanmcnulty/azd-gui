<script lang="ts">
  import { createEventDispatcher } from "svelte";
  import { searchSecurityGroups } from "../lib/bridge";
  import type { ConnectionTarget, SecurityGroupSearch } from "../lib/types";

  export let target: ConnectionTarget;
  export let value = "";
  const dispatch = createEventDispatcher<{ change: string }>();
  let query = "";
  let result: SecurityGroupSearch | null = null;
  let busy = false;
  let error = "";
  let request = 0;
  let selected: { id: string; targetKey: string } | null = null;
  let previousKey = "";
  $: targetKey = `${target.tenantId.toLowerCase()}|${target.subscriptionId.toLowerCase()}`;
  $: if (targetKey !== previousKey) {
    previousKey = targetKey;
    request += 1;
    result = null;
    error = "";
    busy = false;
    if (selected && selected.targetKey !== targetKey) {
      if (value === selected.id) dispatch("change", "");
      selected = null;
    }
  }

  function editQuery(next: string) {
    query = next;
    request += 1;
    result = null;
    error = "";
    busy = false;
  }

  async function search() {
    if (query.trim().length < 2 || query.trim().length > 80 || busy) return;
    const requestId = ++request;
    const searchedTarget = targetKey;
    busy = true;
    result = null;
    error = "";
    try {
      const next = await searchSecurityGroups(target, query.trim());
      if (requestId !== request || searchedTarget !== targetKey) return;
      if (`${next.tenantId.toLowerCase()}|${next.subscriptionId.toLowerCase()}` !== searchedTarget) {
        error = "The search target changed. Search again before selecting a group.";
        return;
      }
      result = next;
    } catch {
      if (requestId === request) error = "Group search is unavailable. Check the Azure CLI connection and directory read access, or enter the group ID manually.";
    } finally {
      if (requestId === request) busy = false;
    }
  }

  function choose(id: string) {
    if (!result || busy || !result.groups.some((group) => group.id === id)) return;
    selected = { id, targetKey };
    dispatch("change", id);
  }
</script>

<section class="group-picker" aria-label="Security group search">
  <label>Find a security group by name
    <input value={query} maxlength="80" placeholder="Enter at least 2 characters" on:input={(event) => editQuery(event.currentTarget.value)} on:keydown={(event) => { if (event.key === "Enter") { event.preventDefault(); void search(); } }} />
  </label>
  <p class="muted-copy">Searches the selected tenant. Names can repeat; check the group ID before selecting. Changing the Azure target clears this group ID.</p>
  <button class="secondary compact" disabled={busy || query.trim().length < 2} on:click={search}>{busy ? "Searching…" : "Search groups"}</button>
  <div aria-live="polite">
    {#if error}<p class="lookup-error">{error}</p>{/if}
    {#if result}
      {#if result.groups.length === 0}<p>No matching security groups. Try a different name prefix or enter the ID manually.</p>{/if}
      <ul>
        {#each result.groups as group}
          <li><span><strong>{group.displayName}</strong><code>{group.id}</code></span><button class="secondary compact" aria-label={`Select ${group.displayName}, ${group.id}`} on:click={() => choose(group.id)}>Select</button></li>
        {/each}
      </ul>
      {#if result.hasMore}<p>More matches are available. Refine the name prefix to narrow the results.</p>{/if}
    {/if}
  </div>
</section>

<style>
  .group-picker { margin-top: 12px; padding: 12px; border: 1px solid var(--border); border-radius: 5px; }
  ul { list-style: none; margin: 12px 0 0; padding: 0; }
  li { display: flex; align-items: center; justify-content: space-between; gap: 12px; margin-top: 10px; }
  li span { min-width: 0; overflow-wrap: anywhere; }
  code { display: block; margin-top: 4px; }
  .lookup-error { color: var(--warning); }
</style>
