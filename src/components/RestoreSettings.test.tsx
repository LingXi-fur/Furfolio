import { render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { useState } from "react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import i18n from "../i18n";
import { RestoreSettings } from "./RestoreSettings";

const { invoke } = vi.hoisted(() => ({ invoke: vi.fn() }));
vi.mock("@tauri-apps/api/core", () => ({ invoke }));

const result = { characterCount: 2, assetCount: 3, totalBytes: 65536 };

function Harness({ onRestored }: { onRestored?: () => void }) {
  const [open, setOpen] = useState(false);
  return <><button onClick={() => setOpen(true)}>Restore</button>{open && <RestoreSettings onClose={() => setOpen(false)} onRestored={onRestored ?? (() => {})} />}</>;
}

async function openRestore() {
  const user = userEvent.setup();
  render(<Harness />);
  await user.click(screen.getByRole("button", { name: "Restore" }));
  return user;
}

describe("RestoreSettings", () => {
  beforeEach(async () => {
    invoke.mockReset();
    invoke.mockResolvedValue(null);
    await i18n.changeLanguage("en");
  });

  it("opens a native modal and focuses Close without restoring until selected", async () => {
    const showModal = vi.fn(function(this: HTMLDialogElement) { this.setAttribute("open", ""); });
    const close = vi.fn(function(this: HTMLDialogElement) { this.removeAttribute("open"); });
    const showDescriptor = Object.getOwnPropertyDescriptor(HTMLDialogElement.prototype, "showModal");
    const closeDescriptor = Object.getOwnPropertyDescriptor(HTMLDialogElement.prototype, "close");
    Object.defineProperties(HTMLDialogElement.prototype, { showModal: { configurable: true, value: showModal }, close: { configurable: true, value: close } });
    try {
      const user = await openRestore();
      const dialog = screen.getByRole("dialog", { name: "Restore backup" });
      expect(showModal).toHaveBeenCalledOnce();
      expect(dialog).toHaveAccessibleDescription(/unencrypted backup directory.*checks every file against the backup's manifest/i);
      expect(screen.getByRole("button", { name: "Close" })).toHaveFocus();
      await user.click(screen.getByRole("button", { name: "Choose backup folder and restore" }));
      expect(invoke).toHaveBeenCalledExactlyOnceWith("restore_library_backup");
      await user.click(screen.getByRole("button", { name: "Close" }));
      expect(close).toHaveBeenCalledOnce();
    } finally {
      if (showDescriptor) Object.defineProperty(HTMLDialogElement.prototype, "showModal", showDescriptor);
      else Reflect.deleteProperty(HTMLDialogElement.prototype, "showModal");
      if (closeDescriptor) Object.defineProperty(HTMLDialogElement.prototype, "close", closeDescriptor);
      else Reflect.deleteProperty(HTMLDialogElement.prototype, "close");
    }
  });

  it("reports success and notifies the app to reload the library", async () => {
    const onRestored = vi.fn();
    render(<Harness onRestored={onRestored} />);
    invoke.mockResolvedValue(result);
    const user = userEvent.setup();
    await user.click(screen.getByRole("button", { name: "Restore" }));
    await user.click(screen.getByRole("button", { name: "Choose backup folder and restore" }));
    await waitFor(() => expect(screen.getByText("Backup restored")).toBeInTheDocument());
    expect(onRestored).toHaveBeenCalledOnce();
  });

  it("treats native null cancellation as neither success nor error", async () => {
    const onRestored = vi.fn();
    render(<Harness onRestored={onRestored} />);
    const user = userEvent.setup();
    await user.click(screen.getByRole("button", { name: "Restore" }));
    await user.click(screen.getByRole("button", { name: "Choose backup folder and restore" }));
    expect(screen.queryByRole("alert")).not.toBeInTheDocument();
    expect(screen.queryByRole("status")).not.toBeInTheDocument();
    expect(onRestored).not.toHaveBeenCalled();
    expect(screen.getByRole("button", { name: "Choose backup folder and restore" })).toBeEnabled();
  });

  it("shows a translated non-empty error without backend details", async () => {
    render(<Harness />);
    invoke.mockRejectedValue({ code: "restoreNotEmpty", message: "/private/backend path" });
    const user = userEvent.setup();
    await user.click(screen.getByRole("button", { name: "Restore" }));
    await user.click(screen.getByRole("button", { name: "Choose backup folder and restore" }));
    const alert = await screen.findByRole("alert");
    expect(within(alert).getByText(/restore is available only into an empty library/i)).toBeInTheDocument();
    expect(alert.textContent).not.toContain("/private");
  });
});
