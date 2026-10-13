/**
 * The popover that edits a placed symbol's Reference or Value in place, over
 * the property's text on the canvas.
 */
import {
  type FormEvent,
  type KeyboardEvent,
  useEffect,
  useId,
  useRef,
  useState,
} from "react";
import { MAX_FIELD_VALUE_CHARS } from "../symbolFields";
import type { ScreenPoint } from "../viewport";

interface InlinePropertyEditorProps {
  field: "Reference" | "Value";
  /** Where the popover opens, in pixels from the canvas's top left. */
  at: ScreenPoint;
  initialValue: string;
  /** Resolves to `null` on success, or the error to show in the popover. */
  onSubmit: (value: string) => Promise<string | null>;
  onCancel: () => void;
}

export function InlinePropertyEditor({
  field,
  at,
  initialValue,
  onSubmit,
  onCancel,
}: InlinePropertyEditorProps) {
  const id = useId();
  const inputRef = useRef<HTMLInputElement>(null);
  const [value, setValue] = useState(initialValue);
  const [error, setError] = useState<string | null>(null);
  const [submitting, setSubmitting] = useState(false);

  useEffect(() => {
    inputRef.current?.focus();
    inputRef.current?.select();
  }, []);

  const submit = async (event: FormEvent<HTMLFormElement>) => {
    event.preventDefault();
    if (submitting) {
      return;
    }
    if (field === "Reference" && value.trim() === "") {
      setError("Reference cannot be empty.");
      inputRef.current?.focus();
      return;
    }
    setSubmitting(true);
    const failure = await onSubmit(value);
    if (failure) {
      setError(failure);
      setSubmitting(false);
      inputRef.current?.focus();
    }
  };

  const handleKeyDown = (event: KeyboardEvent<HTMLFormElement>) => {
    if (event.key === "Escape") {
      event.preventDefault();
      event.stopPropagation();
      onCancel();
    }
  };

  const title = `Edit ${field}`;
  return (
    <form
      className="canvas-popover"
      style={{ left: at.x, top: at.y }}
      aria-label={title}
      onSubmit={(event) => void submit(event)}
      onKeyDown={handleKeyDown}
    >
      <div className="field">
        <label className="field__label" htmlFor={`${id}-value`}>
          {title}
        </label>
        <input
          ref={inputRef}
          id={`${id}-value`}
          className="input"
          value={value}
          maxLength={MAX_FIELD_VALUE_CHARS}
          spellCheck={false}
          autoComplete="off"
          aria-invalid={error !== null}
          aria-describedby={error ? `${id}-error` : undefined}
          onChange={(event) => {
            setValue(event.target.value);
            setError(null);
          }}
        />
      </div>
      {error && (
        <p id={`${id}-error`} className="form__error" role="alert">
          {error}
        </p>
      )}
      <div className="canvas-popover__actions">
        <button type="button" className="button" onClick={onCancel}>
          Cancel
        </button>
        <button
          type="submit"
          className="button button--primary"
          disabled={submitting}
        >
          Apply
        </button>
      </div>
    </form>
  );
}
