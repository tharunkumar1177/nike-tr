/**
 * The model behind interactive routing in the board view: 45-degree route
 * legs, the route being drawn as steps that undo one at a time, snapping to
 * pads, track ends, and vias, picking tracks and vias, choosing copper
 * layers, and each layer's color token. Coordinates are board nanometers
 * with Y down.
 */
import type { BoardDesignRules } from "./bindings/BoardDesignRules";
import type { BoardFootprint } from "./bindings/BoardFootprint";
import type { BoardTrack } from "./bindings/BoardTrack";
import type { BoardVia } from "./bindings/BoardVia";
import type { RoutePiece } from "./bindings/RoutePiece";
import type { TrackRoute } from "./bindings/TrackRoute";
import type { Xy } from "./bindings/Xy";
import { snapToGrid } from "./symbols";
import type { ScreenPoint } from "./viewport";
import { distanceToSegment } from "./wiring";

/** Screen distance within which a click picks a track or via, or snaps to copper. */
export const TRACK_HIT_PX = 6;
/** How many inner layer colors there are before they repeat. */
const INNER_LAYER_COLORS = 6;

/** A point a route can snap to, and the copper there. */
export interface RouteTarget {
  point: Xy;
  net: string | null;
  /** The copper's layer, or `null` for vias and pads with holes, which are on all of them. */
  layer: string | null;
}

/** One click of a route being drawn: the pieces it added, and whether it ended in a via. */
export interface RouteStep {
  pieces: RoutePiece[];
  via: boolean;
  /** The active layer before the step; undoing the step returns to it. */
  layerBefore: string;
}

/** A route being drawn, from where it started. */
export interface RouteDraft {
  net: string | null;
  start: Xy;
  steps: RouteStep[];
}

/** The color custom property a copper layer is drawn with. */
export function layerColorToken(layer: string): string {
  if (layer === "F.Cu") {
    return "--color-layer-f-cu";
  }
  if (layer === "B.Cu") {
    return "--color-layer-b-cu";
  }
  const match = /^In(\d+)\.Cu$/.exec(layer);
  const inner = match ? Number(match[1]) : 1;
  return `--color-layer-in${((inner - 1) % INNER_LAYER_COLORS) + 1}`;
}

/**
 * The corners from `from` to `to` with at most one bend, every piece
 * horizontal, vertical, or at 45 degrees: the straight run first unless
 * `diagonalFirst`.
 */
export function routeLeg(from: Xy, to: Xy, diagonalFirst: boolean): Xy[] {
  const dx = to[0] - from[0];
  const dy = to[1] - from[1];
  const run = Math.min(Math.abs(dx), Math.abs(dy));
  if (run === 0 || Math.abs(dx) === Math.abs(dy)) {
    return [from, to];
  }
  const diagonalX = Math.sign(dx) * run;
  const diagonalY = Math.sign(dy) * run;
  const bend: Xy = diagonalFirst
    ? [from[0] + diagonalX, from[1] + diagonalY]
    : [to[0] - diagonalX, to[1] - diagonalY];
  return [from, bend, to];
}

/** Whether two points are the same. */
function samePoint(a: Xy, b: Xy): boolean {
  return a[0] === b[0] && a[1] === b[1];
}

/** The pieces of the leg from `from` to `to` on `layer`, without empty ones. */
export function legPieces(
  from: Xy,
  to: Xy,
  diagonalFirst: boolean,
  layer: string,
): RoutePiece[] {
  const corners = routeLeg(from, to, diagonalFirst);
  const pieces: RoutePiece[] = [];
  for (let index = 1; index < corners.length; index++) {
    const start = corners[index - 1];
    const end = corners[index];
    if (start && end && !samePoint(start, end)) {
      pieces.push({ start, end, layer });
    }
  }
  return pieces;
}

/** Every piece of the route so far, in drawing order. */
export function routePieces(draft: RouteDraft): RoutePiece[] {
  return draft.steps.flatMap((step) => step.pieces);
}

/** Where the route so far ends: the end of its last piece, or its start. */
export function routeEnd(draft: RouteDraft): Xy {
  const pieces = routePieces(draft);
  return pieces[pieces.length - 1]?.end ?? draft.start;
}

/** The via positions of the route so far, each once, in drawing order. */
export function routeVias(draft: RouteDraft): Xy[] {
  const vias: Xy[] = [];
  let end = draft.start;
  for (const step of draft.steps) {
    end = step.pieces[step.pieces.length - 1]?.end ?? end;
    if (step.via && !vias.some((via) => samePoint(via, end))) {
      vias.push(end);
    }
  }
  return vias;
}

/**
 * The route extended to `to` on `layer`, ending in a via when `via` is set.
 * A step that adds neither a piece nor a via leaves the route as it is.
 */
export function extendRoute(
  draft: RouteDraft,
  to: Xy,
  diagonalFirst: boolean,
  layer: string,
  via: boolean,
): RouteDraft {
  const pieces = legPieces(routeEnd(draft), to, diagonalFirst, layer);
  if (pieces.length === 0 && !via) {
    return draft;
  }
  const step: RouteStep = { pieces, via, layerBefore: layer };
  return { ...draft, steps: [...draft.steps, step] };
}

/** The finished route to send to the core, or `null` when it has no pieces yet. */
export function finishedRoute(
  draft: RouteDraft,
  rules: BoardDesignRules,
): TrackRoute | null {
  const pieces = routePieces(draft);
  if (pieces.length === 0) {
    return null;
  }
  return {
    net: draft.net,
    widthNm: rules.trackWidthNm,
    viaDiameterNm: rules.viaDiameterNm,
    viaDrillNm: rules.viaDrillNm,
    pieces,
    vias: routeVias(draft),
  };
}

/**
 * The layer a via drops the route onto: the mirror of `layer` in the stack,
 * so F.Cu and B.Cu swap and inner layers swap halves.
 */
export function viaLayer(layers: string[], layer: string): string {
  const index = layers.indexOf(layer);
  if (index < 0) {
    return layers[0] ?? layer;
  }
  return layers[layers.length - 1 - index] ?? layer;
}

/**
 * The next layer in `visible` after `layer`, toward the bottom of the board
 * when `step` is 1 and toward the top when it is -1, wrapping around.
 */
export function stepLayer(
  visible: string[],
  layer: string,
  step: 1 | -1,
): string {
  const index = visible.indexOf(layer);
  if (visible.length === 0) {
    return layer;
  }
  if (index < 0) {
    return visible[0] ?? layer;
  }
  const next = (index + step + visible.length) % visible.length;
  return visible[next] ?? layer;
}

/** Every point a route can start, bend onto, or finish at: pads, track ends, and vias. */
export function routeTargets(
  footprints: BoardFootprint[],
  tracks: BoardTrack[],
  vias: BoardVia[],
): RouteTarget[] {
  const targets: RouteTarget[] = [];
  for (const footprint of footprints) {
    const side = footprint.side === "front" ? "F.Cu" : "B.Cu";
    for (const pad of footprint.pads) {
      const layer = pad.drill ? null : side;
      targets.push({ point: pad.position, net: pad.net, layer });
    }
  }
  for (const track of tracks) {
    const { net, layer } = track;
    targets.push({ point: track.start, net, layer });
    targets.push({ point: track.end, net, layer });
  }
  for (const via of vias) {
    targets.push({ point: via.position, net: via.net, layer: null });
  }
  return targets;
}

/** A snapped route point, and the copper it snapped to. */
export interface SnappedRoutePoint {
  point: Xy;
  target: RouteTarget | null;
}

/**
 * Snaps `world` to the nearest target within `radiusNm`, or else to the
 * grid.
 */
export function snapRoutePoint(
  world: ScreenPoint,
  targets: RouteTarget[],
  radiusNm: number,
  gridNm: number,
): SnappedRoutePoint {
  let best: RouteTarget | null = null;
  let bestDistance = radiusNm;
  for (const target of targets) {
    const [x, y] = target.point;
    const distance = Math.hypot(x - world.x, y - world.y);
    if (distance <= bestDistance) {
      best = target;
      bestDistance = distance;
    }
  }
  if (best) {
    return { point: best.point, target: best };
  }
  const point: Xy = [snapToGrid(world.x, gridNm), snapToGrid(world.y, gridNm)];
  return { point, target: null };
}

/** The topmost via whose copper, widened by `toleranceNm`, holds `point`. */
export function findViaAt(
  vias: BoardVia[],
  point: ScreenPoint,
  toleranceNm: number,
): BoardVia | null {
  for (let index = vias.length - 1; index >= 0; index--) {
    const via = vias[index];
    if (!via) {
      continue;
    }
    const [x, y] = via.position;
    const reach = via.diameterNm / 2 + toleranceNm;
    if (Math.hypot(point.x - x, point.y - y) <= reach) {
      return via;
    }
  }
  return null;
}

/**
 * The topmost track on a layer in `visible` whose copper, widened by
 * `toleranceNm`, holds `point`.
 */
export function findTrackAt(
  tracks: BoardTrack[],
  point: ScreenPoint,
  toleranceNm: number,
  visible: ReadonlySet<string>,
): BoardTrack | null {
  for (let index = tracks.length - 1; index >= 0; index--) {
    const track = tracks[index];
    if (!track || !visible.has(track.layer)) {
      continue;
    }
    const start = { x: track.start[0], y: track.start[1] };
    const end = { x: track.end[0], y: track.end[1] };
    const reach = track.widthNm / 2 + toleranceNm;
    if (distanceToSegment(point, start, end) <= reach) {
      return track;
    }
  }
  return null;
}
