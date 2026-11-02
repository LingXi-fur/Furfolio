import { useEffect, type RefObject } from "react";

/** Native modality preserves the mounted page; restore the opener when dismissed. */
export function useModalDialog(
  open: boolean,
  dialogRef: RefObject<HTMLDialogElement | null>,
  initialFocusRef: RefObject<HTMLElement | null>,
  returnFocusRef?: RefObject<HTMLElement | null>,
) {
  useEffect(() => {
    const dialog = dialogRef.current;
    if (!dialog || !open) return;
    const opener = returnFocusRef?.current ?? document.activeElement;
    if (typeof dialog.showModal === "function") dialog.showModal();
    else dialog.setAttribute("open", "");
    initialFocusRef.current?.focus();

    return () => {
      if (typeof dialog.close === "function" && dialog.open) dialog.close();
      else dialog.removeAttribute("open");
      if (opener instanceof HTMLElement && opener.isConnected) opener.focus();
    };
  }, [open, dialogRef, initialFocusRef, returnFocusRef]);
}
