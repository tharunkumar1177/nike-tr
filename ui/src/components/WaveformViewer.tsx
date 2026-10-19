import { useEffect, useRef, useState } from "react";
import type { OpAnnotationKind } from "../bindings/OpAnnotationKind";
import type { WaveformPoint } from "../bindings/WaveformPoint";
import { fetchWaveform } from "../ipc";
import { formatOpValue } from "../si";
import { TRACE_TOKENS, drawWaveforms } from "../waveformPlot";
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
  onRemove: (vector: string) => void;
}

type Fetched =
  | { state: "loading" }
  | { state: "ready"; data: Map<string, WaveformPoint[]> }
  | { state: "error" };

function traceName(trace: Trace): string {
  return trace.quantity === "voltage" ? trace.label : `I(${trace.label})`;
}

function summary(trace: Trace, points: WaveformPoint[] | undefined): string {
  const noun = trace.quantity === "voltage" ? "Voltage" : "Current";
  const name = `${noun} ${traceName(trace)}`;
  if (!points || points.length === 0) {
    return `${name}: No data`;
  }
  const values = points.map((point) => point.y);
  const low = formatOpValue(Math.min(...values), trace.quantity);
  const high = formatOpValue(Math.max(...values), trace.quantity);
  return `${name}: minimum ${low}, maximum ${high}`;
}

export function WaveformViewer({
  runId,
  vectors,
  traces,
  stale,
  onRemove,
}: WaveformViewerProps) {
  const plotRef = useRef<HTMLDivElement>(null);
  const canvasRef = useRef<HTMLCanvasElement>(null);
  const [size, setSize] = useState({ width: 0, height: 0 });
  const [fetched, setFetched] = useState<Fetched>({ state: "loading" });
  const shown = traces.filter((trace) => vectors.includes(trace.vector));
  const wanted = shown.map((trace) => trace.vector).join("\n");

  useEffect(() => {
    const plot = plotRef.current;
    if (!plot || typeof ResizeObserver === "undefined") {
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
  }, []);

  useEffect(() => {
    if (runId === null || size.width === 0) {
      return;
    }
    let current = true;
    const names = wanted === "" ? [] : wanted.split("\n");
    setFetched((previous) =>
      previous.state === "ready" ? previous : { state: "loading" },
    );
    const requests = names.map((vector) =>
      fetchWaveform(runId, vector, null, null, size.width),
    );
    Promise.all(requests).then(
      (results) => {
        if (current) {
          const data = new Map<string, WaveformPoint[]>();
          names.forEach((name, index) => data.set(name, results[index]!));
          setFetched({ state: "ready", data });
        }
      },
      () => {
        if (current) {
          setFetched({ state: "error" });
        }
      },
    );
    return () => {
      current = false;
    };
  }, [runId, size.width, wanted]);

  const data = fetched.state === "ready" ? fetched.data : null;

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
    const series = traces.flatMap((trace, index) => {
      const points = data?.get(trace.vector);
      const color = token(TRACE_TOKENS[index % TRACE_TOKENS.length]!);
      return points ? [{ points, quantity: trace.quantity, color }] : [];
    });
    drawWaveforms(
      context,
      size,
      series,
      {
        background: token("--color-canvas"),
        grid: token("--color-grid-line"),
        gridMajor: token("--color-grid-line-major"),
        text: token("--color-text-secondary"),
      },
      `12px ${token("--font-family")}`,
    );
  }, [data, size, traces]);

  if (traces.length === 0) {
    return (
      <p className="sim-panel__state sim-panel__note">
        Simulation complete. Select the Probe tool and click a wire or pin to
        plot its signal.
      </p>
    );
  }

  let message: string | null = null;
  if (fetched.state === "error") {
    message = "Failed to load waveform data.";
  } else if (fetched.state === "loading" && runId !== null && shown.length) {
    message = "Fetching data...";
  }

  return (
    <div className={stale ? "waveform waveform--stale" : "waveform"}>
      <div ref={plotRef} className="waveform__plot">
        <canvas
          ref={canvasRef}
          className="waveform__canvas"
          aria-hidden="true"
        />
        {message !== null && <p className="waveform__message">{message}</p>}
      </div>
      <ul className="waveform__legend" aria-label="Plotted signals">
        {traces.map((trace, index) => {
          const name = traceName(trace);
          const missing = !vectors.includes(trace.vector);
          const color = `var(${TRACE_TOKENS[index % TRACE_TOKENS.length]})`;
          return (
            <li
              key={trace.vector}
              className={
                missing
                  ? "waveform__item waveform__item--missing"
                  : "waveform__item"
              }
            >
              <span
                className="waveform__swatch"
                style={{ color }}
                aria-hidden="true"
              />
              <span>{missing ? `${name} (No data)` : name}</span>
              <button
                type="button"
                className="icon-button"
                aria-label={`Remove ${name} from plot`}
                onClick={() => onRemove(trace.vector)}
              >
                <RemoveIcon />
              </button>
            </li>
          );
        })}
      </ul>
      <ul className="visually-hidden" aria-label="Waveform results">
        {traces.map((trace) => (
          <li key={trace.vector}>{summary(trace, data?.get(trace.vector))}</li>
        ))}
      </ul>
    </div>
  );
}
