import { open } from "@tauri-apps/plugin-dialog";
import { type FormEvent, useId, useState } from "react";
import { type ErrorReport, toErrorReport } from "../ipc";
import { joinPath } from "../paths";
import { Modal } from "./Modal";

interface ProjectLocationDialogProps {
  mode: "new" | "saveAs";
  initialName: string;
  initialLocation: string;
  /** Resolves to `null` on success, or the error to show in the dialog. */
  onSubmit: (location: string, name: string) => Promise<ErrorReport | null>;
  onCancel: () => void;
}

export function ProjectLocationDialog({
  mode,
  initialName,
  initialLocation,
  onSubmit,
  onCancel,
}: ProjectLocationDialogProps) {
  const nameId = useId();
  const locationId = useId();
  const errorId = useId();
  const [name, setName] = useState(initialName);
  const [location, setLocation] = useState(initialLocation);
  const [error, setError] = useState<ErrorReport | null>(null);
  const [submitting, setSubmitting] = useState(false);
  const canSubmit = name.length > 0 && location.length > 0 && !submitting;

  const browse = async () => {
    try {
      const picked = await open({
        directory: true,
        title: "Choose Location",
        ...(location ? { defaultPath: location } : {}),
      });
      if (typeof picked === "string") {
        setLocation(picked);
        setError(null);
      }
    } catch (failure) {
      setError(toErrorReport(failure));
    }
  };

  const submit = async (event: FormEvent<HTMLFormElement>) => {
    event.preventDefault();
    if (!canSubmit) {
      return;
    }
    setSubmitting(true);
    const failure = await onSubmit(location, name);
    if (failure) {
      setError(failure);
      setSubmitting(false);
    }
  };

  return (
    <Modal
      title={mode === "new" ? "New Project" : "Save Project As"}
      onDismiss={onCancel}
    >
      <form
        className="form"
        onSubmit={(event) => void submit(event)}
        noValidate
      >
        <div className="field">
          <label className="field__label" htmlFor={nameId}>
            Name
          </label>
          <input
            id={nameId}
            className="input"
            value={name}
            maxLength={64}
            autoComplete="off"
            spellCheck={false}
            aria-invalid={error?.kind === "invalidName"}
            aria-describedby={error ? errorId : undefined}
            data-autofocus
            onChange={(event) => {
              setName(event.target.value);
              setError(null);
            }}
          />
        </div>
        <div className="field">
          <label className="field__label" htmlFor={locationId}>
            Location
          </label>
          <div className="field__row">
            <input
              id={locationId}
              className="input"
              value={location}
              placeholder="Choose a folder"
              readOnly
            />
            <button
              type="button"
              className="button"
              onClick={() => void browse()}
            >
              Browse...
            </button>
          </div>
        </div>
        {location && name && (
          <p className="field__hint">
            Creates the folder {joinPath(location, name)}
          </p>
        )}
        {error && (
          <p id={errorId} className="form__error" role="alert">
            {error.detail
              ? `${error.message} ${error.detail}`
              : error.message}
          </p>
        )}
        <div className="modal__actions">
          <button
            type="submit"
            className="button button--primary"
            disabled={!canSubmit}
          >
            {mode === "new" ? "Create" : "Save"}
          </button>
          <button type="button" className="button" onClick={onCancel}>
            Cancel
          </button>
        </div>
      </form>
    </Modal>
  );
}
