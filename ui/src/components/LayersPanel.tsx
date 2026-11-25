/**
 * The Layers panel of the board view: one row per copper layer with its
 * color, a button that shows or hides it, and a radio that makes it the
 * active layer new route pieces go on. The active layer is always shown.
 */
import { useId } from "react";
import { layerColorToken } from "../routing";
import { EyeIcon } from "./icons";

interface LayersPanelProps {
  /** The board's copper layers, top to bottom. */
  layers: string[];
  /** The layers the user hid; the active layer shows regardless. */
  hidden: ReadonlySet<string>;
  activeLayer: string;
  onToggleVisible: (layer: string) => void;
  onSetActive: (layer: string) => void;
}

/** The copper layer list with visibility and active-layer controls. */
export function LayersPanel({
  layers,
  hidden,
  activeLayer,
  onToggleVisible,
  onSetActive,
}: LayersPanelProps) {
  const group = useId();
  return (
    <aside className="layers-panel" aria-label="Layers">
      <h2 className="layers-panel__title">Copper layers</h2>
      <div
        className="layers-panel__rows"
        role="radiogroup"
        aria-label="Active layer"
      >
        {layers.map((layer) => {
          const active = layer === activeLayer;
          const shown = active || !hidden.has(layer);
          return (
            <div key={layer} className="layer-row">
              <span
                className="layer-swatch"
                style={{ backgroundColor: `var(${layerColorToken(layer)})` }}
                aria-hidden="true"
              />
              <label className="layer-row__name">
                <input
                  type="radio"
                  name={group}
                  checked={active}
                  onChange={() => onSetActive(layer)}
                />
                {layer}
              </label>
              <button
                type="button"
                className="icon-button layer-row__toggle"
                aria-label={`Show ${layer}`}
                aria-pressed={shown}
                title={
                  active ? "Active layer is always shown" : `Show ${layer}`
                }
                disabled={active}
                onClick={() => onToggleVisible(layer)}
              >
                <EyeIcon />
              </button>
            </div>
          );
        })}
      </div>
    </aside>
  );
}
