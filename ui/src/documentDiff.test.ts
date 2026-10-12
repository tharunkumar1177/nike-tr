import { describe, expect, it } from "vitest";
import type { DocumentDiff } from "./bindings/DocumentDiff";
import { applyDiff } from "./documentDiff";
import { projectSnapshot } from "./testData";

describe("applyDiff", () => {
  it("applies a diff made against the current revision", () => {
    const project = projectSnapshot({ revision: 4 });
    const diff: DocumentDiff = {
      baseRevision: 4,
      revision: 5,
      titleBlock: { ...project.titleBlock, title: "Amp" },
      gridSizeNm: null,
      history: { ...project.history, dirty: true, canUndo: true },
    };
    const next = applyDiff(project, diff);
    expect(next?.revision).toBe(5);
    expect(next?.titleBlock.title).toBe("Amp");
    expect(next?.gridSizeNm).toBe(project.gridSizeNm);
    expect(next?.history.canUndo).toBe(true);
  });

  it("rejects a diff made against another revision", () => {
    const project = projectSnapshot({ revision: 4 });
    const diff: DocumentDiff = {
      baseRevision: 2,
      revision: 3,
      titleBlock: null,
      gridSizeNm: 2_540_000,
      history: project.history,
    };
    expect(applyDiff(project, diff)).toBeNull();
  });
});
