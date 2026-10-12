import { useState } from "react";
import type { ProjectSnapshot } from "../bindings/ProjectSnapshot";
import {
  type ScreenPoint,
  type Size,
  type Viewport,
  formatZoom,
} from "../viewport";
import { Canvas } from "./Canvas";
import { StatusBar } from "./StatusBar";
import { Toolbar } from "./Toolbar";

interface EditorShellProps {
  project: ProjectSnapshot;
  viewport: Viewport | null;
  controlsDisabled: boolean;
  onViewportChange: (update: (viewport: Viewport) => Viewport) => void;
  onCanvasResize: (size: Size) => void;
  onUndo: () => void;
  onRedo: () => void;
  onOpenProperties: () => void;
}

export function EditorShell({
  project,
  viewport,
  controlsDisabled,
  onViewportChange,
  onCanvasResize,
  onUndo,
  onRedo,
  onOpenProperties,
}: EditorShellProps) {
  const [cursor, setCursor] = useState<ScreenPoint | null>(null);
  const zoomLabel = viewport ? formatZoom(viewport.zoom) : "\u2014";

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
      />
      <main className="editor__workspace">
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
