<script lang="ts">
  import type { ConnectionStatus, ConnectionTarget, TemplateManifest, ToolStatus } from '../lib/types';
  import { connectionTargetAssessment } from '../lib/connections';
  import { toolReadiness } from '../lib/readiness';
  export let tools: ToolStatus[];
  export let connections: ConnectionStatus[];
  export let manifest: TemplateManifest | undefined;
  export let target: ConnectionTarget | null;
  export let fresh: boolean;
  export let interactive: boolean;
  export let onCheckTools: () => void;
  export let onCheckAccounts: () => void;
</script>

<details class="readiness" open>
  <summary>Readiness for current choices</summary>
  <p>These tools and accounts belong to you, the operator. Connected does not mean verified for the selected target or authorized to make every change. The deployed identity’s grants are separate; review the configuration disclosures.</p>
  <p>Selected target: tenant <code>{target?.tenantId ?? 'not selected'}</code> · subscription <code>{target?.subscriptionId ?? 'not selected'}</code></p>
  <ul>
    {#each tools as tool}
      {@const readiness = toolReadiness(tool, manifest)}
      <li><strong>{tool.name}</strong> — {readiness.detected}<br />{readiness.why}<br /><span>Next: {readiness.action}</span></li>
    {/each}
    {#each connections as connection}
      {@const assessment = connectionTargetAssessment(connection, target, fresh)}
      <li><strong>{connection.name} account</strong> — {connection.state}; {connection.account ?? 'no account detected'}<br />{connection.reason}<br />Detected tenant: {connection.tenantId ?? 'unknown'} · subscription: {connection.subscriptionId ?? 'unknown'}<br />{connection.required ? assessment.label : 'Optional; does not block continuing'} · Next: {assessment.action}</li>
    {/each}
  </ul>
  {#if interactive}<p><strong>Terminal handoff required.</strong> After approval, prepare the project and finish its prompts in an external terminal. Opening it does not run the command or prove deployment.</p>{/if}
  <div class="actions">
    {#if tools.length}<button class="text-button" on:click={onCheckTools}>Check tools again</button>{/if}
    {#if connections.length}<button class="text-button" on:click={onCheckAccounts}>Check accounts again</button>{/if}
  </div>
</details>

<style>
  .readiness { margin-top: 24px; padding: 16px; border: 1px solid var(--border); border-radius: 8px; }
  summary { cursor: pointer; font-weight: 600; }
  p, li { font-size: 13px; line-height: 1.6; overflow-wrap: anywhere; }
  ul { padding-left: 20px; }
  li + li { margin-top: 12px; }
  .actions { display: flex; gap: 16px; }
</style>
