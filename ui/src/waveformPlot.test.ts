import { describe, expect, it } from "vitest";
import {
  niceAxis,
  pixelAtTime,
  plotArea,
  tickLabel,
  timeAtPixel,
  timeTicks,
  visiblePoints,
} from "./waveformPlot";

describe("plot geometry", () => {
  it("leaves room for the axes that are shown", () => {
    const size = { width: 400, height: 200 };
    expect(plotArea(size, true, false)).toEqual({
      left: 56,
      right: 388,
      top: 8,
      bottom: 176,
    });
    expect(plotArea(size, false, true).left).toBe(12);
    expect(plotArea(size, false, true).right).toBe(344);
  });

  it("maps times to pixels and back", () => {
    const area = plotArea({ width: 400, height: 200 }, true, false);
    const view = { min: 1e-3, max: 3e-3 };
    expect(pixelAtTime(area, view, 2e-3)).toBeCloseTo(222, 9);
    expect(timeAtPixel(area, view, 222)).toBeCloseTo(2e-3, 15);
    expect(timeAtPixel(area, view, 56)).toBe(1e-3);
  });
});

describe("timeTicks", () => {
  it("keeps only round ticks inside the view", () => {
    expect(timeTicks({ min: 0.5, max: 9.5 })).toEqual([2.5, 5, 7.5]);
    expect(timeTicks({ min: 0, max: 1e-3 })).toEqual([
      0, 2.5e-4, 5e-4, 7.5e-4, 1e-3,
    ]);
  });
});

describe("visiblePoints", () => {
  const points = [0, 1, 2, 3, 4].map((x) => ({ x, y: x * 10 }));

  it("keeps the samples in view and one neighbour each side", () => {
    const xs = visiblePoints(points, { min: 1.5, max: 2.5 }).map((p) => p.x);
    expect(xs).toEqual([1, 2, 3]);
  });

  it("keeps the nearest sample of a view between or beyond samples", () => {
    const between = visiblePoints(points, { min: 2.2, max: 2.4 });
    expect(between.map((p) => p.x)).toEqual([2, 3]);
    const after = visiblePoints(points, { min: 9, max: 10 });
    expect(after.map((p) => p.x)).toEqual([4]);
    const before = visiblePoints(points, { min: -2, max: -1 });
    expect(before.map((p) => p.x)).toEqual([0]);
  });
});

describe("niceAxis", () => {
  it("widens a range to round steps", () => {
    expect(niceAxis(0, 1e-3).ticks).toEqual([0, 2.5e-4, 5e-4, 7.5e-4, 1e-3]);
    expect(niceAxis(-3.3, 3.3).ticks).toEqual([-4, -2, 0, 2, 4]);
  });

  it("keeps zero on an axis that crosses it", () => {
    expect(niceAxis(-0.7, 2.1).ticks).toContain(0);
  });

  it("opens up a flat signal around its value", () => {
    const axis = niceAxis(5, 5);
    expect(axis.min).toBeLessThan(5);
    expect(axis.max).toBeGreaterThan(5);
    expect(niceAxis(0, 0).ticks).toContain(0);
  });
});

describe("tickLabel", () => {
  it("joins the SI value and its unit", () => {
    expect(tickLabel(2e-4, "s")).toBe("200us");
    expect(tickLabel(0, "A")).toBe("0A");
  });
});
