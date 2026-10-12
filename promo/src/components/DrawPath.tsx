import type { SVGProps } from "react";

type DrawPathProps = SVGProps<SVGPathElement> & {
  d: string;
  progress: number;
};

export function DrawPath({ progress, ...rest }: DrawPathProps) {
  if (progress <= 0) {
    return null;
  }
  return (
    <path
      fill="none"
      strokeLinecap="round"
      strokeLinejoin="round"
      {...rest}
      pathLength={1}
      strokeDasharray={1}
      strokeDashoffset={1 - progress}
    />
  );
}
