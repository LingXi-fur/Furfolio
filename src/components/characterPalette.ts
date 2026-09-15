import type { CharacterColor } from "../data/characters";

export const DEFAULT_PALETTE = ["#8A5A3C", "#C9A24B"] as const;

const HEX_COLOR = /^#[0-9a-f]{6}$/i;
const DARK_INK = "#26211B";
const LIGHT_INK = "#F7F2E7";

function colorValue(color: CharacterColor | string | undefined) {
  return typeof color === "string" ? color : color?.hexValue;
}

function validColor(value: string | undefined, fallback: string) {
  return value && HEX_COLOR.test(value) ? value.toUpperCase() : fallback;
}

function relativeLuminance(hex: string) {
  const channels = [1, 3, 5].map((start) => Number.parseInt(hex.slice(start, start + 2), 16) / 255);
  const [red, green, blue] = channels.map((channel) => channel <= 0.04045 ? channel / 12.92 : ((channel + 0.055) / 1.055) ** 2.4);
  return 0.2126 * red + 0.7152 * green + 0.0722 * blue;
}

function contrastRatio(first: string, second: string) {
  const light = Math.max(relativeLuminance(first), relativeLuminance(second));
  const dark = Math.min(relativeLuminance(first), relativeLuminance(second));
  return (light + 0.05) / (dark + 0.05);
}

export function coverPalette(colors: CharacterColor[] | string[]) {
  const primary = validColor(colorValue(colors[0]), DEFAULT_PALETTE[0]);
  const secondary = validColor(colorValue(colors[1]), DEFAULT_PALETTE[1]);
  const onPrimary = contrastRatio(primary, DARK_INK) >= contrastRatio(primary, LIGHT_INK) ? DARK_INK : LIGHT_INK;
  return { primary, secondary, onPrimary };
}
