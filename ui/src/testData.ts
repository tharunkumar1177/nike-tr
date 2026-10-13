import type { ProjectSnapshot } from "./bindings/ProjectSnapshot";
import type { SymbolInstance } from "./bindings/SymbolInstance";
import type { SymbolPreview } from "./bindings/SymbolPreview";

/** A snapshot shaped like the core's for a new, unedited project. */
export function projectSnapshot(
  overrides: Partial<ProjectSnapshot> = {},
): ProjectSnapshot {
  return {
    revision: 1,
    name: "amp",
    directory: "C:\\work\\amp",
    titleBlock: {
      title: "",
      date: "",
      revision: "",
      company: "",
      comments: ["", "", "", "", "", "", "", "", ""],
    },
    gridSizeNm: 1_270_000,
    sheet: { widthNm: 297_000_000, heightNm: 210_000_000 },
    definitions: {},
    instances: [],
    wires: [],
    junctions: [],
    labels: [],
    history: {
      dirty: false,
      canUndo: false,
      canRedo: false,
      undoLabel: null,
      redoLabel: null,
    },
    ...overrides,
  };
}

/** The Device:R unit-1 drawing, shaped like the library index geometry. */
export function resistorPreview(): SymbolPreview {
  return {
    shapes: [
      {
        type: "rect",
        start: [-1_016_000, -2_540_000],
        end: [1_016_000, 2_540_000],
        width: 254_000,
        fill: "none",
      },
    ],
    pins: [
      {
        connection: [0, -3_810_000],
        body: [0, -2_540_000],
        number: "1",
        name: "~",
        electrical: "passive",
      },
      {
        connection: [0, 3_810_000],
        body: [0, 2_540_000],
        number: "2",
        name: "~",
        electrical: "passive",
      },
    ],
    pin_numbers_visible: false,
    pin_names_visible: false,
  };
}

/** A placed resistor at (100, 50) mm, as the core reports it. */
export function resistorInstance(
  overrides: Partial<SymbolInstance> = {},
): SymbolInstance {
  return {
    uuid: "symbol-1",
    libId: "Device:R",
    xNm: 100_000_000,
    yNm: 50_000_000,
    rotation: 0,
    mirror: null,
    reference: {
      text: "R?",
      xNm: 100_000_000,
      yNm: 46_040_000,
      rotation: 0,
      visible: true,
    },
    value: {
      text: "R",
      xNm: 100_000_000,
      yNm: 53_960_000,
      rotation: 0,
      visible: true,
    },
    pins: [
      [100_000_000, 46_190_000],
      [100_000_000, 53_810_000],
    ],
    ...overrides,
  };
}
