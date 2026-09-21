<script lang="ts">
  import Icon from "./Icon.svelte";
  import StatusPill from "./StatusPill.svelte";
  import { getProjectValidationReport } from "../lib/bridge";
  import { evidenceActionText, evidenceBindingDetail } from "../lib/evidence-presentation";
  import type {
    ProjectSummary,
    EvidenceBindingState,
    ValidationCheck,
    ValidationMode,
    ValidationOutcome,
    ValidationReportRead,
    ValidationStatus
  } from "../lib/types";

  export let project: ProjectSummary;

  let result: ValidationReportRead = { state: "notFound" };
  let loading = false;
  let request = 0;

  const modeDetail: Record<ValidationMode, string> = {
    plan: "A plan describes intended checks and does not establish cloud verification.",
    verify: "Verification checks recorded by the project.",
    delivery: "Validation may have sent a synthetic delivery event."
  };

  const bindingDetail = {
    environmentMatch: "The report environment name matches the selected project environment. This does not establish target identity or report freshness.",
    environmentMismatch: "The report environment name differs from the selected project environment. This does not establish target identity or report freshness.",
    environmentUnbound: "The report does not establish an environment binding or report provenance."
  } as const;

  function tone(outcome: ValidationOutcome, binding: EvidenceBindingState): "success" | "warning" | "danger" | "neutral" {
    if (outcome === "failed") return "danger";
    if (binding !== "current") return binding === "mismatch" ? "danger" : "warning";
    if (outcome === "passed") return "success";
    if (outcome === "passedWithWarnings") return "warning";
    return "neutral";
  }

  function outcomeLabel(outcome: ValidationOutcome, binding: EvidenceBindingState): string {
    if (outcome === "failed") return "failed";
    if (binding === "current") return outcome;
    if (binding === "mismatch") return "Context mismatch";
    if (binding === "stale") return "Stale record";
    if (binding === "legacyUnbound") return "Legacy record";
    return "Context incomplete";
  }

  function statusTone(status: ValidationStatus): "success" | "warning" | "danger" | "neutral" {
    if (status === "pass") return "success";
    if (status === "fail") return "danger";
    if (status === "warning") return "warning";
    return "neutral";
  }

  function checkLabel(check: ValidationCheck): string {
    return `${check.phase} · ${check.sideEffect} · ${check.durationMs} ms`;
  }

  async function refresh() {
    const requestId = ++request;
    loading = true;
    try {
      const next = await getProjectValidationReport(project.path);
      if (requestId === request) result = next;
    } catch {
      if (requestId === request) result = { state: "invalid" };
    } finally {
      if (requestId === request) loading = false;
    }
  }

  $: projectBindingKey = `${project.path}\u0000${project.environment ?? ""}`;
  $: if (projectBindingKey) void refresh();
</script>

<section class="workspace-panel full" aria-label="Validation report">
  <div class="section-toolbar">
    <div>
      <h2>Validation report</h2>
      <p>Recorded validation checks are separate from provisioning and deployment success.</p>
    </div>
    {#if loading}
      <StatusPill tone="accent" label="Reading" />
    {:else if result.state === "available" && result.report}
      <StatusPill tone={tone(result.report.outcome, result.report.bindingState)} label={outcomeLabel(result.report.outcome, result.report.bindingState)} />
    {:else if result.state === "notFound"}
      <StatusPill tone="neutral" label="Not found" />
    {:else}
      <StatusPill tone="warning" label="Unavailable" />
    {/if}
  </div>

  {#if loading}
    <p class="muted-copy">Reading the selected project's report…</p>
  {:else if result.state === "available" && result.report}
    <dl class="summary-list columns">
      <div><dt>Mode</dt><dd>{result.report.mode}</dd></div>
      <div><dt>Completed</dt><dd>{new Date(result.report.completedAt).toLocaleString()}</dd></div>
      <div><dt>Duration</dt><dd>{result.report.durationMs} ms</dd></div>
      <div><dt>Checks</dt><dd>{result.report.checks.length}</dd></div>
      <div><dt>Passed</dt><dd>{result.report.summary.pass}</dd></div>
      <div><dt>Warnings</dt><dd>{result.report.summary.warning}</dd></div>
      <div><dt>Failed</dt><dd>{result.report.summary.fail}</dd></div>
      <div><dt>Skipped</dt><dd>{result.report.summary.skipped}</dd></div>
      <div><dt>Planned</dt><dd>{result.report.summary.planned}</dd></div>
      <div><dt>Informational</dt><dd>{result.report.summary.info}</dd></div>
      <div><dt>Correlation</dt><dd>{result.report.bindingState}</dd></div>
    </dl>
    <p class="muted-copy">{modeDetail[result.report.mode]}</p>
    <p class="muted-copy">{bindingDetail[result.report.environmentBinding]}</p>
    <p class="muted-copy">{evidenceBindingDetail[result.report.bindingState]}</p>
    {#if result.report.nextActions.length > 0}
      <ul aria-label="Validation next actions">
        {#each result.report.nextActions as action}
          <li>{evidenceActionText(action)}</li>
        {/each}
      </ul>
    {/if}
    <p class="muted-copy">This local report is not independently authenticated and does not prove provisioning, delivery, or cleanup.</p>
    {#if result.report.checks.length > 0}
      <div class="validation-check-list" aria-label="Validation checks">
        {#each result.report.checks as check, index}
          <div class="validation-check">
            <div><strong>Check {index + 1}</strong><small>{checkLabel(check)}</small></div>
            <StatusPill tone={statusTone(check.status)} label={check.status} />
          </div>
        {/each}
      </div>
    {:else}
      <div class="callout info"><Icon name="check" size={17} /><div><strong>No checks were recorded</strong><p>The report is valid but contains no check results.</p></div></div>
    {/if}
  {:else if result.state === "notFound"}
    <div class="callout info"><Icon name="check" size={17} /><div><strong>No validation report found</strong><p>The project has no report at <code>reports/deployment-validation.json</code>.</p></div></div>
  {:else if result.state === "tooLarge"}
    <div class="callout warning"><Icon name="warning" size={17} /><div><strong>Validation report is too large</strong><p>The app reads reports up to 1 MiB.</p></div></div>
  {:else if result.state === "unsafePath"}
    <div class="callout warning"><Icon name="warning" size={17} /><div><strong>Validation report path is unsafe</strong><p>The report path contains a link or reparse point and was not read.</p></div></div>
  {:else if result.state === "futureVersion"}
    <div class="callout warning"><Icon name="warning" size={17} /><div><strong>Validation report is newer than this app</strong><p>Update the app before relying on this report.</p></div></div>
  {:else}
    <div class="callout warning"><Icon name="warning" size={17} /><div><strong>Validation report is unavailable</strong><p>The file is not a supported report.</p></div></div>
  {/if}

  <div class="project-deployment-actions">
    <button class="secondary compact" disabled={loading} on:click={refresh}><Icon name="refresh" size={14} /> Refresh report</button>
  </div>
</section>

<style>
  .validation-check-list { display: grid; gap: 8px; margin: 16px 0; }
  .validation-check { display: flex; align-items: center; justify-content: space-between; gap: 12px; padding: 12px; border: 1px solid var(--border); border-radius: 5px; }
  .validation-check small { display: block; margin-top: 4px; color: var(--muted); overflow-wrap: anywhere; }
</style>
