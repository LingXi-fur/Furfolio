import { describe, expect, it } from "vitest";
import type { CharacterColor } from "../data/characters";
import { DEFAULT_PALETTE, coverPalette } from "./characterPalette";

const DARK_INK = "#26211B";
const LIGHT_INK = "#F7F2E7";

function relativeLuminance(hex: string) {
  const [red, green, blue] = [1, 3, 5]
    .map((start) => Number.parseInt(hex.slice(start, start + 2), 16) / 255)
    .map((channel) => (channel <= 0.04045 ? channel / 12.92 : ((channel + 0.055) / 1.055) ** 2.4));
  return 0.2126 * red + 0.7152 * green + 0.0722 * blue;
}

function contrastRatio(first: string, second: string) {
  const light = Math.max(relativeLuminance(first), relativeLuminance(second));
  const dark = Math.min(relativeLuminance(first), relativeLuminance(second));
  return (light + 0.05) / (dark + 0.05);
}

const objectColors: CharacterColor[] = [
  { id: "color-one", hexValue: "#74518E", position: 0 },
  { id: "color-two", hexValue: "#E6A66E", position: 1 },
];

describe("coverPalette", () => {
  it("falls back to the default palette for missing or invalid colors", () => {
    const empty = coverPalette([]);
    expect(empty.primary).toBe(DEFAULT_PALETTE[0]);
    expect(empty.secondary).toBe(DEFAULT_PALETTE[1]);

    const invalid = coverPalette(["nope", "#XYZ123"]);
    expect(invalid.primary).toBe(DEFAULT_PALETTE[0]);
    expect(invalid.secondary).toBe(DEFAULT_PALETTE[1]);
  });

  it("normalizes valid hex values to uppercase", () => {
    const palette = coverPalette(["#74518e", "#e6a66e"]);
    expect(palette.primary).toBe("#74518E");
    expect(palette.secondary).toBe("#E6A66E");
  });

  it("accepts CharacterColor objects and string values alike", () => {
    expect(coverPalette(objectColors)).toEqual(coverPalette(["#74518E", "#E6A66E"]));
  });

  it("picks the light ink for dark primaries and the dark ink for light primaries", () => {
    expect(coverPalette(["#111111", "#F5F1E6"]).onPrimary).toBe(LIGHT_INK);
    expect(coverPalette(["#F5F1E6", "#111111"]).onPrimary).toBe(DARK_INK);
  });

  it("keeps the ink on the primary band at WCAG AA contrast", () => {
    for (const colors of [["#111111", "#F5F1E6"], ["#F5F1E6", "#111111"], [...DEFAULT_PALETTE], objectColors] as const) {
      const { primary, onPrimary } = coverPalette([...colors] as string[]);
      expect(contrastRatio(primary, onPrimary)).toBeGreaterThanOrEqual(4.5);
    }
  });
});
