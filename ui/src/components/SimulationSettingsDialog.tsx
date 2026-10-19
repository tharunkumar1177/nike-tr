import { type FormEvent, useId, useState } from "react";
import type { AnalysisSettings } from "../bindings/AnalysisSettings";
import type { ErrorReport } from "../ipc";
import { formatSi, parseSi } from "../si";
import { Modal } from "./Modal";

type Kind = AnalysisSettings["kind"];
type TimeField = "step" | "stop" | "start" | "maxStep";

interface Draft {
  kind: Kind;
  step: string;
  stop: string;
  start: string;
  maxStep: string;
  initialConditions: boolean;
}

const TIME_FIELDS: { field: TimeField; label: string }[] = [
  { field: "step", label: "Time step" },
  { field: "stop", label: "Final time" },
  { field: "start", label: "Initial time" },
  { field: "maxStep", label: "Maximum time step" },
];

const INVALID = "Invalid SI number.";
const POSITIVE = "Must be greater than 0.";
const NOT_NEGATIVE = "Must be greater than or equal to 0.";
const BEFORE_STOP = "Must be less than final time.";
const WITHIN_SPAN = "Must not exceed final time minus initial time.";

function draftOf(settings: AnalysisSettings | null): Draft {
  if (settings?.kind === "transient") {
    return {
      kind: "transient",
      step: formatSi(settings.step),
      stop: formatSi(settings.stop),
      start: formatSi(settings.start),
      maxStep: settings.maxStep === null ? "" : formatSi(settings.maxStep),
      initialConditions: settings.initialConditions,
    };
  }
  return {
    kind: settings?.kind ?? "transient",
    step: "1u",
    stop: "1m",
    start: "0",
    maxStep: "",
    initialConditions: false,
  };
}

type Errors = Record<TimeField, string | undefined>;

interface Checked {
  errors: Errors;
  settings: AnalysisSettings | null;
}

const NO_ERRORS: Errors = {
  step: undefined,
  stop: undefined,
  start: undefined,
  maxStep: undefined,
};

function kindOf(value: string): Kind {
  return value === "operatingPoint" ? "operatingPoint" : "transient";
}

/** The first failing check for a time that must be positive and fit `span`. */
function positiveError(
  value: number | null,
  span: number | null,
): string | undefined {
  if (value === null) {
    return INVALID;
  }
  if (value <= 0) {
    return POSITIVE;
  }
  return span !== null && value > span ? WITHIN_SPAN : undefined;
}

function startError(
  start: number | null,
  stop: number | null,
): string | undefined {
  if (start === null) {
    return INVALID;
  }
  if (start < 0) {
    return NOT_NEGATIVE;
  }
  return stop !== null && start >= stop ? BEFORE_STOP : undefined;
}

/** The first failing check of each time field, and the settings if none fail. */
function check(draft: Draft): Checked {
  if (draft.kind === "operatingPoint") {
    return { errors: NO_ERRORS, settings: { kind: "operatingPoint" } };
  }
  const step = parseSi(draft.step);
  const stop = parseSi(draft.stop);
  const start = parseSi(draft.start);
  const optional = draft.maxStep.trim() === "";
  const maxStep = optional ? null : parseSi(draft.maxStep);
  const span = stop !== null && start !== null ? stop - start : null;
  const errors: Errors = {
    step: positiveError(step, span),
    stop: positiveError(stop, null),
    start: startError(start, stop),
    maxStep: optional ? undefined : positiveError(maxStep, span),
  };
  const failed = Object.values(errors).some((message) => message !== undefined);
  if (failed || step === null || stop === null || start === null) {
    return { errors, settings: null };
  }
  return {
    errors,
    settings: {
      kind: "transient",
      step,
      stop,
      start,
      maxStep,
      initialConditions: draft.initialConditions,
    },
  };
}

function sameSettings(
  a: AnalysisSettings | null,
  b: AnalysisSettings | null,
): boolean {
  return JSON.stringify(a) === JSON.stringify(b);
}

interface SimulationSettingsDialogProps {
  /** The analysis saved in the project, or `null` when none is set. */
  settings: AnalysisSettings | null;
  /** Resolves to `null` on success, or the error to show in the dialog. */
  onApply: (settings: AnalysisSettings) => Promise<ErrorReport | null>;
  onCancel: () => void;
}

export function SimulationSettingsDialog({
  settings,
  onApply,
  onCancel,
}: SimulationSettingsDialogProps) {
  const idPrefix = useId();
  const [draft, setDraft] = useState<Draft>(() => draftOf(settings));
  const [error, setError] = useState<ErrorReport | null>(null);
  const [submitting, setSubmitting] = useState(false);
  const checked = check(draft);

  const update = (changes: Partial<Draft>) => {
    setDraft((current) => ({ ...current, ...changes }));
    setError(null);
  };

  const updateTime = (field: TimeField, value: string) => {
    setDraft((current) => ({ ...current, [field]: value }));
    setError(null);
  };

  const submit = async (event: FormEvent<HTMLFormElement>) => {
    event.preventDefault();
    if (submitting || !checked.settings) {
      return;
    }
    if (sameSettings(checked.settings, settings)) {
      onCancel();
      return;
    }
    setSubmitting(true);
    const failure = await onApply(checked.settings);
    if (failure) {
      setError(failure);
      setSubmitting(false);
    }
  };

  return (
    <Modal title="Simulation Settings" onDismiss={onCancel}>
      <form
        className="form"
        onSubmit={(event) => void submit(event)}
        noValidate
      >
        <div className="field field--inline">
          <label className="field__label" htmlFor={`${idPrefix}-type`}>
            Analysis type
          </label>
          <select
            id={`${idPrefix}-type`}
            className="input"
            value={draft.kind}
            data-autofocus
            onChange={(event) => update({ kind: kindOf(event.target.value) })}
          >
            <option value="operatingPoint">Operating Point (.op)</option>
            <option value="transient">Transient (.tran)</option>
          </select>
        </div>
        {draft.kind === "transient" && (
          <fieldset className="form__section">
            <legend className="form__legend">Transient</legend>
            {TIME_FIELDS.map(({ field, label }) => {
              const id = `${idPrefix}-${field}`;
              const message = checked.errors[field];
              return (
                <div key={field} className="field field--inline">
                  <label className="field__label" htmlFor={id}>
                    {label}
                  </label>
                  <div className="field">
                    <input
                      id={id}
                      className="input"
                      value={draft[field]}
                      placeholder={field === "maxStep" ? "Optional" : undefined}
                      spellCheck={false}
                      autoComplete="off"
                      aria-invalid={message ? true : undefined}
                      aria-describedby={message ? `${id}-error` : undefined}
                      onChange={(event) =>
                        updateTime(field, event.target.value)
                      }
                    />
                    {message && (
                      <span id={`${id}-error`} className="form__error">
                        {message}
                      </span>
                    )}
                  </div>
                </div>
              );
            })}
            <div className="field field--inline">
              <label className="field__label" htmlFor={`${idPrefix}-uic`}>
                Start from initial conditions (UIC)
              </label>
              <input
                id={`${idPrefix}-uic`}
                type="checkbox"
                className="field-grid__show"
                checked={draft.initialConditions}
                onChange={(event) =>
                  update({ initialConditions: event.target.checked })
                }
              />
            </div>
          </fieldset>
        )}
        {error && (
          <p className="form__error" role="alert">
            {error.message}
          </p>
        )}
        <div className="modal__actions">
          <button
            type="submit"
            className="button button--primary"
            disabled={!checked.settings || submitting}
          >
            OK
          </button>
          <button type="button" className="button" onClick={onCancel}>
            Cancel
          </button>
        </div>
      </form>
    </Modal>
  );
}
