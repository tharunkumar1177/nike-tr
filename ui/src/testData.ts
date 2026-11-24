import type { BoardStackup } from "./bindings/BoardStackup";
import type { ProjectSnapshot } from "./bindings/ProjectSnapshot";
import type { StackupEntry } from "./bindings/StackupEntry";
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
    noConnects: [],
    simulationSettings: null,
    footprints: [],
    boardOutline: [],
    stackup: newBoardStackup(),
    ratsnest: [],
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

/** The 2-layer, 1.6 mm stackup the core gives a new board. */
export function newBoardStackup(): BoardStackup {
  const layer = (
    name: string,
    kind: string,
    thicknessNm: number | null,
  ): StackupEntry => ({
    name,
    kind,
    thicknessNm,
    material: null,
    epsilonR: null,
    lossTangent: null,
  });
  return {
    copperLayers: 2,
    thicknessNm: 1_600_000,
    layers: [
      layer("F.SilkS", "Top Silk Screen", null),
      layer("F.Paste", "Top Solder Paste", null),
      layer("F.Mask", "Top Solder Mask", 10_000),
      layer("F.Cu", "copper", 35_000),
      {
        ...layer("dielectric 1", "core", 1_510_000),
        material: "FR4",
        epsilonR: 4.5,
        lossTangent: 0.02,
      },
      layer("B.Cu", "copper", 35_000),
      layer("B.Mask", "Bottom Solder Mask", 10_000),
      layer("B.Paste", "Bottom Solder Paste", null),
      layer("B.SilkS", "Bottom Silk Screen", null),
    ],
    copperFinish: "None",
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
