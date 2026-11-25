import type { BoardDesignRules } from "./bindings/BoardDesignRules";
import type { BoardFootprint } from "./bindings/BoardFootprint";
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
    tracks: [],
    vias: [],
    zones: [],
    designRules: defaultDesignRules(),
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

/** KiCad's default design rules, as the core reports them for a new board. */
export function defaultDesignRules(): BoardDesignRules {
  return {
    clearanceNm: 200_000,
    minTrackWidthNm: 200_000,
    minViaDiameterNm: 500_000,
    minViaDrillNm: 300_000,
    minAnnularRingNm: 100_000,
    edgeClearanceNm: 500_000,
    trackWidthNm: 200_000,
    viaDiameterNm: 600_000,
    viaDrillNm: 300_000,
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

/** An R_0603_1608Metric placed on the front at (20, 20) mm, as the core reports it. */
export function resistorFootprint(
  overrides: Partial<BoardFootprint> = {},
): BoardFootprint {
  const pad = (number: string, x: number, net: string) => ({
    number,
    position: [x, 20_000_000] as [number, number],
    net,
    shape: "round_rect" as const,
    size: [800_000, 950_000] as [number, number],
    rotation: 0,
    cornerRadiusNm: 200_000,
    drill: null,
  });
  return {
    uuid: "footprint-1",
    libId: "Resistor_SMD:R_0603_1608Metric",
    position: [20_000_000, 20_000_000],
    rotation: 0,
    side: "front",
    reference: "R1",
    value: "10k",
    pads: [pad("1", 19_175_000, "GND"), pad("2", 20_825_000, "/OUT")],
    bounds: { min: [18_520_000, 19_270_000], max: [21_480_000, 20_730_000] },
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
