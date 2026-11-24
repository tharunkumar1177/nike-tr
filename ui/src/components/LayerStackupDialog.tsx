/**
 * The Layer Stackup dialog: picks the board's copper layer count and previews
 * the KiCad default stackup the core writes for it, then applies the change
 * as one undo step.
 */
import { type FormEvent, useId, useState } from "react";
import type { BoardStackup } from "../bindings/BoardStackup";
import { COPPER_LAYER_COUNTS, stackupPreview } from "../boardSetup";
import type { ErrorReport } from "../ipc";
import { formatMm } from "../viewport";
import { Modal } from "./Modal";

/** A board without a layer table starts the dialog at two layers. */
const DEFAULT_COPPER_LAYERS = 2;
const NO_VALUE = "—";

interface LayerStackupDialogProps {
  stackup: BoardStackup;
  /** Resolves to `null` on success, or the error to show in the dialog. */
  onApply: (copperLayers: number) => Promise<ErrorReport | null>;
  onCancel: () => void;
}

export function LayerStackupDialog({
  stackup,
  onApply,
  onCancel,
}: LayerStackupDialogProps) {
  const selectId = useId();
  const current = COPPER_LAYER_COUNTS.includes(stackup.copperLayers)
    ? stackup.copperLayers
    : null;
  const [copper, setCopper] = useState(current ?? DEFAULT_COPPER_LAYERS);
  const [error, setError] = useState<ErrorReport | null>(null);
  const [submitting, setSubmitting] = useState(false);

  const rows = stackupPreview(copper);
  const total = rows.reduce((sum, row) => sum + row.thicknessNm, 0);
  const changed = copper !== stackup.copperLayers;
  const options =
    current === null && stackup.copperLayers > 0
      ? [stackup.copperLayers, ...COPPER_LAYER_COUNTS]
      : COPPER_LAYER_COUNTS;

  const submit = async (event: FormEvent<HTMLFormElement>) => {
    event.preventDefault();
    if (!changed || submitting) {
      return;
    }
    setSubmitting(true);
    const failure = await onApply(copper);
    if (failure) {
      setError(failure);
      setSubmitting(false);
    }
  };

  return (
    <Modal title="Layer Stackup" onDismiss={onCancel}>
      <form
        className="form"
        onSubmit={(event) => void submit(event)}
        noValidate
      >
        <div className="field field--inline">
          <label className="field__label" htmlFor={selectId}>
            Copper layers
          </label>
          <select
            id={selectId}
            className="input"
            value={copper}
            data-autofocus
            onChange={(event) => {
              setCopper(Number(event.target.value));
              setError(null);
            }}
          >
            {options.map((count) => (
              <option
                key={count}
                value={count}
                disabled={!COPPER_LAYER_COUNTS.includes(count)}
              >
                {count} layers
              </option>
            ))}
          </select>
        </div>
        <div className="stackup-preview">
          <table className="stackup-table">
            <caption className="waveform__caption">Resulting stackup</caption>
            <thead>
              <tr>
                <th scope="col">Layer</th>
                <th scope="col">Type</th>
                <th scope="col">Material</th>
                <th scope="col">Thickness (mm)</th>
                <th scope="col">εr</th>
                <th scope="col">Loss tan</th>
              </tr>
            </thead>
            <tbody>
              {rows.map((row) => (
                <tr key={row.layer}>
                  <th scope="row">{row.layer}</th>
                  <td>{row.type}</td>
                  <td>{row.material ?? NO_VALUE}</td>
                  <td>{formatMm(row.thicknessNm)}</td>
                  <td>{row.epsilonR ?? NO_VALUE}</td>
                  <td>{row.lossTangent ?? NO_VALUE}</td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
        <output className="stackup-total">
          Total thickness: {formatMm(total, 3)} mm
        </output>
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
            disabled={!changed || submitting}
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
