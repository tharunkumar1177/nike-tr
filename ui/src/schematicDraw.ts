/**
 * Draws the schematic scene onto the sheet canvas: wires, junctions,
 * no-connect flags, net labels, placed symbols with their Reference and Value
 * text, the selection, and the placement and wiring previews.
 * The context arrives in screen space (CSS pixels); world geometry is in
 * nanometers with Y down.
 */
import type { ErcSeverity } from "./bindings/ErcSeverity";
import type { GlobalLabelShape } from "./bindings/GlobalLabelShape";
import type { JunctionDot } from "./bindings/JunctionDot";
import type { MirrorAxis } from "./bindings/MirrorAxis";
import type { NetLabel } from "./bindings/NetLabel";
import type { NoConnectFlag } from "./bindings/NoConnectFlag";
import type { OpAnnotation } from "./bindings/OpAnnotation";
import type { SymbolInstance } from "./bindings/SymbolInstance";
import type { SymbolPreview } from "./bindings/SymbolPreview";
import type { SymbolText } from "./bindings/SymbolText";
import type { WireSegment } from "./bindings/WireSegment";
import type { Xy } from "./bindings/Xy";
import { MIN_LINE_PX, drawShape } from "./drawShapes";
import type { Bounds } from "./libraryPreview";
import type { ProbeOutline } from "./probe";
import { formatOpValue } from "./si";
import {
  type Pose,
  instanceBounds,
  instancePose,
  mirrorFactors,
  normalizeSteps,
  snapToGrid,
} from "./symbols";
import {
  type ScreenPoint,
  type Viewport,
  pxPerNm,
  worldToScreen,
} from "./viewport";
import { LABEL_TEXT_NM, NO_CONNECT_NM, type SnappedPoint } from "./wiring";

/** A symbol being placed: the ghost at the cursor, or a loading placeholder. */
export interface GhostState {
  /** `null` while the definition is still loading. */
  preview: SymbolPreview | null;
  rotationSteps: number;
  mirror: MirrorAxis | null;
  /** Unsnapped world position of the cursor. */
  cursor: ScreenPoint;
}

/** A net label shown before or while it is placed. */
export interface LabelAppearance {
  text: string;
  /** `null` for a local label. */
  shape: GlobalLabelShape | null;
  position: Xy;
  /** Decidegrees counterclockwise. */
  rotation: number;
}

export interface SceneState {
  definitions: { [key in string]: SymbolPreview };
  instances: SymbolInstance[];
  wires: WireSegment[];
  junctions: JunctionDot[];
  labels: NetLabel[];
  noConnects: NoConnectFlag[];
  /** The selected symbol, wire, label, or no-connect flag. */
  selectedUuid: string | null;
  /** In-progress drag: the local preview offset of one item. */
  drag: { uuid: string; dx: number; dy: number } | null;
  ghost: GhostState | null;
  /** The wire being drawn, from its first point to the cursor. */
  wirePreview: ScreenPoint[] | null;
  snap: SnappedPoint | null;
  labelGhost: LabelAppearance | null;
  /** Where a no-connect flag would be placed, while placing one. */
  noConnectGhost: ScreenPoint | null;
  /** Violations from the last ERC run. */
  markers: ErcMarker[];
  selectedMarkerId: string | null;
  /** The schematic changed since the markers were found. */
  markersStale: boolean;
  /** Operating point results, drawn over everything but the previews. */
  annotations: OpAnnotation[];
  /** What the probe tool would measure, while it is active. */
  probeHover: ProbeOutline | null;
  gridSizeNm: number;
}

/** Font size of an operating point annotation, in screen pixels at any zoom. */
export const OP_TEXT_PX = 14;
const OP_OFFSET_PX = 8;
const OP_PADDING_X_PX = 4;
const OP_PADDING_Y_PX = 2;
const OP_GAP_PX = 2;
/** Height of an operating point annotation box, in screen pixels. */
export const OP_BOX_HEIGHT_PX = OP_TEXT_PX + 2 * OP_PADDING_Y_PX;

/**
 * The screen position of each annotation box's top-left corner: up and to the
 * right of its anchor, with boxes that share an anchor stacked downward.
 */
export function opAnnotationOrigins(
  viewport: Viewport,
  annotations: OpAnnotation[],
): ScreenPoint[] {
  const stacks = new Map<string, number>();
  return annotations.map(({ position }) => {
    const key = `${position[0]},${position[1]}`;
    const index = stacks.get(key) ?? 0;
    stacks.set(key, index + 1);
    const at = worldToScreen(viewport, position[0], position[1]);
    return {
      x: at.x + OP_OFFSET_PX,
      y: at.y - OP_OFFSET_PX + index * (OP_BOX_HEIGHT_PX + OP_GAP_PX),
    };
  });
}

export interface OpAnnotationColors {
  fill: string;
  text: string;
  border: string;
}

/** Draws each operating point value in a box beside its anchor. */
export function drawOpAnnotations(
  context: CanvasRenderingContext2D,
  viewport: Viewport,
  annotations: OpAnnotation[],
  colors: OpAnnotationColors,
  fontFamily: string,
) {
  const origins = opAnnotationOrigins(viewport, annotations);
  context.save();
  context.font = `${OP_TEXT_PX}px ${fontFamily}`;
  context.textBaseline = "middle";
  context.lineWidth = 1;
  annotations.forEach((annotation, index) => {
    const origin = origins[index];
    if (!origin) {
      return;
    }
    const text = formatOpValue(annotation.value, annotation.kind);
    const width = context.measureText(text).width + 2 * OP_PADDING_X_PX;
    context.fillStyle = colors.fill;
    context.fillRect(origin.x, origin.y, width, OP_BOX_HEIGHT_PX);
    context.strokeStyle = colors.border;
    context.strokeRect(
      origin.x + 0.5,
      origin.y + 0.5,
      width - 1,
      OP_BOX_HEIGHT_PX - 1,
    );
    context.fillStyle = colors.text;
    context.fillText(
      text,
      origin.x + OP_PADDING_X_PX,
      origin.y + OP_BOX_HEIGHT_PX / 2,
    );
  });
  context.restore();
}

/** Where an ERC violation is, drawn as a diamond over the item it names. */
export interface ErcMarker {
  id: string;
  position: Xy;
  severity: ErcSeverity;
}

/** Width of an ERC marker diamond, in screen pixels at any zoom. */
export const MARKER_PX = 12;

const WIRE_PX = 2;
const JUNCTION_PX = 6;
const LABEL_ANCHOR_PX = 3;
const SNAP_RING_PX = 8;
const SNAP_DOT_PX = 4;
const PROBE_OUTLINE_PX = 2;
const PROBE_SEGMENT_ALPHA = 0.5;

const GHOST_ALPHA = 0.5;
const SELECTION_DASH = [4, 4];
const LOADING_BOX_WIDTH = 96;
const LOADING_BOX_HEIGHT = 40;

function token(styles: CSSStyleDeclaration, name: string): string {
  return styles.getPropertyValue(name).trim();
}

/** The pixel length of a token such as `0.875rem`, resolved against the root font size. */
function tokenPx(
  styles: CSSStyleDeclaration,
  name: string,
  fallback: number,
): number {
  const raw = token(styles, name);
  const value = parseFloat(raw);
  if (!Number.isFinite(value)) {
    return fallback;
  }
  if (raw.endsWith("rem") || raw.endsWith("em")) {
    const root = parseFloat(
      getComputedStyle(document.documentElement).fontSize,
    );
    return value * (Number.isFinite(root) && root > 0 ? root : 16);
  }
  return value;
}

/** Draws the body shapes and pins of a symbol in its local coordinates. */
function drawSymbol(
  context: CanvasRenderingContext2D,
  preview: SymbolPreview,
  stroke: string,
  bodyFill: string,
  pin: string,
  minWidth: number,
) {
  for (const shape of preview.shapes) {
    drawShape(context, shape, stroke, bodyFill, minWidth);
  }
  context.beginPath();
  for (const pinLine of preview.pins) {
    context.moveTo(pinLine.connection[0], pinLine.connection[1]);
    context.lineTo(pinLine.body[0], pinLine.body[1]);
  }
  context.strokeStyle = pin;
  context.lineWidth = minWidth * 1.5;
  context.stroke();
}

/** Applies `pose` to the context so callers draw in symbol-local nanometers. */
function applyPose(
  context: CanvasRenderingContext2D,
  viewport: Viewport,
  scale: number,
  pose: Pose,
) {
  const origin = worldToScreen(viewport, pose.xNm, pose.yNm);
  context.translate(origin.x, origin.y);
  // Schematic rotation is counterclockwise on screen; canvas rotation is clockwise.
  context.rotate((-pose.rotation / 10) * (Math.PI / 180));
  const { fx, fy } = mirrorFactors(pose.mirror);
  context.scale(fx * scale, fy * scale);
}

function drawText(
  context: CanvasRenderingContext2D,
  viewport: Viewport,
  text: SymbolText,
  color: string,
  font: string,
) {
  if (!text.visible) {
    return;
  }
  const at = worldToScreen(viewport, text.xNm, text.yNm);
  context.save();
  context.translate(at.x, at.y);
  context.rotate((-text.rotation / 10) * (Math.PI / 180));
  context.font = font;
  context.textAlign = "center";
  context.textBaseline = "middle";
  context.fillStyle = color;
  context.fillText(text.text, 0, 0);
  context.restore();
}

function drawSelectionBox(
  context: CanvasRenderingContext2D,
  viewport: Viewport,
  bounds: Bounds,
  color: string,
  paddingPx: number,
) {
  const topLeft = worldToScreen(viewport, bounds.minX, bounds.minY);
  const bottomRight = worldToScreen(viewport, bounds.maxX, bounds.maxY);
  context.save();
  context.strokeStyle = color;
  context.lineWidth = 1;
  context.setLineDash(SELECTION_DASH);
  context.strokeRect(
    topLeft.x - paddingPx,
    topLeft.y - paddingPx,
    bottomRight.x - topLeft.x + 2 * paddingPx,
    bottomRight.y - topLeft.y + 2 * paddingPx,
  );
  context.restore();
}

/** Outlines what the probe tool would measure, `paddingPx` clear of it. */
function drawProbeOutline(
  context: CanvasRenderingContext2D,
  viewport: Viewport,
  outline: ProbeOutline,
  color: string,
  paddingPx: number,
) {
  context.save();
  context.strokeStyle = color;
  context.lineWidth = PROBE_OUTLINE_PX;
  context.beginPath();
  if (outline.shape === "circle") {
    const { x, y } = outline.center;
    const at = worldToScreen(viewport, x, y);
    context.arc(at.x, at.y, SNAP_RING_PX / 2, 0, 2 * Math.PI);
    context.stroke();
  } else if (outline.shape === "segment") {
    const start = worldToScreen(viewport, outline.start.x, outline.start.y);
    const end = worldToScreen(viewport, outline.end.x, outline.end.y);
    context.moveTo(start.x, start.y);
    context.lineTo(end.x, end.y);
    context.lineWidth = WIRE_PX + 2 * PROBE_OUTLINE_PX;
    context.globalAlpha = PROBE_SEGMENT_ALPHA;
    context.stroke();
  } else {
    const { bounds } = outline;
    const topLeft = worldToScreen(viewport, bounds.minX, bounds.minY);
    const bottomRight = worldToScreen(viewport, bounds.maxX, bounds.maxY);
    context.strokeRect(
      topLeft.x - paddingPx,
      topLeft.y - paddingPx,
      bottomRight.x - topLeft.x + 2 * paddingPx,
      bottomRight.y - topLeft.y + 2 * paddingPx,
    );
  }
  context.restore();
}

function drawLoadingBox(
  context: CanvasRenderingContext2D,
  at: ScreenPoint,
  color: string,
  font: string,
) {
  context.save();
  context.strokeStyle = color;
  context.lineWidth = 1;
  context.setLineDash(SELECTION_DASH);
  context.strokeRect(
    at.x - LOADING_BOX_WIDTH / 2,
    at.y - LOADING_BOX_HEIGHT / 2,
    LOADING_BOX_WIDTH,
    LOADING_BOX_HEIGHT,
  );
  context.font = font;
  context.textAlign = "center";
  context.textBaseline = "middle";
  context.fillStyle = color;
  context.fillText("Loading...", at.x, at.y);
  context.restore();
}

function strokePolyline(
  context: CanvasRenderingContext2D,
  viewport: Viewport,
  points: ScreenPoint[],
  color: string,
) {
  const first = points[0];
  if (!first || points.length < 2) {
    return;
  }
  context.beginPath();
  const start = worldToScreen(viewport, first.x, first.y);
  context.moveTo(start.x, start.y);
  for (const point of points.slice(1)) {
    const at = worldToScreen(viewport, point.x, point.y);
    context.lineTo(at.x, at.y);
  }
  context.strokeStyle = color;
  context.lineWidth = WIRE_PX;
  context.stroke();
}

function fillDot(
  context: CanvasRenderingContext2D,
  at: ScreenPoint,
  diameterPx: number,
  color: string,
) {
  context.beginPath();
  context.arc(at.x, at.y, diameterPx / 2, 0, 2 * Math.PI);
  context.fillStyle = color;
  context.fill();
}

/** The outline of a global label in its frame: text runs along +x from the anchor. */
function globalLabelOutline(
  shape: GlobalLabelShape,
  textWidth: number,
  textHeight: number,
): { points: [number, number][]; textStart: number } {
  const half = textHeight * 0.8;
  const pad = textHeight * 0.4;
  const pointedStart =
    shape === "input" || shape === "bidirectional" || shape === "triState";
  const pointedEnd =
    shape === "output" || shape === "bidirectional" || shape === "triState";
  const textStart = (pointedStart ? half : 0) + pad;
  const end = textStart + textWidth + pad;
  const points: [number, number][] = pointedStart
    ? [
        [0, 0],
        [half, -half],
      ]
    : [[0, -half]];
  points.push([end, -half]);
  if (pointedEnd) {
    points.push([end + half, 0]);
  }
  points.push([end, half]);
  points.push(pointedStart ? [half, half] : [0, half]);
  return { points, textStart };
}

interface LabelColors {
  text: string;
  anchor: string;
  outline: string;
  fill: string;
}

/**
 * Draws a net label. Text stays readable: labels at 180 and 270 degrees
 * are drawn as 0 and 90 degrees running back from the anchor.
 */
function drawLabel(
  context: CanvasRenderingContext2D,
  viewport: Viewport,
  label: LabelAppearance,
  colors: LabelColors,
  fontFamily: string,
) {
  const scale = pxPerNm(viewport.zoom);
  const textHeight = LABEL_TEXT_NM * scale;
  const rotation = ((label.rotation % 3600) + 3600) % 3600;
  const reversed = rotation >= 1800;
  const angle = reversed ? rotation - 1800 : rotation;
  const direction = reversed ? -1 : 1;
  const at = worldToScreen(viewport, label.position[0], label.position[1]);
  context.save();
  context.translate(at.x, at.y);
  context.rotate((-angle / 10) * (Math.PI / 180));
  context.font = `${textHeight}px ${fontFamily}`;
  context.textAlign = reversed ? "right" : "left";
  const textWidth = context.measureText(label.text).width;
  if (label.shape === null) {
    context.textBaseline = "bottom";
    context.fillStyle = colors.text;
    context.fillText(label.text, direction * textHeight * 0.2, 0);
    context.fillStyle = colors.anchor;
    context.fillRect(
      -LABEL_ANCHOR_PX / 2,
      -LABEL_ANCHOR_PX / 2,
      LABEL_ANCHOR_PX,
      LABEL_ANCHOR_PX,
    );
  } else {
    const outline = globalLabelOutline(label.shape, textWidth, textHeight);
    context.beginPath();
    outline.points.forEach(([x, y], index) => {
      if (index === 0) {
        context.moveTo(direction * x, y);
      } else {
        context.lineTo(direction * x, y);
      }
    });
    context.closePath();
    context.fillStyle = colors.fill;
    context.fill();
    context.strokeStyle = colors.outline;
    context.lineWidth = MIN_LINE_PX * 1.5;
    context.stroke();
    context.textBaseline = "middle";
    context.fillStyle = colors.text;
    context.fillText(label.text, direction * outline.textStart, 0);
  }
  context.restore();
}

function drawSnap(
  context: CanvasRenderingContext2D,
  viewport: Viewport,
  snap: SnappedPoint,
  focus: string,
  gridDot: string,
) {
  const at = worldToScreen(viewport, snap.point.x, snap.point.y);
  if (snap.onConnection) {
    context.beginPath();
    context.arc(at.x, at.y, SNAP_RING_PX / 2, 0, 2 * Math.PI);
    context.strokeStyle = focus;
    context.lineWidth = 2;
    context.stroke();
  } else {
    fillDot(context, at, SNAP_DOT_PX, gridDot);
  }
}

/** Draws a no-connect flag: an X centered on the point it marks. */
function drawNoConnect(
  context: CanvasRenderingContext2D,
  viewport: Viewport,
  point: ScreenPoint,
  color: string,
) {
  const at = worldToScreen(viewport, point.x, point.y);
  const half = (NO_CONNECT_NM / 2) * pxPerNm(viewport.zoom);
  context.beginPath();
  context.moveTo(at.x - half, at.y - half);
  context.lineTo(at.x + half, at.y + half);
  context.moveTo(at.x - half, at.y + half);
  context.lineTo(at.x + half, at.y - half);
  context.strokeStyle = color;
  context.lineWidth = WIRE_PX;
  context.stroke();
}

/** Draws an ERC marker: a hollow diamond centered on the violation. */
function drawMarker(
  context: CanvasRenderingContext2D,
  viewport: Viewport,
  position: Xy,
  color: string,
) {
  const at = worldToScreen(viewport, position[0], position[1]);
  const half = MARKER_PX / 2;
  context.beginPath();
  context.moveTo(at.x, at.y - half);
  context.lineTo(at.x + half, at.y);
  context.lineTo(at.x, at.y + half);
  context.lineTo(at.x - half, at.y);
  context.closePath();
  context.strokeStyle = color;
  context.lineWidth = WIRE_PX;
  context.stroke();
}

function moved(xy: Xy, offset: { dx: number; dy: number } | null): ScreenPoint {
  return offset
    ? { x: xy[0] + offset.dx, y: xy[1] + offset.dy }
    : { x: xy[0], y: xy[1] };
}

/** Draws every wire, label, and placed symbol, the selection, and the previews. */
export function drawSchematicScene(
  context: CanvasRenderingContext2D,
  viewport: Viewport,
  scene: SceneState,
) {
  const styles = getComputedStyle(context.canvas);
  const scale = pxPerNm(viewport.zoom);
  const minWidth = MIN_LINE_PX / scale;
  const stroke = token(styles, "--color-primary");
  const bodyFill = token(styles, "--color-surface-elevated");
  const pin = token(styles, "--color-text-error");
  const text = token(styles, "--color-text-primary");
  const focus = token(styles, "--color-focus");
  const textSecondary = token(styles, "--color-text-secondary");
  const fontFamily = token(styles, "--font-family");
  const textPx = tokenPx(styles, "--text-sm", 14);
  const font = `${textPx * viewport.zoom}px ${fontFamily}`;
  const selectionPadding = tokenPx(styles, "--space-1", 4);

  const warning = token(styles, "--color-text-warning");
  const canvasFill = token(styles, "--color-canvas");

  context.lineCap = "round";
  context.lineJoin = "round";

  const offsetOf = (uuid: string) =>
    scene.drag?.uuid === uuid ? scene.drag : null;

  for (const wire of scene.wires) {
    const offset = offsetOf(wire.uuid);
    strokePolyline(
      context,
      viewport,
      [moved(wire.start, offset), moved(wire.end, offset)],
      wire.uuid === scene.selectedUuid ? focus : stroke,
    );
  }

  for (const label of scene.labels) {
    const offset = offsetOf(label.uuid);
    const selected = label.uuid === scene.selectedUuid;
    const position = moved(label.position, offset);
    drawLabel(
      context,
      viewport,
      { ...label, position: [position.x, position.y] },
      {
        text: selected ? focus : text,
        anchor: selected ? focus : stroke,
        outline: selected ? focus : warning,
        fill: canvasFill,
      },
      fontFamily,
    );
  }

  for (const instance of scene.instances) {
    const preview = scene.definitions[instance.libId];
    if (!preview) {
      continue;
    }
    const selected = instance.uuid === scene.selectedUuid;
    const offset = scene.drag?.uuid === instance.uuid ? scene.drag : null;
    const base = instancePose(instance);
    const pose: Pose = offset
      ? { ...base, xNm: base.xNm + offset.dx, yNm: base.yNm + offset.dy }
      : base;
    context.save();
    applyPose(context, viewport, scale, pose);
    drawSymbol(
      context,
      preview,
      selected ? focus : stroke,
      bodyFill,
      selected ? focus : pin,
      minWidth,
    );
    context.restore();
    const textColor = selected ? focus : text;
    if (instance.reference) {
      drawText(context, viewport, instance.reference, textColor, font);
    }
    if (instance.value) {
      drawText(context, viewport, instance.value, textColor, font);
    }
    if (selected) {
      const bounds = instanceBounds(preview, pose);
      if (bounds) {
        drawSelectionBox(context, viewport, bounds, focus, selectionPadding);
      }
    }
  }

  for (const junction of scene.junctions) {
    const at = worldToScreen(
      viewport,
      junction.position[0],
      junction.position[1],
    );
    fillDot(context, at, JUNCTION_PX, stroke);
  }

  for (const flag of scene.noConnects) {
    const position = moved(flag.position, offsetOf(flag.uuid));
    const selected = flag.uuid === scene.selectedUuid;
    drawNoConnect(context, viewport, position, selected ? focus : stroke);
  }

  if (scene.markers.length > 0) {
    context.save();
    if (scene.markersStale) {
      context.globalAlpha = GHOST_ALPHA;
    }
    for (const marker of scene.markers) {
      const color =
        marker.id === scene.selectedMarkerId
          ? focus
          : marker.severity === "error"
            ? pin
            : warning;
      drawMarker(context, viewport, marker.position, color);
    }
    context.restore();
  }

  if (scene.annotations.length > 0) {
    drawOpAnnotations(
      context,
      viewport,
      scene.annotations,
      { fill: bodyFill, text, border: textSecondary },
      fontFamily,
    );
  }

  if (scene.wirePreview) {
    context.save();
    context.globalAlpha = GHOST_ALPHA;
    strokePolyline(context, viewport, scene.wirePreview, stroke);
    context.restore();
  }

  if (scene.labelGhost) {
    context.save();
    context.globalAlpha = GHOST_ALPHA;
    drawLabel(
      context,
      viewport,
      scene.labelGhost,
      { text, anchor: stroke, outline: warning, fill: canvasFill },
      fontFamily,
    );
    context.restore();
  }

  if (scene.noConnectGhost) {
    context.save();
    context.globalAlpha = GHOST_ALPHA;
    drawNoConnect(context, viewport, scene.noConnectGhost, stroke);
    context.restore();
  }

  if (scene.snap) {
    drawSnap(context, viewport, scene.snap, focus, textSecondary);
  }

  if (scene.probeHover) {
    drawProbeOutline(
      context,
      viewport,
      scene.probeHover,
      focus,
      selectionPadding,
    );
  }

  const ghost = scene.ghost;
  if (ghost) {
    if (ghost.preview) {
      const pose: Pose = {
        xNm: snapToGrid(ghost.cursor.x, scene.gridSizeNm),
        yNm: snapToGrid(ghost.cursor.y, scene.gridSizeNm),
        rotation: normalizeSteps(ghost.rotationSteps) * 900,
        mirror: ghost.mirror,
      };
      context.save();
      context.globalAlpha = GHOST_ALPHA;
      applyPose(context, viewport, scale, pose);
      drawSymbol(context, ghost.preview, stroke, bodyFill, pin, minWidth);
      context.restore();
    } else {
      drawLoadingBox(
        context,
        worldToScreen(viewport, ghost.cursor.x, ghost.cursor.y),
        textSecondary,
        `${textPx}px ${fontFamily}`,
      );
    }
  }
}
