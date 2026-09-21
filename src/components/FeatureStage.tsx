import { type CSSProperties } from "react";
import { useTranslation } from "react-i18next";
import type { CharacterSummary } from "../data/characters";
import { CharacterCover } from "./CharacterCover";
import { DEFAULT_PALETTE, coverPalette } from "./characterPalette";

interface FeatureStageProps {
  character: CharacterSummary;
  index: number;
  opening: boolean;
  onOpen: (id: string) => void;
}

export function FeatureStage({ character, index, opening, onOpen }: FeatureStageProps) {
  const { t, i18n } = useTranslation();
  const palette = coverPalette(character.colors.length > 0 ? character.colors : [...DEFAULT_PALETTE]);
  const style = {
    "--feature-primary": palette.primary,
    "--feature-secondary": palette.secondary,
    "--feature-on-primary": palette.onPrimary,
  } as CSSProperties;
  const updatedAt = new Date(character.updatedAt);
  const revisedDate = Number.isNaN(updatedAt.getTime())
    ? t("library.unknownRevision")
    : new Intl.DateTimeFormat(i18n.language, {
        year: "numeric",
        month: "short",
        day: "numeric",
      }).format(updatedAt);
  const plateNumber = String(index).padStart(2, "0");
  const compactName = character.name.length > 32;

  return (
    <button
      className={`feature-stage${compactName ? " feature-stage-compact-name" : ""}`}
      style={style}
      type="button"
      onClick={() => onOpen(character.id)}
      aria-label={t("library.heroOpen", { name: character.name })}
      aria-busy={opening || undefined}
      aria-disabled={opening || undefined}
      data-character-id={character.id}
    >
      <span className="feature-backdrop-name" aria-hidden="true">{character.name}</span>
      <span className="feature-sequence" aria-hidden="true">{plateNumber}</span>

      <span className="feature-copy">
        <span className="feature-register">
          <span>{t("library.featuredRecord")}</span>
          <i aria-hidden="true" />
          <span>{t("library.plateShort", { number: plateNumber })}</span>
        </span>

        <span className="feature-heading">
          <small>{character.species ?? t("character.unspecifiedSpecies")}</small>
          <strong>{character.name}</strong>
          {character.pronouns && <em>{character.pronouns}</em>}
        </span>

        <span className="feature-tags">
          {character.tags.slice(0, 4).map((tag) => (
            <span className="tag" key={tag.id}>{tag.name}</span>
          ))}
          {character.tags.length > 4 && (
            <span className="tag">+{character.tags.length - 4}</span>
          )}
          {character.tags.length === 0 && (
            <span className="feature-empty">{t("detail.noTags")}</span>
          )}
        </span>

        <span className="feature-footer">
          <span>
            <small>{t("library.heroMeta", { date: revisedDate })}</small>
            <span className="feature-swatches" aria-hidden="true">
              <i style={{ background: palette.primary }} />
              <i style={{ background: palette.secondary }} />
            </span>
          </span>
          <span className="feature-open">
            <span>
              {opening
                ? t("library.openingPlate", { name: character.name })
                : t("library.openPlate", { name: character.name })}
            </span>
            <b aria-hidden="true">{opening ? "…" : "↗"}</b>
          </span>
        </span>
      </span>

      <span className="feature-cover-wrap">
        <CharacterCover
          as="span"
          name={character.name}
          species={character.species}
          colors={character.colors}
          index={index}
        />
      </span>
    </button>
  );
}
