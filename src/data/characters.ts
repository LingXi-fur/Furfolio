import { invoke } from "@tauri-apps/api/core";

export interface Tag {
  id: string;
  name: string;
}

export interface CharacterColor {
  id: string;
  hexValue: string;
  position: number;
}

export interface CharacterSummary {
  id: string;
  name: string;
  species: string | null;
  pronouns: string | null;
  avatarAssetId: string | null;
  tags: Tag[];
  colors: CharacterColor[];
  updatedAt: string;
}

export interface Character {
  id: string;
  name: string;
  avatarAssetId: string | null;
  species: string | null;
  pronouns: string | null;
  description: string | null;
  notes: string | null;
  tags: Tag[];
  colors: CharacterColor[];
  createdAt: string;
  updatedAt: string;
}

export interface CharacterInput {
  name: string;
  species?: string | null;
  pronouns?: string | null;
  description?: string | null;
  notes?: string | null;
  tags?: string[];
  colors?: string[];
}

export interface CommandError {
  code: "invalidInput" | "notFound" | "unsupportedDatabase" | "storageError";
  message: string;
}

export function createCharacter(input: CharacterInput) {
  return invoke<Character>("create_character", { input });
}

export function listCharacters() {
  return invoke<CharacterSummary[]>("list_characters");
}

export function getCharacter(id: string) {
  return invoke<Character>("get_character", { id });
}

export function updateCharacter(id: string, input: CharacterInput) {
  return invoke<Character>("update_character", { id, input });
}

export function deleteCharacter(id: string) {
  return invoke<void>("delete_character", { id });
}
