/**
 * The Symbol Properties dialog: edits a placed symbol's field values and
 * visibility, and adds or removes custom fields, as one undo step.
 */
import {
  type FormEvent,
  Fragment,
  useEffect,
  useId,
  useRef,
  useState,
} from "react";
import type { SymbolField } from "../bindings/SymbolField";
import {
  MAX_FIELD_NAME_CHARS,
  MAX_FIELD_VALUE_CHARS,
  fieldsError,
  isMandatoryField,
  sameFields,
} from "../symbolFields";
import { RemoveIcon } from "./icons";
import { Modal } from "./Modal";

interface FieldRow extends SymbolField {
  /** Stable across edits, so React keeps each row's inputs. */
  id: number;
  mandatory: boolean;
}

function toField({ name, value, visible }: FieldRow): SymbolField {
  return { name, value, visible };
}

interface SymbolPropertiesDialogProps {
  fields: SymbolField[];
  /** Resolves to `null` on success, or the error to show in the dialog. */
  onApply: (fields: SymbolField[]) => Promise<string | null>;
  onCancel: () => void;
}

export function SymbolPropertiesDialog({
  fields,
  onApply,
  onCancel,
}: SymbolPropertiesDialogProps) {
  const idPrefix = useId();
  const errorId = useId();
  const nextIdRef = useRef(fields.length);
  const focusIdRef = useRef<number | null>(null);
  const [rows, setRows] = useState<FieldRow[]>(() =>
    fields.map((field, index) => ({
      ...field,
      id: index,
      mandatory: isMandatoryField(field.name),
    })),
  );
  const [coreError, setCoreError] = useState<string | null>(null);
  const [submitting, setSubmitting] = useState(false);

  const edited = rows.map(toField);
  const changed = !sameFields(edited, fields);
  const problem = fieldsError(edited);
  const error = problem ?? coreError;

  // Focus and select the name of a field the user just added.
  useEffect(() => {
    if (focusIdRef.current === null) {
      return;
    }
    const input = document.getElementById(
      `${idPrefix}-name-${focusIdRef.current}`,
    );
    if (input instanceof HTMLInputElement) {
      input.focus();
      input.select();
    }
    focusIdRef.current = null;
  });

  const update = (id: number, change: Partial<SymbolField>) => {
    setRows((current) =>
      current.map((row) => (row.id === id ? { ...row, ...change } : row)),
    );
    setCoreError(null);
  };

  const addField = () => {
    const id = nextIdRef.current++;
    focusIdRef.current = id;
    setRows((current) => [
      ...current,
      {
        id,
        name: "Field",
        value: "",
        visible: true,
        mandatory: false,
      },
    ]);
    setCoreError(null);
  };

  const removeField = (id: number) => {
    setRows((current) => current.filter((row) => row.id !== id));
    setCoreError(null);
  };

  const submit = async (event: FormEvent<HTMLFormElement>) => {
    event.preventDefault();
    if (!changed || problem !== null || submitting) {
      return;
    }
    setSubmitting(true);
    const failure = await onApply(edited);
    if (failure) {
      setCoreError(failure);
      setSubmitting(false);
    }
  };

  return (
    <Modal title="Symbol Properties" onDismiss={onCancel}>
      <form
        className="form"
        onSubmit={(event) => void submit(event)}
        noValidate
      >
        <div className="field-grid" role="group" aria-label="Fields">
          <span className="field-grid__header">Name</span>
          <span className="field-grid__header">Value</span>
          <span className="field-grid__header">Show</span>
          <span aria-hidden="true" />
          {rows.map((row, index) => {
            const label = row.name.trim() === "" ? "custom field" : row.name;
            return (
              <Fragment key={row.id}>
                {row.mandatory ? (
                  <span className="field-grid__name">{row.name}</span>
                ) : (
                  <input
                    id={`${idPrefix}-name-${row.id}`}
                    className="input"
                    value={row.name}
                    maxLength={MAX_FIELD_NAME_CHARS}
                    spellCheck={false}
                    autoComplete="off"
                    aria-label={`Name of field ${index + 1}`}
                    onChange={(event) =>
                      update(row.id, { name: event.target.value })
                    }
                  />
                )}
                <input
                  className="input"
                  value={row.value}
                  maxLength={MAX_FIELD_VALUE_CHARS}
                  spellCheck={false}
                  autoComplete="off"
                  aria-label={`${label} value`}
                  aria-describedby={error ? errorId : undefined}
                  data-autofocus={row.name === "Reference" ? true : undefined}
                  onChange={(event) =>
                    update(row.id, { value: event.target.value })
                  }
                />
                <input
                  type="checkbox"
                  className="field-grid__show"
                  checked={row.visible}
                  aria-label={`Show ${label}`}
                  onChange={(event) =>
                    update(row.id, { visible: event.target.checked })
                  }
                />
                {row.mandatory ? (
                  <span aria-hidden="true" />
                ) : (
                  <button
                    type="button"
                    className="icon-button"
                    aria-label={`Remove ${label}`}
                    title={`Remove ${label}`}
                    onClick={() => removeField(row.id)}
                  >
                    <RemoveIcon />
                  </button>
                )}
              </Fragment>
            );
          })}
        </div>
        <div>
          <button type="button" className="button" onClick={addField}>
            Add Field
          </button>
        </div>
        {error && (
          <p id={errorId} className="form__error" role="alert">
            {error}
          </p>
        )}
        <div className="modal__actions">
          <button type="button" className="button" onClick={onCancel}>
            Cancel
          </button>
          <button
            type="submit"
            className="button button--primary"
            disabled={!changed || problem !== null || submitting}
          >
            Apply
          </button>
        </div>
      </form>
    </Modal>
  );
}
