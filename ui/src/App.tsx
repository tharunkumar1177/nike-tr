import { open } from "@tauri-apps/plugin-dialog";
import {
  type ReactNode,
  useCallback,
  useEffect,
  useLayoutEffect,
  useRef,
  useState,
} from "react";
import type { AppInfo } from "./bindings/AppInfo";
import type { ProjectProperties } from "./bindings/ProjectProperties";
import type { ProjectSnapshot } from "./bindings/ProjectSnapshot";
import type { RecentProject } from "./bindings/RecentProject";
import { EditorShell } from "./components/EditorShell";
import { ErrorDialog } from "./components/ErrorDialog";
import { ProjectLocationDialog } from "./components/ProjectLocationDialog";
import { ProjectPropertiesDialog } from "./components/ProjectPropertiesDialog";
import { UnsavedChangesDialog } from "./components/UnsavedChangesDialog";
import { type RecentState, WelcomeScreen } from "./components/WelcomeScreen";
import {
  CLOSE_REQUESTED_EVENT,
  type ErrorReport,
  MENU_EVENT,
  OPEN_RECENT_PREFIX,
  closeProject,
  closeWindow,
  createProject,
  editProjectProperties,
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
  | { type: "unsaved"; next: LeaveAction }
  | {
      type: "error";
      title: string;
      error: ErrorReport;
      removeRecentPath?: string;
    };

export function App() {
  const { project, loadError, setProject, resync } = useProjectDocument();
  const [modal, setModal] = useState<ModalState | null>(null);
  const [busy, setBusy] = useState<string | null>(null);
  const [recent, setRecent] = useState<RecentState>({ status: "loading" });
  const [appInfo, setAppInfo] = useState<AppInfo | null>(null);
  const [viewport, setViewport] = useState<Viewport | null>(null);
  const [canvasSize, setCanvasSize] = useState<Size>({ width: 0, height: 0 });

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

  const updateViewport = useCallback(
    (update: (current: Viewport) => Viewport) => {
      setViewport((current) => (current ? update(current) : current));
    },
    [],
  );

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
    updateViewport((current) =>
      zoomAt(current, factor, canvasSize.width / 2, canvasSize.height / 2),
    );
  }

  function zoomToFit() {
    if (project && canvasSize.width > 0 && canvasSize.height > 0) {
      setViewport(fitToSheet(canvasSize, project.sheet));
    }
  }

  function handleMenu(id: string) {
    if (busy !== null || modal !== null) {
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
        updateViewport((current) => zoomToLevel(current, 1, canvasSize));
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
        viewport={viewport}
        controlsDisabled={modal !== null || busy !== null}
        onViewportChange={updateViewport}
        onCanvasResize={setCanvasSize}
        onUndo={() => void runHistory(undo, "Couldn't undo")}
        onRedo={() => void runHistory(redo, "Couldn't redo")}
        onOpenProperties={() => setModal({ type: "properties" })}
      />
    );
  }

  return (
    <>
      {content}
      {modal && renderModal(modal)}
      {busy !== null && (
        <div className="busy-overlay" role="status">
          <p className="busy-overlay__label">{busy}</p>
        </div>
      )}
    </>
  );
}
