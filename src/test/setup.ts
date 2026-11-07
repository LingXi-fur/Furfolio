import "@testing-library/jest-dom/vitest";
import { cleanup } from "@testing-library/react";
import { afterEach } from "vitest";

type MediaListener = (event: MediaQueryListEvent) => void;

const mediaState = {
  systemThemeIsDark: false,
  reducedMotion: false,
  coarsePointer: false,
  viewportWidth: 1280,
};
const mediaListeners = new Map<string, Set<MediaListener>>();

function queryMatches(query: string) {
  return query.split(",").some((alternative) => {
    const conditions = alternative.match(/\([^)]*\)/g);
    if (!conditions?.length) return false;

    return conditions.every((condition) => {
      const normalized = condition.toLocaleLowerCase().replace(/\s+/g, " ");
      if (normalized === "(prefers-color-scheme: dark)") return mediaState.systemThemeIsDark;
      if (normalized === "(prefers-color-scheme: light)") return !mediaState.systemThemeIsDark;
      if (normalized === "(prefers-reduced-motion: reduce)") return mediaState.reducedMotion;
      if (normalized === "(prefers-reduced-motion: no-preference)") return !mediaState.reducedMotion;
      if (normalized === "(pointer: coarse)" || normalized === "(hover: none)") return mediaState.coarsePointer;
      if (normalized === "(pointer: fine)" || normalized === "(hover: hover)") return !mediaState.coarsePointer;

      const width = normalized.match(/^\((min|max)-width: (\d+)px\)$/);
      if (!width) return false;
      const threshold = Number(width[2]);
      return width[1] === "min"
        ? mediaState.viewportWidth >= threshold
        : mediaState.viewportWidth <= threshold;
    });
  });
}

function updateMediaState(update: () => void) {
  const previousMatches = new Map(
    [...mediaListeners.keys()].map((query) => [query, queryMatches(query)])
  );
  update();

  mediaListeners.forEach((listeners, query) => {
    const matches = queryMatches(query);
    if (previousMatches.get(query) === matches) return;
    const event = { matches, media: query } as MediaQueryListEvent;
    listeners.forEach((listener) => listener(event));
  });
}

Object.defineProperty(window, "matchMedia", {
  writable: true,
  value: (query: string) => {
    const listeners = mediaListeners.get(query) ?? new Set<MediaListener>();
    mediaListeners.set(query, listeners);

    return {
      get matches() {
        return queryMatches(query);
      },
      media: query,
      onchange: null,
      addListener: (listener: MediaListener) => listeners.add(listener),
      removeListener: (listener: MediaListener) => listeners.delete(listener),
      addEventListener: (type: string, listener: MediaListener) => {
        if (type === "change") listeners.add(listener);
      },
      removeEventListener: (type: string, listener: MediaListener) => {
        if (type === "change") listeners.delete(listener);
      },
      dispatchEvent: (event: Event) => {
        listeners.forEach((listener) => listener(event as MediaQueryListEvent));
        return true;
      },
    };
  },
});

export function setSystemTheme(isDark: boolean) {
  updateMediaState(() => {
    mediaState.systemThemeIsDark = isDark;
  });
}

export function setReducedMotion(reduced: boolean) {
  updateMediaState(() => {
    mediaState.reducedMotion = reduced;
  });
}

export function setCoarsePointer(coarse: boolean) {
  updateMediaState(() => {
    mediaState.coarsePointer = coarse;
  });
}

export function setViewportWidth(width: number) {
  updateMediaState(() => {
    mediaState.viewportWidth = width;
  });
}

afterEach(() => {
  cleanup();
  mediaListeners.clear();
  mediaState.systemThemeIsDark = false;
  mediaState.reducedMotion = false;
  mediaState.coarsePointer = false;
  mediaState.viewportWidth = 1280;
});
