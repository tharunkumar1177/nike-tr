import { AbsoluteFill, useCurrentFrame } from "remotion";
import { AiCircuitGraphic } from "../components/AiCircuitGraphic";
import { Headline } from "../components/Headline";
import { PipelineLog } from "../components/PipelineLog";
import { ramp } from "../theme";

export function AiScene() {
  const frame = useCurrentFrame();
  const exit = ramp(frame, 165, 180);
  return (
    <AbsoluteFill
      style={{
        transform: `scale(${1 - 0.2 * exit})`,
        filter: `blur(${20 * exit}px)`,
      }}
    >
      <AiCircuitGraphic />
      <PipelineLog />
      <Headline text="Generate with AI." delay={90} />
    </AbsoluteFill>
  );
}
