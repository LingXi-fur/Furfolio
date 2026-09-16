import { type CSSProperties, type FormEvent, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import type { Character, CharacterInput } from "../data/characters";
import { CharacterCover } from "./CharacterCover";
import { DEFAULT_PALETTE } from "./characterPalette";

interface CharacterFormProps {
  character?: Character;
  saving: boolean;
  error: string | null;
  onCancel: () => void;
  onSubmit: (input: CharacterInput) => Promise<void>;
}

interface FormValues {
  name: string;
  species: string;
  pronouns: string;
  description: string;
  notes: string;
  tags: string[];
  colors: string[];
}

type SectionName = "identity" | "profile" | "palette";

function initialValues(character?: Character): FormValues {
  return {
    name: character?.name ?? "",
    species: character?.species ?? "",
    pronouns: character?.pronouns ?? "",
    description: character?.description ?? "",
    notes: character?.notes ?? "",
    tags: character?.tags.map((tag) => tag.name) ?? [],
    colors: character?.colors.map((color) => color.hexValue) ?? [...DEFAULT_PALETTE],
  };
}

export function CharacterForm({ character, saving, error, onCancel, onSubmit }: CharacterFormProps) {
  const { t } = useTranslation();
  const [values, setValues] = useState(() => initialValues(character));
  const [nameError, setNameError] = useState(false);
  const [tagText, setTagText] = useState(values.tags.join(", "));
  const [activeSection, setActiveSection] = useState<SectionName>("identity");
  const nameRef = useRef<HTMLInputElement>(null);
  const identityRef = useRef<HTMLFieldSetElement>(null);
  const profileRef = useRef<HTMLFieldSetElement>(null);
  const paletteRef = useRef<HTMLFieldSetElement>(null);

  const filledFields = [values.name, values.species, values.pronouns, values.description, values.notes, tagText]
    .filter((value) => value.trim()).length + values.colors.filter(Boolean).length;
  const completion = Math.round((filledFields / 8) * 100);
  const completedSteps = Math.ceil((completion / 100) * 4);
  const previewStyle = {
    "--preview-primary": values.colors[0],
    "--preview-secondary": values.colors[1],
  } as CSSProperties;

  function updateText(field: "name" | "species" | "pronouns" | "description" | "notes", value: string) {
    setValues((current) => ({ ...current, [field]: value }));
    if (field === "name" && value.trim()) setNameError(false);
  }

  function updateColor(index: number, value: string) {
    setValues((current) => ({
      ...current,
      colors: current.colors.map((color, colorIndex) => colorIndex === index ? value : color),
    }));
  }

  function goToSection(section: SectionName) {
    setActiveSection(section);
    const target = section === "identity" ? identityRef.current : section === "profile" ? profileRef.current : paletteRef.current;
    const reduceMotion = window.matchMedia("(prefers-reduced-motion: reduce)").matches;
    target?.scrollIntoView({ behavior: reduceMotion ? "auto" : "smooth", block: "start" });
    target?.querySelector<HTMLElement>("input, textarea")?.focus({ preventScroll: true });
  }

  function submit(event: FormEvent) {
    event.preventDefault();
    if (!values.name.trim()) {
      setNameError(true);
      setActiveSection("identity");
      nameRef.current?.focus();
      return;
    }
    const tags = tagText.split(",").map((tag) => tag.trim()).filter(Boolean);
    void onSubmit({ ...values, tags });
  }

  return (
    <main className="workspace view-enter" aria-labelledby="form-title">
      <header className="workspace-heading">
        <button className="back-button" type="button" onClick={onCancel}>← {t("actions.back")}</button>
        <div>
          <p className="eyebrow">{character ? t("form.editEyebrow") : t("form.createEyebrow")}</p>
          <h1 id="form-title">{character ? t("form.editTitle") : t("form.createTitle")}</h1>
        </div>
      </header>

      <form className="editor-layout" onSubmit={submit}>
        <aside className="identity-dock character-sticker-dock" style={previewStyle}>
          <div className="record-preview">
            <CharacterCover
              compact
              name={values.name || t("form.untitled")}
              species={values.species || t("character.unspecifiedSpecies")}
              colors={values.colors}
              registerLabel={character ? t("form.editRegister") : t("form.newRegister")}
            />
          </div>
          <div className="dock-identity" aria-live="polite">
            <span>{values.species || t("character.unspecifiedSpecies")}</span>
            <strong>{values.name || t("form.untitled")}</strong>
            <small>{values.pronouns || t("form.pronounsPending")}</small>
          </div>
          <div className="completion-block">
            <span><b>{completion}%</b>{t("form.complete")}</span>
            <div className="completion-track" aria-hidden="true">
              {[0, 1, 2, 3].map((step) => (
                <span className={step < completedSteps ? "filled" : ""} key={step} />
              ))}
            </div>
          </div>
          <nav className="section-nav" aria-label={t("form.sections")}>
            {(["identity", "profile", "palette"] as SectionName[]).map((section, index) => (
              <button
                className={`section-tab section-tab-${section}${activeSection === section ? " active" : ""}`}
                type="button"
                onClick={() => goToSection(section)}
                key={section}
              >
                <span aria-hidden="true">0{index + 1}</span>
                {t(`form.${section}`)}
              </button>
            ))}
          </nav>
          <p className="local-reminder">⌂ {t("privacy.description")}</p>
        </aside>

        <div className="character-form">
          <fieldset ref={identityRef} className="form-section form-section-identity" onFocus={() => setActiveSection("identity")}>
            <legend className="section-heading">
              <span className="section-sticker" aria-hidden="true">01</span>
              <span><strong>{t("form.identity")}</strong><small>{t("form.identityHint")}</small></span>
            </legend>
            <div className="field-grid">
              <label className="field full-field">
                <span>{t("fields.name")} *</span>
                <input ref={nameRef} autoFocus value={values.name} onChange={(event) => updateText("name", event.target.value)} aria-invalid={nameError} />
                {nameError && <small className="field-error">{t("errors.nameRequired")}</small>}
              </label>
              <label className="field">
                <span>{t("fields.species")}</span>
                <input value={values.species} onChange={(event) => updateText("species", event.target.value)} />
              </label>
              <label className="field">
                <span>{t("fields.pronouns")}</span>
                <input value={values.pronouns} onChange={(event) => updateText("pronouns", event.target.value)} />
              </label>
            </div>
          </fieldset>

          <fieldset ref={profileRef} className="form-section form-section-profile" onFocus={() => setActiveSection("profile")}>
            <legend className="section-heading">
              <span className="section-sticker" aria-hidden="true">02</span>
              <span><strong>{t("form.profile")}</strong><small>{t("form.profileHint")}</small></span>
            </legend>
            <label className="field">
              <span>{t("fields.description")}</span>
              <textarea rows={4} value={values.description} onChange={(event) => updateText("description", event.target.value)} />
            </label>
            <label className="field">
              <span>{t("fields.notes")}</span>
              <textarea rows={5} value={values.notes} onChange={(event) => updateText("notes", event.target.value)} />
            </label>
            <label className="field">
              <span>{t("fields.tags")}</span>
              <input value={tagText} onChange={(event) => setTagText(event.target.value)} placeholder={t("form.tagsPlaceholder")} />
              <small>{t("form.tagsHint")}</small>
            </label>
          </fieldset>

          <fieldset ref={paletteRef} className="form-section form-section-palette" onFocus={() => setActiveSection("palette")}>
            <legend className="section-heading">
              <span className="section-sticker" aria-hidden="true">03</span>
              <span><strong>{t("form.palette")}</strong><small>{t("form.paletteHint")}</small></span>
            </legend>
            <div className="color-fields">
              {values.colors.map((color, index) => (
                <label className="color-field" key={index}>
                  <input aria-label={t("form.colorLabel", { number: index + 1 })} type="color" value={color} onChange={(event) => updateColor(index, event.target.value)} />
                  <span><small>{t("form.colorNumber", { number: index + 1 })}</small><strong>{color.toUpperCase()}</strong></span>
                </label>
              ))}
            </div>
          </fieldset>

          {error && <p className="error-banner" role="alert">{error}</p>}
          <div className="form-actions">
            <span>{t("form.saveHint")}</span>
            <div>
              <button className="secondary-button" type="button" onClick={onCancel}>{t("actions.cancel")}</button>
              <button className="primary-button" type="submit" disabled={saving}>{saving ? t("actions.saving") : t("actions.save")}</button>
            </div>
          </div>
        </div>
      </form>
    </main>
  );
}
