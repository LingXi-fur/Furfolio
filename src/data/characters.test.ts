import { beforeEach, describe, expect, it, vi } from "vitest";

const { invoke } = vi.hoisted(() => ({ invoke: vi.fn() }));

vi.mock("@tauri-apps/api/core", () => ({ invoke }));

import {
  createCharacter,
  deleteCharacter,
  getCharacter,
  listCharacters,
  updateCharacter,
} from "./characters";

describe("character data commands", () => {
  beforeEach(() => {
    invoke.mockReset();
  });

  it("uses the stable Tauri command contracts", async () => {
    const input = { name: "Nova", tags: ["main"], colors: ["#123456"] };

    await createCharacter(input);
    await listCharacters();
    await getCharacter("character-id");
    await updateCharacter("character-id", input);
    await deleteCharacter("character-id");

    expect(invoke.mock.calls).toEqual([
      ["create_character", { input }],
      ["list_characters"],
      ["get_character", { id: "character-id" }],
      ["update_character", { id: "character-id", input }],
      ["delete_character", { id: "character-id" }],
    ]);
  });
});
