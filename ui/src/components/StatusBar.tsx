import { NM_PER_MM, type ScreenPoint, formatMm } from "../viewport";

/** A short interaction hint shown in the status bar. */
export interface StatusHint {
  text: string;
  /** Error hints are drawn in the error color. */
  error: boolean;
}

interface StatusBarProps {
  /** Cursor position in document nanometers, or `null` off the canvas. */
  cursor: ScreenPoint | null;
  gridSizeNm: number;
  zoomLabel: string;
  hint?: StatusHint | null;
}

function coordinate(nm: number | undefined): string {
  return nm === undefined ? "—" : (nm / NM_PER_MM).toFixed(2);
}

export function StatusBar({
  cursor,
  gridSizeNm,
  zoomLabel,
  hint = null,
}: StatusBarProps) {
  return (
    <footer className="status-bar" role="status" aria-live="off">
      <span className="status-bar__group">
        {hint && (
          <span
            className={
              hint.error
                ? "status-bar__hint status-bar__hint--error"
                : "status-bar__hint"
            }
          >
            {hint.text}
          </span>
        )}
        <span className="status-bar__item">X {coordinate(cursor?.x)} mm</span>
        <span className="status-bar__item">Y {coordinate(cursor?.y)} mm</span>
      </span>
      <span className="status-bar__group">
        <span className="status-bar__item">Grid {formatMm(gridSizeNm)} mm</span>
        <span className="status-bar__item">Zoom {zoomLabel}</span>
      </span>
    </footer>
  );
}
