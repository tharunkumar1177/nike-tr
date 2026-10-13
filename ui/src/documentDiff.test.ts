import { describe, expect, it } from "vitest";
import type { DocumentDiff } from "./bindings/DocumentDiff";
import { applyDiff } from "./documentDiff";
import { projectSnapshot, resistorInstance, resistorPreview } from "./testData";

describe("applyDiff", () => {
  it("applies a diff made against the current revision", () => {
    const project = projectSnapshot({ revision: 4 });
    const diff: DocumentDiff = {
      baseRevision: 4,
      revision: 5,
      titleBlock: { ...project.titleBlock, title: "Amp" },
      gridSizeNm: null,
      definitions: null,
      instances: null,
      history: { ...project.history, dirty: true, canUndo: true },
    };
    const next = applyDiff(project, diff);
    expect(next?.revision).toBe(5);
    expect(next?.titleBlock.title).toBe("Amp");
    expect(next?.gridSizeNm).toBe(project.gridSizeNm);
    expect(next?.history.canUndo).toBe(true);
  });

  it("replaces definitions and instances when the diff carries them", () => {
    const project = projectSnapshot({ revision: 4 });
    const preview = resistorPreview();
    const instance = resistorInstance();
    const diff: DocumentDiff = {
      baseRevision: 4,
      revision: 5,
      titleBlock: null,
      gridSizeNm: null,
      definitions: { "Device:R": preview },
      instances: [instance],
      history: project.history,
    };
    const next = applyDiff(project, diff);
    expect(next?.definitions["Device:R"]).toBe(preview);
    expect(next?.instances).toEqual([instance]);
  });

  it("rejects a diff made against another revision", () => {
    const project = projectSnapshot({ revision: 4 });
    const diff: DocumentDiff = {
      baseRevision: 2,
      revision: 3,
      titleBlock: null,
      gridSizeNm: 2_540_000,
      definitions: null,
      instances: null,
      history: project.history,
    };
    expect(applyDiff(project, diff)).toBeNull();
  });
});
