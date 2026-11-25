/**
 * The board view canvas: draws the outline, zones, tracks, vias, footprints,
 * and ratsnest. Outside route mode the user selects a footprint, drags it,
 * rotates it (R), and flips it to the other side (F), or selects a track or
 * via and deletes it. In route mode (X) clicks lay out a 45-degree route
 * that V drops vias into, finished as one track command. Each gesture is one
 * core command and one undo step.
 */
import {
  type KeyboardEvent,
  type PointerEvent,
  useCallback,
  useEffect,
  useMemo,
  useState,
} from "react";
import type { BoardDesignRules } from "../bindings/BoardDesignRules";
import type { BoardFootprint } from "../bindings/BoardFootprint";
import type { BoardTrack } from "../bindings/BoardTrack";
import type { BoardVia } from "../bindings/BoardVia";
import type { BoardZone } from "../bindings/BoardZone";
import type { FootprintTransform } from "../bindings/FootprintTransform";
import type { OutlineGraphic } from "../bindings/OutlineGraphic";
import type { RatsnestSegment } from "../bindings/RatsnestSegment";
import { BOARD_GRID_NM, findFootprintAt, shiftRatsnest } from "../board";
import { type RoutePreview, drawBoardScene } from "../boardDraw";
import {
  deleteRoutes,
  describeIpcError,
  moveFootprints,
  routeTrack,
  transformFootprints,
} from "../ipc";
import {
  type RouteDraft,
  TRACK_HIT_PX,
  extendRoute,
  findTrackAt,
  findViaAt,
  finishedRoute,
  legPieces,
  routeEnd,
  routePieces,
  routeTargets,
  routeVias,
  snapRoutePoint,
  stepLayer,
  viaLayer,
} from "../routing";
import { snappedDragDelta } from "../symbols";
import {
  type ScreenPoint,
  type Size,
  type Viewport,
  formatMm,
  pxPerNm,
} from "../viewport";
import { Canvas, type SceneCursor } from "./Canvas";
import type { StatusHint } from "./StatusBar";

/** Pointer travel, in screen pixels, before a press becomes a drag. */
const DRAG_THRESHOLD_PX = 4;
const SELECTION_HINT =
  "Footprint selected. Drag to move. R: Rotate. F: Flip side. Esc: Deselect";
const TRACK_HINT = "Track selected. Del: Remove";
const VIA_HINT = "Via selected. Del: Remove";
const ROUTE_HINT = "Click a pad or track end to route. Esc: Exit";
const ROUTING_HINT =
  "Click: Corner. Double-click/Enter: Finish. /: Posture. V: Via. Backspace: Undo corner. Esc: Cancel";
const EMPTY_HINT =
  "Board is empty. Set the outline (Tools > Board Outline), then F8 updates it from the schematic.";

/** A footprint being dragged, and its snapped offset so far. */
interface Drag {
  uuid: string;
  startX: number;
  startY: number;
  dx: number;
  dy: number;
}

/** The selected board item. */
interface Selection {
  kind: "footprint" | "track" | "via";
  uuid: string;
}

interface BoardCanvasProps {
  label: string;
  viewport: Viewport | null;
  footprints: BoardFootprint[];
  outline: OutlineGraphic[];
  ratsnest: RatsnestSegment[];
  ratsnestVisible: boolean;
  tracks: BoardTrack[];
  vias: BoardVia[];
  zones: BoardZone[];
  /** The sizes new tracks and vias get. */
  designRules: BoardDesignRules;
  /** The board's copper layers, top to bottom. */
  layers: string[];
  /** The copper layers shown; the active layer is always among them. */
  visibleLayers: ReadonlySet<string>;
  /** The layer new route pieces go on. */
  activeLayer: string;
  onActiveLayerChange: (layer: string) => void;
  routeMode: boolean;
  onRouteModeChange: (active: boolean) => void;
  /** A one-off message, such as the result of an update, shown until the next input. */
  notice: string | null;
  onNoticeConsumed: () => void;
  onHintChange: (hint: StatusHint | null) => void;
  onViewportChange: (update: (viewport: Viewport) => Viewport) => void;
  onResize: (size: Size) => void;
  onCursorMove: (position: ScreenPoint | null) => void;
}

/** How a footprint reads in the screen reader list. */
function describeFootprint(footprint: BoardFootprint): string {
  const name = footprint.reference
    ? `${footprint.reference} ${footprint.libId}`
    : footprint.libId;
  return `${name}, ${footprint.side} side`;
}

/** How a track reads in the screen reader list. */
function describeTrack(track: BoardTrack): string {
  const net = track.net ? `net ${track.net}` : "no net";
  return `${track.layer} track, ${net}, ${formatMm(track.widthNm)} mm wide`;
}

/** How a via reads in the screen reader list. */
function describeVia(via: BoardVia): string {
  const net = via.net ? `net ${via.net}` : "no net";
  return `Via, ${net}, ${via.layers.join(" to ")}`;
}

/** Whether the item `selection` names is still on the board. */
function stillPlaced(
  selection: Selection,
  footprints: BoardFootprint[],
  tracks: BoardTrack[],
  vias: BoardVia[],
): boolean {
  const { kind, uuid } = selection;
  switch (kind) {
    case "footprint":
      return footprints.some((footprint) => footprint.uuid === uuid);
    case "track":
      return tracks.some((track) => track.uuid === uuid);
    case "via":
      return vias.some((via) => via.uuid === uuid);
  }
}

/** The board canvas with footprint editing, track and via selection, and routing. */
export function BoardCanvas({
  label,
  viewport,
  footprints,
  outline,
  ratsnest,
  ratsnestVisible,
  tracks,
  vias,
  zones,
  designRules,
  layers,
  visibleLayers,
  activeLayer,
  onActiveLayerChange,
  routeMode,
  onRouteModeChange,
  notice,
  onNoticeConsumed,
  onHintChange,
  onViewportChange,
  onResize,
  onCursorMove,
}: BoardCanvasProps) {
  const [selection, setSelection] = useState<Selection | null>(null);
  const [hover, setHover] = useState<string | null>(null);
  const [drag, setDrag] = useState<Drag | null>(null);
  const [draft, setDraft] = useState<RouteDraft | null>(null);
  const [diagonalFirst, setDiagonalFirst] = useState(false);
  const [pointer, setPointer] = useState<ScreenPoint | null>(null);
  const [commandError, setCommandError] = useState<string | null>(null);
  const [announcement, setAnnouncement] = useState("Board view");

  useEffect(() => {
    if (
      selection !== null &&
      !stillPlaced(selection, footprints, tracks, vias)
    ) {
      setSelection(null);
    }
  }, [footprints, tracks, vias, selection]);

  useEffect(() => {
    setDraft(null);
    if (routeMode) {
      setSelection(null);
    }
  }, [routeMode]);

  const empty = footprints.length === 0 && outline.length === 0;
  let idleHint: string | null = null;
  if (routeMode) {
    idleHint = draft ? ROUTING_HINT : ROUTE_HINT;
  } else if (selection?.kind === "footprint") {
    idleHint = SELECTION_HINT;
  } else if (selection?.kind === "track") {
    idleHint = TRACK_HINT;
  } else if (selection?.kind === "via") {
    idleHint = VIA_HINT;
  } else if (empty) {
    idleHint = EMPTY_HINT;
  }

  useEffect(() => {
    if (commandError !== null) {
      onHintChange({ text: commandError, error: true });
    } else if (notice !== null) {
      onHintChange({ text: notice, error: false });
    } else if (idleHint !== null) {
      onHintChange({ text: idleHint, error: false });
    } else {
      onHintChange(null);
    }
  }, [commandError, notice, idleHint, onHintChange]);

  useEffect(() => () => onHintChange(null), [onHintChange]);

  const targets = useMemo(
    () => routeTargets(footprints, tracks, vias),
    [footprints, tracks, vias],
  );
  /** Screen pixels of hit tolerance, in board nanometers. */
  const toleranceNm = viewport ? TRACK_HIT_PX / pxPerNm(viewport.zoom) : 0;
  const visibleList = layers.filter((layer) => visibleLayers.has(layer));

  /** Clears the error and the notice when the user acts again. */
  const freshInput = () => {
    setCommandError(null);
    if (notice !== null) {
      onNoticeConsumed();
    }
  };

  /** Runs a board command, showing its failure in the status bar. */
  const runCommand = (command: Promise<void>) => {
    void command.catch((error: unknown) => {
      setCommandError(describeIpcError(error));
    });
  };

  /** Sends the route to the core; a refused route stays open for editing. */
  const commitRoute = (route: RouteDraft) => {
    const request = finishedRoute(route, designRules);
    if (request === null) {
      setDraft(null);
      setAnnouncement("Route cancelled");
      return;
    }
    setDraft(route);
    void routeTrack(request).then(
      () => {
        setDraft(null);
        setAnnouncement("Route finished");
      },
      (error: unknown) => setCommandError(describeIpcError(error)),
    );
  };

  /** Starts a route at a click, or adds a corner to the open one. */
  const routeClick = (world: ScreenPoint) => {
    const { point, target } = snapRoutePoint(
      world,
      targets,
      toleranceNm,
      BOARD_GRID_NM,
    );
    if (draft === null) {
      const layer = target?.layer ?? activeLayer;
      if (layer !== activeLayer) {
        onActiveLayerChange(layer);
      }
      const net = target?.net ?? null;
      setDraft({ net, start: point, steps: [] });
      setAnnouncement(
        net ? `Routing net ${net}` : "Routing a track with no net",
      );
      return;
    }
    const next = extendRoute(draft, point, diagonalFirst, activeLayer, false);
    const reachesNet =
      target !== null && draft.net !== null && target.net === draft.net;
    if (reachesNet && next !== draft) {
      commitRoute(next);
    } else {
      setDraft(next);
    }
  };

  /** Runs the open route to the cursor, drops a via there, and goes on on `layer`. */
  const addVia = (route: RouteDraft, layer: string) => {
    const to = pointer
      ? snapRoutePoint(pointer, targets, toleranceNm, BOARD_GRID_NM).point
      : routeEnd(route);
    setDraft(extendRoute(route, to, diagonalFirst, activeLayer, true));
    onActiveLayerChange(layer);
    setAnnouncement(`Layer switched to ${layer}`);
  };

  /** Removes the open route's last step, or cancels a route without one. */
  const undoCorner = (route: RouteDraft) => {
    const last = route.steps[route.steps.length - 1];
    if (!last) {
      setDraft(null);
      setAnnouncement("Route cancelled");
      return;
    }
    setDraft({ ...route, steps: route.steps.slice(0, -1) });
    if (last.layerBefore !== activeLayer) {
      onActiveLayerChange(last.layerBefore);
    }
    setAnnouncement("Corner removed");
  };

  /** Deletes the tracks and vias in `uuids`, then announces the `noun` deleted. */
  const removeRoutes = async (uuids: string[], noun: string) => {
    await deleteRoutes(uuids);
    setAnnouncement(`${noun} deleted`);
  };

  /** Deletes the selected track or via; returns whether one was selected. */
  const deleteSelection = (): boolean => {
    if (selection === null || selection.kind === "footprint") {
      return false;
    }
    const noun = selection.kind === "via" ? "Via" : "Track";
    runCommand(removeRoutes([selection.uuid], noun));
    return true;
  };

  const handlePointerDown = (
    world: ScreenPoint,
    event: PointerEvent<HTMLDivElement>,
  ) => {
    if (event.button !== 0) {
      return;
    }
    freshInput();
    if (routeMode) {
      routeClick(world);
      return;
    }
    const via = findViaAt(vias, world, toleranceNm);
    const track = via
      ? null
      : findTrackAt(tracks, world, toleranceNm, visibleLayers);
    const copper = via ?? track;
    if (copper) {
      const kind = via ? "via" : "track";
      if (copper.uuid !== selection?.uuid) {
        setSelection({ kind, uuid: copper.uuid });
        setAnnouncement(via ? "Via selected" : "Track selected");
      }
      return;
    }
    const hit = findFootprintAt(footprints, world);
    if (!hit) {
      setSelection(null);
      return;
    }
    if (hit.uuid !== selection?.uuid) {
      setSelection({ kind: "footprint", uuid: hit.uuid });
      setAnnouncement("Footprint selected");
    }
    try {
      event.currentTarget.setPointerCapture(event.pointerId);
    } catch {
      // Environments without active pointers (tests): dragging still works.
    }
    setDrag({ uuid: hit.uuid, startX: world.x, startY: world.y, dx: 0, dy: 0 });
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
    runCommand(moveFootprints([drag.uuid], drag.dx, drag.dy));
  };

  const handleDoubleClick = () => {
    if (routeMode && draft !== null) {
      commitRoute(draft);
    }
  };

  const handleCursorMove = (position: ScreenPoint | null) => {
    if (drag && position) {
      let delta = snappedDragDelta(
        { x: drag.startX, y: drag.startY },
        position,
        BOARD_GRID_NM,
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
    if (routeMode) {
      setPointer(position);
    } else if (!drag) {
      const over = position ? findFootprintAt(footprints, position) : null;
      const uuid = over?.uuid ?? null;
      if (uuid !== hover) {
        setHover(uuid);
      }
    }
    onCursorMove(position);
  };

  /** Handles the keys of route mode; returns whether `key` was one of them. */
  const handleRouteKey = (key: string): boolean => {
    switch (key) {
      case "escape":
        if (draft !== null) {
          setDraft(null);
          setAnnouncement("Route cancelled");
        } else {
          onRouteModeChange(false);
          setAnnouncement("Route mode off");
        }
        return true;
      case "enter":
        if (draft !== null) {
          commitRoute(draft);
        }
        return true;
      case "backspace":
        if (draft !== null) {
          undoCorner(draft);
        }
        return true;
      case "/":
        setDiagonalFirst((current) => !current);
        return true;
      case "v":
        if (draft !== null) {
          addVia(draft, viaLayer(layers, activeLayer));
        }
        return true;
      default:
        return false;
    }
  };

  const handleKeyDown = (event: KeyboardEvent<HTMLDivElement>) => {
    if (event.ctrlKey || event.metaKey || event.altKey) {
      return;
    }
    const key = event.key.toLowerCase();
    if (key === "x") {
      event.preventDefault();
      freshInput();
      onRouteModeChange(!routeMode);
      setAnnouncement(routeMode ? "Route mode off" : "Route mode on");
      return;
    }
    if (key === "pageup" || key === "pagedown") {
      event.preventDefault();
      const layer = stepLayer(
        visibleList,
        activeLayer,
        key === "pageup" ? -1 : 1,
      );
      if (layer === activeLayer) {
        return;
      }
      if (routeMode && draft !== null) {
        addVia(draft, layer);
      } else {
        onActiveLayerChange(layer);
        setAnnouncement(`Layer switched to ${layer}`);
      }
      return;
    }
    if (routeMode) {
      if (handleRouteKey(key)) {
        event.preventDefault();
        freshInput();
      }
      return;
    }
    if (key === "escape") {
      freshInput();
      if (selection !== null) {
        event.preventDefault();
        setSelection(null);
        setAnnouncement("Selection cleared");
      }
      return;
    }
    if (key === "delete" || key === "backspace") {
      freshInput();
      if (deleteSelection()) {
        event.preventDefault();
      }
      return;
    }
    if (selection?.kind !== "footprint" || (key !== "r" && key !== "f")) {
      return;
    }
    event.preventDefault();
    freshInput();
    // R turns three counterclockwise quarter turns: one turn clockwise on screen.
    const transform: FootprintTransform =
      key === "r" ? { type: "rotate", steps: 3 } : { type: "flip" };
    runCommand(transformFootprints([selection.uuid], transform));
  };

  const route = useMemo((): RoutePreview | null => {
    if (!routeMode || draft === null) {
      return null;
    }
    const to = pointer
      ? snapRoutePoint(pointer, targets, toleranceNm, BOARD_GRID_NM).point
      : null;
    const pending = to
      ? legPieces(routeEnd(draft), to, diagonalFirst, activeLayer)
      : [];
    return {
      net: draft.net,
      pieces: routePieces(draft),
      pending,
      vias: routeVias(draft),
      widthNm: designRules.trackWidthNm,
      viaDiameterNm: designRules.viaDiameterNm,
      viaDrillNm: designRules.viaDrillNm,
    };
  }, [
    routeMode,
    draft,
    pointer,
    targets,
    toleranceNm,
    diagonalFirst,
    activeLayer,
    designRules,
  ]);

  const selectedUuid = selection?.uuid ?? null;
  const drawScene = useCallback(
    (context: CanvasRenderingContext2D, _size: Size, view: Viewport) => {
      const dragged = drag ? [drag.uuid] : [];
      const offset = drag ? { x: drag.dx, y: drag.dy } : { x: 0, y: 0 };
      const lines = shiftRatsnest(ratsnest, footprints, dragged, offset);
      drawBoardScene(context, view, {
        footprints,
        outline,
        ratsnest: ratsnestVisible ? lines : null,
        tracks,
        vias,
        zones,
        layers,
        visibleLayers,
        selectedUuid,
        hoverUuid: hover,
        drag,
        route,
      });
    },
    [
      footprints,
      outline,
      ratsnest,
      ratsnestVisible,
      tracks,
      vias,
      zones,
      layers,
      visibleLayers,
      selectedUuid,
      hover,
      drag,
      route,
    ],
  );

  let sceneCursor: SceneCursor | null = null;
  if (routeMode) {
    sceneCursor = "crosshair";
  } else if (drag) {
    sceneCursor = "grabbing";
  } else if (hover !== null) {
    sceneCursor = "pointer";
  }

  return (
    <div className="schematic">
      <Canvas
        label={label}
        viewport={viewport}
        gridSizeNm={BOARD_GRID_NM}
        sheet={null}
        drawScene={drawScene}
        sceneCursor={sceneCursor}
        interaction={{
          onPointerDown: handlePointerDown,
          onPointerUp: handlePointerUp,
          onDoubleClick: handleDoubleClick,
          onKeyDown: handleKeyDown,
        }}
        onViewportChange={onViewportChange}
        onResize={onResize}
        onCursorMove={handleCursorMove}
      />
      <div className="visually-hidden" aria-live="polite">
        {announcement}
      </div>
      <ul className="visually-hidden" aria-label="Placed footprints">
        {footprints.map((footprint) => (
          <li key={footprint.uuid}>{describeFootprint(footprint)}</li>
        ))}
      </ul>
      <ul className="visually-hidden" aria-label="Copper tracks">
        {tracks.map((track) => (
          <li key={track.uuid}>{describeTrack(track)}</li>
        ))}
      </ul>
      <ul className="visually-hidden" aria-label="Vias">
        {vias.map((via) => (
          <li key={via.uuid}>{describeVia(via)}</li>
        ))}
      </ul>
    </div>
  );
}
