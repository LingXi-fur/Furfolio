import { beforeEach, describe, expect, it, vi } from "vitest";
import { backupErrorCode, backupErrorDirectoryName, exportLibraryBackup } from "./backup";

const { invoke } = vi.hoisted(() => ({ invoke: vi.fn() }));
vi.mock("@tauri-apps/api/core", () => ({ invoke }));

const directoryName = "furfolio-backup-20261102T120000Z-11111111-1111-4111-8111-111111111111";
const result = { directoryName, characterCount: 2, assetCount: 3, totalBytes: 65536 };

describe("library backup IPC", () => {
  beforeEach(() => invoke.mockReset());

  it("invokes only the no-argument native export command", async () => {
    invoke.mockResolvedValue(result);
    await expect(exportLibraryBackup()).resolves.toEqual(result);
    expect(invoke).toHaveBeenCalledExactlyOnceWith("export_library_backup");
  });

  it("preserves native folder cancellation as null", async () => {
    invoke.mockResolvedValue(null);
    await expect(exportLibraryBackup()).resolves.toBeNull();
  });

  it.each([
    { ...result, directoryName: "/Users/private/library" },
    { ...result, directoryName: "../furfolio-backup-secret" },
    { ...result, directoryName: "furfolio-backup-not-a-uuid" },
    { ...result, directoryName: `${directoryName}\n` },
    { ...result, directoryName: directoryName.replace("4111", "3111") },
    { ...result, characterCount: 10001 },
    { ...result, characterCount: -1 },
    { ...result, assetCount: 1001 },
    { ...result, totalBytes: 2 * 1024 ** 3 + 1 },
    { ...result, totalBytes: 1.5 },
    { ...result, totalBytes: Number.NaN },
    undefined,
  ])("rejects malformed or out-of-contract results without exposing details: %j", async (payload) => {
    invoke.mockResolvedValue(payload);
    await expect(exportLibraryBackup()).rejects.toEqual({ code: "backupFailed" });
  });

  it("returns only the narrow DTO, never extra backend fields", async () => {
    invoke.mockResolvedValue({ ...result, path: "/private/library", message: "private notes" });
    await expect(exportLibraryBackup()).resolves.toEqual(result);
  });

  it.each([
    "backupBusy", "backupPending", "backupIntegrity", "backupLimit", "backupTimeout",
    "backupNoSpace", "backupDestination", "backupUnsupported", "backupFailed",
    "backupDurabilityUncertain", "backupCleanupPending",
  ])("allowlists fixed error code %s", (code) => {
    expect(backupErrorCode({ code, message: "/private/raw backend details" })).toBe(code);
  });

  it.each([null, "raw backend details", new Error("/private/library"), { code: "storageError" }, { code: "toString" }])("maps unknown errors to the fixed fallback", (error) => {
    expect(backupErrorCode(error)).toBe("backupFailed");
  });

  it("exposes only a generated name for a published uncertain backup", () => {
    expect(backupErrorDirectoryName({ code: "backupDurabilityUncertain", published: true, directoryName })).toBe(directoryName);
  });

  it("accepts only the defined partial generated name for cleanup-pending", () => {
    const partial = ".furfolio-backup-11111111-1111-4111-8111-111111111111.partial";
    expect(backupErrorDirectoryName({ code: "backupCleanupPending", directoryName: partial })).toBe(partial);
    expect(backupErrorDirectoryName({ code: "backupCleanupPending", directoryName })).toBeNull();
    expect(backupErrorDirectoryName({ code: "backupCleanupPending", directoryName: `${partial}\n` })).toBeNull();
    expect(backupErrorDirectoryName({ code: "backupCleanupPending", directoryName: "/private/partial" })).toBeNull();
  });

  it.each([
    { code: "backupDurabilityUncertain", published: false, directoryName },
    { code: "backupDurabilityUncertain", directoryName },
    { code: "backupFailed", published: true, directoryName },
    { code: "backupDurabilityUncertain", published: true, directoryName: "/Users/private/library" },
    { code: "backupDurabilityUncertain", published: true, directoryName: "furfolio-backup-secret/private" },
  ])("never echoes arbitrary error paths or unverified publication", (error) => {
    expect(backupErrorDirectoryName(error)).toBeNull();
  });
});
