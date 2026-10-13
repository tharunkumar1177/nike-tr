/**
 * The schematic canvas: draws placed symbols and drives symbol placement,
 * selection, dragging, transforms, and deletion on top of the generic
 * canvas shell. Every edit is a single core command; the document diff
 * event is the source of truth for what is drawn.
 */
import {
  type KeyboardEvent,
  type MouseEvent,
  type PointerEvent,
  useCallback,
  useEffect,
  useRef,
  useState,
} from "react";
import type { LibraryHit } from "../bindings/LibraryHit";
import type { MirrorAxis } from "../bindings/MirrorAxis";
import type { SymbolInstance } from "../bindings/SymbolInstance";
import type { SymbolPreview } from "../bindings/SymbolPreview";
import {
  deleteSymbols,
  describeIpcError,
  getLibrarySymbol,
  moveSymbols,
  placeSymbol,
  transformSymbols,
} from "../ipc";
import { type GhostState, drawSchematicScene } from "../schematicDraw";
import {
  findInstanceAt,
  normalizeSteps,
  snapToGrid,
  snappedDragDelta,
} from "../symbols";
import {
  type ScreenPoint,
  type SheetDimensions,
  type Size,
  type Viewport,
  pxPerNm,
  screenToWorld,
} from "../viewport";
import { Canvas } from "./Canvas";
import type { StatusHint } from "./StatusBar";

/** A request from the library browser to start placing a symbol. */
export interface PlacementRequest {
  hit: LibraryHit;
  /** The drop point for drags, or `null` to follow the cursor. */
  at: ScreenPoint | null;
}

interface PlacingState {
  libraryId: number;
  name: string;
  /** Quarter turns counterclockwise; normalized when sent or drawn. */
  rotation: number;
  mirror: MirrorAxis | null;
  /** `null` while the definition loads. */
  preview: SymbolPreview | null;
  cursor: ScreenPoint | null;
}

interface DragState {
  uuid: string;
  startX: number;
  startY: number;
  dx: number;
  dy: number;
}

/** Pointer travel, in screen pixels, before a press becomes a drag. */
const DRAG_THRESHOLD_PX = 4;

interface SchematicCanvasProps {
  label: string;
  viewport: Viewport | null;
  gridSizeNm: number;
  sheet: SheetDimensions;
  definitions: { [key in string]: SymbolPreview };
  instances: SymbolInstance[];
  placementRequest: PlacementRequest | null;
  onPlacementRequestConsumed: () => void;
  /** The symbol being dragged from the library browser, if any. */
  symbolDrag: LibraryHit | null;
  onSymbolDrop: (hit: LibraryHit, at: ScreenPoint) => void;
  onHintChange: (hint: StatusHint | null) => void;
  onViewportChange: (update: (viewport: Viewport) => Viewport) => void;
  onResize: (size: Size) => void;
  onCursorMove: (position: ScreenPoint | null) => void;
}

export function SchematicCanvas({
  label,
  viewport,
  gridSizeNm,
  sheet,
  definitions,
  instances,
  placementRequest,
  onPlacementRequestConsumed,
  symbolDrag,
  onSymbolDrop,
  onHintChange,
  onViewportChange,
  onResize,
  onCursorMove,
}: SchematicCanvasProps) {
  const containerRef = useRef<HTMLDivElement>(null);
  const loadTicketRef = useRef(0);
  const sizeRef = useRef<Size>({ width: 0, height: 0 });
  const [placing, setPlacing] = useState<PlacingState | null>(null);
  const [selectedUuid, setSelectedUuid] = useState<string | null>(null);
  const [drag, setDrag] = useState<DragState | null>(null);
  const [hoverUuid, setHoverUuid] = useState<string | null>(null);
  const [announcement, setAnnouncement] = useState("");
  const [commandError, setCommandError] = useState<string | null>(null);

  const beginPlacement = useCallback(
    (hit: LibraryHit, at: ScreenPoint | null) => {
      const ticket = ++loadTicketRef.current;
      setSelectedUuid(null);
      setDrag(null);
      setHoverUuid(null);
      setCommandError(null);
      setAnnouncement(`Placing ${hit.name}`);
      setPlacing({
        libraryId: hit.id,
        name: hit.name,
        rotation: 0,
        mirror: null,
        preview: null,
        cursor: at,
      });
      containerRef.current?.focus();
      const fail = () => {
        if (ticket === loadTicketRef.current) {
          setPlacing(null);
          setCommandError("Failed to load symbol");
        }
      };
      getLibrarySymbol(hit.id).then(
        (symbol) => {
          if (ticket !== loadTicketRef.current) {
            return;
          }
          if (symbol) {
            setPlacing((current) =>
              current && current.libraryId === hit.id
                ? { ...current, preview: symbol.preview }
                : current,
            );
          } else {
            fail();
          }
        },
        () => fail(),
      );
    },
    [],
  );

  const exitPlacement = useCallback(() => {
    // Invalidate any in-flight definition load.
    loadTicketRef.current += 1;
    setPlacing(null);
  }, []);

  useEffect(() => {
    if (placementRequest) {
      onPlacementRequestConsumed();
      beginPlacement(placementRequest.hit, placementRequest.at);
    }
  }, [placementRequest, onPlacementRequestConsumed, beginPlacement]);

  // Drop the selection when the symbol disappears (for example via undo).
  useEffect(() => {
    if (
      selectedUuid !== null &&
      !instances.some((instance) => instance.uuid === selectedUuid)
    ) {
      setSelectedUuid(null);
    }
  }, [instances, selectedUuid]);

  useEffect(() => {
    if (commandError !== null) {
      onHintChange({ text: commandError, error: true });
    } else if (placing) {
      onHintChange({
        text: "Click to place. R: Rotate, X/Y: Mirror. Esc: Cancel",
        error: false,
      });
    } else if (selectedUuid !== null) {
      onHintChange({
        text: "Symbol selected. Drag to move. R/X/Y: Transform. Del: Remove",
        error: false,
      });
    } else {
      onHintChange(null);
    }
  }, [commandError, placing, selectedUuid, onHintChange]);

  const runCommand = (command: Promise<void>, announce: string | null) => {
    void command.then(
      () => {
        if (announce !== null) {
          setAnnouncement(announce);
        }
      },
      (error: unknown) => setCommandError(describeIpcError(error)),
    );
  };

  const placeAt = (world: ScreenPoint) => {
    if (!placing || !placing.preview) {
      return;
    }
    runCommand(
      placeSymbol({
        libraryId: placing.libraryId,
        xNm: snapToGrid(world.x, gridSizeNm),
        yNm: snapToGrid(world.y, gridSizeNm),
        rotation: normalizeSteps(placing.rotation),
        mirror: placing.mirror,
      }),
      "Symbol placed",
    );
  };

  const handlePointerDown = (
    world: ScreenPoint,
    event: PointerEvent<HTMLDivElement>,
  ) => {
    if (event.button !== 0) {
      return;
    }
    setCommandError(null);
    if (placing) {
      placeAt(world);
      return;
    }
    const hit = findInstanceAt(instances, definitions, world);
    if (hit) {
      if (hit.uuid !== selectedUuid) {
        setSelectedUuid(hit.uuid);
        setAnnouncement("Symbol selected");
      }
      const target = event.currentTarget;
      try {
        target.setPointerCapture(event.pointerId);
      } catch {
        // Environments without active pointers (tests): dragging still works.
      }
      setDrag({
        uuid: hit.uuid,
        startX: world.x,
        startY: world.y,
        dx: 0,
        dy: 0,
      });
    } else if (selectedUuid !== null) {
      setSelectedUuid(null);
    }
  };

  const handlePointerUp = (
    _world: ScreenPoint,
    event: PointerEvent<HTMLDivElement>,
  ) => {
    if (!drag) {
      return;
    }
    setDrag(null);
    if (event.type === "pointercancel") {
      return;
    }
    if (drag.dx !== 0 || drag.dy !== 0) {
      runCommand(moveSymbols([drag.uuid], drag.dx, drag.dy), null);
    }
  };

  const handleCursorMove = (position: ScreenPoint | null) => {
    if (placing) {
      setPlacing((current) =>
        current ? { ...current, cursor: position } : current,
      );
    }
    if (drag && position) {
      let delta = snappedDragDelta(
        { x: drag.startX, y: drag.startY },
        position,
        gridSizeNm,
      );
      if (viewport) {
        const movedPx =
          Math.hypot(position.x - drag.startX, position.y - drag.startY) *
          pxPerNm(viewport.zoom);
        if (movedPx < DRAG_THRESHOLD_PX) {
          delta = { x: 0, y: 0 };
        }
      }
      if (delta.x !== drag.dx || delta.y !== drag.dy) {
        setDrag({ ...drag, dx: delta.x, dy: delta.y });
      }
    }
    if (!placing && !drag) {
      const hover = position
        ? (findInstanceAt(instances, definitions, position)?.uuid ?? null)
        : null;
      if (hover !== hoverUuid) {
        setHoverUuid(hover);
      }
    }
    onCursorMove(position);
  };

  const handleKeyDown = (event: KeyboardEvent<HTMLDivElement>) => {
    if (event.ctrlKey || event.metaKey || event.altKey) {
      return;
    }
    const key = event.key.toLowerCase();
    if (placing) {
      if (key === "escape") {
        event.preventDefault();
        exitPlacement();
      } else if (key === "enter") {
        event.preventDefault();
        const fallback =
          viewport && sizeRef.current.width > 0
            ? screenToWorld(
                viewport,
                sizeRef.current.width / 2,
                sizeRef.current.height / 2,
              )
            : null;
        const at = placing.cursor ?? fallback;
        if (at) {
          placeAt(at);
        }
      } else if (key === "r") {
        event.preventDefault();
        setPlacing((current) =>
          current ? { ...current, rotation: current.rotation - 1 } : current,
        );
      } else if (key === "x" || key === "y") {
        event.preventDefault();
        setPlacing((current) =>
          current
            ? { ...current, mirror: current.mirror === key ? null : key }
            : current,
        );
      }
      return;
    }
    if (selectedUuid === null) {
      return;
    }
    const uuids = [selectedUuid];
    if (key === "r") {
      event.preventDefault();
      // R turns clockwise on screen: three counterclockwise steps.
      runCommand(transformSymbols(uuids, { type: "rotate", steps: 3 }), null);
    } else if (key === "x" || key === "y") {
      event.preventDefault();
      runCommand(transformSymbols(uuids, { type: "mirror", axis: key }), null);
    } else if (key === "delete" || key === "backspace") {
      event.preventDefault();
      setSelectedUuid(null);
      runCommand(deleteSymbols(uuids), "Symbol deleted");
    }
  };

  const handleContextMenu = (event: MouseEvent<HTMLDivElement>) => {
    if (placing) {
      event.preventDefault();
      exitPlacement();
    }
  };

  const handleResize = (size: Size) => {
    sizeRef.current = size;
    onResize(size);
  };

  const drawScene = useCallback(
    (context: CanvasRenderingContext2D, _size: Size, view: Viewport) => {
      const ghost: GhostState | null =
        placing && placing.cursor
          ? {
              preview: placing.preview,
              rotationSteps: placing.rotation,
              mirror: placing.mirror,
              cursor: placing.cursor,
            }
          : null;
      drawSchematicScene(context, view, {
        definitions,
        instances,
        selectedUuid,
        drag,
        ghost,
        gridSizeNm,
      });
    },
    [definitions, instances, selectedUuid, drag, placing, gridSizeNm],
  );

  const sceneCursor = drag
    ? "grabbing"
    : hoverUuid !== null && !placing
      ? "pointer"
      : null;

  return (
    <>
      <Canvas
        label={label}
        viewport={viewport}
        gridSizeNm={gridSizeNm}
        sheet={sheet}
        containerRef={containerRef}
        drawScene={drawScene}
        sceneCursor={sceneCursor}
        interaction={{
          onPointerDown: handlePointerDown,
          onPointerUp: handlePointerUp,
          onKeyDown: handleKeyDown,
          onContextMenu: handleContextMenu,
        }}
        dropActive={symbolDrag !== null}
        onSceneDrop={(world) => {
          if (symbolDrag) {
            onSymbolDrop(symbolDrag, world);
          }
        }}
        onViewportChange={onViewportChange}
        onResize={handleResize}
        onCursorMove={handleCursorMove}
      />
      <div className="visually-hidden" aria-live="polite">
        {announcement}
      </div>
      <ul className="visually-hidden" aria-label="Placed symbols">
        {instances.map((instance) => (
          <li key={instance.uuid}>
            {instance.reference
              ? `${instance.reference.text} ${instance.libId}`
              : instance.libId}
          </li>
        ))}
      </ul>
    </>
  );
}
