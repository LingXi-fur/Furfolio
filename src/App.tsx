import { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import "./App.css";

type Theme = "light" | "dark";

function getInitialTheme(): Theme {
  const savedTheme = localStorage.getItem("furfolio-theme");
  if (savedTheme === "light" || savedTheme === "dark") {
    return savedTheme;
  }

  return window.matchMedia("(prefers-color-scheme: dark)").matches ? "dark" : "light";
}

function PawMark() {
  return (
    <svg aria-hidden="true" className="brand-mark" viewBox="0 0 48 48">
      <path d="M24 20.5c-6.7 0-13.3 6.1-13.3 12.2 0 4.2 3.3 6.8 7.3 5.6 2.4-.7 3.8-1.7 6-1.7s3.6 1 6 1.7c4 1.2 7.3-1.4 7.3-5.6 0-6.1-6.6-12.2-13.3-12.2Z" />
      <ellipse cx="9.8" cy="21" rx="5.1" ry="6.5" transform="rotate(-25 9.8 21)" />
      <ellipse cx="19.1" cy="11.2" rx="5.2" ry="6.8" transform="rotate(-8 19.1 11.2)" />
      <ellipse cx="28.9" cy="11.2" rx="5.2" ry="6.8" transform="rotate(8 28.9 11.2)" />
      <ellipse cx="38.2" cy="21" rx="5.1" ry="6.5" transform="rotate(25 38.2 21)" />
    </svg>
  );
}

function App() {
  const { i18n, t } = useTranslation();
  const [theme, setTheme] = useState<Theme>(getInitialTheme);

  useEffect(() => {
    document.documentElement.dataset.theme = theme;
  }, [theme]);

  useEffect(() => {
    const colorScheme = window.matchMedia("(prefers-color-scheme: dark)");

    function followSystemTheme(event: MediaQueryListEvent) {
      if (localStorage.getItem("furfolio-theme") === null) {
        setTheme(event.matches ? "dark" : "light");
      }
    }

    colorScheme.addEventListener("change", followSystemTheme);
    return () => colorScheme.removeEventListener("change", followSystemTheme);
  }, []);

  useEffect(() => {
    document.documentElement.lang = i18n.resolvedLanguage ?? "en";
  }, [i18n.resolvedLanguage]);

  function changeTheme() {
    const nextTheme = theme === "dark" ? "light" : "dark";
    setTheme(nextTheme);
    localStorage.setItem("furfolio-theme", nextTheme);
  }

  function changeLanguage(language: "en" | "zh-CN") {
    void i18n.changeLanguage(language);
    localStorage.setItem("furfolio-language", language);
  }

  return (
    <div className="app-shell">
      <header className="topbar">
        <a className="brand" href="#main" aria-label={t("brand")}>
          <span className="brand-icon">
            <PawMark />
          </span>
          <span>{t("brand")}</span>
        </a>

        <nav className="navigation" aria-label={t("navigation.library")}>
          <a className="navigation-link active" href="#main" aria-current="page">
            {t("navigation.library")}
          </a>
          <button className="navigation-link" type="button">
            {t("navigation.settings")}
          </button>
        </nav>

        <div className="toolbar">
          <label className="language-control">
            <span className="sr-only">{t("language.label")}</span>
            <select
              aria-label={t("language.label")}
              value={i18n.resolvedLanguage?.startsWith("zh") ? "zh-CN" : "en"}
              onChange={(event) => changeLanguage(event.target.value as "en" | "zh-CN")}
            >
              <option value="zh-CN">{t("language.chinese")}</option>
              <option value="en">{t("language.english")}</option>
            </select>
          </label>
          <button
            className="icon-button"
            type="button"
            aria-label={theme === "dark" ? t("theme.light") : t("theme.dark")}
            onClick={changeTheme}
          >
            <span aria-hidden="true">{theme === "dark" ? "☀" : "☾"}</span>
          </button>
        </div>
      </header>

      <main id="main" className="main-content">
        <section className="empty-state" aria-labelledby="empty-title">
          <div className="artwork" aria-hidden="true">
            <div className="artwork-ring ring-one" />
            <div className="artwork-ring ring-two" />
            <div className="artwork-card artwork-card-back" />
            <div className="artwork-card artwork-card-front">
              <PawMark />
            </div>
            <span className="spark spark-one">✦</span>
            <span className="spark spark-two">✦</span>
          </div>

          <p className="eyebrow">{t("empty.eyebrow")}</p>
          <h1 id="empty-title">{t("empty.title")}</h1>
          <p className="intro">{t("empty.description")}</p>

          <div className="actions">
            <button className="primary-button" type="button">
              <span aria-hidden="true">＋</span>
              {t("empty.create")}
            </button>
            <button className="secondary-button" type="button">
              {t("empty.restore")}
            </button>
          </div>

          <aside className="privacy-note">
            <span className="privacy-icon" aria-hidden="true">⌂</span>
            <span>
              <strong>{t("privacy.title")}</strong>
              <small>{t("privacy.description")}</small>
            </span>
          </aside>
        </section>
      </main>

      <footer>{t("status")}</footer>
    </div>
  );
}

export default App;
