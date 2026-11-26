import { cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/svelte";
import { afterEach, expect, it, vi } from "vitest";
import CatalogComponentPins from "./CatalogComponentPins.svelte";
import * as bridge from "../lib/bridge";
import * as componentUpdates from "../lib/component-updates";
import { bundledCatalog } from "../lib/mock-data";
import type { CatalogEntry } from "../lib/types";

const deploymentCheck = bundledCatalog.entries.find(({ id }) => id === "deployment-check")!;

afterEach(() => { cleanup(); vi.restoreAllMocks(); });

it("shows the bundled lock declaration for the exact reviewed template", () => {
  render(CatalogComponentPins, { entry: deploymentCheck });
  const panel = screen.getByRole("region", { name: "Declared template components" });
  expect(panel).toHaveTextContent("deployment-validation");
  expect(panel).toHaveTextContent("0.3.2");
  expect(panel).toHaveTextContent("eb8340a433cf1fe4ab75251ce369e3d58a9bd636");
  expect(panel).toHaveTextContent("Local file integrity requires checking a registered project");
  expect(panel).not.toHaveTextContent("Verified");
});

it("switches to Maester's exact lock and clears it for a different template path", async () => {
  const maester = bundledCatalog.entries.find(({ id }) => id === "maester-automation")!;
  const { rerender } = render(CatalogComponentPins, { entry: deploymentCheck });
  await rerender({ entry: maester });

  const panel = screen.getByRole("region", { name: "Declared template components" });
  const declarations = [
    ["deployment-receipt", "0.2.1"],
    ["deployment-validation", "1.1.1"],
    ["maester-azd-hooks", "0.1.0"],
    ["maester-report-webapp", "0.1.1"]
  ];
  expect(within(panel).getAllByRole("article")).toHaveLength(declarations.length);
  for (const [name, version] of declarations) {
    expect(within(panel).getByRole("article", { name })).toHaveTextContent(version);
  }
  expect(panel).not.toHaveTextContent("0.3.2");
  expect(panel).not.toHaveTextContent("Verified");

  await rerender({ entry: { ...maester, subdirectory: "container-app-job" } });
  expect(screen.queryByRole("region", { name: "Declared template components" })).not.toBeInTheDocument();
});

it("withholds the PIM guide when the selected source is no longer the reviewed artifact", async () => {
  const pilot = bundledCatalog.entries.find(({ id }) => id === "pim-pilot")!;
  const { rerender } = render(CatalogComponentPins, { entry: pilot });
  expect(screen.getByRole("button", { name: "Open update guide" })).toBeInTheDocument();

  for (const entry of [
    { ...pilot, source: "external" as const },
    { ...pilot, trusted: false },
    { ...pilot, commit: "0".repeat(40) },
    { ...pilot, subdirectory: "another-solution" },
    deploymentCheck
  ]) {
    await rerender({ entry });
    expect(screen.queryByRole("button", { name: "Open update guide" })).not.toBeInTheDocument();
  }
});

it("reports a browser opening failure in the current guide panel", async () => {
  const pilot = bundledCatalog.entries.find(({ id }) => id === "pim-pilot")!;
  vi.spyOn(bridge, "openExternalUrl").mockRejectedValue(new Error("Browser unavailable"));
  render(CatalogComponentPins, { entry: pilot });

  await fireEvent.click(screen.getByRole("button", { name: "Open update guide" }));

  expect(await screen.findByRole("alert")).toHaveTextContent("Could not open the update guide.");
});

const mismatches: [string, Partial<CatalogEntry>][] = [
  ["an external entry with matching repository and revision", { source: "external" }],
  ["an unreviewed entry", { trusted: false }],
  ["another catalog entry", { id: "another-example" }],
  ["a different repository", { repository: "https://github.com/nathanmcnulty/azd-myworkid" }],
  ["a different revision", { commit: "0000000000000000000000000000000000000000" }],
  ["a different template path", { subdirectory: "examples/other-template" }]
];

it.each(mismatches)("withholds the declaration for %s", (_scenario, fields) => {
  render(CatalogComponentPins, { entry: { ...deploymentCheck, ...fields } });
  expect(screen.queryByRole("region", { name: "Declared template components" })).not.toBeInTheDocument();
});

it("checks the exact Maester pins on open and supports a live breaking-update refresh", async () => {
  const maester = bundledCatalog.entries.find(({ id }) => id === "maester-automation")!;
  const fresh: componentUpdates.ComponentUpdates = {
    state: "fresh",
    checkedAt: new Date().toISOString(),
    updates: [
      { id: "deployment-receipt", state: "noNewer", version: null, compatibility: "unknown",
        complete: true, historyIncomplete: false, breakingVersions: [], migrationNotes: [] },
      { id: "deployment-validation", state: "updateAvailable", version: "2.0.0", compatibility: "breaking",
        complete: true, historyIncomplete: false, breakingVersions: ["2.0.0"],
        migrationNotes: ["Upgrade the host to PowerShell 7.6 before adopting this version."] }
    ]
  };
  const check = vi.spyOn(componentUpdates, "checkComponentUpdates").mockResolvedValue(fresh);
  const { rerender } = render(CatalogComponentPins, { entry: maester });
  expect(await screen.findByText("Breaking changes declared in 2.0.0.")).toBeInTheDocument();
  expect(screen.getByText("Upgrade the host to PowerShell 7.6 before adopting this version.")).toBeInTheDocument();
  expect(check).toHaveBeenNthCalledWith(1, expect.arrayContaining([
    expect.objectContaining({ id: "deployment-validation", version: "1.1.1",
      sourceRevision: "0c96cc89c554ffc3b3ca82ceda12da6591e816c1" })
  ]), false);
  await fireEvent.click(screen.getByRole("button", { name: "Check for updates" }));
  await waitFor(() => expect(check).toHaveBeenNthCalledWith(2, expect.any(Array), true));
  await rerender({ entry: { ...maester, trusted: false } });
  expect(screen.queryByText("Breaking changes declared in 2.0.0.")).not.toBeInTheDocument();
  expect(check).toHaveBeenCalledTimes(2);
});
it("ignores an update result after leaving the reviewed template", async () => {
  const maester = bundledCatalog.entries.find(({ id }) => id === "maester-automation")!;
  let resolve!: (value: componentUpdates.ComponentUpdates) => void;
  vi.spyOn(componentUpdates, "checkComponentUpdates").mockReturnValueOnce(
    new Promise((done) => { resolve = done; })
  ).mockResolvedValue({ state: "unavailable", checkedAt: null, updates: [] });
  const { rerender } = render(CatalogComponentPins, { entry: maester });
  await rerender({ entry: deploymentCheck });
  resolve({
    state: "fresh", checkedAt: new Date().toISOString(), updates: [{
      id: "deployment-validation", state: "updateAvailable", version: "99.0.0",
      compatibility: "breaking", complete: true, historyIncomplete: false,
      breakingVersions: ["99.0.0"], migrationNotes: []
    }]
  });
  await waitFor(() => expect(screen.getByText("Update information is unavailable.")).toBeInTheDocument());
  expect(screen.queryByText("99.0.0")).not.toBeInTheDocument();
});