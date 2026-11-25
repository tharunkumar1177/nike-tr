import { describe, expect, it } from "vitest";
import type { BoardFootprint } from "./bindings/BoardFootprint";
import type { DocumentDiff } from "./bindings/DocumentDiff";
import type { Xy } from "./bindings/Xy";
import { applyDiff } from "./documentDiff";
import {
  projectSnapshot,
  resistorFootprint,
  resistorInstance,
  resistorPreview,
} from "./testData";

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
      wires: null,
      junctions: null,
      labels: null,
      noConnects: null,
      ercSettings: false,
      simulationSettingsChanged: false,
      footprints: null,
      boardOutline: null,
      stackup: null,
      ratsnest: null,
      tracks: null,
      vias: null,
      zones: null,
      designRules: null,
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
      wires: null,
      junctions: null,
      labels: null,
      noConnects: null,
      ercSettings: false,
      simulationSettingsChanged: false,
      footprints: null,
      boardOutline: null,
      stackup: null,
      ratsnest: null,
      tracks: null,
      vias: null,
      zones: null,
      designRules: null,
      history: project.history,
    };
    const next = applyDiff(project, diff);
    expect(next?.definitions["Device:R"]).toBe(preview);
    expect(next?.instances).toEqual([instance]);
  });

  it("replaces the wiring when the diff carries it", () => {
    const project = projectSnapshot({ revision: 4 });
    const wire = { uuid: "wire-1", start: [0, 0], end: [2_540_000, 0] };
    const diff: DocumentDiff = {
      baseRevision: 4,
      revision: 5,
      titleBlock: null,
      gridSizeNm: null,
      definitions: null,
      instances: null,
      wires: [{ uuid: "wire-1", start: [0, 0], end: [2_540_000, 0] }],
      junctions: [{ uuid: "junction-1", position: [0, 0] }],
      labels: [],
      noConnects: [{ uuid: "flag-1", position: [2_540_000, 0] }],
      ercSettings: false,
      simulationSettingsChanged: false,
      footprints: null,
      boardOutline: null,
      stackup: null,
      ratsnest: null,
      tracks: null,
      vias: null,
      zones: null,
      designRules: null,
      history: project.history,
    };
    const next = applyDiff(project, diff);
    expect(next?.wires).toEqual([wire]);
    expect(next?.junctions).toHaveLength(1);
    expect(next?.noConnects).toEqual([
      { uuid: "flag-1", position: [2_540_000, 0] },
    ]);
    expect(next?.instances).toBe(project.instances);
  });

  it("replaces footprints and the ratsnest of changed nets", () => {
    const project = projectSnapshot({
      revision: 4,
      ratsnest: [
        { net: "/OUT", from: [0, 0], to: [5_000_000, 0] },
        { net: "GND", from: [0, 1_000_000], to: [5_000_000, 1_000_000] },
      ],
    });
    const footprint: BoardFootprint = resistorFootprint();
    const diff: DocumentDiff = {
      baseRevision: 4,
      revision: 5,
      titleBlock: null,
      gridSizeNm: null,
      definitions: null,
      instances: null,
      wires: null,
      junctions: null,
      labels: null,
      noConnects: null,
      ercSettings: false,
      simulationSettingsChanged: false,
      footprints: [footprint],
      boardOutline: null,
      stackup: null,
      ratsnest: {
        nets: ["/OUT", "+5V"],
        lines: [{ net: "/OUT", from: [0, 0], to: [6_000_000, 0] }],
      },
      tracks: null,
      vias: null,
      zones: null,
      designRules: null,
      history: project.history,
    };
    const next = applyDiff(project, diff);
    expect(next?.footprints).toEqual([footprint]);
    expect(next?.boardOutline).toBe(project.boardOutline);
    expect(next?.ratsnest).toEqual([
      { net: "GND", from: [0, 1_000_000], to: [5_000_000, 1_000_000] },
      { net: "/OUT", from: [0, 0], to: [6_000_000, 0] },
    ]);
  });

  it("replaces routed copper and design rules from the diff", () => {
    const project = projectSnapshot({ revision: 4 });
    const track = {
      uuid: "track-1",
      start: [0, 0] as Xy,
      end: [5_000_000, 0] as Xy,
      widthNm: 250_000,
      layer: "F.Cu",
      net: "GND",
    };
    const designRules = { ...project.designRules, trackWidthNm: 250_000 };
    const diff: DocumentDiff = {
      baseRevision: 4,
      revision: 5,
      titleBlock: null,
      gridSizeNm: null,
      definitions: null,
      instances: null,
      wires: null,
      junctions: null,
      labels: null,
      noConnects: null,
      ercSettings: false,
      simulationSettingsChanged: false,
      footprints: null,
      boardOutline: null,
      stackup: null,
      ratsnest: null,
      tracks: [track],
      vias: [],
      zones: [],
      designRules,
      history: project.history,
    };
    const next = applyDiff(project, diff);
    expect(next?.tracks).toEqual([track]);
    expect(next?.designRules.trackWidthNm).toBe(250_000);
    expect(next?.footprints).toBe(project.footprints);
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
      wires: null,
      junctions: null,
      labels: null,
      noConnects: null,
      ercSettings: false,
      simulationSettingsChanged: false,
      footprints: null,
      boardOutline: null,
      stackup: null,
      ratsnest: null,
      tracks: null,
      vias: null,
      zones: null,
      designRules: null,
      history: project.history,
    };
    expect(applyDiff(project, diff)).toBeNull();
  });

  it("asks for a fresh snapshot when the simulation settings changed", () => {
    const project = projectSnapshot({ revision: 4 });
    const diff: DocumentDiff = {
      baseRevision: 4,
      revision: 5,
      titleBlock: null,
      gridSizeNm: null,
      definitions: null,
      instances: null,
      wires: null,
      junctions: null,
      labels: null,
      noConnects: null,
      ercSettings: false,
      simulationSettingsChanged: true,
      footprints: null,
      boardOutline: null,
      stackup: null,
      ratsnest: null,
      tracks: null,
      vias: null,
      zones: null,
      designRules: null,
      history: project.history,
    };
    expect(applyDiff(project, diff)).toBeNull();
  });
});
