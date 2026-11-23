import { cleanup, render, screen } from "@testing-library/svelte";
import { afterEach, expect, it } from "vitest";
import CatalogComponentPins from "./CatalogComponentPins.svelte";
import { bundledCatalog } from "../lib/mock-data";
import type { CatalogEntry } from "../lib/types";

const deploymentCheck = bundledCatalog.entries.find(({ id }) => id === "deployment-check")!;

afterEach(cleanup);

it("shows the bundled lock declaration for the exact reviewed template", () => {
  render(CatalogComponentPins, { entry: deploymentCheck });
  const panel = screen.getByRole("region", { name: "Declared template components" });
  expect(panel).toHaveTextContent("deployment-validation");
  expect(panel).toHaveTextContent("0.3.2");
  expect(panel).toHaveTextContent("eb8340a433cf1fe4ab75251ce369e3d58a9bd636");
  expect(panel).toHaveTextContent("Local file integrity is checked separately");
  expect(panel).not.toHaveTextContent("Verified");
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
