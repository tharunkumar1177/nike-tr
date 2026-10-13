import type { FootprintPreview } from "./bindings/FootprintPreview";
import type { Shape } from "./bindings/Shape";
import type { SymbolPreview } from "./bindings/SymbolPreview";
import type { Xy } from "./bindings/Xy";

/** An axis-aligned box in nanometers. */
export interface Bounds {
  minX: number;
  minY: number;
  maxX: number;
  maxY: number;
}

class BoundsBuilder {
  private box: Bounds | null = null;

  add(x: number, y: number, pad = 0) {
    if (this.box === null) {
      this.box = {
        minX: x - pad,
        minY: y - pad,
        maxX: x + pad,
        maxY: y + pad,
      };
      return;
    }
    this.box.minX = Math.min(this.box.minX, x - pad);
    this.box.minY = Math.min(this.box.minY, y - pad);
    this.box.maxX = Math.max(this.box.maxX, x + pad);
    this.box.maxY = Math.max(this.box.maxY, y + pad);
  }

  addShape(shape: Shape) {
    const pad = shape.width / 2;
    switch (shape.type) {
      case "polyline":
      case "polygon":
      case "bezier":
        for (const [x, y] of shape.points) {
          this.add(x, y, pad);
        }
        return;
      case "rect":
        this.add(shape.start[0], shape.start[1], pad);
        this.add(shape.end[0], shape.end[1], pad);
        return;
      case "circle":
        this.add(shape.center[0], shape.center[1], shape.radius + pad);
        return;
      case "arc": {
        const arc = arcThrough(shape.start, shape.mid, shape.end);
        if (arc) {
          for (const angle of arcExtremes(arc)) {
            this.add(
              arc.centerX + arc.radius * Math.cos(angle),
              arc.centerY + arc.radius * Math.sin(angle),
              pad,
            );
          }
        }
        this.add(shape.start[0], shape.start[1], pad);
        this.add(shape.end[0], shape.end[1], pad);
        return;
      }
    }
  }

  build(): Bounds | null {
    return this.box;
  }
}

export function symbolBounds(preview: SymbolPreview): Bounds | null {
  const builder = new BoundsBuilder();
  for (const shape of preview.shapes) {
    builder.addShape(shape);
  }
  for (const pin of preview.pins) {
    builder.add(pin.connection[0], pin.connection[1]);
    builder.add(pin.body[0], pin.body[1]);
  }
  return builder.build();
}

export function footprintBounds(preview: FootprintPreview): Bounds | null {
  const builder = new BoundsBuilder();
  for (const graphic of preview.graphics) {
    builder.addShape(graphic.shape);
  }
  for (const pad of preview.pads) {
    builder.add(pad.center[0], pad.center[1], Math.hypot(...pad.size) / 2);
  }
  return builder.build();
}

export interface Fit {
  scale: number;
  offsetX: number;
  offsetY: number;
}

/** Scales and centers `bounds` in a `width` by `height` box with a margin on every side. */
export function fitBounds(
  bounds: Bounds,
  width: number,
  height: number,
  margin = 0.1,
): Fit {
  const spanX = Math.max(bounds.maxX - bounds.minX, 1);
  const spanY = Math.max(bounds.maxY - bounds.minY, 1);
  const scale = Math.min(
    (width * (1 - 2 * margin)) / spanX,
    (height * (1 - 2 * margin)) / spanY,
  );
  return {
    scale,
    offsetX: width / 2 - ((bounds.minX + bounds.maxX) / 2) * scale,
    offsetY: height / 2 - ((bounds.minY + bounds.maxY) / 2) * scale,
  };
}

/** A circular arc in Y-down coordinates, ready for `CanvasRenderingContext2D.arc`. */
export interface Arc {
  centerX: number;
  centerY: number;
  radius: number;
  startAngle: number;
  endAngle: number;
  anticlockwise: boolean;
}

const TURN = 2 * Math.PI;

function normalize(angle: number): number {
  return ((angle % TURN) + TURN) % TURN;
}

/** The arc from `start` through `mid` to `end`, or `null` if the points are collinear. */
export function arcThrough(start: Xy, mid: Xy, end: Xy): Arc | null {
  const [ax, ay] = start;
  const [bx, by] = mid;
  const [cx, cy] = end;
  const d = 2 * (ax * (by - cy) + bx * (cy - ay) + cx * (ay - by));
  if (Math.abs(d) < 1e-9) {
    return null;
  }
  const a2 = ax * ax + ay * ay;
  const b2 = bx * bx + by * by;
  const c2 = cx * cx + cy * cy;
  const centerX = (a2 * (by - cy) + b2 * (cy - ay) + c2 * (ay - by)) / d;
  const centerY = (a2 * (cx - bx) + b2 * (ax - cx) + c2 * (bx - ax)) / d;
  const startAngle = Math.atan2(ay - centerY, ax - centerX);
  const midAngle = Math.atan2(by - centerY, bx - centerX);
  const endAngle = Math.atan2(cy - centerY, cx - centerX);
  // Canvas angles increase clockwise on screen. The arc runs that way exactly
  // when `mid` comes before `end` going clockwise from `start`.
  const clockwise =
    normalize(midAngle - startAngle) < normalize(endAngle - startAngle);
  return {
    centerX,
    centerY,
    radius: Math.hypot(ax - centerX, ay - centerY),
    startAngle,
    endAngle,
    anticlockwise: !clockwise,
  };
}

/** The axis-extreme angles (0, 90, 180, 270 degrees) that `arc` sweeps through. */
function arcExtremes(arc: Arc): number[] {
  const from = arc.anticlockwise ? arc.endAngle : arc.startAngle;
  const sweep = normalize(
    arc.anticlockwise
      ? arc.startAngle - arc.endAngle
      : arc.endAngle - arc.startAngle,
  );
  const extremes: number[] = [];
  for (let quarter = 0; quarter < 4; quarter++) {
    const angle = (quarter * Math.PI) / 2;
    if (normalize(angle - from) <= sweep) {
      extremes.push(angle);
    }
  }
  return extremes;
}
