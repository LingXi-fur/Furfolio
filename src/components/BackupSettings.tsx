import { useId, useRef, useState, type RefObject } from "react";
import { useTranslation } from "react-i18next";
import { backupErrorCode, backupErrorDirectoryName, exportLibraryBackup, type BackupErrorCode, type BackupResult } from "../data/backup";
import { useModalDialog } from "./useModalDialog";

interface BackupSettingsProps {
  onClose: () => void;
  returnFocusRef?: RefObject<HTMLElement | null>;
}

export function BackupSettings({ onClose, returnFocusRef }: BackupSettingsProps) {
  const { t, i18n } = useTranslation();
  const titleId = useId();
  const warningId = useId();
  const restoreId = useId();
  const dialogRef = useRef<HTMLDialogElement>(null);
  const closeRef = useRef<HTMLButtonElement>(null);
  const pendingRef = useRef(false);
  const [confirmed, setConfirmed] = useState(false);
  const [busy, setBusy] = useState(false);
  const [result, setResult] = useState<BackupResult | null>(null);
  const [failure, setFailure] = useState<{ code: BackupErrorCode; directoryName: string | null } | null>(null);
  useModalDialog(true, dialogRef, closeRef, returnFocusRef);
  const uncertain = failure?.code === "backupDurabilityUncertain";
  const number = new Intl.NumberFormat(i18n.resolvedLanguage);

  function close() {
    if (!pendingRef.current) onClose();
  }

  async function exportBackup() {
    if (pendingRef.current || !confirmed || uncertain) return;
    pendingRef.current = true;
    setBusy(true);
    setFailure(null);
    setResult(null);
    try {
      setResult(await exportLibraryBackup());
    } catch (error) {
      setFailure({ code: backupErrorCode(error), directoryName: backupErrorDirectoryName(error) });
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
      aria-describedby={`${warningId} ${restoreId}`}
      aria-busy={busy || undefined}
      onCancel={(event) => { event.preventDefault(); close(); }}
      onKeyDown={(event) => {
        if (event.key !== "Escape") return;
        event.preventDefault();
        event.stopPropagation();
        close();
      }}
    >
      <header className="backup-heading"><p className="eyebrow">{t("navigation.settings")}</p><h2 id={titleId}>{t("backup.title")}</h2></header>
      <p>{t("backup.description")}</p>
      <div className="backup-warning">
        <p id={warningId}>{t("backup.warning")}</p>
        <p id={restoreId}>{t("backup.restoreWarning")}</p>
      </div>
      <p className="backup-limits">{t("backup.limits")}</p>
      <p className="backup-limits">{t("backup.payloadDefinition")}</p>
      <label className="backup-confirmation">
        <input type="checkbox" checked={confirmed} disabled={busy || uncertain} onChange={(event) => setConfirmed(event.target.checked)} />
        <span>{t("backup.confirmation")}</span>
      </label>
      {busy && <p className="backup-status" role="status">{t("backup.busy")}</p>}
      {result && <div className="backup-status" role="status">
        <p><strong>{t("backup.success")}</strong></p>
        <p className="backup-directory">{result.directoryName}</p>
        <p>{t("backup.characterCount", { count: result.characterCount })} · {t("backup.assetCount", { count: result.assetCount })} · {t("backup.payloadBytes", { bytes: number.format(result.totalBytes) })}</p>
      </div>}
      {failure && <div className="backup-error" role="alert">
        <p>{t(`backup.errors.${failure.code}`)}</p>
        {failure.directoryName && <p className="backup-directory">{failure.directoryName}</p>}
      </div>}
      <div className="dialog-actions">
        <button ref={closeRef} className="secondary-button" type="button" disabled={busy} onClick={close}>{t("backup.close")}</button>
        {!uncertain && <button className="primary-button" type="button" disabled={!confirmed || busy} onClick={() => void exportBackup()}>{busy ? t("backup.busy") : failure ? t("backup.retry") : t("backup.export")}</button>}
      </div>
    </dialog>
  );
}
