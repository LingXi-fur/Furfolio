import { flushSync } from "react-dom";
import { useCallback, useEffect, useState } from "react";
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

type ViewTransitionLike = { finished: Promise<void> };
type ViewTransitionDocument = Document & { startViewTransition?: (callback: () => void) => ViewTransitionLike };

function reducedMotionActive() {
  return window.matchMedia("(prefers-reduced-motion: reduce)").matches;
}

function startViewTurn(update: () => void) {
  const doc = document as ViewTransitionDocument;
  if (!doc.startViewTransition || reducedMotionActive()) {
    update();
    return;
  }
  doc.startViewTransition(() => { flushSync(update); });
}

function crossFade(update: () => void) {
  const doc = document as ViewTransitionDocument;
  if (!doc.startViewTransition || reducedMotionActive()) {
    update();
    return;
  }
  document.documentElement.dataset.themeTurn = "";
  const transition = doc.startViewTransition(() => { flushSync(update); });
  void transition.finished.finally(() => { delete document.documentElement.dataset.themeTurn; });
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
  const [error, setError] = useState<string | null>(null);

  const loadLibrary = useCallback(async () => {
    setLoading(true);
    setError(null);
    try {
      setCharacters(await listCharacters());
    } catch (loadError) {
      setError(errorMessage(loadError, t("errors.storage")));
    } finally {
      setLoading(false);
    }
  }, [t]);

  useEffect(() => {
    let active = true;
    listCharacters()
      .then((loadedCharacters) => {
        if (active) setCharacters(loadedCharacters);
      })
      .catch((loadError: unknown) => {
        if (active) setError(errorMessage(loadError, t("errors.storage")));
      })
      .finally(() => {
        if (active) setLoading(false);
      });
    return () => { active = false; };
  }, [t]);
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
      if (view.name === "edit") setView({ name: "detail", id: view.id });
      else setView({ name: "library" });
    };
    window.addEventListener("keydown", handleEscape);
    return () => window.removeEventListener("keydown", handleEscape);
  }, [view]);

  function changeTheme() {
    const nextTheme = theme === "dark" ? "light" : "dark";
    localStorage.setItem("furfolio-theme", nextTheme);
    crossFade(() => { setTheme(nextTheme); });
  }

  function changeLanguage(language: "en" | "zh-CN") {
    void i18n.changeLanguage(language);
    localStorage.setItem("furfolio-language", language);
  }

  async function openCharacter(id: string) {
    setError(null);
    try {
      setSelected(await getCharacter(id));
      startViewTurn(() => { setView({ name: "detail", id }); });
    } catch (loadError) {
      setError(errorMessage(loadError, t("errors.storage")));
    }
  }

  async function saveCharacter(input: CharacterInput) {
    setSaving(true);
    setError(null);
    try {
      const saved = view.name === "edit" ? await updateCharacter(view.id, input) : await createCharacter(input);
      setSelected(saved);
      await loadLibrary();
      startViewTurn(() => { setView({ name: "detail", id: saved.id }); });
    } catch (saveError) {
      setError(errorMessage(saveError, t("errors.save")));
    } finally {
      setSaving(false);
    }
  }

  async function removeCharacter() {
    if (!selected) return;
    setDeleting(true);
    setError(null);
    try {
      await deleteCharacter(selected.id);
      setSelected(null);
      await loadLibrary();
      startViewTurn(() => { setView({ name: "library" }); });
    } catch (deleteError) {
      setError(errorMessage(deleteError, t("errors.delete")));
    } finally {
      setDeleting(false);
    }
  }

  function showLibrary() {
    setError(null);
    startViewTurn(() => { setView({ name: "library" }); });
  }

  return (
    <div className={`app-shell view-${view.name}`}>
      <aside className="app-rail">
        <button className="brand" type="button" onClick={showLibrary} aria-label={t("brand")}>
          <span className="brand-icon"><FurfolioMark /></span>
          <span className="brand-word">{t("brand")}</span>
        </button>

        <nav className="navigation" aria-label={t("navigation.label")}>
          <button
            className={`navigation-link${view.name === "library" ? " active" : ""}`}
            type="button"
            onClick={showLibrary}
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
            {characters.slice(0, 8).map((character, index) => (
              <button
                key={character.id}
                type="button"
                className="toc-entry"
                aria-current={"id" in view && view.id === character.id ? "true" : undefined}
                aria-label={t("library.indexEntry", { name: character.name })}
                onClick={() => void openCharacter(character.id)}
              >
                <span className="toc-entry-number">{String(index + 1).padStart(2, "0")}</span>
                <span className="toc-entry-name">{character.name}</span>
                <span className="toc-entry-dots" aria-hidden="true" />
                <span className="toc-entry-plate">{t("library.plateShort", { number: index + 1 })}</span>
              </button>
            ))}
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
        {view.name === "library" && <LibraryView characters={characters} loading={loading} error={error} onCreate={() => { setError(null); startViewTurn(() => { setView({ name: "create" }); }); }} onOpen={(id) => void openCharacter(id)} onRetry={() => void loadLibrary()} />}
        {view.name === "create" && <CharacterForm saving={saving} error={error} onCancel={showLibrary} onSubmit={saveCharacter} />}
        {view.name === "detail" && selected && <CharacterDetail character={selected} plateIndex={characters.findIndex((entry) => entry.id === selected.id) + 1 || 1} deleting={deleting} error={error} onBack={showLibrary} onEdit={() => startViewTurn(() => { setView({ name: "edit", id: selected.id }); })} onDelete={removeCharacter} />}
        {view.name === "edit" && selected && <CharacterForm character={selected} saving={saving} error={error} onCancel={() => startViewTurn(() => { setView({ name: "detail", id: selected.id }); })} onSubmit={saveCharacter} />}
      </div>
    </div>
  );
}

export default App;
