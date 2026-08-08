import { describe, expect, it } from "vitest";
import { previewExternalCatalog, saveCatalogSource } from "./bridge";

describe("external catalog review", () => {
  it("accepts the exact preview once and preserves namespaced entry IDs", async () => {
    const sourceUrl = "https://example.com/azd-gui.catalog.json";
    const preview = await previewExternalCatalog(sourceUrl);

    expect(preview.catalog.entries.length).toBeGreaterThan(0);
    expect(
      preview.catalog.entries.every((entry) =>
        entry.id.startsWith(`external-${preview.catalog.id}.`)
      )
    ).toBe(true);

    const accepted = await saveCatalogSource(sourceUrl, preview.reviewToken);
    expect(accepted).toEqual(preview.catalog);
    await expect(saveCatalogSource(sourceUrl, preview.reviewToken)).rejects.toThrow(
      /review expired/i
    );
  });

  it("rejects a source URL changed after preview", async () => {
    const preview = await previewExternalCatalog("https://example.com/first.json");

    await expect(
      saveCatalogSource("https://example.com/second.json", preview.reviewToken)
    ).rejects.toThrow(/review expired/i);
  });
});
