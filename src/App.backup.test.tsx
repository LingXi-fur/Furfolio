import { fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import i18n from "./i18n";
import App from "./App";

const data = vi.hoisted(() => ({ listCharacters: vi.fn(), getCharacter: vi.fn(), createCharacter: vi.fn(), updateCharacter: vi.fn(), deleteCharacter: vi.fn() }));
vi.mock("./data/characters", () => data);
const media = vi.hoisted(() => ({ listAssets: vi.fn(), mediaCleanupStatus: vi.fn() }));
vi.mock("./data/assets", async (original) => ({ ...await original<typeof import("./data/assets")>(), ...media }));
const { invoke } = vi.hoisted(() => ({ invoke: vi.fn() }));
vi.mock("@tauri-apps/api/core", () => ({ invoke }));

const character = {
  id: "synthetic-id", name: "Synthetic Nova", avatarAssetId: null, species: "Wolf", pronouns: null,
  description: "Synthetic description", notes: "Synthetic private notes", tags: [], colors: [],
  createdAt: "2026-01-01T00:00:00Z", updatedAt: "2026-01-01T00:00:00Z",
};

describe("App backup settings integration", () => {
  beforeEach(async () => {
    localStorage.clear();
    await i18n.changeLanguage("en");
    Object.values(data).forEach((mock) => mock.mockReset());
    data.listCharacters.mockResolvedValue([character]);
    data.getCharacter.mockResolvedValue(character);
    media.listAssets.mockResolvedValue([]);
    media.mediaCleanupStatus.mockResolvedValue({ pending: 0 });
    invoke.mockReset();
    invoke.mockResolvedValue(null);
  });

  it("preserves the mounted detail and returns focus without refetching or exporting", async () => {
    const user = userEvent.setup();
    render(<App />);
    await user.click(await screen.findByRole("button", { name: /open synthetic nova.*featured plate/i }));
    const detail = screen.getByRole("heading", { name: character.name });
    const settings = screen.getByRole("button", { name: "Settings" });
    expect(settings).toBeEnabled();
    await user.click(settings);
    expect(screen.getByRole("dialog", { name: "Library backup" })).toBeInTheDocument();
    expect(detail).toBeInTheDocument();
    await user.keyboard("{Escape}");
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
    expect(screen.getByRole("heading", { name: character.name })).toBe(detail);
    expect(settings).toHaveFocus();
    expect(data.getCharacter).toHaveBeenCalledTimes(1);
    expect(invoke).not.toHaveBeenCalled();
  });

  it.each(["close button", "native cancel"])("returns settings focus to its explicit trigger after %s when mouse activation did not focus the button", async (dismissal) => {
    const user = userEvent.setup();
    render(<App />);
    await user.click(await screen.findByRole("button", { name: /open synthetic nova.*featured plate/i }));
    const settings = screen.getByRole("button", { name: "Settings" });
    screen.getByRole("button", { name: "Edit record" }).focus();
    fireEvent.click(settings);
    const dialog = screen.getByRole("dialog", { name: "Library backup" });
    if (dismissal === "close button") await user.click(within(dialog).getByRole("button", { name: "Close settings" }));
    else fireEvent(dialog, new Event("cancel", { cancelable: true }));
    expect(settings).toHaveFocus();
    expect(screen.getByRole("heading", { name: character.name })).toBeInTheDocument();
  });

  it("preserves the exact edit form and unsaved draft across settings close and reopen", async () => {
    const user = userEvent.setup();
    render(<App />);
    await user.click(await screen.findByRole("button", { name: /open synthetic nova.*featured plate/i }));
    await user.click(screen.getByRole("button", { name: "Edit record" }));
    const notes = screen.getByRole("textbox", { name: "Private notes" });
    await user.type(notes, " unsaved draft");
    await user.click(screen.getByRole("button", { name: "Settings" }));
    await user.keyboard("{Escape}");
    expect(screen.getByRole("textbox", { name: "Private notes" })).toBe(notes);
    expect(notes).toHaveValue("Synthetic private notes unsaved draft");
    await user.click(screen.getByRole("button", { name: "Settings" }));
    expect(screen.getByRole("checkbox")).not.toBeChecked();
    await user.click(screen.getByRole("button", { name: "Close settings" }));
    expect(screen.getByRole("textbox", { name: "Private notes" })).toBe(notes);
    expect(notes).toHaveValue("Synthetic private notes unsaved draft");
    expect(data.updateCharacter).not.toHaveBeenCalled();
    expect(data.getCharacter).toHaveBeenCalledTimes(1);
  });

  it.each(["cancel button", "Escape"])("returns character deletion focus to its trigger after %s even when another control was active", async (dismissal) => {
    const user = userEvent.setup();
    render(<App />);
    await user.click(await screen.findByRole("button", { name: /open synthetic nova.*featured plate/i }));
    const trigger = screen.getByRole("button", { name: "Delete" });
    screen.getByRole("button", { name: "Edit record" }).focus();
    fireEvent.click(trigger);
    const dialog = screen.getByRole("dialog", { name: /delete synthetic nova/i });
    if (dismissal === "cancel button") await user.click(within(dialog).getByRole("button", { name: "Cancel" }));
    else fireEvent(dialog, new Event("cancel", { cancelable: true }));
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
    expect(trigger).toHaveFocus();
    expect(data.deleteCharacter).not.toHaveBeenCalled();
  });

  it("keeps settings and character deletion dialogs separate with their own Escape and focus return", async () => {
    const user = userEvent.setup();
    render(<App />);
    await user.click(await screen.findByRole("button", { name: /open synthetic nova.*featured plate/i }));
    const deleteButton = screen.getByRole("button", { name: "Delete" });
    await user.click(deleteButton);
    const deletion = screen.getByRole("dialog", { name: /delete synthetic nova/i });
    // A programmatic click cannot stack settings above an existing native modal.
    fireEvent.click(screen.getByRole("button", { name: "Settings" }));
    expect(screen.getAllByRole("dialog")).toHaveLength(1);
    expect(screen.queryByRole("dialog", { name: "Library backup" })).not.toBeInTheDocument();
    fireEvent(deletion, new Event("cancel", { cancelable: true }));
    await waitFor(() => expect(deleteButton).toHaveFocus());
    await user.click(screen.getByRole("button", { name: "Settings" }));
    const settings = screen.getByRole("dialog", { name: "Library backup" });
    expect(within(settings).queryByText("Permanent action")).not.toBeInTheDocument();
    await user.keyboard("{Escape}");
    await user.click(deleteButton);
    expect(screen.getByRole("dialog", { name: /delete synthetic nova/i })).toBeInTheDocument();
    expect(screen.getAllByRole("dialog")).toHaveLength(1);
    expect(data.deleteCharacter).not.toHaveBeenCalled();
    expect(invoke).not.toHaveBeenCalled();
  });
});
