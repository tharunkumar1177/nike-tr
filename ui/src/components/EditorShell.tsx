import { useEffect, useLayoutEffect, useRef, useState } from "react";
import type { ErcViolation } from "../bindings/ErcViolation";
import type { LibraryHit } from "../bindings/LibraryHit";
import type { OpAnnotation } from "../bindings/OpAnnotation";
import type { ProjectSnapshot } from "../bindings/ProjectSnapshot";
import type { SimulationProgress } from "../bindings/SimulationProgress";
import type { Xy } from "../bindings/Xy";
import {
  SIMULATION_PROGRESS_EVENT,
  cancelSimulation,
  describeIpcError,
  fetchOpAnnotations,
  runErc,
  startSimulation,
  subscribe,
  toErrorReport,
} from "../ipc";
import type { ErcMarker } from "../schematicDraw";
import { formatSeconds } from "../si";
import type { LibraryStatusState } from "../useLibraryStatus";
import {
  type ScreenPoint,
  type Size,
  type Viewport,
  formatZoom,
} from "../viewport";
import { ErcPanel } from "./ErcPanel";
import { LibraryPanel } from "./LibraryPanel";
import { SimPanel, type SimRun } from "./SimPanel";
import {
  type CanvasTool,
  type PlacementRequest,
  SchematicCanvas,
} from "./SchematicCanvas";
import { StatusBar, type StatusHint } from "./StatusBar";
import { Toolbar } from "./Toolbar";
import type { Trace } from "./WaveformViewer";

/** The last ERC run, and the document revision it checked. */
interface ErcResults {
  violations: ErcViolation[];
  revision: number;
  directory: string;
}

/** The document revision a simulation ran on. */
interface SimSource {
  revision: number;
  directory: string;
}

const NO_MARKERS: ErcMarker[] = [];
const NO_ANNOTATIONS: OpAnnotation[] = [];
const IDLE: SimRun = { state: "idle" };
const ELAPSED_TICK_MS = 100;

function countOf(count: number, noun: string): string {
  return `${count} ${noun}${count === 1 ? "" : "s"}`;
}

function ercSummary(violations: ErcViolation[]): string {
  if (violations.length === 0) {
    return "ERC complete: No violations found";
  }
  const errors = violations.filter((item) => item.severity === "error").length;
  const warnings = violations.length - errors;
  return `ERC complete: ${countOf(errors, "error")}, ${countOf(warnings, "warning")}`;
}

interface EditorShellProps {
  project: ProjectSnapshot;
  viewport: Viewport | null;
  controlsDisabled: boolean;
  library: LibraryStatusState;
  /** Counts annotation requests from the menu; each increase runs one. */
  annotateTicket: number;
  /** Counts Electrical Rules Checker menu requests; each toggles the panel. */
  ercTicket: number;
  /** Increases when the ERC rule severities change; an open panel re-runs. */
  ercSettingsVersion: number;
  /** Counts Run Simulation menu requests; each increase starts one run. */
  simulationTicket: number;
  onOpenSimulationSettings: () => void;
  onViewportChange: (update: (viewport: Viewport) => Viewport) => void;
  onCanvasResize: (size: Size) => void;
  onUndo: () => void;
  onRedo: () => void;
  onOpenProperties: () => void;
  onOpenErcSeverities: () => void;
  onDownloadLibrary: () => void;
}

export function EditorShell({
  project,
  viewport,
  controlsDisabled,
  library,
  annotateTicket,
  ercTicket,
  ercSettingsVersion,
  simulationTicket,
  onOpenSimulationSettings,
  onViewportChange,
  onCanvasResize,
  onUndo,
  onRedo,
  onOpenProperties,
  onOpenErcSeverities,
  onDownloadLibrary,
}: EditorShellProps) {
  const ercTicketRef = useRef(ercTicket);
  const ercSettingsRef = useRef(ercSettingsVersion);
  const [ercOpen, setErcOpen] = useState(false);
  const [ercResults, setErcResults] = useState<ErcResults | null>(null);
  const [ercRunning, setErcRunning] = useState(false);
  const [ercError, setErcError] = useState<string | null>(null);
  const [ercAnnouncement, setErcAnnouncement] = useState("");
  const [selectedViolation, setSelectedViolation] = useState<string | null>(
    null,
  );
  const [focusRequest, setFocusRequest] = useState<Xy | null>(null);
  const [cursor, setCursor] = useState<ScreenPoint | null>(null);
  const [hint, setHint] = useState<StatusHint | null>(null);
  const [libraryOpen, setLibraryOpen] = useState(false);
  const [placementRequest, setPlacementRequest] =
    useState<PlacementRequest | null>(null);
  const [symbolDrag, setSymbolDrag] = useState<LibraryHit | null>(null);
  const [wireMode, setWireMode] = useState(false);
  const [noConnectMode, setNoConnectMode] = useState(false);
  const [toolRequest, setToolRequest] = useState<CanvasTool | null>(null);
  const zoomLabel = viewport ? formatZoom(viewport.zoom) : "—";

  const results =
    ercResults && ercResults.directory === project.directory
      ? ercResults
      : null;
  const ercStale = results !== null && results.revision !== project.revision;
  const ercMarkers = ercOpen && results ? results.violations : NO_MARKERS;

  const simTicketRef = useRef(simulationTicket);
  const simStartRef = useRef(0);
  const [simOpen, setSimOpen] = useState(false);
  const [simRun, setSimRun] = useState<SimRun>(IDLE);
  const [simSource, setSimSource] = useState<SimSource | null>(null);
  const [simAnnouncement, setSimAnnouncement] = useState("");
  const [opAnnotations, setOpAnnotations] = useState(NO_ANNOTATIONS);
  const [annotationsVisible, setAnnotationsVisible] = useState(true);
  const [traces, setTraces] = useState<Trace[]>([]);

  const simCurrent =
    simSource === null || simSource.directory === project.directory;
  const run = simCurrent ? simRun : IDLE;
  const simRunning = run.state === "running";
  const simStale =
    simCurrent && simSource !== null && simSource.revision !== project.revision;
  const shownAnnotations =
    simCurrent && !simStale && annotationsVisible
      ? opAnnotations
      : NO_ANNOTATIONS;

  const toggleErc = () => {
    setSimOpen(false);
    setErcOpen((open) => !open);
  };

  useEffect(() => {
    if (ercTicket === ercTicketRef.current) {
      return;
    }
    ercTicketRef.current = ercTicket;
    setSimOpen(false);
    setErcOpen((open) => !open);
  }, [ercTicket]);

  const elapsed = () => performance.now() - simStartRef.current;

  const runSimulation = () => {
    if (simRunning) {
      return;
    }
    const { revision, directory } = project;
    simStartRef.current = performance.now();
    setErcOpen(false);
    setSimOpen(true);
    setSimSource({ revision, directory });
    setOpAnnotations(NO_ANNOTATIONS);
    setSimRun({ state: "running", progress: null, elapsedMs: 0 });
    setSimAnnouncement("Simulation started.");
    void startSimulation()
      .then(async (finished) => {
        const annotations =
          finished.scale === null
            ? await fetchOpAnnotations(finished.runId)
            : NO_ANNOTATIONS;
        const elapsedMs = elapsed();
        setOpAnnotations(annotations);
        setSimRun({ state: "finished", elapsedMs, result: finished });
        setSimAnnouncement(
          `Simulation complete in ${formatSeconds(elapsedMs)} seconds.`,
        );
      })
      .catch((error: unknown) => {
        const report = toErrorReport(error);
        if (report.kind === "simCancelled") {
          setSimRun({ state: "cancelled", elapsedMs: elapsed() });
          setSimAnnouncement("Simulation cancelled.");
        } else {
          setSimRun({ state: "failed", error: report });
          setSimAnnouncement(`Simulation failed: ${report.message}`);
        }
      });
  };

  const cancelRun = () => {
    cancelSimulation().catch((error: unknown) =>
      setSimAnnouncement(
        `Couldn't cancel the simulation: ${describeIpcError(error)}`,
      ),
    );
  };

  const simActionsRef = useRef({ runSimulation, cancelRun, simRunning });
  useLayoutEffect(() => {
    simActionsRef.current = { runSimulation, cancelRun, simRunning };
  });

  useEffect(() => {
    if (simulationTicket === simTicketRef.current) {
      return;
    }
    simTicketRef.current = simulationTicket;
    simActionsRef.current.runSimulation();
  }, [simulationTicket]);

  useEffect(() => {
    if (!simRunning) {
      return;
    }
    const timer = window.setInterval(() => {
      const elapsedMs = performance.now() - simStartRef.current;
      setSimRun((current) =>
        current.state === "running" ? { ...current, elapsedMs } : current,
      );
    }, ELAPSED_TICK_MS);
    return () => window.clearInterval(timer);
  }, [simRunning]);

  useEffect(() => {
    const onProgress = ({ percentage }: SimulationProgress) =>
      setSimRun((current) =>
        current.state === "running"
          ? { ...current, progress: percentage }
          : current,
      );
    return subscribe(SIMULATION_PROGRESS_EVENT, onProgress);
  }, []);

  const jumpToSymbol = (reference: string) => {
    const instance = project.instances.find(
      (item) => item.reference?.text === reference,
    );
    if (instance) {
      setFocusRequest([instance.xNm, instance.yNm]);
    }
  };

  const runCheck = () => {
    const { revision, directory } = project;
    setErcRunning(true);
    setErcError(null);
    void runErc()
      .then(
        (violations) => {
          setErcResults({ violations, revision, directory });
          setSelectedViolation(null);
          setErcAnnouncement(ercSummary(violations));
        },
        (error: unknown) => setErcError(describeIpcError(error)),
      )
      .finally(() => setErcRunning(false));
  };

  const runCheckRef = useRef(runCheck);
  useLayoutEffect(() => {
    runCheckRef.current = runCheck;
  });

  useEffect(() => {
    if (ercSettingsVersion === ercSettingsRef.current) {
      return;
    }
    ercSettingsRef.current = ercSettingsVersion;
    if (ercOpen) {
      runCheckRef.current();
    }
  }, [ercSettingsVersion, ercOpen]);

  const jumpTo = (violation: ErcViolation) => {
    setSelectedViolation(violation.id);
    setFocusRequest(violation.position);
  };

  useEffect(() => {
    if (controlsDisabled) {
      return;
    }
    const onKeyDown = (event: KeyboardEvent) => {
      if (
        (event.ctrlKey || event.metaKey) &&
        !event.altKey &&
        !event.shiftKey &&
        event.key.toLowerCase() === "l"
      ) {
        event.preventDefault();
        setLibraryOpen((open) => !open);
      } else if (event.key === "Escape" && simActionsRef.current.simRunning) {
        simActionsRef.current.cancelRun();
      }
    };
    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, [controlsDisabled]);

  return (
    <div className="editor">
      <h1 className="visually-hidden">{project.name}</h1>
      <Toolbar
        history={project.history}
        zoomLabel={zoomLabel}
        disabled={controlsDisabled}
        onUndo={onUndo}
        onRedo={onRedo}
        onOpenProperties={onOpenProperties}
        ercOpen={ercOpen}
        onToggleErc={toggleErc}
        libraryOpen={libraryOpen}
        onToggleLibrary={() => setLibraryOpen((open) => !open)}
        wireMode={wireMode}
        onToggleWireMode={() => setWireMode((active) => !active)}
        noConnectMode={noConnectMode}
        onStartTool={setToolRequest}
        simulationRunning={simRunning}
        onRunSimulation={runSimulation}
        onCancelSimulation={cancelRun}
        onOpenSimulationSettings={onOpenSimulationSettings}
        annotationsVisible={annotationsVisible}
        onToggleAnnotations={() => setAnnotationsVisible((shown) => !shown)}
      />
      <main className="editor__workspace">
        {libraryOpen && (
          <LibraryPanel
            library={library}
            onDownload={onDownloadLibrary}
            onPlaceSymbol={(hit) => setPlacementRequest({ hit, at: null })}
            onSymbolDragStart={setSymbolDrag}
            onSymbolDragEnd={() => setSymbolDrag(null)}
          />
        )}
        <div className="editor__center">
          <SchematicCanvas
            label={`Schematic sheet of ${project.name}`}
            viewport={viewport}
            gridSizeNm={project.gridSizeNm}
            sheet={project.sheet}
            definitions={project.definitions}
            instances={project.instances}
            wires={project.wires}
            junctions={project.junctions}
            labels={project.labels}
            noConnects={project.noConnects}
            wireMode={wireMode}
            onWireModeChange={setWireMode}
            onNoConnectModeChange={setNoConnectMode}
            annotateTicket={annotateTicket}
            library={library}
            onDownloadLibrary={onDownloadLibrary}
            onOpenLibrary={() => setLibraryOpen(true)}
            toolRequest={toolRequest}
            onToolRequestConsumed={() => setToolRequest(null)}
            placementRequest={placementRequest}
            onPlacementRequestConsumed={() => setPlacementRequest(null)}
            symbolDrag={symbolDrag}
            onSymbolDrop={(hit, at) => {
              setSymbolDrag(null);
              setPlacementRequest({ hit, at });
            }}
            ercMarkers={ercMarkers}
            ercStale={ercStale}
            selectedMarkerId={ercOpen ? selectedViolation : null}
            onSelectMarker={setSelectedViolation}
            opAnnotations={shownAnnotations}
            focusRequest={focusRequest}
            onFocusRequestConsumed={() => setFocusRequest(null)}
            onHintChange={setHint}
            onViewportChange={onViewportChange}
            onResize={onCanvasResize}
            onCursorMove={setCursor}
          />
          {ercOpen && (
            <ErcPanel
              violations={results?.violations ?? null}
              stale={ercStale}
              running={ercRunning}
              error={ercError}
              selectedId={selectedViolation}
              announcement={ercAnnouncement}
              onRun={runCheck}
              onOpenSeverities={onOpenErcSeverities}
              onSelect={(violation) => setSelectedViolation(violation.id)}
              onJump={jumpTo}
              onClose={() => setErcOpen(false)}
            />
          )}
          {simOpen && (
            <SimPanel
              run={run}
              stale={simStale}
              announcement={simAnnouncement}
              traces={traces}
              onRemoveTrace={(vector) =>
                setTraces((shown) =>
                  shown.filter((trace) => trace.vector !== vector),
                )
              }
              onRun={runSimulation}
              onCancel={cancelRun}
              onOpenSettings={onOpenSimulationSettings}
              onJumpToSymbol={jumpToSymbol}
              onClose={() => setSimOpen(false)}
            />
          )}
        </div>
      </main>
      <StatusBar
        cursor={cursor}
        gridSizeNm={project.gridSizeNm}
        zoomLabel={zoomLabel}
        hint={hint}
      />
    </div>
  );
}
