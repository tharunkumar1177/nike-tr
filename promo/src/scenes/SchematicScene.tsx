import { AbsoluteFill, useCurrentFrame, useVideoConfig } from "remotion";
import { Headline } from "../components/Headline";
import { SchematicGraphic } from "../components/SchematicGraphic";
import { ramp } from "../theme";

export function SchematicScene() {
  const frame = useCurrentFrame();
  const { width } = useVideoConfig();
  const draw = ramp(frame, 15, 75);
  const exit = ramp(frame, 165, 180);
  return (
    <AbsoluteFill style={{ transform: `translateX(${-exit * width}px)` }}>
      <SchematicGraphic progress={draw} />
      <Headline text="Design locally." delay={90} />
    </AbsoluteFill>
  );
}
