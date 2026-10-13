/**
 * Geometry for schematic wiring: orthogonal routes, snapping to pins and
 * wire ends, hit-testing wires, labels, and no-connect flags, and label
 * names. All coordinates are nanometers with Y down.
 */
import type { NetLabel } from "./bindings/NetLabel";
import type { NoConnectFlag } from "./bindings/NoConnectFlag";
import type { SymbolInstance } from "./bindings/SymbolInstance";
import type { WireSegment } from "./bindings/WireSegment";
import type { Xy } from "./bindings/Xy";
import { snapToGrid } from "./symbols";
import type { ScreenPoint } from "./viewport";

/** Screen distance within which the cursor snaps to a pin or wire end. */
export const SNAP_RADIUS_PX = 10;
/** Screen distance within which a click selects a wire. */
export const WIRE_HIT_PX = 6;
/** The longest label name, matching the core. */
export const MAX_LABEL_CHARS = 256;
/** KiCad's default label text height: 1.27 mm. */
export const LABEL_TEXT_NM = 1_270_000;
/** The width of a no-connect flag's X, as KiCad draws it: 1.27 mm. */
export const NO_CONNECT_NM = 1_270_000;

export interface SnappedPoint {
  point: ScreenPoint;
  /** Snapped exactly to a pin end or wire end rather than the grid. */
  onConnection: boolean;
}

/** Every point a wire can connect to: pin ends and wire ends. */
export function connectionPoints(
  instances: SymbolInstance[],
  wires: WireSegment[],
): Xy[] {
  const points: Xy[] = [];
  for (const instance of instances) {
    points.push(...instance.pins);
  }
  for (const wire of wires) {
    points.push(wire.start, wire.end);
  }
  return points;
}

/**
 * Snaps `world` to the nearest connection point within `radiusNm`, or else
 * to the grid.
 */
export function snapWirePoint(
  world: ScreenPoint,
  gridNm: number,
  targets: Xy[],
  radiusNm: number,
): SnappedPoint {
  let best: Xy | null = null;
  let bestDistance = radiusNm;
  for (const target of targets) {
    const distance = Math.hypot(target[0] - world.x, target[1] - world.y);
    if (distance <= bestDistance) {
      best = target;
      bestDistance = distance;
    }
  }
  if (best) {
    return { point: { x: best[0], y: best[1] }, onConnection: true };
  }
  return {
    point: { x: snapToGrid(world.x, gridNm), y: snapToGrid(world.y, gridNm) },
    onConnection: false,
  };
}

/**
 * The points from `from` to `to` with at most one bend: horizontal first
 * unless `verticalFirst`. Straight runs have no bend point.
 */
export function orthogonalRoute(
  from: ScreenPoint,
  to: ScreenPoint,
  verticalFirst: boolean,
): ScreenPoint[] {
  if (from.x === to.x || from.y === to.y) {
    return [from, to];
  }
  const corner = verticalFirst
    ? { x: from.x, y: to.y }
    : { x: to.x, y: from.y };
  return [from, corner, to];
}

/** The polyline of a wire in progress: anchored corners, then the route to the cursor. */
export function wirePath(
  anchors: ScreenPoint[],
  cursor: ScreenPoint | null,
  verticalFirst: boolean,
): ScreenPoint[] {
  const last = anchors[anchors.length - 1];
  if (!last || !cursor) {
    return anchors;
  }
  return [...anchors, ...orthogonalRoute(last, cursor, verticalFirst).slice(1)];
}

/** Drops repeated consecutive points, which the core treats as no segment. */
export function dedupePath(points: ScreenPoint[]): ScreenPoint[] {
  const result: ScreenPoint[] = [];
  for (const point of points) {
    const last = result[result.length - 1];
    if (!last || last.x !== point.x || last.y !== point.y) {
      result.push(point);
    }
  }
  return result;
}

/** The distance from `point` to the segment from `a` to `b`. */
export function distanceToSegment(
  point: ScreenPoint,
  a: ScreenPoint,
  b: ScreenPoint,
): number {
  const dx = b.x - a.x;
  const dy = b.y - a.y;
  const lengthSquared = dx * dx + dy * dy;
  let t = 0;
  if (lengthSquared > 0) {
    const along = (point.x - a.x) * dx + (point.y - a.y) * dy;
    t = Math.max(0, Math.min(1, along / lengthSquared));
  }
  return Math.hypot(point.x - (a.x + t * dx), point.y - (a.y + t * dy));
}

function distanceToWire(point: ScreenPoint, wire: WireSegment): number {
  const start = { x: wire.start[0], y: wire.start[1] };
  const end = { x: wire.end[0], y: wire.end[1] };
  return distanceToSegment(point, start, end);
}

/** The topmost wire within `toleranceNm` of `point`. */
export function findWireAt(
  wires: WireSegment[],
  point: ScreenPoint,
  toleranceNm: number,
): WireSegment | null {
  for (let index = wires.length - 1; index >= 0; index--) {
    const wire = wires[index];
    if (wire && distanceToWire(point, wire) <= toleranceNm) {
      return wire;
    }
  }
  return null;
}

/** Whether `point` lies exactly on a wire, ends included. */
export function isOnWire(wires: WireSegment[], point: ScreenPoint): boolean {
  return wires.some((wire) => distanceToWire(point, wire) === 0);
}

/** Whether `point` is exactly a symbol pin end. */
export function isOnPin(
  instances: SymbolInstance[],
  point: ScreenPoint,
): boolean {
  return instances.some((instance) =>
    instance.pins.some((pin) => pin[0] === point.x && pin[1] === point.y),
  );
}

interface LabelExtent {
  minX: number;
  minY: number;
  maxX: number;
  maxY: number;
}

/** A label's approximate extent for hit-testing, from its text length. */
export function labelBounds(
  label: Pick<NetLabel, "text" | "position" | "rotation">,
): LabelExtent {
  const length = Math.max(1, label.text.length) * LABEL_TEXT_NM * 0.7;
  const half = LABEL_TEXT_NM;
  const [x, y] = label.position;
  switch (((label.rotation % 3600) + 3600) % 3600) {
    case 900:
      return { minX: x - half, minY: y - length, maxX: x + half, maxY: y };
    case 1800:
      return { minX: x - length, minY: y - half, maxX: x, maxY: y + half };
    case 2700:
      return { minX: x - half, minY: y, maxX: x + half, maxY: y + length };
    default:
      return { minX: x, minY: y - half, maxX: x + length, maxY: y + half };
  }
}

/** The topmost label whose extent contains `point`. */
export function findLabelAt(
  labels: NetLabel[],
  point: ScreenPoint,
): NetLabel | null {
  for (let index = labels.length - 1; index >= 0; index--) {
    const label = labels[index];
    if (!label) {
      continue;
    }
    const bounds = labelBounds(label);
    if (
      point.x >= bounds.minX &&
      point.x <= bounds.maxX &&
      point.y >= bounds.minY &&
      point.y <= bounds.maxY
    ) {
      return label;
    }
  }
  return null;
}

/** The topmost no-connect flag whose X, padded by `toleranceNm`, holds `point`. */
export function findNoConnectAt(
  flags: NoConnectFlag[],
  point: ScreenPoint,
  toleranceNm: number,
): NoConnectFlag | null {
  const reach = NO_CONNECT_NM / 2 + toleranceNm;
  for (let index = flags.length - 1; index >= 0; index--) {
    const flag = flags[index];
    if (
      flag &&
      Math.abs(point.x - flag.position[0]) <= reach &&
      Math.abs(point.y - flag.position[1]) <= reach
    ) {
      return flag;
    }
  }
  return null;
}

/** Why a label name is invalid, or `null` when it is fine. */
export function labelTextError(text: string): string | null {
  if (text.length === 0) {
    return "Enter a label name.";
  }
  if ([...text].length > MAX_LABEL_CHARS) {
    return `Use at most ${MAX_LABEL_CHARS} characters.`;
  }
  if (/\s/.test(text)) {
    return "Label names can't contain spaces.";
  }
  return null;
}

/** The next name for repeated placement: trailing digits count up, keeping their width. */
export function nextLabelText(text: string): string {
  const match = /^(.*?)(\d+)$/.exec(text);
  if (!match) {
    return text;
  }
  const prefix = match[1] ?? "";
  const digits = match[2] ?? "";
  const next = (BigInt(digits) + 1n).toString();
  return prefix + next.padStart(digits.length, "0");
}
