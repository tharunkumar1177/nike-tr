import { AbsoluteFill, interpolate, useCurrentFrame } from "remotion";
import { Headline } from "../components/Headline";
import { WAVEFORM_PEAK, WaveformGraphic } from "../components/WaveformGraphic";
import { ramp } from "../theme";

export function SimulationScene() {
  const frame = useCurrentFrame();
  const grid = ramp(frame, 0, 20);
  const draw = ramp(frame, 10, 90);
  const exit = ramp(frame, 165, 180);
  const scale = interpolate(exit, [0, 1], [1, 20]);
  return (
    <AbsoluteFill
      style={{
        opacity: 1 - exit,
        transform: `scale(${scale})`,
        transformOrigin: `${WAVEFORM_PEAK.x}px ${WAVEFORM_PEAK.y}px`,
      }}
    >
      <WaveformGraphic progress={draw} gridOpacity={grid} />
      <Headline text="Simulate instantly." delay={60} />
    </AbsoluteFill>
  );
}
