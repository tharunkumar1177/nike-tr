import { useEffect, useState } from "react";
import type { LibraryHit } from "../bindings/LibraryHit";
import type { ProjectSnapshot } from "../bindings/ProjectSnapshot";
import type { LibraryStatusState } from "../useLibraryStatus";
import {
  type ScreenPoint,
  type Size,
  type Viewport,
  formatZoom,
} from "../viewport";
import { LibraryPanel } from "./LibraryPanel";
import { type PlacementRequest, SchematicCanvas } from "./SchematicCanvas";
import { StatusBar, type StatusHint } from "./StatusBar";
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
  const [hint, setHint] = useState<StatusHint | null>(null);
  const [libraryOpen, setLibraryOpen] = useState(false);
  const [placementRequest, setPlacementRequest] =
    useState<PlacementRequest | null>(null);
  const [symbolDrag, setSymbolDrag] = useState<LibraryHit | null>(null);
  const [wireMode, setWireMode] = useState(false);
  const zoomLabel = viewport ? formatZoom(viewport.zoom) : "—";

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
        wireMode={wireMode}
        onToggleWireMode={() => setWireMode((active) => !active)}
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
          wireMode={wireMode}
          onWireModeChange={setWireMode}
          library={library}
          onDownloadLibrary={onDownloadLibrary}
          onOpenLibrary={() => setLibraryOpen(true)}
          placementRequest={placementRequest}
          onPlacementRequestConsumed={() => setPlacementRequest(null)}
          symbolDrag={symbolDrag}
          onSymbolDrop={(hit, at) => {
            setSymbolDrag(null);
            setPlacementRequest({ hit, at });
          }}
          onHintChange={setHint}
          onViewportChange={onViewportChange}
          onResize={onCanvasResize}
          onCursorMove={setCursor}
        />
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
