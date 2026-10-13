import { describe, expect, it } from "vitest";
import type { SymbolPreview } from "./bindings/SymbolPreview";
import {
  arcThrough,
  fitBounds,
  footprintBounds,
  symbolBounds,
} from "./libraryPreview";

describe("arcThrough", () => {
  it("runs clockwise on screen when the middle point comes first that way", () => {
    const arc = arcThrough([10, 0], [0, 10], [-10, 0]);
    expect(arc).not.toBeNull();
    expect(arc!.centerX).toBeCloseTo(0);
    expect(arc!.centerY).toBeCloseTo(0);
    expect(arc!.radius).toBeCloseTo(10);
    expect(arc!.anticlockwise).toBe(false);
  });

  it("runs anticlockwise for the reversed points", () => {
    expect(arcThrough([-10, 0], [0, 10], [10, 0])!.anticlockwise).toBe(true);
  });

  it("is null for collinear points", () => {
    expect(arcThrough([0, 0], [5, 5], [10, 10])).toBeNull();
  });
});

describe("bounds", () => {
  it("includes the bulge of an arc, pins, and half the line width", () => {
    const preview: SymbolPreview = {
      shapes: [
        {
          type: "arc",
          start: [10, 0],
          mid: [0, 10],
          end: [-10, 0],
          width: 2,
          fill: "none",
        },
      ],
      pins: [
        {
          connection: [0, -20],
          body: [0, -10],
          number: "1",
          name: "A",
          electrical: "input",
        },
      ],
      pin_numbers_visible: true,
      pin_names_visible: true,
    };
    expect(symbolBounds(preview)).toEqual({
      minX: -11,
      minY: -20,
      maxX: 11,
      maxY: 11,
    });
  });

  it("covers rotated pads by their diagonal", () => {
    const bounds = footprintBounds({
      graphics: [],
      pads: [
        {
          number: "1",
          kind: "smd",
          shape: "rect",
          center: [0, 0],
          size: [6, 8],
          rotation: 450,
          corner_radius: 0,
        },
      ],
    });
    expect(bounds).toEqual({ minX: -5, minY: -5, maxX: 5, maxY: 5 });
  });

  it("is null for an empty drawing", () => {
    expect(
      symbolBounds({
        shapes: [],
        pins: [],
        pin_numbers_visible: true,
        pin_names_visible: true,
      }),
    ).toBeNull();
  });
});

describe("fitBounds", () => {
  it("centers the drawing and keeps a margin on its longer side", () => {
    const fit = fitBounds({ minX: 0, minY: 0, maxX: 200, maxY: 100 }, 100, 100);
    expect(fit.scale).toBeCloseTo(0.4);
    expect(fit.offsetX).toBeCloseTo(10);
    expect(fit.offsetY).toBeCloseTo(30);
  });
});
