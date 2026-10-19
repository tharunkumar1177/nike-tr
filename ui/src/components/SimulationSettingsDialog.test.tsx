import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import type { AnalysisSettings } from "../bindings/AnalysisSettings";
import type { ErrorReport } from "../ipc";
import { SimulationSettingsDialog } from "./SimulationSettingsDialog";

afterEach(cleanup);

type Apply = (settings: AnalysisSettings) => Promise<ErrorReport | null>;

function renderDialog(settings: AnalysisSettings | null) {
  const onApply = vi.fn<Apply>(() => Promise.resolve(null));
  const onCancel = vi.fn();
  render(
    <SimulationSettingsDialog
      settings={settings}
      onApply={onApply}
      onCancel={onCancel}
    />,
  );
  return { onApply, onCancel };
}

function field(label: string): HTMLInputElement {
  return screen.getByLabelText<HTMLInputElement>(label);
}

function analysisType(): HTMLSelectElement {
  return screen.getByLabelText<HTMLSelectElement>("Analysis type");
}

function type(label: string, value: string) {
  fireEvent.change(field(label), { target: { value } });
}

function ok(): HTMLButtonElement {
  return screen.getByRole<HTMLButtonElement>("button", { name: "OK" });
}

describe("SimulationSettingsDialog", () => {
  it("starts from a 1u / 1m / 0 transient when no analysis is set", () => {
    renderDialog(null);
    expect(analysisType().value).toBe("transient");
    expect(field("Time step").value).toBe("1u");
    expect(field("Final time").value).toBe("1m");
    expect(field("Initial time").value).toBe("0");
    expect(field("Maximum time step").value).toBe("");
    expect(field("Maximum time step").placeholder).toBe("Optional");
  });

  it("shows saved settings with SI suffixes", () => {
    renderDialog({
      kind: "transient",
      step: 1e-5,
      stop: 2.5e-3,
      start: 0,
      maxStep: 1.5e-7,
      initialConditions: true,
    });
    expect(field("Time step").value).toBe("10u");
    expect(field("Final time").value).toBe("2.5m");
    expect(field("Maximum time step").value).toBe("150n");
    expect(field("Start from initial conditions (UIC)").checked).toBe(true);
  });

  it("hides the transient fields for an operating point", () => {
    renderDialog(null);
    fireEvent.change(analysisType(), { target: { value: "operatingPoint" } });
    expect(screen.queryByLabelText("Time step")).toBeNull();
  });

  it("shows each field's first failing check and disables OK", () => {
    renderDialog(null);
    type("Time step", "-1");
    expect(screen.getByText("Must be greater than 0.")).toBeTruthy();
    expect(field("Time step").getAttribute("aria-invalid")).toBe("true");
    expect(ok().disabled).toBe(true);

    type("Time step", "abc");
    expect(screen.getByText("Invalid SI number.")).toBeTruthy();

    type("Time step", "1u");
    type("Initial time", "2m");
    expect(screen.getByText("Must be less than final time.")).toBeTruthy();

    type("Initial time", "0.5m");
    type("Maximum time step", "1");
    expect(
      screen.getByText("Must not exceed final time minus initial time."),
    ).toBeTruthy();
    expect(ok().disabled).toBe(true);
  });

  it("applies the parsed settings", async () => {
    const { onApply } = renderDialog(null);
    type("Time step", "10u");
    type("Final time", "5m");
    type("Maximum time step", "1u");
    fireEvent.click(field("Start from initial conditions (UIC)"));
    fireEvent.click(ok());
    await vi.waitFor(() => expect(onApply).toHaveBeenCalled());
    expect(onApply).toHaveBeenCalledWith({
      kind: "transient",
      step: 1e-5,
      stop: 5e-3,
      start: 0,
      maxStep: 1e-6,
      initialConditions: true,
    });
  });

  it("closes without applying when nothing changed", () => {
    const { onApply, onCancel } = renderDialog({ kind: "operatingPoint" });
    fireEvent.click(ok());
    expect(onCancel).toHaveBeenCalled();
    expect(onApply).not.toHaveBeenCalled();
  });

  it("shows an error from the core", async () => {
    const onApply = vi.fn<Apply>(() =>
      Promise.resolve({
        kind: "noProject",
        message: "No project is open.",
        detail: null,
      }),
    );
    render(
      <SimulationSettingsDialog
        settings={null}
        onApply={onApply}
        onCancel={vi.fn()}
      />,
    );
    fireEvent.click(ok());
    expect((await screen.findByRole("alert")).textContent).toBe(
      "No project is open.",
    );
  });
});
