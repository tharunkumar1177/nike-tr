/** Tests the convergence notices and the announcements for finished and failed runs. */
import { describe, expect, it } from "vitest";
import {
  CONVERGENCE_HINT,
  convergenceNotice,
  failedAnnouncement,
  finishedAnnouncement,
} from "./convergence";

describe("convergence", () => {
  it("adds a notice only when a run needed help", () => {
    expect(convergenceNotice("direct")).toBeNull();
    expect(convergenceNotice("stepped")?.tone).toBe("info");
    expect(convergenceNotice("rshuntRescue")?.tone).toBe("warning");
  });

  it("announces how a finished run converged", () => {
    expect(finishedAnnouncement(1250, "direct")).toBe(
      "Simulation complete in 1.3 seconds.",
    );
    expect(finishedAnnouncement(2000, "stepped")).toBe(
      "Simulation complete in 2.0 seconds. Converged with gmin or source stepping; the results are exact.",
    );
    expect(finishedAnnouncement(2000, "rshuntRescue")).toBe(
      "Simulation complete in 2.0 seconds. The run needed a shunt-to-ground rescue; results are approximate. Check for floating nodes.",
    );
  });

  it("adds the hint only to convergence failures", () => {
    const message = "The operating point did not converge: singular matrix.";
    expect(
      failedAnnouncement({ kind: "simConvergence", message, detail: null }),
    ).toBe(`Simulation failed: ${message} ${CONVERGENCE_HINT}`);
    expect(
      failedAnnouncement({ kind: "simNoGround", message, detail: null }),
    ).toBe(`Simulation failed: ${message}`);
  });
});
