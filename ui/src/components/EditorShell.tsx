import { useEffect, useState } from "react";
import type { ProjectSnapshot } from "../bindings/ProjectSnapshot";
import type { LibraryStatusState } from "../useLibraryStatus";
import {
  type ScreenPoint,
  type Size,
  type Viewport,
  formatZoom,
} from "../viewport";
import { Canvas } from "./Canvas";
import { LibraryPanel } from "./LibraryPanel";
import { StatusBar } from "./StatusBar";
import { Toolbar } from "./Toolbar";

interface EditorShellProps {
  project: ProjectSnapshot;
  viewport: Viewport | null;
  controlsDisabled: boolean;
  library: LibraryStatusState;
  onViewportChange: (update: (viewport: Viewport) => Viewport) => void;
  onCanvasResize: (size: Size) => void;
  onUndo: () => void;
  onRedo: () => void;
  onOpenProperties: () => void;
  onDownloadLibrary: () => void;
}

export function EditorShell({
  project,
  viewport,
  controlsDisabled,
  library,
  onViewportChange,
  onCanvasResize,
  onUndo,
  onRedo,
  onOpenProperties,
  onDownloadLibrary,
}: EditorShellProps) {
  const [cursor, setCursor] = useState<ScreenPoint | null>(null);
  const [libraryOpen, setLibraryOpen] = useState(false);
  const zoomLabel = viewport ? formatZoom(viewport.zoom) : "\u2014";

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
        libraryOpen={libraryOpen}
        onToggleLibrary={() => setLibraryOpen((open) => !open)}
      />
      <main className="editor__workspace">
        {libraryOpen && (
          <LibraryPanel library={library} onDownload={onDownloadLibrary} />
        )}
        <Canvas
          label={`Schematic sheet of ${project.name}`}
          viewport={viewport}
          gridSizeNm={project.gridSizeNm}
          sheet={project.sheet}
          onViewportChange={onViewportChange}
          onResize={onCanvasResize}
          onCursorMove={setCursor}
        />
      </main>
      <StatusBar
        cursor={cursor}
        gridSizeNm={project.gridSizeNm}
        zoomLabel={zoomLabel}
      />
    </div>
  );
}
