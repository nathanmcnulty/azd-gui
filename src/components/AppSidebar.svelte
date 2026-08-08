<script lang="ts">
  import Icon from "./Icon.svelte";
  import type { CatalogEntry, WizardPlan } from "../lib/types";

  type NavId = "home" | "catalog" | "projects" | "connections" | "tools" | "settings";
  type IconName = "home" | "catalog" | "projects" | "connections" | "tools" | "settings";
  interface NavItem {
    id: NavId;
    label: string;
    icon: IconName;
  }

  export let selectedEntry: CatalogEntry | null;
  export let wizardPlan: WizardPlan | null;
  export let activeStep: number;
  export let navigation: NavItem[];
  export let activeNav: NavId;
  export let expanded: boolean;
  export let connectedConnectionCount: number;
  export let projectCount: number;
  export let azdVersion: string;
  export let appVersion: string;
  export let onNavigate: (id: NavId) => void;
  export let onCloseDeployment: () => void;
  export let onToggle: () => void;
</script>

<aside class:collapsed={!expanded} class="sidebar">
  {#if selectedEntry && wizardPlan}
    <div class="sidebar-heading">
      <span>DEPLOYMENT</span>
      <button aria-label="Close deployment" title="Close deployment" on:click={onCloseDeployment}><Icon name="close" size={15} /></button>
    </div>
    <div class="wizard-template" title={selectedEntry.title}>
      <div class="template-monogram">{selectedEntry.title.slice(0, 2).toUpperCase()}</div>
      <div class="wizard-template-details">
        <strong>{selectedEntry.title}</strong>
        <span>{selectedEntry.commit.slice(0, 8)}</span>
      </div>
    </div>
    <nav class="step-list" aria-label="Deployment steps">
      {#each wizardPlan.pages as page, index}
        <button
          class:active={activeStep === index}
          class:complete={index < activeStep || page.complete}
          disabled={index > activeStep}
          on:click={() => (activeStep = index)}
          aria-current={activeStep === index ? "step" : undefined}
          aria-label={`${index + 1}. ${page.title}`}
          title={page.title}
        >
          <span class="step-marker">
            {#if index < activeStep}<Icon name="check" size={12} />{:else}{index + 1}{/if}
          </span>
          <span class="step-copy">
            <strong>{page.title}</strong>
            <small>{page.description}</small>
          </span>
        </button>
      {/each}
    </nav>
  {:else}
    <div class="sidebar-heading navigation-heading"><span>NAVIGATION</span></div>
    <nav class="sidebar-nav" aria-label="Primary navigation">
      {#each navigation as item}
        <button
          class:active={activeNav === item.id}
          aria-current={activeNav === item.id ? "page" : undefined}
          aria-label={item.id === "connections" ? `${item.label}, ${connectedConnectionCount} connected` : item.id === "projects" ? `${item.label}, ${projectCount} projects` : item.label}
          title={item.label}
          on:click={() => onNavigate(item.id)}
        >
          <Icon name={item.icon} size={22} />
          <span class="nav-label">{item.label}</span>
          {#if item.id === "connections"}
            <span class="nav-count">{connectedConnectionCount}</span>
          {:else if item.id === "projects"}
            <span class="nav-count">{projectCount}</span>
          {/if}
        </button>
      {/each}
    </nav>
  {/if}
  <div class="sidebar-footer">
    <div class="sidebar-versions" aria-hidden={!expanded}>
      <span>azd {azdVersion}</span>
      <span>App {appVersion}</span>
    </div>
    <button
      class="navigation-toggle"
      aria-label={expanded ? "Collapse navigation" : "Expand navigation"}
      aria-expanded={expanded}
      title={expanded ? "Collapse navigation" : "Expand navigation"}
      on:click={onToggle}
    ><Icon name="chevron" size={15} /></button>
  </div>
</aside>
