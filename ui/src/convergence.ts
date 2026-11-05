/**
 * What the simulation panel and its screen-reader announcements say about how a run
 * converged, and how to fix a run that did not.
 */
import type { Convergence } from "./bindings/Convergence";
import type { ErrorReport } from "./ipc";
import { formatSeconds } from "./si";

/** A line shown under the simulation panel header after a run that needed help. */
export interface ConvergenceNotice {
  tone: "info" | "warning";
  text: string;
}

/** The fix to try for any run that failed to converge. */
export const CONVERGENCE_HINT =
  "Check for floating nodes, a voltage-source loop, or a missing ground.";

/** The notice for a finished run, or `null` when it converged directly. */
export function convergenceNotice(
  convergence: Convergence,
): ConvergenceNotice | null {
  switch (convergence) {
    case "direct":
      return null;
    case "stepped":
      return {
        tone: "info",
        text: "Converged with gmin or source stepping. The results are exact.",
      };
    case "rshuntRescue":
      return {
        tone: "warning",
        text: "The first attempt failed; Tron re-ran with a 1 GΩ shunt from every node to ground. Results are approximate — check for floating nodes.",
      };
  }
}

/** The announcement for a run that finished in `elapsedMs`. */
export function finishedAnnouncement(
  elapsedMs: number,
  convergence: Convergence,
): string {
  const complete = `Simulation complete in ${formatSeconds(elapsedMs)} seconds.`;
  switch (convergence) {
    case "direct":
      return complete;
    case "stepped":
      return `${complete} Converged with gmin or source stepping; the results are exact.`;
    case "rshuntRescue":
      return `${complete} The run needed a shunt-to-ground rescue; results are approximate. Check for floating nodes.`;
  }
}

/** The announcement for a run that failed with `error`. */
export function failedAnnouncement(error: ErrorReport): string {
  const failed = `Simulation failed: ${error.message}`;
  return error.kind === "simConvergence"
    ? `${failed} ${CONVERGENCE_HINT}`
    : failed;
}
