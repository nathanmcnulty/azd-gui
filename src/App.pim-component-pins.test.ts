import { cleanup, fireEvent, render, screen } from "@testing-library/svelte";
import { afterEach, expect, it, vi } from "vitest";
import App from "./App.svelte";
import * as bridge from "./lib/bridge";
import { mockBootstrap } from "./lib/mock-data";
import * as pimPlan from "./lib/pim-plan";
import * as pimRoles from "./lib/pim-roles";
import type { CatalogEntry } from "./lib/types";

const nativeSource = vi.hoisted(() => ({ commitOverride: "" }));
vi.mock("../src-tauri/pim-template/source.json", async (importOriginal) => {
  const { default: source } = await importOriginal<{ default: typeof import("../src-tauri/pim-template/source.json") }>();
  return { default: { ...source, get commit() { return nativeSource.commitOverride || source.commit; } } };
});

const pilot = mockBootstrap.catalog.entries.find(({ id }) => id === "pim-pilot")!;

afterEach(() => {
  cleanup();
  vi.restoreAllMocks();
  nativeSource.commitOverride = "";
  window.localStorage.clear();
});

async function openPilot(entry: CatalogEntry) {
  vi.spyOn(bridge, "bootstrap").mockResolvedValue({
    ...structuredClone(mockBootstrap),
    catalog: { ...mockBootstrap.catalog, entries: [entry] }
  });
  vi.spyOn(bridge, "onOperationEvent").mockResolvedValue(() => {});
  render(App);
  await fireEvent.click(await screen.findByRole("button", { name: /browse catalog/i }));
  await fireEvent.click(await screen.findByRole("button", { name: /configure pim pilot/i }));
  await screen.findByRole("heading", { name: "Selected-role PIM pilot" });
}

it.each([null, undefined])("shows the reviewed source template's PIM pins before sign-in (root=%s)", async (subdirectory) => {
  const connect = vi.spyOn(pimPlan, "connectPimPlan");
  const refreshRoles = vi.spyOn(pimRoles, "refreshPimRoles");
  const provenance = vi.spyOn(bridge, "getProjectComponentProvenance");

  await openPilot({ ...pilot, subdirectory });

  const panel = screen.getByRole("region", { name: "Declared template components" });
  expect(panel).toHaveTextContent("graph-delegated-authentication");
  expect(panel).toHaveTextContent("flex-scheduled-poller-host");
  expect(panel).toHaveTextContent("Local file integrity requires checking a registered project");
  expect(panel).not.toHaveTextContent("Verified");
  expect(connect).not.toHaveBeenCalled();
  expect(refreshRoles).not.toHaveBeenCalled();
  expect(provenance).not.toHaveBeenCalled();
});

it("withholds the declaration when the catalog and native PIM pins differ", async () => {
  nativeSource.commitOverride = "0".repeat(40);
  await openPilot(pilot);

  expect(screen.queryByRole("region", { name: "Declared template components" })).not.toBeInTheDocument();
});
