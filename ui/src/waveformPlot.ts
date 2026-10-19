/**
 * Lays out and draws the waveform viewer's plot: a time axis, a voltage axis on the
 * left and a current axis on the right when traces of that kind exist, gridlines, one
 * line per trace, and the cursors. The context arrives in CSS pixels.
 */
import type { OpAnnotationKind } from "./bindings/OpAnnotationKind";
import type { WaveformPoint } from "./bindings/WaveformPoint";
import { formatSi } from "./si";
import type { XRange } from "./waveformView";

export const TICK_COUNT = 5;
const TRACE_WIDTH_PX = 1.5;
const AXIS_WIDTH_PX = 56;
const EDGE_PX = 12;
const TOP_PX = 8;
const BOTTOM_PX = 24;
const TICK_GAP_PX = 6;

/** The trace colors, by token, in the order traces take them. */
export const TRACE_TOKENS = [
  "--color-focus",
  "--color-text-warning",
  "--color-text-error",
  "--color-text-primary",
];

export interface Axis {
  min: number;
  max: number;
  ticks: number[];
}

/**
 * An axis from `min` to `max` widened to round steps of 1, 2, 2.5, or 5 times a power
 * of ten, with about `count` ticks. An empty range is widened around its value.
 */
export function niceAxis(min: number, max: number, count = TICK_COUNT): Axis {
  let low = min;
  let high = max;
  if (low === high) {
    const pad = low === 0 ? 1 : Math.abs(low) * 0.1;
    low -= pad;
    high += pad;
  }
  const raw = (high - low) / (count - 1);
  const power = 10 ** Math.floor(Math.log10(raw));
  const factor = [1, 2, 2.5, 5].find((item) => item * power >= raw) ?? 10;
  const step = factor * power;
  const first = Math.floor(low / step + 1e-9) * step;
  const last = Math.ceil(high / step - 1e-9) * step;
  const ticks: number[] = [];
  for (let index = 0; first + index * step <= last + step * 1e-9; index++) {
    ticks.push(Number((first + index * step).toPrecision(12)));
  }
  return { min: first, max: last, ticks };
}

export function unitOf(quantity: OpAnnotationKind): string {
  return quantity === "voltage" ? "V" : "A";
}

/** A tick label such as `200us`, `1.5V`, or `0A`. */
export function tickLabel(value: number, unit: string): string {
  return `${formatSi(value)}${unit}`;
}

export interface Series {
  points: WaveformPoint[];
  quantity: OpAnnotationKind;
  color: string;
}

export interface PlotColors {
  background: string;
  grid: string;
  gridMajor: string;
  text: string;
}

/** A cursor line to draw. */
export interface CursorLine {
  at: number;
  color: string;
}

interface Size {
  width: number;
  height: number;
}

/** Where the traces are drawn inside the canvas, in CSS pixels. */
export interface PlotArea {
  left: number;
  right: number;
  top: number;
  bottom: number;
}

/** The plot area of a canvas of `size`, leaving room for the axes that are shown. */
export function plotArea(
  size: Size,
  voltage: boolean,
  current: boolean,
): PlotArea {
  return {
    left: voltage ? AXIS_WIDTH_PX : EDGE_PX,
    right: size.width - (current ? AXIS_WIDTH_PX : EDGE_PX),
    top: TOP_PX,
    bottom: size.height - BOTTOM_PX,
  };
}

/** The time at `x` pixels across `area` when it shows `view`. */
export function timeAtPixel(area: PlotArea, view: XRange, x: number): number {
  const fraction = (x - area.left) / (area.right - area.left);
  return view.min + fraction * (view.max - view.min);
}

/** How many pixels across `area` the time `at` is drawn when it shows `view`. */
export function pixelAtTime(area: PlotArea, view: XRange, at: number): number {
  const fraction = (at - view.min) / (view.max - view.min);
  return area.left + fraction * (area.right - area.left);
}

function extent(values: number[]): [number, number] | null {
  let low = Infinity;
  let high = -Infinity;
  for (const value of values) {
    if (Number.isFinite(value)) {
      low = Math.min(low, value);
      high = Math.max(high, value);
    }
  }
  return low <= high ? [low, high] : null;
}

/**
 * The samples of `points` that `view` shows, plus the one just outside each end so a
 * trace whose samples straddle the view still has a range.
 */
export function visiblePoints(
  points: WaveformPoint[],
  view: XRange,
): WaveformPoint[] {
  const first = points.findIndex((point) => point.x >= view.min);
  if (first === -1) {
    return points.slice(-1);
  }
  let end = first;
  while (end < points.length && points[end]!.x <= view.max) {
    end++;
  }
  return points.slice(Math.max(first - 1, 0), end + 1);
}

function axisFor(
  series: Series[],
  quantity: OpAnnotationKind,
  view: XRange,
): Axis | null {
  const values = series
    .filter((item) => item.quantity === quantity)
    .flatMap((item) =>
      visiblePoints(item.points, view).map((point) => point.y),
    );
  const range = extent(values);
  return range ? niceAxis(range[0], range[1]) : null;
}

/** The time ticks for `view`: round steps, without any outside it. */
export function timeTicks(view: XRange): number[] {
  const slack = (view.max - view.min) * 1e-9;
  return niceAxis(view.min, view.max).ticks.filter(
    (tick) => tick >= view.min - slack && tick <= view.max + slack,
  );
}

/**
 * Draws the axes, gridlines, and traces of `series` filling `size`, over the times in
 * `view`, then the `cursors` that `view` shows. The value axes fit the visible samples.
 */
export function drawWaveforms(
  context: CanvasRenderingContext2D,
  size: Size,
  series: Series[],
  colors: PlotColors,
  font: string,
  view: XRange,
  cursors: CursorLine[],
) {
  context.fillStyle = colors.background;
  context.fillRect(0, 0, size.width, size.height);
  const voltage = axisFor(series, "voltage", view);
  const current = axisFor(series, "current", view);
  if (view.max <= view.min || (!voltage && !current)) {
    return;
  }
  const area = plotArea(size, voltage !== null, current !== null);
  const { left, right, top, bottom } = area;
  if (right <= left || bottom <= top) {
    return;
  }
  const xAt = (x: number) => pixelAtTime(area, view, x);
  const yAt = (axis: Axis, y: number) =>
    bottom - ((y - axis.min) / (axis.max - axis.min)) * (bottom - top);

  context.font = font;
  context.lineWidth = 1;
  context.fillStyle = colors.text;
  context.textBaseline = "top";
  context.textAlign = "center";
  for (const tick of timeTicks(view)) {
    const x = Math.round(xAt(tick)) + 0.5;
    context.strokeStyle = colors.grid;
    context.beginPath();
    context.moveTo(x, top);
    context.lineTo(x, bottom);
    context.stroke();
    context.fillText(tickLabel(tick, "s"), x, bottom + TICK_GAP_PX);
  }
  context.textBaseline = "middle";
  const sides: [Axis | null, OpAnnotationKind, number, CanvasTextAlign][] = [
    [voltage, "voltage", left - TICK_GAP_PX, "right"],
    [current, "current", right + TICK_GAP_PX, "left"],
  ];
  for (const [axis, quantity, labelX, align] of sides) {
    if (!axis) {
      continue;
    }
    context.textAlign = align;
    for (const tick of axis.ticks) {
      const y = Math.round(yAt(axis, tick)) + 0.5;
      context.strokeStyle = tick === 0 ? colors.gridMajor : colors.grid;
      context.beginPath();
      context.moveTo(left, y);
      context.lineTo(right, y);
      context.stroke();
      context.fillText(tickLabel(tick, unitOf(quantity)), labelX, y);
    }
  }

  context.save();
  context.beginPath();
  context.rect(left, top, right - left, bottom - top);
  context.clip();
  context.lineWidth = TRACE_WIDTH_PX;
  context.lineJoin = "round";
  for (const item of series) {
    const axis = item.quantity === "voltage" ? voltage : current;
    if (!axis || item.points.length === 0) {
      continue;
    }
    context.strokeStyle = item.color;
    context.beginPath();
    item.points.forEach((point, index) => {
      const x = xAt(point.x);
      const y = yAt(axis, point.y);
      if (index === 0) {
        context.moveTo(x, y);
      } else {
        context.lineTo(x, y);
      }
    });
    context.stroke();
  }
  context.lineWidth = 1;
  for (const cursor of cursors) {
    if (cursor.at < view.min || cursor.at > view.max) {
      continue;
    }
    const x = Math.round(xAt(cursor.at)) + 0.5;
    context.strokeStyle = cursor.color;
    context.beginPath();
    context.moveTo(x, top);
    context.lineTo(x, bottom);
    context.stroke();
  }
  context.restore();
}
