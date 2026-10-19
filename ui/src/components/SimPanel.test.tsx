import {
  cleanup,
  fireEvent,
  render,
  screen,
  waitFor,
  within,
} from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { TraceMeasurements } from "../bindings/TraceMeasurements";
import { type ErrorReport, fetchWaveform, measureTrace } from "../ipc";
import { SimPanel, type SimRun } from "./SimPanel";
import type { Trace } from "./WaveformViewer";

vi.mock("../ipc", async (importOriginal) => ({
  ...(await importOriginal<typeof import("../ipc")>()),
  fetchWaveform: vi.fn(),
  measureTrace: vi.fn(),
}));

const PLOT_WIDTH = 400;
/** Where the plot area starts and ends with only a voltage axis. */
const PLOT_LEFT = 56;
const PLOT_RIGHT = PLOT_WIDTH - 12;

/** The client x of the point `fraction` across the plot area. */
function plotX(fraction: number): number {
  return PLOT_LEFT + fraction * (PLOT_RIGHT - PLOT_LEFT);
}

const SPAN: TraceMeasurements = {
  from: 0,
  to: 1e-3,
  min: -3.3,
  max: 3.3,
  average: 0,
  rms: 2.5,
  peakToPeak: 6.6,
};

const AC_SPAN: TraceMeasurements = {
  from: 1,
  to: 1e6,
  min: -60,
  max: 0,
  average: null,
  rms: null,
  peakToPeak: 60,
};

class FixedSizeObserver {
  private readonly callback: ResizeObserverCallback;

  constructor(callback: ResizeObserverCallback) {
    this.callback = callback;
  }

  observe() {
    const entry = { contentRect: { width: PLOT_WIDTH, height: 200 } };
    this.callback(
      [entry as unknown as ResizeObserverEntry],
      this as unknown as ResizeObserver,
    );
  }

  unobserve() {
    // Nothing to release in the stub.
  }

  disconnect() {
    // Nothing to release in the stub.
  }
}

beforeEach(() => {
  vi.stubGlobal("ResizeObserver", FixedSizeObserver);
  vi.mocked(fetchWaveform).mockReset();
  vi.mocked(fetchWaveform).mockResolvedValue([
    { x: 0, y: -3.3 },
    { x: 1e-3, y: 3.3 },
  ]);
  vi.mocked(measureTrace).mockReset();
  vi.mocked(measureTrace).mockImplementation(
    async (_run, _vector, _quantity, _from, _to, cursors) => ({
      span: SPAN,
      cursors: cursors.map((_at, index) => (index === 0 ? -1 : 2)),
    }),
  );
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
  const analysisKind =
    scale === null ? "operatingPoint" : scale === "time" ? "transient" : "ac";
  return {
    state: "finished",
    elapsedMs: 2000,
    result: { runId: "run-1", scale, vectors, analysisKind },
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

  it("fetches each trace the run has and tabulates its measurements", async () => {
    const { onRemoveTrace } = renderPanel(
      finished("time", ["time", "/out"]),
      false,
      [OUT, SUPPLY],
    );
    const table = screen.getByRole("table", {
      name: "Measurements cover the visible range.",
    });
    expect(await within(table).findByText("6.60 V")).toBeTruthy();
    const header = within(table).getByRole("rowheader", { name: "/OUT" });
    const cells = within(header.closest("tr")!).getAllByRole("cell");
    expect(cells.slice(0, 8).map((cell) => cell.textContent)).toEqual([
      "—",
      "—",
      "—",
      "-3.30 V",
      "3.30 V",
      "0 V",
      "2.50 V",
      "6.60 V",
    ]);
    expect(
      within(table).getByRole("rowheader", { name: "I(V1) (No data)" }),
    ).toBeTruthy();
    expect(fetchWaveform).toHaveBeenCalledTimes(1);
    expect(fetchWaveform).toHaveBeenCalledWith(
      "run-1",
      "/out",
      null,
      null,
      null,
      PLOT_WIDTH,
    );
    expect(measureTrace).toHaveBeenCalledWith(
      "run-1",
      "/out",
      null,
      null,
      null,
      [],
    );
    fireEvent.click(
      within(table).getByRole("button", { name: "Remove /OUT from plot" }),
    );
    expect(onRemoveTrace).toHaveBeenCalledWith("/out");
  });

  it("zooms the time axis with the wheel and fits it again", async () => {
    renderPanel(finished("time", ["time", "/out"]), false, [OUT]);
    expect(await screen.findByText("6.60 V")).toBeTruthy();
    const plot = screen.getByLabelText("Waveform plot");
    expect(plot.dataset.maxX).toBe("0.001");
    const fit = screen.getByRole<HTMLButtonElement>("button", {
      name: "Zoom to Fit",
    });
    expect(fit.disabled).toBe(true);

    fireEvent.wheel(plot, { deltaY: -100, clientX: plotX(0.5) });
    expect(Number(plot.dataset.minX)).toBeCloseTo(1e-4, 12);
    expect(Number(plot.dataset.maxX)).toBeCloseTo(9e-4, 12);
    await waitFor(() => expect(fetchWaveform).toHaveBeenCalledTimes(2));
    const zoomed = vi.mocked(fetchWaveform).mock.calls[1]!;
    const [, , , minX, maxX, budget] = zoomed;
    expect(minX).toBeCloseTo(1e-4, 12);
    expect(maxX).toBeCloseTo(9e-4, 12);
    expect(budget).toBe(PLOT_WIDTH);
    expect(
      await screen.findByText(/^Zoomed to 100 µs to 900 µs\.$/),
    ).toBeTruthy();

    fireEvent.keyDown(plot, { key: "ArrowRight" });
    expect(Number(plot.dataset.minX)).toBeCloseTo(1.8e-4, 12);
    fireEvent.click(fit);
    expect(plot.dataset.minX).toBe("0");
    expect(plot.dataset.maxX).toBe("0.001");
    expect(fit.disabled).toBe(true);
  });

  it("measures between two cursors placed in measure mode", async () => {
    renderPanel(finished("time", ["time", "/out"]), false, [OUT]);
    expect(await screen.findByText("6.60 V")).toBeTruthy();
    const measure = screen.getByRole("button", { name: "Measure" });
    fireEvent.click(measure);
    expect(measure.getAttribute("aria-pressed")).toBe("true");
    const plot = screen.getByLabelText("Waveform plot");
    const pointer = { pointerId: 1, button: 0 };
    fireEvent.pointerDown(plot, { ...pointer, clientX: plotX(0.25) });
    fireEvent.pointerMove(plot, { ...pointer, clientX: plotX(0.75) });
    fireEvent.pointerUp(plot, { ...pointer, clientX: plotX(0.75) });
    expect(Number(plot.dataset.cursorA)).toBeCloseTo(2.5e-4, 12);
    expect(Number(plot.dataset.cursorB)).toBeCloseTo(7.5e-4, 12);

    const table = await screen.findByRole("table", {
      name: "Measurements cover the span between cursors.",
    });
    const column = (name: string) =>
      within(table).getByRole("columnheader", { name });
    expect(column("Cursor A (250 µs)")).toBeTruthy();
    expect(column("Δ (500 µs, 2.00 kHz)")).toBeTruthy();
    expect(await within(table).findByText("-1.00 V")).toBeTruthy();
    expect(within(table).getByText("2.00 V")).toBeTruthy();
    expect(within(table).getByText("3.00 V")).toBeTruthy();
    const [, , , from, to, cursors] = vi.mocked(measureTrace).mock.lastCall!;
    expect(from).toBeCloseTo(2.5e-4, 12);
    expect(to).toBeCloseTo(7.5e-4, 12);
    expect(cursors).toHaveLength(2);
    expect(screen.getByText("Cursor B placed at 750 µs.")).toBeTruthy();

    fireEvent.keyDown(plot, { key: "Escape" });
    expect(plot.dataset.cursorA).toBeUndefined();
    expect(plot.dataset.cursorB).toBeUndefined();
    expect(screen.getByText("Cursors cleared.")).toBeTruthy();
  });

  it("places and moves cursors from the keyboard", async () => {
    renderPanel(finished("time", ["time", "/out"]), false, [OUT]);
    expect(await screen.findByText("6.60 V")).toBeTruthy();
    const plot = screen.getByLabelText("Waveform plot");
    fireEvent.keyDown(plot, { key: "a" });
    expect(plot.dataset.cursorA).toBe("0.0005");
    expect(screen.getByText("Cursor A placed at 500 µs.")).toBeTruthy();
    fireEvent.keyDown(plot, { key: "ArrowRight", shiftKey: true });
    expect(Number(plot.dataset.cursorA)).toBeCloseTo(5.1e-4, 12);
    expect(screen.getByText("Cursor A moved to 510 µs.")).toBeTruthy();
    expect(
      screen.getByRole("table", {
        name: "Measurements cover the visible range.",
      }),
    ).toBeTruthy();
  });

  it("plots AC magnitude and phase against log frequency", async () => {
    vi.mocked(fetchWaveform).mockResolvedValue([
      { x: 1, y: 0 },
      { x: 1e6, y: -60 },
    ]);
    vi.mocked(measureTrace).mockImplementation(
      async (_run, _vector, quantity, _from, _to, cursors) => ({
        span: quantity === null ? SPAN : AC_SPAN,
        cursors: cursors.map((_at, index) => (index === 0 ? -1 : 2)),
      }),
    );
    const { onRemoveTrace } = renderPanel(
      finished("frequency", ["frequency", "/out"]),
      false,
      [OUT],
    );
    const table = screen.getByRole("table", {
      name: "Measurements cover the visible range.",
    });
    expect(await within(table).findByText("60.00 dB")).toBeTruthy();
    const magnitude = within(table).getByRole("rowheader", {
      name: "/OUT Mag",
    });
    const cells = within(magnitude.closest("tr")!).getAllByRole("cell");
    expect(cells.slice(0, 8).map((cell) => cell.textContent)).toEqual([
      "—",
      "—",
      "—",
      "-60.00 dB",
      "0.00 dB",
      "—",
      "—",
      "60.00 dB",
    ]);
    expect(
      within(table).getByRole("rowheader", { name: "/OUT Phase" }),
    ).toBeTruthy();
    expect(within(table).getByText("60.00 °")).toBeTruthy();
    for (const acQuantity of ["magnitudeDb", "phaseDeg"]) {
      expect(fetchWaveform).toHaveBeenCalledWith(
        "run-1",
        "/out",
        acQuantity,
        null,
        null,
        PLOT_WIDTH,
      );
    }

    const plot = screen.getByLabelText("Waveform plot");
    expect(plot.dataset.xScale).toBe("log");
    expect(plot.dataset.minX).toBe("1");
    expect(plot.dataset.maxX).toBe("1000000");
    fireEvent.keyDown(plot, { key: "a" });
    expect(Number(plot.dataset.cursorA)).toBeCloseTo(1e3, 9);
    expect(screen.getByText("Cursor A placed at 1.00 kHz.")).toBeTruthy();
    fireEvent.keyDown(plot, { key: "b" });
    fireEvent.keyDown(plot, { key: "ArrowRight", altKey: true });
    expect(Number(plot.dataset.cursorB)).toBeCloseTo(10 ** 3.06, 6);
    const column = (name: string) =>
      within(table).getByRole("columnheader", { name });
    expect(column("Cursor A (1.00 kHz)")).toBeTruthy();
    expect(column("Δ (148 Hz, ×1.15)")).toBeTruthy();
    expect(await within(table).findAllByText("-1.00 dB")).toHaveLength(1);
    const measured = vi.mocked(measureTrace).mock.lastCall!;
    const [, , quantity, from, to, at] = measured;
    expect(quantity).not.toBeNull();
    expect(from).toBeCloseTo(1e3, 9);
    expect(to).toBeCloseTo(10 ** 3.06, 6);
    expect(at[0]).toBeCloseTo(1e3, 9);

    const remove = within(table).getAllByRole("button", {
      name: "Remove /OUT from plot",
    });
    expect(remove).toHaveLength(1);
    fireEvent.click(remove[0]!);
    expect(onRemoveTrace).toHaveBeenCalledWith("/out");
  });

  it("freezes the view and cursors of results that are out of date", () => {
    renderPanel(finished("time", ["time", "/out"]), true, [OUT]);
    const measure = screen.getByRole<HTMLButtonElement>("button", {
      name: "Measure",
    });
    expect(measure.disabled).toBe(true);
    const plot = screen.getByLabelText("Waveform plot");
    fireEvent.keyDown(plot, { key: "a" });
    expect(plot.dataset.cursorA).toBeUndefined();
    expect(measureTrace).not.toHaveBeenCalled();
  });

  it("resizes from the keyboard within its limits", () => {
    renderPanel({ state: "idle" });
    const handle = screen.getByRole("separator", {
      name: "Resize simulation panel",
    });
    const panel = screen.getByRole("region", { name: "Simulation" });
    expect(handle.getAttribute("aria-valuenow")).toBe("240");
    fireEvent.keyDown(handle, { key: "ArrowUp" });
    expect(handle.getAttribute("aria-valuenow")).toBe("260");
    expect(panel.style.height).toBe("260px");
    fireEvent.keyDown(handle, { key: "ArrowDown" });
    fireEvent.keyDown(handle, { key: "ArrowDown" });
    expect(handle.getAttribute("aria-valuenow")).toBe("240");
    fireEvent.keyDown(handle, { key: "End" });
    const max = String(Math.round(window.innerHeight * 0.8));
    expect(handle.getAttribute("aria-valuenow")).toBe(max);
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
