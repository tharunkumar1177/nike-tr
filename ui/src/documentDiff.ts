import type { DocumentDiff } from "./bindings/DocumentDiff";
import type { ProjectSnapshot } from "./bindings/ProjectSnapshot";
import type { RatsnestSegment } from "./bindings/RatsnestSegment";
import type { RatsnestUpdate } from "./bindings/RatsnestUpdate";

/**
 * Applies a diff from the core. Returns `null` when the diff was made against
 * a different revision, or when the simulation settings changed (the diff
 * doesn't carry them), in which case the caller must fetch a fresh snapshot.
 */
export function applyDiff(
  project: ProjectSnapshot,
  diff: DocumentDiff,
): ProjectSnapshot | null {
  if (
    diff.baseRevision !== project.revision ||
    diff.simulationSettingsChanged
  ) {
    return null;
  }
  return {
    ...project,
    revision: diff.revision,
    titleBlock: diff.titleBlock ?? project.titleBlock,
    gridSizeNm: diff.gridSizeNm ?? project.gridSizeNm,
    definitions: diff.definitions ?? project.definitions,
    instances: diff.instances ?? project.instances,
    wires: diff.wires ?? project.wires,
    junctions: diff.junctions ?? project.junctions,
    labels: diff.labels ?? project.labels,
    noConnects: diff.noConnects ?? project.noConnects,
    footprints: diff.footprints ?? project.footprints,
    boardOutline: diff.boardOutline ?? project.boardOutline,
    stackup: diff.stackup ?? project.stackup,
    ratsnest: applyRatsnest(project.ratsnest, diff.ratsnest),
    tracks: diff.tracks ?? project.tracks,
    vias: diff.vias ?? project.vias,
    zones: diff.zones ?? project.zones,
    designRules: diff.designRules ?? project.designRules,
    history: diff.history,
  };
}

/**
 * Replaces the lines of every net the update names with the update's lines.
 * Lines of other nets stay as they are.
 */
function applyRatsnest(
  lines: RatsnestSegment[],
  update: RatsnestUpdate | null,
): RatsnestSegment[] {
  if (update === null) {
    return lines;
  }
  const changed = new Set(update.nets);
  return [...lines.filter((line) => !changed.has(line.net)), ...update.lines];
}
