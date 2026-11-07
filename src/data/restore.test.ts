import { beforeEach, describe, expect, it, vi } from "vitest";
import { restoreErrorCode, restoreLibraryBackup } from "./restore";

const { invoke } = vi.hoisted(() => ({ invoke: vi.fn() }));
vi.mock("@tauri-apps/api/core", () => ({ invoke }));

const result = { characterCount: 2, assetCount: 3, totalBytes: 65536 };

describe("library restore IPC", () => {
  beforeEach(() => invoke.mockReset());

  it("invokes only the no-argument native restore command", async () => {
    invoke.mockResolvedValue(result);
    await expect(restoreLibraryBackup()).resolves.toEqual(result);
    expect(invoke).toHaveBeenCalledExactlyOnceWith("restore_library_backup");
  });

  it("preserves native folder cancellation as null", async () => {
    invoke.mockResolvedValue(null);
    await expect(restoreLibraryBackup()).resolves.toBeNull();
  });

  it.each([
    { ...result, characterCount: 10001 },
    { ...result, characterCount: -1 },
    { ...result, assetCount: 1001 },
    { ...result, totalBytes: 2 * 1024 ** 3 + 1 },
    { ...result, totalBytes: 1.5 },
    { ...result, totalBytes: Number.NaN },
    undefined,
  ])("rejects malformed or out-of-contract results without exposing details: %j", async (payload) => {
    invoke.mockResolvedValue(payload);
    await expect(restoreLibraryBackup()).rejects.toEqual({ code: "restoreFailed" });
  });

  it("returns only the narrow DTO, never extra backend fields", async () => {
    invoke.mockResolvedValue({ ...result, path: "/private/library", message: "private notes" });
    await expect(restoreLibraryBackup()).resolves.toEqual(result);
  });

  it.each([
    "restoreBusy", "restoreNotEmpty", "restoreIntegrity", "restoreLimit",
    "restoreDestination", "restoreUnsupported", "restoreFailed",
  ])("allowlists fixed error code %s", (code) => {
    expect(restoreErrorCode({ code, message: "/private/raw backend details" })).toBe(code);
  });

  it.each([null, "raw backend details", new Error("/private/library"), { code: "storageError" }, { code: "toString" }])("maps unknown errors to the fixed fallback", (error) => {
    expect(restoreErrorCode(error)).toBe("restoreFailed");
  });
});
