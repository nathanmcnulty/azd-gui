import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import { cleanup, fireEvent, render, screen } from "@testing-library/svelte";
import { afterEach, expect, it, vi } from "vitest";
import ProjectComponentProvenance from "./ProjectComponentProvenance.svelte";
import type { ComponentUpdates } from "../lib/component-updates";
import type { ComponentProvenanceRead } from "../lib/component-provenance";

afterEach(() => {
  cleanup();
  clearMocks();
  vi.unstubAllGlobals();
});

it("checks the inspected template's exact pins through Tauri and refreshes from the current lock", async () => {
  const template = { sourcePath: "C:/Templates/fixture", contentDigest: "d".repeat(64) };
  const installed = {
    id: "deployment-validation",
    version: "1.2.3",
    sourceRepository: "https://github.com/nathanmcnulty/azd-reference",
    sourceRevision: "a".repeat(40)
  };
  const upgraded = { ...installed, version: "2.0.0", sourceRevision: "b".repeat(40) };
  let pin = installed;
  let sourceUnavailable = false;
  const invoke = vi.fn((command: string, _payload?: unknown) => {
    if (command === "get_template_component_provenance") {
      return {
        state: "available", manifestVersion: "1.0", unknownMetadata: false,
        components: [{
          ...pin, status: "verified", unknownMetadata: false,
          files: [{
            source: "components/engine.psm1", target: "scripts/vendor/engine.psm1",
            sha256: "f".repeat(64), status: "verified"
          }]
        }]
      } satisfies ComponentProvenanceRead;
    }
    if (command === "check_component_updates") {
      if (sourceUnavailable) throw new Error("fixture source unavailable");
      return {
        state: "fresh", checkedAt: new Date().toISOString(),
        updates: [{
          id: installed.id, state: "updateAvailable", version: "2.0.0",
          compatibility: "breaking", complete: true, historyIncomplete: false,
          breakingVersions: ["2.0.0"], migrationNotes: ["Upgrade to PowerShell 7.6."]
        }]
      } satisfies ComponentUpdates;
    }
    throw new Error("Unexpected Tauri command: " + command);
  });
  vi.stubGlobal("isTauri", true);
  mockIPC(invoke);
  render(ProjectComponentProvenance, { template });

  expect(await screen.findByText("Breaking changes declared in 2.0.0.")).toBeInTheDocument();
  expect(screen.getByText("Upgrade to PowerShell 7.6.")).toBeInTheDocument();
  expect(invoke.mock.calls).toEqual([
    ["get_template_component_provenance", { sourceBinding: template }],
    ["check_component_updates", { installed: [installed], force: false }]
  ]);

  pin = upgraded;
  sourceUnavailable = true;
  await fireEvent.click(screen.getByRole("button", { name: "Check for updates" }));

  expect(await screen.findByText("Update information is unavailable for this component.")).toBeInTheDocument();
  expect(screen.getByText("2.0.0")).toBeInTheDocument();
  expect(screen.queryByText(/Breaking changes declared|fixture source unavailable/)).not.toBeInTheDocument();
  expect(invoke.mock.calls).toEqual([
    ["get_template_component_provenance", { sourceBinding: template }],
    ["check_component_updates", { installed: [installed], force: false }],
    ["get_template_component_provenance", { sourceBinding: template }],
    ["check_component_updates", { installed: [upgraded], force: true }]
  ]);
});
