/**
 * Lays out and draws the waveform viewer's plot: an x axis of time or log frequency, a
 * value axis on the left and another on the right when traces use them, gridlines, one
 * line per trace, and the cursors. The context arrives in CSS pixels, and x positions
 * are in the axis units of `waveformView`.
 */
import type { OpAnnotationKind } from "./bindings/OpAnnotationKind";
import type { WaveformPoint } from "./bindings/WaveformPoint";
import { formatQuantity, formatSi } from "./si";
import type { XRange, XScale } from "./waveformView";

export const TICK_COUNT = 5;
const TRACE_WIDTH_PX = 1.5;
const AXIS_WIDTH_PX = 56;
/** The extra room an axis title takes beside its tick labels. */
const TITLE_WIDTH_PX = 16;
const TITLE_GAP_PX = 2;
const EDGE_PX = 12;
const TOP_PX = 8;
const BOTTOM_PX = 24;
const TICK_GAP_PX = 6;
/** The closest two frequency labels may sit, centre to centre. */
const LABEL_GAP_PX = 56;
/** Below this many pixels per decade, a log axis has no minor gridlines. */
const MINOR_DECADE_PX = 24;
const DASH_PX = [4, 4];
/** The step of a phase axis is this many degrees times a power of two. */
const PHASE_STEP_DEG = 45;

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

function axisTicks(first: number, last: number, step: number): number[] {
  const ticks: number[] = [];
  for (let index = 0; first + index * step <= last + step * 1e-9; index++) {
    ticks.push(Number((first + index * step).toPrecision(12)));
  }
  return ticks;
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
  // Just under the exact step, so rounding error can't push it past a round factor.
  const raw = ((high - low) / (count - 1)) * (1 - 1e-9);
  const power = 10 ** Math.floor(Math.log10(raw));
  const factor = [1, 2, 2.5, 5].find((item) => item * power >= raw) ?? 10;
  const step = factor * power;
  const first = Math.floor(low / step + 1e-9) * step;
  const last = Math.ceil(high / step - 1e-9) * step;
  return { min: first, max: last, ticks: axisTicks(first, last, step) };
}

/**
 * A phase axis from `min` to `max` degrees, widened to steps of 45° times a power of
 * two, with at most `count` ticks. A flat phase gets a step either side.
 */
export function phaseAxis(min: number, max: number, count = TICK_COUNT): Axis {
  let step = PHASE_STEP_DEG;
  while ((max - min) / step > count - 1) {
    step *= 2;
  }
  let first = Math.floor(min / step + 1e-9) * step;
  let last = Math.ceil(max / step - 1e-9) * step;
  if (first === last) {
    first -= step;
    last += step;
  }
  return { min: first, max: last, ticks: axisTicks(first, last, step) };
}

export function unitOf(quantity: OpAnnotationKind): string {
  return quantity === "voltage" ? "V" : "A";
}

/** A tick label such as `200us`, `1.5V`, or `0A`. */
export function tickLabel(value: number, unit: string): string {
  return `${formatSi(value)}${unit}`;
}

/** A frequency to at most 3 significant digits, such as `10 Hz` or `2.5 kHz`. */
export function frequencyLabel(hertz: number): string {
  const [digits = "", unit = ""] = formatQuantity(hertz, "Hz").split(" ");
  const trimmed = digits.includes(".") ? digits.replace(/\.?0+$/, "") : digits;
  return `${trimmed} ${unit}`;
}

/** A position along the x axis, in axis units, and its label if it has room for one. */
export interface XTick {
  at: number;
  label: string | null;
}

/**
 * The ticks of a log frequency axis showing `view`, at `decadePx` pixels per decade:
 * one at each decade, and at its multiples of 2 to 9 when there is room. Decades are
 * labelled where their labels fit; the multiples are labelled only when fewer than two
 * decades show. A view narrower than the gap between two ticks gets round steps of
 * frequency instead.
 */
export function logTicks(view: XRange, decadePx: number): XTick[] {
  const slack = (view.max - view.min) * 1e-9;
  const minors = decadePx >= MINOR_DECADE_PX;
  const ticks: { at: number; hertz: number; major: boolean }[] = [];
  for (let decade = Math.floor(view.min); decade <= view.max; decade++) {
    for (let multiple = 1; multiple <= (minors ? 9 : 1); multiple++) {
      const at = decade + Math.log10(multiple);
      if (at >= view.min - slack && at <= view.max + slack) {
        ticks.push({
          at,
          hertz: multiple * 10 ** decade,
          major: multiple === 1,
        });
      }
    }
  }
  if (ticks.length < 2) {
    const low = 10 ** view.min;
    const high = 10 ** view.max;
    const round = niceAxis(low, high).ticks.filter(
      (hertz) => hertz >= low * (1 - 1e-9) && hertz <= high * (1 + 1e-9),
    );
    return round.map((hertz) => ({
      at: Math.log10(hertz),
      label: frequencyLabel(hertz),
    }));
  }
  const labelMinors = ticks.filter((tick) => tick.major).length < 2;
  const placed: number[] = [];
  const labels = new Map<number, string>();
  for (const major of [true, false]) {
    ticks.forEach((tick, index) => {
      const x = (tick.at - view.min) * decadePx;
      const wanted = major ? tick.major : !tick.major && labelMinors;
      const free = placed.every((other) => Math.abs(other - x) >= LABEL_GAP_PX);
      if (wanted && free) {
        placed.push(x);
        labels.set(index, frequencyLabel(tick.hertz));
      }
    });
  }
  return ticks.map((tick, index) => ({
    at: tick.at,
    label: labels.get(index) ?? null,
  }));
}

/** Which value axis a trace is drawn against. */
export type Side = "left" | "right";

export interface Series {
  /** In axis units along x. */
  points: WaveformPoint[];
  side: Side;
  /** The unit of the axis on `side`, such as `V` or `dB`. */
  unit: string;
  color: string;
  dashed: boolean;
}

export interface PlotColors {
  background: string;
  grid: string;
  gridMajor: string;
  text: string;
}

/** How the axes are drawn. */
export interface PlotAxes {
  scale: XScale;
  /** Titles beside the value axes, such as `Phase (°)`, or `null` for none. */
  leftTitle: string | null;
  rightTitle: string | null;
}

/** A cursor line to draw, in axis units. */
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

/**
 * The plot area of a canvas of `size`, leaving room for the value axes that are shown,
 * and for their titles when `titled`.
 */
export function plotArea(
  size: Size,
  left: boolean,
  right: boolean,
  titled = false,
): PlotArea {
  const axis = AXIS_WIDTH_PX + (titled ? TITLE_WIDTH_PX : 0);
  return {
    left: left ? axis : EDGE_PX,
    right: size.width - (right ? axis : EDGE_PX),
    top: TOP_PX,
    bottom: size.height - BOTTOM_PX,
  };
}

/** The position at `x` pixels across `area` when it shows `view`. */
export function timeAtPixel(area: PlotArea, view: XRange, x: number): number {
  const fraction = (x - area.left) / (area.right - area.left);
  return view.min + fraction * (view.max - view.min);
}

/** How many pixels across `area` the position `at` is drawn when it shows `view`. */
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

function axisFor(series: Series[], side: Side, view: XRange): Axis | null {
  const onSide = series.filter((item) => item.side === side);
  const values = onSide.flatMap((item) =>
    visiblePoints(item.points, view).map((point) => point.y),
  );
  const range = extent(values);
  if (!range) {
    return null;
  }
  return onSide[0]?.unit === "°"
    ? phaseAxis(range[0], range[1])
    : niceAxis(range[0], range[1]);
}

/** The time ticks for `view`: round steps, without any outside it. */
export function timeTicks(view: XRange): number[] {
  const slack = (view.max - view.min) * 1e-9;
  return niceAxis(view.min, view.max).ticks.filter(
    (tick) => tick >= view.min - slack && tick <= view.max + slack,
  );
}

function xTicks(view: XRange, scale: XScale, widthPx: number): XTick[] {
  if (scale === "log") {
    return logTicks(view, widthPx / (view.max - view.min));
  }
  return timeTicks(view).map((at) => ({ at, label: tickLabel(at, "s") }));
}

function valueLabel(value: number, unit: string, scale: XScale): string {
  return scale === "log" ? `${value} ${unit}` : tickLabel(value, unit);
}

/**
 * Draws the axes, gridlines, and traces of `series` filling `size`, over the positions
 * in `view`, then the `cursors` that `view` shows. The value axes fit the visible
 * samples.
 */
export function drawWaveforms(
  context: CanvasRenderingContext2D,
  size: Size,
  series: Series[],
  colors: PlotColors,
  font: string,
  view: XRange,
  cursors: CursorLine[],
  axes: PlotAxes,
) {
  context.fillStyle = colors.background;
  context.fillRect(0, 0, size.width, size.height);
  const leftAxis = axisFor(series, "left", view);
  const rightAxis = axisFor(series, "right", view);
  if (view.max <= view.min || (!leftAxis && !rightAxis)) {
    return;
  }
  const titled = axes.leftTitle !== null || axes.rightTitle !== null;
  const area = plotArea(size, leftAxis !== null, rightAxis !== null, titled);
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
  context.strokeStyle = colors.grid;
  for (const tick of xTicks(view, axes.scale, right - left)) {
    const x = Math.round(xAt(tick.at)) + 0.5;
    context.beginPath();
    context.moveTo(x, top);
    context.lineTo(x, bottom);
    context.stroke();
    if (tick.label !== null) {
      context.fillText(tick.label, x, bottom + TICK_GAP_PX);
    }
  }
  context.textBaseline = "middle";
  const sides: [Axis | null, Side, number, CanvasTextAlign][] = [
    [leftAxis, "left", left - TICK_GAP_PX, "right"],
    [rightAxis, "right", right + TICK_GAP_PX, "left"],
  ];
  for (const [axis, side, labelX, align] of sides) {
    const unit = series.find((item) => item.side === side)?.unit ?? "";
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
      context.fillText(valueLabel(tick, unit, axes.scale), labelX, y);
    }
  }
  const titles: [string | null, number, number][] = [
    [axes.leftTitle, TITLE_GAP_PX, -Math.PI / 2],
    [axes.rightTitle, size.width - TITLE_GAP_PX, Math.PI / 2],
  ];
  for (const [title, x, angle] of titles) {
    if (title === null) {
      continue;
    }
    context.save();
    context.translate(x, (top + bottom) / 2);
    context.rotate(angle);
    context.textAlign = "center";
    context.textBaseline = "top";
    context.fillText(title, 0, 0);
    context.restore();
  }

  context.save();
  context.beginPath();
  context.rect(left, top, right - left, bottom - top);
  context.clip();
  context.lineWidth = TRACE_WIDTH_PX;
  context.lineJoin = "round";
  for (const item of series) {
    const axis = item.side === "left" ? leftAxis : rightAxis;
    if (!axis || item.points.length === 0) {
      continue;
    }
    context.strokeStyle = item.color;
    context.setLineDash(item.dashed ? DASH_PX : []);
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
  context.setLineDash([]);
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
