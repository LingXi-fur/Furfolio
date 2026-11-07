import { useId, useRef, type RefObject } from "react";
import { useTranslation } from "react-i18next";
import { useModalDialog } from "./useModalDialog";
import { AlertIcon } from "./icons";

interface ConfirmDialogProps {
  open: boolean;
  title: string;
  description: string;
  confirming: boolean;
  confirmLabel?: string;
  error?: string | null;
  returnFocusRef?: RefObject<HTMLElement | null>;
  onCancel: () => void;
  onConfirm: () => void;
}

export function ConfirmDialog({ open, title, description, confirming, confirmLabel, error, returnFocusRef, onCancel, onConfirm }: ConfirmDialogProps) {
  const { t } = useTranslation();
  const titleId = useId();
  const descriptionId = useId();
  const dialogRef = useRef<HTMLDialogElement>(null);
  const cancelRef = useRef<HTMLButtonElement>(null);

  useModalDialog(open, dialogRef, cancelRef, returnFocusRef);

  if (!open) return null;

  return (
    <dialog
      ref={dialogRef}
      className="confirm-dialog"
      aria-labelledby={titleId}
      aria-describedby={descriptionId}
      onCancel={(event) => {
        event.preventDefault();
        if (!confirming) onCancel();
      }}
    >
      <span className="dialog-warning"><AlertIcon /></span>
      <div>
        <p className="dialog-kicker">{t("dialog.irreversible")}</p>
        <h2 id={titleId}>{title}</h2>
        <p id={descriptionId}>{description}</p>
        {error && <p className="dialog-error" role="alert">{error}</p>}
      </div>
      <div className="dialog-actions">
        <button ref={cancelRef} className="secondary-button" type="button" onClick={onCancel} disabled={confirming}>{t("actions.cancel")}</button>
        <button className="danger-button" type="button" onClick={onConfirm} disabled={confirming}>{confirming ? t("actions.deleting") : confirmLabel ?? t("dialog.confirmDelete")}</button>
      </div>
    </dialog>
  );
}
