import { invoke } from "@tauri-apps/api/core";

export interface Asset {
  id: string;
  characterId: string;
  originalFilename: string;
  mimeType: string;
  byteSize: number;
  width: number;
  height: number;
  artistName: string | null;
  sourceUrl: string | null;
  notes: string | null;
  createdAt: string;
}

export interface AssetInput {
  artistName?: string | null;
  sourceUrl?: string | null;
  notes?: string | null;
}

export type MediaErrorCode = "invalidMedia" | "mediaTooLarge" | "mediaBusy" | "cleanupPending" | "storageError" | "invalidInput" | "notFound" | "unsupportedDatabase";
export interface MediaCommandError { code: MediaErrorCode; message: string; applied?: boolean }
export interface MediaCleanupStatus { pending: number }

export function isAppliedCleanup(error: unknown): error is MediaCommandError & { applied: true } {
  return typeof error === "object" && error !== null && "code" in error && error.code === "cleanupPending" && "applied" in error && error.applied === true;
}

export function mediaErrorKey(error: unknown) {
  const code = typeof error === "object" && error !== null && "code" in error ? error.code : "storageError";
  switch (code) {
    case "invalidMedia": case "mediaTooLarge": case "mediaBusy": case "cleanupPending": case "invalidInput": case "notFound":
      return `gallery.errors.${code}`;
    default: return "gallery.errors.storageError";
  }
}

export function validateAssetInput(input: AssetInput): AssetInput {
  const result: AssetInput = {};
  for (const [field, limit] of [["artistName", 200], ["sourceUrl", 2048], ["notes", 10000]] as const) {
    if (input[field] === undefined) continue;
    const value = input[field]?.trim() || null;
    if (value && Array.from(value).length > limit) throw { code: "invalidInput", message: "Invalid image metadata." };
    result[field] = value;
  }
  if (result.sourceUrl) {
    let url: URL;
    try { url = new URL(result.sourceUrl); }
    catch { throw { code: "invalidInput", message: "Invalid image metadata." }; }
    if (!/^https?:$/.test(url.protocol) || url.username || url.password) throw { code: "invalidInput", message: "Invalid image metadata." };
  }
  return result;
}

export function listAssets(characterId: string) { return invoke<Asset[]>("list_assets", { characterId }); }
export function importAsset(characterId: string) { return invoke<Asset | null>("import_asset", { characterId }); }
export async function updateAsset(characterId: string, assetId: string, input: AssetInput) {
  return invoke<Asset>("update_asset", { characterId, assetId, input: validateAssetInput(input) });
}
export function deleteAsset(characterId: string, assetId: string) { return invoke<null>("delete_asset", { characterId, assetId }); }
export function mediaCleanupStatus() { return invoke<MediaCleanupStatus>("media_cleanup_status"); }
export function retryMediaCleanup() { return invoke<MediaCleanupStatus>("retry_media_cleanup"); }

const MAX_PREVIEW_BYTES = 4 * 1024 * 1024;
const PNG_SIGNATURE = [137, 80, 78, 71, 13, 10, 26, 10];
export async function readAssetPreview(characterId: string, assetId: string): Promise<Blob> {
  const payload = await invoke<ArrayBuffer | number[]>("read_asset_preview", { characterId, assetId });
  let bytes: Uint8Array<ArrayBuffer>;
  const invalid = () => ({ code: "invalidMedia", message: "Invalid image preview." });
  if (payload instanceof ArrayBuffer) {
    if (payload.byteLength > MAX_PREVIEW_BYTES) throw invalid();
    bytes = new Uint8Array(payload);
  } else if (Array.isArray(payload)) {
    if (payload.length > MAX_PREVIEW_BYTES || !payload.every((byte) => Number.isInteger(byte) && byte >= 0 && byte <= 255)) throw invalid();
    bytes = Uint8Array.from(payload);
  } else throw invalid();
  if (bytes.length < PNG_SIGNATURE.length || !PNG_SIGNATURE.every((byte, index) => bytes[index] === byte)) throw invalid();
  return new Blob([bytes], { type: "image/png" });
}
