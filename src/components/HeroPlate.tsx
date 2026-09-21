import { type CSSProperties } from "react";
import { useTranslation } from "react-i18next";
import type { CharacterSummary } from "../data/characters";
import { CharacterCover } from "./CharacterCover";
import { DEFAULT_PALETTE, coverPalette } from "./characterPalette";

interface HeroPlateProps {
  character: CharacterSummary;
  index: number;
  onOpen: (id: string) => void;
}

export function HeroPlate({ character, index, onOpen }: HeroPlateProps) {
  const { t, i18n } = useTranslation();
  const palette = coverPalette(character.colors.length > 0 ? character.colors : [...DEFAULT_PALETTE]);
  const style = {
    "--card-primary": palette.primary,
    "--card-secondary": palette.secondary,
  } as CSSProperties;
  const dateFormatter = new Intl.DateTimeFormat(i18n.language, { year: "numeric", month: "short", day: "numeric" });

  return (
    <button
      className="hero-plate"
      style={style}
      type="button"
      onClick={() => onOpen(character.id)}
      aria-label={t("library.heroOpen", { name: character.name })}
    >
      <span className="hero-ghost" aria-hidden="true">{character.name}</span>
      <span className="hero-figure" aria-hidden="true">{String(index).padStart(2, "0")}</span>

      <span className="hero-info">
        <span className="hero-register">
          <span>{t("library.plateShort", { number: index })}</span>
          {character.species ?? t("character.unspecifiedSpecies")}
        </span>
        <strong className="hero-name">{character.name}</strong>
        {character.pronouns && <span className="hero-pronouns">{character.pronouns}</span>}
        <span className="tag-row hero-tags">
          {character.tags.slice(0, 4).map((tag) => <span className="tag" key={tag.id}>{tag.name}</span>)}
          {character.tags.length === 0 && <span className="hero-empty">{t("detail.noTags")}</span>}
        </span>
        <span className="palette-row hero-palette">
          {character.colors.map((color) => <span className="color-chip" key={color.id}><i style={{ background: color.hexValue }} />{color.hexValue}</span>)}
        </span>
        <span className="hero-meta">{t("library.heroMeta", { date: dateFormatter.format(new Date(character.updatedAt)) })}</span>
        <span className="hero-open">{t("library.openPlate")} <b aria-hidden="true">→</b></span>
      </span>

      <span className="hero-cover">
        <CharacterCover name={character.name} species={character.species} colors={character.colors} index={index} />
      </span>
    </button>
  );
}
