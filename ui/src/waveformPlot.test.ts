import { describe, expect, it } from "vitest";
import { niceAxis, tickLabel } from "./waveformPlot";

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
