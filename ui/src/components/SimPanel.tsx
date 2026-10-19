import {
  type KeyboardEvent,
  type PointerEvent,
  type ReactNode,
  useRef,
  useState,
} from "react";
import type { SimulationFinished } from "../bindings/SimulationFinished";
import type { ErrorReport } from "../ipc";
import { formatSeconds } from "../si";
import type { XRange } from "../waveformView";
import { CheckIcon, ErrorIcon, RemoveIcon } from "./icons";
import { type Trace, WaveformViewer } from "./WaveformViewer";

/** The panel's height limits, in CSS pixels, and its keyboard resize step. */
const MIN_HEIGHT_PX = 240;
const PLOTTING_HEIGHT_PX = 320;
const MAX_HEIGHT_FRACTION = 0.8;
const RESIZE_STEP_PX = 20;

/** The view zoomed into, for the run it belongs to. */
interface Zoom {
  runId: string;
  view: XRange;
}

/** Where a resize drag started. */
interface ResizeDrag {
  pointerId: number;
  y: number;
  height: number;
}

function maxHeight(): number {
  return Math.max(
    MIN_HEIGHT_PX,
    Math.round(window.innerHeight * MAX_HEIGHT_FRACTION),
  );
}

/** The latest simulation run, as the panel shows it. */
export type SimRun =
  | { state: "idle" }
  | { state: "running"; progress: number | null; elapsedMs: number }
  | { state: "finished"; elapsedMs: number; result: SimulationFinished }
  | { state: "cancelled"; elapsedMs: number }
  | { state: "failed"; error: ErrorReport };

/** Whether fixing the error starts in the simulation settings. */
function isSettingsError(error: ErrorReport): boolean {
  return error.kind === "simNoAnalysis" || error.kind === "simInvalidAnalysis";
}

/** The reference of the symbol the error is about, if it names one. */
function symbolReference(error: ErrorReport): string | null {
  switch (error.kind) {
    case "simNoModel":
    case "simUnannotated":
    case "simDuplicateReference":
      return error.detail;
    default:
      return null;
  }
}

interface SimPanelProps {
  run: SimRun;
  /** The schematic changed since the last run. */
  stale: boolean;
  /** Read by screen readers when a run starts or ends. */
  announcement: string;
  /** The probed signals, in the order they were added. */
  traces: Trace[];
  onRemoveTrace: (vector: string) => void;
  onRun: () => void;
  onCancel: () => void;
  onOpenSettings: () => void;
  /** Shows the symbol with this reference on the canvas. */
  onJumpToSymbol: (reference: string) => void;
  onClose: () => void;
}

export function SimPanel({
  run,
  stale,
  announcement,
  traces,
  onRemoveTrace,
  onRun,
  onCancel,
  onOpenSettings,
  onJumpToSymbol,
  onClose,
}: SimPanelProps) {
  const [zoom, setZoom] = useState<Zoom | null>(null);
  const [measuring, setMeasuring] = useState(false);
  const [height, setHeight] = useState<number | null>(null);
  const resizeRef = useRef<ResizeDrag>(null);
  const runId = run.state === "finished" ? run.result.runId : null;
  const view = zoom !== null && zoom.runId === runId ? zoom.view : null;
  const changeView = (next: XRange | null) => {
    setZoom(next === null || runId === null ? null : { runId, view: next });
  };

  let content: ReactNode;
  switch (run.state) {
    case "idle":
      content = (
        <div className="sim-panel__state">
          <p className="sim-panel__note">
            Run a simulation to see the circuit&apos;s results.
          </p>
          <button
            type="button"
            className="button button--primary"
            onClick={onRun}
          >
            Run Simulation
          </button>
        </div>
      );
      break;
    case "running":
      content = (
        <div className="sim-panel__state" role="status">
          <progress
            className="sim-panel__progress"
            aria-label="Simulation progress"
            max={100}
            value={run.progress ?? undefined}
          />
          <span>Elapsed: {formatSeconds(run.elapsedMs)} s</span>
        </div>
      );
      break;
    case "finished": {
      const { result } = run;
      content = result.scale ? (
        <WaveformViewer
          key={result.runId}
          runId={stale ? null : result.runId}
          vectors={result.vectors}
          traces={traces}
          stale={stale}
          xScale={result.analysisKind === "ac" ? "log" : "linear"}
          view={view}
          onViewChange={changeView}
          measuring={measuring}
          onRemove={onRemoveTrace}
        />
      ) : (
        <p className="sim-panel__state sim-panel__note">
          Waveforms are not available for Operating Point (.op) analysis. Read
          the values directly from the schematic annotations.
        </p>
      );
      break;
    }
    case "cancelled":
      content = (
        <p className="sim-panel__state" role="status">
          Simulation cancelled.
        </p>
      );
      break;
    case "failed": {
      const { error } = run;
      const detail = error.detail;
      const text = (
        <>
          <span className="sim-row__icon">
            <ErrorIcon />
          </span>
          <span className="sim-row__text">
            <span className="sim-row__message">{error.message}</span>
            {detail !== null && (
              <span className="sim-row__detail">{detail}</span>
            )}
          </span>
        </>
      );
      const reference = symbolReference(error);
      let onClick: (() => void) | null = null;
      if (isSettingsError(error)) {
        onClick = onOpenSettings;
      } else if (reference !== null) {
        onClick = () => onJumpToSymbol(reference);
      }
      content = onClick ? (
        <button type="button" className="sim-row" onClick={onClick}>
          {text}
        </button>
      ) : (
        <div className="sim-row" role="alert">
          {text}
        </div>
      );
      break;
    }
  }

  const running = run.state === "running";
  const plotting =
    run.state === "finished" && run.result.scale !== null && traces.length > 0;
  const shownHeight = height ?? (plotting ? PLOTTING_HEIGHT_PX : MIN_HEIGHT_PX);

  const resizeTo = (next: number) => {
    setHeight(Math.min(Math.max(next, MIN_HEIGHT_PX), maxHeight()));
  };

  const onResizeStart = (event: PointerEvent<HTMLDivElement>) => {
    if (event.button !== 0) {
      return;
    }
    event.preventDefault();
    event.currentTarget.setPointerCapture?.(event.pointerId);
    resizeRef.current = {
      pointerId: event.pointerId,
      y: event.clientY,
      height: shownHeight,
    };
  };

  const onResizeMove = (event: PointerEvent<HTMLDivElement>) => {
    const start = resizeRef.current;
    if (start && start.pointerId === event.pointerId) {
      resizeTo(start.height + start.y - event.clientY);
    }
  };

  const onResizeEnd = () => {
    resizeRef.current = null;
  };

  const onResizeKey = (event: KeyboardEvent<HTMLDivElement>) => {
    const sizes: Record<string, number> = {
      ArrowUp: shownHeight + RESIZE_STEP_PX,
      ArrowDown: shownHeight - RESIZE_STEP_PX,
      Home: MIN_HEIGHT_PX,
      End: maxHeight(),
    };
    const next = sizes[event.key];
    if (next !== undefined) {
      event.preventDefault();
      resizeTo(next);
    }
  };

  return (
    <section
      className={plotting ? "sim-panel sim-panel--plotting" : "sim-panel"}
      style={height === null ? undefined : { height }}
      aria-label="Simulation"
    >
      <div
        className="sim-panel__resize"
        role="separator"
        tabIndex={0}
        aria-label="Resize simulation panel"
        aria-orientation="horizontal"
        aria-valuenow={shownHeight}
        aria-valuemin={MIN_HEIGHT_PX}
        aria-valuemax={maxHeight()}
        onPointerDown={onResizeStart}
        onPointerMove={onResizeMove}
        onPointerUp={onResizeEnd}
        onPointerCancel={onResizeEnd}
        onKeyDown={onResizeKey}
      />
      <header className="sim-panel__header">
        {run.state === "finished" ? (
          <p className="sim-panel__status" role="status">
            <CheckIcon />
            Simulation complete in {formatSeconds(run.elapsedMs)} s.
          </p>
        ) : (
          <h2 className="sim-panel__title">Simulation</h2>
        )}
        {stale && <span className="sim-panel__badge">Out of date</span>}
        <div className="sim-panel__actions">
          {plotting && (
            <>
              <button
                type="button"
                className="button"
                aria-pressed={measuring}
                disabled={stale}
                onClick={() => setMeasuring((previous) => !previous)}
              >
                Measure
              </button>
              <button
                type="button"
                className="button"
                disabled={stale || view === null}
                onClick={() => changeView(null)}
              >
                Zoom to Fit
              </button>
            </>
          )}
          <button type="button" className="button" onClick={onOpenSettings}>
            Settings
          </button>
          {running ? (
            <button type="button" className="button" onClick={onCancel}>
              Cancel
            </button>
          ) : (
            run.state !== "idle" && (
              <button
                type="button"
                className={stale ? "button button--primary" : "button"}
                onClick={onRun}
              >
                Run Simulation
              </button>
            )
          )}
          <button
            type="button"
            className="icon-button"
            aria-label="Close Simulation"
            onClick={onClose}
          >
            <RemoveIcon />
          </button>
        </div>
      </header>
      <div className="sim-panel__content">{content}</div>
      <div className="visually-hidden" aria-live="polite">
        {announcement}
      </div>
    </section>
  );
}
