/** Powers of ten for SPICE suffixes. SPICE ignores case, so `M` is milli. */
const SUFFIX_EXPONENTS: Record<string, number> = {
  f: -15,
  p: -12,
  n: -9,
  u: -6,
  µ: -6,
  m: -3,
  k: 3,
  meg: 6,
  g: 9,
  t: 12,
};

/** The suffix `formatSi` writes for each power of ten. */
const ASCII_PREFIXES = new Map<number, string>([
  [-15, "f"],
  [-12, "p"],
  [-9, "n"],
  [-6, "u"],
  [-3, "m"],
  [0, ""],
  [3, "k"],
  [6, "Meg"],
  [9, "G"],
  [12, "T"],
]);

const DISPLAY_PREFIXES = new Map<number, string>([
  [-15, "f"],
  [-12, "p"],
  [-9, "n"],
  [-6, "µ"],
  [-3, "m"],
  [0, ""],
  [3, "k"],
  [6, "M"],
  [9, "G"],
  [12, "T"],
]);

const SI_PATTERN =
  /^([+-]?)(\d+(?:\.\d*)?|\.\d+)(?:e([+-]?\d+))?(meg|[fpnuµmkgt])?(\d*)$/i;

/**
 * Parses a SPICE number such as `10u`, `2.5m`, `1Meg`, `4k7`, or `1e-6`.
 * Returns `null` when the text is not one.
 */
export function parseSi(text: string): number | null {
  const match = SI_PATTERN.exec(text.trim());
  const mantissa = match?.[2];
  if (!match || mantissa === undefined) {
    return null;
  }
  const [, sign = "", , exponent, suffix, fraction] = match;
  if (
    fraction &&
    (!suffix || exponent !== undefined || mantissa.includes("."))
  ) {
    return null;
  }
  const digits = fraction ? `${mantissa}.${fraction}` : mantissa;
  const suffixPower = SUFFIX_EXPONENTS[suffix?.toLowerCase() ?? ""] ?? 0;
  const power = Number(exponent ?? 0) + suffixPower;
  const value = Number(`${sign}${digits}e${power}`);
  return Number.isFinite(value) ? value : null;
}

/**
 * Writes `value` with an ASCII SPICE suffix so `parseSi` reads back the same
 * number: `1e-5` is `10u`, `2.5e-3` is `2.5m`, and `1.5e-7` is `150n`.
 */
export function formatSi(value: number): string {
  if (value === 0) {
    return "0";
  }
  const [mantissa = "", exponentText = "0"] = value.toExponential().split("e");
  const exponent = Number(exponentText);
  const engineering = Math.floor(exponent / 3) * 3;
  const prefix = ASCII_PREFIXES.get(engineering);
  if (prefix === undefined) {
    return value.toExponential();
  }
  const sign = mantissa.startsWith("-") ? "-" : "";
  const digits = mantissa.replace("-", "").replace(".", "");
  const point = 1 + exponent - engineering;
  const whole = digits.slice(0, point).padEnd(point, "0");
  const fraction = digits.slice(point);
  return `${sign}${whole}${fraction ? `.${fraction}` : ""}${prefix}`;
}

/** Milliseconds as seconds to one decimal place, such as `1.3`. */
export function formatSeconds(elapsedMs: number): string {
  return (elapsedMs / 1000).toFixed(1);
}

/**
 * An operating-point value to 3 significant digits with an SI prefix, such as
 * `3.30 V` or `-1.25 mA`.
 */
export function formatOpValue(
  value: number,
  kind: "voltage" | "current",
): string {
  const unit = kind === "voltage" ? "V" : "A";
  const magnitude = Math.abs(value);
  if (Number.isNaN(value)) {
    return `NaN ${unit}`;
  }
  if (magnitude < 1e-15) {
    return `0 ${unit}`;
  }
  const sign = value < 0 ? "-" : "";
  if (magnitude >= 1e15) {
    return `${sign}${magnitude.toPrecision(3).replace("+", "")} ${unit}`;
  }
  let engineering = Math.floor(Math.log10(magnitude) / 3) * 3;
  if (magnitude / 10 ** engineering < 1) {
    engineering -= 3;
  }
  let text = (magnitude / 10 ** engineering).toPrecision(3);
  if (Number(text) >= 1000 && engineering < 12) {
    engineering += 3;
    text = (magnitude / 10 ** engineering).toPrecision(3);
  }
  return `${sign}${text} ${DISPLAY_PREFIXES.get(engineering) ?? ""}${unit}`;
}
