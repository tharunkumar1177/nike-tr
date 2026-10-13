/**
 * Draws the schematic scene onto the sheet canvas: placed symbols with their
 * Reference and Value text, the selection box, and the placement ghost.
 * The context arrives in screen space (CSS pixels); world geometry is in
 * nanometers with Y down.
 */
import type { MirrorAxis } from "./bindings/MirrorAxis";
import type { SymbolInstance } from "./bindings/SymbolInstance";
import type { SymbolPreview } from "./bindings/SymbolPreview";
import type { SymbolText } from "./bindings/SymbolText";
import { MIN_LINE_PX, drawShape } from "./drawShapes";
import type { Bounds } from "./libraryPreview";
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

/** A symbol being placed: the ghost at the cursor, or a loading placeholder. */
export interface GhostState {
  /** `null` while the definition is still loading. */
  preview: SymbolPreview | null;
  rotationSteps: number;
  mirror: MirrorAxis | null;
  /** Unsnapped world position of the cursor. */
  cursor: ScreenPoint;
}

export interface SceneState {
  definitions: { [key in string]: SymbolPreview };
  instances: SymbolInstance[];
  selectedUuid: string | null;
  /** In-progress drag: the local preview offset of one instance. */
  drag: { uuid: string; dx: number; dy: number } | null;
  ghost: GhostState | null;
  gridSizeNm: number;
}

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

/** Draws every placed symbol, the selection, and the placement ghost. */
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

  context.lineCap = "round";
  context.lineJoin = "round";

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
