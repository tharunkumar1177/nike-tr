/**
 * Geometry for placed schematic symbols: poses, snapping, transformed
 * bounds, and hit-testing. All coordinates are nanometers with Y down.
 */
import type { MirrorAxis } from "./bindings/MirrorAxis";
import type { SymbolInstance } from "./bindings/SymbolInstance";
import type { SymbolPreview } from "./bindings/SymbolPreview";
import { type Bounds, symbolBounds } from "./libraryPreview";
import type { ScreenPoint } from "./viewport";

/** How a symbol sits on the schematic: origin, rotation, and mirror. */
export interface Pose {
  xNm: number;
  yNm: number;
  /** Tenths of a degree, counterclockwise on screen. */
  rotation: number;
  mirror: MirrorAxis | null;
}

export function instancePose(instance: SymbolInstance): Pose {
  return {
    xNm: instance.xNm,
    yNm: instance.yNm,
    rotation: instance.rotation,
    mirror: instance.mirror,
  };
}

/** The signs a mirror applies to symbol coordinates: `x` flips horizontally, `y` vertically. */
export function mirrorFactors(mirror: MirrorAxis | null): {
  fx: 1 | -1;
  fy: 1 | -1;
} {
  return { fx: mirror === "x" ? -1 : 1, fy: mirror === "y" ? -1 : 1 };
}

/** Rotates a point counterclockwise on screen (Y grows down). */
export function rotatePoint(
  x: number,
  y: number,
  decidegrees: number,
): ScreenPoint {
  const radians = (decidegrees / 10) * (Math.PI / 180);
  const cos = Math.cos(radians);
  const sin = Math.sin(radians);
  return { x: x * cos + y * sin, y: -x * sin + y * cos };
}

/** Maps a symbol-local point to schematic coordinates: mirror, rotate, translate. */
export function posePoint(pose: Pose, x: number, y: number): ScreenPoint {
  const { fx, fy } = mirrorFactors(pose.mirror);
  const rotated = rotatePoint(x * fx, y * fy, pose.rotation);
  return { x: pose.xNm + rotated.x, y: pose.yNm + rotated.y };
}

/** The axis-aligned box around `bounds` after `pose` transforms its corners. */
export function poseBounds(pose: Pose, bounds: Bounds): Bounds {
  const corners = [
    posePoint(pose, bounds.minX, bounds.minY),
    posePoint(pose, bounds.maxX, bounds.minY),
    posePoint(pose, bounds.minX, bounds.maxY),
    posePoint(pose, bounds.maxX, bounds.maxY),
  ];
  let minX = Infinity;
  let minY = Infinity;
  let maxX = -Infinity;
  let maxY = -Infinity;
  for (const corner of corners) {
    minX = Math.min(minX, corner.x);
    minY = Math.min(minY, corner.y);
    maxX = Math.max(maxX, corner.x);
    maxY = Math.max(maxY, corner.y);
  }
  return { minX, minY, maxX, maxY };
}

/** The schematic-space bounds of a symbol drawn at `pose`, or `null` when empty. */
export function instanceBounds(
  preview: SymbolPreview,
  pose: Pose,
): Bounds | null {
  const local = symbolBounds(preview);
  return local === null ? null : poseBounds(pose, local);
}

/** True when `point` lies inside the symbol's transformed bounds. */
export function hitTest(
  preview: SymbolPreview,
  pose: Pose,
  point: ScreenPoint,
): boolean {
  const bounds = instanceBounds(preview, pose);
  return (
    bounds !== null &&
    point.x >= bounds.minX &&
    point.x <= bounds.maxX &&
    point.y >= bounds.minY &&
    point.y <= bounds.maxY
  );
}

/**
 * The topmost instance under `point`: the last one in drawing order whose
 * transformed bounds contain it. Instances without a definition are skipped.
 */
export function findInstanceAt(
  instances: SymbolInstance[],
  definitions: { [key in string]: SymbolPreview },
  point: ScreenPoint,
): SymbolInstance | null {
  for (let index = instances.length - 1; index >= 0; index--) {
    const instance = instances[index];
    if (!instance) {
      continue;
    }
    const preview = definitions[instance.libId];
    if (preview && hitTest(preview, instancePose(instance), point)) {
      return instance;
    }
  }
  return null;
}

/** Snaps a coordinate to the nearest grid multiple. */
export function snapToGrid(nm: number, gridNm: number): number {
  if (!(gridNm > 0)) {
    return nm;
  }
  const snapped = Math.round(nm / gridNm) * gridNm;
  // A snapped coordinate of negative zero reads as zero.
  return snapped === 0 ? 0 : snapped;
}

/** Normalizes quarter-turn steps to `[0, 4)`. */
export function normalizeSteps(steps: number): number {
  return ((steps % 4) + 4) % 4;
}

/** The grid-snapped displacement of a drag from `start` to `current`. */
export function snappedDragDelta(
  start: ScreenPoint,
  current: ScreenPoint,
  gridNm: number,
): ScreenPoint {
  return {
    x: snapToGrid(current.x - start.x, gridNm),
    y: snapToGrid(current.y - start.y, gridNm),
  };
}
