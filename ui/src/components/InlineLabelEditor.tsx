/**
 * The popover that asks for a net label's name (and, for global labels, its
 * shape) before the label follows the cursor for placement.
 */
import {
  type FormEvent,
  type KeyboardEvent,
  useEffect,
  useId,
  useRef,
  useState,
} from "react";
import type { GlobalLabelShape } from "../bindings/GlobalLabelShape";
import type { ScreenPoint } from "../viewport";
import { MAX_LABEL_CHARS, labelTextError } from "../wiring";

const SHAPES: { value: GlobalLabelShape; label: string }[] = [
  { value: "input", label: "Input" },
  { value: "output", label: "Output" },
  { value: "bidirectional", label: "Bidirectional" },
  { value: "triState", label: "Tri-state" },
  { value: "passive", label: "Passive" },
];

interface InlineLabelEditorProps {
  global: boolean;
  /** Where the popover opens, in pixels from the canvas's top left. */
  at: ScreenPoint;
  initialText: string;
  initialShape: GlobalLabelShape;
  onSubmit: (text: string, shape: GlobalLabelShape | null) => void;
  onCancel: () => void;
}

export function InlineLabelEditor({
  global,
  at,
  initialText,
  initialShape,
  onSubmit,
  onCancel,
}: InlineLabelEditorProps) {
  const id = useId();
  const inputRef = useRef<HTMLInputElement>(null);
  const [text, setText] = useState(initialText);
  const [shape, setShape] = useState<GlobalLabelShape>(initialShape);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    inputRef.current?.focus();
    inputRef.current?.select();
  }, []);

  const submit = (event: FormEvent<HTMLFormElement>) => {
    event.preventDefault();
    const problem = labelTextError(text);
    if (problem) {
      setError(problem);
      inputRef.current?.focus();
      return;
    }
    onSubmit(text, global ? shape : null);
  };

  const handleKeyDown = (event: KeyboardEvent<HTMLFormElement>) => {
    if (event.key === "Escape") {
      event.preventDefault();
      event.stopPropagation();
      onCancel();
    }
  };

  const title = global ? "Global label" : "Local label";
  return (
    <form
      className="canvas-popover"
      style={{ left: at.x, top: at.y }}
      aria-label={title}
      onSubmit={submit}
      onKeyDown={handleKeyDown}
    >
      <div className="field">
        <label className="field__label" htmlFor={`${id}-name`}>
          {title} name
        </label>
        <input
          ref={inputRef}
          id={`${id}-name`}
          className="input"
          value={text}
          maxLength={MAX_LABEL_CHARS}
          spellCheck={false}
          autoComplete="off"
          aria-invalid={error !== null}
          aria-describedby={error ? `${id}-error` : undefined}
          onChange={(event) => {
            setText(event.target.value);
            setError(null);
          }}
        />
      </div>
      {global && (
        <div className="field">
          <label className="field__label" htmlFor={`${id}-shape`}>
            Shape
          </label>
          <select
            id={`${id}-shape`}
            className="input"
            value={shape}
            onChange={(event) =>
              setShape(event.target.value as GlobalLabelShape)
            }
          >
            {SHAPES.map((option) => (
              <option key={option.value} value={option.value}>
                {option.label}
              </option>
            ))}
          </select>
        </div>
      )}
      {error && (
        <p id={`${id}-error`} className="form__error" role="alert">
          {error}
        </p>
      )}
      <div className="canvas-popover__actions">
        <button type="button" className="button" onClick={onCancel}>
          Cancel
        </button>
        <button type="submit" className="button button--primary">
          Place
        </button>
      </div>
    </form>
  );
}
