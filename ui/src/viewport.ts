/**
 * Canvas viewport math. Document coordinates are nanometers with Y growing
 * down; screen coordinates are CSS pixels relative to the canvas.
 */

export const NM_PER_MM = 1_000_000;
/** CSS pixels per millimeter at 100% zoom (96 dpi). */
export const PX_PER_MM_AT_ACTUAL_SIZE = 96 / 25.4;
export const MIN_ZOOM = 0.1;
export const MAX_ZOOM = 100;
export const ZOOM_STEP = 1.25;
/** Grid lines closer than this on screen are thinned out. */
export const MIN_GRID_SPACING_PX = 8;

export interface Viewport {
  zoom: number;
  /** Screen position of the document origin. */
  originX: number;
  originY: number;
}

export interface Size {
  width: number;
  height: number;
}

export interface ScreenPoint {
  x: number;
  y: number;
}

export interface SheetDimensions {
  widthNm: number;
  heightNm: number;
}

export function pxPerNm(zoom: number): number {
  return (zoom * PX_PER_MM_AT_ACTUAL_SIZE) / NM_PER_MM;
}

export function clampZoom(zoom: number): number {
  return Math.min(MAX_ZOOM, Math.max(MIN_ZOOM, zoom));
}

export function worldToScreen(
  viewport: Viewport,
  xNm: number,
  yNm: number,
): ScreenPoint {
  const scale = pxPerNm(viewport.zoom);
  return {
    x: viewport.originX + xNm * scale,
    y: viewport.originY + yNm * scale,
  };
}

export function screenToWorld(
  viewport: Viewport,
  x: number,
  y: number,
): ScreenPoint {
  const scale = pxPerNm(viewport.zoom);
  return {
    x: (x - viewport.originX) / scale,
    y: (y - viewport.originY) / scale,
  };
}

/** Zooms by `factor` while keeping the document point under (x, y) fixed. */
export function zoomAt(
  viewport: Viewport,
  factor: number,
  x: number,
  y: number,
): Viewport {
  const zoom = clampZoom(viewport.zoom * factor);
  const ratio = zoom / viewport.zoom;
  return {
    zoom,
    originX: x - (x - viewport.originX) * ratio,
    originY: y - (y - viewport.originY) * ratio,
  };
}

/** Sets an absolute zoom level around the center of the canvas. */
export function zoomToLevel(
  viewport: Viewport,
  zoom: number,
  size: Size,
): Viewport {
  return zoomAt(
    viewport,
    zoom / viewport.zoom,
    size.width / 2,
    size.height / 2,
  );
}

export function panBy(viewport: Viewport, dx: number, dy: number): Viewport {
  return {
    zoom: viewport.zoom,
    originX: viewport.originX + dx,
    originY: viewport.originY + dy,
  };
}

/** Fits the whole sheet in the canvas with a margin, centered. */
export function fitToSheet(
  size: Size,
  sheet: SheetDimensions,
  marginPx = 24,
): Viewport {
  const width = Math.max(1, size.width - 2 * marginPx);
  const height = Math.max(1, size.height - 2 * marginPx);
  const zoom = clampZoom(
    Math.min(
      width / (sheet.widthNm * pxPerNm(1)),
      height / (sheet.heightNm * pxPerNm(1)),
    ),
  );
  const scale = pxPerNm(zoom);
  return {
    zoom,
    originX: (size.width - sheet.widthNm * scale) / 2,
    originY: (size.height - sheet.heightNm * scale) / 2,
  };
}

/**
 * The spacing to draw grid lines at: the document grid, widened in 1-2-5
 * steps until neighbouring lines are at least `minPx` apart on screen.
 */
export function visibleGridStep(
  gridNm: number,
  zoom: number,
  minPx = MIN_GRID_SPACING_PX,
): number {
  if (!(gridNm > 0) || !(zoom > 0) || !Number.isFinite(zoom)) {
    return gridNm;
  }
  const scale = pxPerNm(zoom);
  for (let decade = 1; ; decade *= 10) {
    for (const multiplier of [1, 2, 5]) {
      const step = gridNm * multiplier * decade;
      if (step * scale >= minPx) {
        return step;
      }
    }
  }
}

/** Formats nanometers as millimeters without trailing zeros, e.g. "0.635". */
export function formatMm(nm: number, maxDecimals = 4): string {
  return String(Number((nm / NM_PER_MM).toFixed(maxDecimals)));
}

export function formatZoom(zoom: number): string {
  return `${Math.round(zoom * 100)}%`;
}
