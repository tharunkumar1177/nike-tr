import { useEffect, useState } from "react";
import type { AppError } from "../bindings/AppError";
import type { LibraryStatus } from "../bindings/LibraryStatus";
import {
  type ErrorReport,
  cancelLibraryDownload,
  declineLibraryDownload,
  downloadLibrary,
  toErrorReport,
} from "../ipc";
import { formatMegabytes } from "../useLibraryStatus";
import { Modal } from "./Modal";

interface LibraryDialogProps {
  library: LibraryStatus;
  onStatus: (library: LibraryStatus) => void;
  onClose: () => void;
}

function failureSummary(error: AppError): string | null {
  switch (error.kind) {
    case "network":
    case "io":
      return "A network or disk error occurred.";
    case "checksum":
      return "The downloaded file was corrupted or tampered with.";
    default:
      return null;
  }
}

/** Escape can't close the dialog while a download or install is running. */
const stayOpen = () => undefined;

/** Announces download progress in steps of 10% so screen readers aren't flooded. */
function progressAnnouncement(downloaded: number, total: number): string {
  const percent = total > 0 ? Math.floor((downloaded / total) * 10) * 10 : 0;
  return `Downloaded ${percent}%`;
}

export function LibraryDialog({
  library,
  onStatus,
  onClose,
}: LibraryDialogProps) {
  const [commandError, setCommandError] = useState<ErrorReport | null>(null);
  const [pending, setPending] = useState(false);

  useEffect(() => {
    if (library.state === "ready") {
      onClose();
    }
  }, [library.state, onClose]);

  async function run(command: () => Promise<LibraryStatus>) {
    setPending(true);
    setCommandError(null);
    try {
      onStatus(await command());
      return true;
    } catch (error) {
      setCommandError(toErrorReport(error));
      return false;
    } finally {
      setPending(false);
    }
  }

  const commandErrorView = commandError && (
    <>
      <p className="form__error" role="alert">
        {commandError.message}
      </p>
      {commandError.detail && (
        <p className="modal__detail">{commandError.detail}</p>
      )}
    </>
  );

  switch (library.state) {
    case "downloading": {
      const { downloadedBytes, totalBytes } = library;
      const fraction = totalBytes > 0 ? downloadedBytes / totalBytes : 0;
      return (
        <Modal
          key="downloading"
          title="Downloading Library..."
          onDismiss={stayOpen}
        >
          <p className="modal__message">
            Downloading component index... ({formatMegabytes(downloadedBytes)}{" "}
            MB / {formatMegabytes(totalBytes)} MB)
          </p>
          <div
            className="library-progress"
            role="progressbar"
            aria-label="Download progress"
            aria-valuemin={0}
            aria-valuemax={totalBytes}
            aria-valuenow={downloadedBytes}
          >
            <div
              className="library-progress__fill"
              style={{ width: `${Math.min(fraction, 1) * 100}%` }}
            />
          </div>
          <p className="visually-hidden" aria-live="polite">
            {progressAnnouncement(downloadedBytes, totalBytes)}
          </p>
          {commandErrorView}
          <div className="modal__actions">
            <button
              type="button"
              className="button"
              disabled={pending}
              data-autofocus
              onClick={() => void run(cancelLibraryDownload)}
            >
              Cancel
            </button>
          </div>
        </Modal>
      );
    }
    case "installing":
      return (
        <Modal
          key="installing"
          title="Installing Library..."
          onDismiss={stayOpen}
        >
          <p className="modal__message">
            Verifying and installing the {library.kicadTag} library. This may
            take a moment.
          </p>
          <div
            className="library-progress library-progress--indeterminate"
            role="progressbar"
            aria-label="Installing"
          >
            <div className="library-progress__fill" />
          </div>
          <div className="modal__actions">
            <button type="button" className="button" disabled>
              Cancel
            </button>
          </div>
        </Modal>
      );
    case "missing": {
      if (library.error) {
        const summary = failureSummary(library.error);
        return (
          <Modal key="failed" title="Download Failed" onDismiss={onClose}>
            {summary && <p className="modal__message">{summary}</p>}
            <p className="form__error" role="alert">
              {library.error.message}
            </p>
            {library.error.detail && (
              <p className="modal__detail">{library.error.detail}</p>
            )}
            {commandErrorView}
            <div className="modal__actions">
              <button
                type="button"
                className="button"
                disabled={pending}
                onClick={onClose}
              >
                Close
              </button>
              <button
                type="button"
                className="button button--primary"
                disabled={pending}
                data-autofocus
                onClick={() => void run(downloadLibrary)}
              >
                Retry
              </button>
            </div>
          </Modal>
        );
      }
      return (
        <Modal key="offer" title="Component Library" onDismiss={onClose}>
          <p className="modal__message">
            To place symbols and footprints, Tron uses the open KiCad component
            library. It is downloaded once and then used offline.
          </p>
          {library.resumeBytes > 0 && (
            <p className="modal__message">
              Resume download ({formatMegabytes(library.resumeBytes)} MB of{" "}
              {formatMegabytes(library.sizeBytes)} MB).
            </p>
          )}
          {commandErrorView}
          <div className="modal__actions">
            <button
              type="button"
              className="button"
              disabled={pending}
              onClick={() => {
                void run(declineLibraryDownload).then((declined) => {
                  if (declined) {
                    onClose();
                  }
                });
              }}
            >
              Decline
            </button>
            <button
              type="button"
              className="button button--primary"
              disabled={pending}
              data-autofocus
              onClick={() => void run(downloadLibrary)}
            >
              Download
            </button>
          </div>
        </Modal>
      );
    }
    case "ready":
      return null;
  }
}
