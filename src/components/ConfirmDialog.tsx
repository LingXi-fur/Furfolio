import { useEffect, useRef } from "react";
import { useTranslation } from "react-i18next";

interface ConfirmDialogProps {
  open: boolean;
  title: string;
  description: string;
  confirming: boolean;
  error?: string | null;
  onCancel: () => void;
  onConfirm: () => void;
}

export function ConfirmDialog({ open, title, description, confirming, error, onCancel, onConfirm }: ConfirmDialogProps) {
  const { t } = useTranslation();
  const dialogRef = useRef<HTMLDialogElement>(null);
  const cancelRef = useRef<HTMLButtonElement>(null);

  useEffect(() => {
    const dialog = dialogRef.current;
    if (!dialog || !open) return;

    if (typeof dialog.showModal === "function") dialog.showModal();
    else dialog.setAttribute("open", "");
    cancelRef.current?.focus();

    return () => {
      if (typeof dialog.close === "function" && dialog.open) dialog.close();
    };
  }, [open]);

  if (!open) return null;

  return (
    <dialog
      ref={dialogRef}
      className="confirm-dialog"
      aria-labelledby="confirm-title"
      aria-describedby="confirm-description"
      onCancel={(event) => {
        event.preventDefault();
        if (!confirming) onCancel();
      }}
    >
      <span className="dialog-warning" aria-hidden="true">!</span>
      <div>
        <p className="dialog-kicker">{t("dialog.irreversible")}</p>
        <h2 id="confirm-title">{title}</h2>
        <p id="confirm-description">{description}</p>
        {error && <p className="dialog-error" role="alert">{error}</p>}
      </div>
      <div className="dialog-actions">
        <button ref={cancelRef} className="secondary-button" type="button" onClick={onCancel} disabled={confirming}>{t("actions.cancel")}</button>
        <button className="danger-button" type="button" onClick={onConfirm} disabled={confirming}>{confirming ? t("actions.deleting") : t("dialog.confirmDelete")}</button>
      </div>
    </dialog>
  );
}
