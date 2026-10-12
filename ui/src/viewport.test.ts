import { describe, expect, it } from "vitest";
import {
  MAX_ZOOM,
  MIN_ZOOM,
  NM_PER_MM,
  type Viewport,
  fitToSheet,
  formatMm,
  formatZoom,
  panBy,
  pxPerNm,
  screenToWorld,
  visibleGridStep,
  worldToScreen,
  zoomAt,
  zoomToLevel,
} from "./viewport";

const base: Viewport = { zoom: 1, originX: 40, originY: 25 };

describe("viewport", () => {
  it("converts between document and screen coordinates", () => {
    const screen = worldToScreen(base, 10 * NM_PER_MM, 5 * NM_PER_MM);
    const world = screenToWorld(base, screen.x, screen.y);
    expect(world.x).toBeCloseTo(10 * NM_PER_MM);
    expect(world.y).toBeCloseTo(5 * NM_PER_MM);
  });

  it("keeps the point under the cursor fixed while zooming", () => {
    const cursor = { x: 300, y: 180 };
    const before = screenToWorld(base, cursor.x, cursor.y);
    const zoomed = zoomAt(base, 3.5, cursor.x, cursor.y);
    const after = screenToWorld(zoomed, cursor.x, cursor.y);
    expect(zoomed.zoom).toBeCloseTo(3.5);
    expect(after.x).toBeCloseTo(before.x);
    expect(after.y).toBeCloseTo(before.y);
  });

  it("clamps zoom to the supported range", () => {
    expect(zoomAt(base, 1e6, 0, 0).zoom).toBe(MAX_ZOOM);
    expect(zoomAt(base, 1e-6, 0, 0).zoom).toBe(MIN_ZOOM);
  });

  it("pans by screen pixels", () => {
    expect(panBy(base, -15, 30)).toEqual({ zoom: 1, originX: 25, originY: 55 });
  });

  it("sets an absolute zoom around the canvas center", () => {
    const size = { width: 800, height: 600 };
    const zoomed = zoomToLevel({ ...base, zoom: 4 }, 1, size);
    expect(zoomed.zoom).toBe(1);
    const center = screenToWorld({ ...base, zoom: 4 }, 400, 300);
    const after = screenToWorld(zoomed, 400, 300);
    expect(after.x).toBeCloseTo(center.x);
  });

  it("fits and centers an A4 sheet", () => {
    const size = { width: 1000, height: 600 };
    const sheet = { widthNm: 297 * NM_PER_MM, heightNm: 210 * NM_PER_MM };
    const fitted = fitToSheet(size, sheet, 20);
    const topLeft = worldToScreen(fitted, 0, 0);
    const bottomRight = worldToScreen(fitted, sheet.widthNm, sheet.heightNm);
    expect(bottomRight.y - topLeft.y).toBeCloseTo(560);
    expect(topLeft.x).toBeCloseTo(size.width - bottomRight.x);
  });

  it("thins the grid in 1-2-5 steps so lines stay apart", () => {
    const grid = 1.27 * NM_PER_MM;
    expect(visibleGridStep(grid, 4)).toBe(grid);
    const zoom = 0.5;
    const step = visibleGridStep(grid, zoom);
    expect(step * pxPerNm(zoom)).toBeGreaterThanOrEqual(8);
    expect(step / 2).toBeLessThan(8 / pxPerNm(zoom));
    expect([1, 2, 5, 10, 20, 50]).toContain(Math.round(step / grid));
  });

  it("formats lengths and zoom levels for display", () => {
    expect(formatMm(1_270_000)).toBe("1.27");
    expect(formatMm(635_000)).toBe("0.635");
    expect(formatMm(2_000_000)).toBe("2");
    expect(formatZoom(1.234)).toBe("123%");
  });
});
