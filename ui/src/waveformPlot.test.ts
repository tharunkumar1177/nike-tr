import { describe, expect, it } from "vitest";
import {
  type XTick,
  frequencyLabel,
  logTicks,
  niceAxis,
  phaseAxis,
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

describe("phaseAxis", () => {
  it("steps by 45° or a power of two times it", () => {
    expect(phaseAxis(-90, 0).ticks).toEqual([-90, -45, 0]);
    expect(phaseAxis(-170, 10).ticks).toEqual([-180, -135, -90, -45, 0, 45]);
    expect(phaseAxis(-360, 0).ticks).toEqual([-360, -270, -180, -90, 0]);
  });

  it("opens up a flat phase by a step each side", () => {
    expect(phaseAxis(0, 0).ticks).toEqual([-45, 0, 45]);
  });
});

describe("frequencyLabel", () => {
  it("drops trailing zeros", () => {
    expect(frequencyLabel(10)).toBe("10 Hz");
    expect(frequencyLabel(100)).toBe("100 Hz");
    expect(frequencyLabel(1e3)).toBe("1 kHz");
    expect(frequencyLabel(2.5e6)).toBe("2.5 MHz");
    expect(frequencyLabel(159.15)).toBe("159 Hz");
  });
});

describe("logTicks", () => {
  const labelled = (ticks: XTick[]) =>
    ticks.flatMap((tick) => (tick.label === null ? [] : [tick.label]));

  it("labels each decade and grids its multiples", () => {
    const ticks = logTicks({ min: 0, max: 3 }, 200);
    expect(ticks).toHaveLength(28);
    expect(labelled(ticks)).toEqual(["1 Hz", "10 Hz", "100 Hz", "1 kHz"]);
    expect(ticks[1]!.at).toBeCloseTo(Math.log10(2), 12);
  });

  it("skips decade labels that would overlap, and minors when cramped", () => {
    const ticks = logTicks({ min: 0, max: 6 }, 20);
    expect(ticks).toHaveLength(7);
    expect(labelled(ticks)).toEqual(["1 Hz", "1 kHz", "1 MHz"]);
  });

  it("labels the multiples when fewer than two decades show", () => {
    const ticks = logTicks({ min: 2.1, max: 2.9 }, 400);
    expect(labelled(ticks)).toEqual(["200 Hz", "300 Hz", "500 Hz", "700 Hz"]);
  });

  it("falls back to round frequencies in a narrow view", () => {
    const view = { min: Math.log10(150), max: Math.log10(170) };
    const ticks = logTicks(view, 4000);
    expect(labelled(ticks)).toEqual([
      "150 Hz",
      "155 Hz",
      "160 Hz",
      "165 Hz",
      "170 Hz",
    ]);
  });
});
