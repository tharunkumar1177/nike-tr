import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import type { ErrorReport } from "../ipc";
import { SimPanel, type SimRun } from "./SimPanel";

afterEach(cleanup);

function renderPanel(run: SimRun, stale = false) {
  const handlers = {
    onRun: vi.fn(),
    onCancel: vi.fn(),
    onOpenSettings: vi.fn(),
    onJumpToSymbol: vi.fn(),
    onClose: vi.fn(),
  };
  render(<SimPanel run={run} stale={stale} announcement="" {...handlers} />);
  return handlers;
}

function failed(error: Partial<ErrorReport>): SimRun {
  return {
    state: "failed",
    error: { kind: null, message: "Failed.", detail: null, ...error },
  };
}

describe("SimPanel", () => {
  it("offers a first run", () => {
    const { onRun } = renderPanel({ state: "idle" });
    expect(
      screen.getByText("Run a simulation to see the circuit's results."),
    ).toBeTruthy();
    fireEvent.click(screen.getByRole("button", { name: "Run Simulation" }));
    expect(onRun).toHaveBeenCalled();
  });

  it("shows progress and elapsed time while running, with Cancel", () => {
    const { onCancel } = renderPanel({
      state: "running",
      progress: 42,
      elapsedMs: 1250,
    });
    const progress = screen.getByRole<HTMLProgressElement>("progressbar", {
      name: "Simulation progress",
    });
    expect(progress.value).toBe(42);
    expect(screen.getByText("Elapsed: 1.3 s")).toBeTruthy();
    expect(screen.queryByRole("button", { name: "Run Simulation" })).toBeNull();
    fireEvent.click(screen.getByRole("button", { name: "Cancel" }));
    expect(onCancel).toHaveBeenCalled();
  });

  it("leaves the progress bar indeterminate until a percentage arrives", () => {
    renderPanel({ state: "running", progress: null, elapsedMs: 0 });
    expect(screen.getByRole("progressbar").hasAttribute("value")).toBe(false);
  });

  it("reports a finished run and marks it out of date after an edit", () => {
    renderPanel({ state: "finished", elapsedMs: 2000 }, true);
    expect(screen.getByText("Simulation complete in 2.0 s.")).toBeTruthy();
    expect(screen.getByText("Out of date")).toBeTruthy();
    expect(
      screen.getByRole("button", { name: "Run Simulation" }).className,
    ).toBe("button button--primary");
  });

  it("reports a cancelled run", () => {
    renderPanel({ state: "cancelled", elapsedMs: 500 });
    expect(screen.getByText("Simulation cancelled.")).toBeTruthy();
  });

  it("opens the settings from an analysis error", () => {
    const { onOpenSettings } = renderPanel(
      failed({ kind: "simInvalidAnalysis", message: "Bad step." }),
    );
    fireEvent.click(screen.getByRole("button", { name: "Bad step." }));
    expect(onOpenSettings).toHaveBeenCalled();
  });

  it("jumps to the symbol an error names", () => {
    const { onJumpToSymbol } = renderPanel(
      failed({ kind: "simNoModel", message: "No model.", detail: "D1" }),
    );
    fireEvent.click(screen.getByRole("button", { name: /^No model\./ }));
    expect(onJumpToSymbol).toHaveBeenCalledWith("D1");
  });

  it("shows other errors as an alert without an action", () => {
    renderPanel(
      failed({
        kind: "simCircuitRejected",
        message: "ngspice rejected the circuit.",
        detail: "unknown model",
      }),
    );
    expect(screen.getByRole("alert").textContent).toBe(
      "ngspice rejected the circuit.unknown model",
    );
  });
});
