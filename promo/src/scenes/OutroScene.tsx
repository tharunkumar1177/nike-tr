import {
  AbsoluteFill,
  spring,
  useCurrentFrame,
  useVideoConfig,
} from "remotion";
import { KineticText } from "../components/KineticText";
import { colors, ramp, snappySpring, textStyles } from "../theme";

export function OutroScene() {
  const frame = useCurrentFrame();
  const { fps } = useVideoConfig();
  const titleScale = spring({ frame: frame - 10, fps, config: snappySpring });
  const exit = ramp(frame, 165, 180);
  return (
    <AbsoluteFill
      style={{
        alignItems: "center",
        justifyContent: "center",
        gap: 32,
        opacity: 1 - exit,
      }}
    >
      <div
        style={{
          ...textStyles.title,
          color: colors.textPrimary,
          transform: `scale(${titleScale})`,
        }}
      >
        Tron.
      </div>
      <KineticText
        text="Local-first EDA."
        delay={40}
        variant="subtitle"
        color={colors.textSecondary}
      />
      <KineticText
        text="github.com/tharunkumar1177/tron"
        delay={80}
        variant="code"
        color={colors.accent}
        distance={0}
      />
    </AbsoluteFill>
  );
}
