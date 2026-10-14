import {
  type FormEvent,
  Fragment,
  type ReactNode,
  useEffect,
  useRef,
  useState,
} from "react";
import type { ErcRuleSetting } from "../bindings/ErcRuleSetting";
import type { ErcRuleSeverity } from "../bindings/ErcRuleSeverity";
import { type ErrorReport, describeIpcError, getErcRules } from "../ipc";
import { Modal } from "./Modal";

/** Only turned on or off: the pin conflict matrix decides each severity. */
const PIN_TO_PIN = "pin_to_pin";
const MATRIX = "matrix";

type Severities = Record<string, ErcRuleSeverity>;

type LoadState =
  | { status: "loading" }
  | { status: "error"; message: string }
  | { status: "ready"; rules: ErcRuleSetting[] };

function sameSetting(
  key: string,
  a: ErcRuleSeverity | undefined,
  b: ErcRuleSeverity | undefined,
): boolean {
  return key === PIN_TO_PIN ? (a === "ignore") === (b === "ignore") : a === b;
}

/** The settings in `draft` that differ from what the project has. */
function changedSettings(
  rules: ErcRuleSetting[],
  draft: Severities,
): Severities {
  const changed: Severities = {};
  for (const rule of rules) {
    const setting = draft[rule.key];
    if (setting && !sameSetting(rule.key, rule.severity, setting)) {
      changed[rule.key] = setting;
    }
  }
  return changed;
}

function settingsOf(
  rules: ErcRuleSetting[],
  field: "severity" | "defaultSeverity",
): Severities {
  const settings: Severities = {};
  for (const rule of rules) {
    settings[rule.key] = rule[field];
  }
  return settings;
}

interface ErcSeveritiesDialogProps {
  /** Resolves to `null` on success, or the error to show in the dialog. */
  onApply: (severities: Severities) => Promise<ErrorReport | null>;
  onCancel: () => void;
}

export function ErcSeveritiesDialog({
  onApply,
  onCancel,
}: ErcSeveritiesDialogProps) {
  const [load, setLoad] = useState<LoadState>({ status: "loading" });
  const [draft, setDraft] = useState<Severities>({});
  const [error, setError] = useState<ErrorReport | null>(null);
  const [submitting, setSubmitting] = useState(false);
  const firstSelect = useRef<HTMLSelectElement>(null);

  useEffect(() => {
    let active = true;
    getErcRules().then(
      (rules) => {
        if (active) {
          setDraft(settingsOf(rules, "severity"));
          setLoad({ status: "ready", rules });
        }
      },
      (failure: unknown) => {
        if (active) {
          setLoad({ status: "error", message: describeIpcError(failure) });
        }
      },
    );
    return () => {
      active = false;
    };
  }, []);

  useEffect(() => {
    if (load.status === "ready") {
      firstSelect.current?.focus();
    }
  }, [load.status]);

  const rules = load.status === "ready" ? load.rules : [];
  const changed = changedSettings(rules, draft);
  const dirty = Object.keys(changed).length > 0;

  const update = (rule: ErcRuleSetting, value: string) => {
    let setting: ErcRuleSeverity;
    if (value === MATRIX) {
      setting = rule.severity === "ignore" ? "warning" : rule.severity;
    } else if (value === "error" || value === "warning" || value === "ignore") {
      setting = value;
    } else {
      return;
    }
    setDraft((current) => ({ ...current, [rule.key]: setting }));
    setError(null);
  };

  const submit = async (event: FormEvent<HTMLFormElement>) => {
    event.preventDefault();
    if (submitting) {
      return;
    }
    if (!dirty) {
      onCancel();
      return;
    }
    setSubmitting(true);
    const failure = await onApply(changed);
    if (failure) {
      setError(failure);
      setSubmitting(false);
    }
  };

  let content: ReactNode;
  if (load.status === "loading") {
    content = (
      <>
        <p className="modal__message" role="status">Loading settings...</p>
        <div className="modal__actions">
          <button type="button" className="button" onClick={onCancel}>
            Cancel
          </button>
        </div>
      </>
    );
  } else if (load.status === "error") {
    content = (
      <>
        <p className="form__error" role="alert">
          Failed to load project settings: {load.message}
        </p>
        <div className="modal__actions">
          <button type="button" className="button" onClick={onCancel}>
            Close
          </button>
        </div>
      </>
    );
  } else {
    content = (
      <form
        className="form"
        onSubmit={(event) => void submit(event)}
        noValidate
      >
        <div className="field-grid field-grid--severities">
          <span className="field-grid__header">Rule</span>
          <span className="field-grid__header">ID</span>
          <span className="field-grid__header">Severity</span>
          {rules.map((rule, index) => {
            const setting = draft[rule.key] ?? rule.severity;
            const matrix = rule.key === PIN_TO_PIN;
            const value = matrix && setting !== "ignore" ? MATRIX : setting;
            return (
              <Fragment key={rule.key}>
                <span className="field-grid__name">{rule.message}</span>
                <span className="modal__detail">{rule.key}</span>
                <select
                  ref={index === 0 ? firstSelect : undefined}
                  className="input"
                  aria-label={`Severity for ${rule.message}`}
                  value={value}
                  onChange={(event) => update(rule, event.target.value)}
                >
                  {matrix ? (
                    <option value={MATRIX}>Use Pin Conflict Matrix</option>
                  ) : (
                    <>
                      <option value="error">Error</option>
                      <option value="warning">Warning</option>
                    </>
                  )}
                  <option value="ignore">Ignore</option>
                </select>
              </Fragment>
            );
          })}
        </div>
        {error && <p className="form__error" role="alert">{error.message}</p>}
        <div className="modal__actions">
          <button
            type="button"
            className="button modal__action--start"
            onClick={() => {
              setDraft(settingsOf(rules, "defaultSeverity"));
              setError(null);
            }}
          >
            Reset to Defaults
          </button>
          <button
            type="submit"
            className="button button--primary"
            disabled={!dirty || submitting}
          >
            Apply
          </button>
          <button type="button" className="button" onClick={onCancel}>
            Cancel
          </button>
        </div>
      </form>
    );
  }

  return (
    <Modal title="ERC Rule Severities" onDismiss={onCancel}>
      {content}
    </Modal>
  );
}
