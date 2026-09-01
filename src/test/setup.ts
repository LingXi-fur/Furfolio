import "@testing-library/jest-dom/vitest";
import { cleanup } from "@testing-library/react";
import { afterEach } from "vitest";

let systemThemeIsDark = false;
const colorSchemeListeners = new Set<(event: MediaQueryListEvent) => void>();

Object.defineProperty(window, "matchMedia", {
  writable: true,
  value: (query: string) => ({
    matches: systemThemeIsDark,
    media: query,
    onchange: null,
    addListener: () => undefined,
    removeListener: () => undefined,
    addEventListener: (type: string, listener: EventListener) => {
      if (type === "change") {
        colorSchemeListeners.add(listener as (event: MediaQueryListEvent) => void);
      }
    },
    removeEventListener: (type: string, listener: EventListener) => {
      if (type === "change") {
        colorSchemeListeners.delete(listener as (event: MediaQueryListEvent) => void);
      }
    },
    dispatchEvent: () => false,
  }),
});

export function setSystemTheme(isDark: boolean) {
  systemThemeIsDark = isDark;
  const event = { matches: isDark } as MediaQueryListEvent;
  colorSchemeListeners.forEach((listener) => listener(event));
}

afterEach(() => {
  cleanup();
  colorSchemeListeners.clear();
  systemThemeIsDark = false;
});
