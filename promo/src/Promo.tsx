import { AbsoluteFill, Html5Audio, Sequence, staticFile } from "remotion";
import { AiScene } from "./scenes/AiScene";
import { LayoutScene } from "./scenes/LayoutScene";
import { OutroScene } from "./scenes/OutroScene";
import { SchematicScene } from "./scenes/SchematicScene";
import { SimulationScene } from "./scenes/SimulationScene";
import { colors, SCENE_FRAMES } from "./theme";

export function Promo() {
  return (
    <AbsoluteFill style={{ backgroundColor: colors.background }}>
      <Html5Audio src={staticFile("soundtrack.wav")} />
      <Sequence name="Schematic" from={0} durationInFrames={SCENE_FRAMES}>
        <SchematicScene />
      </Sequence>
      <Sequence
        name="Simulation"
        from={SCENE_FRAMES}
        durationInFrames={SCENE_FRAMES}
      >
        <SimulationScene />
      </Sequence>
      <Sequence
        name="Layout"
        from={SCENE_FRAMES * 2}
        durationInFrames={SCENE_FRAMES}
      >
        <LayoutScene />
      </Sequence>
      <Sequence
        name="AI"
        from={SCENE_FRAMES * 3}
        durationInFrames={SCENE_FRAMES}
      >
        <AiScene />
      </Sequence>
      <Sequence
        name="Outro"
        from={SCENE_FRAMES * 4}
        durationInFrames={SCENE_FRAMES}
      >
        <OutroScene />
      </Sequence>
    </AbsoluteFill>
  );
}
