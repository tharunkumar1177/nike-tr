/** Tests for the board view geometry: fitting, picking, pads, and the ratsnest drag. */
import { describe, expect, it } from "vitest";
import type { BoardPad } from "./bindings/BoardPad";
import type { OutlineGraphic } from "./bindings/OutlineGraphic";
import type { RatsnestSegment } from "./bindings/RatsnestSegment";
import {
  EMPTY_BOARD_NM,
  boardBounds,
  findFootprintAt,
  fitBoard,
  offsetFootprint,
  padCornerRadius,
  shiftRatsnest,
} from "./board";
import { resistorFootprint } from "./testData";
import { pxPerNm, screenToWorld } from "./viewport";

const SIZE = { width: 800, height: 600 };

/** A 50 by 40 mm Edge.Cuts rectangle from (100, 60) mm with a 0.1 mm stroke. */
function rectOutline(): OutlineGraphic {
  return {
    uuid: null,
    geometry: {
      type: "rect",
      start: [100_000_000, 60_000_000],
      end: [150_000_000, 100_000_000],
    },
    widthNm: 100_000,
  };
}

/** The first pad of the fixture resistor with `changes` applied. */
function padWith(changes: Partial<BoardPad>): BoardPad {
  const [pad] = resistorFootprint().pads;
  if (!pad) {
    throw new Error("fixture has no pads");
  }
  return { ...pad, ...changes };
}

describe("fitBoard", () => {
  it("shows a 100 mm square from the origin for an empty board", () => {
    const viewport = fitBoard(SIZE, [], []);
    const center = screenToWorld(viewport, SIZE.width / 2, SIZE.height / 2);
    expect(center.x).toBeCloseTo(EMPTY_BOARD_NM / 2);
    expect(center.y).toBeCloseTo(EMPTY_BOARD_NM / 2);
    expect(EMPTY_BOARD_NM * pxPerNm(viewport.zoom)).toBeCloseTo(600 - 48);
  });

  it("centers the outline and the footprints together", () => {
    const viewport = fitBoard(SIZE, [resistorFootprint()], [rectOutline()]);
    const center = screenToWorld(viewport, SIZE.width / 2, SIZE.height / 2);
    const bounds = boardBounds([resistorFootprint()], [rectOutline()]);
    expect(bounds).toEqual({
      minX: 18_520_000,
      minY: 19_270_000,
      maxX: 150_050_000,
      maxY: 100_050_000,
    });
    expect(center.x).toBeCloseTo((18_520_000 + 150_050_000) / 2);
    expect(center.y).toBeCloseTo((19_270_000 + 100_050_000) / 2);
  });

  it("counts a circle outline by its radius", () => {
    const circle: OutlineGraphic = {
      uuid: null,
      geometry: { type: "circle", center: [0, 0], end: [3_000_000, 4_000_000] },
      widthNm: 0,
    };
    expect(boardBounds([], [circle])).toEqual({
      minX: -5_000_000,
      minY: -5_000_000,
      maxX: 5_000_000,
      maxY: 5_000_000,
    });
  });
});

describe("findFootprintAt", () => {
  it("picks the last footprint whose bounds hold the point", () => {
    const first = resistorFootprint();
    const second = resistorFootprint({ uuid: "footprint-2" });
    const point = { x: 20_000_000, y: 20_000_000 };
    expect(findFootprintAt([first, second], point)?.uuid).toBe("footprint-2");
    expect(findFootprintAt([first], { x: 0, y: 0 })).toBeNull();
  });
});

describe("padCornerRadius", () => {
  it("rounds each shape the way it is drawn", () => {
    expect(padCornerRadius(padWith({}))).toBe(200_000);
    expect(padCornerRadius(padWith({ cornerRadiusNm: 900_000 }))).toBe(400_000);
    expect(padCornerRadius(padWith({ shape: "oval" }))).toBe(400_000);
    const circle = padWith({ shape: "circle", size: [1_600_000, 1_600_000] });
    expect(padCornerRadius(circle)).toBe(800_000);
    expect(padCornerRadius(padWith({ shape: "rect" }))).toBe(0);
    expect(padCornerRadius(padWith({ shape: "trapezoid" }))).toBe(0);
  });
});

describe("offsetFootprint", () => {
  it("moves the origin, the pads, and the bounds", () => {
    const moved = offsetFootprint(resistorFootprint(), 635_000, -635_000);
    expect(moved.position).toEqual([20_635_000, 19_365_000]);
    expect(moved.pads[0]?.position).toEqual([19_810_000, 19_365_000]);
    expect(moved.bounds.min).toEqual([19_155_000, 18_635_000]);
  });
});

describe("shiftRatsnest", () => {
  const other = resistorFootprint({
    uuid: "footprint-2",
    pads: [padWith({ position: [30_000_000, 20_000_000] })],
  });
  const lines: RatsnestSegment[] = [
    {
      net: "GND",
      from: [19_175_000, 20_000_000],
      to: [30_000_000, 20_000_000],
    },
    { net: "VCC", from: [40_000_000, 0], to: [20_825_000, 20_000_000] },
  ];

  it("moves only the ends on the dragged footprint's pads", () => {
    const shifted = shiftRatsnest(
      lines,
      [resistorFootprint(), other],
      ["footprint-1"],
      { x: 1_000_000, y: 0 },
    );
    expect(shifted).toEqual([
      {
        net: "GND",
        from: [20_175_000, 20_000_000],
        to: [30_000_000, 20_000_000],
      },
      { net: "VCC", from: [40_000_000, 0], to: [21_825_000, 20_000_000] },
    ]);
  });

  it("leaves the lines alone without a drag", () => {
    const footprints = [resistorFootprint(), other];
    const still = shiftRatsnest(lines, footprints, ["footprint-1"], {
      x: 0,
      y: 0,
    });
    expect(still).toBe(lines);
    expect(shiftRatsnest(lines, footprints, [], { x: 5, y: 5 })).toBe(lines);
  });
});
