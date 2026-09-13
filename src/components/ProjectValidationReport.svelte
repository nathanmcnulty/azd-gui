<script lang="ts">
  import Icon from "./Icon.svelte";
  import StatusPill from "./StatusPill.svelte";
  import { getProjectValidationReport } from "../lib/bridge";
  import type {
    ProjectSummary,
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
    plan: "Planned checks only; no cloud verification ran.",
    verify: "Read-only validation checks recorded by the project.",
    delivery: "Validation may have sent a synthetic delivery event."
  };

  const bindingDetail = {
    environmentMatch: "The report environment name matches the selected project environment. This does not establish target identity or report freshness.",
    environmentMismatch: "The report environment name differs from the selected project environment. This does not establish target identity or report freshness.",
    environmentUnbound: "The report does not establish an environment binding or report provenance."
  } as const;

  function tone(outcome: ValidationOutcome): "success" | "warning" | "danger" | "neutral" {
    if (outcome === "passed") return "success";
    if (outcome === "passedWithWarnings") return "warning";
    if (outcome === "failed") return "danger";
    return "neutral";
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
      <StatusPill tone={tone(result.report.outcome)} label={result.report.outcome} />
    {:else if result.state === "notFound"}
      <StatusPill tone="neutral" label="Not found" />
    {:else}
      <StatusPill tone="warning" label="Unavailable" />
    {/if}
  </div>

  {#if result.state === "available" && result.report}
    <dl class="summary-list columns">
      <div><dt>Mode</dt><dd>{result.report.mode}</dd></div>
      <div><dt>Completed</dt><dd>{new Date(result.report.completedAt).toLocaleString()}</dd></div>
      <div><dt>Duration</dt><dd>{result.report.durationMs} ms</dd></div>
      <div><dt>Checks</dt><dd>{result.report.checks.length}</dd></div>
      <div><dt>Passed</dt><dd>{result.report.summary.pass}</dd></div>
      <div><dt>Warnings</dt><dd>{result.report.summary.warning}</dd></div>
      <div><dt>Failed</dt><dd>{result.report.summary.fail}</dd></div>
      <div><dt>Skipped</dt><dd>{result.report.summary.skipped}</dd></div>
    </dl>
    <p class="muted-copy">{modeDetail[result.report.mode]}</p>
    <p class="muted-copy">{bindingDetail[result.report.environmentBinding]}</p>
    {#if result.report.checks.length > 0}
      <div class="validation-check-list" aria-label="Validation checks">
        {#each result.report.checks as check}
          <div class="validation-check">
            <div><code>{check.id}</code><small>{checkLabel(check)}</small></div>
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
  {:else}
    <div class="callout warning"><Icon name="warning" size={17} /><div><strong>Validation report is unavailable</strong><p>The file is not a supported version 1.0 report.</p></div></div>
  {/if}

  <div class="project-deployment-actions">
    <button class="secondary compact" disabled={loading} on:click={refresh}><Icon name="refresh" size={14} /> Refresh report</button>
  </div>
</section>
