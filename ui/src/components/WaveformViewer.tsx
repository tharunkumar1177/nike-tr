import {
  type KeyboardEvent,
  type PointerEvent,
  useEffect,
  useRef,
  useState,
} from "react";
import type { OpAnnotationKind } from "../bindings/OpAnnotationKind";
import type { TraceReadout } from "../bindings/TraceReadout";
import type { WaveformPoint } from "../bindings/WaveformPoint";
import { fetchWaveform, measureTrace } from "../ipc";
import { formatQuantity } from "../si";
import {
  TRACE_TOKENS,
  drawWaveforms,
  pixelAtTime,
  plotArea,
  timeAtPixel,
  unitOf,
} from "../waveformPlot";
import {
  CURSOR_STEP_FRACTION,
  type Cursors,
  NO_CURSORS,
  PAN_STEP_FRACTION,
  type XRange,
  ZOOM_STEP,
  clampTime,
  measuredSpan,
  panView,
  placedCursors,
  showsWholeRun,
  zoomView,
} from "../waveformView";
import { RemoveIcon } from "./icons";

/** A probed signal. */
export interface Trace {
  vector: string;
  /** The net name of a voltage, or the symbol reference of a current. */
  label: string;
  quantity: OpAnnotationKind;
}

interface WaveformViewerProps {
  /** The run to fetch from, or `null` once its results are out of date. */
  runId: string | null;
  /** Every vector of that run. */
  vectors: string[];
  traces: Trace[];
  stale: boolean;
  /** The times shown, or `null` for the whole run. */
  view: XRange | null;
  onViewChange: (view: XRange | null) => void;
  /** Dragging the plot places cursors instead of panning. */
  measuring: boolean;
  onRemove: (vector: string) => void;
}

/** How long the view must stay still before its samples are fetched again. */
const REFETCH_DELAY_MS = 120;
/** How close the pointer must be to a cursor line to grab it, in CSS pixels. */
const CURSOR_GRAB_PX = 4;
/** How far the pointer must move before a measuring drag places cursor B. */
const SPAN_DRAG_PX = 3;

const CURSOR_TOKENS = { a: "--color-focus", b: "--color-text-warning" };
const CURSOR_NAMES = { a: "A", b: "B" };

type CursorId = "a" | "b";

type Fetched =
  | { state: "loading" }
  | { state: "ready"; data: Map<string, WaveformPoint[]> }
  | { state: "error" };

/** Readouts, with the cursors they were taken at. */
type Readouts =
  | { state: "loading" }
  | { state: "ready"; cursors: Cursors; data: Map<string, TraceReadout> }
  | { state: "error" };

type Drag =
  | { kind: "pan"; pointerId: number; startX: number; startView: XRange }
  | { kind: "cursor"; pointerId: number; cursor: CursorId }
  | { kind: "span"; pointerId: number; startX: number; moved: boolean };

function traceName(trace: Trace): string {
  return trace.quantity === "voltage" ? trace.label : `I(${trace.label})`;
}

/** The times that the samples of every trace span together. */
function runExtentOf(results: WaveformPoint[][]): XRange | null {
  let min = Infinity;
  let max = -Infinity;
  for (const points of results) {
    const first = points[0];
    const last = points[points.length - 1];
    if (first && last) {
      min = Math.min(min, first.x);
      max = Math.max(max, last.x);
    }
  }
  return min < max ? { min, max } : null;
}

function Value({ value, unit }: { value: number | null; unit: string }) {
  if (value === null) {
    return <span className="waveform__empty">—</span>;
  }
  return <>{formatQuantity(value, unit)}</>;
}

export function WaveformViewer({
  runId,
  vectors,
  traces,
  stale,
  view,
  onViewChange,
  measuring,
  onRemove,
}: WaveformViewerProps) {
  const plotRef = useRef<HTMLDivElement>(null);
  const canvasRef = useRef<HTMLCanvasElement>(null);
  const dragRef = useRef<Drag | null>(null);
  const [size, setSize] = useState({ width: 0, height: 0 });
  const [fetched, setFetched] = useState<Fetched>({ state: "loading" });
  const [runExtent, setRunExtent] = useState<XRange | null>(null);
  const [readouts, setReadouts] = useState<Readouts>({ state: "loading" });
  const [cursors, setCursors] = useState<Cursors>(NO_CURSORS);
  const [overCursor, setOverCursor] = useState(false);
  const [announcement, setAnnouncement] = useState("");
  const plotted = traces.length > 0;
  const shown = traces.filter((trace) => vectors.includes(trace.vector));
  const wanted = shown.map((trace) => trace.vector).join("\n");
  const viewMin = view?.min ?? null;
  const viewMax = view?.max ?? null;
  const span = measuredSpan(cursors, view);
  const { a: cursorA, b: cursorB } = cursors;

  useEffect(() => {
    const plot = plotRef.current;
    if (!plotted || !plot || typeof ResizeObserver === "undefined") {
      return;
    }
    const observer = new ResizeObserver(([entry]) => {
      if (entry) {
        const { width, height } = entry.contentRect;
        setSize({ width: Math.floor(width), height: Math.floor(height) });
      }
    });
    observer.observe(plot);
    return () => observer.disconnect();
  }, [plotted]);

  useEffect(() => {
    if (runId === null || size.width === 0) {
      return;
    }
    let current = true;
    const names = wanted === "" ? [] : wanted.split("\n");
    setFetched((previous) =>
      previous.state === "ready" ? previous : { state: "loading" },
    );
    const delay = viewMin === null ? 0 : REFETCH_DELAY_MS;
    const timer = window.setTimeout(() => {
      const requests = names.map((vector) =>
        fetchWaveform(runId, vector, viewMin, viewMax, size.width),
      );
      Promise.all(requests).then(
        (results) => {
          if (current) {
            const data = new Map<string, WaveformPoint[]>();
            names.forEach((name, index) => data.set(name, results[index]!));
            setFetched({ state: "ready", data });
            if (viewMin === null && viewMax === null) {
              setRunExtent(runExtentOf(results));
            }
          }
        },
        () => {
          if (current) {
            setFetched({ state: "error" });
          }
        },
      );
    }, delay);
    return () => {
      current = false;
      window.clearTimeout(timer);
    };
  }, [runId, size.width, wanted, viewMin, viewMax]);

  useEffect(() => {
    if (runId === null) {
      return;
    }
    let current = true;
    const names = wanted === "" ? [] : wanted.split("\n");
    const taken = { a: cursorA, b: cursorB };
    const at = placedCursors(taken);
    const timer = window.setTimeout(() => {
      const requests = names.map((vector) =>
        measureTrace(runId, vector, span.from, span.to, at),
      );
      Promise.all(requests).then(
        (results) => {
          if (current) {
            const data = new Map<string, TraceReadout>();
            names.forEach((name, index) => data.set(name, results[index]!));
            setReadouts({ state: "ready", cursors: taken, data });
          }
        },
        () => {
          if (current) {
            setReadouts({ state: "error" });
          }
        },
      );
    }, REFETCH_DELAY_MS);
    return () => {
      current = false;
      window.clearTimeout(timer);
    };
  }, [runId, wanted, span.from, span.to, cursorA, cursorB]);

  useEffect(() => {
    if (viewMin === null || viewMax === null) {
      return;
    }
    const timer = window.setTimeout(() => {
      const from = formatQuantity(viewMin, "s");
      const to = formatQuantity(viewMax, "s");
      setAnnouncement(`Zoomed to ${from} to ${to}.`);
    }, REFETCH_DELAY_MS);
    return () => window.clearTimeout(timer);
  }, [viewMin, viewMax]);

  const data = fetched.state === "ready" ? fetched.data : null;
  const shownRange = view ?? runExtent;
  const series = traces.flatMap((trace, index) => {
    const points = data?.get(trace.vector);
    const token = TRACE_TOKENS[index % TRACE_TOKENS.length]!;
    return points ? [{ points, quantity: trace.quantity, token }] : [];
  });
  const hasPoints = (quantity: OpAnnotationKind) =>
    series.some((item) => item.quantity === quantity && item.points.length);
  const area = plotArea(size, hasPoints("voltage"), hasPoints("current"));

  useEffect(() => {
    const canvas = canvasRef.current;
    const context = canvas?.getContext("2d");
    if (!canvas || !context || size.width === 0 || size.height === 0) {
      return;
    }
    const ratio = window.devicePixelRatio || 1;
    canvas.width = Math.round(size.width * ratio);
    canvas.height = Math.round(size.height * ratio);
    context.setTransform(ratio, 0, 0, ratio, 0, 0);
    const styles = getComputedStyle(canvas);
    const token = (name: string) => styles.getPropertyValue(name).trim();
    const lines = (["a", "b"] as const).flatMap((id) => {
      const at = id === "a" ? cursorA : cursorB;
      return at === null ? [] : [{ at, color: token(CURSOR_TOKENS[id]) }];
    });
    drawWaveforms(
      context,
      size,
      series.map((item) => ({
        points: item.points,
        quantity: item.quantity,
        color: token(item.token),
      })),
      {
        background: token("--color-canvas"),
        grid: token("--color-grid-line"),
        gridMajor: token("--color-grid-line-major"),
        text: token("--color-text-secondary"),
      },
      `12px ${token("--font-family")}`,
      shownRange ?? { min: 0, max: 0 },
      lines,
    );
  });

  /** Whether the view and cursors respond to input. */
  const interactive = !stale && shownRange !== null;

  const timeAt = (clientX: number): number | null => {
    const plot = plotRef.current;
    if (!plot || !shownRange) {
      return null;
    }
    const x = clientX - plot.getBoundingClientRect().left;
    return clampTime(timeAtPixel(area, shownRange, x), shownRange);
  };

  const cursorNear = (clientX: number): CursorId | null => {
    const plot = plotRef.current;
    if (!plot || !shownRange) {
      return null;
    }
    const x = clientX - plot.getBoundingClientRect().left;
    const near = (at: number | null) =>
      at !== null &&
      Math.abs(pixelAtTime(area, shownRange, at) - x) <= CURSOR_GRAB_PX;
    if (near(cursorB)) {
      return "b";
    }
    return near(cursorA) ? "a" : null;
  };

  const changeView = (next: XRange | null) => {
    if (next === null || !runExtent || showsWholeRun(next, runExtent)) {
      onViewChange(null);
    } else {
      onViewChange(next);
    }
  };

  const zoomAround = (at: number, factor: number) => {
    if (shownRange && runExtent) {
      changeView(zoomView(shownRange, runExtent, at, factor));
    }
  };

  const zoomRef = useRef(zoomAround);
  const timeAtRef = useRef(timeAt);
  const interactiveRef = useRef(interactive);
  useEffect(() => {
    zoomRef.current = zoomAround;
    timeAtRef.current = timeAt;
    interactiveRef.current = interactive;
  });

  useEffect(() => {
    const plot = plotRef.current;
    if (!plotted || !plot) {
      return;
    }
    const onWheel = (event: WheelEvent) => {
      const at = timeAtRef.current(event.clientX);
      if (!interactiveRef.current || at === null || event.deltaY === 0) {
        return;
      }
      event.preventDefault();
      zoomRef.current(at, event.deltaY > 0 ? ZOOM_STEP : 1 / ZOOM_STEP);
    };
    plot.addEventListener("wheel", onWheel, { passive: false });
    return () => plot.removeEventListener("wheel", onWheel);
  }, [plotted]);

  const placeCursor = (cursor: CursorId, at: number, verb: string) => {
    setCursors((previous) => ({ ...previous, [cursor]: at }));
    const time = formatQuantity(at, "s");
    setAnnouncement(`Cursor ${CURSOR_NAMES[cursor]} ${verb} ${time}.`);
  };

  const removeCursor = (cursor: CursorId) => {
    setCursors((previous) => ({ ...previous, [cursor]: null }));
    setAnnouncement(`Cursor ${CURSOR_NAMES[cursor]} removed.`);
  };

  const onPointerDown = (event: PointerEvent<HTMLDivElement>) => {
    const at = timeAt(event.clientX);
    if (event.button !== 0 || !interactive || at === null || !shownRange) {
      return;
    }
    event.preventDefault();
    event.currentTarget.setPointerCapture?.(event.pointerId);
    const { pointerId, clientX } = event;
    const grabbed = cursorNear(clientX);
    if (grabbed) {
      dragRef.current = { kind: "cursor", pointerId, cursor: grabbed };
    } else if (measuring) {
      setCursors({ a: at, b: null });
      dragRef.current = {
        kind: "span",
        pointerId,
        startX: clientX,
        moved: false,
      };
    } else {
      dragRef.current = {
        kind: "pan",
        pointerId,
        startX: clientX,
        startView: shownRange,
      };
    }
  };

  const onPointerMove = (event: PointerEvent<HTMLDivElement>) => {
    const drag = dragRef.current;
    if (!drag || drag.pointerId !== event.pointerId) {
      setOverCursor(interactive && cursorNear(event.clientX) !== null);
      return;
    }
    const at = timeAt(event.clientX);
    if (at === null) {
      return;
    }
    if (drag.kind === "cursor") {
      setCursors((previous) => ({ ...previous, [drag.cursor]: at }));
    } else if (drag.kind === "span") {
      if (drag.moved || Math.abs(event.clientX - drag.startX) > SPAN_DRAG_PX) {
        dragRef.current = { ...drag, moved: true };
        setCursors((previous) => ({ ...previous, b: at }));
      }
    } else if (runExtent) {
      const { startView } = drag;
      const fraction = (event.clientX - drag.startX) / (area.right - area.left);
      const by = -fraction * (startView.max - startView.min);
      changeView(panView(startView, runExtent, by));
    }
  };

  const onPointerUp = (event: PointerEvent<HTMLDivElement>) => {
    const drag = dragRef.current;
    if (!drag || drag.pointerId !== event.pointerId) {
      return;
    }
    dragRef.current = null;
    const at = timeAt(event.clientX);
    if (at === null) {
      return;
    }
    if (drag.kind === "cursor") {
      placeCursor(drag.cursor, at, "moved to");
    } else if (drag.kind === "span") {
      if (drag.moved) {
        placeCursor("b", at, "placed at");
      } else {
        const time = formatQuantity(at, "s");
        setAnnouncement(`Cursor A placed at ${time}.`);
      }
    }
  };

  const onDoubleClick = (event: { clientX: number }) => {
    if (!interactive) {
      return;
    }
    const grabbed = cursorNear(event.clientX);
    if (grabbed) {
      removeCursor(grabbed);
    } else {
      onViewChange(null);
    }
  };

  const onKeyDown = (event: KeyboardEvent<HTMLDivElement>) => {
    if (!interactive || !shownRange || event.ctrlKey || event.metaKey) {
      return;
    }
    const width = shownRange.max - shownRange.min;
    const centre = (shownRange.min + shownRange.max) / 2;
    const step = event.key === "ArrowLeft" ? -1 : 1;
    const arrow = event.key === "ArrowLeft" || event.key === "ArrowRight";
    const moved = event.shiftKey ? "a" : event.altKey ? "b" : null;
    const key = event.key.toLowerCase();
    const anyCursor = cursorA !== null || cursorB !== null;
    if (arrow && moved) {
      const at = moved === "a" ? cursorA : cursorB;
      if (at === null) {
        return;
      }
      const next = at + step * width * CURSOR_STEP_FRACTION;
      placeCursor(moved, clampTime(next, shownRange), "moved to");
    } else if (arrow && runExtent) {
      const by = step * width * PAN_STEP_FRACTION;
      changeView(panView(shownRange, runExtent, by));
    } else if (event.key === "+" || event.key === "=") {
      zoomAround(centre, 1 / ZOOM_STEP);
    } else if (event.key === "-") {
      zoomAround(centre, ZOOM_STEP);
    } else if (event.key === "Home") {
      onViewChange(null);
    } else if ((key === "a" || key === "b") && !event.altKey) {
      placeCursor(key, centre, "placed at");
    } else if (event.key === "Escape" && anyCursor) {
      setCursors(NO_CURSORS);
      setAnnouncement("Cursors cleared.");
    } else {
      return;
    }
    event.preventDefault();
    event.stopPropagation();
  };

  if (!plotted) {
    return (
      <p className="sim-panel__state sim-panel__note">
        Simulation complete. Select the Probe tool and click a wire or pin to
        plot its signal.
      </p>
    );
  }

  let message: string | null = null;
  if (fetched.state === "error" || readouts.state === "error") {
    message = "Failed to load waveform data.";
  } else if (fetched.state === "loading" && runId !== null && shown.length) {
    message = "Fetching data...";
  }

  const taken = readouts.state === "ready" ? readouts : null;
  const bothCursors = cursorA !== null && cursorB !== null;
  const cursorHeader = (id: CursorId) => {
    const at = id === "a" ? cursorA : cursorB;
    const name = `Cursor ${CURSOR_NAMES[id]}`;
    return at === null ? name : `${name} (${formatQuantity(at, "s")})`;
  };
  let deltaHeader = "Δ";
  if (bothCursors) {
    const delta = cursorB - cursorA;
    const time = formatQuantity(delta, "s");
    const frequency = formatQuantity(1 / Math.abs(delta), "Hz");
    deltaHeader = delta === 0 ? `Δ (${time})` : `Δ (${time}, ${frequency})`;
  }
  const takenAt = taken?.cursors ?? NO_CURSORS;
  /** The reading at cursor `id`, while the readouts match where it sits. */
  const cursorValue = (readout: TraceReadout | null, id: CursorId) => {
    const at = id === "a" ? cursorA : cursorB;
    if (!readout || at === null || takenAt[id] === null) {
      return null;
    }
    const index = id === "b" && takenAt.a !== null ? 1 : 0;
    return readout.cursors[index] ?? null;
  };

  const plotClasses = ["waveform__plot"];
  if (interactive && overCursor) {
    plotClasses.push("waveform__plot--on-cursor");
  } else if (interactive && measuring) {
    plotClasses.push("waveform__plot--measuring");
  }

  return (
    <div className={stale ? "waveform waveform--stale" : "waveform"}>
      <div
        ref={plotRef}
        className={plotClasses.join(" ")}
        tabIndex={0}
        aria-label="Waveform plot"
        aria-keyshortcuts="+ - ArrowLeft ArrowRight Home A B Shift+ArrowLeft Shift+ArrowRight Alt+ArrowLeft Alt+ArrowRight Escape"
        data-min-x={shownRange?.min}
        data-max-x={shownRange?.max}
        data-cursor-a={cursorA ?? undefined}
        data-cursor-b={cursorB ?? undefined}
        onPointerDown={onPointerDown}
        onPointerMove={onPointerMove}
        onPointerUp={onPointerUp}
        onPointerCancel={() => {
          dragRef.current = null;
        }}
        onPointerLeave={() => setOverCursor(false)}
        onDoubleClick={onDoubleClick}
        onKeyDown={onKeyDown}
      >
        <canvas
          ref={canvasRef}
          className="waveform__canvas"
          aria-hidden="true"
        />
      </div>
      <div className="waveform__table-container">
        <table className="waveform__table">
          <caption className="waveform__caption">
            {span.betweenCursors
              ? "Measurements cover the span between cursors."
              : "Measurements cover the visible range."}
          </caption>
          <thead>
            <tr>
              <th scope="col">Signal</th>
              <th scope="col">{cursorHeader("a")}</th>
              <th scope="col">{cursorHeader("b")}</th>
              <th scope="col">{deltaHeader}</th>
              <th scope="col">Min</th>
              <th scope="col">Max</th>
              <th scope="col">Avg</th>
              <th scope="col">RMS</th>
              <th scope="col">Pk-Pk</th>
              <th scope="col">Remove</th>
            </tr>
          </thead>
          <tbody>
            {message !== null && (
              <tr>
                <td className="waveform__table-message" colSpan={10}>
                  {message}
                </td>
              </tr>
            )}
            {traces.map((trace, index) => {
              const name = traceName(trace);
              const missing = !vectors.includes(trace.vector);
              const token = TRACE_TOKENS[index % TRACE_TOKENS.length];
              const unit = unitOf(trace.quantity);
              const readout = taken?.data.get(trace.vector) ?? null;
              const measured = readout?.span ?? null;
              const valueA = cursorValue(readout, "a");
              const valueB = cursorValue(readout, "b");
              const delta =
                valueA !== null && valueB !== null ? valueB - valueA : null;
              return (
                <tr
                  key={trace.vector}
                  className={
                    missing
                      ? "waveform__row waveform__row--missing"
                      : "waveform__row"
                  }
                >
                  <th scope="row">
                    <span className="waveform__signal">
                      <span
                        className="waveform__swatch"
                        style={{ color: `var(${token})` }}
                        aria-hidden="true"
                      />
                      {missing ? `${name} (No data)` : name}
                    </span>
                  </th>
                  <td>
                    <Value value={valueA} unit={unit} />
                  </td>
                  <td>
                    <Value value={valueB} unit={unit} />
                  </td>
                  <td>
                    <Value value={delta} unit={unit} />
                  </td>
                  <td>
                    <Value value={measured?.min ?? null} unit={unit} />
                  </td>
                  <td>
                    <Value value={measured?.max ?? null} unit={unit} />
                  </td>
                  <td>
                    <Value value={measured?.average ?? null} unit={unit} />
                  </td>
                  <td>
                    <Value value={measured?.rms ?? null} unit={unit} />
                  </td>
                  <td>
                    <Value value={measured?.peakToPeak ?? null} unit={unit} />
                  </td>
                  <td>
                    <button
                      type="button"
                      className="icon-button"
                      aria-label={`Remove ${name} from plot`}
                      onClick={() => onRemove(trace.vector)}
                    >
                      <RemoveIcon />
                    </button>
                  </td>
                </tr>
              );
            })}
          </tbody>
        </table>
      </div>
      <div className="visually-hidden" aria-live="polite">
        {announcement}
      </div>
    </div>
  );
}
