import { open } from "@tauri-apps/plugin-dialog";
import {
  type ReactNode,
  useCallback,
  useEffect,
  useLayoutEffect,
  useRef,
  useState,
} from "react";
import type { AnalysisSettings } from "./bindings/AnalysisSettings";
import type { AppInfo } from "./bindings/AppInfo";
import type { BoardDesignRules } from "./bindings/BoardDesignRules";
import type { ErcRuleSeverity } from "./bindings/ErcRuleSeverity";
import type { OutlineGraphic } from "./bindings/OutlineGraphic";
import type { ProjectProperties } from "./bindings/ProjectProperties";
import type { ProjectSnapshot } from "./bindings/ProjectSnapshot";
import type { RecentProject } from "./bindings/RecentProject";
import type { Xy } from "./bindings/Xy";
import { fitBoard } from "./board";
import { BoardOutlineDialog } from "./components/BoardOutlineDialog";
import { BoardUpdateDialog } from "./components/BoardUpdateDialog";
import { DesignRulesDialog } from "./components/DesignRulesDialog";
import { EditorShell } from "./components/EditorShell";
import { ErcSeveritiesDialog } from "./components/ErcSeveritiesDialog";
import { ErrorDialog } from "./components/ErrorDialog";
import { LayerStackupDialog } from "./components/LayerStackupDialog";
import { LibraryDialog } from "./components/LibraryDialog";
import { ProjectLocationDialog } from "./components/ProjectLocationDialog";
import { ProjectPropertiesDialog } from "./components/ProjectPropertiesDialog";
import { SimulationSettingsDialog } from "./components/SimulationSettingsDialog";
import type { EditorView } from "./components/Toolbar";
import { UnsavedChangesDialog } from "./components/UnsavedChangesDialog";
import { type RecentState, WelcomeScreen } from "./components/WelcomeScreen";
import {
  CLOSE_REQUESTED_EVENT,
  type ErrorReport,
  MENU_EVENT,
  OPEN_RECENT_PREFIX,
  cancelSimulation,
  closeProject,
  closeWindow,
  createProject,
  editAnalysisSettings,
  editBoardOutline,
  editDesignRules,
  editErcSeverities,
  editProjectProperties,
  editStackup,
  getAppInfo,
  listRecentProjects,
  openProject,
  redo,
  removeRecentProject,
  saveProject,
  saveProjectAs,
  subscribe,
  toErrorReport,
  undo,
} from "./ipc";
import { parentDirectory } from "./paths";
import { useLibraryStatus } from "./useLibraryStatus";
import { useProjectDocument } from "./useProjectDocument";
import {
  type Size,
  type Viewport,
  ZOOM_STEP,
  fitToSheet,
  zoomAt,
  zoomToLevel,
} from "./viewport";

/** What to do once the user has dealt with unsaved changes. */
type LeaveAction =
  | { type: "new" }
  | { type: "open"; path: string | null }
  | { type: "close" }
  | { type: "exit" };

type ModalState =
  | { type: "newProject" }
  | { type: "saveAs" }
  | { type: "properties" }
  | { type: "ercSeverities" }
  | { type: "simulationSettings" }
  | { type: "boardUpdate" }
  | { type: "boardOutline" }
  | { type: "layerStackup" }
  | { type: "designRules" }
  | { type: "unsaved"; next: LeaveAction }
  | {
      type: "error";
      title: string;
      error: ErrorReport;
      removeRecentPath?: string;
    };

export function App() {
  const { project, ercSettingsVersion, loadError, setProject, resync } =
    useProjectDocument();
  const [modal, setModal] = useState<ModalState | null>(null);
  const [busy, setBusy] = useState<string | null>(null);
  const [recent, setRecent] = useState<RecentState>({ status: "loading" });
  const [appInfo, setAppInfo] = useState<AppInfo | null>(null);
  const [viewport, setViewport] = useState<Viewport | null>(null);
  const [view, setView] = useState<EditorView>("schematic");
  const [boardViewport, setBoardViewport] = useState<Viewport | null>(null);
  /** Refit the board once footprints arrive, after updating an empty board. */
  const [boardFitPending, setBoardFitPending] = useState(false);
  const [boardNotice, setBoardNotice] = useState<string | null>(null);
  /** The outline an edit replaced; the board refits once it changes. */
  const [outlineFitFrom, setOutlineFitFrom] = useState<OutlineGraphic[] | null>(
    null,
  );
  const [canvasSize, setCanvasSize] = useState<Size>({ width: 0, height: 0 });
  const [library, adoptLibraryStatus] = useLibraryStatus();
  const [libraryDialogOpen, setLibraryDialogOpen] = useState(false);
  const [annotateTicket, setAnnotateTicket] = useState(0);
  const [ercTicket, setErcTicket] = useState(0);
  const [simulationTicket, setSimulationTicket] = useState(0);
  const libraryOffered = useRef(false);
  const closeLibraryDialog = useCallback(() => setLibraryDialogOpen(false), []);

  useEffect(() => {
    // The download is offered once per launch until the user answers.
    if (libraryOffered.current || library.status !== "ready") {
      return;
    }
    libraryOffered.current = true;
    const status = library.library;
    if (status.state === "missing" && status.consent === "unasked") {
      setLibraryDialogOpen(true);
    }
  }, [library]);

  useEffect(() => {
    // The version line on the welcome screen is omitted if this fails.
    getAppInfo().then(setAppInfo, () => setAppInfo(null));
  }, []);

  const refreshRecent = useCallback(async () => {
    try {
      setRecent({ status: "ready", projects: await listRecentProjects() });
    } catch (error) {
      setRecent({ status: "error", error: toErrorReport(error) });
    }
  }, []);

  useEffect(() => {
    if (project === null) {
      void refreshRecent();
    }
  }, [project, refreshRecent]);

  useEffect(() => {
    if (
      project &&
      viewport === null &&
      canvasSize.width > 0 &&
      canvasSize.height > 0
    ) {
      setViewport(fitToSheet(canvasSize, project.sheet));
    }
  }, [project, viewport, canvasSize]);

  useEffect(() => {
    if (
      !project ||
      view !== "board" ||
      canvasSize.width <= 0 ||
      canvasSize.height <= 0
    ) {
      return;
    }
    const refit = boardFitPending && project.footprints.length > 0;
    if (boardViewport === null || refit) {
      setBoardViewport(
        fitBoard(canvasSize, project.footprints, project.boardOutline),
      );
      setBoardFitPending(false);
    }
  }, [project, view, boardViewport, boardFitPending, canvasSize]);

  useEffect(() => {
    if (
      !project ||
      outlineFitFrom === null ||
      project.boardOutline === outlineFitFrom ||
      canvasSize.width <= 0 ||
      canvasSize.height <= 0
    ) {
      return;
    }
    setBoardViewport(
      fitBoard(canvasSize, project.footprints, project.boardOutline),
    );
    setOutlineFitFrom(null);
  }, [project, outlineFitFrom, canvasSize]);

  const updateViewport = useCallback(
    (update: (current: Viewport) => Viewport) => {
      setViewport((current) => (current ? update(current) : current));
    },
    [],
  );

  const updateBoardViewport = useCallback(
    (update: (current: Viewport) => Viewport) => {
      setBoardViewport((current) => (current ? update(current) : current));
    },
    [],
  );

  /** Updates the viewport of the view on screen. */
  const updateShownViewport =
    view === "board" ? updateBoardViewport : updateViewport;

  async function withBusy<T>(label: string, task: () => Promise<T>) {
    setBusy(label);
    try {
      return await task();
    } finally {
      setBusy(null);
    }
  }

  function showError(title: string, error: unknown) {
    setModal({ type: "error", title, error: toErrorReport(error) });
  }

  function adoptProject(snapshot: ProjectSnapshot) {
    setViewport(null);
    setBoardViewport(null);
    setBoardFitPending(false);
    setOutlineFitFrom(null);
    setBoardNotice(null);
    setView("schematic");
    setProject(snapshot);
    setModal(null);
  }

  async function openAt(path: string, fromRecent: boolean) {
    try {
      const snapshot = await withBusy("Opening project...", () =>
        openProject(path),
      );
      adoptProject(snapshot);
    } catch (error) {
      setModal({
        type: "error",
        title: "Couldn't open project",
        error: toErrorReport(error),
        ...(fromRecent ? { removeRecentPath: path } : {}),
      });
    }
  }

  async function pickAndOpen() {
    let path: string | null;
    try {
      path = await open({ directory: true, title: "Open Project" });
    } catch (error) {
      showError("Couldn't open project", error);
      return;
    }
    if (path !== null) {
      await openAt(path, false);
    }
  }

  async function perform(action: LeaveAction) {
    switch (action.type) {
      case "new":
        setModal({ type: "newProject" });
        return;
      case "open":
        setModal(null);
        if (action.path === null) {
          await pickAndOpen();
        } else {
          await openAt(action.path, true);
        }
        return;
      case "close":
        try {
          await closeProject();
          setModal(null);
          setProject(null);
        } catch (error) {
          showError("Couldn't close project", error);
        }
        return;
      case "exit":
        try {
          await closeWindow();
        } catch (error) {
          showError("Couldn't close Tron", error);
        }
        return;
    }
  }

  function requestLeave(action: LeaveAction) {
    if (project?.history.dirty) {
      setModal({ type: "unsaved", next: action });
    } else {
      void perform(action);
    }
  }

  async function save(): Promise<boolean> {
    try {
      await withBusy("Saving...", saveProject);
      return true;
    } catch (error) {
      showError("Couldn't save project", error);
      return false;
    }
  }

  async function saveThenContinue(next: LeaveAction) {
    setModal(null);
    if (await save()) {
      await perform(next);
    }
  }

  async function runHistory(action: () => Promise<void>, title: string) {
    try {
      await action();
    } catch (error) {
      const report = toErrorReport(error);
      if (report.kind !== "nothingToUndo" && report.kind !== "nothingToRedo") {
        setModal({ type: "error", title, error: report });
      }
    }
  }

  async function submitNewProject(
    location: string,
    name: string,
  ): Promise<ErrorReport | null> {
    try {
      const snapshot = await withBusy("Creating project...", () =>
        createProject(location, name),
      );
      adoptProject(snapshot);
      return null;
    } catch (error) {
      return toErrorReport(error);
    }
  }

  async function submitSaveAs(
    location: string,
    name: string,
  ): Promise<ErrorReport | null> {
    try {
      const snapshot = await withBusy("Saving...", () =>
        saveProjectAs(location, name),
      );
      setProject(snapshot);
      setModal(null);
      return null;
    } catch (error) {
      return toErrorReport(error);
    }
  }

  async function applyProperties(
    properties: ProjectProperties,
  ): Promise<ErrorReport | null> {
    try {
      await editProjectProperties(properties);
      setModal(null);
      return null;
    } catch (error) {
      return toErrorReport(error);
    }
  }

  async function applyErcSeverities(
    severities: Record<string, ErcRuleSeverity>,
  ): Promise<ErrorReport | null> {
    try {
      await editErcSeverities(severities);
      setModal(null);
      return null;
    } catch (error) {
      return toErrorReport(error);
    }
  }

  async function applySimulationSettings(
    analysis: AnalysisSettings,
  ): Promise<ErrorReport | null> {
    try {
      await editAnalysisSettings(analysis);
      setModal(null);
      return null;
    } catch (error) {
      return toErrorReport(error);
    }
  }

  async function removeRecent(path: string) {
    try {
      const projects = await removeRecentProject(path);
      setRecent({ status: "ready", projects });
      setModal(null);
    } catch (error) {
      showError("Couldn't update Recent Projects", error);
    }
  }

  function openRecent(entry: RecentProject) {
    if (entry.exists) {
      void openAt(entry.path, true);
      return;
    }
    setModal({
      type: "error",
      title: "Project not found",
      error: {
        kind: null,
        message: `${entry.path} no longer exists.`,
        detail: null,
      },
      removeRecentPath: entry.path,
    });
  }

  function zoomBy(factor: number) {
    updateShownViewport((current) =>
      zoomAt(current, factor, canvasSize.width / 2, canvasSize.height / 2),
    );
  }

  function zoomToFit() {
    if (!project || canvasSize.width <= 0 || canvasSize.height <= 0) {
      return;
    }
    if (view === "board") {
      setBoardViewport(
        fitBoard(canvasSize, project.footprints, project.boardOutline),
      );
    } else {
      setViewport(fitToSheet(canvasSize, project.sheet));
    }
  }

  /** Closes the update dialog and shows the board it changed. */
  function boardUpdated() {
    if (project && project.footprints.length === 0) {
      setBoardFitPending(true);
    }
    setModal(null);
    setView("board");
    setBoardNotice("Board updated from schematic");
  }

  /** Replaces the board outline, then shows the board fitted to it. */
  async function applyOutline(points: Xy[]): Promise<ErrorReport | null> {
    const replaced = project ? project.boardOutline : null;
    try {
      await editBoardOutline(points);
    } catch (error) {
      return toErrorReport(error);
    }
    setOutlineFitFrom(replaced);
    setModal(null);
    setView("board");
    setBoardNotice("Board outline updated.");
    return null;
  }

  /** Changes the copper layer count, then shows the board. */
  async function applyStackup(
    copperLayers: number,
  ): Promise<ErrorReport | null> {
    try {
      await editStackup(copperLayers);
    } catch (error) {
      return toErrorReport(error);
    }
    setModal(null);
    setView("board");
    setBoardNotice("Layer stackup updated.");
    return null;
  }

  /** Stores new design rules, then shows the board. */
  async function applyDesignRules(
    rules: BoardDesignRules,
  ): Promise<ErrorReport | null> {
    try {
      await editDesignRules(rules);
    } catch (error) {
      return toErrorReport(error);
    }
    setModal(null);
    setView("board");
    setBoardNotice("Design rules updated.");
    return null;
  }

  function handleMenu(id: string) {
    if (busy !== null || modal !== null || libraryDialogOpen) {
      return;
    }
    if (id.startsWith(OPEN_RECENT_PREFIX)) {
      const path = id.slice(OPEN_RECENT_PREFIX.length);
      requestLeave({ type: "open", path });
      return;
    }
    switch (id) {
      case "new-project":
        requestLeave({ type: "new" });
        return;
      case "open-project":
        requestLeave({ type: "open", path: null });
        return;
      case "exit":
        requestLeave({ type: "exit" });
        return;
    }
    if (!project) {
      return;
    }
    switch (id) {
      case "save":
        void save();
        return;
      case "save-as":
        setModal({ type: "saveAs" });
        return;
      case "project-properties":
        setModal({ type: "properties" });
        return;
      case "close-project":
        requestLeave({ type: "close" });
        return;
      case "undo":
        void runHistory(undo, "Couldn't undo");
        return;
      case "redo":
        void runHistory(redo, "Couldn't redo");
        return;
      case "zoom-in":
        zoomBy(ZOOM_STEP);
        return;
      case "zoom-out":
        zoomBy(1 / ZOOM_STEP);
        return;
      case "zoom-fit":
        zoomToFit();
        return;
      case "zoom-actual":
        updateShownViewport((current) => zoomToLevel(current, 1, canvasSize));
        return;
      case "switch-view":
        setView((current) => (current === "board" ? "schematic" : "board"));
        return;
      case "update-board":
        setModal({ type: "boardUpdate" });
        return;
      case "board-outline":
        setModal({ type: "boardOutline" });
        return;
      case "layer-stackup":
        setModal({ type: "layerStackup" });
        return;
      case "design-rules":
        setModal({ type: "designRules" });
        return;
      case "annotate-schematic":
        setAnnotateTicket((ticket) => ticket + 1);
        return;
      case "electrical-rules-checker":
        setErcTicket((ticket) => ticket + 1);
        return;
      case "erc-rule-severities":
        setModal({ type: "ercSeverities" });
        return;
      case "run-simulation":
        setSimulationTicket((ticket) => ticket + 1);
        return;
      case "cancel-simulation":
        cancelSimulation().catch((error: unknown) =>
          showError("Couldn't cancel the simulation", error),
        );
        return;
      case "simulation-settings":
        setModal({ type: "simulationSettings" });
        return;
    }
  }

  function handleCloseRequested() {
    if (busy === null) {
      setModal({ type: "unsaved", next: { type: "exit" } });
    }
  }

  const handlersRef = useRef({ handleMenu, handleCloseRequested });
  useLayoutEffect(() => {
    handlersRef.current = { handleMenu, handleCloseRequested };
  });

  useEffect(() => {
    return subscribe<string>(MENU_EVENT, (id) =>
      handlersRef.current.handleMenu(id),
    );
  }, []);

  useEffect(() => {
    return subscribe<null>(CLOSE_REQUESTED_EVENT, () =>
      handlersRef.current.handleCloseRequested(),
    );
  }, []);

  function renderModal(current: ModalState): ReactNode {
    const dismiss = () => setModal(null);
    switch (current.type) {
      case "newProject": {
        const latest =
          recent.status === "ready" ? recent.projects[0] : undefined;
        return (
          <ProjectLocationDialog
            mode="new"
            initialName=""
            initialLocation={latest ? parentDirectory(latest.path) : ""}
            onSubmit={submitNewProject}
            onCancel={dismiss}
          />
        );
      }
      case "saveAs":
        return project ? (
          <ProjectLocationDialog
            mode="saveAs"
            initialName={`${project.name} copy`}
            initialLocation={parentDirectory(project.directory)}
            onSubmit={submitSaveAs}
            onCancel={dismiss}
          />
        ) : null;
      case "properties":
        return project ? (
          <ProjectPropertiesDialog
            project={project}
            onApply={applyProperties}
            onCancel={dismiss}
          />
        ) : null;
      case "ercSeverities":
        return project ? (
          <ErcSeveritiesDialog
            onApply={applyErcSeverities}
            onCancel={dismiss}
          />
        ) : null;
      case "simulationSettings":
        return project ? (
          <SimulationSettingsDialog
            settings={project.simulationSettings}
            onApply={applySimulationSettings}
            onCancel={dismiss}
          />
        ) : null;
      case "boardUpdate":
        return project ? (
          <BoardUpdateDialog
            onApplied={boardUpdated}
            onDownloadLibrary={() => setLibraryDialogOpen(true)}
            onClose={dismiss}
          />
        ) : null;
      case "boardOutline":
        return project ? (
          <BoardOutlineDialog
            outline={project.boardOutline}
            footprints={project.footprints}
            onApply={applyOutline}
            onCancel={dismiss}
          />
        ) : null;
      case "layerStackup":
        return project ? (
          <LayerStackupDialog
            stackup={project.stackup}
            onApply={applyStackup}
            onCancel={dismiss}
          />
        ) : null;
      case "designRules":
        return project ? (
          <DesignRulesDialog
            rules={project.designRules}
            onApply={applyDesignRules}
            onCancel={dismiss}
          />
        ) : null;
      case "unsaved":
        return (
          <UnsavedChangesDialog
            projectName={project?.name ?? "this project"}
            onSave={() => void saveThenContinue(current.next)}
            onDiscard={() => {
              setModal(null);
              void perform(current.next);
            }}
            onCancel={dismiss}
          />
        );
      case "error": {
        const path = current.removeRecentPath;
        return (
          <ErrorDialog
            title={current.title}
            error={current.error}
            onClose={dismiss}
            onRemoveRecent={
              path === undefined ? undefined : () => void removeRecent(path)
            }
          />
        );
      }
    }
  }

  let content: ReactNode;
  if (project === undefined) {
    content = loadError ? (
      <main className="app-status">
        <div className="app-status__content" role="alert">
          <h1 className="app-status__title app-status__title--error">
            Couldn&apos;t connect to the Tron core.
          </h1>
          <p className="app-status__secondary">{loadError.message}</p>
          <button
            type="button"
            className="button"
            onClick={() => void resync()}
          >
            Retry
          </button>
        </div>
      </main>
    ) : (
      <main className="app-status">
        <p className="app-status__secondary">Loading...</p>
      </main>
    );
  } else if (project === null) {
    content = (
      <WelcomeScreen
        appInfo={appInfo}
        recent={recent}
        disabled={busy !== null}
        onNewProject={() => requestLeave({ type: "new" })}
        onOpenProject={() => requestLeave({ type: "open", path: null })}
        onOpenRecent={openRecent}
        onRetryRecent={() => void refreshRecent()}
      />
    );
  } else {
    content = (
      <EditorShell
        project={project}
        view={view}
        onViewChange={setView}
        viewport={viewport}
        boardViewport={boardViewport}
        onBoardViewportChange={updateBoardViewport}
        onUpdateBoard={() => setModal({ type: "boardUpdate" })}
        onEditOutline={() => setModal({ type: "boardOutline" })}
        onEditStackup={() => setModal({ type: "layerStackup" })}
        onEditDesignRules={() => setModal({ type: "designRules" })}
        boardNotice={boardNotice}
        onBoardNoticeConsumed={() => setBoardNotice(null)}
        controlsDisabled={modal !== null || busy !== null || libraryDialogOpen}
        library={library}
        annotateTicket={annotateTicket}
        ercTicket={ercTicket}
        ercSettingsVersion={ercSettingsVersion}
        simulationTicket={simulationTicket}
        onOpenSimulationSettings={() =>
          setModal({ type: "simulationSettings" })
        }
        onViewportChange={updateViewport}
        onCanvasResize={setCanvasSize}
        onUndo={() => void runHistory(undo, "Couldn't undo")}
        onRedo={() => void runHistory(redo, "Couldn't redo")}
        onOpenProperties={() => setModal({ type: "properties" })}
        onOpenErcSeverities={() => setModal({ type: "ercSeverities" })}
        onDownloadLibrary={() => setLibraryDialogOpen(true)}
      />
    );
  }

  return (
    <>
      {content}
      {modal && renderModal(modal)}
      {!modal && libraryDialogOpen && library.status === "ready" && (
        <LibraryDialog
          library={library.library}
          onStatus={adoptLibraryStatus}
          onClose={closeLibraryDialog}
        />
      )}
      {busy !== null && (
        <div className="busy-overlay" role="status">
          <p className="busy-overlay__label">{busy}</p>
        </div>
      )}
    </>
  );
}
