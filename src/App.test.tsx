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

const namesakeSummary = {
  ...novaSummary,
  id: "nova-namesake-id",
};

function deferred<T>() {
  let resolve!: (value: T) => void;
  let reject!: (reason?: unknown) => void;
  const promise = new Promise<T>((resolvePromise, rejectPromise) => {
    resolve = resolvePromise;
    reject = rejectPromise;
  });
  return { promise, resolve, reject };
}

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

  it("renders character covers as plates without images", async () => {
    data.listCharacters.mockResolvedValue([novaSummary]);
    render(<App />);

    await screen.findByRole("button", { name: /open nova/i });
    const cover = document.querySelector(".character-cover") as HTMLElement | null;
    expect(cover).not.toBeNull();
    expect(cover).toHaveAttribute("aria-hidden", "true");
    expect(cover?.style.getPropertyValue("--cover-primary")).toBe("#74518E");
    expect(cover?.style.getPropertyValue("--cover-secondary")).toBe("#E6A66E");
    expect(cover?.style.getPropertyValue("--cover-on-primary")).toBe("#F7F2E7");
    expect(document.querySelector(".cover-index")?.textContent).toBe("01");
    expect(document.querySelector("img")).not.toBeInTheDocument();
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

  it("returns focus to the create action when creation is canceled", async () => {
    const user = userEvent.setup();
    data.listCharacters.mockResolvedValue([novaSummary]);
    render(<App />);

    await user.click(await screen.findByRole("button", { name: /new character/i }));
    expect(screen.getByRole("textbox", { name: /^name/i })).toHaveFocus();

    await user.click(screen.getByRole("button", { name: /back/i }));

    expect(await screen.findByRole("button", { name: /new character/i })).toHaveFocus();
  });

  it("returns focus to the create action when creation is canceled from the empty state", async () => {
    const user = userEvent.setup();
    render(<App />);

    await user.click(await screen.findByRole("button", { name: /create first character/i }));
    await user.click(screen.getByRole("button", { name: /^cancel$/i }));

    expect(await screen.findByRole("button", { name: /create first character/i })).toHaveFocus();
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

  it("keeps the slash shortcut inside editing controls", async () => {
    const user = userEvent.setup();
    data.listCharacters.mockResolvedValue([novaSummary]);
    render(<App />);

    const search = await screen.findByRole("textbox", { name: /search characters/i });
    await user.click(search);
    await user.keyboard("/");

    expect(search).toHaveValue("/");
    expect(search).toHaveFocus();
  });

  it("returns focus to search after clearing the query", async () => {
    const user = userEvent.setup();
    data.listCharacters.mockResolvedValue([novaSummary, echoSummary]);
    render(<App />);

    const search = await screen.findByRole("textbox", { name: /search characters/i });
    await user.type(search, "forest");
    await user.click(screen.getByRole("button", { name: /clear search/i }));

    expect(search).toHaveValue("");
    expect(search).toHaveFocus();
    expect(screen.getByRole("button", { name: /open echo/i })).toBeInTheDocument();
  });

  it("preserves the filtered library context after returning from detail", async () => {
    const user = userEvent.setup();
    const echo = {
      ...nova,
      id: echoSummary.id,
      name: echoSummary.name,
      species: echoSummary.species,
      tags: echoSummary.tags,
    };
    data.listCharacters.mockResolvedValue([novaSummary, echoSummary]);
    data.getCharacter.mockResolvedValue(echo);
    render(<App />);

    const search = await screen.findByRole("textbox", { name: /search characters/i });
    await user.type(search, "forest");
    await user.click(screen.getByRole("button", { name: /open echo · featured plate/i }));
    await user.click(screen.getByRole("button", { name: /character index/i }));

    expect(await screen.findByRole("textbox", { name: /search characters/i })).toHaveValue("forest");
    expect(screen.getByText("1 record shown")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: /open echo · featured plate/i })).toHaveFocus();
    expect(screen.queryByRole("button", { name: /open nova/i })).not.toBeInTheDocument();
  });

  it("returns focus to search when an edited record leaves the active filter", async () => {
    const user = userEvent.setup();
    const echo = {
      ...nova,
      id: echoSummary.id,
      name: echoSummary.name,
      species: echoSummary.species,
      tags: echoSummary.tags,
    };
    const editedEcho = {
      ...echo,
      tags: [{ id: "tag-dawn", name: "Dawn" }],
    };
    const editedEchoSummary = {
      ...echoSummary,
      tags: editedEcho.tags,
    };
    data.listCharacters
      .mockResolvedValueOnce([novaSummary, echoSummary])
      .mockResolvedValue([novaSummary, editedEchoSummary]);
    data.getCharacter.mockResolvedValue(echo);
    data.updateCharacter.mockResolvedValue(editedEcho);
    render(<App />);

    const search = await screen.findByRole("textbox", { name: /search characters/i });
    await user.type(search, "forest");
    await user.click(screen.getByRole("button", { name: /open echo · featured plate/i }));
    await user.click(screen.getByRole("button", { name: /edit record/i }));
    const tags = screen.getByRole("textbox", { name: /tags/i });
    await user.clear(tags);
    await user.type(tags, "Dawn");
    await user.click(screen.getByRole("button", { name: /save character/i }));
    await user.click(await screen.findByRole("button", { name: /character index/i }));

    expect(await screen.findByRole("heading", { name: /no matching character/i })).toBeInTheDocument();
    expect(screen.getByRole("textbox", { name: /search characters/i })).toHaveValue("forest");
    expect(screen.getByRole("textbox", { name: /search characters/i })).toHaveFocus();
    expect(screen.getByText("0 records shown")).toBeInTheDocument();
  });

  it("searches by name, species, and tag", async () => {
    const user = userEvent.setup();
    data.listCharacters.mockResolvedValue([novaSummary, echoSummary]);
    render(<App />);

    const search = await screen.findByRole("textbox", { name: /search characters/i });
    const featured = screen.getByRole("button", { name: /open nova · featured plate/i });
    expect(featured).toHaveTextContent(/open nova/i);
    expect(featured).toHaveTextContent(/featured record/i);
    expect(screen.getByRole("heading", { name: /character index/i })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: /open echo/i })).toBeInTheDocument();

    const tools = screen.getByRole("region", { name: /character library tools/i });
    expect(within(tools).getByText("02")).toBeInTheDocument();
    expect(within(tools).getByText("2 records shown")).toBeInTheDocument();

    await user.type(search, "forest");
    expect(screen.queryByRole("button", { name: /open nova/i })).not.toBeInTheDocument();
    expect(screen.getByRole("button", { name: /open echo/i })).toBeInTheDocument();
    expect(within(tools).getByText("01")).toBeInTheDocument();
    expect(within(tools).getByText("1 record shown")).toBeInTheDocument();
  });

  it("distinguishes records that share the same name", async () => {
    data.listCharacters.mockResolvedValue([novaSummary, namesakeSummary]);
    render(<App />);

    expect(await screen.findByRole("button", { name: /open nova · featured plate/i })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: /open nova · record 02/i })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: /contents: nova · record 01/i })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: /contents: nova · record 02/i })).toBeInTheDocument();
  });

  it("shows the new library interface in Simplified Chinese", async () => {
    const user = userEvent.setup();
    data.listCharacters.mockResolvedValue([novaSummary, echoSummary]);
    render(<App />);

    await screen.findByRole("heading", { name: /my characters/i });
    await user.selectOptions(screen.getByRole("combobox", { name: "Language" }), "zh-CN");

    expect(screen.getByRole("heading", { name: "角色索引" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: /打开 Nova · 精选图版/ })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: /打开 Echo · 档案 02/ })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: /目录：Nova · 档案 01/ })).toBeInTheDocument();
  });

  it("shows hidden tag count on the featured record", async () => {
    data.listCharacters.mockResolvedValue([{
      ...novaSummary,
      tags: [
        { id: "tag-1", name: "Main" },
        { id: "tag-2", name: "Night" },
        { id: "tag-3", name: "Forest" },
        { id: "tag-4", name: "Calm" },
        { id: "tag-5", name: "Winter" },
        { id: "tag-6", name: "Magic" },
      ],
    }]);
    render(<App />);

    const featured = await screen.findByRole("button", { name: /open nova · featured plate/i });
    expect(within(featured).getByText("+2")).toBeInTheDocument();
  });

  it("keeps featured records usable when the revision date is invalid", async () => {
    data.listCharacters.mockResolvedValue([{ ...novaSummary, updatedAt: "not-a-date" }]);
    render(<App />);

    const featured = await screen.findByRole("button", { name: /open nova · featured plate/i });
    expect(featured).toHaveTextContent("PL. 01");
    expect(featured).toHaveTextContent("Last revised date unavailable");
  });
  it("keeps detail records usable when stored dates are invalid", async () => {
    const user = userEvent.setup();
    data.listCharacters.mockResolvedValue([{ ...novaSummary, updatedAt: "not-a-date" }]);
    data.getCharacter.mockResolvedValue({
      ...nova,
      createdAt: "also-not-a-date",
      updatedAt: "not-a-date",
    });
    render(<App />);

    await user.click(await screen.findByRole("button", { name: /open nova · featured plate/i }));

    expect(await screen.findByRole("heading", { name: "Nova" })).toBeInTheDocument();
    expect(screen.getByText("Entered date unavailable · Last revised date unavailable")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: /character index/i })).toHaveFocus();
  });


  it("keeps the library usable when a single record cannot be opened", async () => {
    const user = userEvent.setup();
    const novaRequest = deferred<typeof nova>();
    data.listCharacters.mockResolvedValue([novaSummary, echoSummary]);
    data.getCharacter.mockReturnValue(novaRequest.promise);
    render(<App />);

    const featured = await screen.findByRole("button", { name: /open nova · featured plate/i });
    await user.click(featured);
    expect(featured).toHaveAttribute("aria-disabled", "true");
    expect(featured).toHaveAttribute("aria-busy", "true");

    novaRequest.reject(new Error("This record is unavailable."));

    expect(await screen.findByRole("alert")).toHaveTextContent("This record is unavailable.");
    expect(screen.getByRole("region", { name: /character library tools/i })).toBeInTheDocument();
    expect(featured).toBeEnabled();
    expect(featured).not.toHaveAttribute("aria-disabled");
    expect(featured).not.toHaveAttribute("aria-busy");
    expect(featured).toHaveTextContent("Open Nova");
    expect(screen.queryByRole("heading", { name: /character index could not be opened/i })).not.toBeInTheDocument();
  });

  it("returns focus to search after dismissing a record error", async () => {
    const user = userEvent.setup();
    data.listCharacters.mockResolvedValue([novaSummary, echoSummary]);
    data.getCharacter.mockRejectedValue(new Error("This record is unavailable."));
    render(<App />);

    await user.click(await screen.findByRole("button", { name: /open nova · featured plate/i }));
    const dismiss = await screen.findByRole("button", { name: /dismiss message/i });
    await user.click(dismiss);

    expect(screen.queryByRole("alert")).not.toBeInTheDocument();
    await waitFor(() => {
      expect(screen.getByRole("textbox", { name: /search characters/i })).toHaveFocus();
    });
  });

  it("keeps global plate numbers when the library is filtered", async () => {
    const user = userEvent.setup();
    const mapleSummary = {
      ...novaSummary,
      id: "maple-id",
      name: "Maple",
      species: "Moth",
      tags: [{ id: "tag-maple-forest", name: "Forest" }],
    };
    data.listCharacters.mockResolvedValue([novaSummary, echoSummary, mapleSummary]);
    render(<App />);

    await user.type(await screen.findByRole("textbox", { name: /search characters/i }), "forest");

    const featured = screen.getByRole("button", { name: /open echo · featured plate/i });
    expect(featured).toHaveTextContent("PL. 02");
    expect(screen.getByRole("button", { name: /open maple · record 03/i })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: /contents: maple · record 03/i })).toBeInTheDocument();
  });

  it("shows the no-results state and restores the full list", async () => {
    const user = userEvent.setup();
    data.listCharacters.mockResolvedValue([novaSummary, echoSummary]);
    render(<App />);

    const search = await screen.findByRole("textbox", { name: /search characters/i });
    await user.type(search, "zzz");

    expect(screen.getByRole("heading", { name: /no matching character/i })).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: /featured plate/i })).not.toBeInTheDocument();
    expect(screen.queryByRole("heading", { name: /character index/i })).not.toBeInTheDocument();
    expect(screen.getByText("0 records shown")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: /clear search field/i })).toBeInTheDocument();

    await user.click(screen.getByRole("button", { name: /^clear search$/i }));

    expect(search).toHaveValue("");
    expect(search).toHaveFocus();
    expect(screen.getByRole("button", { name: /open nova · featured plate/i })).toBeInTheDocument();
  });

  it("recovers the library after a failed load", async () => {
    const user = userEvent.setup();
    data.listCharacters
      .mockRejectedValueOnce(new Error("Local store is locked."))
      .mockResolvedValueOnce([novaSummary]);
    render(<App />);

    expect(await screen.findByRole("heading", { name: /character index could not be opened/i })).toBeInTheDocument();
    expect(screen.getByText("Local store is locked.")).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: /try again/i }));

    expect(await screen.findByRole("button", { name: /open nova · featured plate/i })).toBeInTheDocument();
  });

  it("marks a pending character and prevents duplicate requests", async () => {
    const user = userEvent.setup();
    const novaRequest = deferred<typeof nova>();
    const echo = {
      ...nova,
      id: echoSummary.id,
      name: echoSummary.name,
      species: echoSummary.species,
      tags: echoSummary.tags,
    };
    const echoRequest = deferred<typeof echo>();
    data.listCharacters.mockResolvedValue([novaSummary, echoSummary]);
    data.getCharacter.mockImplementation((id: string) => (
      id === nova.id ? novaRequest.promise : echoRequest.promise
    ));
    render(<App />);

    const featured = await screen.findByRole("button", { name: /open nova · featured plate/i });
    await user.dblClick(featured);

    expect(data.getCharacter).toHaveBeenCalledTimes(1);
    expect(featured).toHaveAttribute("aria-disabled", "true");
    expect(featured).toHaveAttribute("aria-busy", "true");
    expect(featured).toHaveFocus();
    expect(featured).toHaveTextContent("Opening Nova");
    expect(screen.getByRole("button", { name: /contents: nova · record 01/i })).toHaveAttribute("aria-disabled", "true");

    const echoEntry = screen.getByRole("button", { name: /open echo · record 02/i });
    expect(echoEntry).toBeEnabled();
    await user.click(echoEntry);

    expect(data.getCharacter).toHaveBeenCalledTimes(2);
    expect(featured).toBeEnabled();
    expect(featured).not.toHaveAttribute("aria-disabled");
    expect(featured).not.toHaveAttribute("aria-busy");
    expect(echoEntry).toHaveAttribute("aria-disabled", "true");
    expect(echoEntry).toHaveAttribute("aria-busy", "true");
    expect(screen.getByRole("button", { name: /contents: echo · record 02/i })).toHaveAttribute("aria-disabled", "true");

    echoRequest.resolve(echo);
    expect(await screen.findByRole("heading", { name: "Echo" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: /character index/i })).toHaveFocus();

    await user.keyboard("{Escape}");
    expect(await screen.findByRole("button", { name: /open echo · record 02/i })).toHaveFocus();

    novaRequest.resolve(nova);
    await act(async () => { await novaRequest.promise; });
  });

  it("clears pending feedback when Escape cancels record navigation", async () => {
    const user = userEvent.setup();
    const echoRequest = deferred<typeof nova>();
    data.listCharacters.mockResolvedValue([novaSummary, echoSummary]);
    data.getCharacter.mockImplementation((id: string) => (
      id === nova.id ? Promise.resolve(nova) : echoRequest.promise
    ));
    render(<App />);

    await user.click(await screen.findByRole("button", { name: /open nova · featured plate/i }));
    const echoContents = screen.getByRole("button", { name: /contents: echo · record 02/i });
    await user.click(echoContents);
    expect(echoContents).toHaveAttribute("aria-disabled", "true");
    expect(echoContents).toHaveAttribute("aria-busy", "true");

    await user.keyboard("{Escape}");

    expect(await screen.findByRole("heading", { name: /my characters/i })).toBeInTheDocument();
    const echoEntry = screen.getByRole("button", { name: /open echo · record 02/i });
    expect(echoEntry).toBeEnabled();
    expect(echoEntry).not.toHaveAttribute("aria-busy");

    echoRequest.resolve({
      ...nova,
      id: echoSummary.id,
      name: echoSummary.name,
      species: echoSummary.species,
      tags: echoSummary.tags,
    });
    await act(async () => { await echoRequest.promise; });

    expect(screen.getByRole("heading", { name: /my characters/i })).toBeInTheDocument();
  });

  it("keeps the latest character when record requests finish out of order", async () => {
    const user = userEvent.setup();
    const novaRequest = deferred<typeof nova>();
    const echo = {
      ...nova,
      id: echoSummary.id,
      name: echoSummary.name,
      species: echoSummary.species,
      tags: echoSummary.tags,
    };
    const echoRequest = deferred<typeof echo>();
    data.listCharacters.mockResolvedValue([novaSummary, echoSummary]);
    data.getCharacter.mockImplementation((id: string) => (
      id === nova.id ? novaRequest.promise : echoRequest.promise
    ));
    render(<App />);

    await user.click(await screen.findByRole("button", { name: /open nova · featured plate/i }));
    await user.click(screen.getByRole("button", { name: /open echo · record 02/i }));

    echoRequest.resolve(echo);
    expect(await screen.findByRole("heading", { name: "Echo" })).toBeInTheDocument();

    novaRequest.resolve(nova);
    await act(async () => { await novaRequest.promise; });

    expect(screen.getByRole("heading", { name: "Echo" })).toBeInTheDocument();
    expect(screen.queryByRole("heading", { name: "Nova" })).not.toBeInTheDocument();
  });

  it("ignores errors from superseded record requests", async () => {
    const user = userEvent.setup();
    const novaRequest = deferred<typeof nova>();
    const echo = {
      ...nova,
      id: echoSummary.id,
      name: echoSummary.name,
      species: echoSummary.species,
      tags: echoSummary.tags,
    };
    data.listCharacters.mockResolvedValue([novaSummary, echoSummary]);
    data.getCharacter.mockImplementation((id: string) => (
      id === nova.id ? novaRequest.promise : Promise.resolve(echo)
    ));
    render(<App />);

    await user.click(await screen.findByRole("button", { name: /open nova · featured plate/i }));
    await user.click(screen.getByRole("button", { name: /open echo · record 02/i }));
    expect(await screen.findByRole("heading", { name: "Echo" })).toBeInTheDocument();

    novaRequest.reject(new Error("Nova is unavailable."));
    await act(async () => { await novaRequest.promise.catch(() => undefined); });

    expect(screen.getByRole("heading", { name: "Echo" })).toBeInTheDocument();
    expect(screen.queryByRole("alert")).not.toBeInTheDocument();
  });

  it("does not let a pending record request replace the create view", async () => {
    const user = userEvent.setup();
    const novaRequest = deferred<typeof nova>();
    data.listCharacters.mockResolvedValue([novaSummary]);
    data.getCharacter.mockReturnValue(novaRequest.promise);
    render(<App />);

    await user.click(await screen.findByRole("button", { name: /open nova · featured plate/i }));
    await user.click(screen.getByRole("button", { name: /new character/i }));
    expect(screen.getByRole("textbox", { name: /^name/i })).toBeInTheDocument();

    novaRequest.resolve(nova);
    await act(async () => { await novaRequest.promise; });

    expect(screen.getByRole("textbox", { name: /^name/i })).toBeInTheDocument();
    expect(screen.queryByRole("heading", { name: "Nova" })).not.toBeInTheDocument();
  });

  it("does not let a pending record request override deletion navigation", async () => {
    const user = userEvent.setup();
    const echoRequest = deferred<typeof nova>();
    data.listCharacters
      .mockResolvedValueOnce([novaSummary, echoSummary])
      .mockResolvedValueOnce([echoSummary]);
    data.getCharacter.mockImplementation((id: string) => (
      id === nova.id ? Promise.resolve(nova) : echoRequest.promise
    ));
    data.deleteCharacter.mockResolvedValue(undefined);
    render(<App />);

    await user.click(await screen.findByRole("button", { name: /open nova · featured plate/i }));
    await user.click(screen.getByRole("button", { name: /contents: echo · record 02/i }));
    await user.click(screen.getByRole("button", { name: /^delete$/i }));
    await user.click(screen.getByRole("button", { name: /delete character/i }));
    expect(await screen.findByRole("button", { name: /open echo · featured plate/i })).toBeInTheDocument();

    echoRequest.resolve({
      ...nova,
      id: echoSummary.id,
      name: echoSummary.name,
      species: echoSummary.species,
      tags: echoSummary.tags,
    });
    await act(async () => { await echoRequest.promise; });

    expect(screen.getByRole("button", { name: /open echo · featured plate/i })).toBeInTheDocument();
    expect(screen.queryByRole("heading", { name: "Echo" })).not.toBeInTheDocument();
  });

  it("does not reload the current character from the contents rail", async () => {
    const user = userEvent.setup();
    data.listCharacters.mockResolvedValue([novaSummary]);
    data.getCharacter.mockResolvedValue(nova);
    render(<App />);

    await user.click(await screen.findByRole("button", { name: /open nova · featured plate/i }));
    const currentEntry = screen.getByRole("button", { name: /contents: nova · record 01/i });

    expect(currentEntry).toHaveAttribute("aria-current", "true");
    expect(currentEntry).toBeDisabled();
    await user.click(currentEntry);

    expect(data.getCharacter).toHaveBeenCalledTimes(1);
    expect(screen.getByRole("heading", { name: "Nova" })).toBeInTheDocument();
  });

  it("shows record-loading errors without replacing an open detail view", async () => {
    const user = userEvent.setup();
    data.listCharacters.mockResolvedValue([novaSummary, echoSummary]);
    data.getCharacter
      .mockResolvedValueOnce(nova)
      .mockRejectedValueOnce(new Error("Echo is unavailable."));
    render(<App />);

    await user.click(await screen.findByRole("button", { name: /open nova · featured plate/i }));
    await user.click(screen.getByRole("button", { name: /contents: echo · record 02/i }));

    expect(await screen.findByRole("alert")).toHaveTextContent("Echo is unavailable.");
    expect(screen.getByRole("heading", { name: "Nova" })).toBeInTheDocument();
    expect(screen.queryByRole("heading", { name: /character index could not be opened/i })).not.toBeInTheDocument();
  });

  it("renders a phrasing-content cover inside the featured button", async () => {
    data.listCharacters.mockResolvedValue([novaSummary]);
    render(<App />);

    const featured = await screen.findByRole("button", { name: /open nova · featured plate/i });
    expect(featured.querySelector(":scope > .feature-cover-wrap > .character-cover")?.tagName).toBe("SPAN");
    expect(featured.querySelector("div")).not.toBeInTheDocument();
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
    await waitFor(() => expect(screen.getByRole("button", { name: /^delete$/i })).toHaveFocus());
    expect(data.deleteCharacter).not.toHaveBeenCalled();
  });

  it("restores the character entry after using the detail back action", async () => {
    const user = userEvent.setup();
    data.listCharacters.mockResolvedValue([novaSummary]);
    data.getCharacter.mockResolvedValue(nova);
    render(<App />);

    await user.click(await screen.findByRole("button", { name: /open nova/i }));
    await user.click(screen.getByRole("button", { name: /character index/i }));

    expect(await screen.findByRole("button", { name: /open nova · featured plate/i })).toHaveFocus();
  });

  it("restores the contents entry after opening a record from the rail", async () => {
    const user = userEvent.setup();
    data.listCharacters.mockResolvedValue([novaSummary]);
    data.getCharacter.mockResolvedValue(nova);
    render(<App />);

    const contentsEntry = await screen.findByRole("button", { name: /contents: nova · record 01/i });
    await user.click(contentsEntry);
    await user.click(screen.getByRole("button", { name: /character index/i }));

    expect(await screen.findByRole("button", { name: /contents: nova · record 01/i })).toHaveFocus();
  });

  it("falls back to the library entry when the contents rail becomes hidden", async () => {
    const user = userEvent.setup();
    data.listCharacters.mockResolvedValue([novaSummary]);
    data.getCharacter.mockResolvedValue(nova);
    render(<App />);

    await user.click(await screen.findByRole("button", { name: /contents: nova · record 01/i }));
    const contentsRail = document.querySelector<HTMLElement>(".rail-toc");
    expect(contentsRail).not.toBeNull();
    contentsRail!.style.display = "none";

    await user.click(screen.getByRole("button", { name: /character index/i }));

    expect(await screen.findByRole("button", { name: /open nova · featured plate/i })).toHaveFocus();
  });

  it("restores the latest contents entry after opening another detail record", async () => {
    const user = userEvent.setup();
    const echo = {
      ...nova,
      id: echoSummary.id,
      name: echoSummary.name,
      species: echoSummary.species,
      tags: echoSummary.tags,
    };
    data.listCharacters.mockResolvedValue([novaSummary, echoSummary]);
    data.getCharacter.mockImplementation((id: string) => (
      id === nova.id ? Promise.resolve(nova) : Promise.resolve(echo)
    ));
    render(<App />);

    await user.click(await screen.findByRole("button", { name: /open nova · featured plate/i }));
    await user.click(screen.getByRole("button", { name: /contents: echo · record 02/i }));
    await user.click(screen.getByRole("button", { name: /character index/i }));

    expect(await screen.findByRole("button", { name: /contents: echo · record 02/i })).toHaveFocus();
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
    await waitFor(() => expect(screen.getByRole("button", { name: /^delete$/i })).toHaveFocus());
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

  it("returns focus to the create action after deletion", async () => {
    const user = userEvent.setup();
    data.listCharacters
      .mockResolvedValueOnce([novaSummary])
      .mockResolvedValueOnce([]);
    data.getCharacter.mockResolvedValue(nova);
    data.deleteCharacter.mockResolvedValue(undefined);
    render(<App />);

    await user.click(await screen.findByRole("button", { name: /open nova/i }));
    await user.click(screen.getByRole("button", { name: /^delete$/i }));
    await user.click(screen.getByRole("button", { name: /delete character/i }));

    expect(await screen.findByRole("button", { name: /create first character/i })).toHaveFocus();
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

  it("keeps the current view when a superseded save resolves", async () => {
    const user = userEvent.setup();
    const saveRequest = deferred<typeof nova>();
    data.createCharacter.mockReturnValue(saveRequest.promise);
    data.listCharacters.mockResolvedValue([]);
    render(<App />);

    await user.click(await screen.findByRole("button", { name: /create first character/i }));
    await user.type(screen.getByRole("textbox", { name: /^name/i }), "Nova");
    await user.click(screen.getByRole("button", { name: /save character/i }));
    await user.keyboard("{Escape}");

    await user.click(await screen.findByRole("button", { name: /create first character/i }));
    const nextName = screen.getByRole("textbox", { name: /^name/i });
    await user.type(nextName, "Echo");

    saveRequest.resolve(nova);
    await act(async () => { await saveRequest.promise; });

    expect(screen.getByRole("textbox", { name: /^name/i })).toHaveValue("Echo");
    await user.keyboard("{Escape}");
    expect(await screen.findByRole("button", { name: /open nova · featured plate/i })).toBeInTheDocument();
    expect(data.listCharacters).toHaveBeenCalledTimes(1);
  });

  it("shows the library loading state while deletion reloads records", async () => {
    const user = userEvent.setup();
    const reloadRequest = deferred<typeof novaSummary[]>();
    data.listCharacters
      .mockResolvedValueOnce([novaSummary])
      .mockReturnValueOnce(reloadRequest.promise);
    data.getCharacter.mockResolvedValue(nova);
    data.deleteCharacter.mockResolvedValue(undefined);
    render(<App />);

    await user.click(await screen.findByRole("button", { name: /open nova/i }));
    await user.click(screen.getByRole("button", { name: /^delete$/i }));
    await user.click(screen.getByRole("button", { name: /delete character/i }));

    expect(await screen.findByText(/opening the character archive/i)).toBeInTheDocument();
    expect(document.querySelector(".app-content")?.innerHTML).not.toBe("");
    expect(document.querySelector(".state-screen")).toHaveFocus();

    reloadRequest.resolve([]);
    await waitFor(() => {
      expect(screen.getByRole("button", { name: /create first character/i })).toHaveFocus();
    });
  });

  it("does not turn a failed library load into an empty library through navigation", async () => {
    const user = userEvent.setup();
    data.listCharacters.mockRejectedValue(new Error("Local store is locked."));
    render(<App />);

    expect(await screen.findByRole("heading", { name: /character index could not be opened/i })).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "Characters" }));

    expect(screen.getByRole("heading", { name: /character index could not be opened/i })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: /try again/i })).toBeInTheDocument();
    expect(screen.queryByRole("heading", { name: /start your first character record/i })).not.toBeInTheDocument();
  });

  it("does not show a detail-operation error as a library-load failure", async () => {
    const user = userEvent.setup();
    data.listCharacters.mockResolvedValue([novaSummary]);
    data.getCharacter.mockResolvedValue(nova);
    data.deleteCharacter.mockRejectedValue(new Error("The local record is locked."));
    render(<App />);

    await user.click(await screen.findByRole("button", { name: /open nova/i }));
    await user.click(screen.getByRole("button", { name: /^delete$/i }));
    await user.click(screen.getByRole("button", { name: /delete character/i }));
    expect(await screen.findByRole("alert")).toHaveTextContent("The local record is locked.");
    await user.click(screen.getByRole("button", { name: /^cancel$/i }));
    await user.click(screen.getByRole("button", { name: "Characters" }));

    expect(await screen.findByRole("heading", { name: /my characters/i })).toBeInTheDocument();
    expect(screen.queryByRole("heading", { name: /character index could not be opened/i })).not.toBeInTheDocument();
  });

  it("clears a deletion error before editing the record", async () => {
    const user = userEvent.setup();
    data.listCharacters.mockResolvedValue([novaSummary]);
    data.getCharacter.mockResolvedValue(nova);
    data.deleteCharacter.mockRejectedValue(new Error("The local record is locked."));
    render(<App />);

    await user.click(await screen.findByRole("button", { name: /open nova/i }));
    await user.click(screen.getByRole("button", { name: /^delete$/i }));
    await user.click(screen.getByRole("button", { name: /delete character/i }));
    expect(await screen.findByRole("alert")).toHaveTextContent("The local record is locked.");
    await user.click(screen.getByRole("button", { name: /^cancel$/i }));
    await user.click(screen.getByRole("button", { name: /edit record/i }));

    expect(screen.getByRole("textbox", { name: /^name/i })).toBeInTheDocument();
    expect(screen.queryByRole("alert")).not.toBeInTheDocument();
  });

  it("connects the required-name error to the name field", async () => {
    const user = userEvent.setup();
    render(<App />);

    await user.click(await screen.findByRole("button", { name: /create first character/i }));
    const name = screen.getByRole("textbox", { name: /^name/i });
    await user.click(screen.getByRole("button", { name: /save character/i }));

    expect(name).toHaveAttribute("aria-invalid", "true");
    expect(name).toHaveAccessibleDescription("Enter a character name.");
    expect(name).toHaveFocus();
  });

  it("preserves stored tags containing commas when editing other fields", async () => {
    const user = userEvent.setup();
    const commaTagNova = {
      ...nova,
      tags: [{ id: "tag-comma", name: "sona, main" }],
    };
    data.listCharacters.mockResolvedValue([{
      ...novaSummary,
      tags: commaTagNova.tags,
    }]);
    data.getCharacter.mockResolvedValue(commaTagNova);
    data.updateCharacter.mockResolvedValue({ ...commaTagNova, species: "Arctic wolf" });
    render(<App />);

    await user.click(await screen.findByRole("button", { name: /open nova/i }));
    await user.click(screen.getByRole("button", { name: /edit record/i }));
    await user.clear(screen.getByRole("textbox", { name: /species/i }));
    await user.type(screen.getByRole("textbox", { name: /species/i }), "Arctic wolf");
    await user.click(screen.getByRole("button", { name: /save character/i }));

    expect(data.updateCharacter).toHaveBeenCalledWith("nova-id", expect.objectContaining({
      species: "Arctic wolf",
      tags: ["sona, main"],
    }));
  });

  it("caps record completion at one hundred percent", async () => {
    const user = userEvent.setup();
    const threeColorNova = {
      ...nova,
      colors: [
        ...nova.colors,
        { id: "color-three", hexValue: "#193B5C", position: 2 },
      ],
    };
    data.listCharacters.mockResolvedValue([novaSummary]);
    data.getCharacter.mockResolvedValue(threeColorNova);
    render(<App />);

    await user.click(await screen.findByRole("button", { name: /open nova/i }));
    await user.click(screen.getByRole("button", { name: /edit record/i }));

    expect(document.querySelector(".completion-block b")).toHaveTextContent("100%");
    expect(document.querySelectorAll(".completion-track .filled")).toHaveLength(4);
  });

  it("handles rejected superseded view transitions", async () => {
    const user = userEvent.setup();
    let rejectPrevious: ((reason?: unknown) => void) | null = null;
    const startViewTransition = vi.fn((update: () => void) => {
      rejectPrevious?.(new DOMException("Transition was skipped", "InvalidStateError"));
      let reject!: (reason?: unknown) => void;
      const finished = new Promise<void>((_resolve, rejectPromise) => {
        reject = rejectPromise;
      });
      rejectPrevious = reject;
      update();
      return { finished };
    });
    Object.defineProperty(document, "startViewTransition", {
      configurable: true,
      value: startViewTransition,
    });
    data.listCharacters.mockResolvedValue([novaSummary]);
    data.getCharacter.mockResolvedValue(nova);
    render(<App />);

    try {
      await user.click(await screen.findByRole("button", { name: /open nova/i }));
      await user.click(screen.getByRole("button", { name: /edit record/i }));
      await user.click(screen.getByRole("button", { name: "Characters" }));

      expect(await screen.findByRole("heading", { name: /my characters/i })).toBeInTheDocument();
      expect(startViewTransition).toHaveBeenCalled();
    } finally {
      Reflect.deleteProperty(document, "startViewTransition");
    }
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
