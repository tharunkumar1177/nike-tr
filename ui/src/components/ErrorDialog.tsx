import type { ErrorReport } from "../ipc";
import { Modal } from "./Modal";

interface ErrorDialogProps {
  title: string;
  error: ErrorReport;
  onClose: () => void;
  /** When set, the dialog offers to remove the project from the recent list. */
  onRemoveRecent?: (() => void) | undefined;
}

export function ErrorDialog({
  title,
  error,
  onClose,
  onRemoveRecent,
}: ErrorDialogProps) {
  return (
    <Modal title={title} onDismiss={onClose}>
      <p className="modal__message">{error.message}</p>
      {error.detail && <p className="modal__detail">{error.detail}</p>}
      {onRemoveRecent && (
        <p className="modal__message">Remove it from Recent Projects?</p>
      )}
      <div className="modal__actions">
        {onRemoveRecent ? (
          <>
            <button
              type="button"
              className="button button--primary"
              data-autofocus
              onClick={onRemoveRecent}
            >
              Remove
            </button>
            <button type="button" className="button" onClick={onClose}>
              Cancel
            </button>
          </>
        ) : (
          <button
            type="button"
            className="button button--primary"
            data-autofocus
            onClick={onClose}
          >
            OK
          </button>
        )}
      </div>
    </Modal>
  );
}
