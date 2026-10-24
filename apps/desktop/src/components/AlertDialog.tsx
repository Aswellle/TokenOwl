import {
  useEffect,
  useId,
  useRef,
  type ReactNode,
  type RefObject,
} from "react";

/**
 * Shared modal confirmation with the full keyboard contract.
 *
 * Every destructive or privacy-relevant confirmation in the app renders
 * through this component so the interaction rules exist once: the dialog is
 * announced (`alertdialog` + labelled + described), it behaves like a modal
 * (`aria-modal`, Tab cycles inside, background controls are unreachable by
 * keyboard), Escape cancels, the initial focus lands on the non-destructive
 * action so a stray Enter cannot confirm, and focus returns to the trigger
 * on close. The visual shell stays with the caller via `className`, so the
 * LAN prompt and any future confirm keep their own styling.
 */
interface AlertDialogProps {
  title: string;
  body: ReactNode;
  confirmLabel: string;
  cancelLabel: string;
  onConfirm: () => void;
  onCancel: () => void;
  /** Focus returns here after the dialog closes, on either path. */
  returnFocusTo?: RefObject<HTMLElement | null>;
  /** Container class; callers keep their own visual language. */
  className?: string;
  /** Extra class for the confirm button (e.g. `btn--danger`). */
  confirmClassName?: string;
}

export function AlertDialog({
  title,
  body,
  confirmLabel,
  cancelLabel,
  onConfirm,
  onCancel,
  returnFocusTo,
  className = "alert-dialog",
  confirmClassName = "btn btn--primary",
}: AlertDialogProps) {
  const titleId = useId();
  const bodyId = useId();
  const dialogRef = useRef<HTMLDivElement>(null);
  const cancelRef = useRef<HTMLButtonElement>(null);

  useEffect(() => {
    // The non-destructive action takes the initial focus: an accidental
    // Enter must cancel, never confirm.
    cancelRef.current?.focus();
  }, []);

  const finish = (action: "confirm" | "cancel") => {
    returnFocusTo?.current?.focus();
    if (action === "confirm") {
      onConfirm();
    } else {
      onCancel();
    }
  };

  return (
    <div
      className={className}
      role="alertdialog"
      aria-modal="true"
      aria-labelledby={titleId}
      aria-describedby={bodyId}
      ref={dialogRef}
      onKeyDown={(event) => {
        if (event.key === "Escape") {
          event.preventDefault();
          finish("cancel");
          return;
        }
        if (event.key === "Tab") {
          // Two actions only: keep Tab (and Shift+Tab) cycling inside the
          // dialog so background controls stay out of reach.
          const buttons = Array.from(
            dialogRef.current?.querySelectorAll<HTMLButtonElement>("button") ?? [],
          );
          if (buttons.length === 0) return;
          event.preventDefault();
          const active = buttons.indexOf(document.activeElement as HTMLButtonElement);
          const next = event.shiftKey
            ? (active + buttons.length - 1) % buttons.length
            : (active + 1) % buttons.length;
          buttons[next]?.focus();
        }
      }}
    >
      <h3 className={`${className}__title`} id={titleId}>
        {title}
      </h3>
      <p className={`${className}__body`} id={bodyId}>
        {body}
      </p>
      <div className={`${className}__actions`}>
        <button
          className="btn btn--quiet"
          type="button"
          ref={cancelRef}
          onClick={() => finish("cancel")}
        >
          {cancelLabel}
        </button>
        <button
          className={confirmClassName}
          type="button"
          onClick={() => finish("confirm")}
        >
          {confirmLabel}
        </button>
      </div>
    </div>
  );
}
