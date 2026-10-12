import { colors, fonts, stagger } from "../theme";
import { DrawPath } from "./DrawPath";

const PAD_X = 620;
const VIA_X = 1360;

type Trace = {
  startY: number;
  endY: number;
  bendX: number;
};

const TRACES: readonly Trace[] = [
  { startY: 330, endY: 230, bendX: 760 },
  { startY: 380, endY: 330, bendX: 820 },
  { startY: 430, endY: 430, bendX: 880 },
  { startY: 480, endY: 530, bendX: 880 },
  { startY: 530, endY: 630, bendX: 820 },
  { startY: 580, endY: 730, bendX: 760 },
];

function tracePath({ startY, endY, bendX }: Trace): string {
  const diagonal = Math.abs(endY - startY);
  return [
    `M ${PAD_X} ${startY}`,
    `L ${bendX} ${startY}`,
    `L ${bendX + diagonal} ${endY}`,
    `L ${VIA_X} ${endY}`,
  ].join(" ");
}

type PcbTraceGraphicProps = {
  boardOpacity: number;
  progress: number;
  viaScale: number;
};

export function PcbTraceGraphic({
  boardOpacity,
  progress,
  viaScale,
}: PcbTraceGraphicProps) {
  return (
    <svg viewBox="0 0 1920 1080" width={1920} height={1080}>
      <defs>
        <filter id="trace-glow" x="-20%" y="-20%" width="140%" height="140%">
          <feGaussianBlur stdDeviation={6} result="blur" />
          <feMerge>
            <feMergeNode in="blur" />
            <feMergeNode in="SourceGraphic" />
          </feMerge>
        </filter>
      </defs>
      <g opacity={boardOpacity}>
        <rect
          x={380}
          y={120}
          width={1160}
          height={670}
          rx={24}
          fill={colors.surface}
          stroke={colors.textSecondary}
          strokeOpacity={0.35}
          strokeWidth={3}
        />
        <rect
          x={490}
          y={300}
          width={100}
          height={310}
          fill="none"
          stroke={colors.textSecondary}
          strokeWidth={3}
        />
        <text
          x={540}
          y={280}
          textAnchor="middle"
          fill={colors.textSecondary}
          fontFamily={fonts.mono}
          fontSize={32}
        >
          U1
        </text>
        <g fill={colors.accent}>
          {TRACES.map(({ startY }) => (
            <g key={startY}>
              <rect x={440} y={startY - 14} width={40} height={28} rx={4} />
              <rect x={600} y={startY - 14} width={40} height={28} rx={4} />
            </g>
          ))}
        </g>
      </g>
      <g stroke={colors.accent} strokeWidth={14} filter="url(#trace-glow)">
        {TRACES.map((trace, index) => (
          <DrawPath
            key={trace.startY}
            d={tracePath(trace)}
            progress={stagger(progress, index, TRACES.length)}
          />
        ))}
      </g>
      {TRACES.map(({ endY }) => (
        <circle
          key={endY}
          r={18}
          transform={`translate(${VIA_X} ${endY}) scale(${viaScale})`}
          fill={colors.background}
          stroke={colors.accent}
          strokeWidth={8}
        />
      ))}
    </svg>
  );
}
