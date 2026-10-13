import type { ReactNode } from "react";
import type { HistoryStatus } from "../bindings/HistoryStatus";
import type { CanvasTool } from "./SchematicCanvas";
import {
  GlobalLabelIcon,
  LabelIcon,
  LibraryIcon,
  PowerIcon,
  PropertiesIcon,
  RedoIcon,
  UndoIcon,
  WireIcon,
} from "./icons";

interface IconButtonProps {
  label: string;
  shortcut?: string;
  pressed?: boolean;
  disabled: boolean;
  onClick: () => void;
  children: ReactNode;
}

function IconButton({
  label,
  shortcut,
  pressed,
  disabled,
  onClick,
  children,
}: IconButtonProps) {
  return (
    <button
      type="button"
      className="icon-button"
      aria-label={label}
      aria-pressed={pressed}
      title={shortcut ? `${label} (${shortcut})` : label}
      disabled={disabled}
      onClick={onClick}
    >
      {children}
    </button>
  );
}

interface ToolbarProps {
  history: HistoryStatus;
  zoomLabel: string;
  disabled: boolean;
  onUndo: () => void;
  onRedo: () => void;
  onOpenProperties: () => void;
  libraryOpen: boolean;
  onToggleLibrary: () => void;
  wireMode: boolean;
  onToggleWireMode: () => void;
  onStartTool: (tool: CanvasTool) => void;
}

export function Toolbar({
  history,
  zoomLabel,
  disabled,
  onUndo,
  onRedo,
  onOpenProperties,
  libraryOpen,
  onToggleLibrary,
  wireMode,
  onToggleWireMode,
  onStartTool,
}: ToolbarProps) {
  return (
    <div className="toolbar" role="toolbar" aria-label="Editor">
      <div className="toolbar__group">
        <IconButton
          label={history.undoLabel ? `Undo ${history.undoLabel}` : "Undo"}
          shortcut="Ctrl+Z"
          disabled={disabled || !history.canUndo}
          onClick={onUndo}
        >
          <UndoIcon />
        </IconButton>
        <IconButton
          label={history.redoLabel ? `Redo ${history.redoLabel}` : "Redo"}
          shortcut="Ctrl+Y"
          disabled={disabled || !history.canRedo}
          onClick={onRedo}
        >
          <RedoIcon />
        </IconButton>
        <span className="toolbar__separator" aria-hidden="true" />
        <IconButton
          label="Project Properties"
          disabled={disabled}
          onClick={onOpenProperties}
        >
          <PropertiesIcon />
        </IconButton>
        <IconButton
          label="Component Library"
          shortcut="Ctrl+L"
          pressed={libraryOpen}
          disabled={disabled}
          onClick={onToggleLibrary}
        >
          <LibraryIcon />
        </IconButton>
        <span className="toolbar__separator" aria-hidden="true" />
        <IconButton
          label="Draw wire"
          shortcut="W"
          pressed={wireMode}
          disabled={disabled}
          onClick={onToggleWireMode}
        >
          <WireIcon />
        </IconButton>
        <IconButton
          label="Add net label"
          shortcut="L"
          disabled={disabled}
          onClick={() => onStartTool("localLabel")}
        >
          <LabelIcon />
        </IconButton>
        <IconButton
          label="Add global label"
          shortcut="Alt+L"
          disabled={disabled}
          onClick={() => onStartTool("globalLabel")}
        >
          <GlobalLabelIcon />
        </IconButton>
        <IconButton
          label="Add power symbol"
          shortcut="P"
          disabled={disabled}
          onClick={() => onStartTool("power")}
        >
          <PowerIcon />
        </IconButton>
      </div>
      <output className="toolbar__zoom" aria-label="Zoom level">
        {zoomLabel}
      </output>
    </div>
  );
}
