/**
 * The Board Outline dialog: edits the board's `Edge.Cuts` outline as a
 * rectangle or as a polygon of typed corners, checks it the way the core
 * does, and applies it as one closed polygon in one undo step.
 */
import { type FormEvent, Fragment, useId, useRef, useState } from "react";
import type { BoardFootprint } from "../bindings/BoardFootprint";
import type { OutlineGraphic } from "../bindings/OutlineGraphic";
import type { Xy } from "../bindings/Xy";
import {
  MAX_OUTLINE_POINTS,
  outlineDraft,
  parseMm,
  polygonProblem,
  rectangleCorners,
} from "../boardSetup";
import type { ErrorReport } from "../ipc";
import { formatMm } from "../viewport";
import { RemoveIcon } from "./icons";
import { Modal } from "./Modal";

const INVALID_NUMBER = "Invalid millimeter number.";
const NOT_POSITIVE = "Must be greater than 0.";
const REPLACES_CURVES =
  "The current outline has arcs, circles, or separate parts. Applying replaces the whole outline with the shape below.";
/** A polygon needs this many corners, so rows stop being removable here. */
const MIN_VERTICES = 3;

type OutlineMode = "rectangle" | "polygon";

/** One editable corner of the polygon, as typed. */
interface VertexRow {
  id: number;
  x: string;
  y: string;
}

/** A rectangle size: its length, or why the typed text isn't one. */
interface SizeField {
  nm: number | null;
  message: string | undefined;
}

/** Reads a rectangle size: a positive length in millimeters. */
function sizeField(text: string): SizeField {
  const nm = parseMm(text);
  if (nm === null) {
    return { nm: null, message: INVALID_NUMBER };
  }
  return nm > 0
    ? { nm, message: undefined }
    : { nm: null, message: NOT_POSITIVE };
}

/** Whether two corner lists are the same, point by point. */
function samePoints(a: Xy[], b: Xy[]): boolean {
  return (
    a.length === b.length &&
    a.every((point, index) => {
      const other = b[index];
      return (
        other !== undefined && point[0] === other[0] && point[1] === other[1]
      );
    })
  );
}

/**
 * The coordinate halfway between two corners, treating unreadable ones as 0.
 * A corner added halfway along the closing edge keeps the outline simple.
 */
function halfway(
  a: number | null | undefined,
  b: number | null | undefined,
): number {
  return Math.round(((a ?? 0) + (b ?? 0)) / 2);
}

/** How many vertices a polygon has, for announcements. */
function vertexCount(count: number): string {
  return count === 1 ? "1 vertex" : `${count} vertices`;
}

interface BoardOutlineDialogProps {
  outline: OutlineGraphic[];
  footprints: BoardFootprint[];
  /** Resolves to `null` on success, or the error to show in the dialog. */
  onApply: (points: Xy[]) => Promise<ErrorReport | null>;
  onCancel: () => void;
}

export function BoardOutlineDialog({
  outline,
  footprints,
  onApply,
  onCancel,
}: BoardOutlineDialogProps) {
  const idPrefix = useId();
  const nextRowId = useRef(0);
  const [initial] = useState(() => outlineDraft(outline, footprints));
  /** Gives typed corners rows with stable keys. */
  const toRows = (points: Xy[]): VertexRow[] =>
    points.map(([x, y]) => ({
      id: nextRowId.current++,
      x: formatMm(x),
      y: formatMm(y),
    }));
  const [mode, setMode] = useState<OutlineMode>(initial.mode);
  const [origin, setOrigin] = useState<Xy>(initial.origin);
  const [widthText, setWidthText] = useState(formatMm(initial.width));
  const [heightText, setHeightText] = useState(formatMm(initial.height));
  const [rows, setRows] = useState<VertexRow[]>(() => toRows(initial.points));
  const [error, setError] = useState<ErrorReport | null>(null);
  const [submitting, setSubmitting] = useState(false);
  const [announcement, setAnnouncement] = useState("");

  const width = sizeField(widthText);
  const height = sizeField(heightText);
  const parsedRows = rows.map((row) => ({
    x: parseMm(row.x),
    y: parseMm(row.y),
  }));
  const polygon: Xy[] = [];
  for (const { x, y } of parsedRows) {
    if (x !== null && y !== null) {
      polygon.push([x, y]);
    }
  }
  const unreadable = polygon.length !== rows.length;

  let points: Xy[] | null = null;
  let polygonMessage: string | null = null;
  if (mode === "rectangle") {
    if (width.nm !== null && height.nm !== null) {
      points = rectangleCorners(origin, width.nm, height.nm);
    }
  } else if (unreadable) {
    polygonMessage = INVALID_NUMBER;
  } else {
    polygonMessage = polygonProblem(polygon);
    points = polygonMessage === null ? polygon : null;
  }
  const changed =
    initial.replacesCurves ||
    outline.length === 0 ||
    (points !== null && !samePoints(points, initial.points));

  const switchMode = (next: OutlineMode) => {
    if (next === mode) {
      return;
    }
    if (next === "polygon" && points !== null) {
      setRows(toRows(points));
    }
    if (next === "rectangle" && points !== null && points.length > 0) {
      const xs = points.map(([x]) => x);
      const ys = points.map(([, y]) => y);
      const minX = Math.min(...xs);
      const minY = Math.min(...ys);
      setOrigin([minX, minY]);
      setWidthText(formatMm(Math.max(...xs) - minX));
      setHeightText(formatMm(Math.max(...ys) - minY));
    }
    setMode(next);
    setError(null);
    const count =
      next === "polygon" && points !== null ? points.length : rows.length;
    setAnnouncement(
      next === "rectangle"
        ? "Rectangle mode"
        : `Custom polygon mode, ${vertexCount(count)}`,
    );
  };

  const updateRow = (id: number, axis: "x" | "y", value: string) => {
    setRows((current) =>
      current.map((row) => (row.id === id ? { ...row, [axis]: value } : row)),
    );
    setError(null);
  };

  const addVertex = () => {
    const last = parsedRows[parsedRows.length - 1];
    const first = parsedRows[0];
    const x = halfway(last?.x, first?.x);
    const y = halfway(last?.y, first?.y);
    const row = { id: nextRowId.current++, x: formatMm(x), y: formatMm(y) };
    setRows([...rows, row]);
    setError(null);
    setAnnouncement(`Vertex added, ${vertexCount(rows.length + 1)}`);
  };

  const removeVertex = (id: number) => {
    setRows(rows.filter((row) => row.id !== id));
    setError(null);
    setAnnouncement(`Vertex removed, ${vertexCount(rows.length - 1)}`);
  };

  const submit = async (event: FormEvent<HTMLFormElement>) => {
    event.preventDefault();
    if (points === null || !changed || submitting) {
      return;
    }
    setSubmitting(true);
    const failure = await onApply(points);
    if (failure) {
      setError(failure);
      setSubmitting(false);
    }
  };

  const polygonErrorId = `${idPrefix}-polygon-error`;
  /** A labelled rectangle size input with its validation message. */
  const sizeInput = (
    key: "width" | "height",
    label: string,
    text: string,
    onChange: (value: string) => void,
  ) => {
    const id = `${idPrefix}-${key}`;
    const field = sizeField(text);
    return (
      <div className="field field--inline">
        <label className="field__label" htmlFor={id}>
          {label}
        </label>
        <div className="field">
          <input
            id={id}
            className="input"
            value={text}
            inputMode="decimal"
            spellCheck={false}
            autoComplete="off"
            aria-invalid={field.message ? true : undefined}
            aria-describedby={field.message ? `${id}-error` : undefined}
            onChange={(event) => {
              onChange(event.target.value);
              setError(null);
            }}
          />
          {field.message && (
            <span id={`${id}-error`} className="form__error">
              {field.message}
            </span>
          )}
        </div>
      </div>
    );
  };

  return (
    <Modal title="Board Outline" onDismiss={onCancel}>
      <form
        className="form"
        onSubmit={(event) => void submit(event)}
        noValidate
      >
        <fieldset className="form__section">
          <legend className="form__legend">Shape</legend>
          <label className="board-update__option">
            <input
              type="radio"
              name={`${idPrefix}-mode`}
              checked={mode === "rectangle"}
              data-autofocus={mode === "rectangle" ? true : undefined}
              onChange={() => switchMode("rectangle")}
            />
            Rectangle
          </label>
          <label className="board-update__option">
            <input
              type="radio"
              name={`${idPrefix}-mode`}
              checked={mode === "polygon"}
              data-autofocus={mode === "polygon" ? true : undefined}
              onChange={() => switchMode("polygon")}
            />
            Custom polygon
          </label>
        </fieldset>
        {mode === "rectangle" ? (
          <div className="form__section">
            {sizeInput("width", "Width (mm)", widthText, setWidthText)}
            {sizeInput("height", "Height (mm)", heightText, setHeightText)}
          </div>
        ) : (
          <div className="form__section">
            <div
              className="vertex-grid"
              role="group"
              aria-label="Vertices"
              aria-describedby={polygonMessage ? polygonErrorId : undefined}
            >
              <span className="vertex-grid__header">#</span>
              <span className="vertex-grid__header">X (mm)</span>
              <span className="vertex-grid__header">Y (mm)</span>
              <span aria-hidden="true" />
              {rows.map((row, index) => {
                const parsed = parsedRows[index];
                const number = index + 1;
                return (
                  <Fragment key={row.id}>
                    <span className="vertex-grid__index">{number}</span>
                    <input
                      className="input"
                      value={row.x}
                      inputMode="decimal"
                      spellCheck={false}
                      autoComplete="off"
                      aria-label={`Vertex ${number} X (mm)`}
                      aria-invalid={parsed?.x === null ? true : undefined}
                      onChange={(event) =>
                        updateRow(row.id, "x", event.target.value)
                      }
                    />
                    <input
                      className="input"
                      value={row.y}
                      inputMode="decimal"
                      spellCheck={false}
                      autoComplete="off"
                      aria-label={`Vertex ${number} Y (mm)`}
                      aria-invalid={parsed?.y === null ? true : undefined}
                      onChange={(event) =>
                        updateRow(row.id, "y", event.target.value)
                      }
                    />
                    <button
                      type="button"
                      className="icon-button"
                      aria-label={`Remove vertex ${number}`}
                      title={`Remove vertex ${number}`}
                      disabled={rows.length <= MIN_VERTICES}
                      onClick={() => removeVertex(row.id)}
                    >
                      <RemoveIcon />
                    </button>
                  </Fragment>
                );
              })}
            </div>
            <div>
              <button
                type="button"
                className="button"
                disabled={rows.length >= MAX_OUTLINE_POINTS}
                onClick={addVertex}
              >
                Add Vertex
              </button>
            </div>
            {polygonMessage && (
              <span id={polygonErrorId} className="form__error">
                {polygonMessage}
              </span>
            )}
          </div>
        )}
        {initial.replacesCurves && (
          <p className="field__hint field__hint--warning">{REPLACES_CURVES}</p>
        )}
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
            disabled={points === null || !changed || submitting}
          >
            Apply
          </button>
          <button type="button" className="button" onClick={onCancel}>
            Cancel
          </button>
        </div>
      </form>
      <div className="visually-hidden" aria-live="polite">
        {announcement}
      </div>
    </Modal>
  );
}
