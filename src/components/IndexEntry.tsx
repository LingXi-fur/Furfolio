import { type CSSProperties } from "react";
import { useTranslation } from "react-i18next";
import type { CharacterSummary } from "../data/characters";
import { DEFAULT_PALETTE, coverPalette } from "./characterPalette";

interface IndexEntryProps {
  character: CharacterSummary;
  index: number;
  order: number;
  opening: boolean;
  onOpen: (id: string) => void;
}

export function IndexEntry({ character, index, order, opening, onOpen }: IndexEntryProps) {
  const { t } = useTranslation();
  const palette = coverPalette(character.colors.length > 0 ? character.colors : [...DEFAULT_PALETTE]);
  const style = {
    "--entry-primary": palette.primary,
    "--entry-secondary": palette.secondary,
    "--entry-index": order,
  } as CSSProperties;

  return (
    <button
      className="index-entry"
      style={style}
      type="button"
      onClick={() => onOpen(character.id)}
      aria-label={`${t("library.openCharacter", { name: character.name })} · ${t("library.recordNumber", { number: String(index).padStart(2, "0") })}`}
      aria-busy={opening || undefined}
      aria-disabled={opening || undefined}
      data-character-id={character.id}
    >
      <span className="index-number">
        <small>{t("library.recordShort")}</small>
        <strong>{String(index).padStart(2, "0")}</strong>
      </span>

      <span className="index-identity">
        <strong>{character.name}</strong>
        <span>
          {character.species ?? t("character.unspecifiedSpecies")}
          {character.pronouns && <i>{character.pronouns}</i>}
        </span>
      </span>

      <span className="index-tags">
        {character.tags.slice(0, 2).map((tag) => (
          <span className="tag" key={tag.id}>{tag.name}</span>
        ))}
        {character.tags.length > 2 && <span className="tag">+{character.tags.length - 2}</span>}
        {character.tags.length === 0 && <span className="index-empty">{t("detail.noTags")}</span>}
      </span>

      <span className="index-palette" aria-hidden="true">
        <i style={{ background: palette.primary }} />
        <i style={{ background: palette.secondary }} />
      </span>

      <span className="index-open" aria-hidden="true">{opening ? "…" : "↗"}</span>
    </button>
  );
}
