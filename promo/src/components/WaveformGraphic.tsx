import { colors, fonts } from "../theme";
import { DrawPath } from "./DrawPath";

const PLOT = { left: 240, right: 1680, top: 200, bottom: 650 } as const;
const VOLTS_TO_PX = 300;
const END_TIME = 2;
const STEP_TIME = 0.1;
const DECAY_TIME = 0.25;
const OMEGA = 4 * Math.PI;
const SAMPLES = 360;

type Point = { x: number; y: number };

function toX(time: number): number {
  return PLOT.left + (time / END_TIME) * (PLOT.right - PLOT.left);
}

function toY(volts: number): number {
  return PLOT.bottom - volts * VOLTS_TO_PX;
}

function stepResponse(time: number): number {
  const elapsed = time - STEP_TIME;
  const phase = OMEGA * elapsed;
  const ringing = Math.cos(phase) + Math.sin(phase) / (OMEGA * DECAY_TIME);
  return 1 - Math.exp(-elapsed / DECAY_TIME) * ringing;
}

function sampleOutput(): Point[] {
  const points: Point[] = [];
  for (let index = 0; index <= SAMPLES; index += 1) {
    const time = STEP_TIME + (index / SAMPLES) * (END_TIME - STEP_TIME);
    points.push({ x: toX(time), y: toY(stepResponse(time)) });
  }
  return points;
}

function toPath(points: readonly Point[]): string {
  const commands = points.map((point, index) => {
    const command = index === 0 ? "M" : "L";
    return `${command} ${point.x.toFixed(1)} ${point.y.toFixed(1)}`;
  });
  return commands.join(" ");
}

const OUTPUT_PATH = toPath(sampleOutput());

const INPUT_PATH = toPath([
  { x: toX(0), y: toY(0) },
  { x: toX(STEP_TIME), y: toY(0) },
  { x: toX(STEP_TIME), y: toY(1) },
  { x: toX(END_TIME), y: toY(1) },
]);

const AXES_PATH = toPath([
  { x: PLOT.left, y: PLOT.top },
  { x: PLOT.left, y: PLOT.bottom },
  { x: PLOT.right, y: PLOT.bottom },
]);

const PEAK_TIME = STEP_TIME + Math.PI / OMEGA;

export const WAVEFORM_PEAK: Point = {
  x: toX(PEAK_TIME),
  y: toY(stepResponse(PEAK_TIME)),
};

const GRID_X = Array.from({ length: 13 }, (_, i) => PLOT.left + i * 120);
const GRID_Y = Array.from({ length: 7 }, (_, i) => PLOT.top + i * 75);

type WaveformGraphicProps = {
  progress: number;
  gridOpacity: number;
};

export function WaveformGraphic({
  progress,
  gridOpacity,
}: WaveformGraphicProps) {
  return (
    <svg viewBox="0 0 1920 1080" width={1920} height={1080}>
      <g opacity={gridOpacity}>
        <g stroke={colors.surface} strokeWidth={2}>
          {GRID_X.map((x) => (
            <line key={x} x1={x} y1={PLOT.top} x2={x} y2={PLOT.bottom} />
          ))}
          {GRID_Y.map((y) => (
            <line key={y} x1={PLOT.left} y1={y} x2={PLOT.right} y2={y} />
          ))}
        </g>
        <path
          d={AXES_PATH}
          fill="none"
          stroke={colors.textSecondary}
          strokeOpacity={0.5}
          strokeWidth={2}
        />
        <g fontFamily={fonts.mono} fontSize={32} textAnchor="end">
          <text x={1480} y={160} fill={colors.error}>
            V(in)
          </text>
          <text x={PLOT.right} y={160} fill={colors.accent}>
            V(out)
          </text>
        </g>
      </g>
      <DrawPath
        d={INPUT_PATH}
        progress={progress}
        stroke={colors.error}
        strokeWidth={5}
      />
      <DrawPath
        d={OUTPUT_PATH}
        progress={progress}
        stroke={colors.accent}
        strokeWidth={6}
      />
    </svg>
  );
}
