import { describe, expect, it } from "vitest";
import {
  findInstanceAt,
  instanceBounds,
  instancePose,
  normalizeSteps,
  posePoint,
  rotatePoint,
  snapToGrid,
  snappedDragDelta,
} from "./symbols";
import { resistorInstance, resistorPreview } from "./testData";

describe("snapToGrid", () => {
  it("rounds to the nearest grid multiple", () => {
    expect(snapToGrid(63_510_000, 1_270_000)).toBe(63_500_000);
    expect(snapToGrid(64_500_000, 1_270_000)).toBe(64_770_000);
  });

  it("snaps negative coordinates without producing negative zero", () => {
    expect(snapToGrid(-700_000, 1_270_000)).toBe(-1_270_000);
    expect(snapToGrid(-500_000, 1_270_000)).toBe(0);
  });

  it("leaves coordinates alone when the grid is not positive", () => {
    expect(snapToGrid(123, 0)).toBe(123);
  });
});

describe("normalizeSteps", () => {
  it("wraps steps into [0, 4)", () => {
    expect(normalizeSteps(-1)).toBe(3);
    expect(normalizeSteps(0)).toBe(0);
    expect(normalizeSteps(5)).toBe(1);
    expect(normalizeSteps(4)).toBe(0);
  });
});

describe("rotatePoint", () => {
  it("rotates counterclockwise on screen, where Y grows down", () => {
    const up = rotatePoint(1, 0, 900);
    expect(up.x).toBeCloseTo(0);
    expect(up.y).toBeCloseTo(-1);
    const right = rotatePoint(0, 1, 900);
    expect(right.x).toBeCloseTo(1);
    expect(right.y).toBeCloseTo(0);
  });
});

describe("posePoint", () => {
  it("mirrors horizontally across x before translating", () => {
    expect(
      posePoint({ xNm: 10, yNm: 20, rotation: 0, mirror: "x" }, 2, 3),
    ).toEqual({ x: 8, y: 23 });
  });

  it("mirrors vertically across y before translating", () => {
    expect(
      posePoint({ xNm: 10, yNm: 20, rotation: 0, mirror: "y" }, 2, 3),
    ).toEqual({ x: 12, y: 17 });
  });

  it("rotates after mirroring", () => {
    const point = posePoint(
      { xNm: 10, yNm: 20, rotation: 900, mirror: "x" },
      2,
      3,
    );
    expect(point.x).toBeCloseTo(13);
    expect(point.y).toBeCloseTo(22);
  });
});

describe("instanceBounds", () => {
  it("is null for a preview without geometry", () => {
    expect(
      instanceBounds(
        {
          shapes: [],
          pins: [],
          pin_numbers_visible: false,
          pin_names_visible: false,
        },
        { xNm: 0, yNm: 0, rotation: 0, mirror: null },
      ),
    ).toBeNull();
  });

  it("swaps the spans when the symbol is rotated 90 degrees", () => {
    // Local bounds: X spans the 2.286 mm body, Y spans the 7.62 mm pins.
    const bounds = instanceBounds(resistorPreview(), {
      xNm: 100,
      yNm: 50,
      rotation: 900,
      mirror: null,
    });
    expect(bounds?.minX).toBeCloseTo(100 - 3_810_000, 0);
    expect(bounds?.maxX).toBeCloseTo(100 + 3_810_000, 0);
    expect(bounds?.minY).toBeCloseTo(50 - 1_143_000, 0);
    expect(bounds?.maxY).toBeCloseTo(50 + 1_143_000, 0);
  });
});

describe("findInstanceAt", () => {
  const definitions = { "Device:R": resistorPreview() };

  it("returns the topmost instance under the point", () => {
    const below = resistorInstance({ uuid: "below" });
    const above = resistorInstance({ uuid: "above" });
    const point = { x: 100_000_000, y: 50_000_000 };
    expect(findInstanceAt([below, above], definitions, point)?.uuid).toBe(
      "above",
    );
    expect(findInstanceAt([above, below], definitions, point)?.uuid).toBe(
      "below",
    );
  });

  it("misses points outside the transformed bounds", () => {
    const point = { x: 105_000_000, y: 50_000_000 };
    expect(
      findInstanceAt([resistorInstance()], definitions, point),
    ).toBeNull();
  });

  it("uses the rotated bounds", () => {
    // 3 mm right of the origin: outside the upright body, inside once rotated.
    const point = { x: 103_000_000, y: 50_000_000 };
    expect(
      findInstanceAt([resistorInstance()], definitions, point),
    ).toBeNull();
    const rotated = resistorInstance({ rotation: 900 });
    expect(
      findInstanceAt([rotated], definitions, point)?.uuid,
    ).toBe("symbol-1");
  });

  it("skips instances whose definition is missing", () => {
    const point = { x: 100_000_000, y: 50_000_000 };
    expect(findInstanceAt([resistorInstance()], {}, point)).toBeNull();
  });
});

describe("snappedDragDelta", () => {
  it("snaps the drag displacement onto the grid", () => {
    const delta = snappedDragDelta(
      { x: 0, y: 0 },
      { x: 700_000, y: -900_000 },
      1_270_000,
    );
    expect(delta).toEqual({ x: 1_270_000, y: -1_270_000 });
  });

  it("is zero while the pointer stays within half a grid", () => {
    const delta = snappedDragDelta(
      { x: 50_000, y: 60_000 },
      { x: 150_000, y: -40_000 },
      1_270_000,
    );
    expect(delta).toEqual({ x: 0, y: 0 });
  });
});

describe("instancePose", () => {
  it("carries the instance position, rotation, and mirror", () => {
    expect(instancePose(resistorInstance({ rotation: 1800 }))).toEqual({
      xNm: 100_000_000,
      yNm: 50_000_000,
      rotation: 1800,
      mirror: null,
    });
  });
});
