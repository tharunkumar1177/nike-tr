import { colors, fonts, stagger } from "../theme";
import { DrawPath } from "./DrawPath";

const STROKES = [
  "M 460 440 A 60 60 0 1 0 580 440 A 60 60 0 1 0 460 440",
  "M 485 440 Q 502.5 405 520 440 T 555 440",
  "M 520 380 L 520 260 L 820 260",
  "M 820 260 L 845 260 L 860 230 L 890 290 L 920 230 L 950 290 L 980 230 L 1010 290 L 1040 230 L 1055 260 L 1100 260",
  "M 1100 260 L 1470 260",
  "M 1300 260 L 1300 410",
  "M 1240 410 L 1360 410 M 1240 440 L 1360 440",
  "M 1300 440 L 1300 620",
  "M 520 500 L 520 620 L 1470 620",
  "M 900 620 L 900 680 M 860 680 L 940 680 M 875 700 L 925 700 M 890 720 L 910 720",
  "M 1470 260 A 10 10 0 1 0 1490 260 A 10 10 0 1 0 1470 260",
  "M 1470 620 A 10 10 0 1 0 1490 620 A 10 10 0 1 0 1470 620",
];

const JUNCTIONS: ReadonlyArray<readonly [number, number]> = [
  [1300, 260],
  [1300, 620],
];

type Label = {
  text: string;
  x: number;
  y: number;
  anchor: "start" | "middle" | "end";
};

const LABELS: readonly Label[] = [
  { text: "V1", x: 430, y: 451, anchor: "end" },
  { text: "R1", x: 950, y: 205, anchor: "middle" },
  { text: "C1", x: 1390, y: 436, anchor: "start" },
  { text: "OUT", x: 1510, y: 271, anchor: "start" },
];

type SchematicGraphicProps = {
  progress: number;
};

export function SchematicGraphic({ progress }: SchematicGraphicProps) {
  const detail = Math.min(1, Math.max(0, (progress - 0.85) / 0.15));
  return (
    <svg viewBox="0 0 1920 1080" width={1920} height={1080}>
      <g stroke={colors.accent} strokeWidth={6}>
        {STROKES.map((d, index) => (
          <DrawPath
            key={d}
            d={d}
            progress={stagger(progress, index, STROKES.length)}
          />
        ))}
      </g>
      {JUNCTIONS.map(([x, y]) => (
        <circle
          key={`${x}-${y}`}
          cx={x}
          cy={y}
          r={10 * detail}
          fill={colors.accent}
        />
      ))}
      {LABELS.map((label) => (
        <text
          key={label.text}
          x={label.x}
          y={label.y}
          textAnchor={label.anchor}
          fill={colors.textSecondary}
          fontFamily={fonts.mono}
          fontSize={32}
          opacity={detail}
        >
          {label.text}
        </text>
      ))}
    </svg>
  );
}
