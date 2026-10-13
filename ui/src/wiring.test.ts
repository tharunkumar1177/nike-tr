import { describe, expect, it } from "vitest";
import type { NetLabel } from "./bindings/NetLabel";
import { resistorInstance } from "./testData";
import {
  connectionPoints,
  dedupePath,
  findLabelAt,
  findNoConnectAt,
  findWireAt,
  isOnPin,
  isOnWire,
  labelTextError,
  nextLabelText,
  orthogonalRoute,
  snapWirePoint,
  wirePath,
} from "./wiring";

const GRID = 1_270_000;
const wire = {
  uuid: "wire-1",
  start: [0, 0] as [number, number],
  end: [10_160_000, 0] as [number, number],
};

describe("no-connect hit-testing", () => {
  const flags = [
    { uuid: "flag-1", position: [GRID, GRID] as [number, number] },
    { uuid: "flag-2", position: [GRID, GRID] as [number, number] },
  ];

  it("finds the topmost flag within its X and the tolerance", () => {
    const center = { x: GRID, y: GRID };
    expect(findNoConnectAt(flags, center, 0)?.uuid).toBe("flag-2");
    const corner = { x: GRID + GRID / 2, y: GRID - GRID / 2 };
    expect(findNoConnectAt(flags, corner, 0)?.uuid).toBe("flag-2");
    const outside = { x: GRID * 2 + 1, y: GRID };
    expect(findNoConnectAt(flags, outside, 0)).toBeNull();
    expect(findNoConnectAt(flags, outside, GRID)?.uuid).toBe("flag-2");
  });
});

describe("orthogonal routes", () => {
  it("bends once, horizontal first unless toggled", () => {
    const from = { x: 0, y: 0 };
    const to = { x: 5, y: 7 };
    expect(orthogonalRoute(from, to, false)).toEqual([
      from,
      { x: 5, y: 0 },
      to,
    ]);
    expect(orthogonalRoute(from, to, true)).toEqual([from, { x: 0, y: 7 }, to]);
  });

  it("has no bend on a straight run", () => {
    const from = { x: 0, y: 0 };
    expect(orthogonalRoute(from, { x: 0, y: 9 }, false)).toEqual([
      from,
      { x: 0, y: 9 },
    ]);
  });

  it("extends the anchored corners to the cursor", () => {
    const anchors = [
      { x: 0, y: 0 },
      { x: 5, y: 0 },
    ];
    expect(wirePath(anchors, { x: 5, y: 3 }, false)).toEqual([
      ...anchors,
      { x: 5, y: 3 },
    ]);
    expect(wirePath(anchors, null, false)).toEqual(anchors);
  });

  it("drops repeated points", () => {
    const path = dedupePath([
      { x: 0, y: 0 },
      { x: 0, y: 0 },
      { x: 1, y: 0 },
    ]);
    expect(path).toEqual([
      { x: 0, y: 0 },
      { x: 1, y: 0 },
    ]);
  });
});

describe("wire snapping", () => {
  const targets = connectionPoints([resistorInstance()], [wire]);

  it("collects pin ends and wire ends", () => {
    expect(targets).toEqual([
      [100_000_000, 46_190_000],
      [100_000_000, 53_810_000],
      [0, 0],
      [10_160_000, 0],
    ]);
  });

  it("snaps to a nearby pin end before the grid", () => {
    const near = { x: 100_200_000, y: 46_000_000 };
    expect(snapWirePoint(near, GRID, targets, 500_000)).toEqual({
      point: { x: 100_000_000, y: 46_190_000 },
      onConnection: true,
    });
  });

  it("falls back to the grid outside the radius", () => {
    const far = { x: 50_100_000, y: 20_400_000 };
    expect(snapWirePoint(far, GRID, targets, 500_000)).toEqual({
      point: { x: 49_530_000, y: 20_320_000 },
      onConnection: false,
    });
  });

  it("knows exact pin and wire points", () => {
    const instances = [resistorInstance()];
    expect(isOnPin(instances, { x: 100_000_000, y: 53_810_000 })).toBe(true);
    expect(isOnPin(instances, { x: 100_000_000, y: 53_800_000 })).toBe(false);
    expect(isOnWire([wire], { x: 5_080_000, y: 0 })).toBe(true);
    expect(isOnWire([wire], { x: 5_080_000, y: 1 })).toBe(false);
  });
});

describe("hit-testing", () => {
  it("finds a wire within the tolerance", () => {
    expect(findWireAt([wire], { x: 5_000_000, y: 200_000 }, 300_000)).toBe(
      wire,
    );
    expect(findWireAt([wire], { x: 5_000_000, y: 400_000 }, 300_000)).toBe(
      null,
    );
  });

  it("finds a label along its text direction", () => {
    const label: NetLabel = {
      uuid: "label-1",
      text: "OUT",
      shape: null,
      position: [20_000_000, 20_000_000],
      rotation: 0,
    };
    expect(findLabelAt([label], { x: 21_000_000, y: 20_000_000 })).toBe(label);
    expect(findLabelAt([label], { x: 19_000_000, y: 20_000_000 })).toBe(null);
    const turned = { ...label, rotation: 1800 };
    expect(findLabelAt([turned], { x: 19_000_000, y: 20_000_000 })).toBe(
      turned,
    );
  });
});

describe("label names", () => {
  it("rejects empty, spaced, and overlong names", () => {
    expect(labelTextError("")).toBe("Enter a label name.");
    expect(labelTextError("A B")).toBe("Label names can't contain spaces.");
    expect(labelTextError("x".repeat(257))).toBe("Use at most 256 characters.");
    expect(labelTextError("SDA")).toBe(null);
  });

  it("counts trailing digits up, keeping their width", () => {
    expect(nextLabelText("D0")).toBe("D1");
    expect(nextLabelText("D09")).toBe("D10");
    expect(nextLabelText("A99")).toBe("A100");
    expect(nextLabelText("SCL")).toBe("SCL");
    expect(nextLabelText("7")).toBe("8");
  });
});
