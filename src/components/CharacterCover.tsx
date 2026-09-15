import { type CSSProperties } from "react";
import type { CharacterColor } from "../data/characters";
import { coverPalette } from "./characterPalette";

interface CharacterCoverProps {
  name: string;
  colors: CharacterColor[] | string[];
  species?: string | null;
  index?: number;
  registerLabel?: string;
  compact?: boolean;
}

function getInitial(name: string) {
  return Array.from(name.trim())[0]?.toLocaleUpperCase() ?? "?";
}

export function CharacterCover({ name, colors, species, index, registerLabel, compact = false }: CharacterCoverProps) {
  const palette = coverPalette(colors);
  const recordNumber = index === undefined ? null : String(index).padStart(2, "0");
  const style = {
    "--cover-primary": palette.primary,
    "--cover-secondary": palette.secondary,
    "--cover-on-primary": palette.onPrimary,
  } as CSSProperties;

  return (
    <div className={`character-cover${compact ? " compact" : ""}`} style={style} aria-hidden="true">
      <span className="cover-register">{registerLabel ?? (recordNumber ? `PLATE / FF-${recordNumber}` : "FURFOLIO / PLATE")}</span>
      <span className="cover-index">{recordNumber ?? "OC"}</span>
      <span className="cover-band cover-band-primary" />
      <span className="cover-band cover-band-secondary" />
      <span className="cover-initial">{getInitial(name)}</span>
      {!compact && (
        <span className="cover-crops">
          <i className="cover-crop cover-crop-top-left" />
          <i className="cover-crop cover-crop-top-right" />
          <i className="cover-crop cover-crop-bottom-left" />
          <i className="cover-crop cover-crop-bottom-right" />
        </span>
      )}
      <span className="cover-rule" />
      <span className="cover-caption">
        <small>{species}</small>
        <strong>{name}</strong>
        <span className="cover-swatches"><i /><i /></span>
      </span>
    </div>
  );
}
