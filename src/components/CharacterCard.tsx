import { type CSSProperties } from "react";
import { useTranslation } from "react-i18next";
import type { CharacterSummary } from "../data/characters";
import { CharacterCover } from "./CharacterCover";
import { DEFAULT_PALETTE, coverPalette } from "./characterPalette";

interface CharacterCardProps {
  character: CharacterSummary;
  index: number;
  onOpen: (id: string) => void;
}

export function CharacterCard({ character, index, onOpen }: CharacterCardProps) {
  const { t } = useTranslation();
  const palette = coverPalette(character.colors.length > 0 ? character.colors : [...DEFAULT_PALETTE]);
  const style = {
    "--card-primary": palette.primary,
    "--card-secondary": palette.secondary,
  } as CSSProperties;

  return (
    <button
      className="character-card"
      style={style}
      type="button"
      onClick={() => onOpen(character.id)}
      aria-label={t("library.openCharacter", { name: character.name })}
    >
      <CharacterCover
        compact
        name={character.name}
        species={character.species}
        colors={character.colors}
        index={index}
        registerLabel={t("library.characterRecord")}
      />

      <span className="card-content-layer">
        <span className="card-kicker">
          {character.species ?? t("character.unspecifiedSpecies")}
        </span>
        <strong>{character.name}</strong>
        {character.pronouns && (
          <span className="card-pronouns">{character.pronouns}</span>
        )}
        <span className="tag-row">
          {character.tags.slice(0, 2).map((tag) => (
            <span className="tag" key={tag.id}>{tag.name}</span>
          ))}
          {character.tags.length > 2 && (
            <span className="tag">+{character.tags.length - 2}</span>
          )}
        </span>
      </span>

      <span className="card-open" aria-hidden="true">↗</span>
    </button>
  );
}
