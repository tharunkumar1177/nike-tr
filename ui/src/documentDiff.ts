import type { DocumentDiff } from "./bindings/DocumentDiff";
import type { ProjectSnapshot } from "./bindings/ProjectSnapshot";

/**
 * Applies a diff from the core. Returns `null` when the diff was made against
 * a different revision, in which case the caller must fetch a fresh snapshot.
 */
export function applyDiff(
  project: ProjectSnapshot,
  diff: DocumentDiff,
): ProjectSnapshot | null {
  if (diff.baseRevision !== project.revision) {
    return null;
  }
  return {
    ...project,
    revision: diff.revision,
    titleBlock: diff.titleBlock ?? project.titleBlock,
    gridSizeNm: diff.gridSizeNm ?? project.gridSizeNm,
    history: diff.history,
  };
}
