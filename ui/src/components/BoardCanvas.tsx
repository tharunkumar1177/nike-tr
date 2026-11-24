/**
 * The board view canvas: draws the outline, footprints, and ratsnest, and lets
 * the user select a footprint, drag it, rotate it (R), and flip it to the
 * other side (F). Each gesture is one core command and one undo step.
 */
import {
  type KeyboardEvent,
  type PointerEvent,
  useCallback,
  useEffect,
  useState,
} from "react";
import type { BoardFootprint } from "../bindings/BoardFootprint";
import type { FootprintTransform } from "../bindings/FootprintTransform";
import type { OutlineGraphic } from "../bindings/OutlineGraphic";
import type { RatsnestSegment } from "../bindings/RatsnestSegment";
import { BOARD_GRID_NM, findFootprintAt, shiftRatsnest } from "../board";
import { drawBoardScene } from "../boardDraw";
import { describeIpcError, moveFootprints, transformFootprints } from "../ipc";
import { snappedDragDelta } from "../symbols";
import {
  type ScreenPoint,
  type Size,
  type Viewport,
  pxPerNm,
} from "../viewport";
import { Canvas, type SceneCursor } from "./Canvas";
import type { StatusHint } from "./StatusBar";

/** Pointer travel, in screen pixels, before a press becomes a drag. */
const DRAG_THRESHOLD_PX = 4;
const SELECTION_HINT =
  "Footprint selected. Drag to move. R: Rotate. F: Flip side. Esc: Deselect";
const EMPTY_HINT = "Board is empty. F8 updates it from the schematic.";

/** A footprint being dragged, and its snapped offset so far. */
interface Drag {
  uuid: string;
  startX: number;
  startY: number;
  dx: number;
  dy: number;
}

interface BoardCanvasProps {
  label: string;
  viewport: Viewport | null;
  footprints: BoardFootprint[];
  outline: OutlineGraphic[];
  ratsnest: RatsnestSegment[];
  ratsnestVisible: boolean;
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

/** The board canvas with footprint selection, dragging, rotation, and flipping. */
export function BoardCanvas({
  label,
  viewport,
  footprints,
  outline,
  ratsnest,
  ratsnestVisible,
  notice,
  onNoticeConsumed,
  onHintChange,
  onViewportChange,
  onResize,
  onCursorMove,
}: BoardCanvasProps) {
  const [selection, setSelection] = useState<string | null>(null);
  const [hover, setHover] = useState<string | null>(null);
  const [drag, setDrag] = useState<Drag | null>(null);
  const [commandError, setCommandError] = useState<string | null>(null);
  const [announcement, setAnnouncement] = useState("Board view");

  useEffect(() => {
    if (
      selection !== null &&
      !footprints.some((footprint) => footprint.uuid === selection)
    ) {
      setSelection(null);
    }
  }, [footprints, selection]);

  const empty = footprints.length === 0 && outline.length === 0;

  useEffect(() => {
    if (commandError !== null) {
      onHintChange({ text: commandError, error: true });
    } else if (notice !== null) {
      onHintChange({ text: notice, error: false });
    } else if (selection !== null) {
      onHintChange({ text: SELECTION_HINT, error: false });
    } else if (empty) {
      onHintChange({ text: EMPTY_HINT, error: false });
    } else {
      onHintChange(null);
    }
  }, [commandError, notice, selection, empty, onHintChange]);

  useEffect(() => () => onHintChange(null), [onHintChange]);

  /** Clears the error and the notice when the user acts again. */
  const freshInput = () => {
    setCommandError(null);
    if (notice !== null) {
      onNoticeConsumed();
    }
  };

  /** Runs a footprint command, showing its failure in the status bar. */
  const runCommand = (command: Promise<void>) => {
    void command.catch((error: unknown) => {
      setCommandError(describeIpcError(error));
    });
  };

  const handlePointerDown = (
    world: ScreenPoint,
    event: PointerEvent<HTMLDivElement>,
  ) => {
    if (event.button !== 0) {
      return;
    }
    freshInput();
    const hit = findFootprintAt(footprints, world);
    if (!hit) {
      setSelection(null);
      return;
    }
    if (hit.uuid !== selection) {
      setSelection(hit.uuid);
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
    if (!drag) {
      const over = position ? findFootprintAt(footprints, position) : null;
      const uuid = over?.uuid ?? null;
      if (uuid !== hover) {
        setHover(uuid);
      }
    }
    onCursorMove(position);
  };

  const handleKeyDown = (event: KeyboardEvent<HTMLDivElement>) => {
    if (event.ctrlKey || event.metaKey || event.altKey) {
      return;
    }
    const key = event.key.toLowerCase();
    if (key === "escape") {
      freshInput();
      if (selection !== null) {
        event.preventDefault();
        setSelection(null);
        setAnnouncement("Selection cleared");
      }
      return;
    }
    if (selection === null || (key !== "r" && key !== "f")) {
      return;
    }
    event.preventDefault();
    freshInput();
    // R turns three counterclockwise quarter turns: one turn clockwise on screen.
    const transform: FootprintTransform =
      key === "r" ? { type: "rotate", steps: 3 } : { type: "flip" };
    runCommand(transformFootprints([selection], transform));
  };

  const drawScene = useCallback(
    (context: CanvasRenderingContext2D, _size: Size, view: Viewport) => {
      const dragged = drag ? [drag.uuid] : [];
      const offset = drag ? { x: drag.dx, y: drag.dy } : { x: 0, y: 0 };
      const lines = shiftRatsnest(ratsnest, footprints, dragged, offset);
      drawBoardScene(context, view, {
        footprints,
        outline,
        ratsnest: ratsnestVisible ? lines : null,
        selectedUuid: selection,
        hoverUuid: hover,
        drag,
      });
    },
    [footprints, outline, ratsnest, ratsnestVisible, selection, hover, drag],
  );

  let sceneCursor: SceneCursor | null = null;
  if (drag) {
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
    </div>
  );
}
