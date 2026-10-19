import { describe, expect, it } from "vitest";
import { formatOpValue, formatQuantity, formatSi, parseSi } from "./si";

describe("parseSi", () => {
  it("reads SPICE suffixes, ignoring case", () => {
    expect(parseSi("10u")).toBe(1e-5);
    expect(parseSi("10µ")).toBe(1e-5);
    expect(parseSi("2.5m")).toBe(2.5e-3);
    expect(parseSi("2.5M")).toBe(2.5e-3);
    expect(parseSi("1Meg")).toBe(1e6);
    expect(parseSi("150n")).toBe(1.5e-7);
    expect(parseSi("3k")).toBe(3000);
    expect(parseSi("1f")).toBe(1e-15);
    expect(parseSi("2T")).toBe(2e12);
  });

  it("reads plain, exponent, and 4k7 forms", () => {
    expect(parseSi("0")).toBe(0);
    expect(parseSi(" 0.001 ")).toBe(0.001);
    expect(parseSi("1e-6")).toBe(1e-6);
    expect(parseSi("-2.5E3")).toBe(-2500);
    expect(parseSi("4k7")).toBe(4700);
    expect(parseSi(".5")).toBe(0.5);
  });

  it("rejects text that is not a number", () => {
    for (const text of ["", "abc", "1.2.3", "1e", "k", "4.5k7", "1x", "--1"]) {
      expect(parseSi(text)).toBeNull();
    }
  });
});

describe("formatSi", () => {
  it("writes values that read back exactly", () => {
    expect(formatSi(1e-5)).toBe("10u");
    expect(formatSi(2.5e-3)).toBe("2.5m");
    expect(formatSi(0)).toBe("0");
    expect(formatSi(1.5e-7)).toBe("150n");
    expect(formatSi(10)).toBe("10");
    expect(formatSi(1e6)).toBe("1Meg");
    expect(formatSi(-4700)).toBe("-4.7k");
    for (const value of [1e-5, 2.5e-3, 1.5e-7, 0.1 + 0.2, 1e-18, 123456]) {
      expect(parseSi(formatSi(value))).toBe(value);
    }
  });
});

describe("formatOpValue", () => {
  it("shows 3 significant digits with an SI prefix", () => {
    expect(formatOpValue(3.3, "voltage")).toBe("3.30 V");
    expect(formatOpValue(1.2345e-3, "current")).toBe("1.23 mA");
    expect(formatOpValue(-1.25e-3, "current")).toBe("-1.25 mA");
    expect(formatOpValue(-4567.8, "voltage")).toBe("-4.57 kV");
    expect(formatOpValue(15000, "current")).toBe("15.0 kA");
    expect(formatOpValue(2.2e-6, "current")).toBe("2.20 µA");
    expect(formatOpValue(0.9996, "voltage")).toBe("1.00 V");
    expect(formatOpValue(999.7, "voltage")).toBe("1.00 kV");
  });

  it("shows tiny values as zero and huge ones in exponent form", () => {
    expect(formatOpValue(0, "voltage")).toBe("0 V");
    expect(formatOpValue(1e-18, "voltage")).toBe("0 V");
    expect(formatOpValue(-3e-17, "current")).toBe("0 A");
    expect(formatOpValue(1.234e15, "voltage")).toBe("1.23e15 V");
  });
});

describe("formatQuantity", () => {
  it("formats any unit the way operating-point values are", () => {
    expect(formatQuantity(2.5e-4, "s")).toBe("250 µs");
    expect(formatQuantity(-5e-4, "s")).toBe("-500 µs");
    expect(formatQuantity(2000, "Hz")).toBe("2.00 kHz");
    expect(formatQuantity(0, "s")).toBe("0 s");
  });
});
