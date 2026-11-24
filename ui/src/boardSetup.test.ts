/**
 * Tests for the Board Outline and Layer Stackup dialogs' model: reading the
 * current outline, the polygon checks, millimeter parsing, and the default
 * stackup preview.
 */
import { describe, expect, it } from "vitest";
import type { OutlineGraphic } from "./bindings/OutlineGraphic";
import type { Xy } from "./bindings/Xy";
import {
  outlineDraft,
  parseMm,
  polygonProblem,
  rectangleCorners,
  stackupPreview,
} from "./boardSetup";
import { resistorFootprint } from "./testData";

const MM = 1_000_000;

/** A point at whole millimeters. */
function mm(x: number, y: number): Xy {
  return [x * MM, y * MM];
}

/** An outline graphic with the usual 0.1 mm stroke. */
function graphic(geometry: OutlineGraphic["geometry"]): OutlineGraphic {
  return { uuid: null, geometry, widthNm: 100_000 };
}

/** A straight outline segment. */
function line(start: Xy, end: Xy): OutlineGraphic {
  return graphic({ type: "line", start, end });
}

describe("parseMm", () => {
  it("reads millimeters to whole nanometers", () => {
    expect(parseMm("12.5")).toBe(12_500_000);
    expect(parseMm(" -0.635 ")).toBe(-635_000);
    expect(parseMm(".5")).toBe(500_000);
    expect(parseMm("3.")).toBe(3_000_000);
  });

  it("refuses text that is not a plain number", () => {
    expect(parseMm("")).toBeNull();
    expect(parseMm("1e3")).toBeNull();
    expect(parseMm("10mm")).toBeNull();
    expect(parseMm("-")).toBeNull();
  });
});

describe("outlineDraft", () => {
  it("opens a rectangle as a rectangle", () => {
    const draft = outlineDraft(
      [graphic({ type: "rect", start: mm(50, 40), end: mm(10, 0) })],
      [],
    );
    expect(draft.mode).toBe("rectangle");
    expect(draft.origin).toEqual(mm(10, 0));
    expect([draft.width, draft.height]).toEqual([40 * MM, 40 * MM]);
    expect(draft.replacesCurves).toBe(false);
  });

  it("opens a polygon without its closing corner", () => {
    const points = [mm(0, 0), mm(30, 0), mm(15, 20), mm(0, 0)];
    const draft = outlineDraft([graphic({ type: "polygon", points })], []);
    expect(draft.mode).toBe("polygon");
    expect(draft.points).toEqual(points.slice(0, 3));
  });

  it("joins lines that close into one loop, in any direction", () => {
    const draft = outlineDraft(
      [
        line(mm(0, 0), mm(40, 0)),
        line(mm(0, 30), mm(40, 30)),
        line(mm(40, 0), mm(40, 30)),
        line(mm(0, 30), mm(0, 0)),
      ],
      [],
    );
    expect(draft.mode).toBe("polygon");
    expect(draft.points).toEqual([mm(0, 0), mm(40, 0), mm(40, 30), mm(0, 30)]);
  });

  it("replaces curves with the rectangle around them", () => {
    const draft = outlineDraft(
      [graphic({ type: "circle", center: mm(20, 20), end: mm(30, 20) })],
      [],
    );
    expect(draft.mode).toBe("rectangle");
    expect(draft.origin).toEqual(mm(10, 10));
    expect([draft.width, draft.height]).toEqual([20 * MM, 20 * MM]);
    expect(draft.replacesCurves).toBe(true);
  });

  it("treats an open chain of lines as a shape to replace", () => {
    const draft = outlineDraft(
      [line(mm(0, 0), mm(10, 0)), line(mm(10, 0), mm(10, 10))],
      [],
    );
    expect(draft.mode).toBe("rectangle");
    expect(draft.replacesCurves).toBe(true);
  });

  it("surrounds the footprints of a board without an outline", () => {
    const draft = outlineDraft([], [resistorFootprint()]);
    expect(draft.origin).toEqual([8_520_000, 9_270_000]);
    expect([draft.width, draft.height]).toEqual([22_960_000, 21_460_000]);
    expect(draft.replacesCurves).toBe(false);
  });

  it("offers a 100 mm square on an empty board", () => {
    const draft = outlineDraft([], []);
    expect(draft.points).toEqual(rectangleCorners([0, 0], 100 * MM, 100 * MM));
    expect(draft.replacesCurves).toBe(false);
  });
});

describe("polygonProblem", () => {
  it("accepts simple polygons in either winding", () => {
    const square = [mm(0, 0), mm(10, 0), mm(10, 10), mm(0, 10)];
    expect(polygonProblem(square)).toBeNull();
    expect(polygonProblem([...square].reverse())).toBeNull();
    const notch = [mm(0, 0), mm(10, 0), mm(10, 10), mm(5, 5), mm(0, 10)];
    expect(polygonProblem(notch)).toBeNull();
  });

  it("needs three distinct corners and an area", () => {
    expect(polygonProblem([mm(0, 0), mm(5, 0), mm(5, 0), mm(0, 0)])).toBe(
      "The outline needs at least 3 distinct points.",
    );
    expect(polygonProblem([mm(0, 0), mm(5, 0), mm(10, 0)])).toBe(
      "The outline has zero area.",
    );
  });

  it("refuses crossing, folded, and touching edges", () => {
    const crossing = "The outline crosses itself.";
    const bowtie = [mm(0, 0), mm(10, 10), mm(10, 0), mm(0, 10)];
    expect(polygonProblem(bowtie)).toBe(crossing);
    const folded = [mm(0, 0), mm(10, 0), mm(10, 10), mm(5, 0)];
    expect(polygonProblem(folded)).toBe(crossing);
    const onEdge = [mm(0, 0), mm(10, 0), mm(10, 10), mm(5, 10), mm(5, 0)];
    expect(polygonProblem(onEdge)).toBe(crossing);
  });

  it("stays exact at KiCad's largest coordinates", () => {
    const edge = 2_147_483_647;
    const triangle: Xy[] = [
      [-edge, -edge],
      [edge, -edge],
      [edge, edge],
    ];
    expect(polygonProblem(triangle)).toBeNull();
    const sliver: Xy[] = [
      [-edge, -edge],
      [edge, edge - 1],
      [edge - 1, edge - 1],
    ];
    expect(polygonProblem(sliver)).toBeNull();
  });
});

describe("stackupPreview", () => {
  it("has one core between two copper layers", () => {
    const rows = stackupPreview(2);
    expect(rows.map((row) => row.layer)).toEqual([
      "F.Mask",
      "F.Cu",
      "dielectric 1",
      "B.Cu",
      "B.Mask",
    ]);
    expect(rows[2]).toEqual({
      layer: "dielectric 1",
      type: "core",
      material: "FR4",
      thicknessNm: 1_510_000,
      epsilonR: 4.5,
      lossTangent: 0.02,
    });
  });

  it("alternates prepreg and core on thicker boards", () => {
    const rows = stackupPreview(4);
    const dielectrics = rows.filter((row) => row.material === "FR4");
    expect(dielectrics.map((row) => row.type)).toEqual([
      "prepreg",
      "core",
      "prepreg",
    ]);
    expect(dielectrics[0]?.thicknessNm).toBe(480_000);
    const total = rows.reduce((sum, row) => sum + row.thicknessNm, 0);
    expect(total).toBe(1_600_000);
    const eight = stackupPreview(8).filter((row) => row.type === "copper");
    expect(eight.map((row) => row.layer)[3]).toBe("In3.Cu");
    expect(stackupPreview(8)[2]?.thicknessNm).toBe(185_714);
  });
});
