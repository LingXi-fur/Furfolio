import { invoke } from "@tauri-apps/api/core";

export interface BackupResult {
  directoryName: string;
  characterCount: number;
  assetCount: number;
  /** Database and media payload bytes; excludes manifest.json. */
  totalBytes: number;
}

const backupErrorCodes = [
  "backupBusy", "backupPending", "backupIntegrity", "backupLimit", "backupTimeout",
  "backupNoSpace", "backupDestination", "backupUnsupported", "backupFailed",
  "backupDurabilityUncertain", "backupCleanupPending",
] as const;
export type BackupErrorCode = typeof backupErrorCodes[number];

const uuidV4 = "[0-9a-f]{8}-[0-9a-f]{4}-4[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}";
const finalDirectory = new RegExp(`^furfolio-backup-[0-9]{8}T[0-9]{6}Z-${uuidV4}$`);
const partialDirectory = new RegExp(`^\\.furfolio-backup-${uuidV4}\\.partial$`);

function record(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null;
}

function boundedCount(value: unknown, max: number): value is number {
  return typeof value === "number" && Number.isSafeInteger(value) && value >= 0 && value <= max;
}

export async function exportLibraryBackup(): Promise<BackupResult | null> {
  const result = await invoke<unknown>("export_library_backup");
  if (result === null) return null;
  if (!record(result) || typeof result.directoryName !== "string" || !finalDirectory.test(result.directoryName)
    || !boundedCount(result.characterCount, 10000) || !boundedCount(result.assetCount, 1000)
    || !boundedCount(result.totalBytes, 2 * 1024 ** 3)) {
    throw { code: "backupFailed" };
  }
  const { directoryName, characterCount, assetCount, totalBytes } = result;
  return { directoryName, characterCount, assetCount, totalBytes };
}

export function backupErrorCode(error: unknown): BackupErrorCode {
  return record(error) && backupErrorCodes.some((code) => code === error.code)
    ? error.code as BackupErrorCode
    : "backupFailed";
}

/** Never display backend messages, source paths, or arbitrary destination names. */
export function backupErrorDirectoryName(error: unknown): string | null {
  if (!record(error) || typeof error.directoryName !== "string") return null;
  if (error.code === "backupDurabilityUncertain" && error.published === true && finalDirectory.test(error.directoryName)) return error.directoryName;
  if (error.code === "backupCleanupPending" && partialDirectory.test(error.directoryName)) return error.directoryName;
  return null;
}
