/**
 * Draws the board scene onto the board canvas: the Edge.Cuts outline, zone
 * fills, tracks from the bottom layer up, vias, back then front footprints
 * with their pads and Reference and Value text, the ratsnest, the route being
 * drawn, and the hover and selection marks. The context arrives in screen
 * space (CSS pixels); world geometry is in board nanometers with Y down.
 */
import type { BoardFootprint } from "./bindings/BoardFootprint";
import type { BoardPad } from "./bindings/BoardPad";
import type { BoardTrack } from "./bindings/BoardTrack";
import type { BoardVia } from "./bindings/BoardVia";
import type { BoardZone } from "./bindings/BoardZone";
import type { OutlineGraphic } from "./bindings/OutlineGraphic";
import type { RatsnestSegment } from "./bindings/RatsnestSegment";
import type { RoutePiece } from "./bindings/RoutePiece";
import type { Xy } from "./bindings/Xy";
import { offsetFootprint, padCornerRadius } from "./board";
import { MIN_LINE_PX } from "./drawShapes";
import { arcThrough } from "./libraryPreview";
import { layerColorToken } from "./routing";
import {
  NM_PER_MM,
  type ScreenPoint,
  type Viewport,
  pxPerNm,
  worldToScreen,
} from "./viewport";

/** The route being drawn, as the canvas previews it. */
export interface RoutePreview {
  net: string | null;
  /** The pieces fixed by clicks so far. */
  pieces: RoutePiece[];
  /** The leg from the last fixed corner to the cursor. */
  pending: RoutePiece[];
  vias: Xy[];
  widthNm: number;
  viaDiameterNm: number;
  viaDrillNm: number;
}

export interface BoardScene {
  footprints: BoardFootprint[];
  outline: OutlineGraphic[];
  /** `null` while the ratsnest is hidden. */
  ratsnest: RatsnestSegment[] | null;
  tracks: BoardTrack[];
  vias: BoardVia[];
  zones: BoardZone[];
  /** The board's copper layers, top to bottom. */
  layers: string[];
  /** The copper layers shown; tracks and fills on other layers are left out. */
  visibleLayers: ReadonlySet<string>;
  /** The selected footprint, track, or via. */
  selectedUuid: string | null;
  hoverUuid: string | null;
  /** In-progress drag: the local preview offset of one footprint. */
  drag: { uuid: string; dx: number; dy: number } | null;
  route: RoutePreview | null;
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
/** Copper off the net being routed fades to this opacity. */
const DIM_ALPHA = 0.35;
/** Zone fills are their layer's color at this opacity. */
const ZONE_FILL_ALPHA = 0.3;

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

/** Traces a closed polygon through `points`. */
function tracePolygon(
  context: CanvasRenderingContext2D,
  viewport: Viewport,
  points: Xy[],
) {
  context.beginPath();
  points.forEach(([x, y], index) => {
    const at = worldToScreen(viewport, x, y);
    if (index === 0) {
      context.moveTo(at.x, at.y);
    } else {
      context.lineTo(at.x, at.y);
    }
  });
  context.closePath();
}

/** Strokes one straight piece of copper `widthNm` wide with round ends. */
function drawCopperLine(
  context: CanvasRenderingContext2D,
  viewport: Viewport,
  start: Xy,
  end: Xy,
  widthNm: number,
) {
  const from = worldToScreen(viewport, start[0], start[1]);
  const to = worldToScreen(viewport, end[0], end[1]);
  context.lineWidth = Math.max(widthNm * pxPerNm(viewport.zoom), MIN_LINE_PX);
  context.beginPath();
  context.moveTo(from.x, from.y);
  context.lineTo(to.x, to.y);
  context.stroke();
}

/** Fills a via's ring in `color`, then punches its drill in `holeColor`. */
function drawVia(
  context: CanvasRenderingContext2D,
  viewport: Viewport,
  position: Xy,
  diameterNm: number,
  drillNm: number,
  color: string,
  holeColor: string,
) {
  const scale = pxPerNm(viewport.zoom);
  const at = worldToScreen(viewport, position[0], position[1]);
  const radius = Math.max((diameterNm / 2) * scale, MIN_PAD_PX / 2);
  context.fillStyle = color;
  context.beginPath();
  context.arc(at.x, at.y, radius, 0, 2 * Math.PI);
  context.fill();
  context.fillStyle = holeColor;
  context.beginPath();
  context.arc(at.x, at.y, (drillNm / 2) * scale, 0, 2 * Math.PI);
  context.fill();
}

/**
 * Fills each zone's poured copper on the shown layers and outlines the zone;
 * rule areas get a dashed outline only.
 */
function drawZones(
  context: CanvasRenderingContext2D,
  viewport: Viewport,
  scene: BoardScene,
  color: (layer: string) => string,
  dimmed: (net: string | null) => boolean,
) {
  for (const zone of scene.zones) {
    const shown = zone.layers.filter((layer) => scene.visibleLayers.has(layer));
    const [first] = shown;
    if (first === undefined) {
      continue;
    }
    context.save();
    if (dimmed(zone.net)) {
      context.globalAlpha = DIM_ALPHA;
    }
    if (!zone.keepout) {
      for (const fill of zone.fills) {
        if (!scene.visibleLayers.has(fill.layer)) {
          continue;
        }
        context.save();
        context.globalAlpha *= ZONE_FILL_ALPHA;
        context.fillStyle = color(fill.layer);
        tracePolygon(context, viewport, fill.points);
        context.fill();
        context.restore();
      }
    }
    context.strokeStyle = color(first);
    context.lineWidth = 1;
    if (zone.keepout) {
      context.setLineDash(SELECTION_DASH);
    }
    tracePolygon(context, viewport, zone.outline);
    context.stroke();
    context.restore();
  }
}

/** Draws the route being drawn: fixed pieces solid, the leg to the cursor dashed. */
function drawRoute(
  context: CanvasRenderingContext2D,
  viewport: Viewport,
  route: RoutePreview,
  color: (layer: string) => string,
  viaColor: string,
  holeColor: string,
) {
  context.save();
  context.lineCap = "round";
  context.lineJoin = "round";
  for (const piece of route.pieces) {
    context.strokeStyle = color(piece.layer);
    drawCopperLine(context, viewport, piece.start, piece.end, route.widthNm);
  }
  context.globalAlpha = HOVER_ALPHA;
  context.setLineDash(SELECTION_DASH);
  for (const piece of route.pending) {
    context.strokeStyle = color(piece.layer);
    drawCopperLine(context, viewport, piece.start, piece.end, route.widthNm);
  }
  context.restore();
  for (const via of route.vias) {
    drawVia(
      context,
      viewport,
      via,
      route.viaDiameterNm,
      route.viaDrillNm,
      viaColor,
      holeColor,
    );
  }
}

/**
 * Draws the outline, zones, tracks, vias, footprints, ratsnest, the route
 * being drawn, and the hover and selection marks.
 */
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
  const viaColor = token(styles, "--color-via");
  const fontFamily = token(styles, "--font-family");
  const layerColors = new Map<string, string>();
  for (const layer of scene.layers) {
    layerColors.set(layer, token(styles, layerColorToken(layer)));
  }
  const color = (layer: string) =>
    layerColors.get(layer) ?? token(styles, layerColorToken(layer));
  const route = scene.route;
  const dimmed = (net: string | null) => route !== null && net !== route.net;

  context.save();
  context.lineCap = "round";
  context.lineJoin = "round";
  context.strokeStyle = edge;
  for (const graphic of scene.outline) {
    drawOutlineGraphic(context, viewport, graphic);
  }
  context.restore();

  drawZones(context, viewport, scene, color, dimmed);

  context.save();
  context.lineCap = "round";
  for (const layer of [...scene.layers].reverse()) {
    if (!scene.visibleLayers.has(layer)) {
      continue;
    }
    for (const track of scene.tracks) {
      if (track.layer !== layer) {
        continue;
      }
      const selected = track.uuid === scene.selectedUuid;
      context.globalAlpha = dimmed(track.net) ? DIM_ALPHA : 1;
      context.strokeStyle = selected ? focus : color(layer);
      drawCopperLine(context, viewport, track.start, track.end, track.widthNm);
    }
  }
  context.restore();

  for (const via of scene.vias) {
    const selected = via.uuid === scene.selectedUuid;
    context.save();
    context.globalAlpha = dimmed(via.net) ? DIM_ALPHA : 1;
    drawVia(
      context,
      viewport,
      via.position,
      via.diameterNm,
      via.drillNm,
      selected ? focus : viaColor,
      hole,
    );
    context.restore();
  }

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
      context.save();
      context.globalAlpha = dimmed(pad.net) ? DIM_ALPHA : 1;
      drawPad(context, viewport, pad, padColor, hole);
      context.restore();
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
    for (const line of scene.ratsnest) {
      const from = worldToScreen(viewport, line.from[0], line.from[1]);
      const to = worldToScreen(viewport, line.to[0], line.to[1]);
      context.globalAlpha = dimmed(line.net) ? DIM_ALPHA : 1;
      context.beginPath();
      context.moveTo(from.x, from.y);
      context.lineTo(to.x, to.y);
      context.stroke();
    }
    context.restore();
  }

  if (route) {
    drawRoute(context, viewport, route, color, viaColor, hole);
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
