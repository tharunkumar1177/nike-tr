import { describe, expect, it } from "vitest";
import {
  clampView,
  measuredSpan,
  panView,
  placedCursors,
  zoomView,
} from "./waveformView";

const RUN = { min: 0, max: 1e-3 };

describe("zoomView", () => {
  it("keeps the time under the pointer in place", () => {
    const zoomed = zoomView(RUN, RUN, 2.5e-4, 0.5)!;
    expect(zoomed.min).toBeCloseTo(1.25e-4, 15);
    expect(zoomed.max).toBeCloseTo(6.25e-4, 15);
    const fraction = (2.5e-4 - zoomed.min) / (zoomed.max - zoomed.min);
    expect(fraction).toBeCloseTo(0.25, 12);
  });

  it("returns null once the whole run shows", () => {
    expect(zoomView({ min: 1e-4, max: 9e-4 }, RUN, 5e-4, 2)).toBeNull();
  });

  it("stops at a millionth of the run", () => {
    const zoomed = zoomView({ min: 0, max: 1e-9 }, RUN, 0, 1e-3)!;
    expect(zoomed.max - zoomed.min).toBeCloseTo(1e-9, 20);
  });
});

describe("panView", () => {
  it("moves the view without leaving the run", () => {
    const later = panView({ min: 1e-4, max: 3e-4 }, RUN, 1e-4);
    expect(later.min).toBeCloseTo(2e-4, 15);
    expect(later.max).toBeCloseTo(4e-4, 15);
    const end = panView({ min: 1e-4, max: 3e-4 }, RUN, 5e-3);
    expect(end.max).toBeCloseTo(1e-3, 15);
    expect(end.max - end.min).toBeCloseTo(2e-4, 15);
    expect(panView({ min: 1e-4, max: 3e-4 }, RUN, -1).min).toBe(0);
  });

  it("narrows a view wider than the run to the run", () => {
    expect(clampView({ min: -1, max: 2 }, RUN)).toEqual(RUN);
  });
});

describe("measuredSpan", () => {
  it("covers the cursors when both are placed, in time order", () => {
    expect(measuredSpan({ a: 3e-4, b: 1e-4 }, null)).toEqual({
      from: 1e-4,
      to: 3e-4,
      betweenCursors: true,
    });
  });

  it("covers the view, or the whole run, otherwise", () => {
    const view = { min: 1e-4, max: 2e-4 };
    expect(measuredSpan({ a: 5e-4, b: null }, view)).toEqual({
      from: 1e-4,
      to: 2e-4,
      betweenCursors: false,
    });
    expect(measuredSpan({ a: null, b: null }, null)).toEqual({
      from: null,
      to: null,
      betweenCursors: false,
    });
  });

  it("lists the placed cursors A first", () => {
    expect(placedCursors({ a: 2, b: 1 })).toEqual([2, 1]);
    expect(placedCursors({ a: null, b: 1 })).toEqual([1]);
    expect(placedCursors({ a: null, b: null })).toEqual([]);
  });
});
