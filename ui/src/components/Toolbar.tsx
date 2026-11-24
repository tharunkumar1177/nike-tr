import type { ReactNode } from "react";
import type { HistoryStatus } from "../bindings/HistoryStatus";
import type { CanvasTool } from "./SchematicCanvas";
import {
  BoardUpdateIcon,
  ErcIcon,
  EyeIcon,
  GlobalLabelIcon,
  LabelIcon,
  LibraryIcon,
  NoConnectIcon,
  PlayIcon,
  PowerIcon,
  ProbeIcon,
  PropertiesIcon,
  RatsnestIcon,
  RedoIcon,
  SettingsIcon,
  StopIcon,
  UndoIcon,
  WireIcon,
} from "./icons";

interface IconButtonProps {
  label: string;
  shortcut?: string;
  /** Tooltip text in place of the label and shortcut. */
  title?: string;
  pressed?: boolean;
  disabled: boolean;
  onClick: () => void;
  children: ReactNode;
}

function IconButton({
  label,
  shortcut,
  title,
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
      title={title ?? (shortcut ? `${label} (${shortcut})` : label)}
      disabled={disabled}
      onClick={onClick}
    >
      {children}
    </button>
  );
}

/** Which document the editor shows. */
export type EditorView = "schematic" | "board";

interface ViewSwitchProps {
  view: EditorView;
  disabled: boolean;
  onViewChange: (view: EditorView) => void;
}

/** The Schematic and Board buttons; the pressed one is the shown view. */
function ViewSwitch({ view, disabled, onViewChange }: ViewSwitchProps) {
  const button = (target: EditorView, text: string) => (
    <button
      type="button"
      className={
        view === target
          ? "view-switch__button view-switch__button--active"
          : "view-switch__button"
      }
      aria-pressed={view === target}
      title={`${text} (Ctrl+Tab switches)`}
      disabled={disabled}
      onClick={() => onViewChange(target)}
    >
      {text}
    </button>
  );
  return (
    <div className="view-switch" role="group" aria-label="View">
      {button("schematic", "Schematic")}
      {button("board", "Board")}
    </div>
  );
}

interface ToolbarProps {
  view: EditorView;
  onViewChange: (view: EditorView) => void;
  onUpdateBoard: () => void;
  ratsnestVisible: boolean;
  onToggleRatsnest: () => void;
  history: HistoryStatus;
  zoomLabel: string;
  disabled: boolean;
  onUndo: () => void;
  onRedo: () => void;
  onOpenProperties: () => void;
  ercOpen: boolean;
  onToggleErc: () => void;
  libraryOpen: boolean;
  onToggleLibrary: () => void;
  wireMode: boolean;
  onToggleWireMode: () => void;
  noConnectMode: boolean;
  probeMode: boolean;
  /** Starts a canvas tool; for `noConnect` and `probe`, toggles the mode. */
  onStartTool: (tool: CanvasTool) => void;
  simulationRunning: boolean;
  onRunSimulation: () => void;
  onCancelSimulation: () => void;
  onOpenSimulationSettings: () => void;
  annotationsVisible: boolean;
  onToggleAnnotations: () => void;
}

export function Toolbar({
  view,
  onViewChange,
  onUpdateBoard,
  ratsnestVisible,
  onToggleRatsnest,
  history,
  zoomLabel,
  disabled,
  onUndo,
  onRedo,
  onOpenProperties,
  ercOpen,
  onToggleErc,
  libraryOpen,
  onToggleLibrary,
  wireMode,
  onToggleWireMode,
  noConnectMode,
  probeMode,
  onStartTool,
  simulationRunning,
  onRunSimulation,
  onCancelSimulation,
  onOpenSimulationSettings,
  annotationsVisible,
  onToggleAnnotations,
}: ToolbarProps) {
  const common = (
    <>
      <ViewSwitch
        view={view}
        disabled={disabled}
        onViewChange={onViewChange}
      />
      <span className="toolbar__separator" aria-hidden="true" />
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
    </>
  );
  const zoom = (
    <output className="toolbar__zoom" aria-label="Zoom level">
      {zoomLabel}
    </output>
  );

  if (view === "board") {
    return (
      <div className="toolbar" role="toolbar" aria-label="Editor">
        <div className="toolbar__group">
          {common}
          <IconButton
            label="Project Properties"
            disabled={disabled}
            onClick={onOpenProperties}
          >
            <PropertiesIcon />
          </IconButton>
          <IconButton
            label="Component Library"
            title="Available in the Schematic view"
            disabled
            onClick={onToggleLibrary}
          >
            <LibraryIcon />
          </IconButton>
          <span className="toolbar__separator" aria-hidden="true" />
          <IconButton
            label="Update Board from Schematic"
            shortcut="F8"
            disabled={disabled}
            onClick={onUpdateBoard}
          >
            <BoardUpdateIcon />
          </IconButton>
        </div>
        <div className="toolbar__group">
          <IconButton
            label="Show ratsnest"
            pressed={ratsnestVisible}
            disabled={disabled}
            onClick={onToggleRatsnest}
          >
            <RatsnestIcon />
          </IconButton>
          {zoom}
        </div>
      </div>
    );
  }

  return (
    <div className="toolbar" role="toolbar" aria-label="Editor">
      <div className="toolbar__group">
        {common}
        <IconButton
          label="Electrical Rules Checker"
          pressed={ercOpen}
          disabled={disabled}
          onClick={onToggleErc}
        >
          <ErcIcon />
        </IconButton>
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
        <IconButton
          label="Add no-connect flag"
          shortcut="Q"
          pressed={noConnectMode}
          disabled={disabled}
          onClick={() => onStartTool("noConnect")}
        >
          <NoConnectIcon />
        </IconButton>
        <IconButton
          label="Probe signal"
          shortcut="M"
          pressed={probeMode}
          disabled={disabled}
          onClick={() => onStartTool("probe")}
        >
          <ProbeIcon />
        </IconButton>
        <span className="toolbar__separator" aria-hidden="true" />
        <IconButton
          label="Run Simulation"
          shortcut="F5"
          disabled={disabled || simulationRunning}
          onClick={onRunSimulation}
        >
          <PlayIcon />
        </IconButton>
        <IconButton
          label="Cancel Simulation"
          shortcut="Esc"
          disabled={!simulationRunning}
          onClick={onCancelSimulation}
        >
          <StopIcon />
        </IconButton>
        <IconButton
          label="Simulation Settings"
          shortcut="Ctrl+Alt+S"
          disabled={disabled}
          onClick={onOpenSimulationSettings}
        >
          <SettingsIcon />
        </IconButton>
      </div>
      <div className="toolbar__group">
        <IconButton
          label="Show operating point annotations"
          pressed={annotationsVisible}
          disabled={disabled}
          onClick={onToggleAnnotations}
        >
          <EyeIcon />
        </IconButton>
        {zoom}
      </div>
    </div>
  );
}
