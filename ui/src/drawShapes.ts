import type { Fill } from "./bindings/Fill";
import type { Shape } from "./bindings/Shape";
import { arcThrough } from "./libraryPreview";

/** Thinnest line drawn, in CSS pixels, so zero-width outlines stay visible. */
export const MIN_LINE_PX = 1;

/**
 * Traces `shape` onto the current path. Returns `false` for degenerate
 * shapes that have nothing to draw.
 */
function traceShapePath(
  context: CanvasRenderingContext2D,
  shape: Shape,
): boolean {
  switch (shape.type) {
    case "polyline":
    case "polygon": {
      const [first, ...rest] = shape.points;
      if (!first) {
        return false;
      }
      context.moveTo(first[0], first[1]);
      for (const [x, y] of rest) {
        context.lineTo(x, y);
      }
      if (shape.type === "polygon") {
        context.closePath();
      }
      return true;
    }
    case "rect":
      context.rect(
        shape.start[0],
        shape.start[1],
        shape.end[0] - shape.start[0],
        shape.end[1] - shape.start[1],
      );
      return true;
    case "circle":
      context.moveTo(shape.center[0] + shape.radius, shape.center[1]);
      context.arc(
        shape.center[0],
        shape.center[1],
        shape.radius,
        0,
        2 * Math.PI,
      );
      return true;
    case "arc": {
      const arc = arcThrough(shape.start, shape.mid, shape.end);
      context.moveTo(shape.start[0], shape.start[1]);
      if (arc) {
        context.arc(
          arc.centerX,
          arc.centerY,
          arc.radius,
          arc.startAngle,
          arc.endAngle,
          arc.anticlockwise,
        );
      } else {
        context.lineTo(shape.end[0], shape.end[1]);
      }
      return true;
    }
    case "bezier": {
      const [p0, p1, p2, p3] = shape.points;
      if (!p0 || !p1 || !p2 || !p3) {
        return false;
      }
      context.moveTo(p0[0], p0[1]);
      context.bezierCurveTo(p1[0], p1[1], p2[0], p2[1], p3[0], p3[1]);
      return true;
    }
  }
}

/** Strokes and fills `shape` with the given symbol palette colors. */
export function drawShape(
  context: CanvasRenderingContext2D,
  shape: Shape,
  stroke: string,
  bodyFill: string,
  minWidth: number,
) {
  context.beginPath();
  if (!traceShapePath(context, shape)) {
    return;
  }
  const fills: Record<Fill, string | null> = {
    none: null,
    outline: stroke,
    background: bodyFill,
  };
  const fill = fills[shape.fill];
  if (fill !== null) {
    context.fillStyle = fill;
    context.fill();
  }
  context.strokeStyle = stroke;
  context.lineWidth = Math.max(shape.width, minWidth);
  context.stroke();
}
