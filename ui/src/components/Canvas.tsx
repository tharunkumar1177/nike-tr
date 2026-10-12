import {
  type PointerEvent,
  useEffect,
  useLayoutEffect,
  useRef,
  useState,
} from "react";
import {
  type ScreenPoint,
  type SheetDimensions,
  type Size,
  type Viewport,
  panBy,
  screenToWorld,
  visibleGridStep,
  worldToScreen,
  zoomAt,
} from "../viewport";

/** Wheel distance, in pixels, that zooms by a factor of e. */
const WHEEL_PIXELS_PER_E = 650;
const LINE_HEIGHT_PX = 16;
const MAJOR_LINE_EVERY = 10;

type ViewportUpdate = (update: (viewport: Viewport) => Viewport) => void;

interface Drag {
  pointerId: number;
  x: number;
  y: number;
}

interface CanvasProps {
  label: string;
  viewport: Viewport | null;
  gridSizeNm: number;
  sheet: SheetDimensions;
  onViewportChange: ViewportUpdate;
  onResize: (size: Size) => void;
  onCursorMove: (position: ScreenPoint | null) => void;
}

function isTextEntry(target: EventTarget | null): boolean {
  if (!(target instanceof HTMLElement)) {
    return false;
  }
  return (
    target.isContentEditable ||
    ["INPUT", "TEXTAREA", "SELECT", "BUTTON"].includes(target.tagName)
  );
}

function drawSheet(
  canvas: HTMLCanvasElement,
  size: Size,
  viewport: Viewport,
  gridSizeNm: number,
  sheet: SheetDimensions,
) {
  const context = canvas.getContext("2d");
  if (!context) {
    return;
  }
  const ratio = window.devicePixelRatio || 1;
  const pixelWidth = Math.round(size.width * ratio);
  const pixelHeight = Math.round(size.height * ratio);
  if (canvas.width !== pixelWidth || canvas.height !== pixelHeight) {
    canvas.width = pixelWidth;
    canvas.height = pixelHeight;
  }
  context.setTransform(ratio, 0, 0, ratio, 0, 0);

  const styles = getComputedStyle(canvas);
  const color = (name: string) => styles.getPropertyValue(name).trim();

  context.fillStyle = color("--color-canvas");
  context.fillRect(0, 0, size.width, size.height);

  const topLeft = worldToScreen(viewport, 0, 0);
  const bottomRight = worldToScreen(viewport, sheet.widthNm, sheet.heightNm);
  context.fillStyle = color("--color-sheet");
  context.fillRect(
    topLeft.x,
    topLeft.y,
    bottomRight.x - topLeft.x,
    bottomRight.y - topLeft.y,
  );

  const step = visibleGridStep(gridSizeNm, viewport.zoom);
  const visibleMin = screenToWorld(viewport, 0, 0);
  const visibleMax = screenToWorld(viewport, size.width, size.height);
  const minX = Math.max(0, visibleMin.x);
  const maxX = Math.min(sheet.widthNm, visibleMax.x);
  const minY = Math.max(0, visibleMin.y);
  const maxY = Math.min(sheet.heightNm, visibleMax.y);
  const top = Math.max(0, topLeft.y);
  const bottom = Math.min(size.height, bottomRight.y);
  const left = Math.max(0, topLeft.x);
  const right = Math.min(size.width, bottomRight.x);

  const minor = new Path2D();
  const major = new Path2D();
  for (let i = Math.ceil(minX / step); i * step <= maxX; i++) {
    const x = Math.round(worldToScreen(viewport, i * step, 0).x) + 0.5;
    const path = i % MAJOR_LINE_EVERY === 0 ? major : minor;
    path.moveTo(x, top);
    path.lineTo(x, bottom);
  }
  for (let j = Math.ceil(minY / step); j * step <= maxY; j++) {
    const y = Math.round(worldToScreen(viewport, 0, j * step).y) + 0.5;
    const path = j % MAJOR_LINE_EVERY === 0 ? major : minor;
    path.moveTo(left, y);
    path.lineTo(right, y);
  }
  context.lineWidth = 1;
  context.strokeStyle = color("--color-grid-line");
  context.stroke(minor);
  context.strokeStyle = color("--color-grid-line-major");
  context.stroke(major);

  context.strokeStyle = color("--color-sheet-border");
  context.strokeRect(
    Math.round(topLeft.x) + 0.5,
    Math.round(topLeft.y) + 0.5,
    Math.round(bottomRight.x - topLeft.x),
    Math.round(bottomRight.y - topLeft.y),
  );
}

export function Canvas({
  label,
  viewport,
  gridSizeNm,
  sheet,
  onViewportChange,
  onResize,
  onCursorMove,
}: CanvasProps) {
  const containerRef = useRef<HTMLDivElement>(null);
  const canvasRef = useRef<HTMLCanvasElement>(null);
  const dragRef = useRef<Drag | null>(null);
  const callbacksRef = useRef({ onViewportChange, onResize });
  const [size, setSize] = useState<Size>({ width: 0, height: 0 });
  const [spaceHeld, setSpaceHeld] = useState(false);
  const [panning, setPanning] = useState(false);

  useLayoutEffect(() => {
    callbacksRef.current = { onViewportChange, onResize };
  });

  useEffect(() => {
    const container = containerRef.current;
    if (!container || typeof ResizeObserver === "undefined") {
      return;
    }
    const observer = new ResizeObserver(([entry]) => {
      if (!entry) {
        return;
      }
      const next = {
        width: entry.contentRect.width,
        height: entry.contentRect.height,
      };
      setSize(next);
      callbacksRef.current.onResize(next);
    });
    observer.observe(container);
    return () => observer.disconnect();
  }, []);

  useEffect(() => {
    const container = containerRef.current;
    if (!container) {
      return;
    }
    const onWheel = (event: WheelEvent) => {
      event.preventDefault();
      const rect = container.getBoundingClientRect();
      const pixels =
        event.deltaMode === 1 ? event.deltaY * LINE_HEIGHT_PX : event.deltaY;
      const factor = Math.exp(-pixels / WHEEL_PIXELS_PER_E);
      const x = event.clientX - rect.left;
      const y = event.clientY - rect.top;
      callbacksRef.current.onViewportChange((current) =>
        zoomAt(current, factor, x, y),
      );
    };
    container.addEventListener("wheel", onWheel, { passive: false });
    return () => container.removeEventListener("wheel", onWheel);
  }, []);

  useEffect(() => {
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.code === "Space" && !isTextEntry(event.target)) {
        event.preventDefault();
        setSpaceHeld(true);
      }
    };
    const onKeyUp = (event: KeyboardEvent) => {
      if (event.code === "Space") {
        setSpaceHeld(false);
      }
    };
    const onBlur = () => setSpaceHeld(false);
    window.addEventListener("keydown", onKeyDown);
    window.addEventListener("keyup", onKeyUp);
    window.addEventListener("blur", onBlur);
    return () => {
      window.removeEventListener("keydown", onKeyDown);
      window.removeEventListener("keyup", onKeyUp);
      window.removeEventListener("blur", onBlur);
    };
  }, []);

  useEffect(() => {
    const canvas = canvasRef.current;
    if (!canvas || !viewport || size.width === 0 || size.height === 0) {
      return;
    }
    drawSheet(canvas, size, viewport, gridSizeNm, sheet);
  }, [viewport, size, gridSizeNm, sheet]);

  const onPointerDown = (event: PointerEvent<HTMLDivElement>) => {
    if (event.button === 1 || (event.button === 0 && spaceHeld)) {
      event.preventDefault();
      event.currentTarget.setPointerCapture(event.pointerId);
      dragRef.current = {
        pointerId: event.pointerId,
        x: event.clientX,
        y: event.clientY,
      };
      setPanning(true);
    }
  };

  const onPointerMove = (event: PointerEvent<HTMLDivElement>) => {
    const drag = dragRef.current;
    if (drag && drag.pointerId === event.pointerId) {
      const dx = event.clientX - drag.x;
      const dy = event.clientY - drag.y;
      dragRef.current = { ...drag, x: event.clientX, y: event.clientY };
      onViewportChange((current) => panBy(current, dx, dy));
    }
    if (viewport) {
      const rect = event.currentTarget.getBoundingClientRect();
      const x = event.clientX - rect.left;
      const y = event.clientY - rect.top;
      onCursorMove(screenToWorld(viewport, x, y));
    }
  };

  const endPan = (event: PointerEvent<HTMLDivElement>) => {
    if (dragRef.current?.pointerId === event.pointerId) {
      dragRef.current = null;
      setPanning(false);
    }
  };

  const cursorClass = panning
    ? "canvas--panning"
    : spaceHeld
      ? "canvas--pan-ready"
      : "";

  return (
    <div
      ref={containerRef}
      className={`canvas ${cursorClass}`}
      onPointerDown={onPointerDown}
      onPointerMove={onPointerMove}
      onPointerUp={endPan}
      onPointerCancel={endPan}
      onPointerLeave={() => onCursorMove(null)}
      onMouseDown={(event) => {
        if (event.button === 1) {
          event.preventDefault();
        }
      }}
    >
      <canvas
        ref={canvasRef}
        className="canvas__surface"
        role="img"
        aria-label={label}
      />
    </div>
  );
}
