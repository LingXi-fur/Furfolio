import { fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { act } from "react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import i18n from "./i18n";
import App from "./App";
import { setSystemTheme } from "./test/setup";

const data = vi.hoisted(() => ({
  listCharacters: vi.fn(),
  getCharacter: vi.fn(),
  createCharacter: vi.fn(),
  updateCharacter: vi.fn(),
  deleteCharacter: vi.fn(),
}));

vi.mock("./data/characters", () => data);

const nova = {
  id: "nova-id",
  name: "Nova",
  avatarAssetId: null,
  species: "Wolf",
  pronouns: "they/them",
  description: "A quiet stargazer.",
  notes: "Favorite season: winter.",
  tags: [{ id: "tag-main", name: "Main" }],
  colors: [
    { id: "color-one", hexValue: "#74518E", position: 0 },
    { id: "color-two", hexValue: "#E6A66E", position: 1 },
  ],
  createdAt: "2026-01-01T00:00:00Z",
  updatedAt: "2026-01-01T00:00:00Z",
};

const novaSummary = {
  id: nova.id,
  name: nova.name,
  avatarAssetId: null,
  species: nova.species,
  pronouns: nova.pronouns,
  tags: nova.tags,
  colors: nova.colors,
  updatedAt: nova.updatedAt,
};

const echoSummary = {
  ...novaSummary,
  id: "echo-id",
  name: "Echo",
  species: "Fox",
  tags: [{ id: "tag-forest", name: "Forest" }],
};

describe("App", () => {
  beforeEach(async () => {
    localStorage.clear();
    delete document.documentElement.dataset.theme;
    await i18n.changeLanguage("en");
    vi.restoreAllMocks();
    Object.values(data).forEach((mock) => mock.mockReset());
    data.listCharacters.mockResolvedValue([]);
  });

  it("loads the local library and shows the empty state", async () => {
    render(<App />);

    expect(await screen.findByRole("heading", { name: /start your first character record/i })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: /create first character/i })).toBeInTheDocument();
    expect(screen.getByText(/nothing leaves this device/i)).toBeInTheDocument();
    expect(document.querySelector(".furry-companion")).not.toBeInTheDocument();
    expect(document.querySelector(".glyph-ear")).not.toBeInTheDocument();
  });

  it("gives navigation controls localized accessible names", async () => {
    const user = userEvent.setup();
    render(<App />);

    await screen.findByRole("heading", { name: /start your first character record/i });
    expect(screen.getByRole("button", { name: "Characters" })).toHaveAttribute("aria-label", "Characters");
    expect(screen.getByRole("button", { name: "Settings" })).toHaveAttribute("aria-label", "Settings");

    await user.selectOptions(screen.getByRole("combobox", { name: "Language" }), "zh-CN");
    expect(screen.getByRole("button", { name: "我的角色" })).toHaveAttribute("aria-label", "我的角色");
    expect(screen.getByRole("button", { name: "设置" })).toHaveAttribute("aria-label", "设置");
  });

  it("creates a character and opens its detail view", async () => {
    const user = userEvent.setup();
    data.createCharacter.mockResolvedValue(nova);
    data.listCharacters.mockResolvedValueOnce([]).mockResolvedValueOnce([novaSummary]);
    render(<App />);

    await user.click(await screen.findByRole("button", { name: /create first character/i }));
    await user.type(screen.getByRole("textbox", { name: /^name/i }), "Nova");
    await user.type(screen.getByRole("textbox", { name: /species/i }), "Wolf");
    await user.type(screen.getByRole("textbox", { name: /tags/i }), "Main, night");
    await user.click(screen.getByRole("button", { name: /save character/i }));

    await screen.findByRole("heading", { name: "Nova" });
    expect(data.createCharacter).toHaveBeenCalledWith(expect.objectContaining({
      name: "Nova",
      species: "Wolf",
      tags: ["Main", "night"],
    }));
    expect(screen.getByText("A quiet stargazer.")).toBeInTheDocument();
  });

  it("updates the character record preview while editing", async () => {
    const user = userEvent.setup();
    render(<App />);

    await user.click(await screen.findByRole("button", { name: /create first character/i }));
    const dock = document.querySelector(".identity-dock");
    expect(dock).not.toBeNull();
    const preview = within(dock as HTMLElement);

    expect(preview.getAllByText("Untitled character")).toHaveLength(2);
    expect(preview.getAllByText("Species not set")).toHaveLength(2);
    expect(preview.getByText("Pronouns not set")).toBeInTheDocument();

    await user.type(screen.getByRole("textbox", { name: /^name/i }), "Nova");
    await user.type(screen.getByRole("textbox", { name: /species/i }), "Wolf");
    await user.type(screen.getByRole("textbox", { name: /pronouns/i }), "they/them");

    expect(preview.getAllByText("Nova").length).toBeGreaterThan(0);
    expect(preview.getAllByText("Wolf").length).toBeGreaterThan(0);
    expect(preview.getByText("they/them")).toBeInTheDocument();
    expect(document.querySelector(".furry-companion")).not.toBeInTheDocument();
    expect(document.querySelector(".glyph-ear")).not.toBeInTheDocument();
  });

  it("validates the required name before saving", async () => {
    const user = userEvent.setup();
    render(<App />);

    await user.click(await screen.findByRole("button", { name: /create first character/i }));
    await user.click(screen.getByRole("button", { name: /save character/i }));

    const name = screen.getByRole("textbox", { name: /^name/i });
    expect(name).toHaveFocus();
    expect(name).toHaveAttribute("aria-invalid", "true");
    expect(screen.getByText(/enter a character name/i)).toBeInTheDocument();
    expect(data.createCharacter).not.toHaveBeenCalled();
  });

  it("focuses search with the slash shortcut", async () => {
    const user = userEvent.setup();
    data.listCharacters.mockResolvedValue([novaSummary]);
    render(<App />);

    const search = await screen.findByRole("textbox", { name: /search characters/i });
    expect(search).not.toHaveFocus();

    await user.keyboard("/");
    expect(search).toHaveFocus();
  });

  it("searches by name, species, and tag", async () => {
    const user = userEvent.setup();
    data.listCharacters.mockResolvedValue([novaSummary, echoSummary]);
    render(<App />);

    const search = await screen.findByRole("textbox", { name: /search characters/i });
    expect(screen.getByRole("button", { name: /open nova/i })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: /open echo/i })).toBeInTheDocument();

    await user.type(search, "forest");
    expect(screen.queryByRole("button", { name: /open nova/i })).not.toBeInTheDocument();
    expect(screen.getByRole("button", { name: /open echo/i })).toBeInTheDocument();
  });

  it("cancels character deletion without leaving the detail view", async () => {
    const user = userEvent.setup();
    data.listCharacters.mockResolvedValue([novaSummary]);
    data.getCharacter.mockResolvedValue(nova);
    render(<App />);

    await user.click(await screen.findByRole("button", { name: /open nova/i }));
    await user.click(screen.getByRole("button", { name: /^delete$/i }));
    expect(screen.getByRole("dialog", { name: /delete nova/i })).toBeInTheDocument();

    await user.click(screen.getByRole("button", { name: /^cancel$/i }));
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
    expect(screen.getByRole("heading", { name: "Nova" })).toBeInTheDocument();
    expect(data.deleteCharacter).not.toHaveBeenCalled();
  });

  it("keeps the detail view open when Escape cancels deletion", async () => {
    const user = userEvent.setup();
    data.listCharacters.mockResolvedValue([novaSummary]);
    data.getCharacter.mockResolvedValue(nova);
    render(<App />);

    await user.click(await screen.findByRole("button", { name: /open nova/i }));
    await user.click(screen.getByRole("button", { name: /^delete$/i }));
    const dialog = screen.getByRole("dialog", { name: /delete nova/i });

    fireEvent(dialog, new Event("cancel", { cancelable: true }));

    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
    expect(screen.getByRole("heading", { name: "Nova" })).toBeInTheDocument();
    expect(data.deleteCharacter).not.toHaveBeenCalled();
  });

  it("shows deletion errors inside the confirmation dialog", async () => {
    const user = userEvent.setup();
    data.listCharacters.mockResolvedValue([novaSummary]);
    data.getCharacter.mockResolvedValue(nova);
    data.deleteCharacter.mockRejectedValue(new Error("The local record is locked."));
    render(<App />);

    await user.click(await screen.findByRole("button", { name: /open nova/i }));
    await user.click(screen.getByRole("button", { name: /^delete$/i }));
    await user.click(screen.getByRole("button", { name: /delete character/i }));

    expect(await screen.findByRole("alert")).toHaveTextContent("The local record is locked.");
    expect(screen.getByRole("dialog", { name: /delete nova/i })).toBeInTheDocument();
    expect(screen.getByRole("heading", { name: "Nova" })).toBeInTheDocument();
  });

  it("opens, edits, and deletes a character", async () => {
    const user = userEvent.setup();
    data.listCharacters.mockResolvedValue([novaSummary]);
    data.getCharacter.mockResolvedValue(nova);
    data.updateCharacter.mockResolvedValue({ ...nova, name: "Nova Prime" });
    data.deleteCharacter.mockResolvedValue(undefined);
    render(<App />);

    await user.click(await screen.findByRole("button", { name: /open nova/i }));
    await user.click(await screen.findByRole("button", { name: /edit record/i }));
    const name = screen.getByRole("textbox", { name: /^name/i });
    await user.clear(name);
    await user.type(name, "Nova Prime");
    await user.click(screen.getByRole("button", { name: /save character/i }));

    expect(await screen.findByRole("heading", { name: "Nova Prime" })).toBeInTheDocument();
    expect(data.updateCharacter).toHaveBeenCalledWith("nova-id", expect.objectContaining({ name: "Nova Prime" }));

    await user.click(screen.getByRole("button", { name: /^delete$/i }));
    expect(screen.getByRole("dialog", { name: /delete nova prime/i })).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: /delete character/i }));
    await waitFor(() => expect(data.deleteCharacter).toHaveBeenCalledWith("nova-id"));
    expect(await screen.findByRole("heading", { name: /my characters/i })).toBeInTheDocument();
  });

  it("switches between English and Simplified Chinese", async () => {
    const user = userEvent.setup();
    render(<App />);

    await screen.findByRole("heading", { name: /start your first character record/i });
    await user.selectOptions(screen.getByRole("combobox", { name: "Language" }), "zh-CN");

    expect(screen.getByRole("heading", { name: "建立第一份角色档案。" })).toBeInTheDocument();
    expect(localStorage.getItem("furfolio-language")).toBe("zh-CN");
  });

  it("follows the system theme until the user selects one", async () => {
    const user = userEvent.setup();
    render(<App />);
    await screen.findByRole("heading", { name: /start your first character record/i });

    expect(document.documentElement.dataset.theme).toBe("light");
    expect(localStorage.getItem("furfolio-theme")).toBeNull();

    act(() => setSystemTheme(true));
    expect(document.documentElement.dataset.theme).toBe("dark");

    await user.click(screen.getByRole("button", { name: /light theme/i }));
    expect(document.documentElement.dataset.theme).toBe("light");
    expect(localStorage.getItem("furfolio-theme")).toBe("light");

    act(() => setSystemTheme(true));
    expect(document.documentElement.dataset.theme).toBe("light");
  });
});
