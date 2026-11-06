<script lang="ts">
  import { roleIds, type PimRoleCatalog } from '../lib/pim-roles';
  export let catalog: PimRoleCatalog | null = null;
  export let privilegedIds = '';
  export let lessPrivilegedIds = '';
  export let select: (id: string, privileged: boolean, checked: boolean) => void;
  let search = '';
  $: matches = catalog?.roles.filter(role => `${role.displayName} ${role.id}`.toLowerCase().includes(search.toLowerCase())) ?? [];
</script>
<section aria-label="Tenant role picker" class="pim-review">
  <h2>Select pilot roles</h2>
  <p>Search by name and select individual roles. No roles are selected automatically. An empty tier is excluded; an empty pilot cannot generate a plan.</p>
  {#if catalog}
    <p role="status">{catalog.source === 'live' ? 'Live' : 'Cached'} complete role list — tenant {catalog.tenantId}; retrieved {catalog.retrievedAt}. {catalog.roles.length} roles.</p>
    <label>Search roles<input type="search" bind:value={search} placeholder="Role name or ID" /></label>
    <div class="pim-role-options">
      {#each matches as role (role.id)}
        <label class="pim-role-option"><input type="checkbox"
          checked={roleIds(privilegedIds).includes(role.id) || roleIds(lessPrivilegedIds).includes(role.id)}
          disabled={(role.isPrivileged === null || role.isEnabled !== true) && !roleIds(privilegedIds).includes(role.id) && !roleIds(lessPrivilegedIds).includes(role.id)}
          on:change={event => select(role.id, role.isPrivileged === true, event.currentTarget.checked)} />
          <span>{role.displayName} — {role.isPrivileged === null ? 'Unknown privilege' : role.isPrivileged ? 'Privileged' : 'Less privileged'}; {role.isBuiltIn === null ? 'Unknown role type' : role.isBuiltIn ? 'Microsoft built-in' : 'Custom role'}{role.isEnabled !== true ? '; disabled or availability unknown' : ''}<br /><small>{role.id}</small></span>
        </label>
      {:else}<p>{catalog.roles.length ? 'No roles match this search.' : 'The complete tenant role list is empty.'}</p>{/each}
    </div>
  {:else}<p>No role list loaded for this tenant. Enter your Graph identity, review configuration and approve sign-in, then refresh tenant roles. Manual IDs remain available below.</p>{/if}
</section>
<style>
  .pim-role-options { max-height: 22rem; overflow: auto; margin-top: 1rem; }
  .pim-role-option { display: flex; align-items: start; gap: .7rem; padding: .6rem; border-bottom: 1px solid #8884; }
  .pim-role-option input { width: auto; margin-top: .3rem; }
</style>
