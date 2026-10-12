import { NM_PER_MM, type ScreenPoint, formatMm } from "../viewport";

interface StatusBarProps {
  /** Cursor position in document nanometers, or `null` off the canvas. */
  cursor: ScreenPoint | null;
  gridSizeNm: number;
  zoomLabel: string;
}

function coordinate(nm: number | undefined): string {
  return nm === undefined ? "\u2014" : (nm / NM_PER_MM).toFixed(2);
}

export function StatusBar({ cursor, gridSizeNm, zoomLabel }: StatusBarProps) {
  return (
    <footer className="status-bar" role="status" aria-live="off">
      <span className="status-bar__group">
        <span className="status-bar__item">X {coordinate(cursor?.x)} mm</span>
        <span className="status-bar__item">Y {coordinate(cursor?.y)} mm</span>
      </span>
      <span className="status-bar__group">
        <span className="status-bar__item">
          Grid {formatMm(gridSizeNm)} mm
        </span>
        <span className="status-bar__item">Zoom {zoomLabel}</span>
      </span>
    </footer>
  );
}
