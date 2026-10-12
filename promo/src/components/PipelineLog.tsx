import type { CSSProperties } from "react";
import { useCurrentFrame } from "remotion";
import { colors, SAFE_MARGIN, textStyles } from "../theme";

type LogLine = {
  text: string;
  start: number;
  check: boolean;
};

const LINES: readonly LogLine[] = [
  { text: "> Active bandpass filter", start: 10, check: false },
  { text: "Compiling circuit", start: 45, check: true },
  { text: "Passing ERC", start: 70, check: true },
  { text: "Verifying simulation", start: 90, check: true },
];

const CURSOR_BLINK_FRAMES = 15;

const block: CSSProperties = {
  ...textStyles.code,
  position: "absolute",
  left: SAFE_MARGIN,
  top: 400,
  width: 800,
  whiteSpace: "pre",
};

const cursor: CSSProperties = {
  display: "inline-block",
  width: "0.6em",
  height: "1em",
  verticalAlign: "-0.15em",
  backgroundColor: colors.textPrimary,
};

function typedLength(line: LogLine, frame: number): number {
  const total = line.text.length + (line.check ? 2 : 0);
  return Math.min(total, Math.max(0, frame - line.start + 1));
}

function CheckMark() {
  return (
    <svg
      width="0.6em"
      height="0.6em"
      viewBox="0 0 24 24"
      style={{ verticalAlign: "-0.05em" }}
    >
      <path
        d="M 3 13 L 9 19 L 21 5"
        fill="none"
        stroke={colors.accent}
        strokeWidth={3.5}
        strokeLinecap="round"
        strokeLinejoin="round"
      />
    </svg>
  );
}

export function PipelineLog() {
  const frame = useCurrentFrame();
  const started = LINES.filter((line) => frame >= line.start);
  const activeIndex = Math.max(0, started.length - 1);
  const cursorOn = Math.floor(frame / CURSOR_BLINK_FRAMES) % 2 === 0;
  const visibleLines = started.length > 0 ? started : LINES.slice(0, 1);

  return (
    <div style={block}>
      {visibleLines.map((line, index) => {
        const typed = typedLength(line, frame);
        const textLength = line.check ? Math.max(0, typed - 2) : typed;
        const color = line.check ? colors.textSecondary : colors.textPrimary;
        const showCursor = index === activeIndex && cursorOn;
        return (
          <div key={line.text}>
            {line.check && typed >= 1 ? <CheckMark /> : null}
            {line.check && typed >= 2 ? " " : null}
            <span style={{ color }}>{line.text.slice(0, textLength)}</span>
            {showCursor ? <span style={cursor} /> : null}
          </div>
        );
      })}
    </div>
  );
}
