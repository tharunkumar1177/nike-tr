import type { ReactNode } from "react";
import type { HistoryStatus } from "../bindings/HistoryStatus";
import { PropertiesIcon, RedoIcon, UndoIcon } from "./icons";

interface IconButtonProps {
  label: string;
  shortcut?: string;
  disabled: boolean;
  onClick: () => void;
  children: ReactNode;
}

function IconButton({
  label,
  shortcut,
  disabled,
  onClick,
  children,
}: IconButtonProps) {
  return (
    <button
      type="button"
      className="icon-button"
      aria-label={label}
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
}

export function Toolbar({
  history,
  zoomLabel,
  disabled,
  onUndo,
  onRedo,
  onOpenProperties,
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
      </div>
      <output className="toolbar__zoom" aria-label="Zoom level">
        {zoomLabel}
      </output>
    </div>
  );
}
