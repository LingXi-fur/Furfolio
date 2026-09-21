import { flushSync } from "react-dom";
import { useCallback, useEffect, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import "./App.css";
import { CharacterDetail } from "./components/CharacterDetail";
import { CharacterForm } from "./components/CharacterForm";
import { LibraryView } from "./components/LibraryView";
import {
  createCharacter,
  deleteCharacter,
  getCharacter,
  listCharacters,
  updateCharacter,
  type Character,
  type CharacterInput,
  type CharacterSummary,
  type CommandError,
} from "./data/characters";

type Theme = "light" | "dark";
type View = { name: "library" } | { name: "create" } | { name: "detail"; id: string } | { name: "edit"; id: string };
type CharacterFocusOrigin = "library" | "contents";
type LibraryFocusTarget = "create" | { characterId: string; origin: CharacterFocusOrigin } | null;

function getInitialTheme(): Theme {
  const savedTheme = localStorage.getItem("furfolio-theme");
  if (savedTheme === "light" || savedTheme === "dark") return savedTheme;
  return window.matchMedia("(prefers-color-scheme: dark)").matches ? "dark" : "light";
}

function FurfolioMark() {
  return (
    <svg aria-hidden="true" className="brand-mark" viewBox="0 0 44 44">
      <path className="mark-sheet mark-sheet-back" d="M10 9h21l5 5v22H10Z" />
      <path className="mark-sheet mark-sheet-front" d="M7 6h21l5 5v22H7Z" />
      <path className="mark-fold" d="M28 6v6h5" />
      <path className="mark-letter" d="M14 14h10M14 14v13M14 20h8" />
      <path className="mark-swatch" d="M25 26h4v4h-4z" />
    </svg>
  );
}

function LibraryIcon() {
  return <svg aria-hidden="true" viewBox="0 0 24 24"><path d="M4 5.5h16v14H4zM8 5.5v14M12 9h5M12 13h5" /></svg>;
}

function SettingsIcon() {
  return <svg aria-hidden="true" viewBox="0 0 24 24"><path d="M12 8.5a3.5 3.5 0 1 0 0 7 3.5 3.5 0 0 0 0-7Z" /><path d="m19 13.5 1.4 1.1-2 3.4-1.8-.7a7 7 0 0 1-2.1 1.2l-.3 1.9h-4l-.3-1.9a7 7 0 0 1-2.1-1.2L6 18l-2-3.4 1.4-1.1a7 7 0 0 1 0-2.5L4 9.9l2-3.4 1.8.7A7 7 0 0 1 10 6l.3-1.9h4l.3 1.9a7 7 0 0 1 2.1 1.2l1.8-.7 2 3.4-1.4 1.1a7 7 0 0 1 0 2.5Z" /></svg>;
}

function errorMessage(error: unknown, fallback: string) {
  if (typeof error === "object" && error !== null && "message" in error) {
    return String((error as CommandError).message);
  }
  return fallback;
}

function toSummary(character: Character): CharacterSummary {
  const { id, name, avatarAssetId, species, pronouns, tags, colors, updatedAt } = character;
  return { id, name, avatarAssetId, species, pronouns, tags, colors, updatedAt };
}

function upsertSummary(characters: CharacterSummary[], character: Character) {
  const summary = toSummary(character);
  return [...characters.filter((entry) => entry.id !== summary.id), summary].sort((left, right) => {
    const nameOrder = left.name.toLocaleLowerCase().localeCompare(right.name.toLocaleLowerCase());
    return nameOrder || left.id.localeCompare(right.id);
  });
}

type ViewTransitionLike = { finished: Promise<void> };
type ViewTransitionDocument = Document & { startViewTransition?: (callback: () => void) => ViewTransitionLike };

function settleViewTransition(transition: ViewTransitionLike) {
  void transition.finished.catch(() => undefined);
}

function reducedMotionActive() {
  return window.matchMedia("(prefers-reduced-motion: reduce)").matches;
}

function startViewTurn(update: () => void) {
  const doc = document as ViewTransitionDocument;
  if (!doc.startViewTransition || reducedMotionActive()) {
    update();
    return;
  }
  settleViewTransition(doc.startViewTransition(() => { flushSync(update); }));
}

function crossFade(update: () => void) {
  const doc = document as ViewTransitionDocument;
  if (!doc.startViewTransition || reducedMotionActive()) {
    update();
    return;
  }
  document.documentElement.dataset.themeTurn = "";
  const transition = doc.startViewTransition(() => { flushSync(update); });
  void transition.finished.then(
    () => { delete document.documentElement.dataset.themeTurn; },
    () => { delete document.documentElement.dataset.themeTurn; }
  );
}

function App() {
  const { i18n, t } = useTranslation();
  const [theme, setTheme] = useState<Theme>(getInitialTheme);
  const [view, setView] = useState<View>({ name: "library" });
  const [characters, setCharacters] = useState<CharacterSummary[]>([]);
  const [selected, setSelected] = useState<Character | null>(null);
  const [loading, setLoading] = useState(true);
  const [saving, setSaving] = useState(false);
  const [deleting, setDeleting] = useState(false);
  const [libraryError, setLibraryError] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [libraryNotice, setLibraryNotice] = useState<string | null>(null);
  const [libraryQuery, setLibraryQuery] = useState("");
  const [openingCharacterId, setOpeningCharacterId] = useState<string | null>(null);
  const [libraryFocusTarget, setLibraryFocusTarget] = useState<LibraryFocusTarget>(null);
  const openRequestRef = useRef(0);
  const saveRequestRef = useRef(0);
  const libraryRequestRef = useRef(0);

  const cancelSaveRequest = useCallback(() => {
    saveRequestRef.current += 1;
    setSaving(false);
  }, []);

  const loadLibrary = useCallback(async () => {
    const requestId = ++libraryRequestRef.current;
    setLoading(true);
    setLibraryError(null);
    try {
      const loadedCharacters = await listCharacters();
      if (requestId === libraryRequestRef.current) setCharacters(loadedCharacters);
    } catch (loadError) {
      if (requestId === libraryRequestRef.current) setLibraryError(errorMessage(loadError, i18n.t("errors.storage")));
    } finally {
      if (requestId === libraryRequestRef.current) setLoading(false);
    }
  }, [i18n]);

  useEffect(() => {
    let active = true;
    const requestId = ++libraryRequestRef.current;
    listCharacters()
      .then((loadedCharacters) => {
        if (active && requestId === libraryRequestRef.current) setCharacters(loadedCharacters);
      })
      .catch((loadError: unknown) => {
        if (active && requestId === libraryRequestRef.current) setLibraryError(errorMessage(loadError, i18n.t("errors.storage")));
      })
      .finally(() => {
        if (active && requestId === libraryRequestRef.current) setLoading(false);
      });
    return () => { active = false; };
  }, [i18n]);
  useEffect(() => { document.documentElement.dataset.theme = theme; }, [theme]);
  useEffect(() => {
    const colorScheme = window.matchMedia("(prefers-color-scheme: dark)");
    const follow = (event: MediaQueryListEvent) => {
      if (localStorage.getItem("furfolio-theme") === null) setTheme(event.matches ? "dark" : "light");
    };
    colorScheme.addEventListener("change", follow);
    return () => colorScheme.removeEventListener("change", follow);
  }, []);
  useEffect(() => { document.documentElement.lang = i18n.resolvedLanguage ?? "en"; }, [i18n.resolvedLanguage]);
  useEffect(() => {
    const handleEscape = (event: KeyboardEvent) => {
      if (event.key !== "Escape" || view.name === "library" || document.querySelector("dialog[open]")) return;
      openRequestRef.current += 1;
      cancelSaveRequest();
      setOpeningCharacterId(null);
      setError(null);
      startViewTurn(() => {
        if (view.name === "edit") {
          setView({ name: "detail", id: view.id });
        } else {
          setLibraryFocusTarget(
            view.name === "create"
              ? "create"
              : libraryFocusTarget ?? { characterId: view.id, origin: "library" }
          );
          setView({ name: "library" });
        }
      });
    };
    window.addEventListener("keydown", handleEscape);
    return () => window.removeEventListener("keydown", handleEscape);
  }, [cancelSaveRequest, libraryFocusTarget, view]);

  function changeTheme() {
    const nextTheme = theme === "dark" ? "light" : "dark";
    localStorage.setItem("furfolio-theme", nextTheme);
    crossFade(() => { setTheme(nextTheme); });
  }

  function changeLanguage(language: "en" | "zh-CN") {
    void i18n.changeLanguage(language);
    localStorage.setItem("furfolio-language", language);
  }

  function cancelOpenRequest() {
    openRequestRef.current += 1;
    setOpeningCharacterId(null);
  }

  function navigate(nextView: View) {
    cancelOpenRequest();
    cancelSaveRequest();
    setError(null);
    startViewTurn(() => { setView(nextView); });
  }

  async function openCharacter(id: string, origin: CharacterFocusOrigin = "library") {
    if (
      openingCharacterId === id ||
      ("id" in view && view.id === id && selected?.id === id)
    ) return;

    const requestId = ++openRequestRef.current;
    setOpeningCharacterId(id);
    setLibraryNotice(null);
    setError(null);
    try {
      const character = await getCharacter(id);
      if (requestId !== openRequestRef.current) return;
      setOpeningCharacterId(null);
      setSelected(character);
      startViewTurn(() => {
        setLibraryFocusTarget({ characterId: id, origin });
        setView({ name: "detail", id });
      });
    } catch (loadError) {
      if (requestId !== openRequestRef.current) return;
      setOpeningCharacterId(null);
      const message = errorMessage(loadError, t("errors.storage"));
      if (view.name === "library") setLibraryNotice(message);
      else setError(message);
    }
  }

  async function saveCharacter(input: CharacterInput) {
    cancelOpenRequest();
    const requestId = ++saveRequestRef.current;
    setSaving(true);
    setError(null);
    try {
      const saved = view.name === "edit" ? await updateCharacter(view.id, input) : await createCharacter(input);
      setCharacters((current) => upsertSummary(current, saved));
      if (requestId !== saveRequestRef.current) return;
      setSelected(saved);
      await loadLibrary();
      if (requestId !== saveRequestRef.current) return;
      startViewTurn(() => { setView({ name: "detail", id: saved.id }); });
    } catch (saveError) {
      if (requestId === saveRequestRef.current) setError(errorMessage(saveError, t("errors.save")));
    } finally {
      if (requestId === saveRequestRef.current) setSaving(false);
    }
  }

  async function removeCharacter() {
    if (!selected) return;
    cancelOpenRequest();
    cancelSaveRequest();
    setDeleting(true);
    setError(null);
    try {
      await deleteCharacter(selected.id);
      startViewTurn(() => {
        setSelected(null);
        setLibraryFocusTarget("create");
        setView({ name: "library" });
      });
      await loadLibrary();
    } catch (deleteError) {
      setError(errorMessage(deleteError, t("errors.delete")));
    } finally {
      setDeleting(false);
    }
  }

  function showLibrary(focusTarget: LibraryFocusTarget = null) {
    setLibraryNotice(null);
    cancelOpenRequest();
    cancelSaveRequest();
    startViewTurn(() => {
      setLibraryFocusTarget(focusTarget);
      setView({ name: "library" });
    });
  }

  return (
    <div className={`app-shell view-${view.name}`}>
      <aside className="app-rail">
        <button className="brand" type="button" onClick={() => showLibrary()} aria-label={t("brand")}>
          <span className="brand-icon"><FurfolioMark /></span>
          <span className="brand-word">{t("brand")}</span>
        </button>

        <nav className="navigation" aria-label={t("navigation.label")}>
          <button
            className={`navigation-link${view.name === "library" ? " active" : ""}`}
            type="button"
            onClick={() => showLibrary()}
            aria-label={t("navigation.library")}
          >
            <LibraryIcon /><span>{t("navigation.library")}</span>
          </button>
          <button
            className="navigation-link"
            type="button"
            disabled
            title={t("comingSoon")}
            aria-label={t("navigation.settings")}
          >
            <SettingsIcon /><span>{t("navigation.settings")}</span>
          </button>
        </nav>

        {!loading && characters.length > 0 && (
          <nav className="rail-toc" aria-label={t("library.indexLabel")}>
            <p className="toc-heading">{t("library.indexTitle")}<span className="toc-total">· {String(characters.length).padStart(2, "0")}</span></p>
            {characters.slice(0, 8).map((character, index) => {
              const plateNumber = String(index + 1).padStart(2, "0");

              return (
                <button
                  key={character.id}
                  type="button"
                  className="toc-entry"
                  aria-current={"id" in view && view.id === character.id ? "true" : undefined}
                  aria-busy={openingCharacterId === character.id || undefined}
                  aria-disabled={openingCharacterId === character.id || undefined}
                  disabled={"id" in view && view.id === character.id}
                  aria-label={t("library.indexEntry", {
                    name: character.name,
                    record: t("library.recordNumber", { number: plateNumber }),
                  })}
                  data-character-id={character.id}
                  onClick={() => void openCharacter(character.id, "contents")}
                >
                  <span className="toc-entry-number">{plateNumber}</span>
                  <span className="toc-entry-name">{character.name}</span>
                  <span className="toc-entry-dots" aria-hidden="true" />
                  <span className="toc-entry-plate">{openingCharacterId === character.id ? "…" : t("library.plateShort", { number: plateNumber })}</span>
                </button>
              );
            })}
            {characters.length > 8 && <p className="toc-overflow">+{characters.length - 8}</p>}
          </nav>
        )}
        {!loading && characters.length === 0 && (
          <p className="toc-awaiting">{t("library.indexAwaiting")}</p>
        )}

        <div className="rail-controls">
          <label className="language-control">
            <span className="control-label">{t("language.label")}</span>
            <select aria-label={t("language.label")} value={i18n.resolvedLanguage?.startsWith("zh") ? "zh-CN" : "en"} onChange={(event) => changeLanguage(event.target.value as "en" | "zh-CN")}>
              <option value="zh-CN">{t("language.chinese")}</option>
              <option value="en">{t("language.english")}</option>
            </select>
          </label>
          <button className="theme-control" type="button" aria-label={theme === "dark" ? t("theme.light") : t("theme.dark")} onClick={changeTheme}>
            <span aria-hidden="true">{theme === "dark" ? "☀" : "☾"}</span><span>{theme === "dark" ? t("theme.lightShort") : t("theme.darkShort")}</span>
          </button>
          <p className="local-status"><span />{t("status")}</p>
        </div>
      </aside>

      <div className="app-content">
        {view.name === "library" && <LibraryView characters={characters} loading={loading} error={libraryError} notice={libraryNotice} openingCharacterId={openingCharacterId} query={libraryQuery} returnFocusTarget={libraryFocusTarget} onReturnFocus={() => setLibraryFocusTarget(null)} onQueryChange={setLibraryQuery} onCreate={() => { setError(null); setLibraryNotice(null); navigate({ name: "create" }); }} onOpen={(id) => void openCharacter(id, "library")} onRetry={() => void loadLibrary()} onDismissNotice={() => setLibraryNotice(null)} />}
        {view.name === "create" && <CharacterForm saving={saving} error={error} onCancel={() => showLibrary("create")} onSubmit={saveCharacter} />}
        {view.name === "detail" && selected && <CharacterDetail character={selected} plateIndex={characters.findIndex((entry) => entry.id === selected.id) + 1 || 1} deleting={deleting} error={error} onBack={() => showLibrary(libraryFocusTarget ?? { characterId: selected.id, origin: "library" })} onEdit={() => navigate({ name: "edit", id: selected.id })} onDelete={removeCharacter} />}
        {view.name === "edit" && selected && <CharacterForm character={selected} saving={saving} error={error} onCancel={() => navigate({ name: "detail", id: selected.id })} onSubmit={saveCharacter} />}
      </div>
    </div>
  );
}

export default App;
