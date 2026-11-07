import { useId, useRef, useState, type RefObject } from "react";
import { useTranslation } from "react-i18next";
import { restoreErrorCode, restoreLibraryBackup, type RestoreErrorCode, type RestoreResult } from "../data/restore";
import { useModalDialog } from "./useModalDialog";

interface RestoreSettingsProps {
  onClose: () => void;
  onRestored: () => void;
  returnFocusRef?: RefObject<HTMLElement | null>;
}

export function RestoreSettings({ onClose, onRestored, returnFocusRef }: RestoreSettingsProps) {
  const { t, i18n } = useTranslation();
  const titleId = useId();
  const warningId = useId();
  const dialogRef = useRef<HTMLDialogElement>(null);
  const closeRef = useRef<HTMLButtonElement>(null);
  const pendingRef = useRef(false);
  const [busy, setBusy] = useState(false);
  const [result, setResult] = useState<RestoreResult | null>(null);
  const [failure, setFailure] = useState<{ code: RestoreErrorCode } | null>(null);
  useModalDialog(true, dialogRef, closeRef, returnFocusRef);
  const number = new Intl.NumberFormat(i18n.resolvedLanguage);

  function close() {
    if (!pendingRef.current) onClose();
  }

  async function restoreBackup() {
    if (pendingRef.current) return;
    pendingRef.current = true;
    setBusy(true);
    setFailure(null);
    setResult(null);
    try {
      const restored = await restoreLibraryBackup();
      if (restored) {
        setResult(restored);
        onRestored();
      }
    } catch (error) {
      setFailure({ code: restoreErrorCode(error) });
    } finally {
      pendingRef.current = false;
      setBusy(false);
    }
  }

  return (
    <dialog
      ref={dialogRef}
      className="backup-settings"
      aria-labelledby={titleId}
      aria-describedby={warningId}
      aria-busy={busy || undefined}
      onCancel={(event) => { event.preventDefault(); close(); }}
      onKeyDown={(event) => {
        if (event.key !== "Escape") return;
        event.preventDefault();
        event.stopPropagation();
        close();
      }}
    >
      <header className="backup-heading"><p className="eyebrow">{t("navigation.settings")}</p><h2 id={titleId}>{t("restore.title")}</h2></header>
      <p>{t("restore.description")}</p>
      <div className="backup-warning">
        <p id={warningId}>{t("restore.warning")}</p>
      </div>
      {busy && <p className="backup-status" role="status">{t("restore.busy")}</p>}
      {result && <div className="backup-status" role="status">
        <p><strong>{t("restore.success")}</strong></p>
        <p>{t("restore.characterCount", { count: result.characterCount })} · {t("restore.assetCount", { count: result.assetCount })} · {t("restore.payloadBytes", { bytes: number.format(result.totalBytes) })}</p>
      </div>}
      {failure && <div className="backup-error" role="alert">
        <p>{t(`restore.errors.${failure.code}`)}</p>
      </div>}
      <div className="dialog-actions">
        <button ref={closeRef} className="secondary-button" type="button" disabled={busy} onClick={close}>{t("restore.close")}</button>
        <button className="primary-button" type="button" disabled={busy} onClick={() => void restoreBackup()}>{busy ? t("restore.busy") : failure ? t("restore.retry") : t("restore.select")}</button>
      </div>
    </dialog>
  );
}
