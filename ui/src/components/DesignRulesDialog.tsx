/**
 * The Design Rules dialog: edits the board's clearance and minimum sizes,
 * which DRC checks, and the track and via sizes new routes get, in
 * millimeters, then applies them to the `.kicad_pro` as one undo step.
 */
import { type FormEvent, useId, useState } from "react";
import type { BoardDesignRules } from "../bindings/BoardDesignRules";
import { parseMm } from "../boardSetup";
import type { ErrorReport } from "../ipc";
import { NM_PER_MM, formatMm } from "../viewport";
import { Modal } from "./Modal";

/** The largest rule the core accepts: 25 mm. */
const MAX_RULE_NM = 25 * NM_PER_MM;
const INVALID_NUMBER = "Invalid millimeter number.";
const NOT_POSITIVE = "Must be greater than 0.";
const TOO_LARGE = "Must be at most 25 mm.";
const DRILL_TOO_LARGE = "Via drill must be smaller than via diameter.";
const MIN_DRILL_TOO_LARGE =
  "Minimum via drill must be smaller than minimum via diameter.";

type RuleKey = keyof BoardDesignRules;

/** One editable rule: its key and its label. */
interface RuleField {
  key: RuleKey;
  label: string;
}

const CONSTRAINTS: RuleField[] = [
  { key: "clearanceNm", label: "Clearance (mm)" },
  { key: "minTrackWidthNm", label: "Minimum track width (mm)" },
  { key: "minViaDiameterNm", label: "Minimum via diameter (mm)" },
  { key: "minViaDrillNm", label: "Minimum via drill (mm)" },
  { key: "minAnnularRingNm", label: "Minimum annular ring (mm)" },
  { key: "edgeClearanceNm", label: "Copper-to-edge clearance (mm)" },
];

const DEFAULTS: RuleField[] = [
  { key: "trackWidthNm", label: "Default track width (mm)" },
  { key: "viaDiameterNm", label: "Default via diameter (mm)" },
  { key: "viaDrillNm", label: "Default via drill (mm)" },
];

const RULE_FIELDS = [...CONSTRAINTS, ...DEFAULTS];
const RULE_KEYS = RULE_FIELDS.map((field) => field.key);

type RuleTexts = Record<RuleKey, string>;
type RuleMessages = Partial<Record<RuleKey, string>>;

/** One typed rule: its length, or why the text isn't one. */
interface ReadRule {
  nm: number | null;
  message: string | null;
}

/** The typed rules, `null` while any field has a message, and the messages. */
interface ReadRules {
  rules: BoardDesignRules | null;
  messages: RuleMessages;
}

/** Reads one rule: a length in (0, 25] mm, or why the text isn't one. */
function readRule(text: string): ReadRule {
  const nm = parseMm(text);
  if (nm === null) {
    return { nm: null, message: INVALID_NUMBER };
  }
  if (nm <= 0) {
    return { nm: null, message: NOT_POSITIVE };
  }
  if (nm > MAX_RULE_NM) {
    return { nm: null, message: TOO_LARGE };
  }
  return { nm, message: null };
}

/** The rules as typed text, in millimeters. */
function ruleTexts(rules: BoardDesignRules): RuleTexts {
  const texts = {} as RuleTexts;
  for (const key of RULE_KEYS) {
    texts[key] = formatMm(rules[key]);
  }
  return texts;
}

/**
 * The typed rules and the message for each field that doesn't read, or that
 * breaks a rule between fields.
 */
function readRules(texts: RuleTexts): ReadRules {
  const messages: RuleMessages = {};
  const rules = {} as BoardDesignRules;
  for (const key of RULE_KEYS) {
    const { nm, message } = readRule(texts[key]);
    if (message !== null) {
      messages[key] = message;
    }
    rules[key] = nm ?? 0;
  }
  if (!messages.viaDrillNm && rules.viaDrillNm >= rules.viaDiameterNm) {
    messages.viaDrillNm = DRILL_TOO_LARGE;
  }
  if (
    !messages.minViaDrillNm &&
    rules.minViaDrillNm >= rules.minViaDiameterNm
  ) {
    messages.minViaDrillNm = MIN_DRILL_TOO_LARGE;
  }
  const valid = Object.keys(messages).length === 0;
  return { rules: valid ? rules : null, messages };
}

interface DesignRulesDialogProps {
  rules: BoardDesignRules;
  /** Resolves to `null` on success, or the error to show in the dialog. */
  onApply: (rules: BoardDesignRules) => Promise<ErrorReport | null>;
  onCancel: () => void;
}

/** The Design Rules dialog, opened on the board's current rules. */
export function DesignRulesDialog({
  rules: initial,
  onApply,
  onCancel,
}: DesignRulesDialogProps) {
  const idPrefix = useId();
  const [texts, setTexts] = useState<RuleTexts>(() => ruleTexts(initial));
  const [error, setError] = useState<ErrorReport | null>(null);
  const [submitting, setSubmitting] = useState(false);

  const { rules, messages } = readRules(texts);
  const changed =
    rules !== null && RULE_KEYS.some((key) => rules[key] !== initial[key]);

  const submit = async (event: FormEvent<HTMLFormElement>) => {
    event.preventDefault();
    if (rules === null || !changed || submitting) {
      return;
    }
    setSubmitting(true);
    const failure = await onApply(rules);
    if (failure) {
      setError(failure);
      setSubmitting(false);
    }
  };

  /** A labelled millimeter input with its validation message. */
  const ruleInput = ({ key, label }: RuleField, first: boolean) => {
    const id = `${idPrefix}-${key}`;
    const message = messages[key];
    return (
      <div key={key} className="field field--inline field--wide-label">
        <label className="field__label" htmlFor={id}>
          {label}
        </label>
        <div className="field">
          <input
            id={id}
            className="input"
            value={texts[key]}
            inputMode="decimal"
            spellCheck={false}
            autoComplete="off"
            data-autofocus={first ? true : undefined}
            aria-invalid={message ? true : undefined}
            aria-describedby={message ? `${id}-error` : undefined}
            onChange={(event) => {
              setTexts({ ...texts, [key]: event.target.value });
              setError(null);
            }}
          />
          {message && (
            <span id={`${id}-error`} className="form__error">
              {message}
            </span>
          )}
        </div>
      </div>
    );
  };

  return (
    <Modal title="Design Rules" onDismiss={onCancel}>
      <form
        className="form"
        onSubmit={(event) => void submit(event)}
        noValidate
      >
        <fieldset className="form__section">
          <legend className="form__legend">Constraints</legend>
          {CONSTRAINTS.map((field, index) => ruleInput(field, index === 0))}
        </fieldset>
        <fieldset className="form__section">
          <legend className="form__legend">Routing defaults</legend>
          {DEFAULTS.map((field) => ruleInput(field, false))}
        </fieldset>
        {error && (
          <div role="alert">
            <p className="form__error">{error.message}</p>
            {error.detail && <p className="modal__detail">{error.detail}</p>}
          </div>
        )}
        <div className="modal__actions">
          <button
            type="submit"
            className="button button--primary"
            disabled={rules === null || !changed || submitting}
          >
            Apply
          </button>
          <button type="button" className="button" onClick={onCancel}>
            Cancel
          </button>
        </div>
      </form>
    </Modal>
  );
}
