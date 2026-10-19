import { type FormEvent, useId, useState } from "react";
import type { AcSweep } from "../bindings/AcSweep";
import type { AnalysisSettings } from "../bindings/AnalysisSettings";
import type { ErrorReport } from "../ipc";
import { formatSi, parseSi } from "../si";
import { Modal } from "./Modal";

type Kind = AnalysisSettings["kind"];
type TimeField = "step" | "stop" | "start" | "maxStep";
type AcField = "points" | "fstart" | "fstop";
type Field = TimeField | AcField;

interface Draft {
  kind: Kind;
  step: string;
  stop: string;
  start: string;
  maxStep: string;
  initialConditions: boolean;
  sweep: AcSweep;
  points: string;
  fstart: string;
  fstop: string;
}

const TIME_FIELDS: { field: TimeField; label: string }[] = [
  { field: "step", label: "Time step" },
  { field: "stop", label: "Final time" },
  { field: "start", label: "Initial time" },
  { field: "maxStep", label: "Maximum time step" },
];

const SWEEPS: { sweep: AcSweep; label: string; points: string }[] = [
  { sweep: "dec", label: "Decade", points: "per decade" },
  { sweep: "oct", label: "Octave", points: "per octave" },
  { sweep: "lin", label: "Linear", points: "total" },
];

const INVALID = "Invalid SI number.";
const POSITIVE = "Must be greater than 0.";
const NOT_NEGATIVE = "Must be greater than or equal to 0.";
const BEFORE_STOP = "Must be less than final time.";
const WITHIN_SPAN = "Must not exceed final time minus initial time.";
const WHOLE = "Must be a whole number.";
const AFTER_START = "Must be greater than start frequency.";
/** The most points `tron.json` can store. */
const MAX_POINTS = 2 ** 32 - 1;

const TRANSIENT_DEFAULTS = {
  step: "1u",
  stop: "1m",
  start: "0",
  maxStep: "",
  initialConditions: false,
};

const AC_DEFAULTS = {
  sweep: "dec" as AcSweep,
  points: "100",
  fstart: "1",
  fstop: "1Meg",
};

function draftOf(settings: AnalysisSettings | null): Draft {
  const draft: Draft = {
    kind: settings?.kind ?? "transient",
    ...TRANSIENT_DEFAULTS,
    ...AC_DEFAULTS,
  };
  if (settings?.kind === "transient") {
    return {
      ...draft,
      step: formatSi(settings.step),
      stop: formatSi(settings.stop),
      start: formatSi(settings.start),
      maxStep: settings.maxStep === null ? "" : formatSi(settings.maxStep),
      initialConditions: settings.initialConditions,
    };
  }
  if (settings?.kind === "ac") {
    return {
      ...draft,
      sweep: settings.sweep,
      points: String(settings.points),
      fstart: formatSi(settings.fstart),
      fstop: formatSi(settings.fstop),
    };
  }
  return draft;
}

type Errors = Record<Field, string | undefined>;

interface Checked {
  errors: Errors;
  settings: AnalysisSettings | null;
}

const NO_ERRORS: Errors = {
  step: undefined,
  stop: undefined,
  start: undefined,
  maxStep: undefined,
  points: undefined,
  fstart: undefined,
  fstop: undefined,
};

function kindOf(value: string): Kind {
  if (value === "operatingPoint" || value === "ac") {
    return value;
  }
  return "transient";
}

function sweepOf(value: string): AcSweep {
  return value === "oct" || value === "lin" ? value : "dec";
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

function pointsError(points: number | null): string | undefined {
  if (points === null) {
    return INVALID;
  }
  if (points <= 0) {
    return POSITIVE;
  }
  return Number.isInteger(points) && points <= MAX_POINTS ? undefined : WHOLE;
}

function stopFrequencyError(
  stop: number | null,
  start: number | null,
): string | undefined {
  if (stop === null) {
    return INVALID;
  }
  if (stop <= 0) {
    return POSITIVE;
  }
  return start !== null && stop <= start ? AFTER_START : undefined;
}

function failed(errors: Errors): boolean {
  return Object.values(errors).some((message) => message !== undefined);
}

function checkAc(draft: Draft): Checked {
  const points = parseSi(draft.points);
  const fstart = parseSi(draft.fstart);
  const fstop = parseSi(draft.fstop);
  const errors: Errors = {
    ...NO_ERRORS,
    points: pointsError(points),
    fstart: positiveError(fstart, null),
    fstop: stopFrequencyError(fstop, fstart),
  };
  if (failed(errors) || points === null || fstart === null || fstop === null) {
    return { errors, settings: null };
  }
  return {
    errors,
    settings: { kind: "ac", sweep: draft.sweep, points, fstart, fstop },
  };
}

/** The first failing check of each field, and the settings if none fail. */
function check(draft: Draft): Checked {
  if (draft.kind === "operatingPoint") {
    return { errors: NO_ERRORS, settings: { kind: "operatingPoint" } };
  }
  if (draft.kind === "ac") {
    return checkAc(draft);
  }
  const step = parseSi(draft.step);
  const stop = parseSi(draft.stop);
  const start = parseSi(draft.start);
  const optional = draft.maxStep.trim() === "";
  const maxStep = optional ? null : parseSi(draft.maxStep);
  const span = stop !== null && start !== null ? stop - start : null;
  const errors: Errors = {
    ...NO_ERRORS,
    step: positiveError(step, span),
    stop: positiveError(stop, null),
    start: startError(start, stop),
    maxStep: optional ? undefined : positiveError(maxStep, span),
  };
  if (failed(errors) || step === null || stop === null || start === null) {
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

interface TextFieldProps {
  id: string;
  label: string;
  value: string;
  message: string | undefined;
  placeholder?: string;
  onChange: (value: string) => void;
}

function TextField({
  id,
  label,
  value,
  message,
  placeholder,
  onChange,
}: TextFieldProps) {
  return (
    <div className="field field--inline">
      <label className="field__label" htmlFor={id}>
        {label}
      </label>
      <div className="field">
        <input
          id={id}
          className="input"
          value={value}
          placeholder={placeholder}
          spellCheck={false}
          autoComplete="off"
          aria-invalid={message ? true : undefined}
          aria-describedby={message ? `${id}-error` : undefined}
          onChange={(event) => onChange(event.target.value)}
        />
        {message && (
          <span id={`${id}-error`} className="form__error">
            {message}
          </span>
        )}
      </div>
    </div>
  );
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
  const pointsUnit =
    SWEEPS.find(({ sweep }) => sweep === draft.sweep)?.points ?? "total";

  const update = (changes: Partial<Draft>) => {
    setDraft((current) => ({ ...current, ...changes }));
    setError(null);
  };

  const updateField = (field: Field, value: string) => {
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
            <option value="ac">AC Sweep (.ac)</option>
          </select>
        </div>
        {draft.kind === "transient" && (
          <fieldset className="form__section">
            <legend className="form__legend">Transient</legend>
            {TIME_FIELDS.map(({ field, label }) => (
              <TextField
                key={field}
                id={`${idPrefix}-${field}`}
                label={label}
                value={draft[field]}
                message={checked.errors[field]}
                placeholder={field === "maxStep" ? "Optional" : undefined}
                onChange={(value) => updateField(field, value)}
              />
            ))}
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
        {draft.kind === "ac" && (
          <fieldset className="form__section">
            <legend className="form__legend">AC Sweep</legend>
            <div className="field field--inline">
              <label className="field__label" htmlFor={`${idPrefix}-sweep`}>
                Sweep type
              </label>
              <select
                id={`${idPrefix}-sweep`}
                className="input"
                value={draft.sweep}
                onChange={(event) =>
                  update({ sweep: sweepOf(event.target.value) })
                }
              >
                {SWEEPS.map(({ sweep, label }) => (
                  <option key={sweep} value={sweep}>
                    {label}
                  </option>
                ))}
              </select>
            </div>
            <TextField
              id={`${idPrefix}-points`}
              label={`Number of points (${pointsUnit})`}
              value={draft.points}
              message={checked.errors.points}
              onChange={(value) => updateField("points", value)}
            />
            <TextField
              id={`${idPrefix}-fstart`}
              label="Start frequency"
              value={draft.fstart}
              message={checked.errors.fstart}
              onChange={(value) => updateField("fstart", value)}
            />
            <TextField
              id={`${idPrefix}-fstop`}
              label="Stop frequency"
              value={draft.fstop}
              message={checked.errors.fstop}
              onChange={(value) => updateField("fstop", value)}
            />
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
