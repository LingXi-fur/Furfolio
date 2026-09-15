import { useEffect, useMemo, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import type { CharacterSummary } from "../data/characters";
import { CharacterCard } from "./CharacterCard";

interface LibraryViewProps {
  characters: CharacterSummary[];
  loading: boolean;
  error: string | null;
  onCreate: () => void;
  onOpen: (id: string) => void;
  onRetry: () => void;
}

export function LibraryView({ characters, loading, error, onCreate, onOpen, onRetry }: LibraryViewProps) {
  const { t } = useTranslation();
  const [query, setQuery] = useState("");
  const searchRef = useRef<HTMLInputElement>(null);
  const normalizedQuery = query.trim().toLocaleLowerCase();
  const filtered = useMemo(() => characters.filter((character) => {
    if (!normalizedQuery) return true;
    return [character.name, character.species ?? "", ...character.tags.map((tag) => tag.name)]
      .some((value) => value.toLocaleLowerCase().includes(normalizedQuery));
  }), [characters, normalizedQuery]);

  useEffect(() => {
    function focusSearch(event: KeyboardEvent) {
      const target = event.target as HTMLElement | null;
      const editing = target?.matches("input, textarea, select, [contenteditable='true']");
      if (event.key === "/" && !editing) {
        event.preventDefault();
        searchRef.current?.focus();
      }
    }
    window.addEventListener("keydown", focusSearch);
    return () => window.removeEventListener("keydown", focusSearch);
  }, []);

  if (loading) {
    return <main className="state-screen view-enter"><span className="loading-line" /><p>{t("library.loading")}</p></main>;
  }

  if (error) {
    return (
      <main className="state-screen view-enter">
        <span className="state-icon">!</span>
        <h1>{t("errors.libraryTitle")}</h1>
        <p>{error}</p>
        <button className="primary-button" type="button" onClick={onRetry}>{t("actions.retry")}</button>
      </main>
    );
  }

  if (characters.length === 0) {
    return (
      <main className="empty-library view-enter">
        <section className="archive-board" aria-hidden="true">
          <span className="board-register">FURFOLIO / 001</span>
          <div className="board-sheet board-sheet-back" />
          <div className="board-sheet board-sheet-main">
            <span className="board-tab">OC</span>
            <span className="board-frame"><b>?</b></span>
            <span className="board-line board-line-one" />
            <span className="board-line board-line-two" />
            <span className="board-line board-line-three" />
            <span className="board-swatch board-swatch-one" />
            <span className="board-swatch board-swatch-two" />
            <span className="board-code">LOCAL / PRIVATE</span>
          </div>
          <span className="board-note">{t("empty.boardNote")}</span>
        </section>
        <section className="empty-copy">
          <span className="welcome-chip">{t("empty.welcome")}</span>
          <h1>{t("empty.title")}</h1>
          <p className="intro">{t("empty.description")}</p>
          <div className="actions">
            <button className="primary-button" type="button" onClick={onCreate}>
              {t("empty.create")} <span aria-hidden="true">→</span>
            </button>
            <button className="secondary-button" type="button" disabled title={t("comingSoon")}>{t("empty.restore")}</button>
          </div>
          <aside className="privacy-note">
            <span className="privacy-icon" aria-hidden="true">⌂</span>
            <span><strong>{t("privacy.title")}</strong><small>{t("privacy.description")}</small></span>
          </aside>
        </section>
      </main>
    );
  }

  return (
    <main className="library-view view-enter">
      <header className="library-header">
        <div>
          <p className="eyebrow">{t("library.eyebrow")}</p>
          <h1>{t("library.title")}</h1>
        </div>
        <div className="library-count"><strong>{String(characters.length).padStart(2, "0")}</strong><span>{t("library.summary", { count: characters.length })}</span></div>
      </header>

      <section className="library-toolbar" aria-label={t("library.tools")}>
        <label className="search-field">
          <span aria-hidden="true">⌕</span>
          <span className="sr-only">{t("library.searchLabel")}</span>
          <input ref={searchRef} value={query} onChange={(event) => setQuery(event.target.value)} placeholder={t("library.searchPlaceholder")} />
          {query ? <button type="button" onClick={() => setQuery("")} aria-label={t("library.clearSearch")}>×</button> : <kbd>/</kbd>}
        </label>
        <span className="result-count">{t("library.resultCount", { count: filtered.length })}</span>
        <button className="primary-button" type="button" onClick={onCreate}>{t("library.addCharacter")} <span aria-hidden="true">＋</span></button>
      </section>

      {filtered.length > 0 ? (
        <section className="character-grid" aria-label={t("library.characters")}>
          {filtered.map((character, index) => (
            <div className="card-entry" style={{ "--entry-index": index } as React.CSSProperties} key={character.id}>
              <CharacterCard character={character} index={index + 1} onOpen={onOpen} />
            </div>
          ))}
        </section>
      ) : (
        <section className="no-results">
          <span>⌕</span><h2>{t("library.noResultsTitle")}</h2><p>{t("library.noResultsDescription")}</p>
          <button className="secondary-button" type="button" onClick={() => setQuery("")}>{t("library.clearSearch")}</button>
        </section>
      )}
    </main>
  );
}
