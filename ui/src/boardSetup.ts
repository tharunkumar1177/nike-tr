/**
 * The model behind the Board Outline and Layer Stackup dialogs: the current
 * outline read as an editable rectangle or polygon, the polygon checks the
 * core applies, and a preview of KiCad's default stackup for a copper layer
 * count. Lengths are board nanometers with Y down.
 */
import type { BoardFootprint } from "./bindings/BoardFootprint";
import type { OutlineGraphic } from "./bindings/OutlineGraphic";
import type { Xy } from "./bindings/Xy";
import { boardBounds } from "./board";
import { NM_PER_MM } from "./viewport";

/** The copper layer counts a board can have. */
export const COPPER_LAYER_COUNTS = [2, 4, 6, 8];
/** The most corners an outline can have. */
export const MAX_OUTLINE_POINTS = 1024;

const BOARD_THICKNESS_NM = 1_600_000;
const COPPER_THICKNESS_NM = 35_000;
const MASK_THICKNESS_NM = 10_000;
/** The side of the square outline offered for an empty board. */
const DEFAULT_OUTLINE_NM = 100 * NM_PER_MM;
/** The margin around the footprints of a board that has no outline yet. */
const FOOTPRINT_MARGIN_NM = 10 * NM_PER_MM;
const MM_PATTERN = /^[+-]?(\d+(\.\d*)?|\.\d+)$/;

/** The outline as the Board Outline dialog edits it. */
export interface OutlineDraft {
  mode: "rectangle" | "polygon";
  /** The rectangle's corner with the smallest coordinates. */
  origin: Xy;
  width: number;
  height: number;
  /** The polygon's corners, without a closing repeat of the first. */
  points: Xy[];
  /** The current outline has shapes the editor can't keep, such as arcs. */
  replacesCurves: boolean;
}

/** An axis-aligned rectangle by its smallest corner and size. */
interface CornerBox {
  origin: Xy;
  width: number;
  height: number;
}

/** One row of the stackup preview, from the top of the board. */
export interface StackupRow {
  layer: string;
  type: string;
  material: string | null;
  thicknessNm: number;
  epsilonR: number | null;
  lossTangent: number | null;
}

/** Parses a length in millimeters to whole nanometers, or `null`. */
export function parseMm(text: string): number | null {
  const trimmed = text.trim();
  if (!MM_PATTERN.test(trimmed)) {
    return null;
  }
  return Math.round(Number(trimmed) * NM_PER_MM);
}

/** The corners of a rectangle, clockwise on screen from `origin`. */
export function rectangleCorners(
  origin: Xy,
  width: number,
  height: number,
): Xy[] {
  const [x, y] = origin;
  return [
    [x, y],
    [x + width, y],
    [x + width, y + height],
    [x, y + height],
  ];
}

/** Whether two points are the same. */
function samePoint(a: Xy, b: Xy): boolean {
  return a[0] === b[0] && a[1] === b[1];
}

/** Whether either end of `line` is `point`. */
function touches(line: [Xy, Xy], point: Xy): boolean {
  return samePoint(line[0], point) || samePoint(line[1], point);
}

/** `points` without consecutive repeats or a closing repeat of the first. */
function distinctCorners(points: Xy[]): Xy[] {
  const corners: Xy[] = [];
  for (const point of points) {
    const last = corners[corners.length - 1];
    if (!last || !samePoint(last, point)) {
      corners.push(point);
    }
  }
  while (corners.length > 1) {
    const first = corners[0];
    const last = corners[corners.length - 1];
    if (!first || !last || !samePoint(first, last)) {
      break;
    }
    corners.pop();
  }
  return corners;
}

/**
 * The corners of an outline drawn as straight lines that join end to end into
 * one closed loop, or `null` when they don't.
 */
function lineLoop(outline: OutlineGraphic[]): Xy[] | null {
  const lines: [Xy, Xy][] = [];
  for (const graphic of outline) {
    if (graphic.geometry.type !== "line") {
      return null;
    }
    lines.push([graphic.geometry.start, graphic.geometry.end]);
  }
  const first = lines.shift();
  if (!first || lines.length < 2) {
    return null;
  }
  const corners: Xy[] = [first[0]];
  let end = first[1];
  while (lines.length > 0) {
    const index = lines.findIndex((line) => touches(line, end));
    const next = lines[index];
    if (!next) {
      return null;
    }
    lines.splice(index, 1);
    corners.push(end);
    end = samePoint(next[0], end) ? next[1] : next[0];
  }
  return samePoint(end, first[0]) ? corners : null;
}

/** The corners of the current outline when the polygon editor can hold it. */
function editableCorners(outline: OutlineGraphic[]): Xy[] | null {
  const [only] = outline;
  if (outline.length === 1 && only?.geometry.type === "polygon") {
    return distinctCorners(only.geometry.points);
  }
  return lineLoop(outline);
}

/** The box around `points`. */
function cornerBox(points: Xy[]): CornerBox {
  const xs = points.map(([x]) => x);
  const ys = points.map(([, y]) => y);
  const minX = Math.min(...xs);
  const minY = Math.min(...ys);
  return {
    origin: [minX, minY],
    width: Math.max(...xs) - minX,
    height: Math.max(...ys) - minY,
  };
}

/**
 * The outline the dialog opens with. A rectangle opens as one; a polygon, or
 * lines that close into one loop, open as a polygon. Any other outline opens
 * as the rectangle around it, flagged as replaced. A board without an outline
 * gets the rectangle around its footprints with a 10 mm margin, or a 100 mm
 * square at the origin.
 */
export function outlineDraft(
  outline: OutlineGraphic[],
  footprints: BoardFootprint[],
): OutlineDraft {
  const [only] = outline;
  if (outline.length === 1 && only?.geometry.type === "rect") {
    const box = cornerBox([only.geometry.start, only.geometry.end]);
    return {
      mode: "rectangle",
      ...box,
      points: rectangleCorners(box.origin, box.width, box.height),
      replacesCurves: false,
    };
  }
  const corners = editableCorners(outline);
  if (corners && corners.length >= 3) {
    return {
      mode: "polygon",
      ...cornerBox(corners),
      points: corners,
      replacesCurves: false,
    };
  }
  let box: CornerBox = {
    origin: [0, 0],
    width: DEFAULT_OUTLINE_NM,
    height: DEFAULT_OUTLINE_NM,
  };
  const bare = outline.map((graphic) => ({ ...graphic, widthNm: 0 }));
  const outlineBox = boardBounds([], bare);
  const footprintBox = boardBounds(footprints, []);
  if (outlineBox) {
    box = cornerBox([
      [outlineBox.minX, outlineBox.minY],
      [outlineBox.maxX, outlineBox.maxY],
    ]);
  } else if (footprintBox) {
    const margin = FOOTPRINT_MARGIN_NM;
    box = cornerBox([
      [footprintBox.minX - margin, footprintBox.minY - margin],
      [footprintBox.maxX + margin, footprintBox.maxY + margin],
    ]);
  }
  return {
    mode: "rectangle",
    ...box,
    points: rectangleCorners(box.origin, box.width, box.height),
    replacesCurves: outline.length > 0,
  };
}

/** The cross product of `a - o` and `b - o`, exactly. */
function cross(o: Xy, a: Xy, b: Xy): bigint {
  const ax = BigInt(a[0] - o[0]);
  const ay = BigInt(a[1] - o[1]);
  const bx = BigInt(b[0] - o[0]);
  const by = BigInt(b[1] - o[1]);
  return ax * by - ay * bx;
}

/** The dot product of `a - o` and `b - o`, exactly. */
function dot(o: Xy, a: Xy, b: Xy): bigint {
  const ax = BigInt(a[0] - o[0]);
  const ay = BigInt(a[1] - o[1]);
  const bx = BigInt(b[0] - o[0]);
  const by = BigInt(b[1] - o[1]);
  return ax * bx + ay * by;
}

/** The sign of `value`: -1, 0, or 1. */
function sign(value: bigint): number {
  if (value === 0n) {
    return 0;
  }
  return value < 0n ? -1 : 1;
}

/** Whether `p`, collinear with `a` and `b`, lies on the segment `ab`. */
function within(a: Xy, b: Xy, p: Xy): boolean {
  return (
    Math.min(a[0], b[0]) <= p[0] &&
    p[0] <= Math.max(a[0], b[0]) &&
    Math.min(a[1], b[1]) <= p[1] &&
    p[1] <= Math.max(a[1], b[1])
  );
}

/** Whether the segments `ab` and `cd` share any point. */
function segmentsTouch(a: Xy, b: Xy, c: Xy, d: Xy): boolean {
  const d1 = sign(cross(c, d, a));
  const d2 = sign(cross(c, d, b));
  const d3 = sign(cross(a, b, c));
  const d4 = sign(cross(a, b, d));
  if (d1 * d2 < 0 && d3 * d4 < 0) {
    return true;
  }
  return (
    (d1 === 0 && within(c, d, a)) ||
    (d2 === 0 && within(c, d, b)) ||
    (d3 === 0 && within(a, b, c)) ||
    (d4 === 0 && within(a, b, d))
  );
}

/** Whether two edges of the polygon meet anywhere but their shared corner. */
function selfIntersects(corners: Xy[]): boolean {
  const count = corners.length;
  const at = (index: number): Xy => corners[index % count] ?? [0, 0];
  for (let i = 0; i < count; i++) {
    const a = at(i);
    const b = at(i + 1);
    const next = at(i + 2);
    if (cross(b, a, next) === 0n && dot(b, a, next) > 0n) {
      return true;
    }
    for (let j = i + 2; j < count; j++) {
      if (i === 0 && j === count - 1) {
        continue;
      }
      if (segmentsTouch(a, b, at(j), at(j + 1))) {
        return true;
      }
    }
  }
  return false;
}

/**
 * Whether every corner lies on one line, so the polygon encloses no area. Any
 * other polygon with zero area crosses itself.
 */
function collinear(corners: Xy[]): boolean {
  const [first, second] = corners;
  if (first === undefined || second === undefined) {
    return true;
  }
  return corners.every((point) => cross(first, second, point) === 0n);
}

/**
 * Why `points` can't be the board outline, or `null` when they form one
 * simple closed polygon.
 */
export function polygonProblem(points: Xy[]): string | null {
  const corners = distinctCorners(points);
  if (corners.length < 3) {
    return "The outline needs at least 3 distinct points.";
  }
  if (collinear(corners)) {
    return "The outline has zero area.";
  }
  if (selfIntersects(corners)) {
    return "The outline crosses itself.";
  }
  return null;
}

/** The copper layer names of a board with `copper` layers, top to bottom. */
function copperNames(copper: number): string[] {
  const names = ["F.Cu"];
  for (let inner = 1; inner <= copper - 2; inner++) {
    names.push(`In${inner}.Cu`);
  }
  names.push("B.Cu");
  return names;
}

/**
 * KiCad's default stackup for `copper` copper layers, as the core writes it:
 * 1.6 mm in total, 0.035 mm copper, 0.01 mm masks, and FR4 dielectrics
 * sharing the rest. Two layers have one core; more alternate prepreg and
 * core, starting with prepreg.
 */
export function stackupPreview(copper: number): StackupRow[] {
  const names = copperNames(copper);
  const dielectrics = Math.max(names.length - 1, 1);
  const remaining =
    BOARD_THICKNESS_NM -
    COPPER_THICKNESS_NM * names.length -
    2 * MASK_THICKNESS_NM;
  const dielectricNm = Math.floor(
    (remaining + Math.floor(dielectrics / 2)) / dielectrics,
  );
  const mask = (layer: string, type: string): StackupRow => ({
    layer,
    type,
    material: null,
    thicknessNm: MASK_THICKNESS_NM,
    epsilonR: null,
    lossTangent: null,
  });
  const rows = [mask("F.Mask", "Top Solder Mask")];
  names.forEach((name, index) => {
    rows.push({
      layer: name,
      type: "copper",
      material: null,
      thicknessNm: COPPER_THICKNESS_NM,
      epsilonR: null,
      lossTangent: null,
    });
    if (index === names.length - 1) {
      return;
    }
    const number = index + 1;
    rows.push({
      layer: `dielectric ${number}`,
      type: names.length === 2 || number % 2 === 0 ? "core" : "prepreg",
      material: "FR4",
      thicknessNm: dielectricNm,
      epsilonR: 4.5,
      lossTangent: 0.02,
    });
  });
  rows.push(mask("B.Mask", "Bottom Solder Mask"));
  return rows;
}
