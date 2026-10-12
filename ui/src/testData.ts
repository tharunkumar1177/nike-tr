import type { ProjectSnapshot } from "./bindings/ProjectSnapshot";

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
