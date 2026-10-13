/**
 * The schematic canvas: draws symbols, wires, junctions, and net labels, and
 * drives symbol placement, wiring, label and power symbol placement,
 * selection, dragging, transforms, and deletion on top of the generic canvas
 * shell. Every edit is a single core command; the document diff event is the
 * source of truth for what is drawn.
 */
import {
  type KeyboardEvent,
  type MouseEvent,
  type PointerEvent,
  useCallback,
  useEffect,
  useMemo,
  useRef,
  useState,
} from "react";
import type { GlobalLabelShape } from "../bindings/GlobalLabelShape";
import type { JunctionDot } from "../bindings/JunctionDot";
import type { LibraryHit } from "../bindings/LibraryHit";
import type { MirrorAxis } from "../bindings/MirrorAxis";
import type { NetLabel } from "../bindings/NetLabel";
import type { SymbolInstance } from "../bindings/SymbolInstance";
import type { SymbolPreview } from "../bindings/SymbolPreview";
import type { WireSegment } from "../bindings/WireSegment";
import {
  deleteSymbols,
  deleteWiring,
  describeIpcError,
  drawWire,
  getLibrarySymbol,
  moveSymbols,
  moveWiring,
  placeLabel,
  placeSymbol,
  rotateLabels,
  transformSymbols,
} from "../ipc";
import {
  type GhostState,
  type LabelAppearance,
  drawSchematicScene,
} from "../schematicDraw";
import {
  findInstanceAt,
  normalizeSteps,
  snapToGrid,
  snappedDragDelta,
} from "../symbols";
import type { LibraryStatusState } from "../useLibraryStatus";
import {
  type ScreenPoint,
  type SheetDimensions,
  type Size,
  type Viewport,
  pxPerNm,
  screenToWorld,
  worldToScreen,
} from "../viewport";
import {
  SNAP_RADIUS_PX,
  type SnappedPoint,
  WIRE_HIT_PX,
  connectionPoints,
  dedupePath,
  findLabelAt,
  findWireAt,
  isOnPin,
  isOnWire,
  nextLabelText,
  snapWirePoint,
  wirePath,
} from "../wiring";
import { Canvas } from "./Canvas";
import { InlineLabelEditor } from "./InlineLabelEditor";
import { PowerPicker } from "./PowerPicker";
import type { StatusHint } from "./StatusBar";

/** A canvas tool the toolbar can start. */
export type CanvasTool = "localLabel" | "globalLabel" | "power";

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

type ItemKind = "symbol" | "wire" | "label";

interface Selection {
  kind: ItemKind;
  uuid: string;
}

interface DragState extends Selection {
  startX: number;
  startY: number;
  dx: number;
  dy: number;
}

interface WireState {
  /** The fixed points so far: the start and every anchored corner. */
  anchors: ScreenPoint[];
  verticalFirst: boolean;
}

interface LabelPrompt {
  global: boolean;
  /** Where the popover opens, in world coordinates. */
  at: ScreenPoint;
}

interface LabelPlacingState {
  text: string;
  shape: GlobalLabelShape | null;
  /** Quarter turns counterclockwise; normalized when sent or drawn. */
  rotation: number;
}

/** Pointer travel, in screen pixels, before a press becomes a drag. */
const DRAG_THRESHOLD_PX = 4;

const SELECTION_HINTS: Record<ItemKind, string> = {
  symbol: "Symbol selected. Drag to move. R/X/Y: Transform. Del: Remove",
  wire: "Wire selected. Drag to move. Del: Remove",
  label: "Label selected. Drag to move. R: Rotate. Del: Remove",
};

const SELECTION_NOUNS: Record<ItemKind, string> = {
  symbol: "Symbol",
  wire: "Wire",
  label: "Label",
};

function countOf(count: number, noun: string): string {
  return `${count} ${noun}${count === 1 ? "" : "s"}`;
}

interface SchematicCanvasProps {
  label: string;
  viewport: Viewport | null;
  gridSizeNm: number;
  sheet: SheetDimensions;
  definitions: { [key in string]: SymbolPreview };
  instances: SymbolInstance[];
  wires: WireSegment[];
  junctions: JunctionDot[];
  labels: NetLabel[];
  wireMode: boolean;
  onWireModeChange: (active: boolean) => void;
  library: LibraryStatusState;
  onDownloadLibrary: () => void;
  onOpenLibrary: () => void;
  toolRequest: CanvasTool | null;
  onToolRequestConsumed: () => void;
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
  wires,
  junctions,
  labels,
  wireMode,
  onWireModeChange,
  library,
  onDownloadLibrary,
  onOpenLibrary,
  toolRequest,
  onToolRequestConsumed,
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
  const wireModeRef = useRef(wireMode);
  const lastLabelRef = useRef<{ text: string; shape: GlobalLabelShape }>({
    text: "",
    shape: "input",
  });
  const [placing, setPlacing] = useState<PlacingState | null>(null);
  const [selection, setSelection] = useState<Selection | null>(null);
  const [drag, setDrag] = useState<DragState | null>(null);
  const [hover, setHover] = useState(false);
  const [cursor, setCursor] = useState<ScreenPoint | null>(null);
  const [wire, setWire] = useState<WireState | null>(null);
  const [labelPrompt, setLabelPrompt] = useState<LabelPrompt | null>(null);
  const [labelPlacing, setLabelPlacing] = useState<LabelPlacingState | null>(
    null,
  );
  const [powerPickerAt, setPowerPickerAt] = useState<ScreenPoint | null>(null);
  const [announcement, setAnnouncement] = useState("");
  const [commandError, setCommandError] = useState<string | null>(null);

  const targets = useMemo(
    () => connectionPoints(instances, wires),
    [instances, wires],
  );
  const snapRadiusNm = viewport ? SNAP_RADIUS_PX / pxPerNm(viewport.zoom) : 0;

  const snapWire = (world: ScreenPoint): SnappedPoint =>
    snapWirePoint(world, gridSizeNm, targets, snapRadiusNm);

  const exitPlacement = useCallback(() => {
    // Invalidate any in-flight definition load.
    loadTicketRef.current += 1;
    setPlacing(null);
  }, []);

  /** Ends every in-progress tool so another one can start. */
  const clearTools = useCallback(() => {
    exitPlacement();
    setWire(null);
    setLabelPrompt(null);
    setLabelPlacing(null);
    setPowerPickerAt(null);
    setDrag(null);
  }, [exitPlacement]);

  /** Leaves wire mode for another tool without the mode change clearing it. */
  const leaveWireMode = useCallback(() => {
    if (wireModeRef.current) {
      wireModeRef.current = false;
      onWireModeChange(false);
    }
  }, [onWireModeChange]);

  const beginPlacement = useCallback(
    (hit: LibraryHit, at: ScreenPoint | null) => {
      clearTools();
      leaveWireMode();
      const ticket = ++loadTicketRef.current;
      setSelection(null);
      setHover(false);
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
    [clearTools, leaveWireMode],
  );

  useEffect(() => {
    if (placementRequest) {
      onPlacementRequestConsumed();
      beginPlacement(placementRequest.hit, placementRequest.at);
    }
  }, [placementRequest, onPlacementRequestConsumed, beginPlacement]);

  // Entering or leaving wire mode, from the keyboard or the toolbar.
  useEffect(() => {
    if (wireModeRef.current === wireMode) {
      return;
    }
    wireModeRef.current = wireMode;
    clearTools();
    if (wireMode) {
      setSelection(null);
      setCommandError(null);
      setAnnouncement("Wiring mode started");
      containerRef.current?.focus();
    } else {
      setAnnouncement("Wiring mode ended");
    }
  }, [wireMode, clearTools]);

  // Drop the selection when its item disappears (for example via undo).
  useEffect(() => {
    if (selection === null) {
      return;
    }
    const present =
      selection.kind === "symbol"
        ? instances.some((instance) => instance.uuid === selection.uuid)
        : selection.kind === "wire"
          ? wires.some((segment) => segment.uuid === selection.uuid)
          : labels.some((item) => item.uuid === selection.uuid);
    if (!present) {
      setSelection(null);
    }
  }, [instances, wires, labels, selection]);

  useEffect(() => {
    if (commandError !== null) {
      onHintChange({ text: commandError, error: true });
    } else if (labelPrompt) {
      onHintChange({ text: "Enter label name. Esc: Cancel", error: false });
    } else if (labelPlacing) {
      onHintChange({
        text: "Click to place. R: Rotate. Esc: Cancel",
        error: false,
      });
    } else if (placing) {
      onHintChange({
        text: "Click to place. R: Rotate, X/Y: Mirror. Esc: Cancel",
        error: false,
      });
    } else if (wireMode && wire) {
      onHintChange({
        text: "Click to anchor. Double-click/Enter/Pin to finish. /: Toggle bend. Esc: Cancel",
        error: false,
      });
    } else if (wireMode) {
      onHintChange({ text: "Click to start wire. Esc: Exit", error: false });
    } else if (selection !== null) {
      onHintChange({ text: SELECTION_HINTS[selection.kind], error: false });
    } else {
      onHintChange(null);
    }
  }, [
    commandError,
    labelPrompt,
    labelPlacing,
    placing,
    wireMode,
    wire,
    selection,
    onHintChange,
  ]);

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

  /** The cursor, or the middle of the canvas when the pointer is outside it. */
  const cursorOrCenter = useCallback((): ScreenPoint | null => {
    if (cursor) {
      return cursor;
    }
    if (!viewport || sizeRef.current.width === 0) {
      return null;
    }
    return screenToWorld(
      viewport,
      sizeRef.current.width / 2,
      sizeRef.current.height / 2,
    );
  }, [cursor, viewport]);

  const openLabelPrompt = useCallback(
    (global: boolean) => {
      const at = cursorOrCenter();
      if (!at) {
        return;
      }
      clearTools();
      leaveWireMode();
      setSelection(null);
      setCommandError(null);
      setLabelPrompt({ global, at });
    },
    [cursorOrCenter, clearTools, leaveWireMode],
  );

  const openPowerPicker = useCallback(() => {
    const at = cursorOrCenter();
    if (!at) {
      return;
    }
    clearTools();
    leaveWireMode();
    setSelection(null);
    setCommandError(null);
    setPowerPickerAt(at);
  }, [cursorOrCenter, clearTools, leaveWireMode]);

  useEffect(() => {
    if (toolRequest === null) {
      return;
    }
    onToolRequestConsumed();
    if (toolRequest === "power") {
      openPowerPicker();
    } else {
      openLabelPrompt(toolRequest === "globalLabel");
    }
  }, [toolRequest, onToolRequestConsumed, openPowerPicker, openLabelPrompt]);

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

  const placeLabelAt = (world: ScreenPoint) => {
    if (!labelPlacing) {
      return;
    }
    runCommand(
      placeLabel({
        text: labelPlacing.text,
        shape: labelPlacing.shape,
        xNm: snapToGrid(world.x, gridSizeNm),
        yNm: snapToGrid(world.y, gridSizeNm),
        rotation: normalizeSteps(labelPlacing.rotation),
      }),
      "Label placed",
    );
    const next = nextLabelText(labelPlacing.text);
    lastLabelRef.current = { ...lastLabelRef.current, text: next };
    setLabelPlacing({ ...labelPlacing, text: next });
  };

  const finishWire = (points: ScreenPoint[]) => {
    setWire(null);
    const path = dedupePath(points);
    if (path.length < 2) {
      return;
    }
    runCommand(
      drawWire(path.map((point) => [point.x, point.y])),
      "Wire completed",
    );
  };

  const cancelWire = () => {
    setWire(null);
    setAnnouncement("Wire cancelled");
  };

  const handleWirePointerDown = (world: ScreenPoint) => {
    const point = snapWire(world).point;
    if (!wire) {
      setWire({ anchors: [point], verticalFirst: false });
      return;
    }
    const last = wire.anchors[wire.anchors.length - 1];
    if (last && last.x === point.x && last.y === point.y) {
      // The second press of a double-click: the dblclick event finishes.
      return;
    }
    const path = dedupePath(wirePath(wire.anchors, point, wire.verticalFirst));
    if (isOnPin(instances, point) || isOnWire(wires, point)) {
      finishWire(path);
    } else {
      setWire({ ...wire, anchors: path });
    }
  };

  const hitTest = (world: ScreenPoint): Selection | null => {
    const labelHit = findLabelAt(labels, world);
    if (labelHit) {
      return { kind: "label", uuid: labelHit.uuid };
    }
    const symbolHit = findInstanceAt(instances, definitions, world);
    if (symbolHit) {
      return { kind: "symbol", uuid: symbolHit.uuid };
    }
    if (viewport) {
      const tolerance = WIRE_HIT_PX / pxPerNm(viewport.zoom);
      const wireHit = findWireAt(wires, world, tolerance);
      if (wireHit) {
        return { kind: "wire", uuid: wireHit.uuid };
      }
    }
    return null;
  };

  const handlePointerDown = (
    world: ScreenPoint,
    event: PointerEvent<HTMLDivElement>,
  ) => {
    if (event.button !== 0) {
      return;
    }
    setCommandError(null);
    if (labelPrompt || powerPickerAt) {
      setLabelPrompt(null);
      setPowerPickerAt(null);
      return;
    }
    if (labelPlacing) {
      placeLabelAt(world);
      return;
    }
    if (placing) {
      placeAt(world);
      return;
    }
    if (wireMode) {
      handleWirePointerDown(world);
      return;
    }
    const hit = hitTest(world);
    if (hit) {
      if (hit.uuid !== selection?.uuid) {
        setSelection(hit);
        setAnnouncement(`${SELECTION_NOUNS[hit.kind]} selected`);
      }
      const target = event.currentTarget;
      try {
        target.setPointerCapture(event.pointerId);
      } catch {
        // Environments without active pointers (tests): dragging still works.
      }
      setDrag({ ...hit, startX: world.x, startY: world.y, dx: 0, dy: 0 });
    } else if (selection !== null) {
      setSelection(null);
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
    if (event.type === "pointercancel" || (drag.dx === 0 && drag.dy === 0)) {
      return;
    }
    const uuids = [drag.uuid];
    runCommand(
      drag.kind === "symbol"
        ? moveSymbols(uuids, drag.dx, drag.dy)
        : moveWiring(uuids, drag.dx, drag.dy),
      null,
    );
  };

  const handleDoubleClick = () => {
    if (wireMode && wire) {
      finishWire(wire.anchors);
    }
  };

  const handleCursorMove = (position: ScreenPoint | null) => {
    setCursor(position);
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
    if (!placing && !drag && !wireMode && !labelPlacing) {
      const over = position !== null && hitTest(position) !== null;
      if (over !== hover) {
        setHover(over);
      }
    }
    onCursorMove(position);
  };

  const handleToolKey = (
    key: string,
    event: KeyboardEvent<HTMLDivElement>,
  ): boolean => {
    if (key === "w") {
      event.preventDefault();
      if (!wireMode) {
        onWireModeChange(true);
      }
      return true;
    }
    if (key === "l") {
      event.preventDefault();
      openLabelPrompt(false);
      return true;
    }
    if (key === "p") {
      event.preventDefault();
      openPowerPicker();
      return true;
    }
    return false;
  };

  const handleKeyDown = (event: KeyboardEvent<HTMLDivElement>) => {
    if (event.ctrlKey || event.metaKey) {
      return;
    }
    if (event.altKey) {
      // `code`, not `key`: Option+L types a symbol on macOS.
      if (event.code === "KeyL") {
        event.preventDefault();
        openLabelPrompt(true);
      }
      return;
    }
    const key = event.key.toLowerCase();
    if (labelPlacing) {
      if (key === "escape") {
        event.preventDefault();
        setLabelPlacing(null);
      } else if (key === "enter") {
        event.preventDefault();
        const at = cursorOrCenter();
        if (at) {
          placeLabelAt(at);
        }
      } else if (key === "r") {
        event.preventDefault();
        setLabelPlacing({
          ...labelPlacing,
          rotation: labelPlacing.rotation - 1,
        });
      }
      return;
    }
    if (placing) {
      if (key === "escape") {
        event.preventDefault();
        exitPlacement();
      } else if (key === "enter") {
        event.preventDefault();
        const at = placing.cursor ?? cursorOrCenter();
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
    if (wireMode) {
      if (key === "escape") {
        event.preventDefault();
        if (wire) {
          cancelWire();
        } else {
          onWireModeChange(false);
        }
      } else if (key === "enter" && wire) {
        event.preventDefault();
        const at = cursor ? snapWire(cursor).point : null;
        finishWire(
          at ? wirePath(wire.anchors, at, wire.verticalFirst) : wire.anchors,
        );
      } else if (key === "/" && wire) {
        event.preventDefault();
        setWire({ ...wire, verticalFirst: !wire.verticalFirst });
      } else {
        handleToolKey(key, event);
      }
      return;
    }
    if (handleToolKey(key, event) || selection === null) {
      return;
    }
    const uuids = [selection.uuid];
    if (key === "escape") {
      event.preventDefault();
      setSelection(null);
    } else if (key === "delete" || key === "backspace") {
      event.preventDefault();
      setSelection(null);
      runCommand(
        selection.kind === "symbol"
          ? deleteSymbols(uuids)
          : deleteWiring(uuids),
        `${SELECTION_NOUNS[selection.kind]} deleted`,
      );
    } else if (key === "r" && selection.kind === "label") {
      event.preventDefault();
      // R turns clockwise on screen: three counterclockwise steps.
      runCommand(rotateLabels(uuids, 3), null);
    } else if (selection.kind !== "symbol") {
      return;
    } else if (key === "r") {
      event.preventDefault();
      runCommand(transformSymbols(uuids, { type: "rotate", steps: 3 }), null);
    } else if (key === "x" || key === "y") {
      event.preventDefault();
      runCommand(transformSymbols(uuids, { type: "mirror", axis: key }), null);
    }
  };

  const handleContextMenu = (event: MouseEvent<HTMLDivElement>) => {
    if (labelPlacing) {
      event.preventDefault();
      setLabelPlacing(null);
    } else if (placing) {
      event.preventDefault();
      exitPlacement();
    } else if (wireMode) {
      event.preventDefault();
      if (wire) {
        cancelWire();
      } else {
        onWireModeChange(false);
      }
    }
  };

  const handleResize = (size: Size) => {
    sizeRef.current = size;
    onResize(size);
  };

  const wireCursor = useMemo(
    () =>
      wireMode && cursor
        ? snapWirePoint(cursor, gridSizeNm, targets, snapRadiusNm)
        : null,
    [wireMode, cursor, gridSizeNm, targets, snapRadiusNm],
  );
  const wirePreview = useMemo(
    () =>
      wire && wireCursor
        ? wirePath(wire.anchors, wireCursor.point, wire.verticalFirst)
        : null,
    [wire, wireCursor],
  );
  const labelGhost = useMemo((): LabelAppearance | null => {
    if (!labelPlacing || !cursor) {
      return null;
    }
    return {
      text: labelPlacing.text,
      shape: labelPlacing.shape,
      position: [
        snapToGrid(cursor.x, gridSizeNm),
        snapToGrid(cursor.y, gridSizeNm),
      ],
      rotation: normalizeSteps(labelPlacing.rotation) * 900,
    };
  }, [labelPlacing, cursor, gridSizeNm]);

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
        wires,
        junctions,
        labels,
        selectedUuid: selection?.uuid ?? null,
        drag,
        ghost,
        wirePreview,
        snap: wireCursor,
        labelGhost,
        gridSizeNm,
      });
    },
    [
      definitions,
      instances,
      wires,
      junctions,
      labels,
      selection,
      drag,
      placing,
      wirePreview,
      wireCursor,
      labelGhost,
      gridSizeNm,
    ],
  );

  const sceneCursor = drag ? "grabbing" : hover && !placing ? "pointer" : null;

  const popoverAt = (world: ScreenPoint): ScreenPoint =>
    viewport ? worldToScreen(viewport, world.x, world.y) : { x: 0, y: 0 };

  const refocus = () => containerRef.current?.focus();

  const wiringSummary = [
    countOf(wires.length, "wire"),
    countOf(junctions.length, "junction"),
  ].join(", ");

  return (
    <div className="schematic">
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
          onDoubleClick: handleDoubleClick,
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
      {labelPrompt && (
        <InlineLabelEditor
          global={labelPrompt.global}
          at={popoverAt(labelPrompt.at)}
          initialText={lastLabelRef.current.text}
          initialShape={lastLabelRef.current.shape}
          onSubmit={(text, shape) => {
            lastLabelRef.current = {
              text,
              shape: shape ?? lastLabelRef.current.shape,
            };
            setLabelPrompt(null);
            setLabelPlacing({ text, shape, rotation: 0 });
            setAnnouncement(`Placing label ${text}`);
            refocus();
          }}
          onCancel={() => {
            setLabelPrompt(null);
            refocus();
          }}
        />
      )}
      {powerPickerAt && (
        <PowerPicker
          at={popoverAt(powerPickerAt)}
          library={library}
          onPick={(hit) => beginPlacement(hit, powerPickerAt)}
          onSearchLibrary={() => {
            setPowerPickerAt(null);
            onOpenLibrary();
          }}
          onDownloadLibrary={onDownloadLibrary}
          onCancel={() => {
            setPowerPickerAt(null);
            refocus();
          }}
        />
      )}
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
      <ul className="visually-hidden" aria-label="Net labels">
        {labels.map((item) => (
          <li key={item.uuid}>
            {item.shape ? `Global label ${item.text}` : `Label ${item.text}`}
          </li>
        ))}
      </ul>
      <p className="visually-hidden">{wiringSummary}</p>
    </div>
  );
}
