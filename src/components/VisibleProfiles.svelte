<script lang="ts">
  import { listVisibleProfiles } from "../lib/bridge";
  import type { ProfileProvider, VisibleProfiles } from "../lib/types";

  export let providerId: ProfileProvider;
  export let contextKey = "";
  let open = false;
  let loading = false;
  let result: VisibleProfiles | null = null;
  let request = 0;
  let previousContext = "";

  $: if (`${providerId}:${contextKey}` !== previousContext) {
    previousContext = `${providerId}:${contextKey}`;
    request += 1;
    result = null;
    loading = false;
  }

  async function load() {
    const currentRequest = ++request;
    const provider = providerId;
    loading = true;
    result = null;
    try {
      const inventory = await listVisibleProfiles(provider);
      if (currentRequest !== request) return;
      result = inventory.providerId === provider ? inventory : { providerId: provider, state: "unknown", profiles: [], hasMore: false };
    } catch {
      if (currentRequest !== request) return;
      result = { providerId: provider, state: "unknown", profiles: [], hasMore: false };
    } finally {
      if (currentRequest === request) loading = false;
    }
  }
</script>

<details class="visible-profiles" bind:open on:toggle={() => { if (open && !result && !loading) void load(); }}>
  <summary>Locally visible profiles</summary>
  <p>Cached profile hints are unverified. Reconnect this provider with the intended account, then check the selected target.</p>
  <p>{providerId === "azureCli" ? "Azure CLI lists subscriptions known to its logged-in account in the current cloud." : "Azure PowerShell lists saved contexts; its context population limit can omit subscriptions."} This list may be incomplete.</p>
  <button class="secondary compact" disabled={loading} on:click={load}>{loading ? "Loading profiles…" : "Refresh visible profiles"}</button>
  <div aria-live="polite">
    {#if result?.state === "unknown"}
      <p>Visible profiles are unavailable. Check that this provider is installed, or reconnect it and try again.</p>
    {:else if result}
      {#if !result.profiles.length}<p>No local profiles were reported by this provider.</p>{/if}
      <ul>
        {#each result.profiles as profile}
          <li><strong>{profile.account}</strong><span>{profile.subscriptionName ?? profile.subscriptionId}</span><small>Tenant: {profile.tenantId}</small><small>Subscription: {profile.subscriptionId}</small><small>Unverified profile hint</small></li>
        {/each}
      </ul>
      {#if result.hasMore}<p>Showing at most 100 profiles. More contexts may be available in the provider.</p>{/if}
    {/if}
  </div>
</details>

<style>
  .visible-profiles { margin-top: 12px; overflow-wrap: anywhere; }
  summary { cursor: pointer; font-weight: 600; }
  p, small, span { color: var(--muted); }
  p { font-size: 12px; }
  ul { list-style: none; padding: 0; max-height: 260px; overflow-y: auto; }
  li { display: grid; gap: 4px; padding: 10px 0; border-bottom: 1px solid var(--border); }
  small { display: block; }
</style>
