import { useState } from "react";
import { useTranslation } from "react-i18next";
import type { Character } from "../data/characters";
import { CharacterCover } from "./CharacterCover";
import { ConfirmDialog } from "./ConfirmDialog";

interface CharacterDetailProps {
  character: Character;
  deleting: boolean;
  error: string | null;
  onBack: () => void;
  onEdit: () => void;
  onDelete: () => Promise<void>;
}

export function CharacterDetail({ character, deleting, error, onBack, onEdit, onDelete }: CharacterDetailProps) {
  const { t } = useTranslation();
  const [confirmingDelete, setConfirmingDelete] = useState(false);

  return (
    <main className="detail-view view-enter">
      <div className="detail-toolbar">
        <button className="back-button" type="button" onClick={onBack}>← {t("actions.backToLibrary")}</button>
        <div className="detail-actions">
          <button className="secondary-button" type="button" onClick={onEdit}>{t("actions.edit")}</button>
          <button className="danger-link" type="button" onClick={() => setConfirmingDelete(true)}>{t("actions.delete")}</button>
        </div>
      </div>

      <section className="detail-record">
        <aside className="detail-cover-wrap">
          <CharacterCover name={character.name} species={character.species} colors={character.colors} />
          <span className="no-image-badge">{t("detail.noReferenceImage")}</span>
        </aside>

        <div className="detail-body">
          <header className="detail-identity">
            <div className="detail-register"><span>{t("detail.register")}</span><strong>{t("detail.localRecord")}</strong></div>
            <p className="eyebrow">{character.species ?? t("character.unspecifiedSpecies")}</p>
            <h1>{character.name}</h1>
            {character.pronouns && <p className="detail-pronouns">{character.pronouns}</p>}
            <div className="tag-row detail-tags">
              {character.tags.map((tag) => <span className="tag" key={tag.id}>{tag.name}</span>)}
              {character.tags.length === 0 && <span className="muted-text">{t("detail.noTags")}</span>}
            </div>
            <div className="palette-row" aria-label={t("detail.palette")}>
              {character.colors.map((color) => <span className="color-chip" key={color.id}><i style={{ background: color.hexValue }} />{color.hexValue}</span>)}
            </div>
          </header>

          {error && !confirmingDelete && <p className="error-banner" role="alert">{error}</p>}

          <article className="detail-section detail-description">
            <span className="panel-index">01</span>
            <div><h2>{t("fields.description")}</h2><p>{character.description || t("detail.emptyDescription")}</p></div>
          </article>
          <article className="detail-section detail-notes">
            <span className="panel-index">02</span>
            <div><h2>{t("fields.notes")}</h2><p>{character.notes || t("detail.emptyNotes")}</p></div>
          </article>
          <article className="detail-section detail-references">
            <span className="panel-index">03</span>
            <div><h2>{t("detail.references")}</h2><p>{t("detail.referencesHint")}</p><span className="soon-chip">{t("comingSoon")}</span></div>
          </article>
        </div>
      </section>

      <ConfirmDialog
        open={confirmingDelete}
        title={t("detail.deleteTitle", { name: character.name })}
        description={t("detail.deleteConfirmation", { name: character.name })}
        confirming={deleting}
        error={error}
        onCancel={() => setConfirmingDelete(false)}
        onConfirm={() => void onDelete()}
      />
    </main>
  );
}
