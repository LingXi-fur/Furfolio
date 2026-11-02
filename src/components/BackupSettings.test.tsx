import { fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { act, useState } from "react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import i18n from "../i18n";
import { BackupSettings } from "./BackupSettings";

const { invoke } = vi.hoisted(() => ({ invoke: vi.fn() }));
vi.mock("@tauri-apps/api/core", () => ({ invoke }));

const directoryName = "furfolio-backup-20261102T120000Z-11111111-1111-4111-8111-111111111111";
const result = { directoryName, characterCount: 2, assetCount: 3, totalBytes: 65536 };

function Harness() {
  const [open, setOpen] = useState(false);
  return <><button onClick={() => setOpen(true)}>Settings</button>{open && <BackupSettings onClose={() => setOpen(false)} />}</>;
}

function deferred<T>() {
  let resolve!: (value: T) => void;
  const promise = new Promise<T>((done) => { resolve = done; });
  return { promise, resolve };
}

async function openSettings() {
  const user = userEvent.setup();
  render(<Harness />);
  await user.click(screen.getByRole("button", { name: "Settings" }));
  return user;
}

async function confirmAndExport(user: ReturnType<typeof userEvent.setup>) {
  await user.click(screen.getByRole("checkbox"));
  await user.click(screen.getByRole("button", { name: "Choose folder and export" }));
}

describe("BackupSettings", () => {
  beforeEach(async () => {
    invoke.mockReset();
    invoke.mockResolvedValue(null);
    await i18n.changeLanguage("en");
  });

  it("opens a native modal and focuses Close without exporting until informed confirmation", async () => {
    const showModal = vi.fn(function(this: HTMLDialogElement) { this.setAttribute("open", ""); });
    const close = vi.fn(function(this: HTMLDialogElement) { this.removeAttribute("open"); });
    const showDescriptor = Object.getOwnPropertyDescriptor(HTMLDialogElement.prototype, "showModal");
    const closeDescriptor = Object.getOwnPropertyDescriptor(HTMLDialogElement.prototype, "close");
    Object.defineProperties(HTMLDialogElement.prototype, { showModal: { configurable: true, value: showModal }, close: { configurable: true, value: close } });
    try {
      const user = await openSettings();
      const dialog = screen.getByRole("dialog", { name: "Library backup" });
      expect(showModal).toHaveBeenCalledOnce();
      expect(dialog).toHaveAccessibleDescription(/unencrypted.*private notes.*original images.*SQLite pages.*historical content/i);
      expect(within(dialog).getByText(/restore is not implemented/i)).toBeInTheDocument();
      expect(within(dialog).getByText(/macOS only.*64 MiB.*10,000.*1,000.*2 GiB.*2 MiB/i)).toBeInTheDocument();
      expect(screen.getByRole("button", { name: "Close settings" })).toHaveFocus();
      const button = screen.getByRole("button", { name: "Choose folder and export" });
      expect(button).toBeDisabled();
      await user.click(button);
      expect(invoke).not.toHaveBeenCalled();
      await confirmAndExport(user);
      expect(invoke).toHaveBeenCalledExactlyOnceWith("export_library_backup");
      await user.click(screen.getByRole("button", { name: "Close settings" }));
      expect(close).toHaveBeenCalledOnce();
    } finally {
      if (showDescriptor) Object.defineProperty(HTMLDialogElement.prototype, "showModal", showDescriptor);
      else Reflect.deleteProperty(HTMLDialogElement.prototype, "showModal");
      if (closeDescriptor) Object.defineProperty(HTMLDialogElement.prototype, "close", closeDescriptor);
      else Reflect.deleteProperty(HTMLDialogElement.prototype, "close");
    }
  });

  it("treats native null cancellation as neither success nor error and permits another attempt", async () => {
    const user = await openSettings();
    await confirmAndExport(user);
    expect(screen.queryByRole("alert")).not.toBeInTheDocument();
    expect(screen.queryByRole("status")).not.toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Choose folder and export" })).toBeEnabled();
    await user.click(screen.getByRole("button", { name: "Choose folder and export" }));
    expect(invoke).toHaveBeenCalledTimes(2);
  });

  it("guards synchronous double activation and blocks close and Escape while the native request is pending", async () => {
    const pending = deferred<null>();
    invoke.mockReturnValue(pending.promise);
    const user = await openSettings();
    await user.click(screen.getByRole("checkbox"));
    const exportButton = screen.getByRole("button", { name: "Choose folder and export" });
    act(() => { exportButton.click(); exportButton.click(); });
    expect(invoke).toHaveBeenCalledOnce();
    expect(screen.getByRole("dialog")).toHaveAttribute("aria-busy", "true");
    expect(screen.getByRole("status")).toHaveTextContent("Choosing a folder or exporting…");
    expect(screen.getByRole("checkbox")).toBeDisabled();
    expect(screen.getByRole("button", { name: "Close settings" })).toBeDisabled();
    await user.keyboard("{Escape}");
    const cancel = new Event("cancel", { cancelable: true });
    fireEvent(screen.getByRole("dialog"), cancel);
    expect(cancel.defaultPrevented).toBe(true);
    expect(screen.getByRole("dialog")).toBeInTheDocument();
    await act(async () => pending.resolve(null));
    expect(screen.getByRole("button", { name: "Close settings" })).toBeEnabled();
  });

  it("renders only the generated directory name and exact payload counts on success", async () => {
    invoke.mockResolvedValue({ ...result, path: "/private/secret", message: "backend secret" });
    const user = await openSettings();
    await confirmAndExport(user);
    const status = screen.getByRole("status");
    expect(status).toHaveTextContent("Backup exported");
    expect(status).toHaveTextContent(directoryName);
    expect(status).toHaveTextContent("2 characters");
    expect(status).toHaveTextContent("3 images");
    expect(status).toHaveTextContent("65,536 payload bytes");
    expect(status).not.toHaveTextContent(/private|secret/);
    expect(screen.getByText(/payload bytes include the database and media, not the manifest/i)).toBeInTheDocument();
  });

  it.each(["backupBusy", "backupPending", "backupIntegrity", "backupLimit", "backupTimeout", "backupNoSpace", "backupDestination", "backupUnsupported", "backupFailed", "unknownCode"])("localizes %s without raw details and allows retry", async (code) => {
    invoke.mockRejectedValueOnce({ code, message: "/Users/private/raw SQL failure" }).mockResolvedValueOnce(result);
    const user = await openSettings();
    await confirmAndExport(user);
    const alert = screen.getByRole("alert");
    expect(alert).toHaveTextContent(i18n.t(`backup.errors.${code === "unknownCode" ? "backupFailed" : code}`));
    expect(alert).not.toHaveTextContent(/Users|raw SQL/);
    await user.click(screen.getByRole("button", { name: "Try export again" }));
    expect(screen.getByRole("status")).toHaveTextContent("Backup exported");
    expect(invoke).toHaveBeenCalledTimes(2);
  });

  it("warns separately when publication durability is uncertain and does not offer blind retry", async () => {
    invoke.mockRejectedValue({ code: "backupDurabilityUncertain", published: true, directoryName, message: "/private/secret sync failed" });
    const user = await openSettings();
    await confirmAndExport(user);
    const alert = screen.getByRole("alert");
    expect(alert).toHaveTextContent(i18n.t("backup.errors.backupDurabilityUncertain"));
    expect(alert).toHaveTextContent(directoryName);
    expect(alert).toHaveTextContent(/do not retry blindly/i);
    expect(alert).not.toHaveTextContent(/secret|sync failed/);
    expect(screen.queryByRole("status")).not.toBeInTheDocument();
    expect(screen.queryByRole("button", { name: /export|retry/i })).not.toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Close settings" })).toBeEnabled();
  });

  it("keeps uncertain semantics even when the backend supplies an unsafe directory name", async () => {
    invoke.mockRejectedValue({ code: "backupDurabilityUncertain", published: true, directoryName: "/Users/private/secret", message: "secret" });
    const user = await openSettings();
    await confirmAndExport(user);
    expect(screen.getByRole("alert")).toHaveTextContent(/do not retry blindly/i);
    expect(screen.getByRole("alert")).not.toHaveTextContent(/Users|secret/);
    expect(screen.queryByRole("button", { name: /export|retry/i })).not.toBeInTheDocument();
  });

  it("warns about partial remnants for cleanup-pending without echoing a path or claiming success", async () => {
    invoke.mockRejectedValue({ code: "backupCleanupPending", directoryName: "/private/residue", message: "secret" });
    const user = await openSettings();
    await confirmAndExport(user);
    expect(screen.getByRole("alert")).toHaveTextContent(/partial backup directory.*may remain/i);
    expect(screen.getByRole("alert")).not.toHaveTextContent(/private|residue|secret/);
    expect(screen.queryByRole("status")).not.toBeInTheDocument();
  });

  it("can retry a cleanup-pending export while showing only a safe partial generated name", async () => {
    const partial = ".furfolio-backup-11111111-1111-4111-8111-111111111111.partial";
    invoke.mockRejectedValueOnce({ code: "backupCleanupPending", directoryName: partial }).mockResolvedValueOnce(result);
    const user = await openSettings();
    await confirmAndExport(user);
    expect(screen.getByRole("alert")).toHaveTextContent(partial);
    expect(screen.getByRole("alert")).toHaveTextContent(/not a complete backup or a success/i);
    await user.click(screen.getByRole("button", { name: "Try export again" }));
    expect(screen.getByRole("status")).toHaveTextContent("Backup exported");
    expect(invoke).toHaveBeenCalledTimes(2);
  });

  it("does not retain stale success after a later native cancellation", async () => {
    invoke.mockResolvedValueOnce(result).mockResolvedValueOnce(null);
    const user = await openSettings();
    await confirmAndExport(user);
    expect(screen.getByRole("status")).toHaveTextContent("Backup exported");
    await user.click(screen.getByRole("button", { name: "Choose folder and export" }));
    expect(screen.queryByRole("status")).not.toBeInTheDocument();
    expect(screen.queryByRole("alert")).not.toBeInTheDocument();
  });

  it("closes on Escape, returns focus, and resets confirmation when reopened", async () => {
    const user = await openSettings();
    await user.click(screen.getByRole("checkbox"));
    await user.keyboard("{Escape}");
    await waitFor(() => expect(screen.queryByRole("dialog")).not.toBeInTheDocument());
    expect(screen.getByRole("button", { name: "Settings" })).toHaveFocus();
    await user.click(screen.getByRole("button", { name: "Settings" }));
    expect(screen.getByRole("checkbox")).not.toBeChecked();
    expect(invoke).not.toHaveBeenCalled();
    fireEvent(screen.getByRole("dialog"), new Event("cancel", { cancelable: true }));
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Settings" })).toHaveFocus();
  });

  it("provides Chinese warnings, controls, counts and fixed error messages", async () => {
    await i18n.changeLanguage("zh-CN");
    const user = await openSettings();
    expect(screen.getByRole("dialog", { name: "资料库备份" })).toHaveAccessibleDescription(/未加密.*私密备注.*原图.*SQLite.*历史内容/);
    expect(screen.getByText(/尚未实现恢复/)).toBeInTheDocument();
    invoke.mockRejectedValueOnce({ code: "backupNoSpace", message: "secret" }).mockResolvedValueOnce(result);
    await user.click(screen.getByRole("checkbox", { name: /我已了解/ }));
    await user.click(screen.getByRole("button", { name: "选择文件夹并导出" }));
    expect(screen.getByRole("alert")).toHaveTextContent(i18n.t("backup.errors.backupNoSpace"));
    await user.click(screen.getByRole("button", { name: "重新尝试导出" }));
    expect(screen.getByRole("status")).toHaveTextContent("2 位角色");
    expect(screen.getByRole("status")).toHaveTextContent("3 张图片");
    expect(screen.getByRole("status")).toHaveTextContent("65,536 字节有效载荷");
  });
});
