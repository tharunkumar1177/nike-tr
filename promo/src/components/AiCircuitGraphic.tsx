import { spring, useCurrentFrame, useVideoConfig } from "remotion";
import { colors, ramp, snappySpring } from "../theme";
import { DrawPath } from "./DrawPath";

const NODES: ReadonlyArray<readonly [number, number]> = [
  [1040, 480],
  [1220, 300],
  [1220, 660],
  [1420, 480],
  [1600, 300],
  [1600, 660],
  [1740, 480],
];

const EDGES: ReadonlyArray<readonly [number, number]> = [
  [0, 1],
  [0, 2],
  [1, 3],
  [2, 3],
  [3, 4],
  [3, 5],
  [4, 6],
  [5, 6],
];

const EDGES_START = 40;
const EDGE_STAGGER = 10;
const EDGE_FRAMES = 30;

function edgeStart(edgeIndex: number): number {
  return EDGES_START + edgeIndex * EDGE_STAGGER;
}

function nodeAppearFrame(nodeIndex: number): number {
  let appear = Number.POSITIVE_INFINITY;
  EDGES.forEach(([, to], edgeIndex) => {
    if (to === nodeIndex) {
      appear = Math.min(appear, edgeStart(edgeIndex) + EDGE_FRAMES);
    }
  });
  return Number.isFinite(appear) ? appear : EDGES_START;
}

function nodePoint(nodeIndex: number): readonly [number, number] {
  const node = NODES[nodeIndex];
  if (node === undefined) {
    throw new Error(`Edge references missing node ${nodeIndex}`);
  }
  return node;
}

export function AiCircuitGraphic() {
  const frame = useCurrentFrame();
  const { fps } = useVideoConfig();
  return (
    <svg viewBox="0 0 1920 1080" width={1920} height={1080}>
      <g stroke={colors.accent} strokeWidth={4} strokeOpacity={0.8}>
        {EDGES.map(([from, to], edgeIndex) => {
          const [x1, y1] = nodePoint(from);
          const [x2, y2] = nodePoint(to);
          const start = edgeStart(edgeIndex);
          return (
            <DrawPath
              key={`${from}-${to}`}
              d={`M ${x1} ${y1} L ${x2} ${y2}`}
              progress={ramp(frame, start, start + EDGE_FRAMES)}
            />
          );
        })}
      </g>
      {NODES.map(([x, y], nodeIndex) => {
        const scale = spring({
          frame: frame - nodeAppearFrame(nodeIndex),
          fps,
          config: snappySpring,
        });
        return (
          <circle
            key={`${x}-${y}`}
            r={22}
            transform={`translate(${x} ${y}) scale(${scale})`}
            fill={colors.background}
            stroke={colors.accent}
            strokeWidth={6}
          />
        );
      })}
    </svg>
  );
}
