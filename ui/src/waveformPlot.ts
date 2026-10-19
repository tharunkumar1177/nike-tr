/**
 * Lays out and draws the waveform viewer's plot: a time axis, a voltage axis on the
 * left and a current axis on the right when traces of that kind exist, gridlines, and
 * one line per trace. The context arrives in CSS pixels.
 */
import type { OpAnnotationKind } from "./bindings/OpAnnotationKind";
import type { WaveformPoint } from "./bindings/WaveformPoint";
import { formatSi } from "./si";

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

interface Size {
  width: number;
  height: number;
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

function axisFor(series: Series[], quantity: OpAnnotationKind): Axis | null {
  const values = series
    .filter((item) => item.quantity === quantity)
    .flatMap((item) => item.points.map((point) => point.y));
  const range = extent(values);
  return range ? niceAxis(range[0], range[1]) : null;
}

/** Draws the axes, gridlines, and traces of `series` filling `size`. */
export function drawWaveforms(
  context: CanvasRenderingContext2D,
  size: Size,
  series: Series[],
  colors: PlotColors,
  font: string,
) {
  context.fillStyle = colors.background;
  context.fillRect(0, 0, size.width, size.height);
  const xRange = extent(series.flatMap((item) => item.points.map((p) => p.x)));
  const voltage = axisFor(series, "voltage");
  const current = axisFor(series, "current");
  if (!xRange || (!voltage && !current)) {
    return;
  }
  const time = niceAxis(xRange[0], xRange[1]);
  const left = voltage ? AXIS_WIDTH_PX : EDGE_PX;
  const right = size.width - (current ? AXIS_WIDTH_PX : EDGE_PX);
  const top = TOP_PX;
  const bottom = size.height - BOTTOM_PX;
  if (right <= left || bottom <= top) {
    return;
  }
  const xAt = (x: number) =>
    left + ((x - time.min) / (time.max - time.min)) * (right - left);
  const yAt = (axis: Axis, y: number) =>
    bottom - ((y - axis.min) / (axis.max - axis.min)) * (bottom - top);

  context.font = font;
  context.lineWidth = 1;
  context.fillStyle = colors.text;
  context.textBaseline = "top";
  context.textAlign = "center";
  for (const tick of time.ticks) {
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
  context.restore();
}
