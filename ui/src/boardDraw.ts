/**
 * Draws the board scene onto the board canvas: the Edge.Cuts outline, back
 * then front footprints with their pads and Reference and Value text, the
 * ratsnest, and the hover and selection boxes. The context arrives in screen
 * space (CSS pixels); world geometry is in board nanometers with Y down.
 */
import type { BoardFootprint } from "./bindings/BoardFootprint";
import type { BoardPad } from "./bindings/BoardPad";
import type { OutlineGraphic } from "./bindings/OutlineGraphic";
import type { RatsnestSegment } from "./bindings/RatsnestSegment";
import { offsetFootprint, padCornerRadius } from "./board";
import { MIN_LINE_PX } from "./drawShapes";
import { arcThrough } from "./libraryPreview";
import {
  NM_PER_MM,
  type ScreenPoint,
  type Viewport,
  pxPerNm,
  worldToScreen,
} from "./viewport";

export interface BoardScene {
  footprints: BoardFootprint[];
  outline: OutlineGraphic[];
  /** `null` while the ratsnest is hidden. */
  ratsnest: RatsnestSegment[] | null;
  selectedUuid: string | null;
  hoverUuid: string | null;
  /** In-progress drag: the local preview offset of one footprint. */
  drag: { uuid: string; dx: number; dy: number } | null;
}

/** Reference and Value text height on the board. */
const TEXT_NM = NM_PER_MM;
/** Gap between a footprint's bounds and its Reference or Value text. */
const TEXT_GAP_NM = NM_PER_MM / 2;
/** Text smaller than this on screen is left out. */
const MIN_TEXT_PX = 6;
/** Pads with no readable size still show as a dot this wide. */
const MIN_PAD_PX = 2;
const HOVER_ALPHA = 0.6;
const SELECTION_DASH = [4, 4];
/** Clearance between a footprint's bounds and its hover or selection box. */
const SELECTION_PADDING_PX = 4;

/** The value of a CSS custom property on the canvas. */
function token(styles: CSSStyleDeclaration, name: string): string {
  return styles.getPropertyValue(name).trim();
}

/** Traces a `width` by `height` rectangle centered on the origin with corners rounded by `radius`. */
function traceRoundedRect(
  context: CanvasRenderingContext2D,
  width: number,
  height: number,
  radius: number,
) {
  const x = -width / 2;
  const y = -height / 2;
  const r = Math.max(0, Math.min(radius, width / 2, height / 2));
  context.beginPath();
  context.moveTo(x + r, y);
  context.lineTo(x + width - r, y);
  context.arcTo(x + width, y, x + width, y + r, r);
  context.lineTo(x + width, y + height - r);
  context.arcTo(x + width, y + height, x + width - r, y + height, r);
  context.lineTo(x + r, y + height);
  context.arcTo(x, y + height, x, y + height - r, r);
  context.lineTo(x, y + r);
  context.arcTo(x, y, x + r, y, r);
  context.closePath();
}

/** Fills one pad in `color`, then punches its drill hole in `holeColor`. */
function drawPad(
  context: CanvasRenderingContext2D,
  viewport: Viewport,
  pad: BoardPad,
  color: string,
  holeColor: string,
) {
  const scale = pxPerNm(viewport.zoom);
  const at = worldToScreen(viewport, pad.position[0], pad.position[1]);
  const width = Math.max(pad.size[0] * scale, MIN_PAD_PX);
  const height = Math.max(pad.size[1] * scale, MIN_PAD_PX);
  context.save();
  context.translate(at.x, at.y);
  // Board rotation is counterclockwise on screen; canvas rotation is clockwise.
  context.rotate((-pad.rotation / 10) * (Math.PI / 180));
  traceRoundedRect(context, width, height, padCornerRadius(pad) * scale);
  context.fillStyle = color;
  context.fill();
  if (pad.drill) {
    const [drillX, drillY] = pad.drill;
    const holeWidth = drillX * scale;
    const holeHeight = drillY * scale;
    traceRoundedRect(
      context,
      holeWidth,
      holeHeight,
      Math.min(holeWidth, holeHeight) / 2,
    );
    context.fillStyle = holeColor;
    context.fill();
  }
  context.restore();
}

/** Strokes one Edge.Cuts graphic. */
function drawOutlineGraphic(
  context: CanvasRenderingContext2D,
  viewport: Viewport,
  graphic: OutlineGraphic,
) {
  const scale = pxPerNm(viewport.zoom);
  const screen = ([x, y]: [number, number]): ScreenPoint =>
    worldToScreen(viewport, x, y);
  const { geometry } = graphic;
  context.lineWidth = Math.max(graphic.widthNm * scale, MIN_LINE_PX);
  context.beginPath();
  switch (geometry.type) {
    case "line": {
      const start = screen(geometry.start);
      const end = screen(geometry.end);
      context.moveTo(start.x, start.y);
      context.lineTo(end.x, end.y);
      break;
    }
    case "arc": {
      const start = screen(geometry.start);
      const mid = screen(geometry.mid);
      const end = screen(geometry.end);
      const arc = arcThrough(
        [start.x, start.y],
        [mid.x, mid.y],
        [end.x, end.y],
      );
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
        context.moveTo(start.x, start.y);
        context.lineTo(end.x, end.y);
      }
      break;
    }
    case "rect": {
      const start = screen(geometry.start);
      const end = screen(geometry.end);
      context.rect(start.x, start.y, end.x - start.x, end.y - start.y);
      break;
    }
    case "circle": {
      const center = screen(geometry.center);
      const end = screen(geometry.end);
      const radius = Math.hypot(end.x - center.x, end.y - center.y);
      context.moveTo(center.x + radius, center.y);
      context.arc(center.x, center.y, radius, 0, 2 * Math.PI);
      break;
    }
    case "polygon": {
      geometry.points.forEach((point, index) => {
        const at = screen(point);
        if (index === 0) {
          context.moveTo(at.x, at.y);
        } else {
          context.lineTo(at.x, at.y);
        }
      });
      context.closePath();
      break;
    }
  }
  context.stroke();
}

/** The screen box around a footprint's bounds, widened by `paddingPx`. */
function boundsRect(
  viewport: Viewport,
  footprint: BoardFootprint,
  paddingPx: number,
): { x: number; y: number; width: number; height: number } {
  const { min, max } = footprint.bounds;
  const topLeft = worldToScreen(viewport, min[0], min[1]);
  const bottomRight = worldToScreen(viewport, max[0], max[1]);
  return {
    x: topLeft.x - paddingPx,
    y: topLeft.y - paddingPx,
    width: bottomRight.x - topLeft.x + 2 * paddingPx,
    height: bottomRight.y - topLeft.y + 2 * paddingPx,
  };
}

/** Writes the Reference above and the Value below a footprint's bounds. */
function drawFootprintText(
  context: CanvasRenderingContext2D,
  viewport: Viewport,
  footprint: BoardFootprint,
  color: string,
  fontFamily: string,
) {
  const scale = pxPerNm(viewport.zoom);
  const textPx = TEXT_NM * scale;
  if (textPx < MIN_TEXT_PX) {
    return;
  }
  const { min, max } = footprint.bounds;
  const centerX = (min[0] + max[0]) / 2;
  context.save();
  context.font = `${textPx}px ${fontFamily}`;
  context.textAlign = "center";
  context.fillStyle = color;
  if (footprint.reference) {
    const at = worldToScreen(viewport, centerX, min[1] - TEXT_GAP_NM);
    context.textBaseline = "bottom";
    context.fillText(footprint.reference, at.x, at.y);
  }
  if (footprint.value) {
    const at = worldToScreen(viewport, centerX, max[1] + TEXT_GAP_NM);
    context.textBaseline = "top";
    context.fillText(footprint.value, at.x, at.y);
  }
  context.restore();
}

/** Draws the outline, footprints, ratsnest, and the hover and selection boxes. */
export function drawBoardScene(
  context: CanvasRenderingContext2D,
  viewport: Viewport,
  scene: BoardScene,
) {
  const styles = getComputedStyle(context.canvas);
  const edge = token(styles, "--color-board-edge");
  const front = token(styles, "--color-pad-front");
  const back = token(styles, "--color-pad-back");
  const ratsnest = token(styles, "--color-ratsnest");
  const focus = token(styles, "--color-focus");
  const text = token(styles, "--color-text-primary");
  const hole = token(styles, "--color-canvas");
  const fontFamily = token(styles, "--font-family");

  context.save();
  context.lineCap = "round";
  context.lineJoin = "round";
  context.strokeStyle = edge;
  for (const graphic of scene.outline) {
    drawOutlineGraphic(context, viewport, graphic);
  }
  context.restore();

  const drag = scene.drag;
  const placed = scene.footprints.map((footprint) =>
    drag?.uuid === footprint.uuid
      ? offsetFootprint(footprint, drag.dx, drag.dy)
      : footprint,
  );
  const drawn = [
    ...placed.filter((footprint) => footprint.side === "back"),
    ...placed.filter((footprint) => footprint.side === "front"),
  ];
  for (const footprint of drawn) {
    const selected = footprint.uuid === scene.selectedUuid;
    const sideColor = footprint.side === "front" ? front : back;
    const padColor = selected ? focus : sideColor;
    for (const pad of footprint.pads) {
      drawPad(context, viewport, pad, padColor, hole);
    }
    const textColor = selected
      ? focus
      : footprint.side === "front"
        ? text
        : back;
    drawFootprintText(context, viewport, footprint, textColor, fontFamily);
  }

  if (scene.ratsnest && scene.ratsnest.length > 0) {
    context.save();
    context.strokeStyle = ratsnest;
    context.lineWidth = 1;
    context.beginPath();
    for (const line of scene.ratsnest) {
      const from = worldToScreen(viewport, line.from[0], line.from[1]);
      const to = worldToScreen(viewport, line.to[0], line.to[1]);
      context.moveTo(from.x, from.y);
      context.lineTo(to.x, to.y);
    }
    context.stroke();
    context.restore();
  }

  for (const footprint of drawn) {
    const selected = footprint.uuid === scene.selectedUuid;
    const hovered = footprint.uuid === scene.hoverUuid;
    if (!selected && !hovered) {
      continue;
    }
    const box = boundsRect(viewport, footprint, SELECTION_PADDING_PX);
    context.save();
    context.lineWidth = 1;
    if (selected) {
      context.strokeStyle = focus;
      context.setLineDash(SELECTION_DASH);
    } else {
      context.strokeStyle = footprint.side === "front" ? front : back;
      context.globalAlpha = HOVER_ALPHA;
    }
    context.strokeRect(box.x, box.y, box.width, box.height);
    context.restore();
  }
}
