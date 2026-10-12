import type { CSSProperties } from "react";
import { SAFE_MARGIN } from "../theme";
import { KineticText } from "./KineticText";

const anchor: CSSProperties = {
  position: "absolute",
  left: SAFE_MARGIN,
  bottom: SAFE_MARGIN,
};

type HeadlineProps = {
  text: string;
  delay: number;
};

export function Headline({ text, delay }: HeadlineProps) {
  return (
    <div style={anchor}>
      <KineticText text={text} delay={delay} variant="title" />
    </div>
  );
}
