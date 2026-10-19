import type { ReactNode } from "react";
import type { ErrorReport } from "../ipc";
import { CheckIcon, ErrorIcon, RemoveIcon } from "./icons";

/** The latest simulation run, as the panel shows it. */
export type SimRun =
  | { state: "idle" }
  | { state: "running"; progress: number | null; elapsedMs: number }
  | { state: "finished"; elapsedMs: number }
  | { state: "cancelled"; elapsedMs: number }
  | { state: "failed"; error: ErrorReport };

export function formatSeconds(elapsedMs: number): string {
  return (elapsedMs / 1000).toFixed(1);
}

/** Whether fixing the error starts in the simulation settings. */
export function isSettingsError(error: ErrorReport): boolean {
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
  onRun,
  onCancel,
  onOpenSettings,
  onJumpToSymbol,
  onClose,
}: SimPanelProps) {
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
    case "finished":
      content = (
        <p className="sim-panel__state sim-panel__done" role="status">
          <CheckIcon />
          Simulation complete in {formatSeconds(run.elapsedMs)} s.
        </p>
      );
      break;
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
  return (
    <section className="sim-panel" aria-label="Simulation">
      <header className="sim-panel__header">
        <h2 className="sim-panel__title">Simulation</h2>
        {stale && <span className="sim-panel__badge">Out of date</span>}
        <div className="sim-panel__actions">
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
