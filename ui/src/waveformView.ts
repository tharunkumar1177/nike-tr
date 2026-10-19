/**
 * The time range the waveform viewer shows and where its cursors sit: zooming and
 * panning within a run, and the span its measurements cover.
 */

/** A range of time, in seconds. */
export interface XRange {
  min: number;
  max: number;
}

/** The two measurement cursors, in seconds, or `null` where one isn't placed. */
export interface Cursors {
  a: number | null;
  b: number | null;
}

export const NO_CURSORS: Cursors = { a: null, b: null };

/** The narrowest view, as a fraction of the run. */
const MIN_SPAN_FRACTION = 1e-6;
/** How much one wheel step or `+` key narrows the view. */
export const ZOOM_STEP = 1.25;
/** How far an arrow key pans, as a fraction of the view. */
export const PAN_STEP_FRACTION = 0.1;
/** How far an arrow key moves a cursor, as a fraction of the view. */
export const CURSOR_STEP_FRACTION = 0.01;

/** `view` moved, and narrowed if need be, to lie inside `run`. */
export function clampView(view: XRange, run: XRange): XRange {
  const runSpan = run.max - run.min;
  const wanted = Math.max(view.max - view.min, runSpan * MIN_SPAN_FRACTION);
  const span = Math.min(wanted, runSpan);
  const min = Math.min(Math.max(view.min, run.min), run.max - span);
  return { min, max: min + span };
}

/** Whether `view` shows all of `run`. */
export function showsWholeRun(view: XRange, run: XRange): boolean {
  return view.min <= run.min && view.max >= run.max;
}

/**
 * `view` scaled by `factor` around the time `at`, which stays where it is. A factor
 * below 1 zooms in. `null` once the result shows the whole run.
 */
export function zoomView(
  view: XRange,
  run: XRange,
  at: number,
  factor: number,
): XRange | null {
  const zoomed = clampView(
    {
      min: at - (at - view.min) * factor,
      max: at + (view.max - at) * factor,
    },
    run,
  );
  return showsWholeRun(zoomed, run) ? null : zoomed;
}

/** `view` moved later by `by` seconds, without leaving `run`. */
export function panView(view: XRange, run: XRange, by: number): XRange {
  return clampView({ min: view.min + by, max: view.max + by }, run);
}

/** `at` moved into `range`. */
export function clampTime(at: number, range: XRange): number {
  return Math.min(Math.max(at, range.min), range.max);
}

/**
 * The span the measurements cover: between the cursors when both are placed, else the
 * view, where `null` bounds mean the whole run.
 */
export function measuredSpan(
  cursors: Cursors,
  view: XRange | null,
): { from: number | null; to: number | null; betweenCursors: boolean } {
  const { a, b } = cursors;
  if (a !== null && b !== null) {
    return { from: Math.min(a, b), to: Math.max(a, b), betweenCursors: true };
  }
  return {
    from: view?.min ?? null,
    to: view?.max ?? null,
    betweenCursors: false,
  };
}

/** The placed cursors, in the order A then B, as `measure_trace` takes them. */
export function placedCursors(cursors: Cursors): number[] {
  return [cursors.a, cursors.b].filter((at): at is number => at !== null);
}
