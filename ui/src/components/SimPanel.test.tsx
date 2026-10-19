import {
  cleanup,
  fireEvent,
  render,
  screen,
  within,
} from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { type ErrorReport, fetchWaveform } from "../ipc";
import { SimPanel, type SimRun } from "./SimPanel";
import type { Trace } from "./WaveformViewer";

vi.mock("../ipc", async (importOriginal) => ({
  ...(await importOriginal<typeof import("../ipc")>()),
  fetchWaveform: vi.fn(),
}));

const PLOT_WIDTH = 400;

class FixedSizeObserver {
  constructor(private readonly callback: ResizeObserverCallback) {}
  observe() {
    const entry = { contentRect: { width: PLOT_WIDTH, height: 200 } };
    this.callback([entry as ResizeObserverEntry], this as ResizeObserver);
  }
  disconnect() {}
  unobserve() {}
}

beforeEach(() => {
  vi.stubGlobal("ResizeObserver", FixedSizeObserver);
  vi.mocked(fetchWaveform).mockReset();
});

afterEach(() => {
  cleanup();
  vi.unstubAllGlobals();
});

function renderPanel(run: SimRun, stale = false, traces: Trace[] = []) {
  const handlers = {
    onRun: vi.fn(),
    onCancel: vi.fn(),
    onOpenSettings: vi.fn(),
    onJumpToSymbol: vi.fn(),
    onClose: vi.fn(),
    onRemoveTrace: vi.fn(),
  };
  render(
    <SimPanel
      run={run}
      stale={stale}
      announcement=""
      traces={traces}
      {...handlers}
    />,
  );
  return handlers;
}

function finished(scale: string | null, vectors: string[] = []): SimRun {
  return {
    state: "finished",
    elapsedMs: 2000,
    result: { runId: "run-1", scale, vectors },
  };
}

const OUT: Trace = { vector: "/out", label: "/OUT", quantity: "voltage" };
const SUPPLY: Trace = {
  vector: "v1#branch",
  label: "V1",
  quantity: "current",
};

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
    renderPanel(finished(null), true);
    expect(screen.getByRole("status").textContent).toBe(
      "Simulation complete in 2.0 s.",
    );
    expect(screen.getByText("Out of date")).toBeTruthy();
    expect(
      screen.getByRole("button", { name: "Run Simulation" }).className,
    ).toBe("button button--primary");
  });

  it("points an operating point run to the schematic annotations", () => {
    renderPanel(finished(null));
    expect(
      screen.getByText(/^Waveforms are not available for Operating Point/),
    ).toBeTruthy();
  });

  it("asks for a probe before a transient run has traces", () => {
    renderPanel(finished("time", ["time", "/out"]));
    expect(
      screen.getByText(/^Simulation complete\. Select the Probe tool/),
    ).toBeTruthy();
    expect(fetchWaveform).not.toHaveBeenCalled();
  });

  it("fetches and summarizes each trace the run has", async () => {
    vi.mocked(fetchWaveform).mockResolvedValue([
      { x: 0, y: -3.3 },
      { x: 1e-3, y: 3.3 },
    ]);
    const { onRemoveTrace } = renderPanel(
      finished("time", ["time", "/out"]),
      false,
      [OUT, SUPPLY],
    );
    const results = screen.getByRole("list", { name: "Waveform results" });
    expect(
      await within(results).findByText(
        "Voltage /OUT: minimum -3.30 V, maximum 3.30 V",
      ),
    ).toBeTruthy();
    expect(within(results).getByText("Current I(V1): No data")).toBeTruthy();
    expect(fetchWaveform).toHaveBeenCalledTimes(1);
    expect(fetchWaveform).toHaveBeenCalledWith(
      "run-1",
      "/out",
      null,
      null,
      PLOT_WIDTH,
    );
    const legend = screen.getByRole("list", { name: "Plotted signals" });
    expect(within(legend).getByText("I(V1) (No data)")).toBeTruthy();
    fireEvent.click(
      within(legend).getByRole("button", { name: "Remove /OUT from plot" }),
    );
    expect(onRemoveTrace).toHaveBeenCalledWith("/out");
  });

  it("reports a failed fetch", async () => {
    vi.mocked(fetchWaveform).mockRejectedValue(new Error("gone"));
    renderPanel(finished("time", ["time", "/out"]), false, [OUT]);
    expect(
      await screen.findByText("Failed to load waveform data."),
    ).toBeTruthy();
  });

  it("doesn't fetch results that are out of date", () => {
    renderPanel(finished("time", ["time", "/out"]), true, [OUT]);
    expect(fetchWaveform).not.toHaveBeenCalled();
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
