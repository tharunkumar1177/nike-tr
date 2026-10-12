import { loadFont as loadInter } from "@remotion/google-fonts/Inter";
import { loadFont as loadMono } from "@remotion/google-fonts/JetBrainsMono";
import type { CSSProperties } from "react";
import { Easing, interpolate } from "remotion";

export const VIDEO = {
  width: 1920,
  height: 1080,
  fps: 30,
  durationInFrames: 900,
} as const;

export const SCENE_FRAMES = 180;
export const SAFE_MARGIN = 120;

export const colors = {
  background: "#121212",
  surface: "#1e1e1e",
  textPrimary: "#e0e0e0",
  textSecondary: "#a0a0a0",
  error: "#ff8a8a",
  accent: "#64b5f6",
} as const;

const inter = loadInter("normal", {
  weights: ["500", "700"],
  subsets: ["latin"],
});

const jetBrainsMono = loadMono("normal", {
  weights: ["400"],
  subsets: ["latin"],
});

export const fonts = {
  sans: inter.fontFamily,
  mono: jetBrainsMono.fontFamily,
} as const;

export type TextVariant = "title" | "subtitle" | "code";

export const textStyles: Record<TextVariant, CSSProperties> = {
  title: {
    fontFamily: fonts.sans,
    fontSize: 120,
    fontWeight: 700,
    letterSpacing: "-0.02em",
    lineHeight: 1.1,
  },
  subtitle: {
    fontFamily: fonts.sans,
    fontSize: 64,
    fontWeight: 500,
    lineHeight: 1.2,
  },
  code: {
    fontFamily: fonts.mono,
    fontSize: 32,
    fontWeight: 400,
    lineHeight: 1.6,
  },
};

export const easeStandard = Easing.bezier(0.25, 0.1, 0.25, 1);

export const snappySpring = { stiffness: 120, damping: 14, mass: 1 } as const;

export function ramp(frame: number, start: number, end: number): number {
  return interpolate(frame, [start, end], [0, 1], {
    easing: easeStandard,
    extrapolateLeft: "clamp",
    extrapolateRight: "clamp",
  });
}

export function stagger(
  progress: number,
  index: number,
  count: number,
  overlap = 0.5,
): number {
  const width = 1 / (count - (count - 1) * overlap);
  const start = index * width * (1 - overlap);
  return Math.min(1, Math.max(0, (progress - start) / width));
}
