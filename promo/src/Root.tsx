import { Composition } from "remotion";
import { Promo } from "./Promo";
import { VIDEO } from "./theme";

export function Root() {
  return (
    <Composition
      id="TronPromo"
      component={Promo}
      durationInFrames={VIDEO.durationInFrames}
      fps={VIDEO.fps}
      width={VIDEO.width}
      height={VIDEO.height}
    />
  );
}
