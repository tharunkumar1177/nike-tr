import {
  AbsoluteFill,
  spring,
  useCurrentFrame,
  useVideoConfig,
} from "remotion";
import { Headline } from "../components/Headline";
import { PcbTraceGraphic } from "../components/PcbTraceGraphic";
import { ramp, snappySpring } from "../theme";

export function LayoutScene() {
  const frame = useCurrentFrame();
  const { fps } = useVideoConfig();
  const board = ramp(frame, 0, 10);
  const draw = ramp(frame, 10, 100);
  const vias = spring({
    frame: frame - 100,
    fps,
    config: snappySpring,
    durationInFrames: 20,
  });
  const exit = ramp(frame, 165, 180);
  return (
    <AbsoluteFill style={{ opacity: 1 - exit }}>
      <PcbTraceGraphic boardOpacity={board} progress={draw} viaScale={vias} />
      <Headline text="Route precisely." delay={90} />
    </AbsoluteFill>
  );
}
