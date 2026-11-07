import { invoke } from "@tauri-apps/api/core";

export interface RestoreResult {
  characterCount: number;
  assetCount: number;
  /** Database and media payload bytes; excludes manifest.json. */
  totalBytes: number;
}

const restoreErrorCodes = [
  "restoreBusy", "restoreNotEmpty", "restoreIntegrity", "restoreLimit",
  "restoreDestination", "restoreUnsupported", "restoreFailed",
] as const;
export type RestoreErrorCode = typeof restoreErrorCodes[number];

function record(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null;
}

function boundedCount(value: unknown, max: number): value is number {
  return typeof value === "number" && Number.isSafeInteger(value) && value >= 0 && value <= max;
}

export async function restoreLibraryBackup(): Promise<RestoreResult | null> {
  const result = await invoke<unknown>("restore_library_backup");
  if (result === null) return null;
  if (!record(result)
    || !boundedCount(result.characterCount, 10000)
    || !boundedCount(result.assetCount, 1000)
    || !boundedCount(result.totalBytes, 2 * 1024 ** 3)) {
    throw { code: "restoreFailed" };
  }
  const { characterCount, assetCount, totalBytes } = result;
  return { characterCount, assetCount, totalBytes };
}

/** Never display backend messages or source paths. */
export function restoreErrorCode(error: unknown): RestoreErrorCode {
  return record(error) && restoreErrorCodes.some((code) => code === error.code)
    ? error.code as RestoreErrorCode
    : "restoreFailed";
}
