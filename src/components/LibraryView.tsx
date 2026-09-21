import { useEffect, useMemo, useRef } from "react";
import { useTranslation } from "react-i18next";
import type { CharacterSummary } from "../data/characters";
import { FeatureStage } from "./FeatureStage";
import { IndexEntry } from "./IndexEntry";

interface LibraryViewProps {
  characters: CharacterSummary[];
  loading: boolean;
  error: string | null;
  notice: string | null;
  openingCharacterId: string | null;
  query: string;
  returnFocusTarget: "create" | { characterId: string; origin: "library" | "contents" } | null;
  onReturnFocus: () => void;
  onQueryChange: (query: string) => void;
  onCreate: () => void;
  onOpen: (id: string) => void;
  onRetry: () => void;
  onDismissNotice: () => void;
}

function isRendered(element: HTMLElement | null) {
  for (let current: HTMLElement | null = element; current; current = current.parentElement) {
    const style = window.getComputedStyle(current);
    if (style.display === "none" || style.visibility === "hidden") return false;
  }
  return element !== null;
}

export function LibraryView({ characters, loading, error, notice, openingCharacterId, query, returnFocusTarget, onReturnFocus, onQueryChange, onCreate, onOpen, onRetry, onDismissNotice }: LibraryViewProps) {
  const { t } = useTranslation();
  const loadingRef = useRef<HTMLElement>(null);
  const searchRef = useRef<HTMLInputElement>(null);
  const createRef = useRef<HTMLButtonElement>(null);
  const normalizedQuery = query.trim().toLocaleLowerCase();
  const characterIndex = useMemo(
    () => new Map(characters.map((character, index) => [character.id, index + 1])),
    [characters]
  );
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

  useEffect(() => {
    if (!loading || !returnFocusTarget) return;
    loadingRef.current?.focus();
  }, [loading, returnFocusTarget]);

  useEffect(() => {
    if (!returnFocusTarget || loading || error) return;

    const libraryTarget = returnFocusTarget === "create"
      ? createRef.current
      : document.querySelector<HTMLElement>(
        `.library-view [data-character-id="${CSS.escape(returnFocusTarget.characterId)}"]`
      );
    const contentsTarget = returnFocusTarget !== "create" && returnFocusTarget.origin === "contents"
      ? document.querySelector<HTMLElement>(
        `.toc-entry[data-character-id="${CSS.escape(returnFocusTarget.characterId)}"]`
      )
      : null;
    const target = isRendered(contentsTarget)
      ? contentsTarget
      : isRendered(libraryTarget)
        ? libraryTarget
        : searchRef.current ?? createRef.current;

    target?.focus();
    onReturnFocus();
  }, [characters.length, error, loading, onReturnFocus, returnFocusTarget]);

  function dismissNotice() {
    onDismissNotice();
    requestAnimationFrame(() => searchRef.current?.focus());
  }

  function clearSearch() {
    onQueryChange("");
    searchRef.current?.focus();
  }

  if (loading) {
    return <main ref={loadingRef} className="state-screen view-enter" tabIndex={-1} aria-busy="true"><span className="loading-line" /><p>{t("library.loading")}</p></main>;
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
            <button ref={createRef} className="primary-button" type="button" onClick={onCreate}>
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
      <header className="library-masthead">
        <div className="library-title-block">
          <p className="eyebrow">{t("library.eyebrow")}</p>
          <h1>{t("library.characters")}</h1>
        </div>

        <section className="library-tools" aria-label={t("library.tools")}>
          <label className="search-field">
            <span aria-hidden="true">⌕</span>
            <span className="sr-only">{t("library.searchLabel")}</span>
            <input ref={searchRef} value={query} onChange={(event) => onQueryChange(event.target.value)} placeholder={t("library.searchPlaceholder")} />
            {query ? <button type="button" onClick={clearSearch} aria-label={t("library.clearSearchInput")}>×</button> : <kbd aria-hidden="true">/</kbd>}
          </label>
          <span className="library-tally">
            <strong aria-hidden="true">{String(filtered.length).padStart(2, "0")}</strong>
            <span>{t("library.resultCount", { count: filtered.length })}</span>
          </span>
          <button ref={createRef} className="primary-button" type="button" onClick={onCreate}>{t("library.addCharacter")} <span aria-hidden="true">＋</span></button>
        </section>
      </header>

      {notice && (
        <div className="library-notice" role="alert">
          <span>{notice}</span>
          <button type="button" onClick={dismissNotice} aria-label={t("library.dismissNotice")}>×</button>
        </div>
      )}

      {filtered.length > 0 ? (
        <>
          <FeatureStage
            character={filtered[0]}
            index={characterIndex.get(filtered[0].id) ?? 1}
            opening={openingCharacterId === filtered[0].id}
            onOpen={onOpen}
          />
          {filtered.length > 1 && (
            <section className="library-index" aria-labelledby="library-index-heading">
              <header className="index-heading">
                <div>
                  <p className="eyebrow">{t("library.indexEyebrow")}</p>
                  <h2 id="library-index-heading">{t("library.indexHeading")}</h2>
                </div>
                <p>{t("library.indexDescription", { count: filtered.length - 1 })}</p>
              </header>
              <div className="index-list">
                {filtered.slice(1).map((character, order) => (
                  <IndexEntry
                    character={character}
                    index={characterIndex.get(character.id) ?? order + 2}
                    order={order}
                    opening={openingCharacterId === character.id}
                    onOpen={onOpen}
                    key={character.id}
                  />
                ))}
              </div>
            </section>
          )}
        </>
      ) : (
        <section className="no-results">
          <span>⌕</span><h2>{t("library.noResultsTitle")}</h2><p>{t("library.noResultsDescription")}</p>
          <button className="secondary-button" type="button" onClick={clearSearch}>{t("library.clearSearch")}</button>
        </section>
      )}
    </main>
  );
}
