/**
 * Tests for the interactive routing model: 45-degree legs, route steps and
 * their vias, the finished route, layer choice, snapping, and picking tracks
 * and vias.
 */
import { describe, expect, it } from "vitest";
import type { BoardTrack } from "./bindings/BoardTrack";
import type { BoardVia } from "./bindings/BoardVia";
import {
  type RouteDraft,
  extendRoute,
  findTrackAt,
  findViaAt,
  finishedRoute,
  layerColorToken,
  routeEnd,
  routeLeg,
  routeTargets,
  routeVias,
  snapRoutePoint,
  stepLayer,
  viaLayer,
} from "./routing";
import { defaultDesignRules, resistorFootprint } from "./testData";

const FOUR_LAYERS = ["F.Cu", "In1.Cu", "In2.Cu", "B.Cu"];

describe("routeLeg", () => {
  it("runs straight, then diagonal, or the other way round", () => {
    expect(routeLeg([0, 0], [5, 2], false)).toEqual([
      [0, 0],
      [3, 0],
      [5, 2],
    ]);
    expect(routeLeg([0, 0], [5, -2], true)).toEqual([
      [0, 0],
      [2, -2],
      [5, -2],
    ]);
  });

  it("has no bend on a straight or diagonal run", () => {
    expect(routeLeg([0, 0], [0, 7], false)).toEqual([
      [0, 0],
      [0, 7],
    ]);
    expect(routeLeg([0, 0], [-4, 4], true)).toEqual([
      [0, 0],
      [-4, 4],
    ]);
  });
});

describe("route drafts", () => {
  const start: RouteDraft = { net: "GND", start: [0, 0], steps: [] };

  it("adds steps, drops vias, and finishes as one route", () => {
    const corner = extendRoute(start, [10, 0], false, "F.Cu", false);
    const via = extendRoute(corner, [10, 0], false, "F.Cu", true);
    const end = extendRoute(via, [10, 10], false, "B.Cu", false);
    expect(routeEnd(end)).toEqual([10, 10]);
    expect(routeVias(end)).toEqual([[10, 0]]);
    expect(finishedRoute(end, defaultDesignRules())).toEqual({
      net: "GND",
      widthNm: 200_000,
      viaDiameterNm: 600_000,
      viaDrillNm: 300_000,
      pieces: [
        { start: [0, 0], end: [10, 0], layer: "F.Cu" },
        { start: [10, 0], end: [10, 10], layer: "B.Cu" },
      ],
      vias: [[10, 0]],
    });
  });

  it("ignores a click that adds nothing", () => {
    expect(extendRoute(start, [0, 0], false, "F.Cu", false)).toBe(start);
    expect(finishedRoute(start, defaultDesignRules())).toBeNull();
  });
});

describe("layers", () => {
  it("mirrors the layer a via drops onto", () => {
    expect(viaLayer(["F.Cu", "B.Cu"], "F.Cu")).toBe("B.Cu");
    expect(viaLayer(FOUR_LAYERS, "In1.Cu")).toBe("In2.Cu");
    expect(viaLayer(FOUR_LAYERS, "B.Cu")).toBe("F.Cu");
  });

  it("steps through the shown layers, wrapping around", () => {
    expect(stepLayer(FOUR_LAYERS, "F.Cu", 1)).toBe("In1.Cu");
    expect(stepLayer(FOUR_LAYERS, "F.Cu", -1)).toBe("B.Cu");
    expect(stepLayer(["F.Cu", "B.Cu"], "In1.Cu", 1)).toBe("F.Cu");
  });

  it("gives every copper layer a color token", () => {
    expect(layerColorToken("F.Cu")).toBe("--color-layer-f-cu");
    expect(layerColorToken("B.Cu")).toBe("--color-layer-b-cu");
    expect(layerColorToken("In2.Cu")).toBe("--color-layer-in2");
    expect(layerColorToken("In7.Cu")).toBe("--color-layer-in1");
  });
});

describe("snapping and picking", () => {
  const track: BoardTrack = {
    uuid: "track-1",
    start: [0, 0],
    end: [10_000_000, 0],
    widthNm: 250_000,
    layer: "F.Cu",
    net: "GND",
  };
  const via: BoardVia = {
    uuid: "via-1",
    position: [10_000_000, 0],
    diameterNm: 600_000,
    drillNm: 300_000,
    layers: ["F.Cu", "B.Cu"],
    net: "GND",
  };

  it("snaps to nearby copper, or else to the grid", () => {
    const targets = routeTargets([resistorFootprint()], [track], [via]);
    const pad = snapRoutePoint(
      { x: 20_830_000, y: 20_010_000 },
      targets,
      50_000,
      635_000,
    );
    expect(pad).toEqual({
      point: [20_825_000, 20_000_000],
      target: { point: [20_825_000, 20_000_000], net: "/OUT", layer: "F.Cu" },
    });
    const grid = snapRoutePoint(
      { x: 700_000, y: 600_000 },
      targets,
      0,
      635_000,
    );
    expect(grid).toEqual({ point: [635_000, 635_000], target: null });
  });

  it("picks vias and tracks on shown layers only", () => {
    const near = { x: 5_000_000, y: 100_000 };
    expect(findViaAt([via], { x: 10_250_000, y: 0 }, 0)).toBe(via);
    expect(findViaAt([via], { x: 10_400_000, y: 0 }, 0)).toBeNull();
    expect(findTrackAt([track], near, 0, new Set(["F.Cu"]))).toBe(track);
    expect(findTrackAt([track], near, 0, new Set(["B.Cu"]))).toBeNull();
  });
});
