/**
 * Board view geometry: the extent of the board for fitting, which footprint a
 * point picks, pad corner radii, and ratsnest lines following a drag. World
 * coordinates are board nanometers with Y down.
 */
import type { BoardFootprint } from "./bindings/BoardFootprint";
import type { BoardPad } from "./bindings/BoardPad";
import type { OutlineGraphic } from "./bindings/OutlineGraphic";
import type { RatsnestSegment } from "./bindings/RatsnestSegment";
import type { Xy } from "./bindings/Xy";
import { type Bounds, arcThrough } from "./libraryPreview";
import {
  NM_PER_MM,
  type ScreenPoint,
  type Size,
  type Viewport,
  clampZoom,
  pxPerNm,
} from "./viewport";

/** The snap step of a footprint drag: 0.635 mm. */
export const BOARD_GRID_NM = 635_000;
/** The side of the area an empty board view shows, from the origin. */
export const EMPTY_BOARD_NM = 100 * NM_PER_MM;

/** Grows `bounds` to include the point (x, y), widened by `pad` on every side. */
function include(bounds: Bounds | null, x: number, y: number, pad = 0): Bounds {
  if (bounds === null) {
    return { minX: x - pad, minY: y - pad, maxX: x + pad, maxY: y + pad };
  }
  return {
    minX: Math.min(bounds.minX, x - pad),
    minY: Math.min(bounds.minY, y - pad),
    maxX: Math.max(bounds.maxX, x + pad),
    maxY: Math.max(bounds.maxY, y + pad),
  };
}

/** The box around one outline graphic, its stroke width included. */
function outlineBounds(graphic: OutlineGraphic): Bounds | null {
  const pad = graphic.widthNm / 2;
  const { geometry } = graphic;
  let points: Xy[];
  switch (geometry.type) {
    case "line":
    case "rect":
      points = [geometry.start, geometry.end];
      break;
    case "arc": {
      const arc = arcThrough(geometry.start, geometry.mid, geometry.end);
      if (arc) {
        const r = arc.radius + pad;
        return {
          minX: arc.centerX - r,
          minY: arc.centerY - r,
          maxX: arc.centerX + r,
          maxY: arc.centerY + r,
        };
      }
      points = [geometry.start, geometry.mid, geometry.end];
      break;
    }
    case "circle": {
      const [cx, cy] = geometry.center;
      const radius = Math.hypot(geometry.end[0] - cx, geometry.end[1] - cy);
      return include(null, cx, cy, radius + pad);
    }
    case "polygon":
      points = geometry.points;
      break;
  }
  let bounds: Bounds | null = null;
  for (const [x, y] of points) {
    bounds = include(bounds, x, y, pad);
  }
  return bounds;
}

/**
 * The box around the board outline and every footprint, or `null` for an
 * empty board. Arcs count by their whole circle, which only ever widens the
 * fit a little.
 */
export function boardBounds(
  footprints: BoardFootprint[],
  outline: OutlineGraphic[],
): Bounds | null {
  let bounds: Bounds | null = null;
  for (const graphic of outline) {
    const box = outlineBounds(graphic);
    if (box) {
      bounds = include(bounds, box.minX, box.minY);
      bounds = include(bounds, box.maxX, box.maxY);
    }
  }
  for (const footprint of footprints) {
    const { min, max } = footprint.bounds;
    bounds = include(bounds, min[0], min[1]);
    bounds = include(bounds, max[0], max[1]);
  }
  return bounds;
}

/**
 * The viewport that shows the whole board in `size`, centered with a margin.
 * An empty board shows a 100 mm square from the origin.
 */
export function fitBoard(
  size: Size,
  footprints: BoardFootprint[],
  outline: OutlineGraphic[],
  marginPx = 24,
): Viewport {
  const bounds = boardBounds(footprints, outline) ?? {
    minX: 0,
    minY: 0,
    maxX: EMPTY_BOARD_NM,
    maxY: EMPTY_BOARD_NM,
  };
  const spanX = Math.max(bounds.maxX - bounds.minX, NM_PER_MM);
  const spanY = Math.max(bounds.maxY - bounds.minY, NM_PER_MM);
  const width = Math.max(1, size.width - 2 * marginPx);
  const height = Math.max(1, size.height - 2 * marginPx);
  const zoom = clampZoom(
    Math.min(width / (spanX * pxPerNm(1)), height / (spanY * pxPerNm(1))),
  );
  const scale = pxPerNm(zoom);
  const centerX = (bounds.minX + bounds.maxX) / 2;
  const centerY = (bounds.minY + bounds.maxY) / 2;
  return {
    zoom,
    originX: size.width / 2 - centerX * scale,
    originY: size.height / 2 - centerY * scale,
  };
}

/** The topmost footprint whose bounds contain `world`: the last drawn wins. */
export function findFootprintAt(
  footprints: BoardFootprint[],
  world: ScreenPoint,
): BoardFootprint | null {
  for (let index = footprints.length - 1; index >= 0; index--) {
    const footprint = footprints[index];
    if (!footprint) {
      continue;
    }
    const { min, max } = footprint.bounds;
    if (
      world.x >= min[0] &&
      world.x <= max[0] &&
      world.y >= min[1] &&
      world.y <= max[1]
    ) {
      return footprint;
    }
  }
  return null;
}

/**
 * The corner radius a pad is drawn with, in nanometers: circles and ovals are
 * fully rounded, rounded rectangles use their own radius, and every other
 * shape has square corners.
 */
export function padCornerRadius(pad: BoardPad): number {
  const [width, height] = pad.size;
  switch (pad.shape) {
    case "circle":
    case "oval":
      return Math.min(width, height) / 2;
    case "round_rect":
      return Math.min(pad.cornerRadiusNm, Math.min(width, height) / 2);
    case "rect":
    case "trapezoid":
    case "custom":
      return 0;
  }
}

/** `footprint` moved by (dx, dy): its origin, pads, and bounds. */
export function offsetFootprint(
  footprint: BoardFootprint,
  dx: number,
  dy: number,
): BoardFootprint {
  const move = ([x, y]: Xy): Xy => [x + dx, y + dy];
  return {
    ...footprint,
    position: move(footprint.position),
    pads: footprint.pads.map((pad) => ({
      ...pad,
      position: move(pad.position),
    })),
    bounds: {
      min: move(footprint.bounds.min),
      max: move(footprint.bounds.max),
    },
  };
}

/** The key of a board point for exact matching. */
function pointKey([x, y]: Xy): string {
  return `${x},${y}`;
}

/**
 * The ratsnest with every end on a pad of a footprint in `uuids` moved by
 * `delta`, for drawing while those footprints are dragged. Ends on other
 * footprints stay where they are.
 */
export function shiftRatsnest(
  lines: RatsnestSegment[],
  footprints: BoardFootprint[],
  uuids: string[],
  delta: ScreenPoint,
): RatsnestSegment[] {
  if (uuids.length === 0 || (delta.x === 0 && delta.y === 0)) {
    return lines;
  }
  const dragged = new Set(uuids);
  const pads = new Set<string>();
  for (const footprint of footprints) {
    if (dragged.has(footprint.uuid)) {
      for (const pad of footprint.pads) {
        pads.add(pointKey(pad.position));
      }
    }
  }
  const shift = (point: Xy): Xy =>
    pads.has(pointKey(point))
      ? [point[0] + delta.x, point[1] + delta.y]
      : point;
  return lines.map((line) => ({
    ...line,
    from: shift(line.from),
    to: shift(line.to),
  }));
}
