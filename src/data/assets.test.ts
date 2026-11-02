import { beforeEach, describe, expect, it, vi } from "vitest";

const { invoke } = vi.hoisted(() => ({ invoke: vi.fn() }));
vi.mock("@tauri-apps/api/core", () => ({ invoke }));

import {
  deleteAsset, importAsset, isAppliedCleanup, listAssets, mediaCleanupStatus,
  readAssetPreview, retryMediaCleanup, updateAsset, validateAssetInput,
} from "./assets";

const png = [137, 80, 78, 71, 13, 10, 26, 10, 0];

describe("asset IPC boundary", () => {
  beforeEach(() => { invoke.mockReset(); });

  it("uses camelCase IDs and the native single-file import contract", async () => {
    invoke.mockResolvedValue(null);
    await listAssets("character");
    expect(await importAsset("character")).toBeNull();
    await updateAsset("character", "asset", { artistName: "  Artist  ", sourceUrl: "  https://example.com/a  ", notes: "  note  " });
    await deleteAsset("character", "asset");
    await mediaCleanupStatus();
    await retryMediaCleanup();
    expect(invoke.mock.calls).toEqual([
      ["list_assets", { characterId: "character" }],
      ["import_asset", { characterId: "character" }],
      ["update_asset", { characterId: "character", assetId: "asset", input: { artistName: "Artist", sourceUrl: "https://example.com/a", notes: "note" } }],
      ["delete_asset", { characterId: "character", assetId: "asset" }],
      ["media_cleanup_status"], ["retry_media_cleanup"],
    ]);
  });

  it.each([png, Uint8Array.from(png).buffer])("accepts bounded binary PNG previews as number[] or ArrayBuffer", async (binary) => {
    invoke.mockResolvedValue(binary);
    const blob = await readAssetPreview("character", "asset");
    expect(blob.type).toBe("image/png");
    expect(blob.size).toBe(png.length);
    expect(invoke).toHaveBeenCalledWith("read_asset_preview", { characterId: "character", assetId: "asset" });
  });

  it.each([[], [1, 2, 3], [...png, -1], [...png, 256], [...png, 1.5], "file:///private/image.png", new ArrayBuffer(4 * 1024 * 1024 + 1)])("rejects invalid or oversized preview payloads", async (binary) => {
    invoke.mockResolvedValue(binary);
    await expect(readAssetPreview("character", "asset")).rejects.toMatchObject({ code: "invalidMedia" });
  });

  it("normalizes empty metadata and preserves omitted fields", () => {
    expect(validateAssetInput({ artistName: "  ", sourceUrl: null, notes: "  " })).toEqual({ artistName: null, sourceUrl: null, notes: null });
    expect(validateAssetInput({})).toEqual({});
    expect(validateAssetInput({ artistName: "a".repeat(200), notes: "n".repeat(10000) }).artistName).toHaveLength(200);
  });

  it.each([
    { artistName: "a".repeat(201) }, { notes: "n".repeat(10001) },
    { sourceUrl: `https://example.com/${"a".repeat(2048)}` },
    ...["javascript:alert(1)", "file:///tmp/private", "https://user:pass@example.com", "https://user@example.com", "not a URL"].map((sourceUrl) => ({ sourceUrl })),
  ])("rejects invalid metadata before IPC", async (input) => {
    await expect(updateAsset("character", "asset", input)).rejects.toMatchObject({ code: "invalidInput" });
    expect(invoke).not.toHaveBeenCalled();
  });

  it("only treats explicit committed cleanup errors as applied", () => {
    expect(isAppliedCleanup({ code: "cleanupPending", applied: true })).toBe(true);
    expect(isAppliedCleanup({ code: "cleanupPending" })).toBe(false);
    expect(isAppliedCleanup({ code: "storageError", applied: true })).toBe(false);
  });
});
