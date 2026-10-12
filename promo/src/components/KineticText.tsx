import { useCurrentFrame } from "remotion";
import { colors, ramp, textStyles, type TextVariant } from "../theme";

type KineticTextProps = {
  text: string;
  delay: number;
  variant: TextVariant;
  color?: string;
  distance?: number;
};

export function KineticText({
  text,
  delay,
  variant,
  color = colors.textPrimary,
  distance = 50,
}: KineticTextProps) {
  const frame = useCurrentFrame();
  const progress = ramp(frame, delay, delay + 30);
  return (
    <div
      style={{
        ...textStyles[variant],
        color,
        opacity: progress,
        transform: `translateY(${(1 - progress) * distance}px)`,
        whiteSpace: "pre",
      }}
    >
      {text}
    </div>
  );
}
