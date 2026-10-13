import { useEffect, useRef, useState } from "react";
import type { Fill } from "../bindings/Fill";
import type { FootprintPreview } from "../bindings/FootprintPreview";
import type { Pad } from "../bindings/Pad";
import type { Shape } from "../bindings/Shape";
import type { SymbolPreview } from "../bindings/SymbolPreview";
import {
  type Bounds,
  arcThrough,
  fitBounds,
  footprintBounds,
  symbolBounds,
} from "../libraryPreview";
import type { Size } from "../viewport";

export type PreviewGeometry =
  | { kind: "symbol"; preview: SymbolPreview }
  | { kind: "footprint"; preview: FootprintPreview };

interface Palette {
  background: string;
  stroke: string;
  bodyFill: string;
  pin: string;
}

/** Thinnest line drawn, in CSS pixels, so zero-width outlines stay visible. */
const MIN_LINE_PX = 1;

function tracePath(context: CanvasRenderingContext2D, shape: Shape): boolean {
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

function drawShape(
  context: CanvasRenderingContext2D,
  shape: Shape,
  stroke: string,
  bodyFill: string,
  minWidth: number,
) {
  context.beginPath();
  if (!tracePath(context, shape)) {
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

function tracePadOutline(
  context: CanvasRenderingContext2D,
  pad: Pad,
  width: number,
  height: number,
) {
  const x = -width / 2;
  const y = -height / 2;
  switch (pad.shape) {
    case "circle":
      context.arc(0, 0, Math.min(width, height) / 2, 0, 2 * Math.PI);
      return;
    case "oval":
      context.roundRect(x, y, width, height, Math.min(width, height) / 2);
      return;
    case "round_rect":
      context.roundRect(x, y, width, height, pad.corner_radius);
      return;
    case "rect":
    case "trapezoid":
    case "custom":
      context.rect(x, y, width, height);
      return;
  }
}

function drawPad(
  context: CanvasRenderingContext2D,
  pad: Pad,
  palette: Palette,
) {
  context.save();
  context.translate(pad.center[0], pad.center[1]);
  // Pad rotation is counterclockwise on screen; canvas rotation is clockwise.
  context.rotate((-pad.rotation / 10) * (Math.PI / 180));
  if (pad.kind !== "non_plated_hole") {
    context.beginPath();
    tracePadOutline(context, pad, pad.size[0], pad.size[1]);
    context.fillStyle = palette.pin;
    context.fill();
  }
  const drill = pad.drill ?? (pad.kind === "non_plated_hole" ? pad.size : null);
  if (drill) {
    const [width, height] = drill;
    context.beginPath();
    context.roundRect(
      -width / 2,
      -height / 2,
      width,
      height,
      Math.min(width, height) / 2,
    );
    context.fillStyle = palette.background;
    context.fill();
  }
  context.restore();
}

function draw(
  canvas: HTMLCanvasElement,
  size: Size,
  geometry: PreviewGeometry,
) {
  const context = canvas.getContext("2d");
  if (!context) {
    return;
  }
  const ratio = window.devicePixelRatio || 1;
  canvas.width = Math.round(size.width * ratio);
  canvas.height = Math.round(size.height * ratio);

  const styles = getComputedStyle(canvas);
  const color = (name: string) => styles.getPropertyValue(name).trim();
  const palette: Palette = {
    background: color("--color-canvas"),
    stroke: color("--color-primary"),
    bodyFill: color("--color-surface-elevated"),
    pin: color("--color-text-error"),
  };

  context.setTransform(ratio, 0, 0, ratio, 0, 0);
  context.fillStyle = palette.background;
  context.fillRect(0, 0, size.width, size.height);

  const bounds: Bounds | null =
    geometry.kind === "symbol"
      ? symbolBounds(geometry.preview)
      : footprintBounds(geometry.preview);
  if (!bounds) {
    return;
  }
  const fit = fitBounds(bounds, size.width, size.height);
  context.setTransform(
    ratio * fit.scale,
    0,
    0,
    ratio * fit.scale,
    ratio * fit.offsetX,
    ratio * fit.offsetY,
  );
  const minWidth = MIN_LINE_PX / fit.scale;
  context.lineCap = "round";
  context.lineJoin = "round";

  if (geometry.kind === "symbol") {
    for (const shape of geometry.preview.shapes) {
      drawShape(context, shape, palette.stroke, palette.bodyFill, minWidth);
    }
    context.beginPath();
    for (const pin of geometry.preview.pins) {
      context.moveTo(pin.connection[0], pin.connection[1]);
      context.lineTo(pin.body[0], pin.body[1]);
    }
    context.strokeStyle = palette.pin;
    context.lineWidth = minWidth * 1.5;
    context.stroke();
  } else {
    for (const graphic of geometry.preview.graphics) {
      drawShape(
        context,
        graphic.shape,
        palette.stroke,
        palette.bodyFill,
        minWidth,
      );
    }
    for (const pad of geometry.preview.pads) {
      drawPad(context, pad, palette);
    }
  }
}

interface LibraryPreviewProps {
  label: string;
  geometry: PreviewGeometry;
}

/** Draws a symbol or footprint scaled to fit its container. */
export function LibraryPreview({ label, geometry }: LibraryPreviewProps) {
  const containerRef = useRef<HTMLDivElement>(null);
  const canvasRef = useRef<HTMLCanvasElement>(null);
  const [size, setSize] = useState<Size>({ width: 0, height: 0 });

  useEffect(() => {
    const container = containerRef.current;
    if (!container || typeof ResizeObserver === "undefined") {
      return;
    }
    const observer = new ResizeObserver(([entry]) => {
      if (entry) {
        setSize({
          width: entry.contentRect.width,
          height: entry.contentRect.height,
        });
      }
    });
    observer.observe(container);
    return () => observer.disconnect();
  }, []);

  useEffect(() => {
    const canvas = canvasRef.current;
    if (canvas && size.width > 0 && size.height > 0) {
      draw(canvas, size, geometry);
    }
  }, [geometry, size]);

  return (
    <div ref={containerRef} className="library-preview__graphic">
      <canvas
        ref={canvasRef}
        className="canvas__surface"
        role="img"
        aria-label={label}
      />
    </div>
  );
}
